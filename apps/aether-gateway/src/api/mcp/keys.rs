//! Interactive self-service only. MCP credentials cannot mint more credentials.
use crate::LocalMutationOutcome;
use aether_data::repository::management_tokens::{
    CreateManagementTokenRecord, ManagementTokenListQuery, StoredManagementTokenUserSummary,
};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, Uri},
    response::Response,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::failure;
use crate::{control::GatewayAdminPrincipalContext, AppState};

pub(super) fn mount(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/mcp/keys", get(list).post(create))
        .route("/api/mcp/keys/{id}", axum::routing::delete(revoke))
}

fn no_store(value: Value) -> Response {
    use axum::response::IntoResponse;
    let mut response = Json(value).into_response();
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

async fn session(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
    mutation: bool,
) -> Result<GatewayAdminPrincipalContext, Response> {
    if headers
        .get_all(axum::http::header::AUTHORIZATION)
        .iter()
        .count()
        > 1
    {
        return Err(failure(StatusCode::UNAUTHORIZED, "Ambiguous authorization"));
    }
    if mutation {
        if headers
            .get("x-afssr-mcp-key-management")
            .and_then(|v| v.to_str().ok())
            != Some("1")
        {
            return Err(failure(
                StatusCode::FORBIDDEN,
                "Interactive key-management header required",
            ));
        }
        if headers
            .get("sec-fetch-site")
            .is_some_and(|v| v != "same-origin" && v != "none")
        {
            return Err(failure(
                StatusCode::FORBIDDEN,
                "Cross-origin key management is forbidden",
            ));
        }
        let origins: Vec<_> = headers.get_all(axum::http::header::ORIGIN).iter().collect();
        if origins.len() > 1 {
            return Err(failure(StatusCode::FORBIDDEN, "Ambiguous origin"));
        }
        if let Some(origin) = origins.first() {
            let matches = origin
                .to_str()
                .ok()
                .and_then(|origin| url::Url::parse(origin).ok())
                .is_some_and(|origin| {
                    let authority =
                        origin[url::Position::BeforeHost..url::Position::AfterPort].to_owned();
                    headers
                        .get(axum::http::header::HOST)
                        .and_then(|h| h.to_str().ok())
                        .is_some_and(|host| host.eq_ignore_ascii_case(&authority))
                        && matches!(origin.scheme(), "http" | "https")
                });
            if !matches {
                return Err(failure(
                    StatusCode::FORBIDDEN,
                    "Cross-origin key management is forbidden",
                ));
            }
        }
    }
    match crate::control::resolve_local_admin_session_principal(state, headers, uri).await {
        Ok(Some(principal))
            if principal.management_token_id.is_none()
                && crate::roles::can_access_admin_console(&principal.user_role) =>
        {
            Ok(principal)
        }
        Ok(_) => Err(failure(
            StatusCode::UNAUTHORIZED,
            "An administrator login session is required; management API keys are not accepted",
        )),
        Err(_) => Err(failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "Session authentication unavailable",
        )),
    }
}

pub(super) fn permissions(role: &str, read_only: bool) -> Vec<String> {
    let principal = GatewayAdminPrincipalContext {
        user_id: String::new(),
        user_role: role.into(),
        session_id: None,
        management_token_id: None,
        management_token_permissions: None,
    };
    let catalog_scopes: std::collections::BTreeSet<_> = super::catalog::OPERATIONS
        .iter()
        .filter_map(|op| {
            let decision = op.decision(&op.sample_uri(), principal.clone())?;
            let permission = crate::control::management_token_required_permission(
                &axum::http::Method::from_bytes(op.method.as_bytes()).ok()?,
                &decision,
            )?;
            permission
                .rsplit_once(':')
                .map(|(scope, _)| scope.to_owned())
        })
        .collect();
    let permissions = if read_only {
        crate::control::read_only_management_token_permissions()
    } else {
        crate::control::all_assignable_management_token_permissions()
    };
    permissions
        .into_iter()
        .filter(|p| {
            p.rsplit_once(':')
                .is_some_and(|(scope, _)| catalog_scopes.contains(scope))
                && crate::roles::role_has_management_permission(role, p)
        })
        .collect()
}

