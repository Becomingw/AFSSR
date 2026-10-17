use std::collections::BTreeMap;

use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use serde_json::{json, Value};

use crate::formats::{context::FormatError, id::FormatId, openai::namespace::NamespaceToolAliases};
use crate::protocol::canonical::openai_responses_tools_to_canonical;
use crate::provider_compat::codex_code_mode::CodeModeHelper;

const CLAUDE_THINKING_CARRIER_PREFIX: &str = "aether-claude-thinking-v1:";
const MAX_THINKING_CARRIER_BYTES: usize = 1024 * 1024;
const TOOL_SEARCH_ALIAS: &str = "aether_tool_search";
const WEB_CONTEXT_PREFIX: &str = "aether-claude-web-v1:";
const SCHEMA_INPUT: &str = "aether_input";

pub(crate) fn web_sources(result: &Value) -> Vec<Value> {
    result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|r| r["type"] == "web_search_result")
        .filter_map(|r| Some(json!({"type":"url","url":r.get("url")?.as_str()?})))
        .collect()
}

pub(crate) fn encode_web_context(blocks: &[Value], ids: &[String]) -> Result<String, &'static str> {
    let bytes = serde_json::to_vec(&json!({"blocks":blocks,"item_ids":ids}))
        .map_err(|_| "invalid web context")?;
    if bytes.len() > MAX_THINKING_CARRIER_BYTES {
        return Err("web context exceeds limit");
    }
    let carrier = format!("{WEB_CONTEXT_PREFIX}{}", STANDARD_NO_PAD.encode(bytes));
    decode_web_context(&carrier).ok_or("invalid native web context")?;
    Ok(carrier)
}

fn decode_web_context(carrier: &str) -> Option<(Value, Vec<String>)> {
    let data = carrier.strip_prefix(WEB_CONTEXT_PREFIX)?;
    if data.len() > MAX_THINKING_CARRIER_BYTES.div_ceil(3) * 4 {
        return None;
    }
    let bytes = STANDARD_NO_PAD.decode(data).ok()?;
    if bytes.len() > MAX_THINKING_CARRIER_BYTES {
        return None;
    }
    let payload: Value = serde_json::from_slice(&bytes).ok()?;
    let object = payload.as_object().filter(|o| o.len() == 2)?;
    let blocks = object.get("blocks")?.as_array().filter(|b| b.len() == 2)?;
    let use_block = blocks[0].as_object().filter(|b| {
        b.len() >= 4
            && b.len() <= 5
            && b.keys()
                .all(|k| matches!(k.as_str(), "type" | "id" | "name" | "input" | "caller"))
    })?;
    if use_block
        .get("caller")
        .is_some_and(|v| v != &json!({"type":"direct"}))
    {
        return None;
    }
    if use_block.get("type")? != "server_tool_use" || use_block.get("name")? != "web_search" {
        return None;
    }
    let id = use_block.get("id")?.as_str().filter(|s| !s.is_empty())?;
    let input = use_block
        .get("input")?
        .as_object()
        .filter(|i| i.len() == 1)?;
    input.get("query")?.as_str()?;
    let result = blocks[1].as_object().filter(|r| {
        (r.len() == 3 || r.len() == 4)
            && r.keys()
                .all(|k| matches!(k.as_str(), "type" | "tool_use_id" | "content" | "caller"))
            && r.get("caller")
                .is_none_or(|v| v == &json!({"type":"direct"}))
    })?;
    if result.get("type")? != "web_search_tool_result" || result.get("tool_use_id")?.as_str()? != id
    {
        return None;
    }
    let content = result.get("content")?;
    if let Some(items) = content.as_array() {
        for item in items {
            let r = item.as_object()?;
            if r.get("type")? != "web_search_result"
                || r.keys().any(|k| {
                    !matches!(
                        k.as_str(),
                        "type" | "url" | "title" | "encrypted_content" | "page_age"
                    )
                })
            {
                return None;
            }
            for key in ["url", "title", "encrypted_content"] {
                r.get(key)?.as_str()?;
            }
        }
    } else if content.get("type")? != "web_search_tool_result_error"
        || !content.get("error_code")?.is_string()
    {
        return None;
    }
    let ids = object
        .get("item_ids")?
        .as_array()?
        .iter()
        .map(|v| v.as_str().map(str::to_string))
        .collect::<Option<Vec<_>>>()?;
    if ids.len() != 1 || ids[0].is_empty() {
        return None;
    }
    Some((json!(blocks), ids))
}

