use super::*;

#[test]
fn benefit_preview_accepts_a_read_only_post_body_without_write_confirmation() {
    let op = catalog::find("arca_benefits_preview").unwrap();
    assert!(op.read_only());
    assert!(op.tool().input_schema["properties"]["body"].is_object());
    assert!(op.tool().input_schema["properties"]
        .get("confirmation")
        .is_none());
    let args = json!({"body":{"user_id":"fixture"}});
    assert!(execution::build_uri(op, args.as_object().unwrap()).is_ok());
    let mut p = principal("audit_admin");
    p.management_token_permissions = Some(vec!["admin:users:read".into()]);
    assert!(execution::authorized(op, &op.sample_uri(), &p));
    p.management_token_permissions = Some(vec!["admin:users:write".into()]);
    assert!(!execution::authorized(op, &op.sample_uri(), &p));
}
use crate::control::GatewayPublicRequestContext;
use aether_data::repository::management_tokens::{
    InMemoryManagementTokenRepository, StoredManagementToken, StoredManagementTokenUserSummary,
    StoredManagementTokenWithUser,
};
use axum::{
    body::Bytes,
    http::{HeaderMap, Method},
};
use base64::Engine as _;
use hmac::Mac as _;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn principal(role: &str) -> crate::control::GatewayAdminPrincipalContext {
    crate::control::GatewayAdminPrincipalContext {
        user_id: "user-fixture".into(),
        user_role: role.into(),
        session_id: None,
        management_token_id: Some("fixture-key".into()),
        management_token_permissions: Some(
            crate::control::all_assignable_management_token_permissions(),
        ),
    }
}

#[test]
fn mcp_catalog_routes_are_registered_and_names_unique() {
    let mut names = std::collections::HashSet::new();
    let mut missing = vec![];
    for op in catalog::OPERATIONS {
        assert!(names.insert(op.name), "duplicate {}", op.name);
        if op.decision(&op.sample_uri(), principal("admin")).is_none() {
            missing.push(format!("{} {} {}", op.name, op.method, op.path));
        }
        assert!(
            !op.path.contains("reveal")
                && !op.path.contains("export")
                && !op.path.contains("management-tokens")
        );
        assert!(!op.path.contains("wallet") && !op.path.contains("payments"));
        assert_eq!(
            op.tool().input_schema.get("additionalProperties"),
            Some(&json!(false))
        );
    }
    assert!(
        missing.is_empty(),
        "unregistered catalog operations: {missing:?}"
    );
}

#[test]
fn mcp_compatibility_is_readonly_and_requires_its_own_scope() {
    let op = catalog::find("arca_compatibility_list").unwrap();
    assert!(op.read_only());
    assert!(!op.tool().input_schema["properties"]
        .as_object()
        .unwrap()
        .contains_key("body"));
    assert!(execution::build_uri(op, json!({"body":{}}).as_object().unwrap()).is_err());
    assert!(execution::build_uri(
        op,
        json!({"dry_run":false,"confirmation":"fixture"})
            .as_object()
            .unwrap()
    )
    .is_err());
    let writes: Vec<_> = catalog::OPERATIONS
        .iter()
        .filter(|o| o.path.starts_with("/api/admin/compatibility/"))
        .collect();
    assert_eq!(writes.len(), 2);
    assert!(writes.iter().all(|o| o.impact == catalog::Impact::Change));
    for role in ["admin", "audit_admin", "user_admin", "service_admin"] {
        let mut p = principal(role);
        p.management_token_permissions = Some(vec!["admin:compatibility:read".into()]);
        assert!(execution::authorized(op, &op.sample_uri(), &p));
        p.management_token_permissions = Some(vec!["admin:providers:read".into()]);
        assert!(!execution::authorized(op, &op.sample_uri(), &p));
    }
    assert!(!execution::authorized(
        op,
        &op.sample_uri(),
        &principal("user")
    ));
}

fn compatibility_mcp_body(save: bool) -> Value {
    if save {
        json!({"expected_revision":0,"profile":{"id":"00000000-0000-4000-8000-000000000001","config":{
            "client":"codex","client_version":"synthetic-1","client_protocol":"openai:responses",
            "model":"fixture-model","upstream_format":"claude:messages","mapped_model":"fixture-upstream"
        }}})
    } else {
        json!({"expected_revision":0,"profile_id":"00000000-0000-4000-8000-000000000001","expected_fingerprint":"a".repeat(64)})
    }
}

#[test]
fn mcp_compatibility_changes_require_service_role_write_scope_and_strict_bodies() {
    for (name, save) in [
        ("arca_compatibility_profile_save", true),
        ("arca_compatibility_run", false),
    ] {
        let op = catalog::find(name).unwrap();
        assert_eq!(op.impact, catalog::Impact::Change);
        let tool = op.tool();
        assert_eq!(tool.input_schema["required"], json!(["body"]));
        let body_schema = &tool.input_schema["properties"]["body"];
        assert_eq!(body_schema["additionalProperties"], false);
        assert_eq!(
            body_schema["properties"]["expected_revision"]["maximum"],
            9_007_199_254_740_991u64
        );
        assert!(tool.input_schema["properties"]
            .get("confirmation")
            .is_some());
        if save {
            assert_eq!(
                body_schema["properties"]["profile"]["additionalProperties"],
                false
            );
            assert_eq!(
                body_schema["properties"]["profile"]["properties"]["config"]
                    ["additionalProperties"],
                false
            );
        } else {
            assert_eq!(
                body_schema["properties"]["expected_fingerprint"]["maxLength"],
                64
            );
        }
        for role in [
            "admin",
            "service_admin",
            "audit_admin",
            "user_admin",
            "user",
        ] {
            let mut p = principal(role);
            p.management_token_permissions = Some(vec!["admin:compatibility:write".into()]);
            assert_eq!(
                execution::authorized(op, &op.sample_uri(), &p),
                matches!(role, "admin" | "service_admin")
            );
            for scope in ["admin:compatibility:read", "admin:providers:write"] {
                p.management_token_permissions = Some(vec![scope.into()]);
                assert!(!execution::authorized(op, &op.sample_uri(), &p));
            }
        }
        let body = compatibility_mcp_body(save);
        assert!(crate::api::compatibility::validate_mcp_body(name, &body).is_ok());
        let mut unknown = body.clone();
        unknown["request_body"] = json!({"prompt":"synthetic-secret-canary"});
        assert!(crate::api::compatibility::validate_mcp_body(name, &unknown).is_err());
        let mut oversized = body.clone();
        oversized["unexpected"] = json!("x".repeat(8193));
        assert!(crate::api::compatibility::validate_mcp_body(name, &oversized).is_err());
        assert!(
            execution::build_uri(op, json!({"query":{"limit":1}}).as_object().unwrap()).is_err()
        );
        let mut negative = body.clone();
        negative["expected_revision"] = json!(-1);
        assert!(crate::api::compatibility::validate_mcp_body(name, &negative).is_err());
        let mut unsafe_revision = body.clone();
        unsafe_revision["expected_revision"] = json!(9_007_199_254_740_992u64);
        assert!(crate::api::compatibility::validate_mcp_body(name, &unsafe_revision).is_err());
        if save {
            let mut nested = body.clone();
            nested["profile"]["config"]["api_key"] = json!("synthetic-secret-canary");
            assert!(crate::api::compatibility::validate_mcp_body(name, &nested).is_err());
            for label in ["sk-synthetic", "https://example.test", "Bearer-synthetic"] {
                let mut invalid = body.clone();
                invalid["profile"]["config"]["model"] = json!(label);
                assert!(crate::api::compatibility::validate_mcp_body(name, &invalid).is_err());
            }
        } else {
            let mut invalid = body.clone();
            invalid["expected_fingerprint"] = json!("unknown");
            assert!(crate::api::compatibility::validate_mcp_body(name, &invalid).is_err());
        }
    }
}

