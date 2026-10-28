use serde_json::{json, Value};

pub(crate) fn is_deepseek_provider(provider_type: &str, base_url: &str) -> bool {
    let provider_type = provider_type.trim().to_ascii_lowercase();
    if matches!(
        provider_type.as_str(),
        "deepseek" | "deepseek_openai" | "deepseek_anthropic" | "deepseek_compatible"
    ) {
        return true;
    }

    let Some(host) = base_url_host(base_url) else {
        return false;
    };
    host == "deepseek.com" || host.ends_with(".deepseek.com")
}

fn is_deepseek_model(provider_model: &str) -> bool {
    let provider_model = provider_model.trim().to_ascii_lowercase();
    let leaf = provider_model
        .rsplit(['/', ':'])
        .next()
        .unwrap_or(provider_model.as_str());
    leaf == "deepseek" || leaf.starts_with("deepseek-") || leaf.starts_with("deepseek_")
}

fn is_deepseek_upstream(provider_type: &str, base_url: &str, provider_model: &str) -> bool {
    is_deepseek_provider(provider_type, base_url) || is_deepseek_model(provider_model)
}

pub(crate) fn openai_responses_reasoning_replay_policy(
    provider_type: &str,
    base_url: &str,
    provider_model: &str,
) -> crate::ai_serving::OpenAiResponsesReasoningReplayPolicy {
    if is_deepseek_upstream(provider_type, base_url, provider_model) {
        crate::ai_serving::OpenAiResponsesReasoningReplayPolicy::DeepSeekOpaque
    } else {
        crate::ai_serving::OpenAiResponsesReasoningReplayPolicy::OpenAiItemIds
    }
}

pub(crate) fn apply_deepseek_tool_call_thinking_compat(
    provider_request_body: &mut Value,
    provider_type: &str,
    base_url: &str,
    provider_api_format: &str,
    original_request_body: Option<&Value>,
) {
    let provider_model = provider_request_body
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !is_deepseek_upstream(provider_type, base_url, provider_model) {
        return;
    }

    match crate::ai_serving::normalize_api_format_alias(provider_api_format).as_str() {
        "openai:chat" => {
            apply_deepseek_openai_chat_thinking_compat(provider_request_body, original_request_body)
        }
        "claude:messages" => apply_deepseek_claude_messages_thinking_compat(
            provider_request_body,
            original_request_body,
        ),
        _ => {}
    }
}

fn base_url_host(base_url: &str) -> Option<String> {
    let base_url = base_url.trim();
    if base_url.is_empty() {
        return None;
    }

    // Provider configuration historically accepted both absolute URLs and a
    // bare authority/path. Use a real URL parser for both forms: hand-parsing
    // userinfo with `rsplit_once('@')` can mistake an `@` in the path or query
    // for the authority delimiter and classify an attacker-controlled host as
    // `api.deepseek.com`.
    if let Ok(parsed) = url::Url::parse(base_url) {
        if let Some(host) = parsed
            .host_str()
            .filter(|_| matches!(parsed.scheme(), "http" | "https" | "ws" | "wss"))
        {
            return Some(host.to_ascii_lowercase());
        }
        if base_url.contains("://") {
            return None;
        }
    }

    url::Url::parse(&format!("https://{base_url}"))
        .ok()?
        .host_str()
        .map(str::to_ascii_lowercase)
}

fn source_disables_thinking(
    original_request_body: Option<&Value>,
    provider_request_body: &Value,
) -> bool {
    request_explicitly_disables_thinking(provider_request_body)
        || original_request_body.is_some_and(request_explicitly_disables_thinking)
}

fn request_explicitly_disables_thinking(body: &Value) -> bool {
    thinking_type(body).is_some_and(|value| value.eq_ignore_ascii_case("disabled"))
        || reasoning_effort(body).is_some_and(|value| value.eq_ignore_ascii_case("none"))
}

