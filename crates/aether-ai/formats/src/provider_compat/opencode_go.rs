//! OpenCode Go's documented native protocols, not downstream model aliases.

pub const BASE_URL: &str = "https://opencode.ai/inference/go/v1";

pub fn chat_requires_root_schema_wrapper(model: &str) -> bool {
    matches!(
        model,
        "deepseek-flash" | "deepseek-v4-flash" | "deepseek-v4.1-flash" | "deepseek-v4-pro"
    )
}

pub fn uses_gateway_response_history(model: &str) -> bool {
    matches!(
        model.trim(),
        "muse-spark-1.2-contributor" | "muse-spark-1.3-contributor"
    )
}

pub fn expand_go_native_response_history(
    body: &Value,
    model: &str,
    scope: Option<&str>,
) -> Result<Value, crate::FormatError> {
    if !uses_gateway_response_history(model)
        || body
            .get("previous_response_id")
            .and_then(Value::as_str)
            .is_none_or(|id| id.trim().is_empty())
    {
        return Ok(body.clone());
    }
    if scope.is_none_or(|scope| scope.trim().is_empty()) {
        return Err(crate::FormatError::UnsupportedField {
            format: "openai:responses".to_string(),
            field: "previous_response_id".to_string(),
            reason: "Go Muse continuation requires a client credential scope".to_string(),
        });
    }
    let lifted =
        crate::formats::claude::responses_tools::ClaudeResponsesTools::lift_lite_tools(body)?;
    crate::formats::openai::responses::history::expand_previous_response_for_chat(&lifted, scope)
        .map_err(|reason| crate::FormatError::UnsupportedField {
            format: "openai:responses".to_string(),
            field: "previous_response_id".to_string(),
            reason,
        })
}

pub fn uses_functionized_responses_tools(model: &str) -> bool {
    matches!(
        model.trim(),
        "grok-4.6" | "grok-4.7" | "muse-spark-1.2-contributor" | "muse-spark-1.3-contributor"
    )
}

pub fn adapt_native_responses_request_for_model(
    body: &mut Value,
    model: &str,
) -> Result<(), OpenAiResponsesRequestContractViolation> {
    // Transport event preferences cannot affect a complete JSON response.
    // Codex-shaped requests may retain this field when selecting sync mode.
    if body.get("stream").and_then(Value::as_bool) == Some(false) {
        body.as_object_mut().unwrap().remove("stream_options");
    }
    if uses_functionized_responses_tools(model) {
        if let Some(input) = body.get_mut("input").and_then(Value::as_array_mut) {
            for item in input {
                // Codex serializes an absent optional reasoning content as
                // null. xAI rejects that replay with a misleading blob error.
                // Omit only the absent field; keep the signed opaque state.
                if item["type"] == "reasoning" && item.get("content") == Some(&Value::Null) {
                    item.as_object_mut().unwrap().remove("content");
                }
            }
        }
        *body =
            crate::formats::claude::responses_tools::ClaudeResponsesTools::normalize_go_functionized_responses(body, model)
                .map_err(|_| OpenAiResponsesRequestContractViolation {
                    field: "tools",
                    reason: "Go Grok client tools cannot be represented as functions",
                })?;
        return Ok(());
    }
    // Hosted web search is not forwarded to any Go model (see `normalize_go_functionized_responses`);
    // the client's own tools stay.
    drop_hosted_web_search(body);
    adapt_native_responses_request(body)
}

fn is_hosted_web_search(tool: &Value) -> bool {
    matches!(
        tool["type"].as_str(),
        Some("web_search" | "web_search_preview" | "web_search_preview_2025_03_11")
    )
}

fn drop_hosted_web_search(body: &mut Value) {
    let Some(object) = body.as_object_mut() else {
        return;
    };
    if let Some(tools) = object.get_mut("tools").and_then(Value::as_array_mut) {
        tools.retain(|tool| !is_hosted_web_search(tool));
    }
    if object.get("tool_choice").is_some_and(is_hosted_web_search) {
        object.remove("tool_choice");
    }
    if let Some(input) = object.get_mut("input").and_then(Value::as_array_mut) {
        input.retain(|item| item["type"] != "web_search_call");
    }
}

