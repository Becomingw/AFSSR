//! OpenCode Go subscription quota parsing.
//!
//! `GET https://opencode.ai/zen/go/v1/usage` (Bearer API key) returns
//! `{"usage":{"rolling":{..},"weekly":{..},"monthly":{..}}}` where each
//! window carries `status` (`ok` / `rate-limited`), `percent` (used, 0-100)
//! and `resetsAt` (RFC 3339).  The parsed result is stored in the key's
//! `upstream_metadata.opencode_go` bucket and later materialized into the
//! shared quota status snapshot.

use serde_json::{json, Map, Value};

/// Window ids in display order: 5h rolling, weekly, monthly.
pub const OPENCODE_GO_QUOTA_WINDOWS: &[&str] = &["rolling", "weekly", "monthly"];

fn window_percent(window: &Map<String, Value>) -> Option<f64> {
    let value = window
        .get("percent")
        .or_else(|| window.get("usedPercent"))
        .or_else(|| window.get("used_percent"))?;
    let percent = match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().trim_end_matches('%').parse::<f64>().ok(),
        _ => None,
    }?;
    percent.is_finite().then(|| percent.clamp(0.0, 100.0))
}

fn window_reset_at(window: &Map<String, Value>) -> Option<u64> {
    let value = window
        .get("resetsAt")
        .or_else(|| window.get("resets_at"))
        .or_else(|| window.get("reset_at"))?;
    match value {
        Value::String(text) => chrono::DateTime::parse_from_rfc3339(text.trim())
            .ok()
            .and_then(|value| u64::try_from(value.timestamp()).ok()),
        Value::Number(number) => number.as_f64().filter(|value| *value > 0.0).map(|value| {
            if value > 1_000_000_000_000.0 {
                (value / 1000.0) as u64
            } else {
                value as u64
            }
        }),
        _ => None,
    }
}

fn window_status(window: &Map<String, Value>) -> Option<String> {
    window
        .get("status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
}

/// Whether an upstream window status means the window currently rejects
/// requests.
pub fn opencode_go_window_status_is_limited(status: &str) -> bool {
    let normalized = status.trim().to_ascii_lowercase().replace(['_', ' '], "-");
    matches!(
        normalized.as_str(),
        "rate-limited" | "limited" | "exhausted" | "blocked" | "exceeded"
    )
}

/// Parse a successful `/zen/go/v1/usage` body into the metadata bucket.
/// Returns `None` when no window could be parsed.
pub fn parse_opencode_go_usage_response(body: &Value, now_unix_secs: u64) -> Option<Value> {
    let root = body.as_object()?;
    let usage = root.get("usage").and_then(Value::as_object).unwrap_or(root);

    let mut windows = Map::new();
    let mut rate_limited = false;
    for code in OPENCODE_GO_QUOTA_WINDOWS {
        let Some(window) = usage.get(*code).and_then(Value::as_object) else {
            continue;
        };
        let percent = window_percent(window);
        let status = window_status(window);
        if percent.is_none() && status.is_none() {
            continue;
        }
        // With 0% used the upstream reports a now-plus-window placeholder
        // instead of a real reset time; drop it.
        let reset_at = window_reset_at(window).filter(|_| percent.is_none_or(|p| p > 0.0));
        let limited = status
            .as_deref()
            .is_some_and(opencode_go_window_status_is_limited)
            || percent.is_some_and(|p| p >= 100.0);
        rate_limited |= limited;
        windows.insert(
            (*code).to_string(),
            json!({
                "status": status,
                "percent": percent,
                "reset_at": reset_at,
                "limited": limited,
            }),
        );
    }
    if windows.is_empty() {
        return None;
    }
    Some(json!({
        "updated_at": now_unix_secs,
        "windows": Value::Object(windows),
        "rate_limited": rate_limited,
    }))
}

/// Outcome classification for a non-200 usage response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenCodeGoUsageErrorKind {
    /// 401: key invalid or revoked.
    AuthInvalid,
    /// 403: key valid but the workspace has no Go subscription
    /// (OpenCode returns an entitlement error).
    NoSubscription,
    /// 429: the usage endpoint itself is rate limited.
    RateLimited,
    Other,
}

pub fn classify_opencode_go_usage_error(status_code: u16) -> OpenCodeGoUsageErrorKind {
    match status_code {
        401 => OpenCodeGoUsageErrorKind::AuthInvalid,
        403 => OpenCodeGoUsageErrorKind::NoSubscription,
        429 => OpenCodeGoUsageErrorKind::RateLimited,
        _ => OpenCodeGoUsageErrorKind::Other,
    }
}

