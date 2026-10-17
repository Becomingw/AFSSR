use std::collections::BTreeMap;

use crate::{
    control::GatewayPublicRequestContext,
    internal_access::{self, Allocation, InternalAccessConfig},
    AppState, GatewayError,
};
use aether_data_contracts::repository::{
    account_usage::AccountUsageWindow,
    billing::{UsagePolicyWindow, USAGE_POLICY_COST_UNITS_PER_USD},
};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;

pub(crate) fn mount_account_routes(router: Router<AppState>) -> Router<AppState> {
    router.route("/api/users/me/entitlements", get(entitlements))
}

fn response(status: StatusCode, value: serde_json::Value) -> Response {
    let mut result = (status, Json(value)).into_response();
    result.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    result
}

async fn entitlements(State(state): State<AppState>, request: Request) -> Response {
    serve_entitlements(state, request, internal_access::enabled()).await
}

async fn serve_entitlements(state: AppState, request: Request, enabled: bool) -> Response {
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
    let user = match crate::handlers::public::resolve_authenticated_local_user(
        &state,
        &context,
        &parts.headers,
    )
    .await
    {
        Ok(auth) => auth.user.id,
        Err(r) => return r,
    };
    // No caller-provided subject or timezone can change the scope of this endpoint.
    if parts.uri.query().is_some() {
        return response(
            StatusCode::BAD_REQUEST,
            json!({"detail":"This endpoint accepts no filters"}),
        );
    }
    if !enabled {
        return response(StatusCode::OK, json!({"enabled":false}));
    }
    match snapshot(&state, &user).await {
        Ok(value) => response(StatusCode::OK, value),
        Err(_) => response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"detail":"Account entitlements temporarily unavailable"}),
        ),
    }
}

struct PoolWindow {
    query: AccountUsageWindow,
    family: Option<String>,
    period: &'static str,
    limit: Option<f64>,
}

