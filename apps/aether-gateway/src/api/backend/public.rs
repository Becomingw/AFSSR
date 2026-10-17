use axum::routing::{any, get};
use axum::Router;

use crate::{handlers::proxy::proxy_request, state::AppState};

#[cfg(test)]
#[path = "public_tests.rs"]
mod tests;

pub(crate) fn mount_public_support_routes(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/v1/models", get(proxy_request))
        .route(
            "/api/opencode/config",
            get(proxy_request).layer(axum::middleware::from_fn(opencode_no_store)),
        )
        .route("/v1beta/models", get(proxy_request))
        .route("/v1/health", get(proxy_request))
        .route("/health", get(proxy_request))
        .route("/v1/providers", get(proxy_request))
        .route("/v1/providers/{*provider_path}", get(proxy_request))
        .route("/v1/test-connection", get(proxy_request))
        .route("/test-connection", get(proxy_request))
        .route("/api/public/site-info", get(proxy_request))
        .route("/api/public/providers", get(proxy_request))
        .route("/api/public/models", get(proxy_request))
        .route("/api/public/search/models", get(proxy_request))
        .route("/api/public/stats", get(proxy_request))
        .route("/api/public/global-models", get(proxy_request))
        .route("/api/public/health/api-formats", get(proxy_request))
        .route("/api/public/health/models", get(proxy_request))
        .route("/api/public/health/related", get(proxy_request))
        .route("/api/modules/auth-status", get(proxy_request))
        .route("/api/capabilities", get(proxy_request))
        .route("/api/capabilities/user-configurable", get(proxy_request))
        .route("/api/capabilities/model/{*model_path}", get(proxy_request))
        .route("/api/vscodex", any(remote_control_removed))
        .route("/api/vscodex/", any(remote_control_removed))
        .route("/api/vscodex/{*retired_path}", any(remote_control_removed))
        .route("/api/users/me/vscodex", any(remote_control_removed))
        .route("/api/users/me/vscodex/", any(remote_control_removed))
        .route(
            "/api/users/me/vscodex/{*retired_path}",
            any(remote_control_removed),
        )
        .route("/aether-vscodex", any(remote_control_removed))
        .route("/aether-vscodex/", any(remote_control_removed))
        .route(
            "/aether-vscodex/{*retired_path}",
            any(remote_control_removed),
        )
        .route("/install/{*install_path}", get(proxy_request))
        .route("/install-tunnel/{*install_path}", get(proxy_request))
        .route("/i/{*install_path}", get(proxy_request))
        .route("/", get(proxy_request))
}

async fn opencode_no_store(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        http::header::CACHE_CONTROL,
        http::HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().append(
        http::header::VARY,
        http::HeaderValue::from_static("Authorization"),
    );
    response
}

async fn remote_control_removed() -> impl axum::response::IntoResponse {
    (
        axum::http::StatusCode::GONE,
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        axum::Json(
            serde_json::json!({"detail":"Remote control has been removed. Configure clients from API Keys instead."}),
        ),
    )
}
