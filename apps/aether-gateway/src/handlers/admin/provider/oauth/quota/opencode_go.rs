use super::shared::{
    build_provider_quota_execution_plan, build_quota_snapshot_payload, execute_provider_quota_plan,
    persist_provider_quota_refresh_state, quota_refresh_success_invalid_state,
    resolve_provider_quota_execution_timeouts, ProviderQuotaExecutionOutcome,
};
use crate::handlers::admin::request::{AdminAppState, AdminGatewayProviderTransportSnapshot};
use crate::GatewayError;
use aether_admin::provider::opencode_go_quota::{
    build_opencode_go_usage_error_metadata, classify_opencode_go_usage_error,
    parse_opencode_go_usage_response, OpenCodeGoUsageErrorKind,
};
use aether_admin::provider::redaction::admin_provider_metadata_bucket_safe_json;
use aether_contracts::ProxySnapshot;
use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogEndpoint, StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
use aether_provider_pool::{build_opencode_go_pool_quota_request, ProviderPoolQuotaRequestSpec};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

const OPENCODE_GO_BUCKET: &str = "opencode_go";
const OPENCODE_GO_AUTH_INVALID_REASON: &str = "OpenCode Go API key is invalid or revoked";

async fn execute_opencode_go_usage_plan(
    state: &AdminAppState<'_>,
    transport: &AdminGatewayProviderTransportSnapshot,
    spec: ProviderPoolQuotaRequestSpec,
    proxy_override: Option<&ProxySnapshot>,
) -> Result<ProviderQuotaExecutionOutcome, GatewayError> {
    let proxy = match proxy_override {
        Some(proxy) => Some(proxy.clone()),
        None => {
            state
                .resolve_transport_proxy_snapshot_with_tunnel_affinity(transport)
                .await
        }
    };
    let timeouts = Some(resolve_provider_quota_execution_timeouts(
        state.resolve_transport_execution_timeouts(transport),
        proxy.as_ref(),
    ));
    let plan = build_provider_quota_execution_plan(
        transport,
        spec,
        proxy,
        state.resolve_transport_profile(transport),
        timeouts,
    );
    execute_provider_quota_plan(state, transport, plan, "opencode_go:usage").await
}