pub(crate) fn restore_native_web_response(
    body: &Value,
    response: &mut Value,
) -> Result<(), FormatError> {
    let Some(blocks) = body.get("content").and_then(Value::as_array) else {
        return Ok(());
    };
    let Some(output) = response.get_mut("output").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for result in blocks
        .iter()
        .filter(|b| b["type"] == "web_search_tool_result")
    {
        let id = result
            .get("tool_use_id")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("content[].tool_use_id", "web result requires id"))?;
        let native = blocks
            .iter()
            .find(|b| b["type"] == "server_tool_use" && b["id"] == id && b["name"] == "web_search")
            .ok_or_else(|| invalid("content[]", "web result has no matching server use"))?;
        let carrier = encode_web_context(&[native.clone(), result.clone()], &[id.to_string()])
            .map_err(|r| invalid("content[]", r))?;
        if let Some(call) = output
            .iter_mut()
            .find(|i| i["type"] == "web_search_call" && i["id"] == id)
        {
            call["action"]["sources"] = json!(web_sources(result));
        }
        let index = output
            .iter()
            .position(|i| i["type"] == "function_call_output" && i["call_id"] == id)
            .ok_or_else(|| invalid("output[]", "web result mapping is missing"))?;
        output[index] = json!({"type":"reasoning","id":format!("rs_web_{id}"),"summary":[],"status":"completed","encrypted_content":carrier});
    }
    for text in blocks.iter().filter(|b| b["type"] == "text") {
        for citation in text
            .get("citations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if citation["type"] != "web_search_result_location" {
                continue;
            }
            for item in output.iter_mut().filter(|i| i["type"] == "message") {
                for part in item["content"].as_array_mut().into_iter().flatten() {
                    let value = part.get("text").and_then(Value::as_str).unwrap_or_default();
                    let cited = citation
                        .get("cited_text")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if cited.is_empty() {
                        continue;
                    }
                    let matched = value
                        .rfind(cited)
                        .map(|i| (i, cited.chars().count()))
                        .or_else(|| {
                            let original = text.get("text")?.as_str()?;
                            value.rfind(original).map(|i| (i, original.chars().count()))
                        });
                    if let Some((index, length)) = matched {
                        let start = value[..index].chars().count();
                        let annotation = json!({"type":"url_citation","url":citation["url"],"title":citation["title"],
                            "start_index":start,"end_index":start+length});
                        if part.get("annotations").is_none() {
                            part["annotations"] = json!([]);
                        }
                        part["annotations"].as_array_mut().unwrap().push(annotation);
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn encode_thinking_carrier(blocks: &[Value]) -> Option<String> {
    if blocks.is_empty() {
        return None;
    }
    let data = serde_json::to_vec(blocks).ok()?;
    (data.len() <= MAX_THINKING_CARRIER_BYTES).then(|| {
        format!(
            "{CLAUDE_THINKING_CARRIER_PREFIX}{}",
            STANDARD_NO_PAD.encode(data)
        )
    })
}

pub(crate) fn decode_thinking_carrier(carrier: &str) -> Option<Value> {
    if carrier.starts_with(WEB_CONTEXT_PREFIX) {
        return decode_web_context(carrier).map(|v| v.0);
    }
    let payload = carrier.strip_prefix(CLAUDE_THINKING_CARRIER_PREFIX)?;
    if payload.len() > MAX_THINKING_CARRIER_BYTES.div_ceil(3) * 4 {
        return None;
    }
    let data = STANDARD_NO_PAD.decode(payload).ok()?;
    if data.len() > MAX_THINKING_CARRIER_BYTES {
        return None;
    }
    let value: Value = serde_json::from_slice(&data).ok()?;
    let blocks = value.as_array().filter(|b| !b.is_empty())?;
    for block in blocks {
        let object = block.as_object()?;
        match object.get("type").and_then(Value::as_str)? {
            "thinking"
                if object.len() == 3
                    && object.get("thinking").is_some_and(Value::is_string)
                    && object
                        .get("signature")
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.is_empty()) => {}
            "redacted_thinking"
                if object.len() == 2
                    && object
                        .get("data")
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.is_empty()) => {}
            _ => return None,
        }
    }
    Some(value)
}

pub(crate) fn is_claude_provider(context: &Value) -> bool {
    context
        .get("provider_stream_event_api_format")
        .or_else(|| context.get("provider_stream_api_format"))
        .or_else(|| context.get("provider_api_format"))
        .and_then(Value::as_str)
        .and_then(FormatId::parse)
        == Some(FormatId::ClaudeMessages)
}

pub(crate) fn is_go_chat_provider(context: &Value) -> bool {
    context
        .get("provider_type")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind.eq_ignore_ascii_case("opencode_go"))
        && context
            .get("provider_stream_event_api_format")
            .or_else(|| context.get("provider_stream_api_format"))
            .or_else(|| context.get("provider_api_format"))
            .and_then(Value::as_str)
            .and_then(FormatId::parse)
            == Some(FormatId::OpenAiChat)
}

pub(crate) fn is_go_functionized_responses_provider(context: &Value) -> bool {
    context["provider_type"] == "opencode_go"
        && context["provider_api_format"] == "openai:responses"
        && context
            .get("mapped_model")
            .and_then(Value::as_str)
            .is_some_and(crate::provider_compat::opencode_go::uses_functionized_responses_tools)
        && (context["mapped_model"]
            .as_str()
            .is_some_and(crate::provider_compat::opencode_go::uses_gateway_response_history)
            || context.get("original_request_body").is_some_and(|body| {
                ClaudeResponsesTools::from_body(&ClaudeResponsesTools::without_native_web_tools(
                    body,
                ))
                .is_ok_and(|tools| {
                    tools.identities.values().any(|tool| {
                        tool.custom
                            || tool.namespace.is_some()
                            || tool.tool_search
                            || tool.schema_wrapped
                    })
                })
            }))
}

pub(crate) fn is_go_gemini_provider(context: &Value) -> bool {
    context["provider_type"] == "opencode_go"
        && context
            .get("provider_stream_event_api_format")
            .or_else(|| context.get("provider_stream_api_format"))
            .or_else(|| context.get("provider_api_format"))
            .and_then(Value::as_str)
            .and_then(FormatId::parse)
            == Some(FormatId::GeminiGenerateContent)
}

#[derive(Clone, Debug)]
pub(crate) struct ResponsesToolIdentity {
    pub name: String,
    pub namespace: Option<String>,
    pub custom: bool,
    pub tool_search: bool,
    pub schema_wrapped: bool,
    pub code_mode_helper: Option<CodeModeHelper>,
    pub raw_code_mode_input: bool,
}

#[derive(Default, Debug)]
pub(crate) struct ClaudeResponsesTools {
    identities: BTreeMap<String, ResponsesToolIdentity>,
    aliases: BTreeMap<(Option<String>, String), String>,
    tools: Vec<Value>,
    web_tools: Vec<Value>,
}

fn invalid(field: &str, reason: &str) -> FormatError {
    FormatError::LossyConversionBlocked {
        source_format: "openai:responses".to_string(),
        target_format: "claude:messages".to_string(),
        field: field.to_string(),
        reason: reason.to_string(),
    }
}

fn wrap_function_schema(tool: &mut Value) -> Result<bool, FormatError> {
    let function = if tool.get("function").is_some() {
        tool.get_mut("function").unwrap()
    } else {
        tool
    };
    let Some(schema) = function.get("parameters") else {
        return Ok(false);
    };
    if !["oneOf", "anyOf", "allOf"]
        .iter()
        .any(|key| schema.get(*key).is_some())
    {
        return Ok(false);
    }
    let mut nested = schema.clone();
    relocate_schema_refs(&mut nested, schema)?;
    function["parameters"] = json!({
        "type":"object", "properties":{SCHEMA_INPUT:nested},
        "required":[SCHEMA_INPUT], "additionalProperties":false,
    });
    let description = function
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    function["description"] = json!(format!(
        "{description}\nPut the original tool arguments in the {SCHEMA_INPUT} object. The gateway restores the original arguments for the client."
    ));
    Ok(true)
}

// Visit schema positions only: examples/defaults/const may contain literal
// objects named $ref and must not be rewritten as schema references.
fn relocate_schema_refs(schema: &mut Value, root: &Value) -> Result<(), FormatError> {
    let Some(object) = schema.as_object_mut() else {
        return Ok(());
    };
    if [
        "$id",
        "$anchor",
        "$dynamicAnchor",
        "$dynamicRef",
        "$recursiveRef",
        "$recursiveAnchor",
    ]
    .iter()
    .any(|key| object.contains_key(*key))
    {
        return Err(invalid(
            "tools[].parameters",
            "root combinator wrapping cannot relocate scoped or dynamic schema references",
        ));
    }
    if let Some(reference) = object.get_mut("$ref") {
        let value = reference.as_str().ok_or_else(|| {
            invalid(
                "tools[].parameters.$ref",
                "expected a JSON pointer reference",
            )
        })?;
        let suffix = value
            .strip_prefix('#')
            .filter(|suffix| suffix.is_empty() || suffix.starts_with('/'))
            .ok_or_else(|| {
                invalid(
                    "tools[].parameters.$ref",
                    "root combinator wrapping requires local JSON pointer references",
                )
            })?;
        if root.pointer(suffix).is_none() {
            return Err(invalid(
                "tools[].parameters.$ref",
                "schema reference does not resolve",
            ));
        }
        *reference = json!(format!("#/properties/{SCHEMA_INPUT}{suffix}"));
    }
    for key in [
        "$defs",
        "definitions",
        "properties",
        "patternProperties",
        "dependentSchemas",
    ] {
        if let Some(children) = object.get_mut(key).and_then(Value::as_object_mut) {
            for child in children.values_mut() {
                relocate_schema_refs(child, root)?;
            }
        }
    }
    if let Some(dependencies) = object
        .get_mut("dependencies")
        .and_then(Value::as_object_mut)
    {
        for child in dependencies.values_mut().filter(|value| value.is_object()) {
            relocate_schema_refs(child, root)?;
        }
    }
    for key in ["allOf", "anyOf", "oneOf", "prefixItems"] {
        if let Some(children) = object.get_mut(key).and_then(Value::as_array_mut) {
            for child in children {
                relocate_schema_refs(child, root)?;
            }
        }
    }
    for key in [
        "items",
        "additionalItems",
        "additionalProperties",
        "unevaluatedProperties",
        "unevaluatedItems",
        "contains",
        "propertyNames",
        "not",
        "if",
        "then",
        "else",
        "contentSchema",
    ] {
        if let Some(child) = object.get_mut(key) {
            if let Some(children) = child.as_array_mut() {
                for child in children {
                    relocate_schema_refs(child, root)?;
                }
            } else {
                relocate_schema_refs(child, root)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn decode_schema_arguments(arguments: &str) -> Result<String, &'static str> {
    let value: Value =
        serde_json::from_str(arguments).map_err(|_| "wrapped tool arguments must be valid JSON")?;
    let input = value
        .as_object()
        .filter(|o| o.len() == 1)
        .and_then(|o| o.get(SCHEMA_INPUT))
        .filter(|v| v.is_object())
        .ok_or("wrapped tool arguments must contain only the aether_input object")?;
    Ok(input.to_string())
}

fn wrap_history_arguments(item: &mut Value) -> Result<(), FormatError> {
    let arguments = item
        .get("arguments")
        .and_then(Value::as_str)
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .filter(Value::is_object)
        .ok_or_else(|| {
            invalid(
                "input[].arguments",
                "wrapped tool history requires JSON object arguments",
            )
        })?;
    item["arguments"] = json!(json!({SCHEMA_INPUT:arguments}).to_string());
    Ok(())
}

fn custom_as_function(tool: &Value) -> Result<Value, FormatError> {
    let Some(name) = tool
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    else {
        return Err(invalid("tools[].name", "custom tool must have a name"));
    };
    if tool.as_object().is_none_or(|o| {
        o.keys()
            .any(|k| !matches!(k.as_str(), "type" | "name" | "description" | "format"))
    }) {
        return Err(invalid(
            "tools[]",
            "custom tool contains unsupported fields",
        ));
    }
    let original_description = tool
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut description = format!(
        "This API exposes a JSON function wrapper, not the raw custom-tool interface described below. Call it with exactly one argument: {{\"input\":\"the original freeform payload\"}}. The original tool instructions and grammar apply to the string value of input, not to the outer JSON object. Helpers mentioned in the description are not separate top-level tools unless they have their own supplied tool definition.\n\n{original_description}"
    );
    description.push_str("\nPass the exact freeform tool payload in the input string; do not JSON-encode it inside that string.");
    if let Some(format) = tool.get("format") {
        match format.get("type").and_then(Value::as_str) {
            Some("text") => {}
            Some("grammar") => {
                let syntax = format
                    .get("syntax")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("tools[].format", "grammar requires syntax"))?;
                let definition = format
                    .get("definition")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("tools[].format", "grammar requires definition"))?;
                description.push_str(&format!(
                    "\nThe input string must obey this {syntax} grammar:\n{definition}"
                ));
            }
            _ => return Err(invalid("tools[].format", "unsupported custom tool format")),
        }
    }
    Ok(
        json!({"type":"function", "name":name, "description":description,
        "parameters":{"type":"object", "properties":{"input":{"type":"string",
            "description":"Exact original custom-tool payload. Serialize it as one JSON string; do not replace this argument with command parameters or a nested object."}},
        "required":["input"], "additionalProperties":false}}),
    )
}

fn native_web_tool(tool: &Value) -> Result<Value, FormatError> {
    let object = tool.as_object().unwrap();
    if object.keys().any(|k| {
        !matches!(
            k.as_str(),
            "type" | "external_web_access" | "search_content_types" | "filters" | "user_location"
        )
    }) {
        return Err(invalid(
            "tools[].web_search",
            "unsupported native Claude web-search option",
        ));
    }
    if tool.get("external_web_access").is_some_and(|v| v != true) {
        return Err(invalid(
            "tools[].external_web_access",
            "Claude web search cannot provide offline-only search",
        ));
    }
    if let Some(types) = tool.get("search_content_types").filter(|v| !v.is_null()) {
        // Codex advertises both modalities even for ordinary tool requests.
        // Keep native text search available; never fabricate image results.
        let supports_text = types.as_array().is_some_and(|items| {
            items
                .iter()
                .all(|v| matches!(v.as_str(), Some("text" | "image")))
                && (items.is_empty() || items.iter().any(|v| v == "text"))
        });
        if !supports_text {
            return Err(invalid(
                "tools[].search_content_types",
                "Claude web search requires text eligibility; image-only search is unsupported",
            ));
        }
    }
    let mut native = json!({"type":"web_search_20250305", "name":"web_search"});
    if let Some(filters) = tool.get("filters") {
        let allowed = filters
            .as_object()
            .filter(|f| f.len() == 1)
            .and_then(|f| f.get("allowed_domains"))
            .filter(|v| {
                v.as_array()
                    .is_some_and(|items| !items.is_empty() && items.iter().all(Value::is_string))
            })
            .ok_or_else(|| {
                invalid(
                    "tools[].filters",
                    "Claude mapping requires nonempty allowed_domains",
                )
            })?;
        native["allowed_domains"] = allowed.clone();
    }
    if let Some(location) = tool.get("user_location").filter(|v| !v.is_null()) {
        let valid = location.as_object().is_some_and(|l| {
            l.get("type").and_then(Value::as_str) == Some("approximate")
                && l.keys().all(|k| {
                    matches!(
                        k.as_str(),
                        "type" | "city" | "region" | "country" | "timezone"
                    )
                })
                && l.iter().all(|(_, v)| v.is_string())
                && l.len() > 1
        });
        if !valid {
            return Err(invalid(
                "tools[].user_location",
                "unsupported Claude web-search location",
            ));
        }
        native["user_location"] = location.clone();
    }
    Ok(native)
}

fn collected_tools(body: &Value) -> Result<Vec<Value>, FormatError> {
    let mut tools = Vec::new();
    if let Some(input) = body.get("input").and_then(Value::as_array) {
        for item in input.iter().filter(|item| {
            matches!(
                item["type"].as_str(),
                Some("additional_tools" | "tool_search_output")
            )
        }) {
            let additional = item["type"] == "additional_tools";
            if (additional && item.get("role").and_then(Value::as_str) != Some("developer"))
                || (!additional && item["execution"] != "client")
                || item
                    .get("id")
                    .is_some_and(|id| !id.is_null() && !id.is_string())
                || item.as_object().is_none_or(|o| {
                    o.keys().any(|k| {
                        if additional {
                            !matches!(k.as_str(), "type" | "role" | "tools" | "id")
                        } else {
                            !matches!(
                                k.as_str(),
                                "type" | "tools" | "id" | "execution" | "status" | "call_id"
                            )
                        }
                    })
                })
            {
                return Err(invalid(
                    "input[].additional_tools",
                    "unsupported additional_tools envelope",
                ));
            }
            tools.extend(
                item.get("tools")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        invalid("input[].additional_tools.tools", "expected tools array")
                    })?
                    .iter()
                    .cloned(),
            );
        }
    }
    if let Some(existing) = body.get("tools") {
        tools.extend(
            existing
                .as_array()
                .ok_or_else(|| invalid("tools", "expected tools array"))?
                .iter()
                .cloned(),
        );
    }
    let mut unique = Vec::new();
    for tool in tools {
        if !unique.contains(&tool) {
            unique.push(tool);
        }
    }
    Ok(unique)
}

