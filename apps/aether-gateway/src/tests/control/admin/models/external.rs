use std::sync::{Arc, Mutex};
use std::time::Duration;

use aether_crypto::DEVELOPMENT_ENCRYPTION_KEY;
use aether_data::repository::proxy_nodes::{InMemoryProxyNodeRepository, StoredProxyNode};
use axum::body::Body;
use axum::extract::ws::Message;
use axum::routing::any;
use axum::{extract::Request, Router};
use base64::Engine as _;
use http::StatusCode;
use serde_json::json;
use tokio::sync::watch;

use super::super::super::{
    build_router_with_state, sample_proxy_node, start_server, with_tunnel_control_plane_key,
    AppState, TUNNEL_CONTROL_PLANE_TEST_GENERATION, TUNNEL_CONTROL_PLANE_TEST_PSK,
};
use crate::constants::{
    GATEWAY_HEADER, TRUSTED_ADMIN_SESSION_ID_HEADER, TRUSTED_ADMIN_USER_ID_HEADER,
    TRUSTED_ADMIN_USER_ROLE_HEADER,
};
use crate::data::GatewayDataState;
use crate::handlers::admin::{
    set_admin_external_models_source_url_for_tests, ADMIN_EXTERNAL_MODELS_CONFIG_MUTATION_LOCK_KEY,
};
use crate::tunnel::{tunnel_protocol, TunnelProxyConn};

