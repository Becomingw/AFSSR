//! Real Codex / Claude Code request bodies (captured from the actual CLIs by tests/client_e2e)
//! converted for every OpenCode Go model on its native protocol.
//!
//! `GO_PROBE_OUT=<dir>` dumps every converted body (or the conversion error) so
//! tests/client_e2e/go_validate.mjs can check them against the strict Go upstream validators.
use aether_ai_formats::{
    convert_request, finalize_openai_provider_request, FormatContext,
    OpenAiProviderRequestFinalization,
};
use serde_json::{json, Value};

fn context(body: &Value, client: &str, format: &str, model: &str) -> FormatContext {
    FormatContext::default().with_mapped_model(model).with_upstream_stream(true)
        .with_history_scope("fixture")
        .with_report_context(json!({"provider_type":"opencode_go", "provider_api_format":format, "mapped_model":model,
            "client_api_format":client, "needs_conversion":client != format, "original_request_body":body, "api_key_id":"fixture"}))
}

fn models() -> Vec<(String, &'static str)> {
    let Ok(path) = std::env::var("GO_MODELS_FILE") else {
        return Vec::new();
    };
    let table: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut out = Vec::new();
    for (protocol, format) in [
        ("responses", "openai:responses"),
        ("chat", "openai:chat"),
        ("messages", "claude:messages"),
        ("gemini", "gemini:generate_content"),
    ] {
        for model in table[protocol].as_array().into_iter().flatten() {
            out.push((model.as_str().unwrap().to_string(), format));
        }
    }
    out
}

#[test]
fn real_clients_convert_for_every_go_model() {
    let Ok(dir) = std::env::var("GO_PROBE_OUT") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let codex: Value =
        serde_json::from_str(include_str!("fixtures/real_clients/codex_responses.json")).unwrap();
    let claude: Value =
        serde_json::from_str(include_str!("fixtures/real_clients/claude_messages.json")).unwrap();
    // Production Codex also offers the freeform `apply_patch` (a Lark-grammar custom tool).
    let mut codex_freeform = codex.clone();
    codex_freeform["tools"].as_array_mut().unwrap().push(json!({"type":"custom","name":"apply_patch",
        "description":"Use the `apply_patch` tool to edit files.",
        "format":{"type":"grammar","syntax":"lark","definition":"start: begin_patch hunk+ end_patch\nbegin_patch: \"*** Begin Patch\" LF\nend_patch: \"*** End Patch\" LF?\nhunk: /.+/ LF\nLF: /\\n/"}}));
    for (client, api, body) in [
        ("codex", "openai:responses", &codex),
        ("codexfree", "openai:responses", &codex_freeform),
        ("claude", "claude:messages", &claude),
    ] {
        for (model, format) in models() {
            let result = convert_request(api, format, body, &context(body, api, format, &model))
                .map_err(|e| format!("{e:?}"))
                .and_then(|mut converted| {
                    // The gateway always runs this provider-specific finalization after conversion.
                    finalize_openai_provider_request(
                        &mut converted,
                        OpenAiProviderRequestFinalization {
                            source_api_format: api,
                            provider_api_format: format,
                            provider_type: "opencode_go",
                            provider_model: &model,
                            source_model: "client-model",
                            body_rules: None,
                            upstream_is_stream: true,
                            require_body_stream_field: true,
                        },
                    )
                    .map_err(|violation| format!("{violation:?}"))?;
                    Ok(converted)
                });
            let record = match result {
                Ok(converted) => {
                    json!({"ok": true, "format": format, "model": model, "body": converted})
                }
                Err(error) => {
                    json!({"ok": false, "format": format, "model": model, "error": error})
                }
            };
            std::fs::write(
                format!("{dir}/{client}__{model}.json"),
                serde_json::to_string(&record).unwrap(),
            )
            .unwrap();
        }
    }
}

fn finalized(client: &str, format: &str, model: &str, body: &Value) -> Result<Value, String> {
    let mut converted =
        convert_request(client, format, body, &context(body, client, format, model))
            .map_err(|e| format!("{e:?}"))?;
    finalize_openai_provider_request(
        &mut converted,
        OpenAiProviderRequestFinalization {
            source_api_format: client,
            provider_api_format: format,
            provider_type: "opencode_go",
            provider_model: model,
            source_model: "client-model",
            body_rules: None,
            upstream_is_stream: true,
            require_body_stream_field: true,
        },
    )
    .map_err(|violation| format!("{violation:?}"))?;
    Ok(converted)
}

