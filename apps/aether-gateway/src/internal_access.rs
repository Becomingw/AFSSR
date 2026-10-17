use std::collections::BTreeMap;

use aether_data_contracts::repository::billing::{
    UsagePolicyEntitlement, UsagePolicyEntitlementType, UsagePolicyMetric, UsagePolicyRule,
    UsagePolicyWindow,
};
use serde::{Deserialize, Serialize};

use crate::{AppState, GatewayError};

pub(crate) const CONFIG_KEY: &str = "internal_access_policy";

pub(crate) fn enabled() -> bool {
    std::env::var("AFSSR_INTERNAL_MODE")
        .map(|value| value != "false" && value != "0")
        .unwrap_or(!cfg!(test))
}

pub(crate) fn commercial_path(path: &str) -> bool {
    if path.starts_with("/api/admin/users/") && path.contains("/billing/") {
        return true;
    }
    [
        "/api/admin/wallets",
        "/api/admin/billing/plans",
        "/api/admin/billing/presets",
        "/api/admin/payments",
        "/api/admin/redeem",
        "/api/admin/referral",
        "/api/wallet",
        "/api/billing",
        "/api/payment",
        "/api/referral",
        "/api/users/me/wallet",
        "/api/users/me/billing",
        "/api/users/me/referral",
    ]
    .iter()
    .any(|prefix| {
        path == *prefix
            || path.starts_with(&format!("{prefix}/"))
            || path.starts_with(&format!("{prefix}-"))
    })
}

pub(crate) fn retired_generation_path(path: &str) -> bool {
    path.starts_with("/v1beta/")
        || path.starts_with("/v1alpha/")
        || (path.starts_with("/v1/models/")
            && (path.contains(":generateContent") || path.contains(":streamGenerateContent")))
}

pub(crate) async fn quota_model(
    state: &AppState,
    decision: &crate::control::GatewayControlDecision,
    requested: Option<&str>,
) -> Result<Option<String>, GatewayError> {
    let Some(requested) = requested else {
        return Ok(None);
    };
    let directive = decision.model_directive_policy.resolve_reasoning(
        decision
            .auth_endpoint_signature
            .as_deref()
            .unwrap_or("openai:responses"),
        Some(requested),
    );
    let base = directive.base_model().unwrap_or(requested);
    Ok(Some(
        state
            .get_public_global_model_by_name(base)
            .await?
            .map(|m| m.name)
            .unwrap_or_else(|| base.to_string()),
    ))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InternalAccessConfig {
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default)]
    pub default: Option<Allocation>,
    #[serde(default)]
    pub groups: BTreeMap<String, Allocation>,
    #[serde(default)]
    pub users: BTreeMap<String, Allocation>,
    #[serde(default)]
    pub model_families: BTreeMap<String, ModelFamily>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelFamily {
    pub name: String,
    pub models: Vec<String>,
}

pub(crate) struct ScopedPolicy {
    pub model: Option<String>,
    pub models: Option<Vec<String>>,
    pub family: Option<String>,
    pub policy: UsagePolicyEntitlement,
}