impl ClaudeResponsesTools {
    pub(crate) fn lift_lite_tools(body: &Value) -> Result<Value, FormatError> {
        let prefix = body
            .get("input")
            .and_then(Value::as_array)
            .map_or(0, |items| {
                items
                    .iter()
                    .take_while(|item| item["type"] == "additional_tools")
                    .count()
            });
        if prefix == 0 {
            return Ok(body.clone());
        }
        let tools = collected_tools(body)?;
        let mut lifted = body.clone();
        lifted["input"].as_array_mut().unwrap().drain(..prefix);
        lifted["tools"] = json!(tools);
        Ok(lifted)
    }

    pub(crate) fn from_body(body: &Value) -> Result<Self, FormatError> {
        Self::from_body_with_schema_wrapping(body, true)
    }

    fn from_body_with_schema_wrapping(
        body: &Value,
        wrap_schema: bool,
    ) -> Result<Self, FormatError> {
        let mut original = Vec::new();
        let mut web_tools = Vec::new();
        for tool in collected_tools(body)? {
            if matches!(
                tool["type"].as_str(),
                Some("web_search" | "web_search_preview" | "web_search_preview_2025_03_11")
            ) {
                web_tools.push(native_web_tool(&tool)?);
            } else {
                original.push(tool);
            }
        }
        if web_tools.len() > 1 {
            return Err(invalid("tools[]", "duplicate native web-search tools"));
        }
        let mut functionized = original.clone();
        for tool in &mut functionized {
            match tool.get("type").and_then(Value::as_str) {
                Some("custom") => *tool = custom_as_function(tool)?,
                Some("namespace") => {
                    let children = tool
                        .get_mut("tools")
                        .and_then(Value::as_array_mut)
                        .ok_or_else(|| {
                            invalid("tools[].tools", "namespace requires child tools")
                        })?;
                    for child in children {
                        if child.get("type").and_then(Value::as_str) == Some("custom") {
                            *child = custom_as_function(child)?;
                        }
                    }
                }
                Some("function") => {}
                Some("tool_search") if tool["execution"] == "client" => {
                    if tool.as_object().unwrap().keys().any(|k| {
                        !matches!(
                            k.as_str(),
                            "type" | "execution" | "description" | "parameters"
                        )
                    }) {
                        return Err(invalid("tools[]", "unsupported client tool-search fields"));
                    }
                    let parameters = tool
                        .get("parameters")
                        .filter(|p| p.is_object())
                        .ok_or_else(|| {
                            invalid(
                                "tools[].parameters",
                                "client tool search requires an object schema",
                            )
                        })?;
                    *tool = json!({"type":"function", "name":TOOL_SEARCH_ALIAS,
                        "description":tool.get("description").and_then(Value::as_str).unwrap_or("Search for and load tools available to the client."),
                        "parameters":parameters});
                }
                _ => {
                    return Err(invalid(
                        "tools[]",
                        &format!(
                            "this Responses tool type has no Claude client-tool mapping: {}",
                            tool.get("type")
                                .and_then(Value::as_str)
                                .unwrap_or("missing")
                        ),
                    ))
                }
            }
        }
        let canonical =
            openai_responses_tools_to_canonical(Some(&Value::Array(functionized.clone())))
                .unwrap_or_default();
        let namespaces = NamespaceToolAliases::from_canonical_tools(&canonical);
        if namespaces.has_invalid_namespace_tools() {
            return Err(invalid(
                "tools[]",
                "invalid or ambiguous namespace tool definition",
            ));
        }
        let mut result = Self {
            web_tools,
            ..Self::default()
        };
        for (index, tool) in functionized.into_iter().enumerate() {
            if namespaces.is_namespace_tool(index) {
                for child in namespaces.tools_for_source(index) {
                    let raw = &original[index]["tools"][child.source_child_index];
                    let identity = ResponsesToolIdentity {
                        name: child.name.clone(),
                        namespace: Some(child.namespace.clone()),
                        custom: raw["type"] == "custom",
                        tool_search: false,
                        schema_wrapped: false,
                        code_mode_helper: None,
                        raw_code_mode_input: false,
                    };
                    let function = child.to_openai_chat_tool()["function"].clone();
                    let mut flattened = function;
                    flattened["type"] = json!("function");
                    result.insert(child.chat_name.clone(), identity, flattened, wrap_schema)?;
                }
            } else {
                let name = tool
                    .get("name")
                    .and_then(Value::as_str)
                    .or_else(|| {
                        tool.get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(Value::as_str)
                    })
                    .ok_or_else(|| invalid("tools[].name", "function tool requires name"))?
                    .to_string();
                result.insert(
                    name.clone(),
                    ResponsesToolIdentity {
                        name,
                        namespace: None,
                        custom: original[index]["type"] == "custom",
                        tool_search: original[index]["type"] == "tool_search",
                        schema_wrapped: false,
                        code_mode_helper: None,
                        raw_code_mode_input: false,
                    },
                    tool,
                    wrap_schema,
                )?;
            }
        }
        Ok(result)
    }

