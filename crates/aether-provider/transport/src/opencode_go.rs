use std::collections::BTreeMap;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::snapshot::GatewayProviderTransportSnapshot;
use crate::standard::StandardProviderRequestHeadersInput;

pub const USER_AGENT: &str = "AFSSR-Go-Adapter/1.0";

pub fn is_opencode_go_provider(transport: &GatewayProviderTransportSnapshot) -> bool {
    transport
        .provider
        .provider_type
        .trim()
        .eq_ignore_ascii_case("opencode_go")
}

pub fn is_official_go_url(base_url: &str) -> bool {
    url::Url::parse(base_url).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("opencode.ai")
            && url.port_or_known_default() == Some(443)
            && url.username().is_empty()
            && url.password().is_none()
            && ["/zen/go", "/inference/go"]
                .iter()
                .any(|root| url.path() == *root || url.path().starts_with(&format!("{root}/")))
    })
}

pub fn uses_legacy_messages_auth(transport: &GatewayProviderTransportSnapshot) -> bool {
    aether_ai_formats::normalize_api_format_alias(&transport.endpoint.api_format)
        == "claude:messages"
        && url::Url::parse(&transport.endpoint.base_url)
            .is_ok_and(|url| url.path() == "/zen/go" || url.path().starts_with("/zen/go/"))
}

pub(crate) fn apply_request_headers(
    headers: &mut BTreeMap<String, String>,
    input: &StandardProviderRequestHeadersInput<'_>,
) {
    if !is_opencode_go_provider(input.transport)
        && !is_official_go_url(&input.transport.endpoint.base_url)
    {
        return;
    }
    // Identify this gateway honestly rather than impersonating OpenCode.
    let needs_agent = headers.get("user-agent").is_none_or(|agent| {
        let agent = agent.trim().to_ascii_lowercase();
        agent.is_empty()
            || [
                "curl/",
                "python-",
                "reqwest/",
                "node",
                "undici",
                "openai/",
                "anthropic/",
            ]
            .iter()
            .any(|prefix| agent.starts_with(prefix))
    });
    if needs_agent {
        headers.insert("user-agent".to_string(), USER_AGENT.to_string());
    }
    if headers
        .get("x-opencode-session")
        .is_some_and(|v| !v.trim().is_empty())
    {
        return;
    }
    if let Some(value) = input
        .headers
        .get("x-opencode-session")
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.trim().is_empty())
    {
        headers.insert("x-opencode-session".to_string(), value.to_string());
        return;
    }
    let session = [
        "session_id",
        "session-id",
        "x-session-id",
        "x-codex-session-id",
        "x-aether-session-id",
        "x-claude-code-session-id",
        "x-opencode-session-id",
    ]
    .iter()
    .find_map(|name| {
        input
            .headers
            .get(*name)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|v| !v.is_empty())
    })
    .or_else(|| {
        input
            .original_request_body
            .get("prompt_cache_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
    });
    let fallback;
    let session = match session {
        Some(value) => value,
        None => {
            // No conversation identity: avoid merging unrelated requests into
            // one provider cache. Clients should supply a stable session ID.
            fallback = uuid::Uuid::new_v4().to_string();
            &fallback
        }
    };
    let scope = input
        .headers
        .get(http::header::AUTHORIZATION)
        .or_else(|| input.headers.get("x-api-key"))
        .map(|value| value.as_bytes())
        .unwrap_or_default();
    let mut digest = Sha256::new();
    digest.update(b"aether-opencode-go-session-v1");
    digest.update((scope.len() as u64).to_be_bytes());
    digest.update(scope);
    digest.update(session.as_bytes());
    headers.insert(
        "x-opencode-session".to_string(),
        format!("aether-{:x}", digest.finalize()),
    );
}

#[cfg(test)]
mod tests {
    use super::is_official_go_url;

    #[test]
    fn official_url_detection_is_bounded_to_go() {
        for url in [
            "https://opencode.ai/inference/go/v1",
            "https://opencode.ai/zen/go/v1",
        ] {
            assert!(is_official_go_url(url));
        }
        for url in [
            "https://opencode.ai/zen/v1",
            "https://opencode.ai/inference/go-evil/v1",
            "https://opencode.ai.evil.test/inference/go/v1",
            "http://opencode.ai/zen/go/v1",
            "https://opencode.ai:444/zen/go/v1",
            "https://user@opencode.ai/zen/go/v1",
        ] {
            assert!(!is_official_go_url(url));
        }
    }
}
