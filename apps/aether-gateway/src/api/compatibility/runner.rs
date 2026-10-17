use super::schema::*;
use crate::ai_serving::api::compatibility_formats::*;
use serde_json::{json, Value};

const PASS: &str =
    "Synthetic conversion and semantic assertions passed; not a live client/model test";
const STREAM_PASS: &str =
    "Synthetic text SSE content and terminal checked; tool streaming and transport not tested";
const CONVERSION: &str = "Protocol conversion rejected the synthetic fixture";
const UNSUPPORTED: &str = "Format library does not support this synthetic field or format";
const ASSERTION: &str =
    "Converted output lost expected synthetic content, tool linkage, schema or terminal";
const NO_CANCEL: &str = "Cancellation requires a live transport; not executed by the offline suite";
const NO_COMPACT: &str =
    "Compaction/context recovery requires client and model execution; not executed";
const NO_CLIENT: &str = "Client/version is a user-supplied label; no real client was launched";
const NO_MODEL: &str = "No provider request or real model execution was performed";

pub(super) fn valid_case(c: &CaseResult) -> bool {
    let known = [
        "basic_request",
        "tool_definition",
        "tool_return",
        "multi_turn_tools",
        "long_history",
        "image",
        "sync_response",
        "tool_response",
        "stream",
        "cancel",
        "compression",
        "real_client",
        "real_model",
    ];
    known.contains(&c.case.as_str())
        && match c.status {
            Status::Passed => {
                if c.case == "stream" {
                    c.reason == STREAM_PASS
                } else {
                    known[..8].contains(&c.case.as_str()) && c.reason == PASS
                }
            }
            Status::Failed => {
                known[..9].contains(&c.case.as_str())
                    && [CONVERSION, ASSERTION].contains(&c.reason.as_str())
            }
            Status::Unsupported => known[..9].contains(&c.case.as_str()) && c.reason == UNSUPPORTED,
            Status::NotTested => matches!(
                (c.case.as_str(), c.reason.as_str()),
                ("cancel", NO_CANCEL)
                    | ("compression", NO_COMPACT)
                    | ("real_client", NO_CLIENT)
                    | ("real_model", NO_MODEL)
            ),
        }
}

pub(super) fn run(profile: &Profile) -> Run {
    let mut cases = Vec::new();
    for case in [
        "basic_request",
        "tool_definition",
        "tool_return",
        "multi_turn_tools",
        "long_history",
        "image",
    ] {
        cases.push(result(case, check_request(&profile.config, case)));
    }
    for case in ["sync_response", "tool_response"] {
        cases.push(result(
            case,
            check_response(&profile.config, case == "tool_response"),
        ));
    }
    let mut stream = result("stream", check_stream(&profile.config));
    if stream.status == Status::Passed {
        stream.reason = STREAM_PASS.into();
    }
    cases.push(stream);
    for (case, reason) in [
        ("cancel", NO_CANCEL),
        ("compression", NO_COMPACT),
        ("real_client", NO_CLIENT),
        ("real_model", NO_MODEL),
    ] {
        cases.push(CaseResult {
            case: case.into(),
            status: Status::NotTested,
            reason: reason.into(),
        });
    }
    Run {
        id: uuid::Uuid::new_v4(),
        profile_id: profile.id,
        config: profile.config.clone(),
        config_fingerprint: profile.config.fingerprint(),
        suite: SUITE.into(),
        evidence: Evidence::OfflineSynthetic,
        created_at: chrono::Utc::now().timestamp(),
        cases,
    }
}

enum Failure {
    Conversion,
    Unsupported,
    Assertion,
}
impl From<FormatError> for Failure {
    fn from(e: FormatError) -> Self {
        match e {
            FormatError::UnsupportedField { .. } | FormatError::UnsupportedFormat(_) => {
                Self::Unsupported
            }
            _ => Self::Conversion,
        }
    }
}
fn result(case: &str, r: Result<(), Failure>) -> CaseResult {
    let (status, reason) = match r {
        Ok(()) => (Status::Passed, PASS),
        Err(Failure::Conversion) => (Status::Failed, CONVERSION),
        Err(Failure::Unsupported) => (Status::Unsupported, UNSUPPORTED),
        Err(Failure::Assertion) => (Status::Failed, ASSERTION),
    };
    CaseResult {
        case: case.into(),
        status,
        reason: reason.into(),
    }
}