fn thinking_type(body: &Value) -> Option<&str> {
    body.get("thinking")
        .and_then(Value::as_object)
        .and_then(|thinking| thinking.get("type"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn reasoning_effort(body: &Value) -> Option<&str> {
    body.get("reasoning_effort")
        .and_then(Value::as_str)
        .or_else(|| {
            body.get("reasoning")
                .and_then(Value::as_object)
                .and_then(|reasoning| reasoning.get("effort"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn set_deepseek_thinking_type(body: &mut Value, thinking_type: &str) {
    let Some(object) = body.as_object_mut() else {
        return;
    };

    match object.get_mut("thinking") {
        Some(Value::Object(thinking)) => {
            thinking.insert("type".to_string(), Value::String(thinking_type.to_string()));
        }
        _ => {
            object.insert(
                "thinking".to_string(),
                json!({
                    "type": thinking_type,
                }),
            );
        }
    }
}

fn apply_deepseek_openai_chat_thinking_compat(
    provider_request_body: &mut Value,
    original_request_body: Option<&Value>,
) {
    let disabled = source_disables_thinking(original_request_body, provider_request_body);
    set_deepseek_thinking_type(
        provider_request_body,
        if disabled { "disabled" } else { "enabled" },
    );

    let Some(object) = provider_request_body.as_object_mut() else {
        return;
    };
    if disabled {
        if reasoning_effort(&Value::Object(object.clone()))
            .is_some_and(|value| value.eq_ignore_ascii_case("none"))
        {
            object.remove("reasoning_effort");
        }
        return;
    }

    let Some(messages) = object.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    for message in messages {
        let Some(message_object) = message.as_object_mut() else {
            continue;
        };
        let is_assistant = message_object
            .get("role")
            .and_then(Value::as_str)
            .is_some_and(|role| role.trim().eq_ignore_ascii_case("assistant"));
        if !is_assistant {
            continue;
        }
        let has_tool_calls = message_object
            .get("tool_calls")
            .and_then(Value::as_array)
            .is_some_and(|calls| !calls.is_empty());
        if has_tool_calls {
            // Thinking mode rejects a replayed assistant tool-call turn without its
            // `reasoning_content` ("must be passed back to the API"), and some DeepSeek-serving
            // relays treat an empty string as missing. A single space is the smallest value that
            // every such upstream accepts when no reasoning text survived the client round-trip.
            let has_reasoning_text = message_object
                .get("reasoning_content")
                .and_then(Value::as_str)
                .is_some_and(|text| !text.trim().is_empty());
            if !has_reasoning_text {
                message_object.insert(
                    "reasoning_content".to_string(),
                    Value::String(TOOL_CALL_REASONING_CONTENT_PLACEHOLDER.to_string()),
                );
            }
            continue;
        }
        if message_object
            .get("reasoning_content")
            .is_some_and(|value| !value.is_null())
        {
            continue;
        }
        message_object.insert(
            "reasoning_content".to_string(),
            Value::String(String::new()),
        );
    }
}

/// Placeholder replayed as `reasoning_content` on assistant tool-call messages whose reasoning
/// text was not preserved by the client (for example Responses clients that only replay the
/// `function_call` item). Not empty: some upstreams reject `""` as missing.
const TOOL_CALL_REASONING_CONTENT_PLACEHOLDER: &str = " ";

fn apply_deepseek_claude_messages_thinking_compat(
    provider_request_body: &mut Value,
    original_request_body: Option<&Value>,
) {
    if source_disables_thinking(original_request_body, provider_request_body) {
        set_deepseek_thinking_type(provider_request_body, "disabled");
        return;
    }

    let Some(messages) = provider_request_body
        .get_mut("messages")
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for message in messages {
        let Some(message_object) = message.as_object_mut() else {
            continue;
        };
        let is_assistant = message_object
            .get("role")
            .and_then(Value::as_str)
            .is_some_and(|role| role.trim().eq_ignore_ascii_case("assistant"));
        if !is_assistant {
            continue;
        }
        ensure_claude_assistant_message_has_thinking_block(message_object);
    }
}

fn ensure_claude_assistant_message_has_thinking_block(
    message: &mut serde_json::Map<String, Value>,
) {
    let thinking_block = json!({
        "type": "thinking",
        "thinking": "",
    });
    match message.get_mut("content") {
        Some(Value::Array(blocks)) => {
            if blocks.iter().any(is_claude_thinking_block) {
                return;
            }
            blocks.insert(0, thinking_block);
        }
        Some(Value::String(text)) => {
            let text = std::mem::take(text);
            message.insert(
                "content".to_string(),
                Value::Array(vec![
                    thinking_block,
                    json!({
                        "type": "text",
                        "text": text,
                    }),
                ]),
            );
        }
        Some(Value::Null) | None => {
            message.insert("content".to_string(), Value::Array(vec![thinking_block]));
        }
        Some(other) => {
            let existing = std::mem::take(other);
            message.insert(
                "content".to_string(),
                Value::Array(vec![thinking_block, existing]),
            );
        }
    }
}

fn is_claude_thinking_block(block: &Value) -> bool {
    block
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|block_type| block_type.trim().eq_ignore_ascii_case("thinking"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        apply_deepseek_tool_call_thinking_compat, is_deepseek_provider,
        openai_responses_reasoning_replay_policy,
    };

    #[test]
    fn detects_deepseek_provider_by_type_or_host() {
        assert!(is_deepseek_provider(
            "deepseek",
            "https://relay.example.com"
        ));
        assert!(is_deepseek_provider(
            "custom",
            "https://api.deepseek.com/v1"
        ));
        assert!(is_deepseek_provider("custom", "api.deepseek.com/v1"));
        assert!(is_deepseek_provider("custom", "api.deepseek.com:443/v1"));
        assert!(!is_deepseek_provider(
            "custom",
            "https://example.com/deepseek"
        ));
        assert!(!is_deepseek_provider(
            "custom",
            "https://api.deepseek.com.evil.example/v1"
        ));
        assert!(!is_deepseek_provider(
            "custom",
            "https://api.deepseek.com@evil.example/v1"
        ));
        assert!(!is_deepseek_provider(
            "custom",
            "https://evil.example/path@api.deepseek.com/v1"
        ));
        assert!(!is_deepseek_provider(
            "custom",
            "https://evil.example/?relay=@api.deepseek.com"
        ));
        assert!(!is_deepseek_provider("custom", "ftp://api.deepseek.com/v1"));
        assert_eq!(
            openai_responses_reasoning_replay_policy(
                "custom",
                "https://api.deepseek.com/v1",
                "deepseek-v4-flash",
            ),
            crate::ai_serving::OpenAiResponsesReasoningReplayPolicy::DeepSeekOpaque
        );
        assert_eq!(
            openai_responses_reasoning_replay_policy(
                "openai",
                "https://api.openai.com/v1",
                "gpt-5.6-sol",
            ),
            crate::ai_serving::OpenAiResponsesReasoningReplayPolicy::OpenAiItemIds
        );
        assert_eq!(
            openai_responses_reasoning_replay_policy(
                "custom",
                "https://api.b.ai/v1",
                "deepseek-v4-flash",
            ),
            crate::ai_serving::OpenAiResponsesReasoningReplayPolicy::DeepSeekOpaque
        );
        assert_eq!(
            openai_responses_reasoning_replay_policy(
                "custom",
                "https://api.b.ai/v1",
                "not-deepseek-compatible",
            ),
            crate::ai_serving::OpenAiResponsesReasoningReplayPolicy::OpenAiItemIds
        );
    }

    #[test]
    fn custom_deepseek_host_preserves_production_shaped_opaque_reasoning_replay() {
        let reasoning_items = (0..66)
            .map(|index| {
                json!({
                    "type": "reasoning",
                    "encrypted_content": format!("550e8400-e29b-41d4-a716-{index:012}"),
                    "content": [{
                        "type": "reasoning_text",
                        "text": format!("opaque DeepSeek reasoning {index}")
                    }]
                })
            })
            .collect::<Vec<_>>();
        let request = json!({
            "model": "deepseek-v4-flash",
            "input": reasoning_items.clone(),
            "future_request_field": {"preserve": true}
        });
        let replay_policy = openai_responses_reasoning_replay_policy(
            "custom",
            "https://api.deepseek.com/v1",
            "deepseek-v4-flash",
        );
        let mut provider_body = crate::ai_serving::build_standard_request_body_with_model_directives_and_request_headers_and_reasoning_replay_policy(
            &request,
            "openai:responses",
            "deepseek-v4-flash",
            "custom",
            "openai:responses",
            "/v1/responses",
            false,
            None,
            None,
            None,
            false,
            replay_policy,
        )
        .expect("custom DeepSeek Responses body should build");
        crate::ai_serving::finalize_openai_provider_request_with_codex_model_capabilities_and_reasoning_replay_policy(
            &mut provider_body,
            crate::ai_serving::OpenAiProviderRequestFinalization {
                source_api_format: "openai:responses",
                provider_api_format: "openai:responses",
                provider_type: "custom",
                provider_model: "deepseek-v4-flash",
                source_model: "deepseek-v4-flash",
                body_rules: None,
                upstream_is_stream: false,
                require_body_stream_field: false,
            },
            None,
            replay_policy,
        )
        .expect("custom DeepSeek finalization should accept opaque reasoning replay");
        assert_eq!(provider_body["input"].as_array().map(Vec::len), Some(66));
        assert_eq!(provider_body["future_request_field"]["preserve"], true);

        let mut deepseek = json!({"input": reasoning_items.clone()});
        let mut openai = json!({"input": reasoning_items});

        assert_eq!(
            crate::ai_serving::strip_incompatible_openai_responses_reasoning_items_with_policy(
                &mut deepseek,
                "openai:responses",
                openai_responses_reasoning_replay_policy(
                    "custom",
                    "https://api.deepseek.com/v1",
                    "deepseek-v4-flash",
                ),
            ),
            0
        );
        assert_eq!(deepseek["input"].as_array().map(Vec::len), Some(66));

        assert_eq!(
            crate::ai_serving::strip_incompatible_openai_responses_reasoning_items_with_policy(
                &mut openai,
                "openai:responses",
                openai_responses_reasoning_replay_policy(
                    "openai",
                    "https://api.openai.com/v1",
                    "gpt-5.6-sol",
                ),
            ),
            66
        );
        assert_eq!(openai["input"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn openai_chat_deepseek_adds_thinking_and_placeholder_reasoning_content() {
        let mut body = json!({
            "model": "deepseek-chat",
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": null, "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "lookup", "arguments": "{}"}
                }]},
                {"role": "tool", "tool_call_id": "call_1", "content": "{}"}
            ]
        });

        apply_deepseek_tool_call_thinking_compat(
            &mut body,
            "deepseek",
            "https://api.deepseek.com/v1",
            "openai:chat",
            None,
        );

        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["messages"][1]["reasoning_content"], " ");
    }

    #[test]
    fn custom_relay_deepseek_model_adds_chat_thinking_compat() {
        let mut body = json!({
            "model": "deepseek-v4-flash",
            "messages": [
                {"role": "user", "content": "inspect the repository"},
                {"role": "assistant", "content": null, "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "inspect", "arguments": "{}"}
                }]},
                {"role": "tool", "tool_call_id": "call_1", "content": "done"}
            ]
        });

        apply_deepseek_tool_call_thinking_compat(
            &mut body,
            "custom",
            "https://api.b.ai/v1",
            "openai:chat",
            None,
        );

        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["messages"][1]["reasoning_content"], " ");
    }

    #[test]
    fn openai_chat_deepseek_tool_call_keeps_reasoning_text_and_replaces_empty_value() {
        let tool_call = json!([{
            "id": "call_1",
            "type": "function",
            "function": {"name": "lookup", "arguments": "{}"}
        }]);
        let mut body = json!({
            "model": "deepseek-v4.1-flash",
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": null, "reasoning_content": "plan", "tool_calls": tool_call},
                {"role": "tool", "tool_call_id": "call_1", "content": "{}"},
                {"role": "assistant", "content": null, "reasoning_content": "", "tool_calls": tool_call},
                {"role": "tool", "tool_call_id": "call_1", "content": "{}"},
                {"role": "assistant", "content": "done"}
            ]
        });

        apply_deepseek_tool_call_thinking_compat(
            &mut body,
            "opencode_go",
            "https://opencode.ai/zen/go/v1",
            "openai:chat",
            None,
        );

        assert_eq!(body["messages"][1]["reasoning_content"], "plan");
        assert_eq!(body["messages"][3]["reasoning_content"], " ");
        assert_eq!(body["messages"][5]["reasoning_content"], "");
    }

    #[test]
    fn responses_to_chat_tool_call_history_replays_reasoning_content_for_deepseek() {
        // Issue #18: Responses clients replay only `function_call` items for tool-call turns
        // without reasoning text, which used to reach the thinking-mode upstream without
        // `reasoning_content` and fail every later turn with a 400.
        for reasoning in [
            None,
            Some(json!({
                "type": "reasoning",
                "id": "rs_1",
                "summary": [{"type": "summary_text", "text": "need to list files"}],
                "encrypted_content": "opaque-upstream-state"
            })),
        ] {
            let mut input = vec![json!({
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "inspect"}]
            })];
            input.extend(reasoning.clone());
            input.push(json!({
                "type": "function_call",
                "call_id": "call_x",
                "name": "shell",
                "arguments": "{}"
            }));
            input.push(json!({
                "type": "function_call_output",
                "call_id": "call_x",
                "output": "ok"
            }));
            input.push(json!({
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "continue"}]
            }));
            let responses_body = json!({"model": "deepseek-v4.1-flash", "input": input});
            let mut body = super::super::normalize_openai_responses_request_to_openai_chat_request(
                &responses_body,
            )
            .expect("openai chat request");

            apply_deepseek_tool_call_thinking_compat(
                &mut body,
                "custom",
                "https://relay.example.com/v1",
                "openai:chat",
                Some(&responses_body),
            );

            let assistant = body["messages"]
                .as_array()
                .expect("messages")
                .iter()
                .find(|message| message.get("tool_calls").is_some())
                .expect("assistant tool-call message")
                .clone();
            let reasoning_content = assistant["reasoning_content"]
                .as_str()
                .expect("reasoning_content on assistant tool-call message");
            assert!(!reasoning_content.is_empty());
            if reasoning.is_some() {
                assert_eq!(reasoning_content, "need to list files");
            }
        }
    }

    #[test]
    fn custom_relay_non_deepseek_model_is_not_rewritten() {
        let original = json!({
            "model": "not-deepseek-compatible",
            "messages": [{"role": "assistant", "content": "done"}]
        });
        let mut body = original.clone();

        apply_deepseek_tool_call_thinking_compat(
            &mut body,
            "custom",
            "https://api.b.ai/v1",
            "openai:chat",
            None,
        );

        assert_eq!(body, original);
    }

    #[test]
    fn openai_chat_deepseek_honors_disabled_thinking() {
        let original = json!({"reasoning_effort": "none"});
        let mut body = json!({
            "model": "deepseek-chat",
            "reasoning_effort": "none",
            "messages": [
                {"role": "assistant", "content": "hi"}
            ]
        });

        apply_deepseek_tool_call_thinking_compat(
            &mut body,
            "deepseek",
            "https://api.deepseek.com/v1",
            "openai:chat",
            Some(&original),
        );

        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("reasoning_effort").is_none());
        assert!(body["messages"][0].get("reasoning_content").is_none());
    }

    #[test]
    fn claude_messages_deepseek_prepends_empty_thinking_block() {
        let mut body = json!({
            "model": "deepseek-3.2",
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "call_1", "name": "lookup", "input": {}}
                ]}
            ]
        });

        apply_deepseek_tool_call_thinking_compat(
            &mut body,
            "deepseek",
            "https://api.deepseek.com",
            "claude:messages",
            None,
        );

        assert_eq!(body["messages"][1]["content"][0]["type"], "thinking");
        assert_eq!(body["messages"][1]["content"][0]["thinking"], "");
        assert_eq!(body["messages"][1]["content"][1]["type"], "tool_use");
    }

    #[test]
    fn claude_messages_deepseek_converts_string_assistant_content_to_blocks() {
        let mut body = json!({
            "model": "deepseek-3.2",
            "messages": [{
                "role": "assistant",
                "content": "done"
            }]
        });

        apply_deepseek_tool_call_thinking_compat(
            &mut body,
            "deepseek",
            "https://api.deepseek.com",
            "claude:messages",
            None,
        );

        assert_eq!(body["messages"][0]["content"][0]["type"], "thinking");
        assert_eq!(body["messages"][0]["content"][1]["type"], "text");
        assert_eq!(body["messages"][0]["content"][1]["text"], "done");
    }

    #[test]
    fn claude_messages_deepseek_preserves_existing_thinking_block() {
        let mut body = json!({
            "model": "deepseek-3.2",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "plan", "signature": "sig"},
                    {"type": "text", "text": "answer"}
                ]
            }]
        });

        apply_deepseek_tool_call_thinking_compat(
            &mut body,
            "deepseek",
            "https://api.deepseek.com",
            "claude:messages",
            None,
        );

        assert_eq!(body["messages"][0]["content"].as_array().unwrap().len(), 2);
        assert_eq!(body["messages"][0]["content"][0]["thinking"], "plan");
        assert_eq!(body["messages"][0]["content"][0]["signature"], "sig");
    }
}
