//! Profile pictures: `PUT/DELETE /api/avatars/me` for the signed-in user and
//! `GET /api/avatars/{user_id}` for everybody who knows a user id.
//!
//! The picture itself is public to anybody holding the (unguessable) user id because browsers
//! cannot send the in-memory access token with an `<img>` request. Every response that references
//! someone carries the picture's version, so the URL changes whenever the picture does and can be
//! cached forever.

use aether_data::repository::avatars::{avatar_content_type_for, MAX_AVATAR_BYTES};
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::control::GatewayPublicRequestContext;
use crate::AppState;

pub(crate) fn mount_avatar_routes(router: Router<AppState>) -> Router<AppState> {
    router.merge(
        Router::new()
            .route(
                "/api/avatars/me",
                axum::routing::put(own_avatar).delete(own_avatar),
            )
            .route("/api/avatars/{user_id}", get(read_avatar))
            // A little above the cap so the handler can answer with a clear message.
            .layer(DefaultBodyLimit::max(MAX_AVATAR_BYTES + 16 * 1024)),
    )
}

fn error(status: StatusCode, detail: &str) -> Response {
    let mut response = (status, Json(json!({ "detail": detail }))).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn valid_user_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 36 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Version string as it appears in URLs and ETags.
fn etag(version: i64) -> String {
    format!("\"{version}\"")
}

#[derive(serde::Deserialize)]
struct VersionQuery {
    v: Option<i64>,
}

async fn read_avatar(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Query(query): Query<VersionQuery>,
    headers: HeaderMap,
) -> Response {
    if !valid_user_id(&user_id) {
        return error(StatusCode::NOT_FOUND, "No picture");
    }
    let Some(repository) = state.data.avatar_repository() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Picture storage unavailable",
        );
    };
    let avatar = match repository.get(&user_id).await {
        Ok(Some(avatar)) => avatar,
        Ok(None) => {
            let mut response = error(StatusCode::NOT_FOUND, "No picture");
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=60"),
            );
            return response;
        }
        Err(_) => {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Picture storage unavailable",
            )
        }
    };
    let tag = etag(avatar.version);
    // Only a request that names the current version may be cached forever; an old or missing
    // version is revalidated soon so a replaced picture shows up.
    let cache = if query.v == Some(avatar.version) {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=300"
    };
    let not_modified = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(',').any(|candidate| candidate.trim() == tag));
    let mut response = if not_modified {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        let mut response = Bytes::from(avatar.data).into_response();
        if let Ok(value) = HeaderValue::from_str(&avatar.content_type) {
            response.headers_mut().insert(header::CONTENT_TYPE, value);
        }
        response
    };
    let out = response.headers_mut();
    out.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    if let Ok(value) = HeaderValue::from_str(&tag) {
        out.insert(header::ETAG, value);
    }
    out.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    out.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    response
}

async fn own_avatar(State(state): State<AppState>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    if parts.headers.get_all(header::AUTHORIZATION).iter().count() > 1 {
        return error(StatusCode::UNAUTHORIZED, "Ambiguous authorization");
    }
    let context = GatewayPublicRequestContext::from_request_parts(
        uuid::Uuid::new_v4().to_string(),
        &parts.method,
        &parts.uri,
        &parts.headers,
        None,
    );
    let auth = match crate::handlers::public::resolve_authenticated_local_user(
        &state,
        &context,
        &parts.headers,
    )
    .await
    {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let Some(repository) = state.data.avatar_repository() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Picture storage unavailable",
        );
    };
    let user_id = auth.user.id.clone();
    if parts.method == axum::http::Method::DELETE {
        return match repository.delete(&user_id).await {
            Ok(removed) => json_ok(json!({ "removed": removed })),
            Err(_) => error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Picture storage unavailable",
            ),
        };
    }
    let bytes = match axum::body::to_bytes(body, MAX_AVATAR_BYTES + 1).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "The picture is too large (256 KiB at most)",
            )
        }
    };
    if bytes.len() > MAX_AVATAR_BYTES {
        return error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "The picture is too large (256 KiB at most)",
        );
    }
    let Some(content_type) = avatar_content_type_for(&bytes) else {
        return error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Only PNG, JPEG and WebP pictures are accepted",
        );
    };
    match repository.put(&user_id, content_type, &bytes).await {
        Ok(version) => json_ok(json!({
            "version": version,
            "url": format!("/api/avatars/{user_id}?v={version}"),
        })),
        Err(_) => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Picture storage unavailable",
        ),
    }
}

