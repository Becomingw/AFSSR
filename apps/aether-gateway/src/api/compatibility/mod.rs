//! Offline-only protocol evidence. No transport, provider-key lookup or arbitrary payload input.
mod runner;
mod schema;
#[cfg(test)]
mod tests;

use crate::{control::GatewayPublicRequestContext, AppState, GatewayError};
use axum::{
    body::Bytes,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use schema::*;
use serde_json::{json, Value};

pub(crate) const CONFIG_KEY: &str = "compatibility_workbench_v1";

/// Validate before MCP preview/ticket issuance; never echo an arbitrary body into a preview.
pub(crate) fn validate_mcp_body(operation: &str, body: &Value) -> Result<(), &'static str> {
    if serde_json::to_vec(body)
        .map_err(|_| "Invalid compatibility body")?
        .len()
        > 8192
    {
        return Err("Compatibility body exceeds 8 KiB");
    }
    let revision = match operation {
        "arca_compatibility_profile_save" => {
            let input: SaveProfile = serde_json::from_value(body.clone())
                .map_err(|_| "Invalid compatibility profile schema")?;
            if input.profile.id.to_string().len()
                != body["profile"]["id"].as_str().unwrap_or_default().len()
            {
                return Err("Profile ID must be a canonical UUID");
            }
            input.profile.config.validate()?;
            input.expected_revision
        }
        "arca_compatibility_run" => {
            let input: RunInput =
                serde_json::from_value(body.clone()).map_err(|_| "Invalid offline run schema")?;
            if body["profile_id"].as_str().unwrap_or_default().len() != 36
                || input.expected_fingerprint.len() != 64
                || !input
                    .expected_fingerprint
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(
                    "Run requires a canonical UUID and exact lowercase SHA-256 fingerprint",
                );
            }
            input.expected_revision
        }
        _ => return Err("Unknown compatibility operation"),
    };
    if revision > 9_007_199_254_740_991 {
        return Err("Compatibility revision exceeds safe integer range");
    }
    Ok(())
}

fn response(status: StatusCode, value: Value) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}
fn error(status: StatusCode, detail: &str) -> Response {
    response(status, json!({"detail": detail}))
}

pub(crate) async fn admin_response(
    state: &AppState,
    context: &GatewayPublicRequestContext,
    body: Option<&Bytes>,
) -> Result<Option<Response>, GatewayError> {
    let Some(decision) = context
        .control_decision
        .as_ref()
        .filter(|d| d.route_family.as_deref() == Some("compatibility_manage"))
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
            "Compatibility permission denied",
        )));
    }
    if decision
        .public_query_string
        .as_deref()
        .is_some_and(|q| !q.is_empty())
    {
        return Ok(Some(error(
            StatusCode::BAD_REQUEST,
            "Compatibility endpoints do not accept query parameters",
        )));
    }
    let body = body.map(Bytes::as_ref).unwrap_or_default();
    Ok(Some(
        dispatch(
            state,
            &principal.user_role,
            decision.route_kind.as_deref().unwrap_or(""),
            body,
        )
        .await,
    ))
}

async fn read(state: &AppState) -> Result<(Option<Value>, Snapshot), Response> {
    let raw = state
        .read_system_config_json_value_strong(CONFIG_KEY)
        .await
        .map_err(|_| {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Compatibility storage unavailable",
            )
        })?;
    let snapshot = match raw.as_ref() {
        Some(v) => serde_json::from_value::<Snapshot>(v.clone()).map_err(|_| {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Unsupported compatibility schema; data was not overwritten",
            )
        })?,
        None => Snapshot::default(),
    };
    snapshot.validate().map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Invalid compatibility snapshot; data was not overwritten",
        )
    })?;
    Ok((raw, snapshot))
}

