//! `ARCA_*` environment aliases for the inherited `AETHER_*` / `AFSSR_*` names.
//!
//! Service code still reads the legacy names. [`apply_env_aliases`] runs once at
//! process start, before any other thread exists, and mirrors every `ARCA_<X>`
//! variable onto `AETHER_<X>` and `AFSSR_<X>`. `ARCA_*` therefore wins when both
//! spellings are set, and legacy-only configuration keeps working with a
//! deprecation warning.

use std::collections::BTreeMap;
use std::ffi::OsString;

const NEW_PREFIX: &str = "ARCA_";
const LEGACY_PREFIXES: [&str; 2] = ["AETHER_", "AFSSR_"];
const MAX_LISTED_NAMES: usize = 12;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct EnvAliasPlan {
    /// Legacy variables to set (or overwrite) from their `ARCA_*` counterpart.
    pub set: Vec<(String, OsString)>,
    /// Legacy variables configured without an `ARCA_*` counterpart.
    pub deprecated: Vec<String>,
    /// Legacy variables whose differing value is replaced by the `ARCA_*` one.
    pub shadowed: Vec<String>,
}

pub fn plan_env_aliases(vars: impl IntoIterator<Item = (OsString, OsString)>) -> EnvAliasPlan {
    let env: BTreeMap<String, OsString> = vars
        .into_iter()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value)))
        .collect();
    let mut plan = EnvAliasPlan::default();

    for (key, value) in &env {
        let Some(suffix) = key.strip_prefix(NEW_PREFIX) else {
            continue;
        };
        for prefix in LEGACY_PREFIXES {
            let legacy = format!("{prefix}{suffix}");
            match env.get(&legacy) {
                Some(existing) if existing == value => {}
                Some(_) => {
                    plan.shadowed.push(legacy.clone());
                    plan.set.push((legacy, value.clone()));
                }
                None => plan.set.push((legacy, value.clone())),
            }
        }
    }

    for key in env.keys() {
        let Some(suffix) = LEGACY_PREFIXES
            .iter()
            .find_map(|prefix| key.strip_prefix(prefix))
        else {
            continue;
        };
        if !env.contains_key(&format!("{NEW_PREFIX}{suffix}")) {
            plan.deprecated.push(key.clone());
        }
    }
    plan
}

fn summarize(names: &[String]) -> String {
    let mut listed = names
        .iter()
        .take(MAX_LISTED_NAMES)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if names.len() > MAX_LISTED_NAMES {
        listed.push_str(&format!(", ... (+{} more)", names.len() - MAX_LISTED_NAMES));
    }
    listed
}

/// Applies the aliases to the process environment and warns on stderr (tracing is
/// not initialised yet). Call before spawning any thread or async runtime.
pub fn apply_env_aliases() {
    let plan = plan_env_aliases(std::env::vars_os());
    for (key, value) in &plan.set {
        std::env::set_var(key, value);
    }
    if !plan.deprecated.is_empty() {
        eprintln!(
            "WARNING: deprecated environment variables in use: {}. Rename the AETHER_/AFSSR_ prefix to ARCA_; the legacy names still work for now.",
            summarize(&plan.deprecated)
        );
    }
    if !plan.shadowed.is_empty() {
        eprintln!(
            "WARNING: ARCA_* values override differing legacy variables: {}.",
            summarize(&plan.shadowed)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        pairs
            .iter()
            .map(|(k, v)| (OsString::from(*k), OsString::from(*v)))
            .collect()
    }

    #[test]
    fn arca_only_is_mirrored_without_warnings() {
        let plan = plan_env_aliases(vars(&[("ARCA_LOG_FORMAT", "json")]));
        assert_eq!(
            plan.set,
            vec![
                ("AETHER_LOG_FORMAT".to_string(), OsString::from("json")),
                ("AFSSR_LOG_FORMAT".to_string(), OsString::from("json")),
            ]
        );
        assert!(plan.deprecated.is_empty());
        assert!(plan.shadowed.is_empty());
    }

    #[test]
    fn legacy_only_is_deprecated_and_left_untouched() {
        let plan = plan_env_aliases(vars(&[
            ("AETHER_LOG_FORMAT", "json"),
            ("AFSSR_MCP_ENABLED", "true"),
        ]));
        assert!(plan.set.is_empty());
        assert_eq!(
            plan.deprecated,
            vec!["AETHER_LOG_FORMAT", "AFSSR_MCP_ENABLED"]
        );
    }

    #[test]
    fn arca_wins_over_differing_legacy_value() {
        let plan = plan_env_aliases(vars(&[("ARCA_PORT", "1"), ("AETHER_PORT", "2")]));
        assert_eq!(plan.shadowed, vec!["AETHER_PORT"]);
        assert!(plan
            .set
            .contains(&("AETHER_PORT".to_string(), OsString::from("1"))));
        assert!(plan.deprecated.is_empty());
    }

    #[test]
    fn equal_legacy_value_is_not_reported() {
        let plan = plan_env_aliases(vars(&[("ARCA_PORT", "1"), ("AETHER_PORT", "1")]));
        assert!(plan.shadowed.is_empty());
        assert_eq!(
            plan.set,
            vec![("AFSSR_PORT".to_string(), OsString::from("1"))]
        );
    }

    #[test]
    fn unrelated_variables_are_ignored() {
        let plan = plan_env_aliases(vars(&[
            ("DATABASE_URL", "x"),
            ("AETHERX", "y"),
            ("ARCA", "z"),
        ]));
        assert_eq!(plan, EnvAliasPlan::default());
    }

    #[test]
    fn long_lists_are_truncated() {
        let names = (0..15).map(|i| format!("AETHER_V{i}")).collect::<Vec<_>>();
        let text = summarize(&names);
        assert!(text.ends_with("(+3 more)"));
    }
}
