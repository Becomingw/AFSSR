use super::*;
use aether_data::repository::feedback::InMemoryFeedbackRepository;
use axum::http::HeaderMap;
use serde_json::Value;
use std::sync::Arc;

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
    let headers = crate::api::mcp::test_session_headers(state, &u.id).await;
    (u.id, headers)
}
async fn server(state: AppState) -> (String, tokio::task::JoinHandle<()>) {
    let router = crate::router::build_router_with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap()
    });
    (format!("http://{addr}"), task)
}
fn input() -> Value {
    json!({"title":"Tool call failed","description":"Synthetic issue only <script>alert(1)</script>","category":"model","model":"fixture","client_request_id":uuid::Uuid::new_v4().to_string()})
}

#[tokio::test]
async fn feedback_http_owner_isolation_admin_replies_and_full_lifecycle() {
    let state = AppState::new().unwrap().with_data_state_for_tests(
        crate::data::GatewayDataState::disabled()
            .with_feedback_repository_for_tests(Arc::new(InMemoryFeedbackRepository::default())),
    );
    let (_, owner) = user(&state, "user", "owner").await;
    let (_, other) = user(&state, "user", "other").await;
    let mut admins = vec![];
    for role in ["admin", "audit_admin", "user_admin", "service_admin"] {
        let (_, headers) = user(&state, role, role).await;
        admins.push((role, headers));
    }
    let (url, task) = server(state).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    assert_eq!(
        client
            .get(format!("{url}/api/feedback"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let submission = input();
    let created = client
        .post(format!("{url}/api/feedback"))
        .headers(owner.clone())
        .json(&submission)
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created: Value = created.json().await.unwrap();
    let id = created["issue"]["id"].as_str().unwrap();
    let replay: Value = client
        .post(format!("{url}/api/feedback"))
        .headers(owner.clone())
        .json(&submission)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(replay["replayed"], true);
    let list: Value = client
        .get(format!("{url}/api/feedback"))
        .headers(other.clone())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list["total"], 0);
    assert_eq!(
        client
            .get(format!("{url}/api/feedback/{id}"))
            .headers(other.clone())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let command = json!({"action":"reply","content":"Synthetic reply","expected_version":1,"client_request_id":uuid::Uuid::new_v4().to_string()});
    assert_eq!(
        client
            .post(format!("{url}/api/feedback/{id}/commands"))
            .headers(other.clone())
            .json(&command)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .get(format!("{url}/api/admin/feedback"))
            .headers(other.clone())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let mut version = 1;
    for (role, headers) in &admins {
        let list = client
            .get(format!("{url}/api/admin/feedback"))
            .headers(headers.clone())
            .send()
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK, "{role}");
        let list: Value = list.json().await.unwrap();
        assert_eq!(list["total"], 1);
        let mut forged = input();
        forged["owner_id"] = json!("another-user");
        assert_eq!(
            client
                .post(format!("{url}/api/feedback"))
                .headers(headers.clone())
                .json(&forged)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        let reply = json!({"action":"reply","content":format!("Reply from {role}"),"expected_version":version,"client_request_id":uuid::Uuid::new_v4().to_string()});
        let result = client
            .post(format!("{url}/api/admin/feedback/{id}/commands"))
            .headers(headers.clone())
            .json(&reply)
            .send()
            .await
            .unwrap();
        let status = result.status();
        let data: Value = result.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{role} {data}");
        version += 1;
        assert_eq!(data["issue"]["version"], version);
        for status in ["resolved", "closed", "open"] {
            let update = json!({"action":"set_status","status":status,"comment":"Verified final synthetic solution","expected_version":version,"client_request_id":uuid::Uuid::new_v4().to_string()});
            let response = client
                .post(format!("{url}/api/admin/feedback/{id}/commands"))
                .headers(headers.clone())
                .json(&update)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{role} {status}");
            let data: Value = response.json().await.unwrap();
            version += 1;
            assert_eq!(data["issue"]["version"], version);
            assert_eq!(data["issue"]["status"], status);
        }
    }
    let stale = client
        .post(format!("{url}/api/feedback/{id}/commands"))
        .headers(owner.clone())
        .json(&command)
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    for (index, status) in ["resolved", "open", "closed"].into_iter().enumerate() {
        let (headers, base) = if index == 2 {
            (admins[0].1.clone(), "api/admin/feedback")
        } else {
            (owner.clone(), "api/feedback")
        };
        let update = json!({"action":"set_status","status":status,"comment":"Verified synthetic transition","expected_version":version,"client_request_id":uuid::Uuid::new_v4().to_string()});
        let response = client
            .post(format!("{url}/{base}/{id}/commands"))
            .headers(headers)
            .json(&update)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let data: Value = response.json().await.unwrap();
        assert_eq!(data["issue"]["status"], status);
        version += 1;
    }
    let detail: Value = client
        .get(format!("{url}/api/feedback/{id}"))
        .headers(owner.clone())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail["events"].as_array().unwrap().len(), 20);
    assert!(detail["issue"]["description"]
        .as_str()
        .unwrap()
        .contains("<script>"));
    let filter: Value = client
        .get(format!("{url}/api/feedback?after=1&limit=1"))
        .headers(owner.clone())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(filter["total"], 1);
    assert_eq!(filter["items"][0]["status"], "closed");
    assert_eq!(
        client
            .get(format!("{url}/api/feedback?owner_id=other"))
            .headers(owner)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    task.abort();
}

#[tokio::test]
async fn feedback_http_user_and_actual_admin_create_as_themselves_on_authorized_paths() {
    let repo = Arc::new(InMemoryFeedbackRepository::default());
    let state = AppState::new().unwrap().with_data_state_for_tests(
        crate::data::GatewayDataState::disabled().with_feedback_repository_for_tests(repo.clone()),
    );
    let mut identities = vec![];
    for role in [
        "user",
        "admin",
        "audit_admin",
        "user_admin",
        "service_admin",
    ] {
        let (id, headers) = user(&state, role, &format!("create-{role}")).await;
        identities.push((role, id, headers));
    }
    let fixture: Value =
        serde_json::from_str(include_str!("../diagnostics/fixtures/evidence.json")).unwrap();
    let mut candidate: aether_data_contracts::repository::candidates::StoredRequestCandidate =
        serde_json::from_value(fixture["candidate"].clone()).unwrap();
    candidate.user_id = Some(identities[0].1.clone());
    let state = state.with_data_state_for_tests(
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
    let unknown = dispatch(
        &state,
        "unknown-id",
        "unknown",
        false,
        &Method::POST,
        &Uri::from_static("/api/feedback"),
        &Bytes::from(input().to_string()),
    )
    .await;
    assert_eq!(unknown.status(), StatusCode::FORBIDDEN);
    let (url, task) = server(state).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    for path in ["/api/feedback", "/api/admin/feedback"] {
        assert_eq!(
            client
                .post(format!("{url}{path}"))
                .json(&input())
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    for (role, owner_id, headers) in &identities {
        let submission = input();
        let response = client
            .post(format!("{url}/api/feedback"))
            .headers(headers.clone())
            .json(&submission)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED, "{role}");
        let data: Value = response.json().await.unwrap();
        assert_eq!(data["issue"]["owner_id"], *owner_id);
        assert_eq!(data["issue"]["visibility"], "private");
        let detail = repo
            .detail(data["issue"]["id"].as_str().unwrap(), None, 0, 100)
            .await
            .unwrap();
        assert_eq!(detail.events[0].actor_role, *role);
        assert_eq!(
            detail.events[0].actor_id.as_deref(),
            Some(owner_id.as_str())
        );
        if *role != "user" {
            let replay = client
                .post(format!("{url}/api/admin/feedback"))
                .headers(headers.clone())
                .json(&submission)
                .send()
                .await
                .unwrap();
            assert_eq!(replay.status(), StatusCode::OK);
            let replay: Value = replay.json().await.unwrap();
            assert_eq!(replay["replayed"], true);
            assert_eq!(replay["issue"]["id"], data["issue"]["id"]);
            let response = client
                .post(format!("{url}/api/admin/feedback"))
                .headers(headers.clone())
                .json(&input())
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::CREATED, "{role}");
            let data: Value = response.json().await.unwrap();
            assert_eq!(data["issue"]["owner_id"], *owner_id);
            let detail = repo
                .detail(data["issue"]["id"].as_str().unwrap(), None, 0, 100)
                .await
                .unwrap();
            assert_eq!(detail.events[0].actor_role, *role);
        } else {
            assert_eq!(
                client
                    .post(format!("{url}/api/admin/feedback"))
                    .headers(headers.clone())
                    .json(&submission)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
        }
        for field in ["owner_id", "actor_id", "actor_role", "role"] {
            let mut forged = input();
            forged[field] = json!("another-user");
            for path in if *role == "user" {
                vec!["/api/feedback"]
            } else {
                vec!["/api/feedback", "/api/admin/feedback"]
            } {
                assert_eq!(
                    client
                        .post(format!("{url}{path}"))
                        .headers(headers.clone())
                        .json(&forged)
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    StatusCode::BAD_REQUEST,
                    "{role} {field}"
                );
            }
        }
    }
    assert_eq!(
        repo.list(&FeedbackQuery {
            limit: 100,
            ..Default::default()
        })
        .await
        .unwrap()
        .total,
        9
    );
    for (role, owner_id, headers) in &identities {
        let mut submission = input();
        submission["request_id"] = json!("fixture-request");
        submission["include_diagnostics"] = json!(true);
        let response = client
            .post(format!("{url}/api/feedback"))
            .headers(headers.clone())
            .json(&submission)
            .send()
            .await
            .unwrap();
        if *role == "user" {
            assert_eq!(response.status(), StatusCode::CREATED);
        } else {
            assert_eq!(
                response.status(),
                StatusCode::NOT_FOUND,
                "admin session does not bypass request ownership"
            );
            let response = client
                .post(format!("{url}/api/admin/feedback"))
                .headers(headers.clone())
                .json(&submission)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
            let data: Value = response.json().await.unwrap();
            assert_eq!(data["issue"]["owner_id"], *owner_id);
            assert_eq!(data["issue"]["visibility"], "private");
            assert!(data["issue"]["description"]
                .as_str()
                .unwrap()
                .contains("fixture-request"));
        }
    }
    assert_eq!(
        repo.list(&FeedbackQuery {
            limit: 100,
            ..Default::default()
        })
        .await
        .unwrap()
        .total,
        14
    );
    task.abort();
}

#[test]
fn feedback_scope_is_narrow_and_new_scope_is_not_inherited_by_legacy_keys() {
    for role in ["admin", "audit_admin", "user_admin", "service_admin"] {
        assert!(crate::roles::role_has_management_permission(
            role,
            "admin:feedback:read"
        ));
        assert!(crate::roles::role_has_management_permission(
            role,
            "admin:feedback:write"
        ));
    }
    assert!(!crate::roles::role_has_management_permission(
        "user",
        "admin:feedback:read"
    ));
    assert!(!crate::roles::role_has_management_permission(
        "audit_admin",
        "admin:providers:write"
    ));
    assert!(!crate::control::legacy_full_management_token_permissions()
        .contains(&"admin:feedback:write".into()));
}

#[tokio::test]
async fn feedback_http_public_participation_private_event_projection_and_owner_final_solution() {
    let state = AppState::new().unwrap().with_data_state_for_tests(
        crate::data::GatewayDataState::disabled()
            .with_feedback_repository_for_tests(Arc::new(InMemoryFeedbackRepository::default())),
    );
    let (_, owner) = user(&state, "user", "public-owner").await;
    let (_, participant) = user(&state, "user", "participant").await;
    let (_, spectator) = user(&state, "user", "spectator").await;
    let (_, admin) = user(&state, "service_admin", "actual-admin").await;
    let (url, task) = server(state).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let private: Value = client
        .post(format!("{url}/api/feedback"))
        .headers(owner.clone())
        .json(&input())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(private["issue"]["visibility"], "private");
    let private_id = private["issue"]["id"].as_str().unwrap();
    let mut submission = input();
    submission["visibility"] = json!("public");
    let public: Value = client
        .post(format!("{url}/api/feedback"))
        .headers(owner.clone())
        .json(&submission)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(public["issue"]["visibility"], "public");
    let id = public["issue"]["id"].as_str().unwrap();
    assert_eq!(
        client
            .get(format!("{url}/api/feedback/{id}"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let list: Value = client
        .get(format!("{url}/api/feedback"))
        .headers(participant.clone())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list["total"], 1);
    assert_eq!(list["items"][0]["id"], id);
    assert_eq!(
        client
            .get(format!("{url}/api/feedback/{private_id}"))
            .headers(participant.clone())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .get(format!("{url}/api/feedback?owner_id=any"))
            .headers(participant.clone())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let command = |version: i64, action: &str, content: &str, status: &str, visibility: &str| {
        let mut value = json!({"expected_version":version,"client_request_id":uuid::Uuid::new_v4().to_string(),"action":action,"visibility":visibility});
        if action == "reply" {
            value["content"] = json!(content);
        } else {
            value["status"] = json!(status);
            value["comment"] = json!(content);
        }
        value
    };
    let reply = command(1, "reply", "private participant body", "", "private");
    assert_eq!(
        client
            .post(format!("{url}/api/feedback/{id}/commands"))
            .headers(participant.clone())
            .json(&reply)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let hidden: Value = client
        .get(format!(
            "{url}/api/feedback/{id}?after_event={}&limit=1",
            public["issue"]["activity_seq"]
        ))
        .headers(spectator.clone())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(hidden["events"].as_array().unwrap().is_empty());
    assert!(
        hidden["next_event_cursor"].as_i64().unwrap()
            > public["issue"]["activity_seq"].as_i64().unwrap()
    );
    for headers in [&owner, &participant, &admin] {
        let detail: Value = client
            .get(format!("{url}/api/feedback/{id}"))
            .headers(headers.clone())
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(detail.to_string().contains("private participant body"));
    }
    let visible = command(2, "reply", "public participant body", "", "public");
    assert_eq!(
        client
            .post(format!("{url}/api/feedback/{id}/commands"))
            .headers(participant.clone())
            .json(&visible)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    for status in ["resolved", "closed", "open"] {
        let forbidden = command(3, "set_status", "Not authorized", status, "public");
        assert_eq!(
            client
                .post(format!("{url}/api/feedback/{id}/commands"))
                .headers(participant.clone())
                .json(&forbidden)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    for comment in ["", "  "] {
        let invalid = command(3, "set_status", comment, "resolved", "private");
        assert_eq!(
            client
                .post(format!("{url}/api/feedback/{id}/commands"))
                .headers(owner.clone())
                .json(&invalid)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let solve = command(
        3,
        "set_status",
        "final private solution",
        "resolved",
        "private",
    );
    assert_eq!(
        client
            .post(format!("{url}/api/feedback/{id}/commands"))
            .headers(owner.clone())
            .json(&solve)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let detail: Value = client
        .get(format!("{url}/api/feedback/{id}"))
        .headers(spectator.clone())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail["issue"]["status"], "resolved");
    assert!(!detail.to_string().contains("private participant body"));
    assert!(!detail.to_string().contains("final private solution"));
    assert!(detail.to_string().contains("public participant body"));
    let status = detail["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "status")
        .unwrap();
    assert_eq!(status["visibility"], "private");
    assert!(status["content"].is_null());
    let close = command(4, "set_status", "Closed", "closed", "private");
    assert_eq!(
        client
            .post(format!("{url}/api/feedback/{id}/commands"))
            .headers(owner.clone())
            .json(&close)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(format!("{url}/api/admin/feedback/{id}/commands"))
            .headers(admin.clone())
            .json(&close)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let reopen = command(5, "set_status", "Reopening", "open", "public");
    assert_eq!(
        client
            .post(format!("{url}/api/feedback/{id}/commands"))
            .headers(owner.clone())
            .json(&reopen)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    // Explicit public events never bypass the enclosing private issue's access check.
    let public_private_reply = command(1, "reply", "not globally readable", "", "public");
    assert_eq!(
        client
            .post(format!("{url}/api/admin/feedback/{private_id}/commands"))
            .headers(admin)
            .json(&public_private_reply)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        client
            .get(format!("{url}/api/feedback/{private_id}"))
            .headers(spectator)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    task.abort();
}

#[tokio::test]
async fn feedback_http_numbers_pinning_and_mentionable_administrators() {
    let state = AppState::new().unwrap().with_data_state_for_tests(
        crate::data::GatewayDataState::disabled()
            .with_feedback_repository_for_tests(Arc::new(InMemoryFeedbackRepository::default())),
    );
    let (_, owner) = user(&state, "user", "owner").await;
    let (_, stranger) = user(&state, "user", "stranger").await;
    let (_, admin) = user(&state, "admin", "root").await;
    let (_, auditor) = user(&state, "audit_admin", "auditor").await;
    let (url, task) = server(state).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();

    // The first issue is #1 and can be opened by number; private issues stay hidden from others.
    let created: Value = client
        .post(format!("{url}/api/feedback"))
        .headers(owner.clone())
        .json(&input())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(created["issue"]["number"], 1);
    let id = created["issue"]["id"].as_str().unwrap().to_string();
    let by_number = client
        .get(format!("{url}/api/feedback/1"))
        .headers(owner.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(by_number.status(), StatusCode::OK);
    let by_number: Value = by_number.json().await.unwrap();
    assert_eq!(by_number["issue"]["id"], id);
    for number in ["1", "999"] {
        assert_eq!(
            client
                .get(format!("{url}/api/feedback/{number}"))
                .headers(stranger.clone())
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND,
            "number {number} must not reveal a private issue or whether it exists"
        );
    }

    // Only administrators can pin, and the pin is visible in the list and history.
    let pin = |expected_version: i64, pinned: bool| json!({"action":"set_pinned","pinned":pinned,"expected_version":expected_version,"client_request_id":uuid::Uuid::new_v4().to_string()});
    let denied = client
        .post(format!("{url}/api/feedback/1/commands"))
        .headers(owner.clone())
        .json(&pin(1, true))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let pinned = client
        .post(format!("{url}/api/feedback/1/commands"))
        .headers(admin.clone())
        .json(&pin(1, true))
        .send()
        .await
        .unwrap();
    assert_eq!(pinned.status(), StatusCode::OK);
    let pinned: Value = pinned.json().await.unwrap();
    assert!(pinned["issue"]["pinned_at"].is_i64());
    let detail: Value = client
        .get(format!("{url}/api/feedback/{id}"))
        .headers(owner.clone())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(detail["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["kind"] == "pin" && event["content"] == "pinned"));

    // Replies may mention administrators; without SMTP the notification is a silent no-op.
    let reply = client
        .post(format!("{url}/api/feedback/1/commands"))
        .headers(owner.clone())
        .json(&json!({"action":"reply","content":"cc @root and @ghost #1","expected_version":2,"client_request_id":uuid::Uuid::new_v4().to_string()}))
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), StatusCode::OK);

    // The mentionable endpoint answers for any signed-in user (the list itself comes from the
    // user directory, which is empty in this fixture).
    let names = client
        .get(format!("{url}/api/feedback/mentionable"))
        .headers(stranger)
        .send()
        .await
        .unwrap();
    assert_eq!(names.status(), StatusCode::OK);
    let names: Value = names.json().await.unwrap();
    assert!(names["items"].is_array());
    let _ = auditor;
    task.abort();
}

#[test]
fn mentionable_names_are_active_administrators_sorted_and_deduplicated() {
    let users = [
        ("root", "admin", true),
        ("alice", "user", true),
        ("Zed", "service_admin", true),
        ("gone", "admin", false),
        ("auditor", "audit_admin", true),
        ("ROOT", "user_admin", true),
        ("guest", "super", true),
    ];
    assert_eq!(mentionable_names(users), vec!["auditor", "root", "Zed"]);
}

#[tokio::test]
async fn feedback_http_owner_and_admin_edit_and_delete_issues_and_replies() {
    let state = AppState::new().unwrap().with_data_state_for_tests(
        crate::data::GatewayDataState::disabled()
            .with_feedback_repository_for_tests(Arc::new(InMemoryFeedbackRepository::default())),
    );
    let (_, owner) = user(&state, "user", "owner").await;
    let (_, helper) = user(&state, "user", "helper").await;
    let (_, admin) = user(&state, "admin", "root").await;
    let (url, task) = server(state).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let post = |path: String, headers: &HeaderMap, body: Value| {
        let client = client.clone();
        let headers = headers.clone();
        async move {
            let response = client
                .post(path)
                .headers(headers)
                .json(&body)
                .send()
                .await
                .unwrap();
            (response.status(), response.json::<Value>().await.unwrap())
        }
    };
    let command = |version: i64, extra: Value| {
        let mut body = json!({"expected_version":version,"client_request_id":uuid::Uuid::new_v4().to_string()});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        body
    };

    let mut public = input();
    public["visibility"] = json!("public");
    let (_, created) = post(format!("{url}/api/feedback"), &owner, public).await;
    let id = created["issue"]["id"].as_str().unwrap().to_string();
    let commands = format!("{url}/api/feedback/{id}/commands");

    // Editing the issue: owner yes, another participant no.
    let edit = json!({"action":"edit_issue","title":"Renamed","description":"Better words"});
    let (status, _) = post(commands.clone(), &helper, command(1, edit.clone())).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, edited) = post(commands.clone(), &owner, command(1, edit)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(edited["issue"]["title"], "Renamed");
    assert!(edited["issue"]["edited_at"].is_i64());

    // Helper replies publicly, then edits and (as an admin would) loses the reply.
    let (_, replied) = post(
        commands.clone(),
        &helper,
        command(
            2,
            json!({"action":"reply","content":"first try","visibility":"public"}),
        ),
    )
    .await;
    let seq = client
        .get(format!("{url}/api/feedback/{id}"))
        .headers(owner.clone())
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["content"] == "first try")
        .unwrap()["seq"]
        .as_i64()
        .unwrap();
    let version = replied["issue"]["version"].as_i64().unwrap();
    let (status, _) = post(
        commands.clone(),
        &owner,
        command(
            version,
            json!({"action":"edit_reply","seq":seq,"content":"hijack"}),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the issue owner cannot rewrite another's reply"
    );
    let (status, fixed) = post(
        commands.clone(),
        &helper,
        command(
            version,
            json!({"action":"edit_reply","seq":seq,"content":"second try"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let version = fixed["issue"]["version"].as_i64().unwrap();
    let (status, _) = post(
        commands.clone(),
        &owner,
        command(version, json!({"action":"delete_reply","seq":seq})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, removed) = post(
        format!("{url}/api/admin/feedback/{id}/commands"),
        &admin,
        command(version, json!({"action":"delete_reply","seq":seq})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "administrators can remove any reply: {removed}"
    );
    let detail: Value = client
        .get(format!("{url}/api/feedback/{id}"))
        .headers(owner.clone())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let gone = detail["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["seq"] == seq)
        .unwrap();
    assert!(gone["content"].is_null() && gone["deleted_at"].is_i64());
    assert!(
        !detail.to_string().contains("second try"),
        "deleted text must not remain"
    );

    // Deleting the issue: not by helper, by the owner; afterwards it is gone by id and number.
    let version = removed["issue"]["version"].as_i64().unwrap();
    let delete = json!({"action":"delete_issue"});
    let (status, _) = post(commands.clone(), &helper, command(version, delete.clone())).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, deleted) = post(commands.clone(), &owner, command(version, delete)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(deleted["issue"]["deleted_at"].is_i64());
    for path in [id.clone(), "1".to_string()] {
        for headers in [&owner, &admin] {
            let response = client
                .get(format!("{url}/api/feedback/{path}"))
                .headers(headers.clone())
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        }
    }
    task.abort();
}