#[tokio::test]
async fn mcp_compatibility_confirmation_is_exact_single_use_and_checks_before_preview() {
    let handler = McpHandler {
        state: AppState::new()
            .unwrap()
            .with_data_state_for_tests(crate::data::GatewayDataState::disabled()),
        runtime: Arc::new(Runtime::default()),
    };
    let identity = RequestIdentity {
        hash: VerifiedManagementTokenHash::new("synthetic-compatibility-dispatch".into()),
        remote: "127.0.0.1:12345".parse().unwrap(),
    };
    for (name, save) in [
        ("arca_compatibility_profile_save", true),
        ("arca_compatibility_run", false),
    ] {
        let op = catalog::find(name).unwrap();
        let args = json!({"body":compatibility_mcp_body(save)});
        let mut p = principal("service_admin");
        p.management_token_permissions = Some(vec!["admin:compatibility:write".into()]);
        for role in ["audit_admin", "user_admin"] {
            let mut denied = p.clone();
            denied.user_role = role.into();
            assert_eq!(
                handler
                    .execute(op, args.as_object().unwrap().clone(), &identity, &denied)
                    .await["status"],
                403
            );
        }
        let mut invalid = args.clone();
        invalid["body"]["api_key"] = json!("synthetic-secret-canary");
        assert_eq!(
            handler
                .execute(op, json!({}).as_object().unwrap().clone(), &identity, &p)
                .await["status"],
            400
        );
        let rejected = handler
            .execute(op, invalid.as_object().unwrap().clone(), &identity, &p)
            .await;
        assert_eq!(rejected["status"], 400);
        assert!(rejected.get("confirmation").is_none());
        assert!(!rejected.to_string().contains("synthetic-secret-canary"));
        let mut no_ticket = args.clone();
        no_ticket["dry_run"] = json!(false);
        assert_eq!(
            handler
                .execute(op, no_ticket.as_object().unwrap().clone(), &identity, &p)
                .await["status"],
            409
        );
        let preview = handler
            .execute(op, args.as_object().unwrap().clone(), &identity, &p)
            .await;
        assert_eq!(preview["executed"], false);
        let ticket = preview["confirmation"].as_str().unwrap();
        let mut confirmed = no_ticket;
        confirmed["confirmation"] = json!(ticket);
        let mut mismatch = confirmed.clone();
        mismatch["body"]["expected_revision"] = json!(1);
        assert_eq!(
            handler
                .execute(op, mismatch.as_object().unwrap().clone(), &identity, &p)
                .await["status"],
            409
        );
        let mut other_key = p.clone();
        other_key.management_token_id = Some("other-compat-key".into());
        assert_eq!(
            handler
                .execute(
                    op,
                    confirmed.as_object().unwrap().clone(),
                    &identity,
                    &other_key
                )
                .await["status"],
            409
        );
        let mut downgraded = p.clone();
        downgraded.management_token_permissions = Some(vec!["admin:compatibility:read".into()]);
        assert_eq!(
            handler
                .execute(
                    op,
                    confirmed.as_object().unwrap().clone(),
                    &identity,
                    &downgraded
                )
                .await["status"],
            403
        );
        let outcome = handler
            .execute(op, confirmed.as_object().unwrap().clone(), &identity, &p)
            .await;
        // Real local dispatch, deliberately no persistent DB: no success may be fabricated.
        assert_eq!(outcome["status"], if save { 503 } else { 404 }, "{outcome}");
        assert_eq!(outcome["ok"], false);
        assert_eq!(
            handler
                .execute(op, confirmed.as_object().unwrap().clone(), &identity, &p)
                .await["status"],
            409
        );
    }
}

#[test]
fn mcp_diagnostic_is_read_only_and_does_not_inherit_feedback_write_scope() {
    let op = catalog::find("arca_request_diagnostics").unwrap();
    assert!(op.read_only());
    assert!(!op.tool().input_schema["properties"]
        .as_object()
        .unwrap()
        .contains_key("body"));
    let mut p = principal("audit_admin");
    p.management_token_permissions = Some(vec!["admin:monitoring:read".into()]);
    assert!(execution::authorized(op, &op.sample_uri(), &p));
    assert!(crate::api::diagnostics::can_read_admin(&p));
    p.management_token_permissions = Some(vec!["admin:feedback:write".into()]);
    assert!(!execution::authorized(op, &op.sample_uri(), &p));
    assert!(!crate::api::diagnostics::can_read_admin(&p));
    let feedback = catalog::find("arca_feedback_command").unwrap();
    assert!(execution::authorized(feedback, &feedback.sample_uri(), &p));
    p.user_role = "user".into();
    p.management_token_permissions = Some(vec!["admin:monitoring:read".into()]);
    assert!(!execution::authorized(op, &op.sample_uri(), &p));
}

