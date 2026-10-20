use std::collections::{BTreeMap, BTreeSet};

use aether_admin::provider::{
    pool as admin_provider_pool_pure, status as admin_provider_status_pure,
};
use aether_data_contracts::repository::candidates::StoredRequestCandidate;
use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;
use aether_scheduler_core::{
    auth_api_key_concurrency_limit_reached, build_provider_concurrent_limit_map,
    candidate_is_selectable_with_runtime_state, candidate_runtime_skip_reason_with_state,
    effective_provider_key_rpm_limit, CandidateRuntimeSelectabilityInput,
};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::data::auth::GatewayAuthApiKeySnapshot;
use crate::GatewayError;

use super::{SchedulerMinimalCandidateSelectionCandidate, SchedulerRuntimeState};

pub(super) use aether_scheduler_core::should_skip_provider_quota;

pub(super) struct CandidateRuntimeSelectionSnapshot {
    pub(super) recent_candidates: Vec<StoredRequestCandidate>,
    pub(super) provider_concurrent_limits: BTreeMap<String, usize>,
    pub(super) provider_key_rpm_states: BTreeMap<String, StoredProviderCatalogKey>,
    pub(super) pool_provider_ids: BTreeSet<String>,
    provider_quota_blocks_requests: BTreeMap<String, bool>,
    key_account_quota_exhausted: BTreeMap<String, bool>,
    key_oauth_invalid: BTreeMap<String, bool>,
    provider_key_rpm_reset_ats: BTreeMap<String, Option<u64>>,
}

pub(super) async fn read_candidate_runtime_selection_snapshot(
    state: &(impl SchedulerRuntimeState + ?Sized),
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    now_unix_secs: u64,
) -> Result<CandidateRuntimeSelectionSnapshot, GatewayError> {
    let provider_concurrent_limits = read_provider_concurrent_limits(state, candidates).await?;
    let provider_pool_state = read_provider_pool_state_map(state, candidates).await?;
    let provider_skip_exhausted_accounts = provider_pool_state
        .iter()
        .map(|(provider_id, state)| (provider_id.clone(), state.skip_exhausted_accounts))
        .collect::<BTreeMap<_, _>>();
    let pool_provider_ids = provider_pool_state
        .iter()
        .filter_map(|(provider_id, state)| state.pool_enabled.then_some(provider_id.clone()))
        .collect::<BTreeSet<_>>();
    let provider_key_rpm_states = read_provider_key_rpm_states(state, candidates).await?;
    let recent_candidates = if runtime_snapshot_requires_recent_candidates(
        auth_snapshot,
        &provider_concurrent_limits,
        &provider_key_rpm_states,
        now_unix_secs,
    ) {
        state.read_recent_request_candidates(128).await?
    } else {
        Vec::new()
    };
    let key_account_quota_exhausted = read_key_account_quota_exhaustion_map(
        candidates,
        &provider_key_rpm_states,
        &provider_skip_exhausted_accounts,
    );
    let key_oauth_invalid =
        read_key_oauth_invalid_map(candidates, &provider_key_rpm_states, now_unix_secs);
    let provider_quota_blocks_requests =
        read_provider_quota_block_map(state, candidates, now_unix_secs).await?;
    let provider_key_rpm_reset_ats =
        read_provider_key_rpm_reset_at_map(state, candidates, now_unix_secs);

    Ok(CandidateRuntimeSelectionSnapshot {
        recent_candidates,
        provider_concurrent_limits,
        provider_key_rpm_states,
        pool_provider_ids,
        provider_quota_blocks_requests,
        key_account_quota_exhausted,
        key_oauth_invalid,
        provider_key_rpm_reset_ats,
    })
}