fn text(v: &str) -> CanonicalContentBlock {
    CanonicalContentBlock::Text {
        text: v.into(),
        extensions: Default::default(),
    }
}
fn message(role: CanonicalRole, content: Vec<CanonicalContentBlock>) -> CanonicalMessage {
    CanonicalMessage {
        role,
        content,
        extensions: Default::default(),
    }
}
fn fixture(case: &str, model: &str) -> CanonicalRequest {
    let mut request = CanonicalRequest {
        model: model.into(),
        messages: vec![message(CanonicalRole::User, vec![text("fixture-question")])],
        ..Default::default()
    };
    request.generation.max_tokens = Some(256);
    if matches!(case, "tool_definition" | "tool_return" | "multi_turn_tools") {
        request.tools.push(CanonicalToolDefinition { name: "fixture_lookup".into(), description: None,
            parameters: Some(json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"]})), strict: None, extensions: Default::default() });
    }
    let rounds = match case {
        "tool_return" => 1,
        "multi_turn_tools" => 3,
        _ => 0,
    };
    for i in 0..rounds {
        request.messages.push(message(
            CanonicalRole::Assistant,
            vec![CanonicalContentBlock::ToolUse {
                id: format!("call_fixture_{i}"),
                name: "fixture_lookup".into(),
                input: json!({"query":format!("fixture-{i}")}),
                extensions: Default::default(),
            }],
        ));
        request.messages.push(message(
            CanonicalRole::Tool,
            vec![CanonicalContentBlock::ToolResult {
                tool_use_id: format!("call_fixture_{i}"),
                name: Some("fixture_lookup".into()),
                output: None,
                content_text: Some(format!("fixture-result-{i}")),
                is_error: false,
                extensions: Default::default(),
            }],
        ));
        request.messages.push(message(
            CanonicalRole::User,
            vec![text(&format!("fixture-next-{i}"))],
        ));
    }
    if case == "long_history" {
        request.messages = (0..64)
            .map(|i| {
                message(
                    if i % 2 == 0 {
                        CanonicalRole::User
                    } else {
                        CanonicalRole::Assistant
                    },
                    vec![text(&format!(
                        "fixture-history-{i}:{}",
                        "synthetic ".repeat(128)
                    ))],
                )
            })
            .collect();
    }
    if case == "image" {
        request.messages[0].content.push(CanonicalContentBlock::Image {
            data: Some("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aN1sAAAAASUVORK5CYII=".into()),
            url: None, media_type: Some("image/png".into()), detail: None, extensions: Default::default(),
        });
    }
    request
}

// Compare semantic content independently of protocol-specific message packing and extensions.
fn content_signature(blocks: impl Iterator<Item = CanonicalContentBlock>) -> Vec<Value> {
    blocks
        .map(|b| match b {
            CanonicalContentBlock::Text { text, .. } => json!(["text", text]),
            CanonicalContentBlock::ToolUse {
                id, name, input, ..
            } => json!(["call", id, name, input]),
            CanonicalContentBlock::ToolResult {
                tool_use_id,
                content_text,
                output,
                is_error,
                ..
            } => {
                let value = content_text
                    .or_else(|| output.as_ref().and_then(Value::as_str).map(str::to_owned));
                json!(["result", tool_use_id, value, is_error])
            }
            CanonicalContentBlock::Image {
                data,
                url,
                media_type,
                ..
            } => {
                // Parsers may represent a data URI as either url or inline data.
                let carrier = data.or_else(|| {
                    url.map(|s| s.split_once(",").map(|(_, v)| v.to_owned()).unwrap_or(s))
                });
                json!([
                    "image",
                    carrier,
                    media_type.unwrap_or_else(|| "image/png".into())
                ])
            }
            _ => json!(["unexpected"]),
        })
        .collect()
}

fn check_request(config: &ProfileConfig, case: &str) -> Result<(), Failure> {
    let source = config.client_protocol.as_str();
    let target = config.upstream_format.as_str();
    let expected = fixture(case, &config.model);
    let input = emit_request(source, &expected, &FormatContext::default())?;
    let ctx = FormatContext::default().with_mapped_model(&config.mapped_model);
    let converted = convert_request(source, target, &input, &ctx)?;
    let actual = parse_request(target, &converted, &FormatContext::default())?;
    let wanted = content_signature(expected.messages.iter().flat_map(|m| m.content.clone()));
    let got = content_signature(actual.messages.iter().flat_map(|m| m.content.clone()));
    let tool_ok = expected.tools.len() == actual.tools.len()
        && expected
            .tools
            .iter()
            .zip(&actual.tools)
            .all(|(e, a)| e.name == a.name && e.parameters == a.parameters);
    if wanted != got || !tool_ok || actual.model != config.mapped_model {
        return Err(Failure::Assertion);
    }
    Ok(())
}

fn check_response(config: &ProfileConfig, tool: bool) -> Result<(), Failure> {
    let content = if tool {
        vec![CanonicalContentBlock::ToolUse {
            id: "call_fixture_0".into(),
            name: "fixture_lookup".into(),
            input: json!({"query":"fixture-0"}),
            extensions: Default::default(),
        }]
    } else {
        vec![text("fixture-answer")]
    };
    let stop = if tool {
        CanonicalStopReason::ToolUse
    } else {
        CanonicalStopReason::EndTurn
    };
    let expected = CanonicalResponse {
        id: "fixture_response".into(),
        model: config.mapped_model.clone(),
        outputs: vec![],
        content: content.clone(),
        stop_reason: Some(stop.clone()),
        usage: None,
        extensions: Default::default(),
    };
    let input = emit_response(
        config.upstream_format.as_str(),
        &expected,
        &FormatContext::default(),
    )?;
    let converted = convert_response(
        config.upstream_format.as_str(),
        config.client_protocol.as_str(),
        &input,
        &FormatContext::default(),
    )?;
    let actual = parse_response(
        config.client_protocol.as_str(),
        &converted,
        &FormatContext::default(),
    )?;
    if content_signature(actual.content.into_iter()) != content_signature(content.into_iter())
        || actual.stop_reason != Some(stop)
    {
        return Err(Failure::Assertion);
    }
    Ok(())
}

fn stream_events(protocol: Protocol) -> Vec<Value> {
    match protocol {
        Protocol::Chat => vec![
            json!({"id":"fixture","model":"fixture","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant","content":"fixture-answer"},"finish_reason":null}]}),
            json!({"id":"fixture","model":"fixture","object":"chat.completion.chunk","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}),
        ],
        Protocol::Claude => vec![
            json!({"type":"message_start","message":{"id":"fixture","type":"message","role":"assistant","model":"fixture","content":[],"usage":{"input_tokens":1,"output_tokens":0}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"fixture-answer"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":1}}),
            json!({"type":"message_stop"}),
        ],
        Protocol::Responses => vec![
            json!({"type":"response.created","response":{"id":"fixture","model":"fixture","status":"in_progress","output":[]}}),
            json!({"type":"response.output_item.added","output_index":0,"item":{"type":"message","id":"msg_fixture","role":"assistant","content":[],"status":"in_progress"}}),
            json!({"type":"response.content_part.added","output_index":0,"content_index":0,"item_id":"msg_fixture","part":{"type":"output_text","text":"","annotations":[]}}),
            json!({"type":"response.output_text.delta","output_index":0,"content_index":0,"item_id":"msg_fixture","delta":"fixture-answer"}),
            json!({"type":"response.output_text.done","output_index":0,"content_index":0,"item_id":"msg_fixture","text":"fixture-answer"}),
            json!({"type":"response.completed","response":{"id":"fixture","model":"fixture","status":"completed","output":[{"type":"message","id":"msg_fixture","role":"assistant","status":"completed","content":[{"type":"output_text","text":"fixture-answer","annotations":[]}]}],"usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}}),
        ],
    }
}

fn check_stream(config: &ProfileConfig) -> Result<(), Failure> {
    let spec = build_stream_transcoder(
        config.upstream_format.as_str(),
        config.client_protocol.as_str(),
        &FormatContext::default(),
    )?;
    if spec.source.as_str() != config.upstream_format.as_str()
        || spec.target.as_str() != config.client_protocol.as_str()
    {
        return Err(Failure::Assertion);
    }
    let report = json!({"provider_api_format":config.upstream_format.as_str(),"client_api_format":config.client_protocol.as_str(),"mapped_model":config.model});
    let mut matrix = StreamingStandardFormatMatrix::default();
    let mut bytes = Vec::new();
    for event in stream_events(config.upstream_format) {
        bytes.extend(
            matrix
                .transform_line(&report, format!("data: {event}\n").into_bytes())
                .map_err(|_| Failure::Conversion)?,
        );
    }
    if config.upstream_format == Protocol::Chat {
        bytes.extend(
            matrix
                .transform_line(&report, b"data: [DONE]\n".to_vec())
                .map_err(|_| Failure::Conversion)?,
        );
    }
    bytes.extend(matrix.finish(&report).map_err(|_| Failure::Conversion)?);
    let output = String::from_utf8(bytes).map_err(|_| Failure::Assertion)?;
    let events: Vec<Value> = output
        .lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .filter(|v| *v != "[DONE]")
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()
        .map_err(|_| Failure::Assertion)?;
    let terminal = match config.client_protocol {
        Protocol::Chat => events
            .iter()
            .any(|e| e["choices"][0]["finish_reason"] == "stop"),
        Protocol::Claude => events.iter().any(|e| e["type"] == "message_stop"),
        Protocol::Responses => events
            .iter()
            .any(|e| e["type"] == "response.completed" && e["response"]["status"] == "completed"),
    };
    if !terminal
        || !output.contains("fixture-answer")
        || events.iter().any(|e| {
            e.get("error").is_some()
                || matches!(e["type"].as_str(), Some("error" | "response.failed"))
        })
    {
        return Err(Failure::Assertion);
    }
    Ok(())
}
