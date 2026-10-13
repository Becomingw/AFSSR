use std::collections::BTreeMap;

use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use serde_json::{json, Value};

use crate::formats::{context::FormatError, id::FormatId, openai::namespace::NamespaceToolAliases};
use crate::protocol::canonical::openai_responses_tools_to_canonical;

const CLAUDE_THINKING_CARRIER_PREFIX: &str = "aether-claude-thinking-v1:";
const MAX_THINKING_CARRIER_BYTES: usize = 1024 * 1024;
const TOOL_SEARCH_ALIAS: &str = "aether_tool_search";
const WEB_CONTEXT_PREFIX: &str = "aether-claude-web-v1:";

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

#[derive(Clone, Debug)]
pub(crate) struct ResponsesToolIdentity {
    pub name: String,
    pub namespace: Option<String>,
    pub custom: bool,
    pub tool_search: bool,
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
    let mut description = tool
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
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
        "parameters":{"type":"object", "properties":{"input":{"type":"string"}},
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
                    };
                    let function = child.to_openai_chat_tool()["function"].clone();
                    let mut flattened = function;
                    flattened["type"] = json!("function");
                    result.insert(child.chat_name.clone(), identity, flattened)?;
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
                    },
                    tool,
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
        identity: ResponsesToolIdentity,
        tool: Value,
    ) -> Result<(), FormatError> {
        if self.identities.contains_key(&alias) {
            return Err(invalid("tools[].name", "duplicate tool identity"));
        }
        self.aliases.insert(
            (identity.namespace.clone(), identity.name.clone()),
            alias.clone(),
        );
        self.identities.insert(alias, identity);
        self.tools.push(tool);
        Ok(())
    }

    pub(crate) fn from_report_context(context: &Value) -> Self {
        if !is_claude_provider(context) {
            return Self::default();
        }
        context
            .get("original_request_body")
            .and_then(|body| {
                let expanded =
                    crate::formats::openai::responses::history::expand_previous_response_for_chat(
                        body,
                        context.get("api_key_id").and_then(Value::as_str),
                    )
                    .ok()?;
                Self::from_body(&expanded).ok()
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
            .filter(|(_, tool)| tool.name == name)
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

    pub(crate) fn normalize(body: &Value) -> Result<Value, FormatError> {
        let tools = Self::from_body(body)?;
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
                            item["arguments"] = json!(json!({"input":text}).to_string());
                            item["type"] = json!("function_call");
                            item.as_object_mut().unwrap().remove("input");
                        }
                        if let Some(alias) = alias {
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
                if identity.custom {
                    let arguments = item
                        .get("arguments")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let input = decode_custom_arguments(arguments).map_err(|_| {
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