fn json_ok(value: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Picture versions for the given users; users without a picture are absent. Failures only mean
/// that no picture is shown, never that the page around them fails.
pub(crate) async fn versions_for(
    state: &AppState,
    user_ids: impl IntoIterator<Item = String>,
) -> std::collections::HashMap<String, i64> {
    let mut ids: Vec<String> = user_ids.into_iter().collect();
    ids.sort();
    ids.dedup();
    let Some(repository) = state.data.avatar_repository() else {
        return Default::default();
    };
    repository.versions(&ids).await.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_data::repository::avatars::InMemoryAvatarRepository;
    use aether_data::repository::feedback::InMemoryFeedbackRepository;
    use serde_json::Value;
    use std::sync::Arc;

    #[test]
    fn only_plain_ids_are_looked_up() {
        assert!(valid_user_id("0b1c2d3e-4f50-4a6b-8c7d-9e0f1a2b3c4d"));
        for bad in ["", "../etc", "a/b", "a b", &"x".repeat(37), "ü"] {
            assert!(!valid_user_id(bad), "{bad:?}");
        }
    }

    async fn user(state: &AppState, name: &str) -> (String, HeaderMap) {
        let u = state
            .create_local_auth_user_with_settings(
                Some(format!("{name}@example.test")),
                true,
                name.into(),
                "hash".into(),
                "user".into(),
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

    fn png() -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(b"synthetic image payload");
        bytes
    }

    #[tokio::test]
    async fn pictures_are_validated_served_publicly_with_versioned_caching_and_removable() {
        let state = AppState::new().unwrap().with_data_state_for_tests(
            crate::data::GatewayDataState::disabled()
                .with_avatar_repository_for_tests(Arc::new(InMemoryAvatarRepository::default()))
                .with_feedback_repository_for_tests(
                    Arc::new(InMemoryFeedbackRepository::default()),
                ),
        );
        let (alice_id, alice) = user(&state, "alice").await;
        let (_, bob) = user(&state, "bob").await;
        let (url, task) = server(state).await;
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let put = |headers: HeaderMap, body: Vec<u8>| {
            let client = client.clone();
            let url = format!("{url}/api/avatars/me");
            async move {
                client
                    .put(url)
                    .headers(headers)
                    .body(body)
                    .send()
                    .await
                    .unwrap()
            }
        };

        // Uploading needs a session, and only real PNG/JPEG/WebP bytes are accepted.
        assert_eq!(
            client
                .put(format!("{url}/api/avatars/me"))
                .body(png())
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        for (label, body, status) in [
            (
                "svg",
                b"<svg xmlns='http://www.w3.org/2000/svg'><script>alert(1)</script></svg>".to_vec(),
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
            ),
            (
                "text",
                b"hello".to_vec(),
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
            ),
            ("empty", Vec::new(), StatusCode::UNSUPPORTED_MEDIA_TYPE),
            (
                "huge",
                {
                    let mut big = png();
                    big.resize(MAX_AVATAR_BYTES + 1, 0);
                    big
                },
                StatusCode::PAYLOAD_TOO_LARGE,
            ),
        ] {
            assert_eq!(put(alice.clone(), body).await.status(), status, "{label}");
        }
        assert_eq!(
            client
                .get(format!("{url}/api/avatars/{alice_id}"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND,
            "nothing was stored by the rejected uploads"
        );

        let stored: Value = put(alice.clone(), png()).await.json().await.unwrap();
        let version = stored["version"].as_i64().unwrap();
        assert_eq!(
            stored["url"],
            format!("/api/avatars/{alice_id}?v={version}")
        );

        // Anybody (no session) can fetch it; the matching version may be cached for good.
        let fetched = client
            .get(format!("{url}/api/avatars/{alice_id}?v={version}"))
            .send()
            .await
            .unwrap();
        assert_eq!(fetched.status(), StatusCode::OK);
        let headers = fetched.headers().clone();
        assert_eq!(headers["content-type"], "image/png");
        assert_eq!(
            headers["cache-control"],
            "public, max-age=31536000, immutable"
        );
        assert_eq!(headers["x-content-type-options"], "nosniff");
        assert!(headers["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("sandbox"));
        assert_eq!(fetched.bytes().await.unwrap().to_vec(), png());
        // A stale or missing version is revalidated soon, and the ETag answers 304.
        let unversioned = client
            .get(format!("{url}/api/avatars/{alice_id}"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            unversioned.headers()["cache-control"],
            "public, max-age=300"
        );
        let etag = unversioned.headers()["etag"].clone();
        let revalidated = client
            .get(format!("{url}/api/avatars/{alice_id}"))
            .header("if-none-match", etag)
            .send()
            .await
            .unwrap();
        assert_eq!(revalidated.status(), StatusCode::NOT_MODIFIED);
        for bad in ["../x", "a%20b"] {
            assert_eq!(
                client
                    .get(format!("{url}/api/avatars/{bad}"))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::NOT_FOUND
            );
        }

        // Pictures belong to the signed-in user: bob replaces and removes only his own.
        let bobs: Value = put(bob.clone(), png()).await.json().await.unwrap();
        assert_ne!(bobs["url"], stored["url"]);
        let replaced: Value = put(alice.clone(), {
            let mut other = png();
            other.extend_from_slice(b" v2");
            other
        })
        .await
        .json()
        .await
        .unwrap();
        assert!(
            replaced["version"].as_i64().unwrap() > version,
            "a replacement gets a newer version"
        );

        // The versions ride along with discussion responses so the console can show pictures.
        let created: Value = client
            .post(format!("{url}/api/feedback"))
            .headers(alice.clone())
            .json(&json!({"title":"T","description":"D","category":"bug","client_request_id":uuid::Uuid::new_v4().to_string()}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let id = created["issue"]["id"].as_str().unwrap();
        let detail: Value = client
            .get(format!("{url}/api/feedback/{id}"))
            .headers(bob.clone())
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(detail["avatars"][&alice_id], replaced["version"]);
        let list: Value = client
            .get(format!("{url}/api/feedback"))
            .headers(bob.clone())
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(list["avatars"][&alice_id], replaced["version"]);

        // The signed-in user learns the version of their own picture from /api/users/me.
        let me: Value = client
            .get(format!("{url}/api/users/me"))
            .headers(alice.clone())
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(me["avatar_version"], replaced["version"], "{me}");

        let removed: Value = client
            .delete(format!("{url}/api/avatars/me"))
            .headers(alice.clone())
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(removed["removed"], true);
        assert_eq!(
            client
                .get(format!("{url}/api/avatars/{alice_id}"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
        let detail: Value = client
            .get(format!("{url}/api/feedback/{id}"))
            .headers(bob.clone())
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(
            detail["avatars"].get(&alice_id).is_none(),
            "no picture, no entry"
        );
        assert_eq!(
            client
                .delete(format!("{url}/api/avatars/me"))
                .headers(alice)
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap()["removed"],
            false
        );
        task.abort();
    }
}