#[tokio::test]
async fn feedback_diagnostic_attachment_requires_monitoring_even_with_feedback_write() {
    let mut p = principal("admin");
    p.management_token_permissions = Some(vec!["admin:feedback:write".into()]);
    let op = catalog::find("arca_feedback_command").unwrap();
    let uri = op.sample_uri();
    let context = GatewayPublicRequestContext::from_request_parts(
        "fixture",
        &Method::POST,
        &uri,
        &HeaderMap::new(),
        op.decision(&uri, p),
    );
    let state = AppState::new().unwrap();
    let body = Bytes::from(json!({"action":"reply","content":"Fixture","diagnostic_request_id":"fixture-request","expected_version":1,"client_request_id":uuid::Uuid::new_v4().to_string()}).to_string());
    let result = crate::api::feedback::admin_response(&state, &context, Some(&body))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn mcp_read_scoped_diagnostics_use_safe_projection_and_no_inference() {
    let fixture: Value =
        serde_json::from_str(include_str!("../diagnostics/fixtures/evidence.json")).unwrap();
    let candidate = serde_json::from_value(fixture["candidate"].clone()).unwrap();
    let state = AppState::new()
        .unwrap()
        .with_request_candidate_data_reader_for_tests(Arc::new(
            aether_data::repository::candidates::InMemoryRequestCandidateRepository::seed(vec![
                candidate,
            ]),
        ));
    let handler = McpHandler {
        state,
        runtime: Arc::new(Runtime::default()),
    };
    let identity = RequestIdentity {
        hash: VerifiedManagementTokenHash::new("unused-direct-test".into()),
        remote: "127.0.0.1:12345".parse().unwrap(),
    };
    let mut p = principal("audit_admin");
    p.management_token_permissions = Some(vec!["admin:monitoring:read".into()]);
    let op = catalog::find("arca_request_diagnostics").unwrap();
    let args = json!({"path":{"request_id":"fixture-request"}});
    let result = handler
        .execute(op, args.as_object().unwrap().clone(), &identity, &p)
        .await;
    assert_eq!(result["status"], 200);
    assert_eq!(result["executed"], false);
    assert_eq!(result["data"]["state"], "failed");
    for forbidden in ["CANARY", "10.20.30.40", "Bearer", "Authorization"] {
        assert!(!result.to_string().contains(forbidden), "{forbidden}");
    }
    p.management_token_permissions = Some(vec!["admin:feedback:write".into()]);
    let denied = handler
        .execute(op, args.as_object().unwrap().clone(), &identity, &p)
        .await;
    assert_eq!(denied["status"], 403);
    assert!(denied.get("data").is_none());
}

#[test]
fn mcp_discovery_and_execution_obey_role_and_key_scope() {
    let users = catalog::find("arca_users_update").unwrap();
    let providers = catalog::find("arca_providers_update").unwrap();
    for (role, users_allowed, providers_allowed) in [
        ("admin", true, true),
        ("audit_admin", false, false),
        ("user_admin", true, false),
        ("service_admin", false, true),
        ("user", false, false),
    ] {
        let p = principal(role);
        assert_eq!(
            execution::authorized(users, &users.sample_uri(), &p),
            users_allowed,
            "{role}"
        );
        assert_eq!(
            execution::authorized(providers, &providers.sample_uri(), &p),
            providers_allowed,
            "{role}"
        );
    }
    let mut p = principal("admin");
    p.management_token_permissions = Some(crate::control::read_only_management_token_permissions());
    assert!(!execution::authorized(users, &users.sample_uri(), &p));
    let op = catalog::find("arca_users_list").unwrap();
    assert!(execution::authorized(op, &op.sample_uri(), &p));
    p.management_token_permissions = Some(vec![]);
    assert!(!execution::authorized(op, &op.sample_uri(), &p));
    for role in ["admin", "audit_admin", "user_admin", "service_admin"] {
        assert!(
            keys::permissions(role, true)
                .iter()
                .all(|p| p.ends_with(":read")
                    && crate::roles::role_has_management_permission(role, p))
        );
        assert!(keys::permissions(role, false)
            .iter()
            .all(|p| !p.contains("management_tokens")
                && crate::roles::role_has_management_permission(role, p)));
    }
    assert!(keys::permissions("user", false).is_empty());
}

#[test]
fn mcp_uri_rejects_path_injection_secrets_and_unbounded_queries() {
    let op = catalog::find("arca_users_get").unwrap();
    for id in [
        "../providers",
        "x%2freveal",
        "x?include_key=true",
        "x#fragment",
        "http://attacker",
        ".",
        "..",
        "x\\evil",
    ] {
        assert!(
            execution::build_uri(op, json!({"path":{"user_id":id}}).as_object().unwrap()).is_err(),
            "{id}"
        );
    }
    for query in [
        json!({"include_key":true}),
        json!({"include_bodies":true}),
        json!({"access_token":"secret"}),
        json!({"limit":10000}),
        json!({"nested":{"bad":true}}),
    ] {
        assert!(execution::build_uri(
            op,
            json!({"path":{"user_id":"uuid-1"},"query":query})
                .as_object()
                .unwrap()
        )
        .is_err());
    }
    assert!(execution::build_uri(
        op,
        json!({"path":{"user_id":"uuid-1"},"body":{}})
            .as_object()
            .unwrap()
    )
    .is_err());
    let uri = execution::build_uri(
        op,
        json!({"path":{"user_id":"uuid-1"},"query":{"search":"a&b","limit":20}})
            .as_object()
            .unwrap(),
    )
    .unwrap();
    assert!(uri.to_string().contains("a%26b"));
}

#[test]
fn mcp_redacts_secrets_and_bodies_without_hiding_usage_or_ids() {
    let mut value = json!({"password":"private","nested":{"api_key":"private","access_token":"private","request_body":{"prompt":"private"},"header_rules":[]},"setting":{"key":"jwt_secret","value":"private"},"text":"Bearer opaque","total_tokens":200,"api_key_id":"id-1","key_id":"id-2"});
    execution::redact(&mut value);
    assert!(!value.to_string().contains("private"));
    assert!(!value.to_string().contains("opaque"));
    assert_eq!(value["total_tokens"], 200);
    assert_eq!(value["api_key_id"], "id-1");
    for (name, path, body) in [
        (
            "arca_system_config_set",
            "/api/admin/system/configs/bark_device_key",
            json!({"value":"secret-canary"}),
        ),
        (
            "arca_pool_batch_import",
            "/api/admin/pool/p/keys/batch-import",
            json!({"content":"secret-canary"}),
        ),
        (
            "arca_proxy_create",
            "/api/admin/proxy-nodes/manual",
            json!({"proxy_url":"socks5://u:secret-canary@example.test"}),
        ),
    ] {
        let mut preview = json!({"body":body});
        execution::redact_preview(
            catalog::find(name).unwrap(),
            &path.parse().unwrap(),
            &mut preview,
        );
        execution::redact(&mut preview);
        assert!(!preview.to_string().contains("secret-canary"), "{name}");
    }
    let mut urls = json!([
        "https://u:password-canary@example.test",
        "https://example.test?api_key=query-canary"
    ]);
    execution::redact(&mut urls);
    assert!(!urls.to_string().contains("canary"));
}

async fn fixture(role: &str) -> (AppState, String, String) {
    let state = AppState::new().unwrap();
    let user = state
        .create_local_auth_user_with_settings(
            Some("mcp@example.test".into()),
            true,
            "mcp_fixture".into(),
            "not-a-password".into(),
            role.into(),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap()
        .unwrap();
    let raw = "ae-mcp-test-fixture-only".to_owned();
    let token = StoredManagementToken::new("mcp-key".into(), user.id.clone(), "MCP/test".into())
        .unwrap()
        .with_permissions(Some(json!(
            crate::control::all_assignable_management_token_permissions()
        )));
    let summary = StoredManagementTokenUserSummary {
        id: user.id.clone(),
        username: user.username.clone(),
        email: user.email.clone(),
        role: role.into(),
    };
    let repository = Arc::new(InMemoryManagementTokenRepository::seed_with_hashes(
        vec![StoredManagementTokenWithUser::new(token, summary)],
        vec![(
            format!("{:x}", Sha256::digest(raw.as_bytes())),
            "mcp-key".into(),
        )],
    ));
    let state = state.with_data_state_for_tests(
        crate::data::GatewayDataState::with_management_token_repository_for_tests(repository)
            .with_system_config_values_for_tests([
                ("site_name".into(), json!("Before")),
                (
                    "internal_access_policy".into(),
                    json!({"timezone":"UTC","default":null,"groups":{},"users":{}}),
                ),
            ]),
    );
    (state, raw, user.id)
}

async fn start(state: AppState) -> (String, tokio::task::JoinHandle<()>) {
    let router = crate::router::build_router_with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (format!("http://{addr}"), handle)
}

fn rpc(method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})
}
fn post(client: &reqwest::Client, url: &str, raw: &str, body: Value) -> reqwest::RequestBuilder {
    client
        .post(format!("{url}/mcp"))
        .bearer_auth(raw)
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2025-11-25")
        .json(&body)
}

