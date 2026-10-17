use super::*;

#[tokio::test]
async fn removed_remote_control_routes_cannot_pair_proxy_or_serve_assets() {
    let router = crate::router::build_router_with_state(AppState::new().unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    for path in [
        "/api/vscodex",
        "/api/vscodex/",
        "/api/vscodex/pair",
        "/api/vscodex/ws",
        "/api/users/me/vscodex",
        "/api/users/me/vscodex/",
        "/api/users/me/vscodex/devices",
        "/api/users/me/vscodex/pairings",
        "/api/users/me/vscodex/ws-tickets",
        "/api/users/me/vscodex/devices/fixture-device",
        "/aether-vscodex",
        "/aether-vscodex/",
        "/aether-vscodex/index.html",
        "/aether-vscodex/assets/fixture.js",
    ] {
        for method in [
            reqwest::Method::GET,
            reqwest::Method::POST,
            reqwest::Method::DELETE,
        ] {
            let response = client
                .request(method, format!("http://{address}{path}"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::GONE, "{path}");
            assert_eq!(response.headers()["cache-control"], "no-store");
            let body = response.text().await.unwrap();
            assert!(body.contains("Remote control has been removed"));
            assert!(!body.contains("<html"));
        }
    }
    task.abort();
}