#[tokio::test]
async fn shared_catalog_import_and_explicit_refresh_sync_metadata_without_touching_channel_overrides(
) {
    use super::super::super::{
        sample_admin_global_model, sample_admin_provider_model, sample_provider,
    };
    use aether_data::repository::global_models::InMemoryGlobalModelReadRepository;
    use aether_data::repository::provider_catalog::InMemoryProviderCatalogReadRepository;
    use aether_data_contracts::repository::global_models::{
        AdminProviderModelListQuery, GlobalModelReadRepository,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    let hits = Arc::new(AtomicUsize::new(0));
    let source_hits = hits.clone();
    let upstream = Router::new().route("/api.json", any(move || {
        let hits = source_hits.clone();
        async move {
            let version = hits.fetch_add(1, Ordering::SeqCst) + 1;
            if version > 2 { return (StatusCode::BAD_GATEWAY, axum::Json(json!({"error":"fixture unavailable"}))); }
            (StatusCode::OK, axum::Json(json!({"openai":{"name":"OpenAI","npm":"@ai-sdk/openai","models":{
                "catalog-model":{"name":"Catalog Model","tool_call":true,"reasoning":true,"limit":{"context":version * 100000,"output":8192},"cost":{"input":version,"output":version * 3,"cache_read":0.1}},
                "existing":{"name":"Existing","tool_call":true,"cost":{"input":version,"output":version * 2}}
            }}})))
        }
    }));
    let (source_url, source_handle) = start_server(upstream).await;
    let _guard = set_admin_external_models_source_url_for_tests(&format!("{source_url}/api.json"));
    let mut existing = sample_admin_global_model("global-existing", "existing", "My custom label");
    existing.config = Some(json!({"custom_setting":true}));
    let channel =
        sample_admin_provider_model("channel-existing", "channel", "global-existing", "existing");
    let repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![existing])
            .with_admin_provider_models(vec![channel.clone()]),
    );
    let state = AppState::new().expect("state").with_data_state_for_tests(
        GatewayDataState::with_provider_catalog_reader_for_tests(Arc::new(
            InMemoryProviderCatalogReadRepository::seed(
                vec![sample_provider("channel", "openai", 10)],
                vec![],
                vec![],
            ),
        ))
        .with_global_model_repository_for_tests(repository.clone()),
    );
    let (url, handle) = start_server(build_router_with_state(state)).await;
    let client = reqwest::Client::new();
    let catalog_url = format!("{url}/api/admin/models/external");
    // The catalog is never fetched implicitly: reading before a refresh finds nothing.
    assert_eq!(
        trusted_admin(client.get(&catalog_url))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    assert_eq!(
        trusted_admin(client.post(format!("{url}/api/admin/models/external/refresh")))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    for _ in 0..2 {
        assert_eq!(
            trusted_admin(client.get(&catalog_url))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    let response = trusted_admin(client.post(format!(
        "{url}/api/admin/providers/channel/import-from-upstream"
    )))
    .json(&json!({"model_ids":["catalog-model","unknown-model","catalog-model"]}))
    .send()
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let imported: serde_json::Value = response.json().await.unwrap();
    assert_eq!(imported["success"].as_array().unwrap().len(), 2);
    let model_id = imported["success"][0]["global_model_id"].as_str().unwrap();
    let read = |id: String| {
        let repository = repository.clone();
        async move {
            repository
                .get_admin_global_model_by_id(&id)
                .await
                .unwrap()
                .unwrap()
        }
    };
    let model = read(model_id.to_string()).await;
    assert_eq!(model.display_name, "Catalog Model");
    assert_eq!(model.config.as_ref().unwrap()["context_limit"], 100000);
    assert_eq!(
        model.config.as_ref().unwrap()["catalog_api_formats"],
        json!(["openai:responses"])
    );
    assert_eq!(
        model.default_tiered_pricing.as_ref().unwrap()["tiers"][0]["input_price_per_1m"],
        1.0
    );
    let unknown_id = imported["success"][1]["global_model_id"].as_str().unwrap();
    assert_eq!(
        read(unknown_id.to_string()).await.config.unwrap()["catalog_metadata_status"],
        "unmatched"
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "import reuses the shared snapshot"
    );
    let refresh_url = format!("{url}/api/admin/models/external/refresh");
    let response = trusted_admin(client.post(&refresh_url))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let refreshed: serde_json::Value = response.json().await.unwrap();
    assert_eq!(refreshed["sync"]["updated"], 2);
    assert_eq!(refreshed["sync"]["unmatched"], 1);
    assert_eq!(refreshed["sync"]["failed"], json!([]));
    assert_eq!(
        read(model_id.to_string()).await.config.unwrap()["context_limit"],
        200000
    );
    let original = read("global-existing".into()).await;
    assert_eq!(original.display_name, "My custom label");
    assert_eq!(original.config.unwrap()["custom_setting"], true);
    let associations = repository
        .list_admin_provider_models(&AdminProviderModelListQuery {
            provider_id: "channel".into(),
            offset: 0,
            limit: 100,
            is_active: None,
        })
        .await
        .unwrap();
    assert_eq!(associations.len(), 3);
    let unchanged = associations
        .iter()
        .find(|item| item.id == channel.id)
        .unwrap();
    assert_eq!(unchanged.tiered_pricing, channel.tiered_pricing);
    assert_eq!(
        unchanged.provider_model_mappings,
        channel.provider_model_mappings
    );
    let response = trusted_admin(client.post(&refresh_url))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let snapshot: serde_json::Value = trusted_admin(client.get(&catalog_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        snapshot["openai"]["models"]["catalog-model"]["limit"]["context"],
        200000
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        3,
        "failed refresh preserves the old snapshot without another fetch"
    );
    handle.abort();
    source_handle.abort();
}

fn trusted_admin(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    request
        .header(GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
}

fn online_manual_proxy_node(node_id: &str, proxy_url: impl Into<String>) -> StoredProxyNode {
    let mut node = sample_proxy_node(node_id);
    node.name = node_id.to_string();
    node.status = "online".to_string();
    node.is_manual = true;
    node.tunnel_mode = false;
    node.tunnel_connected = false;
    node.proxy_url = Some(proxy_url.into());
    node.last_heartbeat_at_unix_secs = None;
    node.tunnel_connected_at_unix_secs = None;
    node.remote_config = None;
    node
}

#[tokio::test]
async fn gateway_manages_admin_external_models_proxy_config_locally() {
    let manual_node = online_manual_proxy_node("manual-node", "http://127.0.0.1:8899");
    let mut offline_node = online_manual_proxy_node("offline-node", "http://127.0.0.1:8900");
    offline_node.status = "offline".to_string();
    let repository = Arc::new(InMemoryProxyNodeRepository::seed(vec![
        manual_node,
        offline_node,
    ]));
    let data_state = GatewayDataState::with_proxy_node_repository_for_tests(repository)
        .with_system_config_values_for_tests(Vec::<(String, serde_json::Value)>::new());
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(data_state),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;
    let client = reqwest::Client::new();
    let config_url = format!("{gateway_url}/api/admin/models/external/config");

    let response = trusted_admin(client.get(&config_url))
        .send()
        .await
        .expect("initial config request should succeed");
    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(payload["proxy_node_id"], serde_json::Value::Null);

    for invalid_payload in [
        json!({}),
        json!({ "proxy_node_id": true }),
        json!({ "proxy_node_id": "" }),
        json!({ "proxy_node_id": "   " }),
    ] {
        let response = trusted_admin(client.put(&config_url))
            .json(&invalid_payload)
            .send()
            .await
            .expect("invalid config request should complete");
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "invalid payload should be rejected: {invalid_payload}"
        );
    }

    let response = trusted_admin(client.put(&config_url))
        .json(&json!({ "proxy_node_id": "missing-node" }))
        .send()
        .await
        .expect("missing-node config request should complete");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = trusted_admin(client.put(&config_url))
        .json(&json!({ "proxy_node_id": "offline-node" }))
        .send()
        .await
        .expect("offline-node config request should complete");
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let response = trusted_admin(client.get(&config_url))
        .send()
        .await
        .expect("config should remain readable");
    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(payload["proxy_node_id"], serde_json::Value::Null);

    let response = trusted_admin(client.put(&config_url))
        .json(&json!({ "proxy_node_id": "manual-node" }))
        .send()
        .await
        .expect("manual-node config request should succeed");
    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(payload["proxy_node_id"], "manual-node");
    assert!(payload["cache_cleared"].is_boolean());

    let response = trusted_admin(client.get(&config_url))
        .send()
        .await
        .expect("saved config request should succeed");
    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(payload["proxy_node_id"], "manual-node");

    let response = trusted_admin(client.put(&config_url))
        .json(&json!({ "proxy_node_id": null }))
        .send()
        .await
        .expect("direct config request should succeed");
    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(payload["proxy_node_id"], serde_json::Value::Null);
    assert!(payload["cache_cleared"].is_boolean());

    gateway_handle.abort();
}

#[tokio::test]
async fn gateway_serializes_external_models_proxy_selection_with_proxy_node_deletion() {
    let node_a = online_manual_proxy_node("node-a", "http://127.0.0.1:8898");
    let node_b = online_manual_proxy_node("node-b", "http://127.0.0.1:8899");
    let repository = Arc::new(InMemoryProxyNodeRepository::seed(vec![node_a, node_b]));
    let data_state = GatewayDataState::with_proxy_node_repository_for_tests(repository)
        .with_system_config_values_for_tests([(
            "external_models_proxy_node_id".to_string(),
            json!("node-a"),
        )]);
    let state = AppState::new()
        .expect("gateway should build")
        .with_data_state_for_tests(data_state.clone());
    let control_state = state.clone();
    let lock = control_state
        .runtime_state()
        .lock_try_acquire(
            ADMIN_EXTERNAL_MODELS_CONFIG_MUTATION_LOCK_KEY,
            "external-models-race-test",
            Duration::from_secs(60),
        )
        .await
        .expect("test mutation lock should be available")
        .expect("test mutation lock should be acquired");

    let gateway = build_router_with_state(state);
    let (gateway_url, gateway_handle) = start_server(gateway).await;
    let client = reqwest::Client::new();
    let config_url = format!("{gateway_url}/api/admin/models/external/config");

    let response = trusted_admin(client.put(&config_url))
        .json(&json!({ "proxy_node_id": "node-b" }))
        .send()
        .await
        .expect("contended config request should complete");
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let response =
        trusted_admin(client.delete(format!("{gateway_url}/api/admin/proxy-nodes/node-a")))
            .send()
            .await
            .expect("contended delete request should complete");
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        data_state
            .find_system_config_value("external_models_proxy_node_id")
            .await
            .expect("selector lookup should succeed"),
        Some(json!("node-a"))
    );
    assert!(data_state
        .find_proxy_node("node-a")
        .await
        .expect("node lookup should succeed")
        .is_some());

    assert!(control_state
        .runtime_state()
        .lock_release(&lock)
        .await
        .expect("test mutation lock should release"));

    let response = trusted_admin(client.put(&config_url))
        .json(&json!({ "proxy_node_id": "node-b" }))
        .send()
        .await
        .expect("config request should succeed after lock release");
    assert_eq!(response.status(), StatusCode::OK);

    let response =
        trusted_admin(client.delete(format!("{gateway_url}/api/admin/proxy-nodes/node-a")))
            .send()
            .await
            .expect("delete request should succeed after lock release");
    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(payload["cleared_external_models_proxy"], false);
    assert_eq!(
        data_state
            .find_system_config_value("external_models_proxy_node_id")
            .await
            .expect("selector lookup should succeed"),
        Some(json!("node-b")),
        "deleting node A must not overwrite a concurrently chosen node B"
    );

    let response = trusted_admin(client.put(&config_url))
        .json(&json!({ "proxy_node_id": "node-a" }))
        .send()
        .await
        .expect("deleted-node config request should complete");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        data_state
            .find_system_config_value("external_models_proxy_node_id")
            .await
            .expect("selector lookup should succeed"),
        Some(json!("node-b")),
        "a failed save must not leave a dangling deleted node ID"
    );

    gateway_handle.abort();
}

#[tokio::test]
async fn gateway_reports_unavailable_external_models_proxy_and_fails_closed() {
    let direct_source_hits = Arc::new(Mutex::new(0usize));
    let direct_source_hits_clone = Arc::clone(&direct_source_hits);
    let direct_source = Router::new().route(
        "/api.json",
        any(move |_request: Request| {
            let direct_source_hits_inner = Arc::clone(&direct_source_hits_clone);
            async move {
                *direct_source_hits_inner.lock().expect("mutex should lock") += 1;
                (
                    StatusCode::OK,
                    axum::Json(json!({
                        "unexpected-direct-provider": {
                            "name": "Unexpected direct fallback",
                            "models": {}
                        }
                    })),
                )
            }
        }),
    );
    let (direct_source_url, direct_source_handle) = start_server(direct_source).await;
    let _guard =
        set_admin_external_models_source_url_for_tests(&format!("{direct_source_url}/api.json"));

    let mut offline_node = online_manual_proxy_node("offline-node", "http://127.0.0.1:8900");
    offline_node.status = "offline".to_string();
    let repository = Arc::new(InMemoryProxyNodeRepository::seed(vec![offline_node]));
    let data_state = GatewayDataState::with_proxy_node_repository_for_tests(repository)
        .with_system_config_values_for_tests([(
            "external_models_proxy_node_id".to_string(),
            json!("offline-node"),
        )]);
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(data_state),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;
    let client = reqwest::Client::new();

    let response =
        trusted_admin(client.get(format!("{gateway_url}/api/admin/models/external/config")))
            .send()
            .await
            .expect("config request should succeed");
    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(payload["proxy_node_id"], "offline-node");

    let response = trusted_admin(client.get(format!("{gateway_url}/api/admin/models/external")))
        .send()
        .await
        .expect("catalog request should complete");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(*direct_source_hits.lock().expect("mutex should lock"), 0);

    gateway_handle.abort();
    direct_source_handle.abort();
}

#[tokio::test]
async fn gateway_fetches_external_models_through_connected_tunnel_node() {
    let source_url = "https://models.dev.test/api.json";
    let _guard = set_admin_external_models_source_url_for_tests(source_url);

    let mut tunnel_node = with_tunnel_control_plane_key(
        sample_proxy_node("tunnel-node"),
        TUNNEL_CONTROL_PLANE_TEST_PSK,
    );
    tunnel_node.name = "Tunnel Node".to_string();
    tunnel_node.status = "online".to_string();
    tunnel_node.tunnel_mode = true;
    tunnel_node.tunnel_connected = true;
    tunnel_node.remote_config = None;
    let repository = Arc::new(InMemoryProxyNodeRepository::seed(vec![tunnel_node]));
    let data_state = GatewayDataState::with_proxy_node_repository_for_tests(repository)
        .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY)
        .with_system_config_values_for_tests([(
            "external_models_proxy_node_id".to_string(),
            json!("tunnel-node"),
        )]);
    let state = AppState::new()
        .expect("gateway should build")
        .with_data_state_for_tests(data_state);
    let tunnel_state = state.tunnel.app_state();
    let (proxy_tx, mut proxy_rx) = aether_runtime::bounded_queue(8);
    let (proxy_close_tx, _) = watch::channel(false);
    tunnel_state.hub.register_proxy(Arc::new(
        TunnelProxyConn::new(
            700,
            "tunnel-node".to_string(),
            "Tunnel Node".to_string(),
            proxy_tx,
            proxy_close_tx,
            16,
            2,
        )
        .with_tunnel_generation(TUNNEL_CONTROL_PLANE_TEST_GENERATION.to_string())
        .with_authenticated_key(TUNNEL_CONTROL_PLANE_TEST_PSK.to_string()),
    ));

    let gateway = build_router_with_state(state);
    let (gateway_url, gateway_handle) = start_server(gateway).await;
    let request_task = tokio::spawn(async move {
        trusted_admin(
            reqwest::Client::new().post(format!("{gateway_url}/api/admin/models/external/refresh")),
        )
        .send()
        .await
    });

    let request_headers = match tokio::time::timeout(Duration::from_secs(5), proxy_rx.recv())
        .await
        .expect("headers frame should arrive before timeout")
        .expect("headers frame should arrive")
    {
        Message::Binary(data) => data,
        other => panic!("unexpected message: {other:?}"),
    };
    let request_header =
        tunnel_protocol::FrameHeader::parse(&request_headers).expect("request header should parse");
    assert_eq!(request_header.msg_type, tunnel_protocol::REQUEST_HEADERS);
    let meta_payload = tunnel_protocol::decode_payload(&request_headers, &request_header)
        .expect("request header payload should decode");
    let meta: tunnel_protocol::RequestMeta =
        serde_json::from_slice(&meta_payload).expect("request meta should parse");
    assert_eq!(meta.method, "GET");
    assert_eq!(meta.url, source_url);
    assert_eq!(meta.follow_redirects, Some(false));
    assert_eq!(
        meta.headers.get("accept").map(String::as_str),
        Some("application/json")
    );

    let request_body = match tokio::time::timeout(Duration::from_secs(5), proxy_rx.recv())
        .await
        .expect("body frame should arrive before timeout")
        .expect("body frame should arrive")
    {
        Message::Binary(data) => data,
        other => panic!("unexpected message: {other:?}"),
    };
    let request_body_header =
        tunnel_protocol::FrameHeader::parse(&request_body).expect("request body should parse");
    assert_eq!(request_body_header.msg_type, tunnel_protocol::REQUEST_BODY);
    assert_ne!(
        request_body_header.flags & tunnel_protocol::FLAG_END_STREAM,
        0,
        "catalog request body frame should close the stream"
    );

    let response_meta = tunnel_protocol::ResponseMeta {
        status: 200,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
    };
    let response_meta_bytes =
        serde_json::to_vec(&response_meta).expect("response meta should serialize");
    let mut response_headers_frame = tunnel_protocol::encode_frame(
        request_header.stream_id,
        tunnel_protocol::RESPONSE_HEADERS,
        0,
        &response_meta_bytes,
    );
    tunnel_state
        .hub
        .handle_proxy_frame(700, &mut response_headers_frame)
        .await;

    let response_payload = serde_json::to_vec(&json!({
        "tunnel-provider": {
            "name": "Tunnel",
            "models": {
                "tunnel": {
                    "name": "TUNNEL"
                }
            }
        }
    }))
    .expect("response payload should serialize");
    let mut response_body_frame = tunnel_protocol::encode_frame(
        request_header.stream_id,
        tunnel_protocol::RESPONSE_BODY,
        0,
        &response_payload,
    );
    tunnel_state
        .hub
        .handle_proxy_frame(700, &mut response_body_frame)
        .await;

    let mut response_end_frame = tunnel_protocol::encode_frame(
        request_header.stream_id,
        tunnel_protocol::STREAM_END,
        0,
        &[],
    );
    tunnel_state
        .hub
        .handle_proxy_frame(700, &mut response_end_frame)
        .await;

    let response = tokio::time::timeout(Duration::from_secs(5), request_task)
        .await
        .expect("catalog request should complete before timeout")
        .expect("request task should complete")
        .expect("catalog request should succeed");
    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(
        payload["catalog"]["tunnel-provider"]["models"]["tunnel"]["name"],
        "TUNNEL"
    );

    gateway_handle.abort();
}

#[tokio::test]
async fn gateway_rejects_external_models_proxy_config_when_data_stores_are_unavailable() {
    let gateway = build_router_with_state(AppState::new().expect("gateway should build"));
    let (gateway_url, gateway_handle) = start_server(gateway).await;
    let client = reqwest::Client::new();

    let response =
        trusted_admin(client.put(format!("{gateway_url}/api/admin/models/external/config")))
            .json(&json!({ "proxy_node_id": null }))
            .send()
            .await
            .expect("missing config store request should complete");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    gateway_handle.abort();

    let data_state =
        GatewayDataState::disabled()
            .with_system_config_values_for_tests(Vec::<(String, serde_json::Value)>::new());
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(data_state),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response =
        trusted_admin(client.put(format!("{gateway_url}/api/admin/models/external/config")))
            .json(&json!({ "proxy_node_id": "manual-node" }))
            .send()
            .await
            .expect("missing proxy reader request should complete");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    gateway_handle.abort();
}

#[tokio::test]
async fn gateway_handles_admin_external_models_locally_with_trusted_admin_principal() {
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let upstream_hits_clone = Arc::clone(&upstream_hits);
    let upstream = Router::new().route(
        "/api/admin/models/external",
        any(move |_request: Request| {
            let upstream_hits_inner = Arc::clone(&upstream_hits_clone);
            async move {
                *upstream_hits_inner.lock().expect("mutex should lock") += 1;
                (StatusCode::OK, Body::from("unexpected upstream hit"))
            }
        }),
    );

    let (_upstream_url, upstream_handle) = start_server(upstream).await;
    let external_source_hits = Arc::new(Mutex::new(0usize));
    let external_source_hits_clone = Arc::clone(&external_source_hits);
    let external_source = Router::new().route(
        "/api.json",
        any(move |_request: Request| {
            let external_source_hits_inner = Arc::clone(&external_source_hits_clone);
            async move {
                *external_source_hits_inner
                    .lock()
                    .expect("mutex should lock") += 1;
                (
                    StatusCode::OK,
                    axum::Json(json!({
                        "direct-provider": {
                            "name": "Direct",
                            "models": {
                                "direct": {
                                    "name": "DIRECT"
                                }
                            }
                        }
                    })),
                )
            }
        }),
    );
    let (external_source_url, external_source_handle) = start_server(external_source).await;
    let proxy_hits = Arc::new(Mutex::new(0usize));
    let proxy_hits_clone = Arc::clone(&proxy_hits);
    let proxy_auths = Arc::new(Mutex::new(Vec::<Option<String>>::new()));
    let proxy_auths_clone = Arc::clone(&proxy_auths);
    let proxy = Router::new().fallback(any(move |request: Request| {
        let proxy_hits_inner = Arc::clone(&proxy_hits_clone);
        let proxy_auths_inner = Arc::clone(&proxy_auths_clone);
        async move {
            *proxy_hits_inner.lock().expect("mutex should lock") += 1;
            proxy_auths_inner.lock().expect("mutex should lock").push(
                request
                    .headers()
                    .get("proxy-authorization")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string),
            );
            (
                StatusCode::OK,
                axum::Json(json!({
                    "manual-provider": {
                        "name": "Manual",
                        "models": {
                            "manual": {
                                "name": "MANUAL"
                            }
                        }
                    }
                })),
            )
        }
    }));
    let (proxy_url, proxy_handle) = start_server(proxy).await;
    let _guard =
        set_admin_external_models_source_url_for_tests(&format!("{external_source_url}/api.json"));

    let mut manual_node = online_manual_proxy_node("manual-node", proxy_url);
    manual_node.proxy_username = Some("alice".to_string());
    manual_node.proxy_password = Some("supersecret".to_string());
    let repository = Arc::new(InMemoryProxyNodeRepository::seed(vec![manual_node]));
    let data_state = GatewayDataState::with_proxy_node_repository_for_tests(repository)
        .with_system_config_values_for_tests(Vec::<(String, serde_json::Value)>::new());
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(data_state),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;
    let client = reqwest::Client::new();

    // Reading never fetches: without a snapshot the catalog is simply unavailable.
    let response = trusted_admin(client.get(format!("{gateway_url}/api/admin/models/external")))
        .send()
        .await
        .expect("request should succeed");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(*external_source_hits.lock().expect("mutex should lock"), 0);

    let response =
        trusted_admin(client.post(format!("{gateway_url}/api/admin/models/external/refresh")))
            .send()
            .await
            .expect("refresh request should succeed");
    assert_eq!(response.status(), StatusCode::OK);
    let response = trusted_admin(client.get(format!("{gateway_url}/api/admin/models/external")))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(
        payload["direct-provider"]["models"]["direct"]["name"],
        json!("DIRECT")
    );
    assert_eq!(*external_source_hits.lock().expect("mutex should lock"), 1);
    assert_eq!(*upstream_hits.lock().expect("mutex should lock"), 0);

    let response =
        trusted_admin(client.put(format!("{gateway_url}/api/admin/models/external/config")))
            .json(&json!({ "proxy_node_id": "manual-node" }))
            .send()
            .await
            .expect("proxy config request should succeed");
    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(payload["proxy_node_id"], "manual-node");
    assert_eq!(payload["cache_cleared"], false);

    let response = trusted_admin(client.get(format!("{gateway_url}/api/admin/models/external")))
        .send()
        .await
        .expect("proxied request should succeed");
    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(
        payload["direct-provider"]["models"]["direct"]["name"],
        "DIRECT"
    );
    assert_eq!(*proxy_hits.lock().expect("mutex should lock"), 0);
    // Egress changes only apply when the catalog is explicitly refreshed.
    let response =
        trusted_admin(client.delete(format!("{gateway_url}/api/admin/models/external/cache")))
            .send()
            .await
            .expect("clear should succeed");
    assert_eq!(response.status(), StatusCode::OK);
    let response = trusted_admin(client.get(format!("{gateway_url}/api/admin/models/external")))
        .send()
        .await
        .expect("read should succeed");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let response =
        trusted_admin(client.post(format!("{gateway_url}/api/admin/models/external/refresh")))
            .send()
            .await
            .expect("refresh should succeed");
    assert_eq!(response.status(), StatusCode::OK);
    let response = trusted_admin(client.get(format!("{gateway_url}/api/admin/models/external")))
        .send()
        .await
        .expect("read should succeed");
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(
        payload["manual-provider"]["models"]["manual"]["name"],
        json!("MANUAL")
    );
    assert_eq!(*external_source_hits.lock().expect("mutex should lock"), 1);
    assert_eq!(*proxy_hits.lock().expect("mutex should lock"), 1);
    let expected_proxy_auth = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode("alice:supersecret")
    );
    assert_eq!(
        proxy_auths.lock().expect("mutex should lock").as_slice(),
        [Some(expected_proxy_auth)]
    );

    gateway_handle.abort();
    proxy_handle.abort();
    external_source_handle.abort();
    upstream_handle.abort();
}

#[tokio::test]
async fn gateway_clears_admin_external_models_cache_locally_with_trusted_admin_principal() {
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let upstream_hits_clone = Arc::clone(&upstream_hits);
    let upstream = Router::new().route(
        "/api/admin/models/external/cache",
        any(move |_request: Request| {
            let upstream_hits_inner = Arc::clone(&upstream_hits_clone);
            async move {
                *upstream_hits_inner.lock().expect("mutex should lock") += 1;
                (StatusCode::OK, Body::from("unexpected upstream hit"))
            }
        }),
    );

    let (upstream_url, upstream_handle) = start_server(upstream).await;
    let gateway = build_router_with_state(AppState::new().expect("gateway should build"));
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response = trusted_admin(
        reqwest::Client::new().delete(format!("{gateway_url}/api/admin/models/external/cache")),
    )
    .send()
    .await
    .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(payload["cleared"], false);
    assert_eq!(payload["message"], "缓存不存在");
    assert_eq!(*upstream_hits.lock().expect("mutex should lock"), 0);

    gateway_handle.abort();
    upstream_handle.abort();
}