fn plan(
    allocation: &Allocation,
    config: &InternalAccessConfig,
    now: u64,
) -> Result<Vec<PoolWindow>, GatewayError> {
    let timezone = &config.timezone;
    let mut windows: BTreeMap<
        (Option<String>, Option<String>, &'static str),
        (UsagePolicyWindow, Option<f64>, Option<Vec<String>>),
    > = BTreeMap::new();
    for (name, window) in [
        ("five_hour", UsagePolicyWindow::Rolling { seconds: 18_000 }),
        (
            "weekly",
            UsagePolicyWindow::CalendarWeek {
                timezone: Some(timezone.into()),
                week_start: 1,
            },
        ),
        (
            "monthly",
            UsagePolicyWindow::CalendarMonth {
                timezone: Some(timezone.into()),
            },
        ),
    ] {
        windows.insert((None, None, name), (window, None, None));
    }
    for scoped in config
        .scoped_policies(allocation)
        .map_err(GatewayError::Internal)?
    {
        for rule in scoped.policy.rules {
            let period = match &rule.window {
                UsagePolicyWindow::Rolling { .. } => "five_hour",
                UsagePolicyWindow::CalendarDay { .. } => "daily",
                UsagePolicyWindow::CalendarWeek { .. } => "weekly",
                UsagePolicyWindow::CalendarMonth { .. } => "monthly",
                _ => return Err(GatewayError::Internal("Unsupported account window".into())),
            };
            windows
                .entry((scoped.model.clone(), scoped.family.clone(), period))
                .and_modify(|(_, limit, _)| {
                    *limit = Some(limit.map_or(rule.limit, |old| old.min(rule.limit)));
                })
                .or_insert((rule.window, Some(rule.limit), scoped.models.clone()));
        }
    }
    windows
        .into_iter()
        .map(|((model, family, period), (window, limit, models))| {
            let (from, until, _, _) =
                crate::plan_usage_policy::usage_window_bounds(&window, None, now)?;
            Ok(PoolWindow {
                query: AccountUsageWindow {
                    model,
                    models,
                    from,
                    until,
                },
                family,
                period,
                limit,
            })
        })
        .collect()
}

pub(crate) async fn snapshot(
    state: &AppState,
    user: &str,
) -> Result<serde_json::Value, GatewayError> {
    let (config, allocation) = internal_access::allocation(state, user).await?;
    snapshot_allocation(state, user, &config, &allocation).await
}

pub(crate) async fn snapshot_allocation(
    state: &AppState,
    user: &str,
    config: &InternalAccessConfig,
    allocation: &Allocation,
) -> Result<serde_json::Value, GatewayError> {
    let repo = state
        .data
        .account_usage_repository()
        .ok_or_else(|| GatewayError::Internal("Quota storage unavailable".into()))?;
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let windows = plan(allocation, config, now)?;
    let queries: Vec<_> = windows.iter().map(|w| w.query.clone()).collect();
    let amounts = repo
        .read_windows(user, now, &queries)
        .await
        .map_err(|_| GatewayError::Internal("Quota read failed".into()))?;
    if amounts.len() != windows.len() {
        return Err(GatewayError::Internal("Incomplete quota snapshot".into()));
    }
    let unit = USAGE_POLICY_COST_UNITS_PER_USD as f64;
    let values: Vec<_> = windows.iter().zip(amounts).map(|(w, a)| {
        let used = a.finalized_units as f64 / unit;
        let reserved = a.reserved_units as f64 / unit;
        json!({"model":w.query.model,"family":w.family,"family_name":w.family.as_ref().and_then(|id|config.model_families.get(id)).map(|family|&family.name),"models":w.query.models,"period":w.period,"limit_usd":w.limit,"used_usd":used,"reserved_usd":reserved,
            "remaining_usd":w.limit.map(|limit| (limit-used-reserved).max(0.0)),"starts_at":w.query.from,"ends_at":w.query.until,
            "resets_at":(w.period!="five_hour").then_some(w.query.until)})
    }).collect();
    Ok(
        json!({"enabled":true,"generated_at":now,"timezone":config.timezone,
        "source":if config.users.contains_key(user){"personal"}else{"inherited"},
        "unlimited":windows.iter().all(|w| w.limit.is_none()),"windows":values}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn account_windows_match_enforcement_and_merge_wildcard() {
        let config=internal_access::parse(json!({"timezone":"Asia/Shanghai","default":{"weekly_budget_usd":100,"models":{"*":{"weekly_budget_usd":80},"fabel":{"daily_budget_usd":5}}},"users":{"individual":{"monthly_budget_usd":12}}})).unwrap();
        let now = 1_792_152_000;
        let windows = plan(&config.resolve("member", &[]), &config, now).unwrap();
        assert_eq!(windows.len(), 4);
        assert_eq!(
            windows.iter().find(|w| w.period == "weekly").unwrap().limit,
            Some(80.0)
        );
        let rolling = windows.iter().find(|w| w.period == "five_hour").unwrap();
        assert_eq!(
            (rolling.query.from, rolling.query.until),
            (now - 18_000, now + 1)
        );
        let model = windows.iter().find(|w| w.query.model.is_some()).unwrap();
        assert_eq!(model.query.model.as_deref(), Some("fabel"));
        assert_eq!(model.period, "daily");
        let individual = plan(&config.resolve("individual", &[]), &config, now).unwrap();
        assert_eq!(individual.len(), 3);
        assert_eq!(individual.iter().filter(|w| w.limit.is_some()).count(), 1);
        assert_eq!(
            individual
                .iter()
                .find(|w| w.period == "monthly")
                .unwrap()
                .limit,
            Some(12.0)
        );
    }

    #[derive(Debug, Default)]
    struct OwnLedger(std::sync::Mutex<Vec<String>>);
    #[async_trait::async_trait]
    impl aether_data_contracts::repository::account_usage::AccountUsageRepository for OwnLedger {
        async fn read_windows(
            &self,
            user: &str,
            _: u64,
            windows: &[AccountUsageWindow],
        ) -> Result<
            Vec<aether_data_contracts::repository::account_usage::AccountUsageAmounts>,
            aether_data_contracts::DataLayerError,
        > {
            self.0.lock().unwrap().push(user.into());
            Ok(windows
                .iter()
                .map(
                    |_| aether_data_contracts::repository::account_usage::AccountUsageAmounts {
                        finalized_units: 125_000_000,
                        reserved_units: 25_000_000,
                    },
                )
                .collect())
        }
    }

    #[tokio::test]
    async fn personal_endpoint_is_session_scoped_for_every_role_and_fails_closed() {
        let ledger = std::sync::Arc::new(OwnLedger::default());
        let state = AppState::new().unwrap().with_data_state_for_tests(
            crate::data::GatewayDataState::disabled()
                .with_account_usage_for_tests(ledger.clone())
                .with_system_config_values_for_tests([(
                    internal_access::CONFIG_KEY.to_string(),
                    json!({"timezone":"Asia/Shanghai","default":{"monthly_budget_usd":10}}),
                )]),
        );
        let mut sessions = Vec::new();
        for role in [
            "user",
            "admin",
            "audit_admin",
            "service_admin",
            "user_admin",
        ] {
            let user = state
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
            let headers = crate::api::mcp::test_session_headers(&state, &user.id).await;
            sessions.push((user.id, headers));
        }
        let router = Router::new()
            .route(
                "/api/users/me/entitlements",
                get(|State(state): State<AppState>, request: Request| {
                    serve_entitlements(state, request, true)
                }),
            )
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap()
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let url = format!("http://{address}/api/users/me/entitlements");
        assert_eq!(
            client.get(&url).send().await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        for (id, headers) in &sessions {
            let response = client
                .get(&url)
                .headers(headers.clone())
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["cache-control"], "no-store");
            let body: serde_json::Value = response.json().await.unwrap();
            let month = body["windows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|w| w["period"] == "monthly")
                .unwrap();
            assert_eq!(month["used_usd"], 1.25);
            assert_eq!(month["reserved_usd"], 0.25);
            assert_eq!(month["remaining_usd"], 8.5);
            assert_eq!(ledger.0.lock().unwrap().last().unwrap(), id);
            assert!(!body.to_string().contains("example.test"));
            assert_eq!(
                client
                    .get(format!("{url}?user_id=another-user"))
                    .headers(headers.clone())
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
            );
            assert_eq!(
                client
                    .post(&url)
                    .headers(headers.clone())
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::METHOD_NOT_ALLOWED
            );
        }
        assert_eq!(ledger.0.lock().unwrap().len(), sessions.len());
        task.abort();
        let missing = AppState::new().unwrap();
        assert!(snapshot(&missing, "user").await.is_err());
    }
}