fn default_timezone() -> String {
    "Asia/Shanghai".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Allocation {
    #[serde(default)]
    pub unlimited: bool,
    #[serde(default)]
    pub five_hour_budget_usd: Option<f64>,
    #[serde(default)]
    pub weekly_budget_usd: Option<f64>,
    #[serde(default)]
    pub monthly_budget_usd: Option<f64>,
    #[serde(default)]
    pub models: BTreeMap<String, ModelAllowance>,
    #[serde(default)]
    pub families: BTreeMap<String, ModelAllowance>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelAllowance {
    #[serde(default)]
    pub five_hour_budget_usd: Option<f64>,
    #[serde(default)]
    pub daily_budget_usd: Option<f64>,
    #[serde(default)]
    pub weekly_budget_usd: Option<f64>,
    #[serde(default)]
    pub monthly_budget_usd: Option<f64>,
}

pub(crate) fn parse(value: serde_json::Value) -> Result<InternalAccessConfig, String> {
    let config: InternalAccessConfig = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if config.timezone.parse::<chrono_tz::Tz>().is_err() {
        return Err("timezone must be an IANA timezone".to_string());
    }
    if config.groups.len() + config.users.len() > 10_000 {
        return Err("too many internal allocations".to_string());
    }
    if config.model_families.len() > 32 {
        return Err("at most 32 model families are supported".into());
    }
    for (id, family) in &config.model_families {
        if id.is_empty()
            || id.len() > 64
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || family.name.trim().is_empty()
            || family.name.len() > 128
            || family.models.is_empty()
            || family.models.len() > 256
            || family.models.iter().enumerate().any(|(i, model)| {
                model.trim().is_empty()
                    || model == "*"
                    || model.len() > 128
                    || model.trim() != model
                    || family.models[..i].contains(model)
            })
        {
            return Err("invalid model family: use a stable ID, display name and unique exact public model names".into());
        }
    }
    for allocation in config
        .default
        .iter()
        .chain(config.groups.values())
        .chain(config.users.values())
    {
        if allocation.unlimited
            && (allocation.five_hour_budget_usd.is_some()
                || allocation.weekly_budget_usd.is_some()
                || allocation.monthly_budget_usd.is_some()
                || !allocation.models.is_empty()
                || !allocation.families.is_empty())
        {
            return Err(
                "unlimited cannot be combined with budgets or model allowances".to_string(),
            );
        }
        if allocation.models.len() + allocation.families.len() > 15 {
            return Err("an allocation supports at most 15 model/family allowances".to_string());
        }
        for model in allocation.models.keys() {
            if model.trim().is_empty() || model.len() > 128 || model.trim() != model {
                return Err("model name must be a non-empty exact public model name".to_string());
            }
        }
        for scoped in config.scoped_policies(allocation)? {
            scoped.policy.validate().map_err(|e| e.to_string())?;
        }
    }
    Ok(config)
}

impl Allocation {
    pub(crate) fn policies(
        &self,
        timezone: &str,
    ) -> Result<Vec<(Option<String>, UsagePolicyEntitlement)>, String> {
        let mut result = Vec::new();
        let total = ModelAllowance {
            five_hour_budget_usd: self.five_hour_budget_usd,
            weekly_budget_usd: self.weekly_budget_usd,
            monthly_budget_usd: self.monthly_budget_usd,
            ..Default::default()
        };
        let rules = total.rules(timezone);
        if !rules.is_empty() {
            result.push((None, policy(rules)));
        }
        for (model, allowance) in &self.models {
            let rules = allowance.rules(timezone);
            if !rules.is_empty() {
                result.push(((model != "*").then(|| model.clone()), policy(rules)));
            }
        }
        Ok(result)
    }
}

impl ModelAllowance {
    fn rules(&self, timezone: &str) -> Vec<UsagePolicyRule> {
        let windows = [
            (
                self.five_hour_budget_usd,
                UsagePolicyWindow::Rolling { seconds: 18_000 },
            ),
            (
                self.daily_budget_usd,
                UsagePolicyWindow::CalendarDay {
                    timezone: Some(timezone.to_string()),
                },
            ),
            (
                self.weekly_budget_usd,
                UsagePolicyWindow::CalendarWeek {
                    timezone: Some(timezone.to_string()),
                    week_start: 1,
                },
            ),
            (
                self.monthly_budget_usd,
                UsagePolicyWindow::CalendarMonth {
                    timezone: Some(timezone.to_string()),
                },
            ),
        ];
        let mut rules = Vec::new();
        for (limit, window) in windows {
            if let Some(limit) = limit {
                rules.push(UsagePolicyRule {
                    metric: UsagePolicyMetric::ActualCostUsd,
                    window,
                    limit,
                    enforcement: Default::default(),
                });
            }
        }
        rules
    }
}

fn policy(rules: Vec<UsagePolicyRule>) -> UsagePolicyEntitlement {
    UsagePolicyEntitlement {
        entitlement_type: UsagePolicyEntitlementType::UsagePolicy,
        policy_id: None,
        name: None,
        replacement_group: None,
        rules,
    }
}

// Groups grant optional sub-pools by union; the most generous limit wins for each shared
// model/window. A personal allocation replaces those benefits, including reductions.
fn generous<T: PartialOrd + Copy>(a: Option<T>, b: Option<T>) -> Option<T> {
    match (a, b) {
        (Some(a), Some(b)) => Some(if a > b { a } else { b }),
        _ => None,
    }
}

fn max_explicit(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

impl InternalAccessConfig {
    pub(crate) fn scoped_policies(
        &self,
        allocation: &Allocation,
    ) -> Result<Vec<ScopedPolicy>, String> {
        if allocation.unlimited {
            return Ok(Vec::new());
        }
        let mut result = allocation
            .policies(&self.timezone)?
            .into_iter()
            .map(|(model, policy)| ScopedPolicy {
                model,
                models: None,
                family: None,
                policy,
            })
            .collect::<Vec<_>>();
        for (id, allowance) in &allocation.families {
            let definition = self
                .model_families
                .get(id)
                .ok_or_else(|| format!("unknown model family: {id}"))?;
            let rules = allowance.rules(&self.timezone);
            if !rules.is_empty() {
                result.push(ScopedPolicy {
                    model: None,
                    models: Some(definition.models.clone()),
                    family: Some(id.clone()),
                    policy: policy(rules),
                });
            }
        }
        if result.iter().map(|s| s.policy.rules.len()).sum::<usize>() > 64 {
            return Err("effective allocation exceeds 64 quota windows".into());
        }
        Ok(result)
    }

    pub(crate) fn resolve(&self, user: &str, groups: &[String]) -> Allocation {
        if let Some(allocation) = self.users.get(user) {
            return allocation.clone();
        }
        let mut granted = groups.iter().filter_map(|id| self.groups.get(id));
        let Some(mut result) = granted.next().cloned() else {
            return self.default.clone().unwrap_or_default();
        };
        for allocation in granted {
            if result.unlimited || allocation.unlimited {
                return Allocation {
                    unlimited: true,
                    ..Default::default()
                };
            }
            result.five_hour_budget_usd =
                max_explicit(result.five_hour_budget_usd, allocation.five_hour_budget_usd);
            result.weekly_budget_usd =
                max_explicit(result.weekly_budget_usd, allocation.weekly_budget_usd);
            result.monthly_budget_usd =
                max_explicit(result.monthly_budget_usd, allocation.monthly_budget_usd);
            for (model, quota) in &allocation.models {
                result
                    .models
                    .entry(model.clone())
                    .and_modify(|current| {
                        current.five_hour_budget_usd =
                            generous(current.five_hour_budget_usd, quota.five_hour_budget_usd);
                        current.daily_budget_usd =
                            generous(current.daily_budget_usd, quota.daily_budget_usd);
                        current.weekly_budget_usd =
                            generous(current.weekly_budget_usd, quota.weekly_budget_usd);
                        current.monthly_budget_usd =
                            generous(current.monthly_budget_usd, quota.monthly_budget_usd);
                    })
                    .or_insert_with(|| quota.clone());
            }
            for (family, quota) in &allocation.families {
                result
                    .families
                    .entry(family.clone())
                    .and_modify(|current| {
                        current.five_hour_budget_usd =
                            generous(current.five_hour_budget_usd, quota.five_hour_budget_usd);
                        current.daily_budget_usd =
                            generous(current.daily_budget_usd, quota.daily_budget_usd);
                        current.weekly_budget_usd =
                            generous(current.weekly_budget_usd, quota.weekly_budget_usd);
                        current.monthly_budget_usd =
                            generous(current.monthly_budget_usd, quota.monthly_budget_usd);
                    })
                    .or_insert_with(|| quota.clone());
            }
        }
        result
    }
}

pub(crate) async fn load(state: &AppState) -> Result<InternalAccessConfig, GatewayError> {
    match state
        .read_system_config_json_value_strong(CONFIG_KEY)
        .await?
    {
        Some(value) => parse(value).map_err(GatewayError::Internal),
        None => Ok(InternalAccessConfig {
            timezone: default_timezone(),
            ..Default::default()
        }),
    }
}

pub(crate) async fn allocation(
    state: &AppState,
    user: &str,
) -> Result<(InternalAccessConfig, Allocation), GatewayError> {
    let config = load(state).await?;
    let memberships = state
        .list_user_group_memberships_by_user_ids(&[user.to_string()])
        .await?;
    let groups = memberships
        .into_iter()
        .map(|m| m.group_id)
        .collect::<Vec<_>>();
    let allocation = config.resolve(user, &groups);
    Ok((config, allocation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn multiple_families_are_explicit_shared_pools_and_personal_allocation_replaces_groups() {
        let config = parse(json!({"model_families":{
            "fabel":{"name":"Fabel","models":["fabel-fast","fabel-pro"]},
            "premium":{"name":"Premium","models":["fabel-pro","other-pro"]}
        },"groups":{"g":{"weekly_budget_usd":100,"families":{"fabel":{"weekly_budget_usd":30},"premium":{"daily_budget_usd":10}}}},
        "users":{"personal":{"weekly_budget_usd":5,"families":{"fabel":{"weekly_budget_usd":2}}}}})).unwrap();
        let group = config.resolve("member", &["g".into()]);
        let policies = config.scoped_policies(&group).unwrap();
        assert_eq!(policies.len(), 3);
        assert_eq!(
            policies[1].models.as_deref(),
            Some(&["fabel-fast".to_string(), "fabel-pro".to_string()][..])
        );
        let personal = config.resolve("personal", &["g".into()]);
        assert_eq!(personal.weekly_budget_usd, Some(5.0));
        assert!(!personal.families.contains_key("premium"));
        for value in [
            json!({"default":{"families":{"missing":{"weekly_budget_usd":1}}}}),
            json!({"model_families":{"x":{"name":"x","models":["a","a"]}}}),
            json!({"model_families":{"x":{"name":"x","models":["*"]}}}),
            json!({"model_families":{"x":{"name":"x","models":[]}}}),
            json!({"model_families":{"x":{"name":"x","models":["a"]}},"default":{"families":{"x":{"daily_budget_usd":-1}}}}),
        ] {
            assert!(parse(value).is_err());
        }
        let zero = parse(json!({"model_families":{"x":{"name":"x","models":["a"]}},
            "default":{"families":{"x":{"daily_budget_usd":0}},"models":{"a":{"daily_budget_usd":0}}}}))
        .unwrap();
        let policies = zero
            .scoped_policies(&zero.resolve("new-user", &[]))
            .unwrap();
        assert_eq!(policies.len(), 2);
        assert!(policies
            .iter()
            .flat_map(|scoped| &scoped.policy.rules)
            .all(|rule| rule.cost_limit_units() == Some(0)));
    }

    #[test]
    fn grants_union_and_personal_override_and_explicit_unlimited() {
        let config = parse(json!({"groups":{
            "normal":{"monthly_budget_usd":20,"models":{"glm":{"five_hour_budget_usd":10,"daily_budget_usd":20}}},
            "research":{"monthly_budget_usd":50,"models":{"glm":{"five_hour_budget_usd":30,"daily_budget_usd":40},"claude":{}}}
        },"users":{"limited":{"models":{"glm":{"daily_budget_usd":5}}},"vip":{"unlimited":true}}})).unwrap();
        let groups = vec!["normal".into(), "research".into()];
        let effective = config.resolve("member", &groups);
        assert_eq!(effective.monthly_budget_usd, Some(50.0));
        assert_eq!(effective.models["glm"].five_hour_budget_usd, Some(30.0));
        assert!(effective.models.contains_key("claude"));
        assert!(!effective.models.contains_key("other"));
        assert!(!config
            .resolve("limited", &groups)
            .models
            .contains_key("claude"));
        let personal = config.resolve("limited", &groups);
        assert_eq!(personal.monthly_budget_usd, None);
        assert_eq!(personal.models["glm"].five_hour_budget_usd, None);
        assert_eq!(personal.models["glm"].daily_budget_usd, Some(5.0));
        assert!(config.resolve("vip", &groups).unlimited);
        assert!(config.resolve("unknown", &[]).models.is_empty());
    }

    #[test]
    fn new_users_have_zero_budgets_while_copied_users_keep_personal_unlimited() {
        let zero = json!({"five_hour_budget_usd":0,"weekly_budget_usd":0,"monthly_budget_usd":0});
        let config = parse(json!({"default":zero,"groups":{"default-group":zero},
            "users":{"copied-user":{"unlimited":true}}}))
        .unwrap();
        for groups in [vec![], vec!["default-group".to_string()]] {
            let allocation = config.resolve("new-user", &groups);
            assert!(!allocation.unlimited);
            let policies = config.scoped_policies(&allocation).unwrap();
            assert_eq!(policies.len(), 1);
            assert_eq!(policies[0].policy.rules.len(), 3);
            assert!(policies[0]
                .policy
                .rules
                .iter()
                .all(|rule| rule.cost_limit_units() == Some(0)));
            assert!(config.resolve("copied-user", &groups).unlimited);
        }
    }

    #[test]
    fn validates_all_windows_and_money_and_does_not_accept_typos() {
        for value in [
            json!({"timezone":"invalid"}),
            json!({"default":{"monthly_budget_usd":-1}}),
            json!({"default":{"five_hour_budget_usd":-1}}),
            json!({"default":{"weekly_budget_usd":-1}}),
            json!({"default":{"models":{"glm":{"daily_requests":5}}}}),
            json!({"default":{"models":{"glm":{"daily_budget_usd":-1}}}}),
            json!({"default":{"unlimited":true,"monthly_budget_usd":10}}),
            json!({"default":{"unlimited":true,"weekly_budget_usd":10}}),
            json!({"defualt":{}}),
        ] {
            assert!(parse(value).is_err());
        }
        let config = parse(json!({"default":{"monthly_budget_usd":25,"models":{"glm":{"five_hour_budget_usd":10,"daily_budget_usd":50,"weekly_budget_usd":200,"monthly_budget_usd":600}}}})).unwrap();
        let policies = config.default.unwrap().policies(&config.timezone).unwrap();
        assert_eq!(policies.len(), 2);
        assert_eq!(policies[1].1.rules.len(), 4);
        assert_eq!(
            policies[1].1.rules[0].window,
            UsagePolicyWindow::Rolling { seconds: 18_000 }
        );
        assert!(policies.iter().all(|(_, p)| p
            .rules
            .iter()
            .all(|r| r.metric == UsagePolicyMetric::ActualCostUsd)));
        assert!(parse(json!({"default":{"models":{"glm":{"daily_budget_usd":0.125}}}})).is_ok());
    }

    #[test]
    fn total_money_group_benefits_and_model_unbounded_windows_are_preserved() {
        let config = parse(json!({"groups":{
            "a":{"five_hour_budget_usd":1.5,"weekly_budget_usd":5,"models":{"glm":{"daily_budget_usd":0.5}}},
            "b":{"five_hour_budget_usd":2.5,"models":{"glm":{}}}
        },"users":{"narrow":{"five_hour_budget_usd":0.25,"models":{"glm":{"daily_budget_usd":0.1}}}}})).unwrap();
        let groups = vec!["a".into(), "b".into()];
        let effective = config.resolve("member", &groups);
        assert_eq!(effective.five_hour_budget_usd, Some(2.5));
        assert_eq!(effective.weekly_budget_usd, Some(5.0));
        assert_eq!(effective.models["glm"].daily_budget_usd, None);
        assert_eq!(
            config.resolve("narrow", &groups).five_hour_budget_usd,
            Some(0.25)
        );
    }

    #[test]
    fn commerce_is_retired_without_removing_internal_cost_rules_or_three_protocols() {
        for path in [
            "/api/admin/payments/gateways",
            "/api/admin/billing/plans",
            "/api/admin/users/user-1/billing/grant-plan",
            "/api/wallet/recharge",
            "/api/payment/epay/notify",
            "/api/users/me/referral",
        ] {
            assert!(commercial_path(path));
        }
        for path in [
            "/api/admin/billing/rules",
            "/api/admin/billing/collectors",
            "/v1/responses",
            "/v1/messages",
            "/v1/chat/completions",
        ] {
            assert!(!commercial_path(path));
            assert!(!retired_generation_path(path));
        }
        assert!(retired_generation_path(
            "/v1beta/models/gemini:generateContent"
        ));
    }
}