#[test]
fn codex_hosted_web_search_never_reaches_a_backend_that_cannot_run_it() {
    let codex: Value =
        serde_json::from_str(include_str!("fixtures/real_clients/codex_responses.json")).unwrap();
    assert!(
        codex["tools"].to_string().contains("web_search"),
        "fixture must carry the hosted tool"
    );
    for (format, model) in [
        ("openai:chat", "glm-5.3"),
        ("openai:chat", "deepseek-v4-pro"),
        ("openai:chat", "beam-free"),
        ("claude:messages", "qwen3.8-max"),
        ("claude:messages", "claude-haiku-5-5"),
        ("claude:messages", "minimax-m3"),
        ("gemini:generate_content", "gemini-4-argon-contributor"),
        ("openai:responses", "muse-spark-1.3-contributor"),
        ("openai:responses", "gpt-6-luna"),
        ("openai:responses", "grok-4.7"),
    ] {
        let converted = finalized("openai:responses", format, model, &codex)
            .unwrap_or_else(|e| panic!("{model}: {e}"));
        let text = converted.to_string();
        assert!(
            !text.contains("web_search_2025")
                && !text.contains("googleSearch")
                && !text.contains("\"web_search\""),
            "{model} still carries a hosted web tool"
        );
        assert!(
            text.contains("exec_command"),
            "{model} lost the client's shell tool"
        );
    }
    // Grok documents a hosted search, but it is not forwarded either (unverified, and unpriceable
    // under a hard cost limit): the client's tools are what Codex needs.
    let grok = finalized("openai:responses", "openai:responses", "grok-4.7", &codex).unwrap();
    assert!(!grok.to_string().contains("\"web_search\""));
}

