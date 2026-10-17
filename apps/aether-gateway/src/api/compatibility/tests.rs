use super::*;
use axum::http::{HeaderMap, Method, Uri};

fn profile(client: Client, source: Protocol, target: Protocol) -> Profile {
    Profile {
        id: uuid::Uuid::new_v4(),
        config: ProfileConfig {
            client,
            client_version: "synthetic-1".into(),
            client_protocol: source,
            model: "fixture-model".into(),
            upstream_format: target,
            mapped_model: "fixture-upstream".into(),
            channel: None,
        },
    }
}

#[test]
fn real_offline_conversions_execute_all_nine_protocol_pairs() {
    for source in [Protocol::Chat, Protocol::Responses, Protocol::Claude] {
        for target in [Protocol::Chat, Protocol::Responses, Protocol::Claude] {
            let p = profile(Client::Generic, source, target);
            let run = runner::run(&p);
            assert_eq!(run.cases.len(), 13);
            for case in &run.cases[..9] {
                assert_eq!(
                    case.status,
                    Status::Passed,
                    "{source:?}->{target:?}: {}: {}",
                    case.case,
                    case.reason
                );
            }
            assert!(run.cases[9..].iter().all(|c| c.status == Status::NotTested));
            let snapshot = Snapshot {
                profiles: vec![p],
                runs: vec![run],
                ..Default::default()
            };
            snapshot.validate().unwrap();
        }
    }
    for (client, source) in [
        (Client::Codex, Protocol::Responses),
        (Client::ClaudeCode, Protocol::Claude),
        (Client::OpenCode, Protocol::Chat),
    ] {
        let p = profile(client, source, Protocol::Claude);
        p.config.validate().unwrap();
        assert!(runner::run(&p).cases[..9]
            .iter()
            .all(|c| c.status == Status::Passed));
    }
}

#[test]
fn strict_schema_unknown_status_and_sensitive_input_fail_closed() {
    let mut value =
        serde_json::to_value(profile(Client::Generic, Protocol::Chat, Protocol::Claude)).unwrap();
    value["config"]["api_key"] = json!("synthetic-do-not-save");
    assert!(serde_json::from_value::<Profile>(value).is_err());
    for input in [
        "sk-secret",
        "Bearer-secret",
        "https://example.test",
        "body with spaces",
    ] {
        let mut p = profile(Client::Generic, Protocol::Chat, Protocol::Claude);
        p.config.model = input.into();
        assert!(p.config.validate().is_err());
    }
    assert!(serde_json::from_value::<Status>(json!("unknown")).is_err());
    assert!(serde_json::from_value::<Protocol>(json!("made-up")).is_err());
    let mut snapshot = Snapshot::default();
    snapshot.schema_version = 2;
    assert!(snapshot.validate().is_err());
    let p = profile(Client::Generic, Protocol::Chat, Protocol::Claude);
    let mut run = runner::run(&p);
    run.cases[0].reason = "untrusted customer body".into();
    snapshot = Snapshot {
        profiles: vec![p],
        runs: vec![run],
        ..Default::default()
    };
    assert!(snapshot.validate().is_err());
}

#[test]
fn fingerprint_changes_and_stale_results_never_become_current_passes() {
    let p = profile(Client::Codex, Protocol::Responses, Protocol::Claude);
    let run = runner::run(&p);
    let mut snapshot = Snapshot {
        profiles: vec![p.clone()],
        runs: vec![run],
        ..Default::default()
    };
    assert_eq!(payload(&snapshot, "admin")["runs"][0]["stale"], false);
    snapshot.profiles[0].config.client_version = "synthetic-2".into();
    assert_ne!(
        p.config.fingerprint(),
        snapshot.profiles[0].config.fingerprint()
    );
    assert_eq!(payload(&snapshot, "admin")["runs"][0]["stale"], true);
    let json = payload(&snapshot, "admin");
    assert_eq!(json["real_model_status"], "not_tested");
    assert_eq!(json["real_client_status"], "not_tested");
    let saved = serde_json::to_string(&snapshot).unwrap();
    assert!(
        !saved.contains("fixture-question")
            && !saved.contains("fixture-answer")
            && !saved.contains("iVBOR")
    );
}