/// Metadata bucket patch for a failed usage request.  Previously observed
/// window data is preserved by the caller's merge.
pub fn build_opencode_go_usage_error_metadata(status_code: u16, now_unix_secs: u64) -> Value {
    let kind = classify_opencode_go_usage_error(status_code);
    json!({
        "updated_at": now_unix_secs,
        "last_error": format!("HTTP {status_code}"),
        "last_error_status": status_code,
        "auth_invalid": kind == OpenCodeGoUsageErrorKind::AuthInvalid,
        "no_subscription": kind == OpenCodeGoUsageErrorKind::NoSubscription,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_786_000_000;

    #[test]
    fn parses_three_windows_with_percent_and_reset() {
        let body = json!({
            "usage": {
                "rolling": {"status": "ok", "percent": 42, "resetsAt": "2026-08-13T16:27:38.287Z"},
                "weekly": {"status": "ok", "percent": 3.5, "resetsAt": "2026-08-17T00:00:00.287Z"},
                "monthly": {"status": "ok", "percent": "1", "resetsAt": "2026-09-13T06:06:01.287Z"}
            }
        });
        let parsed = parse_opencode_go_usage_response(&body, NOW).expect("parsed");
        assert_eq!(parsed["updated_at"], json!(NOW));
        assert_eq!(parsed["rate_limited"], json!(false));
        assert_eq!(parsed["windows"]["rolling"]["percent"], json!(42.0));
        assert_eq!(parsed["windows"]["weekly"]["percent"], json!(3.5));
        assert_eq!(parsed["windows"]["monthly"]["percent"], json!(1.0));
        assert_eq!(
            parsed["windows"]["rolling"]["reset_at"],
            json!(1_786_638_458u64)
        );
        assert_eq!(parsed["windows"]["rolling"]["limited"], json!(false));
    }

    #[test]
    fn marks_rate_limited_and_full_windows_as_limited() {
        let body = json!({
            "usage": {
                "rolling": {"status": "rate-limited", "percent": 97, "resetsAt": "2026-08-13T16:27:38Z"},
                "weekly": {"status": "ok", "percent": 100, "resetsAt": "2026-08-17T00:00:00Z"},
                "monthly": {"status": "ok", "percent": 10, "resetsAt": "2026-09-13T06:06:01Z"}
            }
        });
        let parsed = parse_opencode_go_usage_response(&body, NOW).expect("parsed");
        assert_eq!(parsed["rate_limited"], json!(true));
        assert_eq!(parsed["windows"]["rolling"]["limited"], json!(true));
        assert_eq!(parsed["windows"]["weekly"]["limited"], json!(true));
        assert_eq!(parsed["windows"]["monthly"]["limited"], json!(false));
    }

    #[test]
    fn drops_placeholder_reset_at_when_unused_and_tolerates_missing_fields() {
        let body = json!({
            "usage": {
                "rolling": {"status": "ok", "percent": 0, "resetsAt": "2026-08-13T16:27:38Z"},
                "weekly": {"percent": 5},
                "monthly": {"resetsAt": "2026-09-13T06:06:01Z"}
            }
        });
        let parsed = parse_opencode_go_usage_response(&body, NOW).expect("parsed");
        assert_eq!(parsed["windows"]["rolling"]["reset_at"], Value::Null);
        assert_eq!(parsed["windows"]["weekly"]["status"], Value::Null);
        assert_eq!(parsed["windows"]["weekly"]["reset_at"], Value::Null);
        // A window with neither percent nor status carries no signal.
        assert!(parsed["windows"].get("monthly").is_none());
    }

    #[test]
    fn returns_none_for_unparseable_payloads() {
        assert!(parse_opencode_go_usage_response(&json!({}), NOW).is_none());
        assert!(parse_opencode_go_usage_response(&json!({"usage": {}}), NOW).is_none());
        assert!(parse_opencode_go_usage_response(&json!([1, 2]), NOW).is_none());
        assert!(
            parse_opencode_go_usage_response(&json!({"usage": {"rolling": "x"}}), NOW).is_none()
        );
    }

    #[test]
    fn classifies_auth_and_entitlement_errors() {
        assert_eq!(
            classify_opencode_go_usage_error(401),
            OpenCodeGoUsageErrorKind::AuthInvalid
        );
        assert_eq!(
            classify_opencode_go_usage_error(403),
            OpenCodeGoUsageErrorKind::NoSubscription
        );
        assert_eq!(
            classify_opencode_go_usage_error(429),
            OpenCodeGoUsageErrorKind::RateLimited
        );
        assert_eq!(
            classify_opencode_go_usage_error(500),
            OpenCodeGoUsageErrorKind::Other
        );
        let forbidden = build_opencode_go_usage_error_metadata(403, NOW);
        assert_eq!(forbidden["no_subscription"], json!(true));
        assert_eq!(forbidden["auth_invalid"], json!(false));
        assert_eq!(forbidden["last_error"], json!("HTTP 403"));
        let unauthorized = build_opencode_go_usage_error_metadata(401, NOW);
        assert_eq!(unauthorized["auth_invalid"], json!(true));
        assert_eq!(unauthorized["no_subscription"], json!(false));
    }
}