fn runtime_snapshot_requires_recent_candidates(
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    provider_concurrent_limits: &BTreeMap<String, usize>,
    provider_key_rpm_states: &BTreeMap<String, StoredProviderCatalogKey>,
    now_unix_secs: u64,
) -> bool {
    if auth_snapshot
        .and_then(|snapshot| snapshot.api_key_concurrent_limit)
        .is_some_and(|limit| limit > 0)
    {
        return true;
    }

    if provider_concurrent_limits.values().any(|limit| *limit > 0) {
        return true;
    }

    provider_key_rpm_states.values().any(|key| {
        key.concurrent_limit.is_some_and(|limit| limit > 0)
            || effective_provider_key_rpm_limit(key, now_unix_secs).is_some()
    })
}

pub(super) fn auth_snapshot_concurrency_limit_reached(
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    snapshot: &CandidateRuntimeSelectionSnapshot,
    now_unix_secs: u64,
) -> bool {
    auth_snapshot
        .and_then(|snapshot| {
            usize::try_from(snapshot.api_key_concurrent_limit?)
                .ok()
                .and_then(|limit| {
                    if limit == 0 {
                        return None;
                    }
                    Some((snapshot.api_key_id.as_str(), limit))
                })
        })
        .is_some_and(|(api_key_id, limit)| {
            auth_api_key_concurrency_limit_reached(
                &snapshot.recent_candidates,
                now_unix_secs,
                api_key_id,
                limit,
            )
        })
}

pub(super) fn is_candidate_selectable(
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
    snapshot: &CandidateRuntimeSelectionSnapshot,
    now_unix_secs: u64,
) -> bool {
    let pool_group = snapshot
        .pool_provider_ids
        .contains(candidate.provider_id.as_str());
    candidate_is_selectable_with_runtime_state(CandidateRuntimeSelectabilityInput {
        candidate,
        recent_candidates: &snapshot.recent_candidates,
        provider_concurrent_limits: &snapshot.provider_concurrent_limits,
        provider_key_rpm_states: &snapshot.provider_key_rpm_states,
        now_unix_secs,
        provider_quota_blocks_requests: snapshot
            .provider_quota_blocks_requests
            .get(candidate.provider_id.as_str())
            .copied()
            .unwrap_or(false),
        account_quota_exhausted: !pool_group
            && snapshot
                .key_account_quota_exhausted
                .get(candidate.key_id.as_str())
                .copied()
                .unwrap_or(false),
        oauth_invalid: !pool_group
            && snapshot
                .key_oauth_invalid
                .get(candidate.key_id.as_str())
                .copied()
                .unwrap_or(false),
        enforce_key_circuit_breaker: !pool_group,
        rpm_reset_at: (!pool_group)
            .then(|| {
                snapshot
                    .provider_key_rpm_reset_ats
                    .get(candidate.key_id.as_str())
                    .copied()
                    .flatten()
            })
            .flatten(),
    })
}

pub(super) fn current_candidate_runtime_skip_reason(
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
    snapshot: &CandidateRuntimeSelectionSnapshot,
    now_unix_secs: u64,
) -> Option<&'static str> {
    let pool_group = snapshot
        .pool_provider_ids
        .contains(candidate.provider_id.as_str());
    let provider_quota_blocks_requests = snapshot
        .provider_quota_blocks_requests
        .get(candidate.provider_id.as_str())
        .copied()
        .unwrap_or(false);
    let rpm_reset_at = (!pool_group)
        .then(|| {
            snapshot
                .provider_key_rpm_reset_ats
                .get(candidate.key_id.as_str())
                .copied()
                .flatten()
        })
        .flatten();

    candidate_runtime_skip_reason_with_state(CandidateRuntimeSelectabilityInput {
        candidate,
        recent_candidates: &snapshot.recent_candidates,
        provider_concurrent_limits: &snapshot.provider_concurrent_limits,
        provider_key_rpm_states: &snapshot.provider_key_rpm_states,
        now_unix_secs,
        provider_quota_blocks_requests,
        account_quota_exhausted: !pool_group
            && snapshot
                .key_account_quota_exhausted
                .get(candidate.key_id.as_str())
                .copied()
                .unwrap_or(false),
        oauth_invalid: !pool_group
            && snapshot
                .key_oauth_invalid
                .get(candidate.key_id.as_str())
                .copied()
                .unwrap_or(false),
        enforce_key_circuit_breaker: !pool_group,
        rpm_reset_at,
    })
}