/// Go's Messages backends are Anthropic-compatible, not Anthropic: they validate strictly and
/// refuse Claude-Code-only extensions (`context_management`, hosted tools, `builtin_tools`).
/// None of them changes what the model can do for a coding client, so they are removed.
fn strip_messages_extensions(body: &mut Value) {
    lift_system_role_messages(body);
    let Some(object) = body.as_object_mut() else {
        return;
    };
    for key in [
        "context_management",
        "builtin_tools",
        "container",
        "mcp_servers",
        "betas",
        "anthropic_beta",
    ] {
        object.remove(key);
    }
    let mut dropped_tool = false;
    if let Some(tools) = object.get_mut("tools").and_then(Value::as_array_mut) {
        let before = tools.len();
        tools.retain(|tool| {
            tool.get("type")
                .and_then(Value::as_str)
                .is_none_or(|kind| kind == "custom")
        });
        dropped_tool = tools.len() != before;
        if tools.is_empty() {
            object.remove("tools");
        }
    }
    if dropped_tool || !object.contains_key("tools") {
        let hosted_choice = object.get("tool_choice").is_some_and(|choice| {
            !matches!(
                choice["type"].as_str(),
                Some("auto" | "any" | "tool" | "none")
            ) || !object.contains_key("tools")
        });
        if hosted_choice {
            object.remove("tool_choice");
        }
    }
}

/// Claude Code injects mid-conversation `role: "system"` messages (agent lists, reminders).
/// Anthropic-compatible backends only know `user` and `assistant`, so their text joins the
/// top-level `system` prompt instead of being dropped.
fn lift_system_role_messages(body: &mut Value) {
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    if !messages
        .iter()
        .any(|m| matches!(m["role"].as_str(), Some("system" | "developer")))
    {
        return;
    }
    let mut lifted = Vec::new();
    messages.retain(|message| {
        if !matches!(message["role"].as_str(), Some("system" | "developer")) {
            return true;
        }
        match &message["content"] {
            Value::String(text) => lifted.push(json!({"type":"text", "text":text})),
            Value::Array(blocks) => lifted.extend(
                blocks
                    .iter()
                    .filter(|b| b["type"] == "text" && b["text"].is_string())
                    .map(|b| json!({"type":"text", "text":b["text"]})),
            ),
            _ => {}
        }
        false
    });
    if lifted.is_empty() {
        return;
    }
    let object = body.as_object_mut().unwrap();
    let mut system = match object.remove("system") {
        Some(Value::Array(blocks)) => blocks,
        Some(Value::String(text)) if !text.is_empty() => vec![json!({"type":"text", "text":text})],
        _ => Vec::new(),
    };
    system.extend(lifted);
    object.insert("system".to_string(), Value::Array(system));
}

pub fn adapt_messages_request(body: &mut Value, source: &Value, model: &str) {
    strip_messages_extensions(body);
    if !matches!(model, "qwen3.8-max" | "qwen3.8-flash") {
        return;
    }
    let thinking = body.get("thinking").is_some();
    let effort = body
        .get("output_config")
        .and_then(|v| v.get("effort"))
        .is_some();
    if !thinking || !effort {
        return;
    }
    // These two controls are mutually exclusive on Qwen 3.8. Prefer an explicit
    // token budget / disabled thinking; otherwise retain the requested effort.
    let explicit_budget = source
        .get("reasoning")
        .and_then(|r| r.get("budget_tokens"))
        .is_some()
        || source
            .get("thinking")
            .and_then(|r| r.get("budget_tokens"))
            .is_some();
    if explicit_budget || body["thinking"]["type"] == "disabled" {
        let config = body
            .get_mut("output_config")
            .and_then(Value::as_object_mut)
            .unwrap();
        config.remove("effort");
        if config.is_empty() {
            body.as_object_mut().unwrap().remove("output_config");
        }
    } else {
        body.as_object_mut().unwrap().remove("thinking");
    }
}

use crate::formats::openai::responses::request::OpenAiResponsesRequestContractViolation;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Default)]
struct NativeTools {
    // Always namespace outbound names, even when currently unique: tools loaded
    // in a later turn must not change the identity of an existing call.
    identities: BTreeMap<String, (Option<String>, String)>,
    tools: Vec<Value>,
}

