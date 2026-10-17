use crate::{control::GatewayPublicRequestContext, internal_access, AppState, GatewayError};
use axum::{
    body::Bytes,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewInput {
    user_id: String,
    #[serde(default)]
    api_key_id: Option<String>,
    #[serde(default)]
    config: Option<Value>,
}

fn response(status: StatusCode, value: Value) -> Response {
    let mut result = (status, Json(value)).into_response();
    result.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    result
}

pub(crate) async fn admin_response(
    state: &AppState,
    context: &GatewayPublicRequestContext,
    body: Option<&Bytes>,
) -> Result<Option<Response>, GatewayError> {
    let Some(decision) = context
        .control_decision
        .as_ref()
        .filter(|d| d.route_kind.as_deref() == Some("access_preview"))
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
            json!({"detail":"Entitlement preview permission denied"}),
        )));
    }
    let Some(body) = body.filter(|b| b.len() <= 256 * 1024) else {
        return Ok(Some(response(
            StatusCode::BAD_REQUEST,
            json!({"detail":"A bounded preview body is required"}),
        )));
    };
    let input = match serde_json::from_slice::<PreviewInput>(body) {
        Ok(input)
            if !input.user_id.is_empty()
                && input.user_id.len() <= 128
                && input
                    .api_key_id
                    .as_ref()
                    .is_none_or(|id| !id.is_empty() && id.len() <= 128) =>
        {
            input
        }
        _ => {
            return Ok(Some(response(
                StatusCode::BAD_REQUEST,
                json!({"detail":"Invalid entitlement preview input"}),
            )))
        }
    };
    match preview(state, input).await {
        Ok(Some(value)) => Ok(Some(response(StatusCode::OK, value))),
        Ok(None) => Ok(Some(response(
            StatusCode::NOT_FOUND,
            json!({"detail":"User or owned API key not found"}),
        ))),
        Err(PreviewError::Invalid) => Ok(Some(response(
            StatusCode::BAD_REQUEST,
            json!({"detail":"Invalid proposed entitlement configuration"}),
        ))),
        Err(PreviewError::Unavailable) => Ok(Some(response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"detail":"Effective entitlements temporarily unavailable"}),
        ))),
    }
}

enum PreviewError {
    Invalid,
    Unavailable,
}
impl From<GatewayError> for PreviewError {
    fn from(_: GatewayError) -> Self {
        Self::Unavailable
    }
}
impl From<aether_data_contracts::DataLayerError> for PreviewError {
    fn from(_: aether_data_contracts::DataLayerError) -> Self {
        Self::Unavailable
    }
}

async fn preview(state: &AppState, input: PreviewInput) -> Result<Option<Value>, PreviewError> {
    let Some(user) = state.find_user_auth_by_id(&input.user_id).await? else {
        return Ok(None);
    };
    let saved = internal_access::load(state).await?;
    let proposed = input
        .config
        .map(internal_access::parse)
        .transpose()
        .map_err(|_| PreviewError::Invalid)?;
    let memberships = state
        .list_user_group_memberships_by_user_ids(std::slice::from_ref(&user.id))
        .await?;
    let groups: Vec<_> = memberships.into_iter().map(|m| m.group_id).collect();
    let policies = state
        .data
        .resolve_user_effective_list_policies(&user)
        .await?;
    let key = if let Some(id) = input.api_key_id {
        let Some(key) = state
            .data
            .read_auth_api_key_snapshot_strong(
                &user.id,
                &id,
                chrono::Utc::now().timestamp().max(0) as u64,
            )
            .await?
        else {
            return Ok(None);
        };
        if key.user_id != user.id {
            return Ok(None);
        }
        Some(
            json!({"id":key.api_key_id,"name":key.api_key_name,"currently_usable":key.currently_usable,
            "allowed_models":key.effective_allowed_models(),"allowed_providers":key.effective_allowed_providers(),"allowed_api_formats":key.effective_allowed_api_formats(),
            "rate_limit":key.api_key_rate_limit,"concurrent_limit":key.api_key_concurrent_limit,"expires_at":key.api_key_expires_at_unix_secs}),
        )
    } else {
        None
    };
    let current = scenario(state, &user.id, &groups, &saved).await?;
    let proposed = if let Some(config) = proposed.as_ref() {
        Some(scenario(state, &user.id, &groups, config).await?)
    } else {
        None
    };
    let changed = proposed.as_ref().is_some_and(|v| {
        v["source"] != current["source"]
            || v["allocation"] != current["allocation"]
            || v["quota"]["timezone"] != current["quota"]["timezone"]
            || v["quota"]["windows"].as_array().map(|ws| {
                ws.iter()
                    .map(|w| (&w["family"], &w["models"]))
                    .collect::<Vec<_>>()
            }) != current["quota"]["windows"].as_array().map(|ws| {
                ws.iter()
                    .map(|w| (&w["family"], &w["models"]))
                    .collect::<Vec<_>>()
            })
    });
    Ok(Some(
        json!({"enabled":internal_access::enabled(),"user":{"id":user.id,"username":user.username,"is_active":user.is_active && !user.is_deleted},
        "model_access":{"source":if user.allowed_models_mode == "inherit" {"inherited"} else {"personal"},"mode":user.allowed_models_mode,"allowed_models":policies.allowed_models,
            "allowed_providers":policies.allowed_providers,"allowed_api_formats":policies.allowed_api_formats},
        "key":key,"current":current,"proposed":proposed,"changed":changed,
        "semantics":{"personal_replaces_groups":true,"family_pools_included_in_total":true,"overlapping_families":"all_matching_limits_apply","membership_usage":"current_members_history","preview_grants_access":false}}),
    ))
}