/// Merge a patch into the existing `opencode_go` metadata bucket so that a
/// transient error keeps the last observed windows.
fn build_opencode_go_metadata_update(
    current_upstream_metadata: Option<&serde_json::Value>,
    patch: serde_json::Value,
    replace_windows: bool,
) -> serde_json::Value {
    let mut merged = current_upstream_metadata
        .and_then(|value| value.get(OPENCODE_GO_BUCKET))
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    if replace_windows {
        // A successful read is authoritative: clear stale error flags.
        for field in [
            "last_error",
            "last_error_status",
            "auth_invalid",
            "no_subscription",
        ] {
            merged.remove(field);
        }
    }
    if let Some(patch) = patch.as_object() {
        for (key, value) in patch {
            merged.insert(key.clone(), value.clone());
        }
    }
    let merged = serde_json::Value::Object(merged);
    json!({
        OPENCODE_GO_BUCKET: admin_provider_metadata_bucket_safe_json(OPENCODE_GO_BUCKET, Some(&merged)),
    })
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub(crate) async fn refresh_opencode_go_provider_quota_locally(
    state: &AdminAppState<'_>,
    provider: &StoredProviderCatalogProvider,
    endpoint: &StoredProviderCatalogEndpoint,
    keys: Vec<StoredProviderCatalogKey>,
    proxy_override: Option<ProxySnapshot>,
) -> Result<Option<serde_json::Value>, GatewayError> {
    let mut results = Vec::new();
    let mut success_count = 0usize;
    let mut failed_count = 0usize;

    for key in keys {
        let Some(transport) = state
            .read_provider_transport_snapshot(&provider.id, &endpoint.id, &key.id)
            .await?
        else {
            failed_count += 1;
            results.push(json!({
                "key_id": key.id,
                "key_name": key.name,
                "status": "error",
                "message": "Provider transport snapshot unavailable",
            }));
            continue;
        };

        let api_key = transport.key.decrypted_api_key.trim();
        if api_key.is_empty() {
            failed_count += 1;
            results.push(json!({
                "key_id": key.id,
                "key_name": key.name,
                "status": "error",
                "message": "缺少 OpenCode Go API Key",
            }));
            continue;
        }
        let Some(spec) =
            build_opencode_go_pool_quota_request(&key.id, &transport.endpoint.base_url, api_key)
        else {
            failed_count += 1;
            results.push(json!({
                "key_id": key.id,
                "key_name": key.name,
                "status": "error",
                "message": "OpenCode Go 端点地址无效",
            }));
            continue;
        };

        let result =
            match execute_opencode_go_usage_plan(state, &transport, spec, proxy_override.as_ref())
                .await?
            {
                ProviderQuotaExecutionOutcome::Response(result) => result,
                ProviderQuotaExecutionOutcome::Failure(_) => {
                    failed_count += 1;
                    results.push(json!({
                        "key_id": key.id,
                        "key_name": key.name,
                        "status": "error",
                        "message": "OpenCode Go usage 请求执行失败",
                        "status_code": 502,
                    }));
                    continue;
                }
            };

        let now = now_unix_secs();
        let (mut oauth_invalid_at_unix_secs, mut oauth_invalid_reason) =
            quota_refresh_success_invalid_state(&key);
        let mut message = None::<String>;
        let (status, metadata_update) = if result.status_code == 200 {
            match result
                .body
                .as_ref()
                .and_then(|body| body.json_body.as_ref())
                .and_then(|body| parse_opencode_go_usage_response(body, now))
            {
                Some(parsed) => {
                    // A readable usage response proves the key is valid.
                    if oauth_invalid_reason.as_deref() == Some(OPENCODE_GO_AUTH_INVALID_REASON) {
                        oauth_invalid_at_unix_secs = None;
                        oauth_invalid_reason = None;
                    }
                    (
                        "success",
                        Some(build_opencode_go_metadata_update(
                            key.upstream_metadata.as_ref(),
                            parsed,
                            true,
                        )),
                    )
                }
                None => {
                    message = Some("无法解析 OpenCode Go usage 响应".to_string());
                    ("no_metadata", None)
                }
            }
        } else {
            let kind = classify_opencode_go_usage_error(result.status_code);
            message = Some(format!(
                "OpenCode Go usage 返回状态码 {}",
                result.status_code
            ));
            let status = match kind {
                OpenCodeGoUsageErrorKind::AuthInvalid => {
                    oauth_invalid_at_unix_secs = Some(now);
                    oauth_invalid_reason = Some(OPENCODE_GO_AUTH_INVALID_REASON.to_string());
                    "auth_invalid"
                }
                OpenCodeGoUsageErrorKind::NoSubscription => "no_subscription",
                OpenCodeGoUsageErrorKind::RateLimited => "rate_limited",
                OpenCodeGoUsageErrorKind::Other => "error",
            };
            (
                status,
                Some(build_opencode_go_metadata_update(
                    key.upstream_metadata.as_ref(),
                    build_opencode_go_usage_error_metadata(result.status_code, now),
                    false,
                )),
            )
        };

        if !persist_provider_quota_refresh_state(
            state,
            &key.id,
            metadata_update.as_ref(),
            oauth_invalid_at_unix_secs,
            oauth_invalid_reason,
            None,
        )
        .await?
        {
            failed_count += 1;
            results.push(json!({
                "key_id": key.id,
                "key_name": key.name,
                "status": "error",
                "message": "Key 状态写入失败",
            }));
            continue;
        }

        if status == "success" {
            success_count += 1;
        } else {
            failed_count += 1;
        }

        let mut payload = serde_json::Map::new();
        payload.insert("key_id".to_string(), json!(key.id));
        payload.insert("key_name".to_string(), json!(key.name));
        payload.insert("status".to_string(), json!(status));
        if let Some(message) = message {
            payload.insert("message".to_string(), json!(message));
        }
        if result.status_code != 200 {
            payload.insert("status_code".to_string(), json!(result.status_code));
        }
        if let Some(metadata) = metadata_update
            .as_ref()
            .and_then(|value| value.get(OPENCODE_GO_BUCKET))
        {
            payload.insert("metadata".to_string(), metadata.clone());
        }
        if let Some(quota_snapshot) = build_quota_snapshot_payload(
            OPENCODE_GO_BUCKET,
            key.status_snapshot.as_ref(),
            metadata_update.as_ref(),
        ) {
            payload.insert("quota_snapshot".to_string(), quota_snapshot);
        }
        results.push(serde_json::Value::Object(payload));
    }

    Ok(Some(json!({
        "success": success_count,
        "failed": failed_count,
        "total": success_count + failed_count,
        "results": results,
        "message": format!("已处理 {} 个 Key", success_count + failed_count),
        "auto_removed": 0,
    })))
}

#[cfg(test)]
mod tests {
    use super::build_opencode_go_metadata_update;
    use serde_json::json;

    #[test]
    fn success_update_clears_previous_error_flags() {
        let update = build_opencode_go_metadata_update(
            Some(&json!({
                "opencode_go": {
                    "last_error": "HTTP 403",
                    "no_subscription": true,
                    "updated_at": 1u64
                }
            })),
            json!({
                "updated_at": 2u64,
                "windows": {"rolling": {"percent": 10.0, "limited": false}},
                "rate_limited": false
            }),
            true,
        );
        assert!(update.pointer("/opencode_go/no_subscription").is_none());
        assert!(update.pointer("/opencode_go/last_error").is_none());
        assert_eq!(
            update.pointer("/opencode_go/updated_at"),
            Some(&json!(2u64))
        );
    }

    #[test]
    fn error_update_preserves_last_observed_windows() {
        let update = build_opencode_go_metadata_update(
            Some(&json!({
                "opencode_go": {
                    "updated_at": 1u64,
                    "windows": {"weekly": {"percent": 100.0, "limited": true, "reset_at": 9u64}}
                }
            })),
            json!({"updated_at": 2u64, "last_error": "HTTP 500", "no_subscription": false}),
            false,
        );
        assert_eq!(
            update.pointer("/opencode_go/windows/weekly/limited"),
            Some(&json!(true))
        );
        assert!(update.pointer("/opencode_go/last_error").is_some());
        assert_eq!(
            update.pointer("/opencode_go/no_subscription"),
            Some(&json!(false))
        );
    }
}