impl NativeTools {
    fn from_body(body: &Value) -> Result<Self, &'static str> {
        let mut result = Self::default();
        let mut definitions = Vec::new();
        if let Some(input) = body.get("input").and_then(Value::as_array) {
            for item in input {
                if matches!(
                    item["type"].as_str(),
                    Some("additional_tools" | "tool_search_output")
                ) {
                    definitions.extend(
                        item.get("tools")
                            .and_then(Value::as_array)
                            .ok_or("invalid loaded tool definitions")?
                            .iter()
                            .cloned(),
                    );
                }
            }
        }
        if let Some(tools) = body.get("tools") {
            definitions.extend(
                tools
                    .as_array()
                    .ok_or("tools must be an array")?
                    .iter()
                    .cloned(),
            );
        }
        let mut unique = Vec::new();
        for tool in definitions {
            if !unique.contains(&tool) {
                unique.push(tool);
            }
        }
        for tool in unique {
            if tool["type"] == "namespace" {
                let namespace = tool["name"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("namespace name is required")?;
                let children = tool["tools"]
                    .as_array()
                    .ok_or("namespace tools must be an array")?;
                for child in children {
                    result.insert(child.clone(), Some(namespace.to_string()))?;
                }
            } else if matches!(tool["type"].as_str(), Some("function" | "custom")) {
                result.insert(tool, None)?;
            } else {
                result.tools.push(tool);
            }
        }
        Ok(result)
    }

    fn insert(&mut self, mut tool: Value, namespace: Option<String>) -> Result<(), &'static str> {
        if !matches!(tool["type"].as_str(), Some("function" | "custom")) {
            return Err("only client function/custom tools can be nested in a namespace");
        }
        let name = tool["name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("tool name is required")?
            .to_string();
        let alias = namespace
            .as_ref()
            .map_or_else(|| name.clone(), |ns| format!("{ns}__{name}"));
        if alias.len() > 64
            || !alias
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return Err("tool alias is not a valid upstream function name");
        }
        if self
            .identities
            .insert(alias.clone(), (namespace, name))
            .is_some()
        {
            return Err("ambiguous tool aliases; namespace/name collision");
        }
        tool["name"] = json!(alias);
        self.tools.push(tool);
        Ok(())
    }

    fn map_call(&self, item: &mut Value, restore: bool) -> Result<bool, &'static str> {
        if !matches!(
            item["type"].as_str(),
            Some("function_call" | "custom_tool_call" | "function" | "custom")
        ) {
            return Ok(false);
        }
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            return Ok(false);
        };
        if restore {
            let Some((namespace, original)) = self.identities.get(name) else {
                return Ok(false);
            };
            let changed = name != original
                || namespace
                    .as_ref()
                    .is_some_and(|ns| item["namespace"] != *ns);
            item["name"] = json!(original);
            if let Some(namespace) = namespace {
                item["namespace"] = json!(namespace);
            }
            Ok(changed)
        } else {
            let namespace = item.get("namespace").and_then(Value::as_str);
            let alias = namespace.map_or_else(|| name.to_string(), |ns| format!("{ns}__{name}"));
            if namespace.is_some() && !self.identities.contains_key(&alias) {
                return Err("unknown namespaced tool in history or tool choice");
            }
            item["name"] = json!(alias);
            item.as_object_mut().unwrap().remove("namespace");
            Ok(true)
        }
    }
}

pub fn adapt_native_responses_request(
    body: &mut Value,
) -> Result<(), OpenAiResponsesRequestContractViolation> {
    let error = |reason| OpenAiResponsesRequestContractViolation {
        field: "tools",
        reason,
    };
    let tools = NativeTools::from_body(body).map_err(error)?;
    if let Some(input) = body.get_mut("input").and_then(Value::as_array_mut) {
        let prefix = input
            .iter()
            .take_while(|i| i["type"] == "additional_tools")
            .count();
        input.drain(..prefix);
        for item in input {
            tools.map_call(item, false).map_err(error)?;
        }
    }
    if !tools.tools.is_empty() || body.get("tools").is_some() {
        body["tools"] = json!(tools.tools);
    }
    if let Some(choice) = body.get_mut("tool_choice") {
        tools.map_call(choice, false).map_err(error)?;
    }
    Ok(())
}

pub fn native_responses_tools_need_restore(context: &Value) -> bool {
    context["provider_type"] == "opencode_go"
        && context["provider_api_format"] == "openai:responses"
        && context["client_api_format"] == "openai:responses"
        && (crate::formats::claude::responses_tools::is_go_functionized_responses_provider(context)
            || context["mapped_model"]
                .as_str()
                .is_some_and(uses_gateway_response_history)
            || context.get("original_request_body").is_some_and(|body| {
                NativeTools::from_body(body).is_ok_and(|tools| {
                    tools
                        .identities
                        .values()
                        .any(|(namespace, _)| namespace.is_some())
                })
            }))
}

