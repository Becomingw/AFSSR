use super::*;
use aether_data::repository::{
    candidates::InMemoryRequestCandidateRepository, feedback::InMemoryFeedbackRepository,
    usage::InMemoryUsageReadRepository,
};
use axum::http::HeaderMap;
use std::sync::Arc;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/evidence.json")).unwrap()
}
fn candidate() -> StoredRequestCandidate {
    serde_json::from_value(fixture()["candidate"].clone()).unwrap()
}
fn stage(value: &Diagnostic, id: &str) -> (&'static str, &'static str) {
    let s = value.stages.iter().find(|s| s.id == id).unwrap();
    (s.status, s.code)
}

#[test]
fn format_and_stream_error_fixtures_are_structured_without_free_text() {
    let fixture = fixture();
    for case in fixture["format_errors"].as_array().unwrap() {
        let mut c = candidate();
        c.error_type = Some(case["error_type"].as_str().unwrap().into());
        c.extra_data.as_mut().unwrap()["client_api_format"] = case["format"].clone();
        let result = build(
            "fixture-request",
            &Evidence {
                usage: None,
                candidates: vec![c],
            },
        );
        assert_eq!(
            stage(&result, "upstream"),
            ("failed", case["expected"].as_str().unwrap())
        );
        let encoded = serde_json::to_string(&result).unwrap();
        for secret in [
            "CANARY",
            "10.20.30.40",
            "http://",
            "Bearer",
            "Authorization",
            "messages",
            "upstream_request",
            "upstream_response",
        ] {
            assert!(!encoded.contains(secret), "{secret}: {encoded}");
        }
    }
    for code in fixture["stream_errors"].as_array().unwrap() {
        let mut c = candidate();
        c.error_type = Some(code.as_str().unwrap().into());
        c.status_code = Some(200);
        let result = build(
            "fixture-request",
            &Evidence {
                usage: None,
                candidates: vec![c],
            },
        );
        assert_eq!(result.state, "failed");
        assert_eq!(
            stage(&result, "stream"),
            ("failed", "stream_failed_or_interrupted")
        );
    }
}

#[test]
fn missing_running_conversion_and_unclassified_evidence_never_imply_success() {
    let missing = build("missing", &Evidence::default());
    assert_eq!(missing.state, "unknown");
    assert!(missing.stages.iter().all(|s| s.status == "unknown"));
    let mut c = candidate();
    c.status = RequestCandidateStatus::Streaming;
    c.error_type = None;
    c.status_code = Some(200);
    c.finished_at_unix_ms = None;
    let result = build(
        "fixture-request",
        &Evidence {
            usage: None,
            candidates: vec![c.clone()],
        },
    );
    assert_eq!(result.state, "in_progress");
    assert_eq!(stage(&result, "stream").0, "unknown");
    assert_eq!(stage(&result, "model_acl").0, "unknown");
    assert_eq!(stage(&result, "quota").0, "unknown");
    c.status = RequestCandidateStatus::Skipped;
    c.skip_reason = Some("provider_request_body_build_failed".into());
    let result = build(
        "fixture-request",
        &Evidence {
            usage: None,
            candidates: vec![c.clone()],
        },
    );
    assert_eq!(
        stage(&result, "conversion"),
        ("failed", "format_conversion_failed")
    );
    c.status = RequestCandidateStatus::Failed;
    c.skip_reason = None;
    c.error_type = Some("CANARY-failure".into());
    let result = build(
        "fixture-request",
        &Evidence {
            usage: None,
            candidates: vec![c],
        },
    );
    assert_eq!(result.state, "failed");
    assert_eq!(stage(&result, "upstream").0, "unknown");
}

#[test]
fn contradictory_or_unowned_records_are_not_visible() {
    let mut c = candidate();
    let mut evidence = Evidence {
        usage: None,
        candidates: vec![c.clone()],
    };
    assert!(evidence.visible_to("fixture-owner"));
    assert!(!evidence.visible_to("other"));
    c.user_id = None;
    evidence.candidates.push(c);
    assert!(!evidence.visible_to("fixture-owner"));
}