    pub(crate) fn attach_native_web_tools(
        body: &Value,
        request: &mut crate::protocol::canonical::CanonicalRequest,
    ) -> Result<(), FormatError> {
        let tools = Self::from_body(body)?;
        if tools.web_tools.is_empty() {
            return Ok(());
        }
        if tools.identities.contains_key("web_search") {
            return Err(invalid(
                "tools[].name",
                "native web search conflicts with a client tool",
            ));
        }
        let extension = request
            .extensions
            .entry("claude".to_string())
            .or_insert_with(|| json!({}));
        extension["builtin_tools"] = json!(tools.web_tools);
        Ok(())
    }

    fn insert(
        &mut self,
        alias: String,
        mut identity: ResponsesToolIdentity,
        mut tool: Value,
        wrap_schema: bool,
    ) -> Result<(), FormatError> {
        if self.identities.contains_key(&alias) {
            return Err(invalid("tools[].name", "duplicate tool identity"));
        }
        identity.schema_wrapped = wrap_schema && wrap_function_schema(&mut tool)?;
        self.aliases.insert(
            (identity.namespace.clone(), identity.name.clone()),
            alias.clone(),
        );
        self.identities.insert(alias, identity);
        self.tools.push(tool);
        Ok(())
    }

    pub(crate) fn from_report_context(context: &Value) -> Self {
        if !is_claude_provider(context)
            && !is_go_chat_provider(context)
            && !is_go_gemini_provider(context)
            && !is_go_functionized_responses_provider(context)
        {
            return Self::default();
        }
        context
            .get("original_request_body")
            .and_then(|body| {
                let lifted = Self::lift_lite_tools(body).ok()?;
                if is_go_functionized_responses_provider(context) {
                    // Muse needs scoped full replay; other native Responses
                    // providers keep their own upstream history semantics.
                    let lifted =
                        crate::provider_compat::opencode_go::expand_go_native_response_history(
                            &lifted,
                            context["mapped_model"].as_str().unwrap_or_default(),
                            context["api_key_id"].as_str(),
                        )
                        .ok()?;
                    return Self::from_go_body(
                        &Self::without_native_web_tools(&lifted),
                        context["mapped_model"] != "muse-spark-1.3-contributor",
                    )
                    .ok();
                }
                let expanded =
                    crate::formats::openai::responses::history::expand_previous_response_for_chat(
                        &lifted,
                        context.get("api_key_id").and_then(Value::as_str),
                    )
                    .ok()?;
                if context["provider_type"] == "opencode_go" {
                    let expanded = if is_go_gemini_provider(context) {
                        Self::without_native_web_tools(&expanded)
                    } else {
                        expanded
                    };
                    let wrap_schema = if is_go_chat_provider(context) {
                        crate::provider_compat::opencode_go::chat_requires_root_schema_wrapper(
                            context["mapped_model"].as_str().unwrap_or_default(),
                        )
                    } else {
                        !matches!(
                            context["mapped_model"].as_str(),
                            Some("qwen3.7-max" | "minimax-m2.5")
                        )
                    };
                    let mut tools = Self::from_go_body(&expanded, wrap_schema).ok()?;
                    if is_go_chat_provider(context)
                        && matches!(
                            context["mapped_model"].as_str(),
                            Some("mimo-v2.5" | "mimo-v2.6-flash" | "mimo-v2.6-pro")
                        )
                    {
                        for namespace in collected_tools(&expanded).ok()? {
                            if namespace["type"] != "namespace" || namespace["name"] != "functions"
                            {
                                continue;
                            }
                            for child in namespace["tools"].as_array().into_iter().flatten() {
                                if CodeModeHelper::advertised(child).is_empty() {
                                    continue;
                                }
                                for identity in tools.identities.values_mut().filter(|i| {
                                    i.custom
                                        && i.name == "exec"
                                        && i.namespace.as_deref() == Some("functions")
                                        && i.code_mode_helper.is_none()
                                }) {
                                    identity.raw_code_mode_input = true;
                                }
                            }
                        }
                    }
                    Some(tools)
                } else {
                    Self::from_body(&expanded).ok()
                }
            })
            .unwrap_or_default()
    }