#[test]
fn claude_code_extensions_are_not_forwarded_to_go_messages() {
    let claude: Value =
        serde_json::from_str(include_str!("fixtures/real_clients/claude_messages.json")).unwrap();
    assert!(claude.get("context_management").is_some());
    for model in [
        "minimax-m2.7",
        "qwen3.7-max",
        "claude-haiku-5-5",
        "qwen3.8-max",
    ] {
        let converted = finalized("claude:messages", "claude:messages", model, &claude).unwrap();
        for key in ["context_management", "builtin_tools"] {
            assert!(converted.get(key).is_none(), "{model} still carries {key}");
        }
        assert!(converted["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t.get("type").is_none() || t["type"] == "custom"));
    }
}

#[test]
fn claude_code_web_tools_stay_client_functions_on_gemini() {
    let claude: Value =
        serde_json::from_str(include_str!("fixtures/real_clients/claude_messages.json")).unwrap();
    let converted = finalized(
        "claude:messages",
        "gemini:generate_content",
        "gemini-4-argon-contributor",
        &claude,
    )
    .unwrap();
    let tools = converted["tools"].as_array().unwrap();
    assert!(tools
        .iter()
        .all(|t| t.get("googleSearch").is_none() && t.get("urlContext").is_none()));
    let names: Vec<_> = tools
        .iter()
        .flat_map(|t| {
            t["functionDeclarations"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .map(|d| d["name"].as_str().unwrap().to_string())
        .collect();
    assert!(
        names.contains(&"WebSearch".to_string()) && names.contains(&"WebFetch".to_string()),
        "{names:?}"
    );
}

#[test]
fn go_responses_stream_with_reasoning_reaches_a_claude_code_client_as_text() {
    use aether_ai_formats::formats::shared::stream_rewrite::maybe_build_ai_surface_stream_rewriter;
    let sse = include_str!("fixtures/real_clients/go_responses_reasoning_text.sse");
    let report = json!({"provider_type":"opencode_go", "provider_api_format":"openai:responses", "mapped_model":"gpt-6-luna",
        "client_api_format":"claude:messages", "needs_conversion":true, "model":"gpt-6-luna"});
    let mut rewriter =
        maybe_build_ai_surface_stream_rewriter(Some(&report)).expect("conversion rewriter");
    let mut wire = rewriter.push_chunk(sse.as_bytes()).unwrap();
    wire.extend(rewriter.finish().unwrap());
    let wire = String::from_utf8(wire).unwrap();
    let blocks: Vec<_> = wire
        .lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|e| e["type"] == "content_block_start")
        .map(|e| e["content_block"]["type"].as_str().unwrap().to_string())
        .collect();
    println!("{blocks:?}");
    assert!(wire.contains("E2E_DONE rounds=0"), "answer lost:\n{wire}");
    assert_eq!(
        blocks.last().map(String::as_str),
        Some("text"),
        "no block may follow the answer: {blocks:?}"
    );
}

/// The path the gateway really takes for Claude Code and Chat clients (no Responses ingress).
#[test]
fn standard_builder_applies_go_rules_to_claude_code_requests() {
    use aether_ai_formats::formats::shared::standard_matrix::build_standard_request_body_with_model_directives_and_request_headers_and_reasoning_replay_policy as build;
    let claude: Value =
        serde_json::from_str(include_str!("fixtures/real_clients/claude_messages.json")).unwrap();
    for (format, model) in [
        ("openai:responses", "gpt-6-luna"),
        ("openai:chat", "glm-5.3"),
        ("claude:messages", "qwen3.7-max"),
        ("gemini:generate_content", "gemini-4-argon-contributor"),
    ] {
        let body = build(
            &claude, "claude:messages", model, "opencode_go", format, "/v1/messages", true, None, Some("key"), None, false,
            aether_ai_formats::formats::openai::responses::OpenAiResponsesReasoningReplayPolicy::OpenAiItemIds,
        );
        assert!(body.is_some(), "{format} {model}: body build failed");
    }
}

#[test]
fn go_gemini_thought_and_tool_call_reach_a_claude_code_client_once_each() {
    use aether_ai_formats::formats::shared::stream_rewrite::maybe_build_ai_surface_stream_rewriter;
    let sse = include_str!("fixtures/real_clients/go_gemini_thought_tool.sse");
    let report = json!({"provider_type":"opencode_go", "provider_api_format":"gemini:generate_content", "mapped_model":"gemini-4-argon-contributor",
        "client_api_format":"claude:messages", "needs_conversion":true, "model":"gemini-4-argon-contributor"});
    let mut rewriter =
        maybe_build_ai_surface_stream_rewriter(Some(&report)).expect("conversion rewriter");
    let mut wire = rewriter.push_chunk(sse.as_bytes()).unwrap();
    wire.extend(rewriter.finish().unwrap());
    let wire = String::from_utf8(wire).unwrap();
    let blocks: Vec<_> = wire
        .lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|e| e["type"] == "content_block_start")
        .map(|e| e["content_block"]["type"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(blocks, ["thinking", "tool_use"], "{wire}");
    // Gemini names no calls: the id must differ from turn to turn, or Claude Code treats the
    // second call as one it already answered and loops.
    assert!(!wire.contains("call_auto_"), "{wire}");
}

#[test]
fn claude_code_tool_history_replays_to_gemini_without_extra_thoughts() {
    use aether_ai_formats::formats::shared::standard_matrix::build_standard_request_body_with_model_directives_and_request_headers_and_reasoning_replay_policy as build;
    let body = json!({"model":"x","max_tokens":100,"stream":true,
        "messages":[
            {"role":"user","content":[{"type":"text","text":"go"}]},
            {"role":"assistant","content":[{"type":"thinking","thinking":"Considering","signature":"sig"},{"type":"tool_use","id":"toolu_1","name":"Write","input":{"file_path":"/tmp/a","content":"A"}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"ok"}]}],
        "tools":[{"name":"Write","description":"w","input_schema":{"type":"object","properties":{"file_path":{"type":"string"}}}}]});
    let converted = build(&body, "claude:messages", "gemini-4-argon-contributor", "opencode_go", "gemini:generate_content", "/v1/messages", true, None, Some("key"), None, false,
        aether_ai_formats::formats::openai::responses::OpenAiResponsesReasoningReplayPolicy::OpenAiItemIds).unwrap();
    let model_parts = converted["contents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["role"] == "model")
        .unwrap()["parts"]
        .as_array()
        .unwrap()
        .clone();
    let kinds: Vec<_> = model_parts
        .iter()
        .map(|p| {
            if p.get("functionCall").is_some() {
                "call"
            } else if p["thought"] == true {
                "thought"
            } else {
                "text"
            }
        })
        .collect();
    assert_eq!(kinds, ["thought", "call"], "{model_parts:?}");
}
