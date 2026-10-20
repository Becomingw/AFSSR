use std::collections::BTreeMap;

use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogEndpoint;
use serde_json::Value;

use crate::capability::ProviderPoolCapabilities;
use crate::provider::{
    provider_pool_matching_endpoint, ProviderPoolAdapter, ProviderPoolMemberInput,
};
use crate::quota::provider_pool_member_quota_snapshot;
use crate::quota_refresh::ProviderPoolQuotaRequestSpec;

pub const OPENCODE_GO_PROVIDER_TYPE: &str = "opencode_go";
pub const OPENCODE_GO_DEFAULT_ORIGIN: &str = "https://opencode.ai";
pub const OPENCODE_GO_USAGE_PATH: &str = "/zen/go/v1/usage";

/// Provider pool behavior for OpenCode Go subscription keys.
///
/// OpenCode Go keys carry three independent quota windows (5h rolling,
/// weekly, monthly). They are plain API keys, so quota refresh works without
/// OAuth and must also run when the provider is not configured as an account
/// pool (`pool_advanced` absent).
#[derive(Debug, Clone, Default)]
pub struct OpenCodeGoProviderPoolAdapter;

impl ProviderPoolAdapter for OpenCodeGoProviderPoolAdapter {
    fn provider_type(&self) -> &'static str {
        OPENCODE_GO_PROVIDER_TYPE
    }

    fn capabilities(&self) -> ProviderPoolCapabilities {
        ProviderPoolCapabilities {
            plan_tier: false,
            quota_reset: true,
            quota_refresh: true,
        }
    }

    fn quota_refresh_endpoint(
        &self,
        endpoints: &[StoredProviderCatalogEndpoint],
        include_inactive: bool,
    ) -> Option<StoredProviderCatalogEndpoint> {
        provider_pool_matching_endpoint(endpoints, include_inactive, |endpoint| {
            opencode_go_usage_url_for_base_url(&endpoint.base_url).is_some()
        })
    }

    fn quota_refresh_missing_endpoint_message(&self) -> String {
        "找不到有效的 OpenCode Go 端点".to_string()
    }

    fn quota_refresh_without_pool(&self) -> bool {
        true
    }

    fn skip_exhausted_accounts_without_pool(&self) -> bool {
        true
    }

    fn quota_hard_blocked(&self, input: &ProviderPoolMemberInput<'_>) -> bool {
        provider_pool_member_quota_snapshot(input.key, input.provider_type)
            .and_then(|quota| quota.get("code"))
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|code| code.eq_ignore_ascii_case("no_subscription"))
    }
}

/// Resolve the usage URL from an endpoint base URL such as
/// `https://opencode.ai/inference/go/v1`. Only the origin is kept; the usage
/// path is fixed by OpenCode. Non-HTTPS or credential-bearing URLs are refused.
pub fn opencode_go_usage_url_for_base_url(base_url: &str) -> Option<String> {
    let base_url = base_url.trim();
    let origin = if base_url.is_empty() {
        OPENCODE_GO_DEFAULT_ORIGIN.to_string()
    } else {
        let url = url::Url::parse(base_url).ok()?;
        if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
            return None;
        }
        let host = url.host_str()?;
        match url.port() {
            Some(port) => format!("https://{host}:{port}"),
            None => format!("https://{host}"),
        }
    };
    Some(format!("{origin}{OPENCODE_GO_USAGE_PATH}"))
}