pub fn restore_native_responses_payload(
    body: &mut Value,
    context: &Value,
) -> Result<bool, &'static str> {
    if context["provider_type"] != "opencode_go"
        || context["provider_api_format"] != "openai:responses"
    {
        return Ok(false);
    }
    let Some(original) = context.get("original_request_body") else {
        return Ok(false);
    };
    if crate::formats::claude::responses_tools::is_go_functionized_responses_provider(context) {
        let response = if body.get("response").is_some() {
            &mut body["response"]
        } else {
            body
        };
        crate::formats::claude::responses_tools::ClaudeResponsesTools::from_report_context(context)
            .restore_response(response)
            .map_err(|_| "invalid Go Grok tool response wrapper")?;
        crate::formats::openai::responses::history::record_converted_response_history(
            context, response,
        );
        return Ok(true);
    }
    let tools = NativeTools::from_body(original)?;
    let mut changed = false;
    if let Some(item) = body.get_mut("item") {
        changed |= tools.map_call(item, true)?;
    }
    let response = if body.get("response").is_some() {
        &mut body["response"]
    } else {
        body
    };
    if let Some(output) = response.get_mut("output").and_then(Value::as_array_mut) {
        for item in output {
            changed |= tools.map_call(item, true)?;
        }
    }
    Ok(changed)
}

/// Map a presentation preference to instructions on endpoints without a native
/// verbosity parameter. This never alters tools, schemas or thinking controls.
pub fn adapt_chat_request(body: &mut serde_json::Value) -> Result<(), crate::FormatError> {
    let Some(verbosity) = body.get("verbosity").filter(|v| !v.is_null()).cloned() else {
        return Ok(());
    };
    let instruction = match verbosity.as_str() {
        Some("low") => "Keep the final answer concise. Still execute all tools needed to complete the task.",
        Some("medium") => "Use a moderate level of detail in the final answer. Still execute all tools needed to complete the task.",
        Some("high") => "Provide a detailed final answer. Still execute all tools needed to complete the task.",
        _ => return Err(crate::FormatError::InvalidEnumValue { format:"openai:chat".to_string(), field:"verbosity".to_string(), value:verbosity.to_string() }),
    };
    let messages = body
        .get_mut("messages")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| crate::FormatError::RequestEmitFailed {
            format: "openai:chat".to_string(),
        })?;
    messages.insert(
        0,
        serde_json::json!({"role":"system", "content":instruction}),
    );
    body.as_object_mut().unwrap().remove("verbosity");
    Ok(())
}

pub fn model_api_format(model: &str) -> Option<&'static str> {
    match model.trim() {
        "gemini-4-argon-contributor" => Some("gemini:generate_content"),
        "gpt-6-luna"
        | "gpt-5.6-luna"
        | "grok-4.6"
        | "grok-4.7"
        | "muse-spark-1.2-contributor"
        | "muse-spark-1.3-contributor" => Some("openai:responses"),
        "claude-haiku-5-5" | "minimax-m2.5" | "minimax-m2.7" | "minimax-m3" | "qwen3.6-plus"
        | "qwen3.7-max" | "qwen3.7-plus" | "qwen3.8-max" | "qwen3.8-flash" => {
            Some("claude:messages")
        }
        "glm-5.1"
        | "glm-5.2"
        | "glm-5.3"
        | "glm-5.3-flash"
        | "deepseek-flash"
        | "deepseek-v4-flash"
        | "deepseek-v4-flash-vision-exp"
        | "deepseek-v4.1-flash"
        | "deepseek-v4-pro"
        | "kimi-k2.6"
        | "kimi-k2.7-code"
        | "kimi-k3"
        | "mimo-v2.5"
        | "mimo-v2.5-pro"
        | "mimo-v2.6-flash"
        | "mimo-v2.6-pro"
        | "hy3"
        | "hy4-preview"
        | "space-bunny"
        | "longcat-2.0"
        | "longcat-2.5-preview-free"
        | "step-5-preview"
        | "step-5-preview-free"
        | "beam-free"
        | "cat-jam-free" => Some("openai:chat"),
        // A model listing does not establish its native protocol; see `inferred_model_api_format`
        // for the family rule used when the model is not in this table.
        _ => None,
    }
}

