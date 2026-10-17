use crate::{control::GatewayPublicRequestContext, AppState, GatewayError};
use aether_data::repository::feedback::*;
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Request, State},
    http::{Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;

mod mentions;
#[cfg(test)]
mod tests;

pub(crate) fn mount_feedback_routes(router: Router<AppState>) -> Router<AppState> {
    router.merge(
        Router::new()
            .route("/api/feedback", get(user_request).post(user_request))
            .route("/api/feedback/{id}", get(user_request))
            .route(
                "/api/feedback/{id}/commands",
                axum::routing::post(user_request),
            )
            .layer(DefaultBodyLimit::max(64 * 1024)),
    )
}

fn error(status: StatusCode, detail: &str) -> Response {
    response(status, json!({"detail":detail}))
}
fn response(status: StatusCode, value: serde_json::Value) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}
fn store_error(error_value: FeedbackError) -> Response {
    match error_value {
        FeedbackError::NotFound => error(StatusCode::NOT_FOUND, "Feedback not found"),
        FeedbackError::Forbidden => error(
            StatusCode::FORBIDDEN,
            "Feedback operation not permitted for the current user and role",
        ),
        FeedbackError::Conflict => error(
            StatusCode::CONFLICT,
            "Feedback changed or idempotency key was reused; read back before retrying",
        ),
        FeedbackError::Invalid(detail) => error(StatusCode::BAD_REQUEST, detail),
        FeedbackError::Unavailable => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Feedback storage unavailable",
        ),
    }
}