async fn scenario(
    state: &AppState,
    user: &str,
    groups: &[String],
    config: &internal_access::InternalAccessConfig,
) -> Result<Value, PreviewError> {
    let allocation = config.resolve(user, groups);
    let source = if config.users.contains_key(user) {
        "personal"
    } else if groups.iter().any(|g| config.groups.contains_key(g)) {
        "group"
    } else {
        "default"
    };
    let snapshot = super::account::snapshot_allocation(state, user, config, &allocation).await?;
    Ok(json!({"source":source,"group_ids":groups,"allocation":allocation,"quota":snapshot}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{classify_control_route, GatewayAdminPrincipalContext};
    #[test]
    fn preview_is_read_permission_not_an_account_mutation() {
        for (role, allowed) in [
            ("admin", true),
            ("audit_admin", true),
            ("user_admin", true),
            ("service_admin", false),
            ("user", false),
        ] {
            let mut d = classify_control_route(
                &axum::http::Method::POST,
                &"/api/admin/access-preview".parse().unwrap(),
                &axum::http::HeaderMap::new(),
            )
            .unwrap();
            d.admin_principal = Some(GatewayAdminPrincipalContext {
                user_id: "actor".into(),
                user_role: role.into(),
                session_id: None,
                management_token_id: Some("m".into()),
                management_token_permissions: Some(vec!["admin:users:read".into()]),
            });
            assert_eq!(
                crate::control::validate_admin_role_route_permission(&axum::http::Method::POST, &d)
                    .is_ok(),
                allowed,
                "{role}"
            );
            assert!(
                crate::control::validate_management_token_admin_route_permission(
                    &axum::http::Method::POST,
                    &d,
                    Some(&["admin:users:read".into()])
                )
                .is_ok()
            );
            assert!(
                crate::control::validate_management_token_admin_route_permission(
                    &axum::http::Method::POST,
                    &d,
                    Some(&["admin:users:write".into()])
                )
                .is_err()
            );
        }
        assert!(
            serde_json::from_value::<PreviewInput>(json!({"user_id":"u","password":"canary"}))
                .is_err()
        );
    }

    #[derive(Debug, Default)]
    struct Ledger(std::sync::Mutex<Vec<String>>);
    #[async_trait::async_trait]
    impl aether_data_contracts::repository::account_usage::AccountUsageRepository for Ledger {
        async fn read_windows(
            &self,
            user: &str,
            _: u64,
            windows: &[aether_data_contracts::repository::account_usage::AccountUsageWindow],
        ) -> Result<
            Vec<aether_data_contracts::repository::account_usage::AccountUsageAmounts>,
            aether_data_contracts::DataLayerError,
        > {
            self.0.lock().unwrap().push(user.into());
            Ok(windows
                .iter()
                .map(
                    |_| aether_data_contracts::repository::account_usage::AccountUsageAmounts {
                        finalized_units: 100_000_000,
                        reserved_units: 25_000_000,
                    },
                )
                .collect())
        }
    }

    #[tokio::test]
    async fn http_preview_receives_valid_body_and_does_not_mutate_saved_policy() {
        let original = json!({"default":{"weekly_budget_usd":20}});
        let state = AppState::new().unwrap().with_data_state_for_tests(
            crate::data::GatewayDataState::disabled()
                .with_account_usage_for_tests(std::sync::Arc::new(Ledger::default()))
                .with_system_config_values_for_tests([(
                    internal_access::CONFIG_KEY.into(),
                    original,
                )]),
        );
        let admin = state
            .create_local_auth_user_with_settings(
                None,
                true,
                "preview-admin".into(),
                "synthetic-hash".into(),
                "admin".into(),
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap()
            .unwrap();
        let headers = crate::api::mcp::test_session_headers(&state, &admin.id).await;
        let proposed = json!({"users":{admin.id.clone():{"unlimited":true}}});
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = crate::router::build_router_with_state(state);
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        for _ in 0..2 {
            let result = client
                .post(format!("http://{address}/api/admin/access-preview"))
                .headers(headers.clone())
                .json(&json!({"user_id":admin.id,"config":proposed}))
                .send()
                .await
                .unwrap();
            assert_eq!(result.status(), StatusCode::OK);
            let result: Value = result.json().await.unwrap();
            assert_eq!(result["proposed"]["allocation"]["unlimited"], true);
            assert_eq!(result["current"]["allocation"]["unlimited"], false);
            assert_eq!(result["changed"], true);
        }
        let oversized = client
            .post(format!("http://{address}/api/admin/access-preview"))
            .headers(headers)
            .body(vec![b' '; 256 * 1024 + 1])
            .send()
            .await
            .unwrap();
        assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
        task.abort();
    }

    #[tokio::test]
    async fn preview_uses_subject_ledger_and_personal_replacement_without_saving() {
        let ledger = std::sync::Arc::new(Ledger::default());
        let original = json!({"default":{"weekly_budget_usd":20}});
        let state = AppState::new().unwrap().with_data_state_for_tests(
            crate::data::GatewayDataState::disabled()
                .with_account_usage_for_tests(ledger.clone())
                .with_system_config_values_for_tests([(
                    internal_access::CONFIG_KEY.into(),
                    original.clone(),
                )]),
        );
        let user = state
            .create_local_auth_user_with_settings(
                None,
                true,
                "member".into(),
                "hash-canary".into(),
                "user".into(),
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap()
            .unwrap();
        let draft = json!({"model_families":{"f":{"name":"Family","models":["a","b"]}},"default":{"weekly_budget_usd":20},"users":{user.id.clone():{"monthly_budget_usd":5,"families":{"f":{"daily_budget_usd":2}}}}});
        let output = preview(
            &state,
            PreviewInput {
                user_id: user.id.clone(),
                api_key_id: None,
                config: Some(draft),
            },
        )
        .await
        .ok()
        .flatten()
        .unwrap();
        assert_eq!(output["current"]["source"], "default");
        assert_eq!(output["proposed"]["source"], "personal");
        assert_eq!(
            output["proposed"]["allocation"]["weekly_budget_usd"],
            Value::Null
        );
        let windows = output["proposed"]["quota"]["windows"].as_array().unwrap();
        let family = windows.iter().find(|w| w["family"] == "f").unwrap();
        assert_eq!(family["models"], json!(["a", "b"]));
        assert_eq!(family["remaining_usd"], 0.75);
        assert_eq!(output["changed"], true);
        assert!(ledger.0.lock().unwrap().iter().all(|id| id == &user.id));
        assert_eq!(
            state
                .read_system_config_json_value_strong(internal_access::CONFIG_KEY)
                .await
                .unwrap(),
            Some(original)
        );
        assert!(!output.to_string().contains("hash-canary"));
        assert!(preview(
            &state,
            PreviewInput {
                user_id: user.id,
                api_key_id: Some("foreign-key".into()),
                config: None
            }
        )
        .await
        .ok()
        .flatten()
        .is_none());
    }
}
