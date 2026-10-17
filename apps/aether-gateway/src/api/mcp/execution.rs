use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use axum::{
    body::{to_bytes, Bytes},
    http::{HeaderMap, Method, Uri},
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::{catalog::Operation, McpHandler, RequestIdentity};
use crate::control::{
    validate_admin_role_route_permission, validate_management_token_admin_route_permission,
    GatewayAdminPrincipalContext, GatewayPublicRequestContext,
};

pub(super) const OUTPUT_LIMIT: usize = 128 * 1024;

pub(super) fn principal(
    auth: &crate::management_token_auth::AuthenticatedManagementToken,
) -> GatewayAdminPrincipalContext {
    GatewayAdminPrincipalContext {
        user_id: auth.user.id.clone(),
        user_role: auth.user.role.clone(),
        session_id: None,
        management_token_id: Some(auth.token.id.clone()),
        management_token_permissions: Some(auth.permissions.clone()),
    }
}

pub(super) fn authorized(
    op: &Operation,
    uri: &Uri,
    principal: &GatewayAdminPrincipalContext,
) -> bool {
    let Some(decision) = op.decision(uri, principal.clone()) else {
        return false;
    };
    let method = Method::from_bytes(op.method.as_bytes()).expect("static operation method");
    validate_admin_role_route_permission(&method, &decision).is_ok()
        && validate_management_token_admin_route_permission(
            &method,
            &decision,
            principal.management_token_permissions.as_deref(),
        )
        .is_ok()
}

// Paths never come from the caller: only scalar substitutions in a static catalog.
pub(super) fn build_uri(op: &Operation, args: &Map<String, Value>) -> Result<Uri, &'static str> {
    let allowed = if op.name == "arca_benefits_preview" {
        &["path", "query", "body"][..]
    } else if op.read_only() {
        &["path", "query"][..]
    } else {
        &["path", "query", "body", "dry_run", "confirmation"][..]
    };
    if args.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("Unknown argument");
    }
    if op.name.starts_with("arca_compatibility_")
        && args
            .get("query")
            .is_some_and(|q| !q.as_object().is_some_and(Map::is_empty))
    {
        return Err("Compatibility operations do not accept query parameters");
    }
    let empty = Map::new();
    let path = match args.get("path") {
        Some(Value::Object(path)) => path,
        None => &empty,
        _ => return Err("path must be an object"),
    };
    let params = op.parameters();
    if path.len() != params.len() {
        return Err("Exact path parameters are required");
    }
    let mut uri = op.path.to_owned();
    for name in params {
        let value = path
            .get(name)
            .and_then(Value::as_str)
            .ok_or("Missing path parameter")?;
        if value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b'.')
            || value == "."
            || value.contains("..")
        {
            return Err("Invalid path identifier; URL escapes and separators are not allowed");
        }
        uri = uri.replace(&format!("{{{name}}}"), value);
    }
    if let Some(value) = args.get("query") {
        let query = value.as_object().ok_or("query must be an object")?;
        if query.len() > 32 {
            return Err("Too many query parameters");
        }
        let mut encoded = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in query {
            let normalized = key.to_ascii_lowercase();
            if key.len() > 64
                || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                || [
                    "secret",
                    "token",
                    "password",
                    "authorization",
                    "include_key",
                    "reveal",
                    "body",
                    "bodies",
                    "payload",
                    "curl",
                    "export",
                ]
                .iter()
                .any(|word| normalized.contains(word))
            {
                return Err("Sensitive or invalid query parameter");
            }
            let values = match value {
                Value::Array(values) if values.len() <= 50 => values.as_slice(),
                Value::Array(_) => return Err("Too many filter values"),
                _ => std::slice::from_ref(value),
            };
            for value in values {
                let text = match value {
                    Value::String(s) if s.len() <= 2048 => s.clone(),
                    Value::Bool(_) | Value::Number(_) => value.to_string(),
                    Value::Null => continue,
                    _ => return Err("Query values must be bounded scalars"),
                };
                if matches!(key.as_str(), "limit" | "page_size" | "per_page")
                    && text.parse::<u32>().map_or(true, |v| v == 0 || v > 500)
                {
                    return Err("Page size must be between 1 and 500");
                }
                encoded.append_pair(key, &text);
            }
        }
        let query = encoded.finish();
        if !query.is_empty() {
            uri.push('?');
            uri.push_str(&query);
        }
    }
    uri.parse().map_err(|_| "Invalid operation URI")
}