fn usage_fixture() -> StoredRequestUsageAudit {
    StoredRequestUsageAudit::new(
        "usage-fixture".into(),
        "fixture-request".into(),
        Some("fixture-owner".into()),
        None,
        None,
        None,
        "CANARY-PROVIDER".into(),
        "CANARY-MODEL".into(),
        Some("CANARY-TARGET".into()),
        None,
        None,
        None,
        None,
        Some("openai:responses".into()),
        None,
        None,
        None,
        None,
        None,
        true,
        true,
        0,
        0,
        0,
        0.0,
        0.0,
        Some(200),
        None,
        Some("stream_missing_terminal_event".into()),
        Some(100),
        Some(20),
        "failed".into(),
        "void".into(),
        1000,
        1001,
        Some(1001),
    )
    .unwrap()
}

#[test]
fn usage_snapshot_is_allowlisted_and_http_200_does_not_hide_stream_failure() {
    let mut usage = usage_fixture();
    usage.request_headers = Some(json!({"Authorization":"CANARY-HEADER"}));
    usage.request_body = Some(json!({"messages":"CANARY-PROMPT"}));
    usage.response_body = Some(json!({"text":"CANARY-RESPONSE"}));
    usage.request_metadata = Some(
        json!({"target_url":"http://10.20.30.40/CANARY-TARGET", "routing_failure_diagnostic":{"kind":"request_conversion", "message":"CANARY-MESSAGE"}}),
    );
    let result = build(
        "fixture-request",
        &Evidence {
            usage: Some(usage.clone()),
            candidates: vec![],
        },
    );
    assert_eq!(result.state, "failed");
    assert_eq!(stage(&result, "stream").0, "failed");
    assert_eq!(stage(&result, "conversion").0, "failed");
    assert!(!serde_json::to_string(&result).unwrap().contains("CANARY"));
    usage.status = "pending".into();
    usage.finalized_at_unix_secs = None;
    usage.error_category = None;
    usage.request_metadata = None;
    let result = build(
        "fixture-request",
        &Evidence {
            usage: Some(usage),
            candidates: vec![],
        },
    );
    assert_eq!(result.state, "in_progress");
    assert_eq!(stage(&result, "stream").0, "unknown");
}

#[test]
fn finalized_successful_fallback_preserves_attempt_failures_without_failing_request() {
    let failed = candidate();
    let mut skipped = candidate();
    skipped.retry_index = 1;
    skipped.status = RequestCandidateStatus::Skipped;
    skipped.error_type = None;
    skipped.skip_reason = Some("provider_request_body_build_failed".into());
    let mut stale = candidate();
    stale.candidate_index = 1;
    stale.status = RequestCandidateStatus::Pending;
    stale.error_type = None;
    stale.finished_at_unix_ms = None;
    let mut success = candidate();
    success.candidate_index = 2;
    success.status = RequestCandidateStatus::Success;
    success.error_type = None;
    success.status_code = Some(200);
    let mut usage = usage_fixture();
    usage.status = "completed".into();
    usage.error_category = None;
    let mut evidence = Evidence {
        usage: Some(usage.clone()),
        candidates: vec![failed, skipped, stale, success],
    };
    let result = build("fixture-request", &evidence);
    assert_eq!(result.state, "known");
    assert_eq!(result.final_outcome, "completed");
    assert!(result.has_attempt_failures);
    assert_eq!(stage(&result, "conversion").0, "failed");
    assert_eq!(
        stage(&result, "upstream"),
        ("failed", "invalid_request_error")
    );
    usage.status = "failed".into();
    evidence.usage = Some(usage.clone());
    let result = build("fixture-request", &evidence);
    assert_eq!(result.state, "failed");
    assert_eq!(result.final_outcome, "failed");
    usage.status = "streaming".into();
    usage.finalized_at_unix_secs = None;
    evidence.usage = Some(usage.clone());
    let result = build("fixture-request", &evidence);
    assert_eq!(result.state, "in_progress");
    assert_eq!(result.final_outcome, "in_progress");
    usage.status = "completed".into();
    evidence.usage = Some(usage);
    let result = build("fixture-request", &evidence);
    assert_eq!(result.state, "unknown");
    assert_eq!(result.final_outcome, "unknown");
    evidence.usage = None;
    let result = build("fixture-request", &evidence);
    assert_ne!(result.state, "in_progress");
    assert_eq!(result.final_outcome, "unknown");
}