#[test]
fn precise_routes_and_role_boundaries() {
    for (method, path) in [
        (Method::GET, "/api/admin/compatibility"),
        (Method::POST, "/api/admin/compatibility/profiles"),
        (Method::POST, "/api/admin/compatibility/runs"),
    ] {
        let route = crate::control::classify_control_route(
            &method,
            &path.parse::<Uri>().unwrap(),
            &HeaderMap::new(),
        )
        .unwrap();
        assert_eq!(route.route_family.as_deref(), Some("compatibility_manage"));
        for role in [
            "admin",
            "audit_admin",
            "user_admin",
            "service_admin",
            "user",
            "unknown",
        ] {
            let permission = if method == Method::GET {
                "admin:compatibility:read"
            } else {
                "admin:compatibility:write"
            };
            assert_eq!(
                crate::roles::role_has_management_permission(role, permission),
                if method == Method::GET {
                    crate::roles::can_access_admin_console(role)
                } else {
                    crate::roles::can_manage_services(role)
                }
            );
        }
    }
    for path in [
        "/api/admin/compatibility-extra",
        "/api/admin/compatibility/runs/extra",
        "/api/admin/compatibility/profiles/x",
    ] {
        let route = crate::control::classify_control_route(
            &Method::POST,
            &path.parse::<Uri>().unwrap(),
            &HeaderMap::new(),
        );
        assert!(!route.is_some_and(|r| r.route_family.as_deref() == Some("compatibility_manage")));
    }
}