async fn list(State(state): State<AppState>, headers: HeaderMap, uri: Uri) -> Response {
    let principal = match session(&state, &headers, &uri, false).await {
        Ok(p) => p,
        Err(response) => return response,
    };
    let page = match state
        .list_management_tokens(&ManagementTokenListQuery {
            user_id: Some(principal.user_id.clone()),
            limit: 100,
            ..Default::default()
        })
        .await
    {
        Ok(page) => page,
        Err(_) => return failure(StatusCode::SERVICE_UNAVAILABLE, "Key storage unavailable"),
    };
    let items: Vec<_> = page
        .items
        .into_iter()
        .filter(|item| item.token.name.starts_with("MCP/"))
        .map(|item| item.token)
        .collect();
    no_store(
        json!({"items":items,"user_id":principal.user_id,"role":principal.user_role,"read_permissions":permissions(&principal.user_role,true),"write_permissions":permissions(&principal.user_role,false),"endpoint":"/mcp","enabled":!std::env::var("AFSSR_MCP_ENABLED").is_ok_and(|v| matches!(v.as_str(),"false"|"0")),"max_keys":20}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateInput {
    name: String,
    #[serde(default = "default_read_only")]
    read_only: bool,
    #[serde(default = "default_days")]
    expires_in_days: u64,
    allowed_ips: Option<Value>,
    permissions: Option<Vec<String>>,
}
fn default_read_only() -> bool {
    true
}
fn default_days() -> u64 {
    30
}

async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    let principal = match session(&state, &headers, &uri, true).await {
        Ok(p) => p,
        Err(response) => return response,
    };
    if body.len() > 16 * 1024 {
        return failure(StatusCode::PAYLOAD_TOO_LARGE, "Key configuration too large");
    }
    let input: CreateInput = match serde_json::from_slice(&body) {
        Ok(input) => input,
        Err(_) => return failure(StatusCode::BAD_REQUEST, "Invalid key configuration"),
    };
    let name = input.name.trim();
    if name.is_empty() || name.len() > 80 || !(1..=90).contains(&input.expires_in_days) {
        return failure(
            StatusCode::BAD_REQUEST,
            "Name must be 1-80 bytes; expiry must be 1-90 days",
        );
    }
    let allowed_ips = match crate::handlers::shared::parse_json_ip_rules(input.allowed_ips.as_ref())
    {
        Ok(ips) => ips,
        Err(_) => return failure(StatusCode::BAD_REQUEST, "Invalid IP allowlist"),
    };
    let allowed = permissions(&principal.user_role, input.read_only);
    let scopes = input.permissions.unwrap_or_else(|| allowed.clone());
    if scopes.is_empty()
        || scopes.len() > allowed.len()
        || scopes.iter().any(|scope| !allowed.contains(scope))
    {
        return failure(
            StatusCode::FORBIDDEN,
            "Requested scopes exceed this user's role or selected access mode",
        );
    }
    let existing = match state
        .list_management_tokens(&ManagementTokenListQuery {
            user_id: Some(principal.user_id.clone()),
            limit: 1,
            ..Default::default()
        })
        .await
    {
        Ok(page) => page,
        Err(_) => return failure(StatusCode::SERVICE_UNAVAILABLE, "Key storage unavailable"),
    };
    if existing.total >= 20 {
        return failure(StatusCode::CONFLICT, "User management-key quota reached");
    }
    let user = match state.find_user_auth_by_id(&principal.user_id).await {
        Ok(Some(user))
            if user.is_active && !user.is_deleted && user.role == principal.user_role =>
        {
            user
        }
        _ => {
            return failure(
                StatusCode::UNAUTHORIZED,
                "User status changed; sign in again",
            )
        }
    };
    let raw = crate::handlers::shared::generate_gateway_secret_plaintext("ae", "-");
    let record = CreateManagementTokenRecord {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user.id.clone(),
        user: StoredManagementTokenUserSummary {
            id: user.id,
            username: user.username,
            email: user.email,
            role: user.role,
        },
        token_hash: format!("{:x}", Sha256::digest(raw.as_bytes())),
        token_prefix: Some(raw.chars().take(10).collect()),
        name: format!("MCP/{name}"),
        description: Some("Agent MCP self-service credential".into()),
        allowed_ips,
        permissions: Some(json!(scopes)),
        expires_at_unix_secs: Some(
            chrono::Utc::now().timestamp().max(0) as u64 + input.expires_in_days * 86400,
        ),
        is_active: true,
    };
    match state.create_management_token(&record).await {
        Ok(LocalMutationOutcome::Applied(token)) => {
            tracing::info!(log_type="audit", event_name="mcp_key_created", user_id=%principal.user_id, key_id=%token.id, "MCP self-service credential created");
            let mut response = no_store(json!({"token":raw,"data":token}));
            *response.status_mut() = StatusCode::CREATED;
            response
        }
        Ok(LocalMutationOutcome::Invalid(_)) => {
            failure(StatusCode::BAD_REQUEST, "Key configuration rejected")
        }
        _ => failure(StatusCode::SERVICE_UNAVAILABLE, "Key creation unavailable"),
    }
}

async fn revoke(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let principal = match session(&state, &headers, &uri, true).await {
        Ok(p) => p,
        Err(response) => return response,
    };
    let token = match state.get_management_token_with_user(&id).await {
        Ok(Some(item))
            if item.token.user_id == principal.user_id && item.token.name.starts_with("MCP/") =>
        {
            item.token
        }
        Ok(_) => return failure(StatusCode::NOT_FOUND, "Key not found"),
        Err(_) => return failure(StatusCode::SERVICE_UNAVAILABLE, "Key storage unavailable"),
    };
    match state
        .delete_management_token_for_user(&token.id, &principal.user_id)
        .await
    {
        Ok(true) => {
            tracing::info!(log_type="audit", event_name="mcp_key_revoked", user_id=%principal.user_id, key_id=%token.id, "MCP self-service credential revoked");
            no_store(json!({"ok":true}))
        }
        Ok(false) => failure(StatusCode::NOT_FOUND, "Key not found"),
        Err(_) => failure(StatusCode::SERVICE_UNAVAILABLE, "Key storage unavailable"),
    }
}