    pub(crate) fn identity(&self, alias: &str) -> Option<&ResponsesToolIdentity> {
        self.identities.get(alias)
    }

    fn alias_for_item(&self, item: &Value) -> Result<Option<&str>, FormatError> {
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            return Ok(None);
        };
        let namespace = item
            .get("namespace")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(alias) = self.aliases.get(&(namespace.clone(), name.to_string())) {
            return Ok(Some(alias));
        }
        if namespace.is_some() {
            return Err(invalid(
                "input[].namespace",
                "unknown namespaced tool identity",
            ));
        }
        let matches = self
            .identities
            .iter()
            .filter(|(_, tool)| tool.name == name && tool.code_mode_helper.is_none())
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [(alias, _)] => Ok(Some(alias.as_str())),
            [] => Ok(None),
            _ => Err(invalid(
                "input[].name",
                "ambiguous tool identity requires namespace",
            )),
        }
    }

    fn from_go_body(body: &Value, wrap_schema: bool) -> Result<Self, FormatError> {
        let mut tools = Self::from_body_with_schema_wrapping(body, wrap_schema)?;
        let lifted = Self::lift_lite_tools(body)?;
        for namespace in lifted
            .get("tools")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if namespace["type"] != "namespace" || namespace["name"] != "functions" {
                continue;
            }
            for tool in namespace
                .get("tools")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let helpers = CodeModeHelper::advertised(tool)
                    .into_iter()
                    .filter(|helper| !tools.identities.contains_key(helper.name()))
                    .collect::<Vec<_>>();
                if helpers.contains(&CodeModeHelper::ListTools) {
                    if let Some(alias) = tools
                        .aliases
                        .get(&(Some("functions".to_string()), "exec".to_string()))
                    {
                        if let Some(dispatcher) = tools
                            .tools
                            .iter_mut()
                            .find(|definition| definition["name"] == *alias)
                        {
                            let original = dispatcher["description"].as_str().unwrap_or_default();
                            let names = helpers
                                .iter()
                                .map(|helper| helper.name())
                                .collect::<Vec<_>>()
                                .join(", ");
                            dispatcher["description"] = json!(format!("For individual client tool operations, prefer the separately supplied JSON {names} functions; they do not require JavaScript. This generic dispatcher is for JavaScript orchestration: its input must contain actual JavaScript, not an empty object or an empty block. Do not use an empty dispatcher call to request tool discovery.\n\n{original}"));
                        }
                    }
                }
                for helper in helpers {
                    // Keep the original (namespace, name) -> alias mapping:
                    // helpers are additional native identities for the same exec.
                    tools.identities.insert(
                        helper.name().to_string(),
                        ResponsesToolIdentity {
                            name: "exec".to_string(),
                            namespace: Some("functions".to_string()),
                            custom: true,
                            tool_search: false,
                            schema_wrapped: false,
                            code_mode_helper: Some(helper),
                            raw_code_mode_input: false,
                        },
                    );
                    tools.tools.push(helper.definition());
                }
            }
        }
        Ok(tools)
    }

    pub(crate) fn normalize(body: &Value) -> Result<Value, FormatError> {
        Self::normalize_with_tools(body, Self::from_body(body)?)
    }

    pub(crate) fn normalize_go_messages(
        body: &Value,
        model: Option<&str>,
    ) -> Result<Value, FormatError> {
        // Native Go probes verified these endpoints accept the original unions;
        // Claude's required object wrapper instead confuses their arguments.
        let wrap_schema = !matches!(model, Some("qwen3.7-max" | "minimax-m2.5"));
        Self::normalize_with_tools(body, Self::from_go_body(body, wrap_schema)?)
    }

    fn normalize_with_tools(body: &Value, tools: Self) -> Result<Value, FormatError> {
        let mut body = body.clone();
        if let Some(input) = body.get_mut("input").and_then(Value::as_array_mut) {
            let native_web_ids = input
                .iter()
                .filter_map(|item| item.get("encrypted_content").and_then(Value::as_str))
                .filter_map(decode_web_context)
                .flat_map(|(_, ids)| ids)
                .collect::<Vec<_>>();
            input.retain(|item| {
                !(item["type"] == "web_search_call"
                    && item
                        .get("id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| native_web_ids.iter().any(|native| native == id)))
            });
            input.retain(|v| v["type"] != "additional_tools");
            for item in input {
                match item.get("type").and_then(Value::as_str) {
                    Some("tool_search_call") => {
                        if item["execution"] != "client"
                            || !item["arguments"].is_object()
                            || !item["call_id"].is_string()
                        {
                            return Err(invalid(
                                "input[].tool_search_call",
                                "client tool search requires a call id and object arguments",
                            ));
                        }
                        item["type"] = json!("function_call");
                        item["name"] = json!(TOOL_SEARCH_ALIAS);
                        item["arguments"] = json!(item["arguments"].to_string());
                        item.as_object_mut().unwrap().remove("execution");
                        if tools
                            .identity(TOOL_SEARCH_ALIAS)
                            .is_some_and(|i| i.schema_wrapped)
                        {
                            wrap_history_arguments(item)?;
                        }
                    }
                    Some("tool_search_output") => {
                        if !item["call_id"].is_string() {
                            return Err(invalid(
                                "input[].tool_search_output",
                                "client tool-search output requires call id",
                            ));
                        }
                        let output = json!({"loaded_tools":item["tools"]}).to_string();
                        item["type"] = json!("function_call_output");
                        item["output"] = json!(output);
                        item.as_object_mut().unwrap().remove("tools");
                        item.as_object_mut().unwrap().remove("execution");
                    }
                    Some("function_call" | "custom_tool_call") => {
                        let alias = tools.alias_for_item(item)?.map(str::to_string);
                        if item["type"] == "custom_tool_call" {
                            let text =
                                item.get("input").and_then(Value::as_str).ok_or_else(|| {
                                    invalid("input[].input", "custom call input must be a string")
                                })?;
                            let helper =
                                if item["name"] == "exec" && item["namespace"] == "functions" {
                                    CodeModeHelper::decode(text).filter(|(helper, _)| {
                                        tools
                                            .identity(helper.name())
                                            .is_some_and(|i| i.code_mode_helper == Some(*helper))
                                    })
                                } else {
                                    None
                                };
                            if let Some((helper, arguments)) = helper {
                                item["name"] = json!(helper.name());
                                // A rejected call was never executed. Preserve its raw
                                // payload as data, not invalid JSON that breaks replay
                                // in the upstream's function-call history parser.
                                let arguments = if serde_json::from_str::<Value>(&arguments).is_ok()
                                {
                                    arguments
                                } else {
                                    json!({"_afssr_rejected_arguments":arguments}).to_string()
                                };
                                item["arguments"] = json!(arguments);
                                item["type"] = json!("function_call");
                                item.as_object_mut().unwrap().remove("input");
                                item.as_object_mut().unwrap().remove("namespace");
                                continue;
                            }
                            item["arguments"] = json!(json!({"input":text}).to_string());
                            item["type"] = json!("function_call");
                            item.as_object_mut().unwrap().remove("input");
                        }
                        if let Some(alias) = alias {
                            if tools.identity(&alias).is_some_and(|i| i.schema_wrapped) {
                                wrap_history_arguments(item)?;
                            }
                            item["name"] = json!(alias);
                        }
                        item.as_object_mut().unwrap().remove("namespace");
                    }
                    Some("custom_tool_call_output") => item["type"] = json!("function_call_output"),
                    _ => {}
                }
            }
        }
        if body.get("tools").is_some() || !tools.tools.is_empty() {
            body["tools"] = json!(tools.tools);
        }
        if let Some(choice) = body.get_mut("tool_choice").filter(|v| v.is_object()) {
            if matches!(
                choice["type"].as_str(),
                Some("web_search" | "web_search_preview")
            ) && !tools.web_tools.is_empty()
            {
                *choice = json!({"type":"function", "name":"web_search"});
            } else if choice["type"] == "tool_search" {
                if !tools.identities.values().any(|t| t.tool_search) {
                    return Err(invalid(
                        "tool_choice",
                        "client tool-search choice requires a matching tool",
                    ));
                }
                *choice = json!({"type":"function", "name":TOOL_SEARCH_ALIAS});
            } else if matches!(choice["type"].as_str(), Some("function" | "custom")) {
                let alias = tools.alias_for_item(choice)?.map(str::to_string);
                if let Some(alias) = alias {
                    choice["name"] = json!(alias);
                }
                choice["type"] = json!("function");
                choice.as_object_mut().unwrap().remove("namespace");
            } else {
                return Err(invalid(
                    "tool_choice",
                    "unsupported Claude tool-choice mapping",
                ));
            }
        }
        Ok(body)
    }

    pub(crate) fn normalize_go_chat(
        body: &Value,
        model: Option<&str>,
    ) -> Result<Value, FormatError> {
        let wrap_schema = crate::provider_compat::opencode_go::chat_requires_root_schema_wrapper(
            model.unwrap_or_default(),
        );
        let tools = Self::from_go_body(body, wrap_schema)?;
        // Chat has no server-side web-search protocol. Do not silently lose it
        // when reusing the bidirectional client-tool bridge.
        if !tools.web_tools.is_empty() {
            return Err(invalid(
                "tools[]",
                "this Go Chat endpoint cannot execute native web search",
            ));
        }
        // Most Go Chat models accept native unions. DeepSeek's verified native
        // schema validator requires an object root; retain its unions inside
        // the reversible carrier rather than flattening their constraints.
        Self::normalize_with_tools(body, tools)
    }

    pub(crate) fn normalize_go_gemini(body: &Value) -> Result<Value, FormatError> {
        if body.get("tool_choice").is_some_and(is_native_web_tool) {
            return Err(invalid(
                "tool_choice",
                "Go Gemini cannot force a native web tool through function calling mode",
            ));
        }
        let web_tools = collected_tools(body)?
            .into_iter()
            .filter(is_native_web_tool)
            .collect::<Vec<_>>();
        for tool in &web_tools {
            if tool
                .get("external_web_access")
                .is_some_and(|value| !value.is_null() && value != &Value::Bool(true))
            {
                return Err(invalid(
                    "tools[].external_web_access",
                    "Go Gemini cannot represent cache-only web search",
                ));
            }
            if tool.as_object().is_some_and(|object| {
                object.iter().any(|(key, value)| {
                    !matches!(key.as_str(), "type" | "external_web_access") && !value.is_null()
                })
            }) {
                return Err(invalid(
                    "tools[]",
                    "Go Gemini web options have no audited native mapping",
                ));
            }
        }
        let client = Self::without_native_web_tools(body);
        let mut normalized =
            Self::normalize_with_tools(&client, Self::from_go_body(&client, true)?)?;
        if let Some(text) = normalized["input"].as_str() {
            normalized["input"] = json!([{"role":"user","content":text}]);
        }
        if !web_tools.is_empty() {
            normalized
                .as_object_mut()
                .unwrap()
                .entry("tools")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or_else(|| invalid("tools", "expected array of tools"))?
                .extend(web_tools);
        }
        if let Some(verbosity) = normalized
            .get_mut("text")
            .and_then(Value::as_object_mut)
            .and_then(|text| text.remove("verbosity"))
        {
            if !verbosity.is_null() {
                let preference = match verbosity.as_str() {
                    Some("low") => "Keep the final answer concise.",
                    Some("medium") => "Use a moderate level of detail in the final answer.",
                    Some("high") => "Provide a detailed final answer.",
                    _ => return Err(invalid("text.verbosity", "unknown verbosity preference")),
                };
                normalized["input"].as_array_mut().ok_or_else(|| invalid("input", "Go Gemini requires array input"))?
                    .insert(0,json!({"role":"developer","content":format!("{preference} Still execute all tools needed to complete the task.")}));
            }
        }
        let input = normalized["input"]
            .as_array()
            .ok_or_else(|| invalid("input", "Go Gemini requires array input"))?;
        let mut instructions = Vec::new();
        if let Some(existing) = normalized
            .get("instructions")
            .filter(|value| !value.is_null())
        {
            instructions.push(
                existing
                    .as_str()
                    .ok_or_else(|| invalid("instructions", "expected text instructions"))?
                    .to_string(),
            );
        }
        let mut retained = Vec::new();
        for item in input {
            if !matches!(item["role"].as_str(), Some("system" | "developer")) {
                retained.push(item.clone());
                continue;
            }
            let text = if let Some(text) = item["content"].as_str() {
                text.to_string()
            } else if let Some(parts) = item["content"].as_array() {
                parts
                    .iter()
                    .map(|part| {
                        if !matches!(part["type"].as_str(), Some("input_text" | "text")) {
                            return Err(invalid(
                                "input[].content",
                                "Gemini system instructions must be text",
                            ));
                        }
                        part["text"]
                            .as_str()
                            .map(str::to_string)
                            .ok_or_else(|| invalid("input[].content", "expected instruction text"))
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .join("\n")
            } else {
                return Err(invalid("input[].content", "expected text instructions"));
            };
            instructions.push(text);
        }
        normalized["input"] = json!(retained);
        if !instructions.is_empty() {
            normalized["instructions"] = json!(instructions.join("\n\n"));
        }
        Ok(normalized)
    }

    pub(crate) fn normalize_go_functionized_responses(
        body: &Value,
        model: &str,
    ) -> Result<Value, FormatError> {
        let mut web_tools = collected_tools(body)?
            .into_iter()
            .filter(is_native_web_tool)
            .collect::<Vec<_>>();
        for tool in &mut web_tools {
            if let Some(external) = tool.get("external_web_access") {
                if external != &Value::Bool(true) {
                    return Err(invalid(
                        "tools[].external_web_access",
                        "Go Grok cannot represent cache-only web search",
                    ));
                }
                // xAI web search is online by default; its Responses validator
                // rejects OpenAI's redundant true flag. Never omit false.
                tool.as_object_mut().unwrap().remove("external_web_access");
            }
        }
        let mut client = Self::without_native_web_tools(body);
        let native_choice = client
            .get("tool_choice")
            .filter(|choice| is_native_web_tool(choice))
            .cloned();
        if native_choice.is_some() {
            client.as_object_mut().unwrap().remove("tool_choice");
        }
        // Native probes verify Muse 1.3 accepts root combinators directly.
        // An extra Claude-style wrapper instead causes its allOf calls to fail.
        let mut normalized = Self::normalize_with_tools(
            &client,
            Self::from_go_body(&client, model != "muse-spark-1.3-contributor")?,
        )?;
        if !web_tools.is_empty() {
            let tools = normalized
                .as_object_mut()
                .unwrap()
                .entry("tools")
                .or_insert_with(|| json!([]));
            tools.as_array_mut().unwrap().extend(web_tools);
        }
        if let Some(choice) = native_choice {
            normalized["tool_choice"] = choice;
        }
        Ok(normalized)
    }

    fn without_native_web_tools(body: &Value) -> Value {
        let mut body = body.clone();
        if let Some(tools) = body.get_mut("tools").and_then(Value::as_array_mut) {
            tools.retain(|tool| !is_native_web_tool(tool));
        }
        if let Some(input) = body.get_mut("input").and_then(Value::as_array_mut) {
            for item in input.iter_mut().filter(|item| {
                matches!(
                    item["type"].as_str(),
                    Some("additional_tools" | "tool_search_output")
                )
            }) {
                if let Some(tools) = item.get_mut("tools").and_then(Value::as_array_mut) {
                    tools.retain(|tool| !is_native_web_tool(tool));
                }
            }
        }
        body
    }

    pub(crate) fn restore_response(&self, response: &mut Value) -> Result<(), FormatError> {
        if let Some(output) = response.get_mut("output").and_then(Value::as_array_mut) {
            for item in output {
                if item["type"] != "function_call" {
                    continue;
                }
                let Some(identity) = item
                    .get("name")
                    .and_then(Value::as_str)
                    .and_then(|name| self.identity(name))
                else {
                    continue;
                };
                item["name"] = json!(identity.name);
                if let Some(namespace) = &identity.namespace {
                    item["namespace"] = json!(namespace);
                }
                if identity.schema_wrapped {
                    item["arguments"] = json!(decode_schema_arguments(
                        item.get("arguments")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                    )
                    .map_err(|reason| invalid("output[].arguments", reason))?);
                }
                if identity.custom {
                    let arguments = item
                        .get("arguments")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let input = identity
                        .code_mode_helper
                        .map_or_else(
                            || decode_code_mode_arguments(arguments, identity.raw_code_mode_input),
                            |helper| helper.encode(arguments),
                        )
                        .map_err(|_| {
                            invalid(
                                "output[].arguments",
                                "Claude custom tool input wrapper is invalid",
                            )
                        })?;
                    item["type"] = json!("custom_tool_call");
                    item["input"] = json!(input);
                    item.as_object_mut().unwrap().remove("arguments");
                }
                if identity.tool_search {
                    restore_tool_search_call(item)
                        .map_err(|reason| invalid("output[].arguments", reason))?;
                }
            }
        }
        Ok(())
    }
}

fn is_native_web_tool(tool: &Value) -> bool {
    matches!(
        tool["type"].as_str(),
        Some("web_search" | "web_search_preview" | "web_search_preview_2025_03_11")
    )
}

pub(crate) fn restore_tool_search_call(item: &mut Value) -> Result<(), &'static str> {
    let arguments = item
        .get("arguments")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let value: Value = serde_json::from_str(arguments)
        .map_err(|_| "client tool-search arguments must be valid JSON")?;
    if !value.is_object() {
        return Err("client tool-search arguments must be an object");
    }
    item["type"] = json!("tool_search_call");
    item["execution"] = json!("client");
    item["arguments"] = value;
    item.as_object_mut().unwrap().remove("name");
    item.as_object_mut().unwrap().remove("namespace");
    Ok(())
}

pub(crate) fn decode_custom_arguments(arguments: &str) -> Result<String, &'static str> {
    let value: Value = serde_json::from_str(arguments)
        .map_err(|_| "Claude custom tool arguments are not valid JSON")?;
    let object = value
        .as_object()
        .filter(|o| o.len() == 1)
        .ok_or("Claude custom tool arguments must contain only input")?;
    object
        .get("input")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or("Claude custom tool input must be a string")
}

pub(crate) fn decode_code_mode_arguments(
    arguments: &str,
    allow_raw: bool,
) -> Result<String, &'static str> {
    match decode_custom_arguments(arguments) {
        Ok(input) => Ok(input),
        // MiMo's rich Codex tool calls can carry the original freeform payload
        // directly in arguments. Only an explicitly declared Code Mode custom
        // tool accepts this opaque string; never extract code from chat text.
        Err(_) if allow_raw && !arguments.trim().is_empty() => Ok(arguments.to_string()),
        Err(error) => Err(error),
    }
}