fn payload(snapshot: &Snapshot, role: &str) -> Value {
    let profiles: Vec<_> = snapshot
        .profiles
        .iter()
        .map(|p| {
            json!({
                "id":p.id, "config":p.config, "config_fingerprint":p.config.fingerprint(),
            })
        })
        .collect();
    let runs: Vec<_> = snapshot
        .runs
        .iter()
        .rev()
        .map(|run| {
            let stale = snapshot
                .profiles
                .iter()
                .find(|p| p.id == run.profile_id)
                .is_none_or(|p| p.config.fingerprint() != run.config_fingerprint);
            json!({"run":run, "stale":stale})
        })
        .collect();
    json!({"schema_version":1, "revision":snapshot.revision, "profiles":profiles, "runs":runs,
        "can_run":crate::roles::can_manage_services(role), "suite":SUITE,
        "fingerprint_scope":"offline_profile_and_converter_source_only",
        "real_model_status":"not_tested", "real_client_status":"not_tested"})
}

async fn dispatch(state: &AppState, role: &str, kind: &str, body: &[u8]) -> Response {
    if !crate::roles::can_access_admin_console(role) {
        return error(StatusCode::FORBIDDEN, "Administrator role required");
    }
    if kind != "compatibility_list" && !crate::roles::can_manage_services(role) {
        return error(
            StatusCode::FORBIDDEN,
            "Only service or super administrators may run or configure compatibility tests",
        );
    }
    if body.len() > 8192 {
        return error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Compatibility input exceeds 8 KiB",
        );
    }
    let (raw, mut snapshot) = match read(state).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match kind {
        "compatibility_list" => return response(StatusCode::OK, payload(&snapshot, role)),
        "compatibility_profile" => {
            let input = match serde_json::from_slice::<SaveProfile>(body) {
                Ok(v) => v,
                Err(_) => return error(
                    StatusCode::BAD_REQUEST,
                    "Invalid profile schema; arbitrary payloads and credentials are not accepted",
                ),
            };
            if input.expected_revision != snapshot.revision {
                return error(
                    StatusCode::CONFLICT,
                    "Compatibility revision changed; refresh before saving",
                );
            }
            if let Err(detail) = input.profile.config.validate() {
                return error(StatusCode::BAD_REQUEST, detail);
            }
            if let Some(p) = snapshot
                .profiles
                .iter_mut()
                .find(|p| p.id == input.profile.id)
            {
                *p = input.profile;
            } else if snapshot.profiles.len() < 64 {
                snapshot.profiles.push(input.profile);
            } else {
                return error(StatusCode::CONFLICT, "Profile capacity is 64");
            }
        }
        "compatibility_run" => {
            let input = match serde_json::from_slice::<RunInput>(body) {
                Ok(v) => v,
                Err(_) => return error(StatusCode::BAD_REQUEST, "Invalid offline run schema"),
            };
            if input.expected_revision != snapshot.revision {
                return error(
                    StatusCode::CONFLICT,
                    "Compatibility revision changed; refresh before running",
                );
            }
            let Some(profile) = snapshot.profiles.iter().find(|p| p.id == input.profile_id) else {
                return error(StatusCode::NOT_FOUND, "Compatibility profile not found");
            };
            if input.expected_fingerprint != profile.config.fingerprint() {
                return error(
                    StatusCode::CONFLICT,
                    "Configuration or converter changed; refresh before running",
                );
            }
            let run = runner::run(profile);
            if snapshot.runs.len() == 200 {
                snapshot.runs.remove(0);
            }
            snapshot.runs.push(run);
        }
        _ => return error(StatusCode::NOT_FOUND, "Compatibility route not found"),
    }
    let Some(revision) = snapshot.revision.checked_add(1) else {
        return error(StatusCode::CONFLICT, "Revision exhausted");
    };
    snapshot.revision = revision;
    if snapshot.validate().is_err() {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Generated compatibility result failed validation",
        );
    }
    let replacement = serde_json::to_value(&snapshot).expect("fixed snapshot serialization");
    match state
        .data
        .save_compatibility_snapshot(raw.as_ref(), &replacement)
        .await
    {
        Ok(true) => response(StatusCode::OK, payload(&snapshot, role)),
        Ok(false) => error(
            StatusCode::CONFLICT,
            "Compatibility snapshot changed concurrently; refresh and retry",
        ),
        Err(_) => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Compatibility persistence unavailable; no saved result was confirmed",
        ),
    }
}