async fn user_request(State(state): State<AppState>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    if parts
        .headers
        .get_all(axum::http::header::AUTHORIZATION)
        .iter()
        .count()
        > 1
    {
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
    let bytes = match axum::body::to_bytes(body, 64 * 1024).await {
        Ok(body) => body,
        Err(_) => return error(StatusCode::PAYLOAD_TOO_LARGE, "Feedback request too large"),
    };
    dispatch(
        &state,
        &auth.user.id,
        &auth.user.role,
        false,
        &parts.method,
        &parts.uri,
        &bytes,
    )
    .await
}

pub(crate) async fn admin_response(
    state: &AppState,
    context: &GatewayPublicRequestContext,
    body: Option<&Bytes>,
) -> Result<Option<Response>, GatewayError> {
    let Some(decision) = context
        .control_decision
        .as_ref()
        .filter(|d| d.route_family.as_deref() == Some("feedback_manage"))
    else {
        return Ok(None);
    };
    let Some(principal) = decision.admin_principal.as_ref() else {
        return Ok(Some(error(
            StatusCode::UNAUTHORIZED,
            "Administrator authentication required",
        )));
    };
    if crate::control::validate_admin_role_route_permission(&context.request_method, decision)
        .and_then(|_| {
            crate::control::validate_management_token_admin_route_permission(
                &context.request_method,
                decision,
                principal.management_token_permissions.as_deref(),
            )
        })
        .is_err()
    {
        return Ok(Some(error(
            StatusCode::FORBIDDEN,
            "Feedback permission denied",
        )));
    }
    let uri: Uri = context
        .request_path_and_query()
        .parse()
        .unwrap_or_else(|_| Uri::from_static("/invalid"));
    let empty = Bytes::new();
    if body
        .and_then(|body| serde_json::from_slice::<serde_json::Value>(body).ok())
        .is_some_and(|value| {
            value.get("diagnostic_request_id").is_some()
                || value
                    .get("include_diagnostics")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                || value.get("request_id").is_some_and(|v| !v.is_null())
        })
        && !crate::api::diagnostics::can_read_admin(principal)
    {
        return Ok(Some(error(
            StatusCode::FORBIDDEN,
            "Monitoring read permission required to attach diagnostics",
        )));
    }
    Ok(Some(
        dispatch(
            state,
            &principal.user_id,
            &principal.user_role,
            true,
            &context.request_method,
            &uri,
            body.unwrap_or(&empty),
        )
        .await,
    ))
}

async fn dispatch(
    state: &AppState,
    user: &str,
    role: &str,
    admin: bool,
    method: &Method,
    uri: &Uri,
    body: &Bytes,
) -> Response {
    if admin && !crate::roles::can_access_admin_console(role) {
        return error(StatusCode::FORBIDDEN, "Administrator role required");
    }
    let actual_admin = is_feedback_admin(role);
    let viewer = if actual_admin { None } else { Some(user) };
    let prefix = if admin {
        "/api/admin/feedback"
    } else {
        "/api/feedback"
    };
    let suffix = uri
        .path()
        .strip_prefix(prefix)
        .unwrap_or("/invalid")
        .trim_matches('/');
    let parts: Vec<_> = if suffix.is_empty() {
        vec![]
    } else {
        suffix.split('/').collect()
    };
    let mentionable = parts == ["mentionable"];
    let numbered = parts.first().is_some_and(|id| {
        !id.is_empty() && id.len() <= 9 && id.chars().all(|c| c.is_ascii_digit())
    });
    if parts.len() > 2
        || (!mentionable
            && !numbered
            && parts
                .first()
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_err()))
        || (parts.len() == 2 && parts[1] != "commands")
    {
        return error(StatusCode::NOT_FOUND, "Feedback route not found");
    }
    let Some(repository) = state.data.feedback_repository() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Feedback storage unavailable; apply the feedback migration",
        );
    };
    if method == Method::GET && mentionable {
        return list_mentionable_admins(state).await;
    }
    // `#12` style numbers resolve to the issue id; visibility is still enforced by the repository.
    let issue_id: String = match parts.first() {
        Some(id) if numbered => match repository.id_for_number(id.parse().unwrap_or(0)).await {
            Ok(Some(found)) => found,
            Ok(None) => return error(StatusCode::NOT_FOUND, "Feedback not found"),
            Err(e) => return store_error(e),
        },
        Some(id) => (*id).to_string(),
        None => String::new(),
    };
    let mut query = std::collections::BTreeMap::new();
    for (k, v) in url::form_urlencoded::parse(uri.query().unwrap_or("").as_bytes()) {
        if ![
            "status",
            "category",
            "search",
            "after",
            "skip",
            "limit",
            "owner_id",
            "after_event",
        ]
        .contains(&k.as_ref())
            || query.insert(k.into_owned(), v.into_owned()).is_some()
        {
            return error(
                StatusCode::BAD_REQUEST,
                "Invalid or duplicate feedback filter",
            );
        }
    }
    let integer = |key: &str, default: i64| -> Result<i64, Response> {
        query
            .get(key)
            .map(|v| v.parse::<i64>())
            .transpose()
            .map(|v| v.unwrap_or(default))
            .map_err(|_| error(StatusCode::BAD_REQUEST, "Invalid feedback pagination"))
    };
    let limit = match integer("limit", 20) {
        Ok(v) if (1..=100).contains(&v) => v as usize,
        _ => return error(StatusCode::BAD_REQUEST, "limit must be 1-100"),
    };
    if method == Method::GET && parts.is_empty() {
        let offset = match integer("skip", 0) {
            Ok(v) if (0..=1_000_000).contains(&v) => v as usize,
            _ => return error(StatusCode::BAD_REQUEST, "Invalid skip"),
        };
        let after = if query.contains_key("after") {
            match integer("after", 0) {
                Ok(v) if v >= 0 => Some(v),
                _ => return error(StatusCode::BAD_REQUEST, "Invalid cursor"),
            }
        } else {
            None
        };
        if after.is_some() && offset != 0 {
            return error(StatusCode::BAD_REQUEST, "Do not combine after and skip");
        }
        if query.get("status").is_some_and(|v| !valid_status(v))
            || query.get("category").is_some_and(|v| !valid_category(v))
            || query.get("search").is_some_and(|v| v.chars().count() > 200)
        {
            return error(StatusCode::BAD_REQUEST, "Invalid feedback filter");
        }
        if !actual_admin && query.contains_key("owner_id") {
            return error(
                StatusCode::FORBIDDEN,
                "Ordinary viewers cannot filter by owner_id",
            );
        }
        let q = FeedbackQuery {
            viewer_id: viewer.map(str::to_owned),
            owner_id: query.get("owner_id").cloned(),
            status: query.get("status").cloned(),
            category: query.get("category").cloned(),
            search: query.get("search").cloned(),
            after,
            offset,
            limit,
        };
        return match repository.list(&q).await {
            Ok(page) => response(StatusCode::OK, serde_json::to_value(page).unwrap()),
            Err(e) => store_error(e),
        };
    }
    if method == Method::GET && parts.len() == 1 {
        let after = match integer("after_event", 0) {
            Ok(v) if v >= 0 => v,
            _ => return error(StatusCode::BAD_REQUEST, "Invalid event cursor"),
        };
        return match repository.detail(&issue_id, viewer, after, limit).await {
            Ok(detail) => response(StatusCode::OK, serde_json::to_value(detail).unwrap()),
            Err(e) => store_error(e),
        };
    }
    if method != Method::POST {
        return error(
            StatusCode::METHOD_NOT_ALLOWED,
            "Feedback method not allowed",
        );
    }
    if body.len() > 64 * 1024 {
        return error(StatusCode::PAYLOAD_TOO_LARGE, "Feedback request too large");
    }
    let mut mention_text: Option<String> = None;
    let mut mention_source: Option<mentions::MentionSource> = None;
    let result = if parts.is_empty() {
        if authorize_feedback_create(user, role).is_err() {
            return error(
                StatusCode::FORBIDDEN,
                "An authenticated user or actual administrator is required to create feedback",
            );
        }
        let mut value: serde_json::Value = match serde_json::from_slice(body) {
            Ok(value) => value,
            Err(_) => return error(StatusCode::BAD_REQUEST, "Invalid feedback submission"),
        };
        let include = match value
            .as_object_mut()
            .and_then(|v| v.remove("include_diagnostics"))
        {
            None | Some(serde_json::Value::Bool(false)) => false,
            Some(serde_json::Value::Bool(true)) => true,
            _ => {
                return error(
                    StatusCode::BAD_REQUEST,
                    "include_diagnostics must be boolean",
                )
            }
        };
        let mut input: CreateFeedback = match serde_json::from_value(value) {
            Ok(input) => input,
            Err(_) => return error(StatusCode::BAD_REQUEST, "Invalid feedback submission"),
        };
        input.title = input.title.trim().to_owned();
        input.description = input.description.trim().to_owned();
        mention_text = Some(input.description.clone());
        mention_source = Some(mentions::MentionSource::Created);
        if let Some(id) = input.request_id.as_deref() {
            let summary = match crate::api::diagnostics::feedback_summary(
                state,
                id,
                if admin { None } else { Some(user) },
            )
            .await
            {
                Ok(summary) => summary,
                Err(response) => return response,
            };
            if include {
                input.description.push_str(&format!("\n\n{summary}"));
            }
        } else if include {
            return error(
                StatusCode::BAD_REQUEST,
                "Request ID required to attach diagnostics",
            );
        }
        repository.create(user, role, &input).await
    } else if parts.len() == 2 {
        let mut value: serde_json::Value = match serde_json::from_slice(body) {
            Ok(value) => value,
            Err(_) => return error(StatusCode::BAD_REQUEST, "Invalid feedback command"),
        };
        let diagnostic = value
            .as_object_mut()
            .and_then(|v| v.remove("diagnostic_request_id"));
        let mut input: FeedbackCommand = match serde_json::from_value(value) {
            Ok(input) => input,
            Err(_) => return error(StatusCode::BAD_REQUEST, "Invalid feedback command"),
        };
        if let Some(diagnostic) = diagnostic {
            let (Some(id), FeedbackAction::Reply { content }) =
                (diagnostic.as_str(), &mut input.action)
            else {
                return error(
                    StatusCode::BAD_REQUEST,
                    "Diagnostics can only be attached to a reply with a request ID",
                );
            };
            // Check feedback visibility before reading another request or its evidence.
            if let Err(e) = repository.detail(&issue_id, viewer, 0, 1).await {
                return store_error(e);
            }
            let summary = match crate::api::diagnostics::feedback_summary(
                state,
                id,
                if admin { None } else { Some(user) },
            )
            .await
            {
                Ok(summary) => summary,
                Err(response) => return response,
            };
            content.push_str(&format!("\n\n{summary}"));
        }
        mention_source = match &input.action {
            FeedbackAction::Reply { .. } => Some(mentions::MentionSource::Reply),
            FeedbackAction::SetStatus {
                comment: Some(_), ..
            } => Some(mentions::MentionSource::StatusComment),
            _ => None,
        };
        mention_text = match &input.action {
            FeedbackAction::Reply { content } => Some(content.clone()),
            FeedbackAction::SetStatus { comment, .. } => comment.clone(),
            FeedbackAction::SetPinned { .. } => None,
        };
        repository
            .command(&issue_id, user, role, !admin, &input)
            .await
    } else {
        return error(
            StatusCode::METHOD_NOT_ALLOWED,
            "Use the commands endpoint to reply or change status",
        );
    };
    match result {
        Ok(mutation) => {
            tracing::info!(log_type="audit",event_name="feedback_mutation",user_id=user,role,issue_id=%mutation.issue.summary.id,version=mutation.issue.summary.version,replayed=mutation.replayed,"Feedback changed");
            if !mutation.replayed {
                if let (Some(text), Some(source)) = (mention_text.take(), mention_source) {
                    if text.contains('@') {
                        tokio::spawn(mentions::notify_mentions(
                            state.clone(),
                            mutation.issue.summary.clone(),
                            user.to_owned(),
                            source,
                            text,
                        ));
                    }
                }
            }
            response(
                if parts.is_empty() && !mutation.replayed {
                    StatusCode::CREATED
                } else {
                    StatusCode::OK
                },
                serde_json::to_value(mutation).unwrap(),
            )
        }
        Err(e) => store_error(e),
    }
}

/// Administrator usernames that can be `@`-mentioned: active administrators only, sorted,
/// case-insensitively de-duplicated. Only usernames are exposed, never e-mail addresses or ids.
fn mentionable_names<'a>(users: impl IntoIterator<Item = (&'a str, &'a str, bool)>) -> Vec<String> {
    let mut names: Vec<String> = users
        .into_iter()
        .filter(|(_, role, active)| *active && is_feedback_admin(role))
        .map(|(username, _, _)| username.to_string())
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    names
}

async fn list_mentionable_admins(state: &AppState) -> Response {
    match state.data.list_export_users().await {
        Ok(users) => {
            let names = mentionable_names(
                users
                    .iter()
                    .map(|user| (user.username.as_str(), user.role.as_str(), user.is_active)),
            );
            response(StatusCode::OK, json!({ "items": names }))
        }
        Err(_) => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "User directory unavailable",
        ),
    }
}