#[tokio::test]
async fn no_database_never_claims_a_saved_result_and_readonly_cannot_run() {
    let state = AppState::new()
        .unwrap()
        .with_data_state_for_tests(crate::data::GatewayDataState::disabled());
    let p = profile(Client::Generic, Protocol::Chat, Protocol::Claude);
    let body = serde_json::to_vec(&json!({"expected_revision":0,"profile":p})).unwrap();
    for role in ["audit_admin", "user_admin", "user", "unknown"] {
        assert_eq!(
            dispatch(&state, role, "compatibility_profile", &body)
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            dispatch(&state, role, "compatibility_run", b"{}")
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    for role in ["service_admin", "admin"] {
        assert_eq!(
            dispatch(&state, role, "compatibility_profile", &body)
                .await
                .status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
}

#[tokio::test]
async fn http_role_boundaries_are_enforced_before_any_runner_or_persistence() {
    let state = AppState::new()
        .unwrap()
        .with_data_state_for_tests(crate::data::GatewayDataState::disabled());
    let mut identities = Vec::new();
    for role in [
        "admin",
        "audit_admin",
        "user_admin",
        "service_admin",
        "user",
    ] {
        let user = state
            .create_local_auth_user_with_settings(
                Some(format!("compat-{role}@example.test")),
                true,
                format!("compat-{role}"),
                "fixture-hash".into(),
                role.into(),
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap()
            .unwrap();
        identities.push((
            role,
            crate::api::mcp::test_session_headers(&state, &user.id).await,
        ));
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = crate::router::build_router_with_state(state);
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    assert_eq!(
        client
            .get(format!("http://{addr}/api/admin/compatibility"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    for (role, headers) in identities {
        let read = client
            .get(format!("http://{addr}/api/admin/compatibility"))
            .headers(headers.clone())
            .send()
            .await
            .unwrap();
        assert_eq!(
            read.status(),
            if role == "user" {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::OK
            },
            "read {role}"
        );
        for path in ["profiles", "runs"] {
            let write = client
                .post(format!("http://{addr}/api/admin/compatibility/{path}"))
                .headers(headers.clone())
                .json(&json!({}))
                .send()
                .await
                .unwrap();
            let expected = match role {
                "user" => StatusCode::UNAUTHORIZED,
                "audit_admin" | "user_admin" => StatusCode::FORBIDDEN,
                _ => StatusCode::BAD_REQUEST,
            };
            assert_eq!(write.status(), expected, "write {path} {role}");
        }
        if matches!(role, "admin" | "service_admin") {
            let valid = json!({"expected_revision":0,"profile":profile(Client::Codex, Protocol::Responses, Protocol::Claude)});
            let write = client
                .post(format!("http://{addr}/api/admin/compatibility/profiles"))
                .headers(headers)
                .json(&valid)
                .send()
                .await
                .unwrap();
            assert_eq!(
                write.status(),
                StatusCode::SERVICE_UNAVAILABLE,
                "valid {role} body must reach persistence, not fail empty-body parsing"
            );
        }
    }
    task.abort();
}

async fn body_json(response: Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Explicitly opted-in local disposable database only; never uses DATABASE_URL or repository .env.
#[tokio::test]
#[ignore = "requires AFSSR_COMPAT_TEST_DATABASE_URL for a disposable local afssr_compatibility_test database"]
async fn postgres_compatibility_roundtrip_restart_stale_and_atomic_cas() {
    let url = std::env::var("AFSSR_COMPAT_TEST_DATABASE_URL")
        .expect("explicit disposable DB URL required");
    let parsed = url::Url::parse(&url).unwrap();
    assert_eq!(parsed.host_str(), Some("127.0.0.1"));
    assert_eq!(parsed.path(), "/afssr_compatibility_test");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::query("CREATE TABLE IF NOT EXISTS system_configs (id text PRIMARY KEY, key text UNIQUE NOT NULL, value json NOT NULL, description text, created_at timestamptz, updated_at timestamptz)").execute(&pool).await.unwrap();
    // Refuse to destroy an existing snapshot even in this explicitly disposable database.
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM system_configs")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let make_state = || {
        AppState::new().unwrap().with_data_state_for_tests(
            crate::data::GatewayDataState::from_config(
                crate::data::GatewayDataConfig::from_postgres_url(&url, false),
            )
            .unwrap(),
        )
    };
    let state = make_state();
    let p = profile(Client::Codex, Protocol::Responses, Protocol::Claude);
    let input = serde_json::to_vec(&json!({"expected_revision":0,"profile":p})).unwrap();
    let (first, competing) = tokio::join!(
        dispatch(&state, "service_admin", "compatibility_profile", &input),
        dispatch(&state, "admin", "compatibility_profile", &input),
    );
    assert_eq!(
        [first.status(), competing.status()]
            .iter()
            .filter(|s| **s == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        [first.status(), competing.status()]
            .iter()
            .filter(|s| **s == StatusCode::CONFLICT)
            .count(),
        1
    );
    let saved = if first.status() == StatusCode::OK {
        first
    } else {
        competing
    };
    let saved = body_json(saved).await;
    let run_input = serde_json::to_vec(&json!({"expected_revision":1,"profile_id":p.id,"expected_fingerprint":saved["profiles"][0]["config_fingerprint"]})).unwrap();
    let tested = dispatch(&state, "service_admin", "compatibility_run", &run_input).await;
    assert_eq!(tested.status(), StatusCode::OK);
    let tested = body_json(tested).await;
    assert_eq!(tested["runs"][0]["stale"], false);
    assert!(tested["runs"][0]["run"]["cases"].as_array().unwrap()[..9]
        .iter()
        .all(|c| c["status"] == "passed"));
    assert_eq!(
        dispatch(&state, "admin", "compatibility_run", &run_input)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    drop(state);
    let restarted = make_state();
    let loaded =
        body_json(dispatch(&restarted, "audit_admin", "compatibility_list", b"").await).await;
    assert_eq!(loaded["runs"], tested["runs"]);
    assert_eq!(loaded["can_run"], false);
    let mut changed = p.clone();
    changed.config.mapped_model = "fixture-upstream-v2".into();
    let edit = serde_json::to_vec(&json!({"expected_revision":2,"profile":changed})).unwrap();
    let updated =
        body_json(dispatch(&restarted, "admin", "compatibility_profile", &edit).await).await;
    assert_eq!(updated["runs"][0]["stale"], true);
    let (raw, _) = read(&restarted).await.unwrap();
    let mut a = raw.clone().unwrap();
    a["revision"] = json!(4);
    let mut b = a.clone();
    b["revision"] = json!(5);
    let (one, two) = tokio::join!(
        restarted.data.save_compatibility_snapshot(raw.as_ref(), &a),
        restarted.data.save_compatibility_snapshot(raw.as_ref(), &b)
    );
    assert_ne!(
        one.unwrap(),
        two.unwrap(),
        "exactly one CAS writer must succeed"
    );
    let mut invalid = raw.unwrap();
    invalid["schema_version"] = json!(2);
    sqlx::query("UPDATE system_configs SET value = $1 WHERE key = $2")
        .bind(&invalid)
        .bind(CONFIG_KEY)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        dispatch(&restarted, "admin", "compatibility_profile", &edit)
            .await
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let text =
        sqlx::query_scalar::<_, String>("SELECT value::text FROM system_configs WHERE key = $1")
            .bind(CONFIG_KEY)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        !text.contains("fixture-question")
            && !text.contains("fixture-answer")
            && !text.contains("iVBOR")
    );
    pool.close().await;
}