/// Protocol of a model: the table above, else the family rule, so a new Go model works with nothing but
/// the key. OpenCode itself does the same: every Go model is Chat unless it belongs to one of the
/// three vendor families that are served natively (OpenAI Responses, Anthropic Messages, Google).
/// A wrong guess is a deterministic 400 for that one model, never a wrong answer. Only plain
/// upstream ids are inferred: an alias with a path or a client prefix (`opencode-go/...`) is not.
pub fn inferred_model_api_format(model: &str) -> Option<&'static str> {
    if let Some(known) = model_api_format(model) {
        return Some(known);
    }
    let model = model.trim();
    let valid = !model.is_empty()
        && model.len() <= 80
        && model.starts_with(|c: char| c.is_ascii_alphanumeric())
        && model
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if !valid {
        return None;
    }
    let model = model.to_ascii_lowercase();
    Some(
        if model.starts_with("gpt-") || model.starts_with("grok-") || model.starts_with("muse-") {
            "openai:responses"
        } else if model.starts_with("claude-")
            || model.starts_with("minimax-")
            || model.starts_with("qwen")
        {
            "claude:messages"
        } else if model.starts_with("gemini-") {
            "gemini:generate_content"
        } else {
            "openai:chat"
        },
    )
}

pub fn adapt_gemini_request(
    body: &mut Value,
    request: &crate::protocol::canonical::CanonicalRequest,
) {
    for tool in body
        .get_mut("tools")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        for declaration in tool
            .get_mut("functionDeclarations")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            if let Some(object) = declaration.as_object_mut() {
                // The legacy serializer cleans JSON Schema into OpenAPI and
                // stringifies integer constraints. Use the canonical schema,
                // not that lossy intermediate, for the JSON Schema endpoint.
                let name = object.get("name").and_then(Value::as_str);
                let schema = request
                    .tools
                    .iter()
                    .find(|tool| Some(tool.name.as_str()) == name)
                    .and_then(|tool| tool.parameters.clone());
                if let Some(schema) = schema {
                    object.remove("parameters");
                    object.insert("parametersJsonSchema".to_string(), schema);
                }
            }
        }
    }
}

pub fn model_supports_api_format(model: &str, api_format: &str) -> bool {
    model_api_format(model)
        .is_none_or(|native| native == crate::normalize_api_format_alias(api_format))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_protocol_is_not_inferred_from_client_alias() {
        assert_eq!(model_api_format("gpt-6-luna"), Some("openai:responses"));
        assert_eq!(
            model_api_format("claude-haiku-5-5"),
            Some("claude:messages")
        );
        assert_eq!(model_api_format("space-bunny"), Some("openai:chat"));
        assert_eq!(
            model_api_format("gemini-4-argon-contributor"),
            Some("gemini:generate_content")
        );
        assert_eq!(model_api_format("gpt-6.1-sol"), None);
        assert_eq!(model_api_format("opencode-go/glm-5.3-flash"), None);
        assert_eq!(inferred_model_api_format("opencode-go/glm-5.3-flash"), None);
        assert_eq!(inferred_model_api_format("a b"), None);
    }

    #[test]
    fn known_models_only_match_native_endpoint_not_compact() {
        assert!(model_supports_api_format("glm-5.3-flash", "OPENAI:CHAT"));
        assert!(!model_supports_api_format(
            "glm-5.3-flash",
            "openai:responses"
        ));
        assert!(!model_supports_api_format(
            "gpt-6-luna",
            "openai:responses:compact"
        ));
        // Unlisted models follow OpenCode's own family rule instead of needing a manual mapping.
        assert_eq!(
            inferred_model_api_format("future-model"),
            Some("openai:chat")
        );
        assert_eq!(
            inferred_model_api_format("qwen4-max"),
            Some("claude:messages")
        );
        assert_eq!(
            inferred_model_api_format("gpt-7-luna"),
            Some("openai:responses")
        );
        assert_eq!(inferred_model_api_format("omen-alpha"), Some("openai:chat"));
        assert_eq!(model_api_format("future-model"), None);
        assert!(model_supports_api_format(
            "opencode-go/x",
            "claude:messages"
        ));
    }
}