pub(super) async fn read_provider_concurrent_limits(
    state: &(impl SchedulerRuntimeState + ?Sized),
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
) -> Result<BTreeMap<String, usize>, GatewayError> {
    let provider_ids = candidates
        .iter()
        .map(|candidate| candidate.provider_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if provider_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let providers = state
        .read_provider_catalog_providers_by_ids(&provider_ids)
        .await?;
    Ok(build_provider_concurrent_limit_map(providers))
}

pub(super) async fn read_provider_key_rpm_states(
    state: &(impl SchedulerRuntimeState + ?Sized),
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
) -> Result<BTreeMap<String, StoredProviderCatalogKey>, GatewayError> {
    let key_ids = candidates
        .iter()
        .map(|candidate| candidate.key_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if key_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let keys = state.read_provider_catalog_keys_by_ids(&key_ids).await?;
    Ok(keys
        .into_iter()
        .map(|key| (key.id.clone(), key))
        .collect::<BTreeMap<_, _>>())
}

async fn read_provider_quota_block_map(
    state: &(impl SchedulerRuntimeState + ?Sized),
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
    now_unix_secs: u64,
) -> Result<BTreeMap<String, bool>, GatewayError> {
    let provider_ids = candidates
        .iter()
        .map(|candidate| candidate.provider_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut quota_blocks = BTreeMap::new();

    for provider_id in provider_ids {
        let blocks_requests = state
            .read_provider_quota_snapshot(&provider_id)
            .await?
            .as_ref()
            .is_some_and(|quota| should_skip_provider_quota(quota, now_unix_secs));
        quota_blocks.insert(provider_id, blocks_requests);
    }

    Ok(quota_blocks)
}

#[derive(Debug, Clone, Copy, Default)]
struct ProviderPoolState {
    pool_enabled: bool,
    skip_exhausted_accounts: bool,
}

async fn read_provider_pool_state_map(
    state: &(impl SchedulerRuntimeState + ?Sized),
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
) -> Result<BTreeMap<String, ProviderPoolState>, GatewayError> {
    let provider_ids = candidates
        .iter()
        .map(|candidate| candidate.provider_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if provider_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let providers = state
        .read_provider_catalog_providers_by_ids(&provider_ids)
        .await?;
    Ok(providers
        .into_iter()
        .map(|provider| {
            let pool_advanced = provider
                .config
                .as_ref()
                .and_then(|value| value.get("pool_advanced"));
            let skip_exhausted_accounts =
                provider_skip_exhausted_accounts(pool_advanced, provider.provider_type.as_str());
            (
                provider.id,
                ProviderPoolState {
                    pool_enabled: pool_advanced.is_some(),
                    skip_exhausted_accounts,
                },
            )
        })
        .collect())
}

/// Effective skip-exhausted flag: `pool_advanced.skip_exhausted_accounts`
/// when configured.  Without an
/// account pool, provider types whose quota snapshot is refreshed outside the
/// pool (OpenCode Go) skip exhausted keys by default so that normal
/// scheduling stops sending requests to a key until its window resets.
fn provider_skip_exhausted_accounts(
    pool_advanced: Option<&serde_json::Value>,
    provider_type: &str,
) -> bool {
    match pool_advanced {
        Some(pool_advanced) => pool_advanced
            .as_object()
            .and_then(|value| value.get("skip_exhausted_accounts"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        None => aether_provider_pool::ProviderPoolService::with_builtin_adapters()
            .skip_exhausted_accounts_without_pool(provider_type),
    }
}

fn read_key_account_quota_exhaustion_map(
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
    provider_key_rpm_states: &BTreeMap<String, StoredProviderCatalogKey>,
    provider_skip_exhausted_accounts: &BTreeMap<String, bool>,
) -> BTreeMap<String, bool> {
    candidates
        .iter()
        .map(|candidate| {
            let exhausted = provider_key_rpm_states
                .get(candidate.key_id.as_str())
                .is_some_and(|key| {
                    let account_exhausted =
                        admin_provider_pool_pure::admin_pool_key_model_quota_exhausted(
                            key,
                            candidate.provider_type.as_str(),
                            candidate.selected_provider_model_name.as_str(),
                        )
                        .unwrap_or_else(|| {
                            admin_provider_pool_pure::admin_pool_key_account_quota_exhausted(
                                key,
                                candidate.provider_type.as_str(),
                            )
                        });
                    let hard_blocked =
                        admin_provider_pool_pure::admin_pool_key_model_quota_hard_blocked(
                            key,
                            candidate.provider_type.as_str(),
                            candidate.selected_provider_model_name.as_str(),
                        );
                    let skip_configured = provider_skip_exhausted_accounts
                        .get(candidate.provider_id.as_str())
                        .copied()
                        .unwrap_or(false);
                    hard_blocked || (skip_configured && account_exhausted)
                });
            (candidate.key_id.clone(), exhausted)
        })
        .collect()
}

fn read_key_oauth_invalid_map(
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
    provider_key_rpm_states: &BTreeMap<String, StoredProviderCatalogKey>,
    now_unix_secs: u64,
) -> BTreeMap<String, bool> {
    candidates
        .iter()
        .map(|candidate| {
            let oauth_invalid = provider_key_rpm_states
                .get(candidate.key_id.as_str())
                .is_some_and(|key| {
                    key_requires_oauth_reauth(key, candidate.provider_type.as_str(), now_unix_secs)
                });
            (candidate.key_id.clone(), oauth_invalid)
        })
        .collect()
}

fn key_requires_oauth_reauth(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
    now_unix_secs: u64,
) -> bool {
    if !key.auth_type.trim().eq_ignore_ascii_case("oauth") {
        return false;
    }

    let invalid_reason = key
        .oauth_invalid_reason
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    if !invalid_reason.is_empty() {
        return oauth_invalid_reason_blocks_scheduling(
            key,
            provider_type,
            invalid_reason,
            now_unix_secs,
        );
    }

    false
}

fn oauth_invalid_reason_blocks_scheduling(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
    invalid_reason: &str,
    now_unix_secs: u64,
) -> bool {
    let trimmed_reason = invalid_reason.trim();

    let account_state = admin_provider_status_pure::resolve_pool_account_state(
        Some(provider_type),
        key.upstream_metadata.as_ref(),
        Some(trimmed_reason),
    );
    if account_state.blocked
        && !account_state.recoverable
        && account_state
            .code
            .as_deref()
            .is_some_and(oauth_account_state_code_is_hard_block)
    {
        return true;
    }

    if oauth_invalid_reason_has_tag(trimmed_reason, "[REFRESH_FAILED]") {
        return oauth_access_token_expired(key, now_unix_secs);
    }

    false
}

fn oauth_invalid_reason_has_tag(reason: &str, tag: &str) -> bool {
    reason
        .lines()
        .map(str::trim)
        .any(|line| line.starts_with(tag))
}

fn oauth_access_token_expired(key: &StoredProviderCatalogKey, now_unix_secs: u64) -> bool {
    let now_unix_secs = if now_unix_secs == 0 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_secs())
            .unwrap_or(0)
    } else {
        now_unix_secs
    };
    key.expires_at_unix_secs
        .is_none_or(|expires_at| expires_at == 0 || expires_at <= now_unix_secs)
}

fn oauth_account_state_code_is_hard_block(code: &str) -> bool {
    matches!(
        code.trim().to_ascii_lowercase().as_str(),
        "account_banned"
            | "account_suspended"
            | "account_disabled"
            | "workspace_deactivated"
            | "account_forbidden"
            | "account_blocked"
            | "account_verification"
            | "oauth_token_invalid"
    )
}

fn read_provider_key_rpm_reset_at_map(
    state: &(impl SchedulerRuntimeState + ?Sized),
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
    now_unix_secs: u64,
) -> BTreeMap<String, Option<u64>> {
    candidates
        .iter()
        .map(|candidate| {
            (
                candidate.key_id.clone(),
                state.provider_key_rpm_reset_at(candidate.key_id.as_str(), now_unix_secs),
            )
        })
        .collect::<BTreeMap<_, _>>()
}

#[cfg(test)]
mod opencode_go_quota_tests {
    use super::*;
    use serde_json::json;

    fn candidate(provider_type: &str) -> SchedulerMinimalCandidateSelectionCandidate {
        SchedulerMinimalCandidateSelectionCandidate {
            provider_id: "provider-1".to_string(),
            provider_name: "OpenCode Go".to_string(),
            provider_type: provider_type.to_string(),
            provider_priority: 0,
            endpoint_id: "endpoint-1".to_string(),
            endpoint_api_format: "openai:chat".to_string(),
            key_id: "key-1".to_string(),
            key_name: "key".to_string(),
            key_auth_type: "api_key".to_string(),
            key_internal_priority: 10,
            key_global_priority_for_format: None,
            key_capabilities: None,
            model_id: "model-1".to_string(),
            global_model_id: "global-1".to_string(),
            global_model_name: "glm-5.1".to_string(),
            selected_provider_model_name: "glm-5.1".to_string(),
            supports_streaming: true,
            mapping_matched_model: None,
        }
    }

    fn exhausted_key(reset_at: u64) -> StoredProviderCatalogKey {
        let mut key = StoredProviderCatalogKey::new(
            "key-1".to_string(),
            "provider-1".to_string(),
            "key".to_string(),
            "api_key".to_string(),
            None,
            true,
        )
        .expect("key should build");
        key.status_snapshot = Some(json!({
            "quota": {
                "provider_type": "opencode_go",
                "code": "exhausted",
                "exhausted": true,
                "windows": [
                    {"code": "5h", "scope": "account", "used_ratio": 1.0, "is_exhausted": true, "reset_at": reset_at},
                    {"code": "weekly", "scope": "account", "used_ratio": 0.3, "is_exhausted": false}
                ]
            }
        }));
        key
    }

    fn far_future() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600
    }

    #[test]
    fn non_pool_opencode_go_skips_exhausted_keys_by_default() {
        assert!(provider_skip_exhausted_accounts(None, "opencode_go"));
        assert!(!provider_skip_exhausted_accounts(None, "codex"));
        assert!(!provider_skip_exhausted_accounts(None, "custom"));

        let candidates = vec![candidate("opencode_go")];
        let keys = BTreeMap::from([("key-1".to_string(), exhausted_key(far_future()))]);
        let skip = BTreeMap::from([(
            "provider-1".to_string(),
            provider_skip_exhausted_accounts(None, "opencode_go"),
        )]);
        let map = read_key_account_quota_exhaustion_map(&candidates, &keys, &skip);
        assert_eq!(map.get("key-1"), Some(&true));
    }

    #[test]
    fn non_pool_opencode_go_key_becomes_selectable_after_reset_at() {
        let candidates = vec![candidate("opencode_go")];
        let keys = BTreeMap::from([("key-1".to_string(), exhausted_key(1_000_000_000))]);
        let skip = BTreeMap::from([("provider-1".to_string(), true)]);
        let map = read_key_account_quota_exhaustion_map(&candidates, &keys, &skip);
        assert_eq!(map.get("key-1"), Some(&false));
    }

    #[test]
    fn pool_mode_honors_configured_skip_exhausted_accounts() {
        let enabled = json!({"skip_exhausted_accounts": true});
        let disabled =
            json!({"scheduling_presets": [{"preset": "quota_balanced", "enabled": true}]});
        assert!(provider_skip_exhausted_accounts(
            Some(&enabled),
            "opencode_go"
        ));
        assert!(!provider_skip_exhausted_accounts(
            Some(&disabled),
            "opencode_go"
        ));
    }
}
