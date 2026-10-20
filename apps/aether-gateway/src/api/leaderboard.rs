use crate::{control::GatewayPublicRequestContext, AppState, GatewayError};
use aether_admin::observability::{
    leaderboard::{build_report, LeaderboardWindow},
    stats::AdminStatsTimeRange,
};
use aether_data_contracts::repository::usage::{
    UsageActivityQuery, UsageLeaderboardGroupBy, UsageLeaderboardQuery,
};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;

#[cfg(test)]
mod tests;

pub(crate) fn mount_leaderboard_routes(router: Router<AppState>) -> Router<AppState> {
    router.route("/api/leaderboard", get(user_report))
}
fn response(status: StatusCode, value: serde_json::Value) -> Response {
    let mut r = (status, Json(value)).into_response();
    r.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    r
}
async fn user_report(State(state): State<AppState>, request: Request) -> Response {
    let (parts, _) = request.into_parts();
    if parts
        .headers
        .get_all(axum::http::header::AUTHORIZATION)
        .iter()
        .count()
        > 1
    {
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
    if let Err(r) =
        crate::handlers::public::resolve_authenticated_local_user(&state, &context, &parts.headers)
            .await
    {
        return r;
    }
    // The user endpoint always uses the public view (anonymity opt-outs honoured, no
    // amounts or exact times), including for administrator sessions.
    match report(&state, parts.uri.query(), true).await {
        Ok(r) => r,
        Err(_) => response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"detail":"Leaderboard temporarily unavailable; retry or select a shorter range"}),
        ),
    }
}

pub(crate) async fn report(
    state: &AppState,
    query: Option<&str>,
    masked: bool,
) -> Result<Response, GatewayError> {
    let mut params = std::collections::BTreeMap::new();
    for (k, v) in url::form_urlencoded::parse(query.unwrap_or("").as_bytes()) {
        if !matches!(k.as_ref(), "preset" | "tz_offset_minutes")
            || params.insert(k.into_owned(), v.into_owned()).is_some()
        {
            return Ok(response(
                StatusCode::BAD_REQUEST,
                json!({"detail":"Unknown or duplicate leaderboard filter"}),
            ));
        }
    }
    let preset = params
        .get("preset")
        .map(String::as_str)
        .unwrap_or("this_week");
    if !matches!(
        preset,
        "this_week" | "last_week" | "last7days" | "last30days"
    ) {
        return Ok(response(
            StatusCode::BAD_REQUEST,
            json!({"detail":"Supported ranges: this_week, last_week, last7days, last30days"}),
        ));
    }
    let offset = params
        .get("tz_offset_minutes")
        .map(|v| v.parse::<i32>())
        .transpose();
    let offset = match offset {
        Ok(v) if v.is_none_or(|v| (-720..=840).contains(&v) && v % 15 == 0) => v.unwrap_or(480),
        _ => {
            return Ok(response(
                StatusCode::BAD_REQUEST,
                json!({"detail":"Invalid UTC offset"}),
            ))
        }
    };
    let normalized = format!("preset={preset}&tz_offset_minutes={offset}");
    let range = AdminStatsTimeRange::resolve_optional(Some(&normalized))
        .map_err(GatewayError::Internal)?
        .expect("preset has a range");
    let Some((from, until)) = range.to_unix_bounds() else {
        return Ok(response(
            StatusCode::BAD_REQUEST,
            json!({"detail":"Invalid range"}),
        ));
    };
    if !state.has_usage_data_reader() {
        return Ok(response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"detail":"Usage storage is not configured"}),
        ));
    }
    let activity = state
        .summarize_usage_activity(&UsageActivityQuery {
            from,
            until: until + 6 * 3600,
            tz_offset_minutes: offset,
        })
        .await?;
    let users = state
        .summarize_usage_leaderboard(&UsageLeaderboardQuery {
            created_from_unix_secs: from,
            created_until_unix_secs: until,
            group_by: UsageLeaderboardGroupBy::User,
            user_id: None,
            provider_name: None,
            model: None,
        })
        .await?;
    let models = state
        .summarize_usage_leaderboard(&UsageLeaderboardQuery {
            created_from_unix_secs: from,
            created_until_unix_secs: until,
            group_by: UsageLeaderboardGroupBy::Model,
            user_id: None,
            provider_name: None,
            model: None,
        })
        .await?;
    let mut names = std::collections::BTreeMap::new();
    let ids: Vec<_> = users
        .iter()
        .map(|u| u.group_key.clone())
        .chain(activity.iter().map(|r| r.user_id.clone()))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if masked {
        // Public view (#2): show usernames by default, except for users who opted
        // into anonymity in their preferences, and deleted/inactive accounts.
        for user in state.list_users_by_ids(&ids).await? {
            if user.is_deleted || !user.is_active {
                continue;
            }
            let anonymous = state
                .read_user_preferences(&user.id)
                .await?
                .is_some_and(|p| p.leaderboard_anonymous);
            if !anonymous {
                names.insert(user.id, user.username);
            }
        }
    } else {
        for user in state.list_users_by_ids(&ids).await? {
            names.insert(user.id, user.username);
        }
        for u in &users {
            if let Some(name) = &u.legacy_name {
                names.entry(u.group_key.clone()).or_insert(name.clone());
            }
        }
    }
    let anchor = (until as i64 - 1 + i64::from(offset) * 60).div_euclid(86400);
    let start = (from as i64 + i64::from(offset) * 60).div_euclid(86400);
    let mut value = serde_json::to_value(build_report(
        &users,
        &models,
        &activity,
        &names,
        LeaderboardWindow {
            start_day: start,
            end_day: anchor,
            offset_minutes: offset,
        },
        masked,
    ))
    .map_err(|e| GatewayError::Internal(e.to_string()))?;
    value["start_date"] = json!(range.start_date.to_string());
    value["end_date"] = json!(range.end_date.to_string());
    value["tz_offset_minutes"] = json!(offset);
    value["generated_at"] = json!(chrono::Utc::now().timestamp());
    value["activity_observed_from"] = json!(activity.iter().map(|r| r.day).min());
    Ok(response(StatusCode::OK, value))
}