async fn user(state: &AppState, role: &str, name: &str) -> (String, HeaderMap) {
    let u = state
        .create_local_auth_user_with_settings(
            Some(format!("{name}@example.test")),
            true,
            name.into(),
            "hash".into(),
            role.into(),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap()
        .unwrap();
    (
        u.id.clone(),
        crate::api::mcp::test_session_headers(state, &u.id).await,
    )
}

#[tokio::test]
async fn diagnostic_http_ownership_feedback_opt_in_and_admin_permissions() {
    let state = AppState::new().unwrap();
    let (owner_id, owner) = user(&state, "user", "diagnostic-owner").await;
    let (_, other) = user(&state, "user", "diagnostic-other").await;
    let (_, admin) = user(&state, "audit_admin", "diagnostic-admin").await;
    let mut c = candidate();
    c.user_id = Some(owner_id);
    let state = state.with_data_state_for_tests(
        crate::data::GatewayDataState::with_request_candidate_and_usage_repository_for_tests(
            Arc::new(InMemoryRequestCandidateRepository::seed(vec![c])),
            Arc::new(InMemoryUsageReadRepository::default()),
        )
        .with_feedback_repository_for_tests(Arc::new(InMemoryFeedbackRepository::default())),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let router = crate::router::build_router_with_state(state);
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let path = format!("{url}/api/requests/fixture-request/diagnostics");
    assert_eq!(
        client.get(&path).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(format!("{path}?owner_id=other"))
            .headers(owner.clone())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let own = client
        .get(&path)
        .headers(owner.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(own.headers()[header::CACHE_CONTROL], "no-store");
    let own: Value = own.json().await.unwrap();
    assert_eq!(own["state"], "failed");
    assert!(!own.to_string().contains("CANARY"));
    let hidden: Value = client
        .get(&path)
        .headers(other.clone())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(hidden["state"], "unknown");
    assert!(hidden["stages"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["status"] == "unknown"));
    let missing: Value = client
        .get(format!("{url}/api/requests/missing/diagnostics"))
        .headers(owner.clone())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(hidden["stages"], missing["stages"]);
    assert_eq!(
        client
            .get(format!(
                "{url}/api/admin/monitoring/diagnostics/fixture-request"
            ))
            .headers(other.clone())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let admin_result = client
        .get(format!(
            "{url}/api/admin/monitoring/diagnostics/fixture-request"
        ))
        .headers(admin.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(admin_result.status(), StatusCode::OK);
    let admin_result: Value = admin_result.json().await.unwrap();
    assert!(!admin_result.to_string().contains("CANARY"));
    let mut input = json!({"title":"Fixture failure","description":"Synthetic report","category":"model","request_id":"fixture-request","client_request_id":uuid::Uuid::new_v4().to_string()});
    assert_eq!(
        client
            .post(format!("{url}/api/feedback"))
            .headers(other)
            .json(&input)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let plain: Value = client
        .post(format!("{url}/api/feedback"))
        .headers(owner.clone())
        .json(&input)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(plain["issue"]["description"], "Synthetic report");
    input["include_diagnostics"] = json!(true);
    input["client_request_id"] = json!(uuid::Uuid::new_v4().to_string());
    let attached = client
        .post(format!("{url}/api/feedback"))
        .headers(owner)
        .json(&input)
        .send()
        .await
        .unwrap();
    assert_eq!(attached.status(), StatusCode::CREATED);
    let attached: Value = attached.json().await.unwrap();
    assert!(attached["issue"]["description"]
        .as_str()
        .unwrap()
        .contains("RequestID: fixture-request"));
    assert!(!attached.to_string().contains("CANARY"));
    let reply = json!({"action":"reply","content":"Reviewed fixture","diagnostic_request_id":"fixture-request","expected_version":1,"client_request_id":uuid::Uuid::new_v4().to_string()});
    assert_eq!(
        client
            .post(format!(
                "{url}/api/admin/feedback/{}/commands",
                attached["issue"]["id"].as_str().unwrap()
            ))
            .headers(admin)
            .json(&reply)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    task.abort();
}
