use super::*;
use aether_data::repository::usage::InMemoryUsageReadRepository;
use aether_data_contracts::repository::usage::StoredRequestUsageAudit;
use std::sync::Arc;

fn sample(id: &str) -> StoredRequestUsageAudit {
    let now = chrono::Utc::now().timestamp();
    StoredRequestUsageAudit::new(
        id.into(),
        id.into(),
        Some(id.into()),
        None,
        Some(format!("private-{id}")),
        None,
        "Fixture".into(),
        "fixture-model".into(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
        false,
        10,
        20,
        30,
        1.234,
        1.0,
        Some(200),
        None,
        None,
        Some(100),
        None,
        "completed".into(),
        "settled".into(),
        now,
        now,
        Some(now),
    )
    .unwrap()
}

#[tokio::test]
async fn leaderboard_last_night_reads_until_six_without_extending_accounting_range() {
    let range =
        AdminStatsTimeRange::resolve_optional(Some("preset=last_week&tz_offset_minutes=480"))
            .unwrap()
            .unwrap();
    let (_, until) = range.to_unix_bounds().unwrap();
    let items: Vec<_> = [
        ("night-start", until - 2 * 3600, "a"),
        ("night-tail", until + 6 * 3600 - 1, "a"),
        ("next-morning", until + 6 * 3600, "b"),
    ]
    .into_iter()
    .map(|(id, t, user)| {
        let mut item = sample(id);
        item.created_at_unix_ms = t;
        item.user_id = Some(user.into());
        item
    })
    .collect();
    let state = AppState::new().unwrap().with_data_state_for_tests(
        crate::data::GatewayDataState::with_usage_repository_for_tests(Arc::new(
            InMemoryUsageReadRepository::seed(items),
        )),
    );
    let response = report(
        &state,
        Some("preset=last_week&tz_offset_minutes=480"),
        false,
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["requests"], 1);
    assert_eq!(value["hours"][5]["requests"], 0);
    assert_eq!(value["hours"][22]["requests"], 1);
    assert_eq!(value["early_birds"].as_array().unwrap().len(), 1);
    assert_eq!(value["early_birds"][0]["time"], "22:00:00");
    assert_eq!(value["night_owls"].as_array().unwrap().len(), 1);
    assert_eq!(value["night_owls"][0]["date"], range.end_date.to_string());
    assert_eq!(value["night_owls"][0]["winners"][0]["value"], 10.0);
    assert_eq!(value["night_owls"][0]["winners"][0]["id"], "a");
}

#[tokio::test]
async fn leaderboard_http_masks_before_serialization_and_obeys_admin_roles() {
    let state = AppState::new().unwrap().with_data_state_for_tests(
        crate::data::GatewayDataState::with_usage_repository_for_tests(Arc::new(
            InMemoryUsageReadRepository::seed([
                sample("secret-a"),
                sample("secret-b"),
                sample("secret-c"),
            ]),
        )),
    );
    let mut actors = vec![];
    for role in [
        "user",
        "admin",
        "audit_admin",
        "user_admin",
        "service_admin",
    ] {
        let u = state
            .create_local_auth_user_with_settings(
                Some(format!("{role}@example.test")),
                true,
                role.into(),
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
        actors.push((
            role,
            crate::api::mcp::test_session_headers(&state, &u.id).await,
        ));
    }
    let router = crate::router::build_router_with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    assert_eq!(
        client
            .get(format!("{url}/api/leaderboard"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    for (role, headers) in &actors {
        let public = client
            .get(format!("{url}/api/leaderboard?preset=this_week"))
            .headers(headers.clone())
            .send()
            .await
            .unwrap();
        assert_eq!(public.status(), StatusCode::OK, "{role}");
        assert_eq!(public.headers()["cache-control"], "no-store");
        let text = public.text().await.unwrap();
        assert!(!text.contains("secret-"));
        assert!(!text.contains("private-"));
        assert!(!text.contains("1.234"));
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["masked"], true);
        assert_eq!(value["requests"], 3);
        assert_eq!(value["users"], 3);
        assert!(value["cost"].is_null());
        let admin = client
            .get(format!(
                "{url}/api/admin/stats/leaderboard/overview?preset=this_week"
            ))
            .headers(headers.clone())
            .send()
            .await
            .unwrap();
        if *role == "user" {
            assert_eq!(admin.status(), StatusCode::UNAUTHORIZED);
        } else {
            assert_eq!(admin.status(), StatusCode::OK, "{role}");
            let value: serde_json::Value = admin.json().await.unwrap();
            assert_eq!(value["masked"], false);
            assert!(value["cost"].as_f64().is_some_and(|v| v > 0.0));
            assert!(value["boards"]
                .as_array()
                .unwrap()
                .iter()
                .any(|b| b["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|i| i["id"] == "secret-a")));
        }
    }
    for query in [
        "preset=this_year",
        "preset=this_week&user_id=secret-a",
        "preset=this_week&tz_offset_minutes=9999",
        "preset=this_week&preset=last_week",
    ] {
        assert_eq!(
            client
                .get(format!("{url}/api/leaderboard?{query}"))
                .headers(actors[0].1.clone())
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    task.abort();
}
