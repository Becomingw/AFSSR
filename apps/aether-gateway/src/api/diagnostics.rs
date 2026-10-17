//! A deliberately allowlisted projection of existing audit evidence, never a replay.
use crate::{control::GatewayPublicRequestContext, AppState, GatewayError};
use aether_data_contracts::repository::{
    candidates::{RequestCandidateStatus, StoredRequestCandidate, REQUEST_CANDIDATE_ERROR_TYPES},
    usage::StoredRequestUsageAudit,
};
use axum::{
    extract::{Path, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use serde_json::{json, Value};

#[cfg(test)]
mod tests;

pub(crate) fn mount_diagnostic_routes(router: Router<AppState>) -> Router<AppState> {
    router.route("/api/requests/{request_id}/diagnostics", get(user_request))
}

pub(super) fn response(status: StatusCode, value: Value) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        && id != "."
        && id != ".."
}

pub(crate) fn can_read_admin(principal: &crate::control::GatewayAdminPrincipalContext) -> bool {
    let mut decision = crate::control::classify_control_route(
        &axum::http::Method::GET,
        &axum::http::Uri::from_static("/api/admin/monitoring/diagnostics/permission-check"),
        &axum::http::HeaderMap::new(),
    )
    .expect("registered diagnostic route");
    decision.admin_principal = Some(principal.clone());
    crate::control::validate_admin_role_route_permission(&axum::http::Method::GET, &decision)
        .and_then(|_| {
            crate::control::validate_management_token_admin_route_permission(
                &axum::http::Method::GET,
                &decision,
                principal.management_token_permissions.as_deref(),
            )
        })
        .is_ok()
}

async fn user_request(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: Request,
) -> Response {
    let (parts, _) = request.into_parts();
    if parts.headers.get_all(header::AUTHORIZATION).iter().count() > 1 {
        return response(
            StatusCode::UNAUTHORIZED,
            json!({"detail":"Ambiguous authorization"}),
        );
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
    if parts.uri.query().is_some_and(|query| !query.is_empty()) {
        return response(
            StatusCode::BAD_REQUEST,
            json!({"detail":"Diagnostic filters are not supported"}),
        );
    }
    diagnostic_response(&state, &id, Some(&auth.user.id)).await
}

pub(crate) async fn admin_response(
    state: &AppState,
    context: &GatewayPublicRequestContext,
) -> Result<Option<Response>, GatewayError> {
    let Some(decision) = context
        .control_decision
        .as_ref()
        .filter(|d| d.route_kind.as_deref() == Some("request_diagnostics"))
    else {
        return Ok(None);
    };
    let Some(principal) = decision.admin_principal.as_ref() else {
        return Ok(Some(response(
            StatusCode::UNAUTHORIZED,
            json!({"detail":"Administrator authentication required"}),
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
        return Ok(Some(response(
            StatusCode::FORBIDDEN,
            json!({"detail":"Diagnostic permission denied"}),
        )));
    }
    let id = decision
        .public_path
        .strip_prefix("/api/admin/monitoring/diagnostics/")
        .unwrap_or("");
    if context
        .request_query_string
        .as_deref()
        .is_some_and(|query| !query.is_empty())
    {
        return Ok(Some(response(
            StatusCode::BAD_REQUEST,
            json!({"detail":"Diagnostic filters are not supported"}),
        )));
    }
    Ok(Some(diagnostic_response(state, id, None).await))
}

async fn diagnostic_response(state: &AppState, id: &str, owner: Option<&str>) -> Response {
    if !valid_id(id) {
        return response(
            StatusCode::BAD_REQUEST,
            json!({"detail":"Invalid request ID"}),
        );
    }
    match read(state, id, owner).await {
        Ok(diagnostic) => response(
            StatusCode::OK,
            serde_json::to_value(diagnostic).expect("diagnostic serializes"),
        ),
        Err(_) => response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"detail":"Diagnostic evidence unavailable"}),
        ),
    }
}

#[derive(Default)]
struct Evidence {
    usage: Option<StoredRequestUsageAudit>,
    candidates: Vec<StoredRequestCandidate>,
}

impl Evidence {
    fn visible_to(&self, owner: &str) -> bool {
        let has_owner = self
            .usage
            .as_ref()
            .is_some_and(|u| u.user_id.as_deref() == Some(owner))
            || self
                .candidates
                .iter()
                .any(|c| c.user_id.as_deref() == Some(owner));
        has_owner
            && self
                .usage
                .as_ref()
                .is_none_or(|u| u.user_id.as_deref() == Some(owner))
            && self
                .candidates
                .iter()
                .all(|c| c.user_id.as_deref() == Some(owner))
    }
}

async fn evidence(state: &AppState, id: &str) -> Result<Evidence, GatewayError> {
    let usage = state
        .data
        .find_request_usage_by_request_id_shallow(id)
        .await
        .map_err(|_| GatewayError::Internal("Diagnostic usage read failed".into()))?;
    let candidates = state
        .data
        .read_request_candidate_trace(id, false)
        .await
        .map_err(|_| GatewayError::Internal("Diagnostic candidate read failed".into()))?
        .map(|trace| trace.candidates)
        .unwrap_or_default();
    Ok(Evidence { usage, candidates })
}

pub(crate) async fn read(
    state: &AppState,
    id: &str,
    owner: Option<&str>,
) -> Result<Diagnostic, GatewayError> {
    let mut evidence = evidence(state, id).await?;
    // Missing and inaccessible evidence have the same response, including no record counts.
    if owner.is_some_and(|owner| !evidence.visible_to(owner)) {
        evidence = Evidence::default();
    }
    Ok(build(id, &evidence))
}

pub(crate) async fn feedback_summary(
    state: &AppState,
    id: &str,
    owner: Option<&str>,
) -> Result<String, Response> {
    if !valid_id(id) {
        return Err(response(
            StatusCode::BAD_REQUEST,
            json!({"detail":"Invalid request ID"}),
        ));
    }
    let evidence = evidence(state, id).await.map_err(|_| {
        response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"detail":"Diagnostic evidence unavailable"}),
        )
    })?;
    if owner.is_some_and(|owner| !evidence.visible_to(owner))
        || (evidence.usage.is_none() && evidence.candidates.is_empty())
    {
        return Err(response(
            StatusCode::NOT_FOUND,
            json!({"detail":"Request unavailable"}),
        ));
    }
    Ok(build(id, &evidence).summary)
}