pub fn build_opencode_go_pool_quota_request(
    key_id: &str,
    base_url: &str,
    api_key: &str,
) -> Option<ProviderPoolQuotaRequestSpec> {
    let url = opencode_go_usage_url_for_base_url(base_url)?;
    let mut headers = BTreeMap::new();
    headers.insert(
        "authorization".to_string(),
        format!("Bearer {}", api_key.trim()),
    );
    headers.insert("accept".to_string(), "application/json".to_string());
    Some(ProviderPoolQuotaRequestSpec {
        request_id: format!("opencode-go-quota:{key_id}"),
        provider_name: OPENCODE_GO_PROVIDER_TYPE.to_string(),
        quota_kind: OPENCODE_GO_PROVIDER_TYPE.to_string(),
        method: "GET".to_string(),
        url,
        headers,
        content_type: None,
        json_body: None,
        client_api_format: "openai:chat".to_string(),
        provider_api_format: "opencode_go:usage".to_string(),
        model_name: Some("opencode-go-usage".to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;
    use serde_json::json;

    fn key_with_quota(quota: Value) -> StoredProviderCatalogKey {
        let mut key = StoredProviderCatalogKey::new(
            "key-1".to_string(),
            "provider-1".to_string(),
            "key-1".to_string(),
            "api_key".to_string(),
            None,
            true,
        )
        .expect("key should build");
        key.status_snapshot = Some(json!({ "quota": quota }));
        key
    }

    #[test]
    fn usage_url_keeps_only_origin_of_endpoint_base_url() {
        assert_eq!(
            opencode_go_usage_url_for_base_url("https://opencode.ai/inference/go/v1").as_deref(),
            Some("https://opencode.ai/zen/go/v1/usage")
        );
        assert_eq!(
            opencode_go_usage_url_for_base_url("").as_deref(),
            Some("https://opencode.ai/zen/go/v1/usage")
        );
        assert_eq!(
            opencode_go_usage_url_for_base_url("http://opencode.ai/x"),
            None
        );
        assert_eq!(
            opencode_go_usage_url_for_base_url("https://u:p@opencode.ai/x"),
            None
        );
    }

    #[test]
    fn quota_request_uses_bearer_get_without_body() {
        let spec = build_opencode_go_pool_quota_request(
            "k1",
            "https://opencode.ai/inference/go/v1",
            " sk-test ",
        )
        .expect("spec");
        assert_eq!(spec.method, "GET");
        assert_eq!(spec.url, "https://opencode.ai/zen/go/v1/usage");
        assert_eq!(
            spec.headers.get("authorization").map(String::as_str),
            Some("Bearer sk-test")
        );
        assert!(spec.json_body.is_none());
        assert!(!format!("{spec:?}").contains("sk-test"));
    }

    #[test]
    fn adapter_runs_without_pool_and_skips_exhausted_by_default() {
        let adapter = OpenCodeGoProviderPoolAdapter;
        assert!(adapter.supports_quota_refresh());
        assert!(adapter.quota_refresh_without_pool());
        assert!(adapter.skip_exhausted_accounts_without_pool());
    }

    #[test]
    fn adapter_treats_exhausted_window_until_reset_as_exhausted() {
        let adapter = OpenCodeGoProviderPoolAdapter;
        let future = crate::quota::provider_pool_current_unix_secs().unwrap() + 3600;
        let key = key_with_quota(json!({
            "provider_type": "opencode_go",
            "code": "exhausted",
            "exhausted": true,
            "windows": [
                {"code": "5h", "scope": "account", "used_ratio": 1.0, "is_exhausted": true, "reset_at": future},
                {"code": "weekly", "scope": "account", "used_ratio": 0.4, "is_exhausted": false}
            ]
        }));
        let input = ProviderPoolMemberInput {
            provider_type: "opencode_go",
            key: &key,
            auth_config: None,
            provider_model_name: None,
        };
        assert!(adapter.quota_exhausted(&input));
        assert!(!adapter.quota_hard_blocked(&input));

        let elapsed = key_with_quota(json!({
            "provider_type": "opencode_go",
            "code": "exhausted",
            "exhausted": true,
            "windows": [
                {"code": "5h", "scope": "account", "used_ratio": 1.0, "is_exhausted": true, "reset_at": 1_000_000_000u64}
            ]
        }));
        let input = ProviderPoolMemberInput {
            provider_type: "opencode_go",
            key: &elapsed,
            auth_config: None,
            provider_model_name: None,
        };
        assert!(!adapter.quota_exhausted(&input));
    }

    #[test]
    fn adapter_hard_blocks_keys_without_go_subscription() {
        let adapter = OpenCodeGoProviderPoolAdapter;
        let key = key_with_quota(json!({
            "provider_type": "opencode_go",
            "code": "no_subscription",
            "exhausted": true,
        }));
        let input = ProviderPoolMemberInput {
            provider_type: "opencode_go",
            key: &key,
            auth_config: None,
            provider_model_name: None,
        };
        assert!(adapter.quota_hard_blocked(&input));
    }
}