pub(super) struct Ticket {
    pub owner: String,
    pub fingerprint: String,
    pub expires: Instant,
}

pub(super) fn fingerprint(
    op: &Operation,
    uri: &Uri,
    body: &Value,
    principal: &GatewayAdminPrincipalContext,
) -> String {
    // Canonical maps make tickets independent of JSON object key order.
    fn canonical(value: &Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(k, v)| (k.clone(), canonical(v)))
                    .collect::<BTreeMap<_, _>>()
                    .into_iter()
                    .collect(),
            ),
            Value::Array(array) => Value::Array(array.iter().map(canonical).collect()),
            _ => value.clone(),
        }
    }
    let data = json!([
        op.name,
        uri.to_string(),
        body,
        principal.user_id,
        principal.user_role,
        principal.management_token_id,
        principal.management_token_permissions
    ]);
    format!(
        "{:x}",
        Sha256::digest(canonical(&data).to_string().as_bytes())
    )
}

impl McpHandler {
    pub(super) async fn execute(
        &self,
        op: &Operation,
        args: Map<String, Value>,
        identity: &RequestIdentity,
        principal: &GatewayAdminPrincipalContext,
    ) -> Value {
        let uri = match build_uri(op, &args) {
            Ok(uri) => uri,
            Err(error) => return json!({"ok":false,"status":400,"error":error}),
        };
        if !authorized(op, &uri, principal) {
            return json!({"ok":false,"status":403,"error":"This user and API key do not have permission for this operation"});
        }
        let body = args.get("body").cloned().unwrap_or(json!({}));
        if !body.is_object() {
            return json!({"ok":false,"status":400,"error":"body must be an object"});
        }
        if matches!(
            op.name,
            "arca_compatibility_profile_save" | "arca_compatibility_run"
        ) {
            if let Err(error) = crate::api::compatibility::validate_mcp_body(op.name, &body) {
                return json!({"ok":false,"status":400,"error":error});
            }
        }
        if let Some(dry_run) = args.get("dry_run") {
            if !dry_run.is_boolean() {
                return json!({"ok":false,"status":400,"error":"dry_run must be boolean"});
            }
        }
        if !op.read_only() {
            let fingerprint = fingerprint(op, &uri, &body, principal);
            let owner = principal.management_token_id.clone().unwrap_or_default();
            let Ok(mut tickets) = self.runtime.tickets.lock() else {
                return json!({"ok":false,"status":503,"error":"Confirmation store unavailable"});
            };
            let now = Instant::now();
            tickets.retain(|_, ticket| ticket.expires > now);
            if args.get("dry_run").and_then(Value::as_bool).unwrap_or(true) {
                if tickets.len() >= 512
                    || tickets.values().filter(|t| t.owner == owner).count() >= 10
                {
                    return json!({"ok":false,"status":429,"error":"Too many pending previews; wait for expiry"});
                }
                let confirmation = uuid::Uuid::new_v4().to_string();
                tickets.insert(
                    confirmation.clone(),
                    Ticket {
                        owner,
                        fingerprint,
                        expires: now + Duration::from_secs(300),
                    },
                );
                let mut preview = json!({"method":op.method,"path":uri.path(),"query":args.get("query"),"body":body});
                redact_preview(op, &uri, &mut preview);
                redact(&mut preview);
                return json!({"ok":true,"executed":false,"preview":preview,"confirmation":confirmation,"expires_in_seconds":300,"notice":"No operation has executed. This ticket binds exact arguments, not human approval. Obtain explicit approval for downtime or destructive changes before execution."});
            }
            let valid = args
                .get("confirmation")
                .and_then(Value::as_str)
                .and_then(|id| tickets.get(id))
                .is_some_and(|ticket| ticket.owner == owner && ticket.fingerprint == fingerprint);
            if !valid {
                return json!({"ok":false,"status":409,"error":"Missing, expired or mismatched confirmation. Preview the exact operation again; never blindly retry a write."});
            }
            tickets.remove(args["confirmation"].as_str().unwrap());
        }
        let method = Method::from_bytes(op.method.as_bytes()).expect("static method");
        let decision = op
            .decision(&uri, principal.clone())
            .expect("authorized local route");
        let trace_id = uuid::Uuid::new_v4().to_string();
        let context = GatewayPublicRequestContext::from_request_parts(
            trace_id.clone(),
            &method,
            &uri,
            &HeaderMap::new(),
            Some(decision),
        );
        let bytes = Bytes::from(body.to_string());
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/json"),
        );
        let result = crate::handlers::proxy::maybe_build_local_admin_proxy_response(
            &self.state,
            &context,
            &identity.remote,
            &headers,
            if op.read_only() { None } else { Some(&bytes) },
        )
        .await;
        let mut response = match result {
            Ok(Some(response)) => response,
            Ok(None) => {
                return json!({"ok":false,"status":503,"error":"Operation has no local implementation; remote fallback is disabled","trace_id":trace_id})
            }
            Err(_) => {
                return json!({"ok":false,"status":503,"error":"Management operation unavailable; consult server audit logs","trace_id":trace_id})
            }
        };
        crate::audit::emit_admin_audit(
            &mut response,
            &trace_id,
            &method,
            &uri.to_string(),
            context.control_decision.as_ref(),
        );
        let status = response.status();
        let data = match to_bytes(response.into_body(), OUTPUT_LIMIT).await {
            Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
                .unwrap_or(json!({"detail":"Non-JSON response omitted"})),
            Err(_) => {
                json!({"detail":"Response exceeded MCP output limit; narrow filters or reduce page size","truncated":true})
            }
        };
        let mut result = json!({"ok":status.is_success(),"executed":!op.read_only(),"status":status.as_u16(),"data":data,"trace_id":trace_id});
        redact(&mut result);
        result
    }
}