#[derive(Serialize)]
pub(crate) struct Diagnostic {
    schema_version: u8,
    request_id: String,
    state: &'static str,
    final_outcome: &'static str,
    has_attempt_failures: bool,
    stages: Vec<Stage>,
    summary: String,
}

#[derive(Serialize)]
struct Stage {
    id: &'static str,
    status: &'static str,
    code: &'static str,
    evidence: Vec<&'static str>,
}

fn build(id: &str, evidence: &Evidence) -> Diagnostic {
    let mut stages: Vec<_> = [
        "identity",
        "model_acl",
        "quota",
        "candidate_filter",
        "conversion",
        "tunnel",
        "upstream",
        "stream",
    ]
    .into_iter()
    .map(|id| Stage {
        id,
        status: "unknown",
        code: "not_recorded",
        evidence: vec![],
    })
    .collect();
    let mut mark = |index: usize, status, code, source| {
        let stage = &mut stages[index];
        if stage.status != "failed" || status == "failed" {
            stage.status = status;
            stage.code = code;
        }
        if !stage.evidence.contains(&source) {
            stage.evidence.push(source);
        }
    };
    let usage = evidence.usage.as_ref();
    let latest_attempt = evidence
        .candidates
        .iter()
        .filter(|c| c.started_at_unix_ms.is_some())
        .max_by_key(|c| (c.candidate_index, c.retry_index));
    let active = usage
        .map(|u| matches!(u.status.as_str(), "pending" | "streaming"))
        .unwrap_or_else(|| {
            latest_attempt.is_some_and(|c| {
                matches!(
                    c.status,
                    RequestCandidateStatus::Pending | RequestCandidateStatus::Streaming
                )
            })
        });
    let mut failed = usage.is_some_and(|u| {
        matches!(u.status.as_str(), "failed" | "cancelled" | "error")
            || u.status_code.is_some_and(|s| s >= 400)
    });
    if usage.is_some_and(|u| u.user_id.is_some())
        || evidence.candidates.iter().any(|c| c.user_id.is_some())
    {
        mark(
            0,
            "known",
            "owner_recorded",
            "usage.user_id / candidates.user_id",
        );
    }
    if !evidence.candidates.is_empty() {
        mark(3, "known", "candidates_recorded", "candidates.status");
    }
    if let Some(u) = usage {
        if u.routing_candidate_id().is_some() {
            mark(3, "known", "candidate_selected", "usage.candidate_id");
        }
        if u.has_format_conversion {
            mark(
                4,
                "known",
                "conversion_configured",
                "usage.has_format_conversion",
            );
        }
        if u.first_byte_time_ms.is_some() {
            mark(
                6,
                "known",
                "first_byte_recorded",
                "usage.first_byte_time_ms",
            );
        }
        if u.is_stream && u.status == "completed" && u.finalized_at_unix_secs.is_some() {
            mark(
                7,
                "known",
                "completion_recorded",
                "usage.status + finalized_at",
            );
        }
        if u.routing_failure_diagnostic()
            .and_then(|d| d.get("kind"))
            .and_then(Value::as_str)
            == Some("request_conversion")
        {
            mark(
                4,
                "failed",
                "format_conversion_failed",
                "usage.routing_failure_diagnostic.kind",
            );
        }
    }
    for c in &evidence.candidates {
        if c.status == RequestCandidateStatus::Skipped {
            mark(3, "failed", "candidate_rejected", "candidates.status");
        }
        if c.extra_data
            .as_ref()
            .and_then(|d| d.pointer("/failure_diagnostic/kind"))
            .and_then(Value::as_str)
            == Some("request_conversion")
        {
            mark(
                4,
                "failed",
                "format_conversion_failed",
                "candidates.extra_data.failure_diagnostic.kind",
            );
        }
    }
    let mut classify = |code: &str, source| {
        // Return only constants, not stored strings. Unknown errors never become guessed causes.
        let (index, safe_code) = match code {
            "plan_usage_limit_exceeded" => (2, "plan_usage_limit_exceeded"),
            "model_access_denied" | "model_not_allowed" => (1, "model_access_denied"),
            "candidate_list_empty" | "all_candidates_skipped" => (3, "no_eligible_candidates"),
            "provider_request_body_build_failed"
            | "provider_request_body_missing"
            | "request_conversion_error"
            | "response_conversion_error" => (4, "format_conversion_failed"),
            "proxy_error" => (5, "proxy_transport_failed"),
            "stream_missing_terminal_event"
            | "stream_terminal_error"
            | "stream_http_error"
            | "execution_runtime_stream_read_error"
            | "execution_runtime_stream_chunk_decode_error"
            | "execution_runtime_stream_frame_decode_error"
            | "execution_runtime_stream_rewrite_error"
            | "execution_runtime_stream_rewrite_flush_error"
            | "downstream_disconnect"
            | "client_delivery_failed"
            | "local_stream_candidate_watchdog_timeout" => (7, "stream_failed_or_interrupted"),
            "invalid_request_error" => (6, "invalid_request_error"),
            "protocol_error"
            | "upstream_response_decode_failed"
            | "invalid_provider_success_response" => (6, "upstream_format_error"),
            "connect_timeout" | "first_byte_timeout" | "read_timeout" | "tls_error" => {
                (6, "upstream_transport_error")
            }
            "authentication_error"
            | "permission_error"
            | "rate_limit_error"
            | "resource_exhausted"
            | "overloaded_error"
            | "not_found_error"
            | "api_error"
            | "server_error"
            | "upstream_error"
            | "upstream4xx"
            | "upstream5xx"
            | "retryable_upstream_status"
            | "execution_runtime_http_error"
            | "execution_runtime_stream_non_success_status" => (6, "upstream_error_recorded"),
            _ => return false,
        };
        mark(index, "failed", safe_code, source);
        true
    };
    if let Some(u) = usage {
        if let Some(code) = u.error_category.as_deref() {
            failed |= classify(code, "usage.error_category");
        }
        if let Some(code) = u
            .request_metadata
            .as_ref()
            .and_then(|m| m.get("transport_error_type"))
            .and_then(Value::as_str)
        {
            failed |= classify(code, "usage.request_metadata.transport_error_type");
        }
    }
    for c in &evidence.candidates {
        failed |= matches!(
            c.status,
            RequestCandidateStatus::Failed | RequestCandidateStatus::Cancelled
        );
        if let Some(code) = c
            .error_type
            .as_deref()
            .filter(|code| REQUEST_CANDIDATE_ERROR_TYPES.contains(code))
        {
            failed |= classify(code, "candidates.error_type");
        }
        if let Some(code) = c.skip_reason.as_deref() {
            failed |= classify(code, "candidates.skip_reason");
        }
    }
    failed |= stages.iter().any(|s| s.status == "failed");
    let has_attempt_failures = evidence.candidates.iter().any(|c| {
        matches!(
            c.status,
            RequestCandidateStatus::Failed
                | RequestCandidateStatus::Cancelled
                | RequestCandidateStatus::Skipped
        ) || c.error_type.is_some()
    });
    let final_outcome = match usage {
        Some(u) if matches!(u.status.as_str(), "pending" | "streaming") => "in_progress",
        Some(u) if u.status == "cancelled" => "cancelled",
        Some(u)
            if matches!(u.status.as_str(), "failed" | "error")
                || u.status_code.is_some_and(|s| s >= 400)
                || matches!(
                    u.error_category.as_deref(),
                    Some("stream_missing_terminal_event" | "stream_terminal_error")
                ) =>
        {
            "failed"
        }
        Some(u)
            if u.status == "completed"
                && u.finalized_at_unix_secs.is_some()
                && u.error_category.is_none()
                && u.status_code.is_some_and(|s| (200..300).contains(&s)) =>
        {
            "completed"
        }
        _ => "unknown",
    };
    let state = match final_outcome {
        "completed" => "known",
        "failed" | "cancelled" => "failed",
        "in_progress" => "in_progress",
        _ if usage.is_some() => "unknown",
        _ if active => "in_progress",
        _ if failed => "failed",
        _ => "unknown",
    };
    let summary = format!("RequestID: {id}\nDiagnostic v1: {state}; final_outcome: {final_outcome}; has_attempt_failures: {has_attempt_failures}\n{}", stages.iter().map(|s| format!("{}: {} ({})", s.id, s.status, s.code)).collect::<Vec<_>>().join("\n"));
    Diagnostic {
        schema_version: 1,
        request_id: id.into(),
        state,
        final_outcome,
        has_attempt_failures,
        stages,
        summary,
    }
}