#[tokio::test]
async fn mcp_http_protocol_auth_discovery_and_read_round_trip() {
    let (state, raw, user_id) = fixture("user_admin").await;
    let (url, handle) = start(state.clone()).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    for key in ["", "sk-inference-must-not-work", "ae-unknown"] {
        assert_eq!(
            post(&client, &url, key, rpc("tools/list", json!({})))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    let init=post(&client,&url,&raw,rpc("initialize",json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"mcp-test","version":"1"}}))).send().await.unwrap();
    assert_eq!(init.status(), StatusCode::OK);
    let init: Value = init.json().await.unwrap();
    assert!(
        init["result"]["capabilities"]["tools"].is_object(),
        "{init}"
    );
    let mut cursor = Value::Null;
    let mut names = vec![];
    loop {
        let params = if cursor.is_null() {
            json!({})
        } else {
            json!({"cursor":cursor})
        };
        let response: Value = post(&client, &url, &raw, rpc("tools/list", params))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let tools = response["result"]["tools"].as_array().expect("tools list");
        names.extend(
            tools
                .iter()
                .map(|tool| tool["name"].as_str().unwrap().to_owned()),
        );
        cursor = response["result"]["nextCursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    assert!(names.contains(&"arca_users_update".into()));
    assert!(!names.contains(&"arca_providers_update".into()));
    let response: Value = post(
        &client,
        &url,
        &raw,
        rpc("tools/call", json!({"name":"arca_identity","arguments":{}})),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(
        response["result"]["structuredContent"]["user_id"], user_id,
        "{response}"
    );
    let denied:Value=post(&client,&url,&raw,rpc("tools/call",json!({"name":"arca_providers_update","arguments":{"path":{"provider_id":"provider-1"},"body":{}}}))).send().await.unwrap().json().await.unwrap();
    assert_eq!(denied["result"]["isError"], true, "{denied}");
    let response: Value = post(
        &client,
        &url,
        &raw,
        rpc(
            "tools/call",
            json!({"name":"arca_benefits_get","arguments":{}}),
        ),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert!(
        response["result"]["structuredContent"].is_object(),
        "{response}"
    );
    assert_eq!(
        response["result"]["structuredContent"]["ok"], true,
        "{response}"
    );
    assert_eq!(
        post(&client, &url, &raw, rpc("tools/list", json!({})))
            .header("Origin", "https://attacker.test")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post(&client, &url, &raw, rpc("tools/list", json!({})))
            .header("Host", "attacker.test")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(format!("{url}/api/mcp/keys"))
            .bearer_auth(&raw)
            .header("X-AFSSR-MCP-Key-Management", "1")
            .json(&json!({"name":"bad"}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let mut user = state.find_user_auth_by_id(&user_id).await.unwrap().unwrap();
    user.role = "audit_admin".into();
    let state = state.with_auth_users_for_tests([user]);
    let downgraded: Value = post(
        &client,
        &url,
        &raw,
        rpc(
            "tools/call",
            json!({"name":"arca_users_update","arguments":{"path":{"user_id":"other"}}}),
        ),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(downgraded["result"]["isError"], true, "{downgraded}");
    assert!(state
        .delete_management_token_for_user("mcp-key", &user_id)
        .await
        .unwrap());
    assert_eq!(
        post(&client, &url, &raw, rpc("tools/list", json!({})))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    handle.abort();
}

#[tokio::test]
async fn mcp_http_write_round_trip_and_resource_bounds() {
    let (state, raw, _) = fixture("admin").await;
    let (url, handle) = start(state).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let arguments = json!({"path":{"config_key":"site_name"},"body":{"value":"After HTTP","expected_value":"Before"}});
    let preview: Value = post(
        &client,
        &url,
        &raw,
        rpc(
            "tools/call",
            json!({"name":"arca_system_config_set","arguments":arguments}),
        ),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    let ticket = preview["result"]["structuredContent"]["confirmation"]
        .as_str()
        .unwrap();
    let mut execution = arguments.clone();
    execution["dry_run"] = json!(false);
    execution["confirmation"] = json!(ticket);
    let result: Value = post(
        &client,
        &url,
        &raw,
        rpc(
            "tools/call",
            json!({"name":"arca_system_config_set","arguments":execution}),
        ),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(
        result["result"]["structuredContent"]["ok"], true,
        "{result}"
    );
    let read: Value = post(
        &client,
        &url,
        &raw,
        rpc(
            "tools/call",
            json!({"name":"arca_system_config","arguments":{"path":{"config_key":"site_name"}}}),
        ),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(
        read["result"]["structuredContent"]["data"]["value"], "After HTTP",
        "{read}"
    );
    let replay: Value = post(
        &client,
        &url,
        &raw,
        rpc(
            "tools/call",
            json!({"name":"arca_system_config_set","arguments":execution}),
        ),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(replay["result"]["isError"], true);
    let oversized = "x".repeat(1024 * 1024 + 1);
    assert_eq!(
        client
            .post(format!("{url}/mcp"))
            .bearer_auth(&raw)
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .body(oversized)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let duplicate = post(&client, &url, &raw, rpc("tools/list", json!({})))
        .header("Authorization", format!("Bearer {raw}"));
    assert_eq!(
        duplicate.send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    handle.abort();
}

#[tokio::test]
async fn mcp_write_requires_exact_single_use_ticket_and_live_permissions() {
    let (state, _, _) = fixture("admin").await;
    let handler = McpHandler {
        state,
        runtime: Arc::new(Runtime::default()),
    };
    let identity = RequestIdentity {
        hash: VerifiedManagementTokenHash::new("unused-in-direct-dispatch-test".into()),
        remote: "127.0.0.1:12345".parse().unwrap(),
    };
    let p = principal("admin");
    let op = catalog::find("arca_system_config_set").unwrap();
    let args = json!({"path":{"config_key":"site_name"},"body":{"value":"After","expected_value":"Before"}});
    let preview = handler
        .execute(op, args.as_object().unwrap().clone(), &identity, &p)
        .await;
    assert_eq!(preview["executed"], false);
    let confirmation = preview["confirmation"].as_str().unwrap();
    let mut execute = args.clone();
    execute["dry_run"] = json!(false);
    execute["confirmation"] = json!(confirmation);
    let mut mismatch = execute.clone();
    mismatch["body"]["value"] = json!("Other");
    assert_eq!(
        handler
            .execute(op, mismatch.as_object().unwrap().clone(), &identity, &p)
            .await["status"],
        409
    );
    let mut other = p.clone();
    other.management_token_id = Some("other-key".into());
    assert_eq!(
        handler
            .execute(op, execute.as_object().unwrap().clone(), &identity, &other)
            .await["status"],
        409
    );
    let mut downgraded = p.clone();
    downgraded.user_role = "audit_admin".into();
    assert_eq!(
        handler
            .execute(
                op,
                execute.as_object().unwrap().clone(),
                &identity,
                &downgraded
            )
            .await["status"],
        403
    );
    let result = handler
        .execute(op, execute.as_object().unwrap().clone(), &identity, &p)
        .await;
    assert_eq!(result["status"], 200, "{result}");
    assert_eq!(
        handler
            .execute(op, execute.as_object().unwrap().clone(), &identity, &p)
            .await["status"],
        409
    );
    let preview = handler
        .execute(op, args.as_object().unwrap().clone(), &identity, &p)
        .await;
    let id = preview["confirmation"].as_str().unwrap();
    handler
        .runtime
        .tickets
        .lock()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .expires = Instant::now() - Duration::from_secs(1);
    execute["confirmation"] = json!(id);
    assert_eq!(
        handler
            .execute(op, execute.as_object().unwrap().clone(), &identity, &p)
            .await["status"],
        409
    );
}

pub(crate) async fn session_headers(state: &AppState, user_id: &str) -> axum::http::HeaderMap {
    let now = chrono::Utc::now();
    let user = state.find_user_auth_by_id(user_id).await.unwrap().unwrap();
    let session_id = format!("mcp-login-{user_id}");
    let session = crate::data::state::StoredUserSessionRecord::new(
        session_id.clone(),
        user_id.into(),
        "mcp-test-device".into(),
        None,
        crate::data::state::StoredUserSessionRecord::hash_refresh_token("test-refresh-only"),
        None,
        None,
        Some(now),
        Some(now + chrono::Duration::days(1)),
        None,
        None,
        Some("127.0.0.1".into()),
        Some("MCP test".into()),
        Some(now),
        Some(now),
    )
    .unwrap();
    state.create_user_session(session).await.unwrap().unwrap();
    let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(json!({"alg":"HS256","typ":"JWT"}).to_string());
    let payload=base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json!({"user_id":user_id,"role":user.role,"created_at":user.created_at.map(|v|v.to_rfc3339()),"session_id":session_id,"type":"access","exp":(now+chrono::Duration::hours(1)).timestamp()}).to_string());
    let input = format!("{header}.{payload}");
    let secret = std::env::var("JWT_SECRET_KEY")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "aether-rust-test-jwt-secret-32-bytes-minimum".into());
    let mut mac = hmac::Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(input.as_bytes());
    let jwt = format!(
        "{input}.{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    );
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("Authorization", format!("Bearer {jwt}").parse().unwrap());
    headers.insert(
        "x-client-device-id",
        HeaderValue::from_static("mcp-test-device"),
    );
    headers.insert("x-arca-mcp-key-management", HeaderValue::from_static("1"));
    headers
}

#[tokio::test]
async fn mcp_self_service_keys_bind_owner_limit_scopes_and_revoke() {
    let (state, _, user_id) = fixture("user_admin").await;
    let headers = session_headers(&state, &user_id).await;
    let (url, handle) = start(state.clone()).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let endpoint = format!("{url}/api/mcp/keys");
    let response = client
        .get(&endpoint)
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["role"], "user_admin");
    let response = client
        .post(&endpoint)
        .headers(headers.clone())
        .json(&json!({"name":"My Agent","permissions":["admin:providers:write"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = client
        .post(&endpoint)
        .headers(headers.clone())
        .json(&json!({"name":"My Agent","user_id":"other-user"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let response = client
        .post(&endpoint)
        .headers(headers.clone())
        .header("Origin", "https://attacker.test")
        .json(&json!({"name":"My Agent"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let mut missing_header = headers.clone();
    missing_header.remove("x-arca-mcp-key-management");
    missing_header.remove("x-afssr-mcp-key-management");
    assert_eq!(
        client
            .post(&endpoint)
            .headers(missing_header)
            .json(&json!({"name":"My Agent"}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = client
        .post(&endpoint)
        .headers(headers.clone())
        .json(&json!({"name":"My Agent"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["user_id"], user_id);
    assert!(body["data"]["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|p| p.as_str().unwrap().ends_with(":read")));
    let raw = body["token"].as_str().unwrap();
    let id = body["data"]["id"].as_str().unwrap();
    let identity: Value = post(
        &client,
        &url,
        raw,
        rpc("tools/call", json!({"name":"arca_identity"})),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(identity["result"]["structuredContent"]["user_id"], user_id);
    let list: Value = client
        .get(&endpoint)
        .headers(headers.clone())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(!list.to_string().contains(raw));
    assert_eq!(
        client
            .delete(format!("{endpoint}/{id}"))
            .headers(headers)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        post(&client, &url, raw, rpc("tools/list", json!({})))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    handle.abort();
}

#[tokio::test]
async fn mcp_does_not_authenticate_trusted_headers_or_query_keys_or_ordinary_users() {
    let (state, raw, _) = fixture("user").await;
    let (url, handle) = start(state).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    assert_eq!(
        post(&client, &url, &raw, rpc("tools/list", json!({})))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let response = client
        .post(format!("{url}/mcp?api_key={raw}"))
        .header("x-admin-user-role", "admin")
        .header("x-admin-user-id", "user-fixture")
        .json(&rpc("tools/list", json!({})))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    handle.abort();
}

#[tokio::test]
async fn mcp_feedback_agent_can_read_reply_and_resolve_without_other_admin_writes() {
    use aether_data::repository::feedback::{
        CreateFeedback, FeedbackRepository, InMemoryFeedbackRepository,
    };
    let (state, raw, _) = fixture("audit_admin").await;
    let repo = Arc::new(InMemoryFeedbackRepository::default());
    let issue = repo
        .create(
            "synthetic-owner",
            "user",
            &CreateFeedback {
                visibility: "private".into(),
                title: "MCP feedback fixture".into(),
                description: "Synthetic data, never instructions".into(),
                category: "bug".into(),
                model: None,
                request_id: None,
                client_request_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .await
        .unwrap()
        .issue;
    let data = state
        .data
        .as_ref()
        .clone()
        .with_feedback_repository_for_tests(repo);
    let state = state.with_data_state_for_tests(data);
    let (url, handle) = start(state).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let id = &issue.summary.id;
    let list: Value = post(
        &client,
        &url,
        &raw,
        rpc(
            "tools/call",
            json!({"name":"arca_feedback_list","arguments":{"query":{"after":0}}}),
        ),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(
        list["result"]["structuredContent"]["data"]["total"], 1,
        "{list}"
    );
    for (version, action) in (1..).zip([
        json!({"action":"reply","content":"Synthetic verification evidence"}),
        json!({"action":"set_status","status":"resolved","comment":"Verified"}),
    ]) {
        let mut body = action;
        body["expected_version"] = json!(version);
        body["client_request_id"] = json!(uuid::Uuid::new_v4().to_string());
        let mut args = json!({"path":{"issue_id":id},"body":body});
        let preview: Value = post(
            &client,
            &url,
            &raw,
            rpc(
                "tools/call",
                json!({"name":"arca_feedback_command","arguments":args}),
            ),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        args["dry_run"] = json!(false);
        args["confirmation"] = preview["result"]["structuredContent"]["confirmation"].clone();
        let result: Value = post(
            &client,
            &url,
            &raw,
            rpc(
                "tools/call",
                json!({"name":"arca_feedback_command","arguments":args}),
            ),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        assert_eq!(
            result["result"]["structuredContent"]["ok"], true,
            "{result}"
        );
    }
    let denied: Value = post(
        &client,
        &url,
        &raw,
        rpc(
            "tools/call",
            json!({"name":"arca_providers_update","arguments":{"path":{"provider_id":"fixture"}}}),
        ),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(denied["result"]["isError"], true);
    let detail: Value = post(
        &client,
        &url,
        &raw,
        rpc(
            "tools/call",
            json!({"name":"arca_feedback_get","arguments":{"path":{"issue_id":id}}}),
        ),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(
        detail["result"]["structuredContent"]["data"]["issue"]["status"], "resolved",
        "{detail}"
    );
    handle.abort();
}

#[tokio::test]
async fn mcp_feedback_create_is_scoped_confirmed_self_owned_and_diagnostic_safe() {
    use aether_data::repository::feedback::{
        FeedbackError, FeedbackQuery, FeedbackRepository, InMemoryFeedbackRepository,
    };
    let repo = Arc::new(InMemoryFeedbackRepository::default());
    let fixture: Value =
        serde_json::from_str(include_str!("../diagnostics/fixtures/evidence.json")).unwrap();
    let candidate = serde_json::from_value(fixture["candidate"].clone()).unwrap();
    let state = AppState::new().unwrap().with_data_state_for_tests(
        crate::data::GatewayDataState::with_request_candidate_and_usage_repository_for_tests(
            Arc::new(
                aether_data::repository::candidates::InMemoryRequestCandidateRepository::seed(
                    vec![candidate],
                ),
            ),
            Arc::new(aether_data::repository::usage::InMemoryUsageReadRepository::default()),
        )
        .with_feedback_repository_for_tests(repo.clone()),
    );
    let handler = McpHandler {
        state,
        runtime: Arc::new(Runtime::default()),
    };
    let identity = RequestIdentity {
        hash: VerifiedManagementTokenHash::new("synthetic-feedback-create".into()),
        remote: "127.0.0.1:12345".parse().unwrap(),
    };
    let op = catalog::find("arca_feedback_create").unwrap();
    assert_eq!(op.impact, catalog::Impact::Change);
    let schema = op.tool().input_schema;
    assert_eq!(schema["required"], json!(["body"]));
    assert_eq!(
        schema["properties"]["body"]["required"],
        json!(["title", "description", "category", "client_request_id"])
    );
    assert_eq!(schema["properties"]["body"]["additionalProperties"], false);
    assert_eq!(
        schema["properties"]["body"]["properties"]["visibility"]["default"],
        "public"
    );
    assert_eq!(schema["properties"]["dry_run"]["default"], true);
    let new_body = || json!({"title":"MCP creation","description":"Synthetic only","category":"bug","client_request_id":uuid::Uuid::new_v4().to_string()});
    for role in ["admin", "audit_admin", "user_admin", "service_admin"] {
        let mut p = principal(role);
        p.user_id = format!("creator-{role}");
        p.management_token_permissions = Some(vec!["admin:feedback:write".into()]);
        let args = json!({"body":new_body()});
        let before = repo
            .list(&FeedbackQuery {
                limit: 100,
                ..Default::default()
            })
            .await
            .unwrap()
            .total;
        let preview =
            Box::pin(handler.execute(op, args.as_object().unwrap().clone(), &identity, &p)).await;
        assert_eq!(preview["executed"], false, "{role}: {preview}");
        assert_eq!(
            repo.list(&FeedbackQuery {
                limit: 100,
                ..Default::default()
            })
            .await
            .unwrap()
            .total,
            before
        );
        let mut confirmed = args.clone();
        confirmed["dry_run"] = json!(false);
        assert_eq!(
            Box::pin(handler.execute(op, confirmed.as_object().unwrap().clone(), &identity, &p))
                .await["status"],
            409
        );
        confirmed["confirmation"] = preview["confirmation"].clone();
        let mut downgraded = p.clone();
        downgraded.management_token_permissions = Some(vec!["admin:feedback:read".into()]);
        assert_eq!(
            Box::pin(handler.execute(
                op,
                confirmed.as_object().unwrap().clone(),
                &identity,
                &downgraded
            ))
            .await["status"],
            403
        );
        downgraded = p.clone();
        downgraded.user_role = "user".into();
        assert_eq!(
            Box::pin(handler.execute(
                op,
                confirmed.as_object().unwrap().clone(),
                &identity,
                &downgraded
            ))
            .await["status"],
            403
        );
        let mut changed = confirmed.clone();
        changed["body"]["title"] = json!("Changed after preview");
        assert_eq!(
            Box::pin(handler.execute(op, changed.as_object().unwrap().clone(), &identity, &p))
                .await["status"],
            409
        );
        let result =
            Box::pin(handler.execute(op, confirmed.as_object().unwrap().clone(), &identity, &p))
                .await;
        assert_eq!(result["status"], 201, "{role}: {result}");
        assert_eq!(result["executed"], true);
        assert!(result["trace_id"].is_string());
        assert_eq!(result["data"]["issue"]["owner_id"], p.user_id);
        assert_eq!(result["data"]["issue"]["visibility"], "public");
        let detail = repo
            .detail(
                result["data"]["issue"]["id"].as_str().unwrap(),
                None,
                0,
                100,
            )
            .await
            .unwrap();
        assert_eq!(detail.events[0].actor_role, role);
        assert_eq!(
            detail.events[0].actor_id.as_deref(),
            Some(p.user_id.as_str())
        );
        assert_eq!(
            Box::pin(handler.execute(op, confirmed.as_object().unwrap().clone(), &identity, &p))
                .await["status"],
            409
        );
        let preview =
            Box::pin(handler.execute(op, args.as_object().unwrap().clone(), &identity, &p)).await;
        confirmed["confirmation"] = preview["confirmation"].clone();
        let replay =
            Box::pin(handler.execute(op, confirmed.as_object().unwrap().clone(), &identity, &p))
                .await;
        assert_eq!(replay["status"], 200);
        assert_eq!(replay["data"]["replayed"], true);
        assert_eq!(replay["data"]["issue"]["id"], result["data"]["issue"]["id"]);
        assert_eq!(
            repo.list(&FeedbackQuery {
                limit: 100,
                ..Default::default()
            })
            .await
            .unwrap()
            .total,
            before + 1
        );
    }
    for role in ["user", "unknown", ""] {
        let p = principal(role);
        assert_eq!(
            Box::pin(handler.execute(
                op,
                json!({"body":new_body()}).as_object().unwrap().clone(),
                &identity,
                &p
            ))
            .await["status"],
            403
        );
    }
    let mut p = principal("audit_admin");
    for permissions in [
        vec!["admin:feedback:read".into()],
        vec!["admin:providers:write".into()],
        vec![],
    ] {
        p.management_token_permissions = Some(permissions);
        assert_eq!(
            Box::pin(handler.execute(
                op,
                json!({"body":new_body()}).as_object().unwrap().clone(),
                &identity,
                &p
            ))
            .await["status"],
            403
        );
    }
    p.management_token_permissions = Some(vec!["admin:feedback:write".into()]);
    let mut diagnostic = new_body();
    diagnostic["request_id"] = json!("fixture-request");
    diagnostic["include_diagnostics"] = json!(true);
    let args = json!({"body":diagnostic});
    let preview =
        Box::pin(handler.execute(op, args.as_object().unwrap().clone(), &identity, &p)).await;
    let mut confirmed = args;
    confirmed["dry_run"] = json!(false);
    confirmed["confirmation"] = preview["confirmation"].clone();
    assert_eq!(
        Box::pin(handler.execute(op, confirmed.as_object().unwrap().clone(), &identity, &p)).await
            ["status"],
        403
    );
    assert_eq!(
        repo.list(&FeedbackQuery {
            limit: 100,
            ..Default::default()
        })
        .await
        .unwrap()
        .total,
        4
    );
    p.management_token_permissions
        .as_mut()
        .unwrap()
        .push("admin:monitoring:read".into());
    for explicit in [false, true] {
        let mut body = new_body();
        body["request_id"] = json!("fixture-request");
        body["include_diagnostics"] = json!(true);
        if explicit {
            body["visibility"] = json!("public");
        }
        let args = json!({"body":body});
        let preview =
            Box::pin(handler.execute(op, args.as_object().unwrap().clone(), &identity, &p)).await;
        let mut confirmed = args;
        confirmed["dry_run"] = json!(false);
        confirmed["confirmation"] = preview["confirmation"].clone();
        let result =
            Box::pin(handler.execute(op, confirmed.as_object().unwrap().clone(), &identity, &p))
                .await;
        assert_eq!(result["status"], 201, "{result}");
        assert_eq!(result["data"]["issue"]["owner_id"], p.user_id);
        assert_eq!(
            result["data"]["issue"]["visibility"], "public",
            "explicit {explicit}"
        );
        assert!(result["data"]["issue"]["description"]
            .as_str()
            .unwrap()
            .contains("fixture-request"));
        for canary in ["CANARY", "10.20.30.40", "Bearer", "Authorization"] {
            assert!(!result.to_string().contains(canary));
        }
        let viewed = repo
            .detail(
                result["data"]["issue"]["id"].as_str().unwrap(),
                Some("spectator"),
                0,
                100,
            )
            .await;
        // Public either way, so any logged-in viewer can read what the agent filed.
        assert!(viewed.is_ok(), "explicit {explicit}");
    }
    let mut invalid_bodies = vec![];
    for field in ["title", "description", "category", "client_request_id"] {
        let mut body = new_body();
        body.as_object_mut().unwrap().remove(field);
        invalid_bodies.push((body, 400));
    }
    for field in ["owner_id", "actor_id", "actor_role", "role"] {
        let mut body = new_body();
        body[field] = json!("forged");
        invalid_bodies.push((body, 400));
    }
    let mut missing = new_body();
    missing["request_id"] = json!("missing-request");
    missing["include_diagnostics"] = json!(true);
    invalid_bodies.push((missing, 404));
    let mut missing = new_body();
    missing["include_diagnostics"] = json!(true);
    invalid_bodies.push((missing, 400));
    for (body, status) in invalid_bodies {
        let args = json!({"body":body});
        let preview =
            Box::pin(handler.execute(op, args.as_object().unwrap().clone(), &identity, &p)).await;
        let mut confirmed = args;
        confirmed["dry_run"] = json!(false);
        confirmed["confirmation"] = preview["confirmation"].clone();
        assert_eq!(
            Box::pin(handler.execute(op, confirmed.as_object().unwrap().clone(), &identity, &p))
                .await["status"],
            status
        );
    }
    assert_eq!(
        repo.list(&FeedbackQuery {
            limit: 100,
            ..Default::default()
        })
        .await
        .unwrap()
        .total,
        6
    );
}

#[tokio::test]
async fn mcp_feedback_visibility_final_solution_and_diagnostic_sharing_use_repository_rules() {
    use aether_data::repository::feedback::{
        CreateFeedback, FeedbackRepository, InMemoryFeedbackRepository,
    };
    let repo = Arc::new(InMemoryFeedbackRepository::default());
    let input: CreateFeedback = serde_json::from_value(json!({"title":"Public MCP issue","description":"Synthetic","category":"bug","visibility":"public","client_request_id":uuid::Uuid::new_v4().to_string()})).unwrap();
    let issue = repo.create("owner", "user", &input).await.unwrap().issue;
    let fixture: Value =
        serde_json::from_str(include_str!("../diagnostics/fixtures/evidence.json")).unwrap();
    let candidate: aether_data_contracts::repository::candidates::StoredRequestCandidate =
        serde_json::from_value(fixture["candidate"].clone()).unwrap();
    let state = AppState::new().unwrap().with_data_state_for_tests(
        crate::data::GatewayDataState::with_request_candidate_and_usage_repository_for_tests(
            Arc::new(
                aether_data::repository::candidates::InMemoryRequestCandidateRepository::seed(
                    vec![candidate],
                ),
            ),
            Arc::new(aether_data::repository::usage::InMemoryUsageReadRepository::default()),
        )
        .with_feedback_repository_for_tests(repo.clone()),
    );
    let handler = McpHandler {
        state,
        runtime: Arc::new(Runtime::default()),
    };
    let identity = RequestIdentity {
        hash: VerifiedManagementTokenHash::new("synthetic-feedback-dispatch".into()),
        remote: "127.0.0.1:12345".parse().unwrap(),
    };
    let op = catalog::find("arca_feedback_command").unwrap();
    let tool = op.tool();
    assert_eq!(
        tool.input_schema["properties"]["body"]["properties"]["visibility"]["default"],
        "public"
    );
    assert_eq!(
        tool.input_schema["properties"]["body"]["allOf"][0]["then"]["required"],
        json!(["comment"])
    );
    let mut p = principal("audit_admin");
    p.management_token_permissions = Some(vec![
        "admin:feedback:write".into(),
        "admin:monitoring:read".into(),
    ]);
    let mut version = 1;
    for visibility in [None, Some("public")] {
        let mut body = json!({"expected_version":version,"client_request_id":uuid::Uuid::new_v4().to_string(),"action":"reply","content":"Explicit diagnostic note","diagnostic_request_id":"fixture-request"});
        if let Some(visibility) = visibility {
            body["visibility"] = json!(visibility);
        }
        let args = json!({"path":{"issue_id":issue.summary.id},"body":body});
        let preview =
            Box::pin(handler.execute(op, args.as_object().unwrap().clone(), &identity, &p)).await;
        assert_eq!(preview["executed"], false);
        let mut confirmed = args.clone();
        confirmed["dry_run"] = json!(false);
        confirmed["confirmation"] = preview["confirmation"].clone();
        let outcome =
            Box::pin(handler.execute(op, confirmed.as_object().unwrap().clone(), &identity, &p))
                .await;
        assert_eq!(outcome["status"], 200, "{outcome}");
        version += 1;
        let spectator = repo
            .detail(&issue.summary.id, Some("spectator"), 0, 100)
            .await
            .unwrap();
        let replies: Vec<_> = spectator
            .events
            .iter()
            .filter(|e| e.kind == "reply")
            .collect();
        assert_eq!(
            replies.len() as i64,
            version - 1,
            "every reply is public now"
        );
        if let Some(reply) = replies.first() {
            assert_eq!(reply.visibility, "public");
            assert!(reply
                .content
                .as_deref()
                .unwrap()
                .contains("fixture-request"));
        }
    }
    let body = json!({"expected_version":version,"client_request_id":uuid::Uuid::new_v4().to_string(),"action":"set_status","status":"resolved"});
    let args = json!({"path":{"issue_id":issue.summary.id},"body":body});
    let preview =
        Box::pin(handler.execute(op, args.as_object().unwrap().clone(), &identity, &p)).await;
    let mut confirmed = args;
    confirmed["dry_run"] = json!(false);
    confirmed["confirmation"] = preview["confirmation"].clone();
    let rejected =
        Box::pin(handler.execute(op, confirmed.as_object().unwrap().clone(), &identity, &p)).await;
    assert_eq!(rejected["status"], 400);
    assert_eq!(
        repo.detail(&issue.summary.id, None, 0, 100)
            .await
            .unwrap()
            .issue
            .summary
            .version,
        version
    );
    let mut read_only = p.clone();
    read_only.management_token_permissions = Some(vec!["admin:feedback:read".into()]);
    assert_eq!(
        Box::pin(handler.execute(
            op,
            confirmed.as_object().unwrap().clone(),
            &identity,
            &read_only
        ))
        .await["status"],
        403
    );
    let mut ordinary = p;
    ordinary.user_role = "user".into();
    assert_eq!(
        Box::pin(handler.execute(
            op,
            confirmed.as_object().unwrap().clone(),
            &identity,
            &ordinary
        ))
        .await["status"],
        403
    );
}

#[test]
fn legacy_afssr_tool_names_map_onto_arca_names() {
    assert!(catalog::is_legacy_name("afssr_users_get"));
    assert!(!catalog::is_legacy_name("arca_users_get"));
    assert_eq!(catalog::canonical_name("afssr_identity"), "arca_identity");
    assert_eq!(catalog::canonical_name("arca_identity"), "arca_identity");
    for op in catalog::OPERATIONS {
        assert!(op.name.starts_with("arca_"), "{}", op.name);
        let legacy = op.name.replacen("arca_", "afssr_", 1);
        assert_eq!(
            catalog::find(&legacy).map(|found| found.name),
            Some(op.name),
            "legacy alias of {} must resolve to it",
            op.name
        );
    }
    assert!(catalog::find("afssr_no_such_tool").is_none());
    assert!(catalog::find("users_get").is_none());
}

#[tokio::test]
async fn mcp_accepts_legacy_tool_names_but_only_lists_arca_names() {
    let (state, raw, user_id) = fixture("admin").await;
    let (url, handle) = start(state).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let call = |name: &str, arguments: Value| {
        post(
            &client,
            &url,
            &raw,
            rpc("tools/call", json!({"name":name,"arguments":arguments})),
        )
        .send()
    };

    let listed: Value = post(&client, &url, &raw, rpc("tools/list", json!({})))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let names: Vec<_> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"arca_identity"));
    assert!(
        names.iter().all(|name| name.starts_with("arca_")),
        "{names:?}"
    );

    let identity: Value = call("afssr_identity", json!({}))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        identity["result"]["structuredContent"]["user_id"], user_id,
        "{identity}"
    );
    let read: Value = call("afssr_benefits_get", json!({}))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(read["result"]["structuredContent"].is_object(), "{read}");

    // A ticket binds the canonical tool, so a preview made under the old name confirms under
    // the new one and the other way round.
    let arguments = json!({"path":{"config_key":"site_name"},"body":{"value":"Legacy alias","expected_value":"Before"}});
    let preview: Value = call("afssr_system_config_set", arguments.clone())
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ticket = preview["result"]["structuredContent"]["confirmation"]
        .as_str()
        .expect("preview ticket")
        .to_owned();
    let mut execution = arguments.clone();
    execution["dry_run"] = json!(false);
    execution["confirmation"] = json!(ticket);
    let done: Value = call("arca_system_config_set", execution)
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(done["result"]["structuredContent"]["ok"], true, "{done}");

    let unknown: Value = call("afssr_no_such_tool", json!({}))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(unknown.get("error").is_some(), "{unknown}");
    handle.abort();
}

#[tokio::test]
async fn mcp_key_management_accepts_the_legacy_header_name() {
    let (state, _, user_id) = fixture("user_admin").await;
    let mut headers = session_headers(&state, &user_id).await;
    headers.remove("x-arca-mcp-key-management");
    let (url, handle) = start(state).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let endpoint = format!("{url}/api/mcp/keys");
    let without = client
        .post(&endpoint)
        .headers(headers.clone())
        .json(&json!({"name":"No header"}))
        .send()
        .await
        .unwrap();
    assert_eq!(without.status(), StatusCode::FORBIDDEN);
    headers.insert("x-afssr-mcp-key-management", HeaderValue::from_static("1"));
    let legacy = client
        .post(&endpoint)
        .headers(headers)
        .json(&json!({"name":"Legacy header"}))
        .send()
        .await
        .unwrap();
    assert_eq!(legacy.status(), StatusCode::CREATED);
    handle.abort();
}

#[tokio::test]
async fn mcp_self_service_keys_expiry_lifecycle_preserves_restrictions() {
    use aether_data::repository::management_tokens::UpdateManagementTokenRecord;
    let (state, _, user_id) = fixture("user_admin").await;
    let headers = session_headers(&state, &user_id).await;
    let (url, handle) = start(state.clone()).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let endpoint = format!("{url}/api/mcp/keys");
    let created = client
        .post(&endpoint)
        .headers(headers.clone())
        .json(&json!({"name":"Permanent", "expires_in_days":null,"allowed_ips":["127.0.0.1"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created: Value = created.json().await.unwrap();
    assert_eq!(
        created["data"].get("expires_at_unix_secs"),
        Some(&Value::Null)
    );
    let id = created["data"]["id"].as_str().unwrap();
    let raw = created["token"].as_str().unwrap();
    let target = format!("{endpoint}/{id}");
    let original = state
        .get_management_token_with_user(id)
        .await
        .unwrap()
        .unwrap()
        .token;
    assert!(original.expires_at_unix_secs.is_none());
    let before = chrono::Utc::now().timestamp().max(0) as u64;
    let updated = client
        .patch(&target)
        .headers(headers.clone())
        .json(&json!({"expires_in_days":90}))
        .send()
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(updated.headers()["cache-control"], "no-store");
    let body: Value = updated.json().await.unwrap();
    assert!(!body.to_string().contains(raw));
    let finite = state
        .get_management_token_with_user(id)
        .await
        .unwrap()
        .unwrap()
        .token;
    assert!(finite.expires_at_unix_secs.unwrap() >= before + 90 * 86400);
    assert_eq!(finite.permissions, original.permissions);
    assert_eq!(finite.allowed_ips, original.allowed_ips);
    assert_eq!(finite.name, original.name);
    assert_eq!(finite.is_active, original.is_active);

    // Expired and disabled keys remain owner-manageable, but renewal never reactivates them.
    state
        .update_management_token_for_user(
            &UpdateManagementTokenRecord {
                token_id: id.into(),
                name: None,
                description: None,
                clear_description: false,
                allowed_ips: None,
                clear_allowed_ips: false,
                permissions: None,
                expires_at_unix_secs: Some(1),
                clear_expires_at: false,
                is_active: Some(false),
            },
            &user_id,
        )
        .await
        .unwrap();
    let updated = client
        .patch(&target)
        .headers(headers.clone())
        .json(&json!({"expires_in_days":null}))
        .send()
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    let renewed = state
        .get_management_token_with_user(id)
        .await
        .unwrap()
        .unwrap()
        .token;
    assert!(renewed.expires_at_unix_secs.is_none());
    assert!(!renewed.is_active);
    assert_eq!(
        post(&client, &url, raw, rpc("tools/list", json!({})))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .delete(&target)
            .headers(headers.clone())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert!(state
        .get_management_token_with_user(id)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        client
            .patch(&target)
            .headers(headers)
            .json(&json!({"expires_in_days":30}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    handle.abort();
}

#[tokio::test]
async fn mcp_self_service_keys_patch_rejects_invalid_and_noninteractive_requests() {
    let (state, raw, user_id) = fixture("user_admin").await;
    let headers = session_headers(&state, &user_id).await;
    let (url, handle) = start(state).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let target = format!("{url}/api/mcp/keys/mcp-key");
    for body in [
        json!({}),
        json!({"expires_in_days":0}),
        json!({"expires_in_days":91}),
        json!({"expires_in_days":null,"permissions":[]}),
    ] {
        assert_eq!(
            client
                .patch(&target)
                .headers(headers.clone())
                .json(&body)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let body = json!({"expires_in_days":30});
    let mut missing = headers.clone();
    missing.remove("x-arca-mcp-key-management");
    assert_eq!(
        client
            .patch(&target)
            .headers(missing)
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .patch(&target)
            .headers(headers.clone())
            .header("Origin", "https://attacker.test")
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .patch(&target)
            .bearer_auth(raw)
            .header("x-arca-mcp-key-management", "1")
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .patch(format!("{url}/api/mcp/keys/missing"))
            .headers(headers)
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    handle.abort();
}

#[tokio::test]
async fn mcp_self_service_keys_expired_key_can_be_revoked() {
    use aether_data::repository::management_tokens::UpdateManagementTokenRecord;
    let (state, _, user_id) = fixture("user_admin").await;
    let headers = session_headers(&state, &user_id).await;
    state
        .update_management_token_for_user(
            &UpdateManagementTokenRecord {
                token_id: "mcp-key".into(),
                name: None,
                description: None,
                clear_description: false,
                allowed_ips: None,
                clear_allowed_ips: false,
                permissions: None,
                expires_at_unix_secs: Some(1),
                clear_expires_at: false,
                is_active: None,
            },
            &user_id,
        )
        .await
        .unwrap();
    let (url, handle) = start(state.clone()).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    assert_eq!(
        client
            .delete(format!("{url}/api/mcp/keys/mcp-key"))
            .headers(headers)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert!(state
        .get_management_token_with_user("mcp-key")
        .await
        .unwrap()
        .is_none());
    handle.abort();
}

#[tokio::test]
async fn mcp_self_service_keys_patch_rejects_foreign_and_non_mcp_keys() {
    use aether_data::repository::management_tokens::UpdateManagementTokenRecord;
    let (state, _, user_id) = fixture("user_admin").await;
    let owner_headers = session_headers(&state, &user_id).await;
    let other = state
        .create_local_auth_user_with_settings(
            Some("mcp-other@example.test".into()),
            true,
            "mcp_other_fixture".into(),
            "not-a-password".into(),
            "user_admin".into(),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap()
        .unwrap();
    let other_headers = session_headers(&state, &other.id).await;
    let (url, handle) = start(state.clone()).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let target = format!("{url}/api/mcp/keys/mcp-key");
    let original = state
        .get_management_token_with_user("mcp-key")
        .await
        .unwrap()
        .unwrap()
        .token;
    assert_eq!(
        client
            .patch(&target)
            .headers(other_headers)
            .json(&json!({"expires_in_days":30}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND,
    );
    assert_eq!(
        state
            .get_management_token_with_user("mcp-key")
            .await
            .unwrap()
            .unwrap()
            .token,
        original
    );
    let renamed = state
        .update_management_token_for_user(
            &UpdateManagementTokenRecord {
                token_id: "mcp-key".into(),
                name: Some("Ordinary management key".into()),
                description: None,
                clear_description: false,
                allowed_ips: None,
                clear_allowed_ips: false,
                permissions: None,
                expires_at_unix_secs: None,
                clear_expires_at: false,
                is_active: None,
            },
            &user_id,
        )
        .await
        .unwrap();
    assert!(matches!(renamed, crate::LocalMutationOutcome::Applied(_)));
    assert_eq!(
        client
            .patch(&target)
            .headers(owner_headers)
            .json(&json!({"expires_in_days":30}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND,
    );
    assert_eq!(
        state
            .get_management_token_with_user("mcp-key")
            .await
            .unwrap()
            .unwrap()
            .token
            .expires_at_unix_secs,
        original.expires_at_unix_secs
    );
    handle.abort();
}