pub(super) fn redact_preview(op: &Operation, uri: &Uri, preview: &mut Value) {
    let credential_operation = matches!(
        op.name,
        "arca_upstream_keys_create"
            | "arca_upstream_keys_update"
            | "arca_pool_batch_import"
            | "arca_ops_configure"
            | "arca_proxy_create"
            | "arca_proxy_update"
            | "arca_proxy_config_set"
            | "arca_ldap_set"
    );
    let sensitive_config = uri
        .path()
        .strip_prefix("/api/admin/system/configs/")
        .is_some_and(sensitive_setting_name);
    if credential_operation || sensitive_config {
        preview["body"] = json!("[REDACTED: credential-bearing operation]");
    }
}

pub(super) fn redact(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // Global settings often store secrets in an otherwise innocuous `value` field.
            let map_is_setting =
                map.contains_key("value") && map.get("key").is_some_and(Value::is_string);
            let sensitive_setting = ["key", "name", "config_key"]
                .iter()
                .filter_map(|k| map.get(*k).and_then(Value::as_str))
                .any(sensitive_setting_name);
            for (key, value) in map.iter_mut() {
                if sensitive_name(key)
                    || (sensitive_setting && key == "value")
                    || (key == "key" && value.is_string() && !map_is_setting)
                {
                    *value = json!("[REDACTED]");
                } else {
                    redact(value);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact),
        Value::String(text)
            if ["sk-", "oc_sk_", "ae-", "ae_", "Bearer ", "PRIVATE KEY"]
                .iter()
                .any(|prefix| text.contains(prefix)) =>
        {
            *value = json!("[REDACTED]");
        }
        Value::String(text)
            if url::Url::parse(text).is_ok_and(|url| {
                url.password().is_some()
                    || !url.username().is_empty()
                    || url
                        .query_pairs()
                        .any(|(key, _)| sensitive_name(&key) || key == "key")
            }) =>
        {
            *value = json!("[REDACTED: credential-bearing URL]");
        }
        _ => {}
    }
}

fn sensitive_setting_name(name: &str) -> bool {
    sensitive_name(name)
        || name.to_ascii_lowercase().ends_with("_key")
        || aether_admin::system::is_sensitive_admin_system_config_key(name)
}

fn sensitive_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("password")
        || name.contains("secret")
        || name.contains("authorization")
        || name.contains("cookie")
        || name.contains("private_key")
        || name.contains("encrypted")
        || name.ends_with("_token")
        || matches!(
            name.as_str(),
            "api_key"
                | "key_plaintext"
                | "token"
                | "token_hash"
                | "access_token"
                | "refresh_token"
                | "credentials"
                | "auth_config"
                | "header_rules"
                | "body_rules"
                | "headers"
                | "request_body"
                | "response_body"
                | "response"
                | "content_preview"
                | "request_headers"
                | "response_headers"
                | "payload"
                | "raw"
                | "curl_command"
                | "encryption_key"
                | "api_key_encrypted"
        )
}
