use aether_ai_formats::protocol::canonical::{
    canonical_to_openai_chat_response, from_openai_responses_to_canonical_response,
};
use aether_ai_formats::{convert_request, convert_response, FormatContext, FormatError};
use serde_json::{json, Value};

#[test]
fn responses_tool_history_keeps_readable_reasoning_with_opaque_state() {
    for (field, part_type, text) in [
        ("summary", "summary_text", "  synthetic summary\n"),
        ("content", "reasoning_text", "synthetic reasoning"),
    ] {
        for encrypted in [Value::Null, json!("synthetic-opaque-state")] {
            let mut reasoning = json!({
                "type": "reasoning", "id": "rs_fixture", "encrypted_content": encrypted
            });
            reasoning[field] = json!([{"type": part_type, "text": text}]);
            let body = json!({
                "model": "fixture", "reasoning": {"effort": "high"},
                "input": [
                    {"role": "user", "content": "Read a nonce"},
                    reasoning,
                    {"type": "function_call", "call_id": "call_fixture", "name": "read_nonce", "arguments": "{}"},
                    {"type": "function_call_output", "call_id": "call_fixture", "output": "synthetic nonce"},
                    {"role": "user", "content": "Continue"}
                ]
            });
            for provider in ["custom", "opencode_go"] {
                let context = FormatContext::default().with_report_context(json!({
                    "provider_type": provider, "provider_api_format": "openai:chat",
                    "client_api_format": "openai:responses", "needs_conversion": true,
                    "original_request_body": body
                }));
                let converted = convert_request("openai:responses", "openai:chat", &body, &context)
                    .expect("convert synthetic tool history");
                let messages = converted["messages"].as_array().expect("messages");
                assert_eq!(messages.len(), 4);
                assert_eq!(messages[1]["role"], "assistant");
                assert_eq!(messages[1]["content"], Value::Null);
                assert_eq!(
                    messages[1]["reasoning_content"], text,
                    "{field}, {encrypted}, {provider}"
                );
                assert_eq!(messages[1]["tool_calls"][0]["id"], "call_fixture");
                assert_eq!(messages[2]["tool_call_id"], "call_fixture");
                assert_eq!(messages[2]["content"], "synthetic nonce");
                assert_eq!(messages[3]["content"], "Continue");
                if provider == "opencode_go" {
                    assert!(messages[1].get("reasoning_parts").is_none());
                } else if !encrypted.is_null() {
                    assert_eq!(
                        messages[1]["reasoning_parts"],
                        json!([
                            {"type": "redacted_thinking", "data": encrypted}
                        ])
                    );
                }
            }
        }
    }
}

#[test]
fn canonical_chat_response_emitter_keeps_readable_reasoning_with_opaque_state() {
    let body = json!({
        "id": "resp_fixture", "object": "response", "model": "fixture", "status": "completed",
        "output": [
            {"type": "reasoning", "id": "rs_fixture", "summary": [
                {"type": "summary_text", "text": "synthetic summary"}
            ], "encrypted_content": "synthetic-opaque-state"},
            {"type": "function_call", "call_id": "call_fixture", "name": "read_nonce", "arguments": "{}", "status": "completed"}
        ]
    });
    // The public response conversion still rejects opaque state it cannot replay.
    // Exercise the shared canonical emitter without relaxing that separate guard.
    assert!(matches!(
        convert_response(
            "openai:responses",
            "openai:chat",
            &body,
            &FormatContext::default()
        ),
        Err(FormatError::LossyConversionBlocked { .. })
    ));
    let canonical = from_openai_responses_to_canonical_response(&body).expect("canonical response");
    let converted = canonical_to_openai_chat_response(&canonical);
    let message = &converted["choices"][0]["message"];
    assert_eq!(message["reasoning_content"], "synthetic summary");
    assert_eq!(
        message["reasoning_parts"],
        json!([
            {"type": "redacted_thinking", "data": "synthetic-opaque-state"}
        ])
    );
    assert_eq!(message["tool_calls"][0]["id"], "call_fixture");
}

#[test]
fn responses_tool_history_without_readable_reasoning_does_not_invent_it() {
    for reasoning in [
        None,
        Some(
            json!({"type": "reasoning", "id": "rs_fixture", "summary": [], "encrypted_content": "synthetic-opaque-state"}),
        ),
        Some(
            json!({"type": "reasoning", "id": "rs_fixture", "summary": [{"type": "summary_text", "text": " \n"}], "encrypted_content": "synthetic-opaque-state"}),
        ),
    ] {
        let mut input = vec![json!({"role": "user", "content": "Read a nonce"})];
        input.extend(reasoning);
        input.push(json!({"type": "function_call", "call_id": "call_fixture", "name": "read_nonce", "arguments": "{}"}));
        input.push(json!({"type": "function_call_output", "call_id": "call_fixture", "output": "synthetic nonce"}));
        let body = json!({"model": "fixture", "input": input});
        let converted = convert_request(
            "openai:responses",
            "openai:chat",
            &body,
            &FormatContext::default(),
        )
        .expect("convert synthetic tool history without readable reasoning");
        let message = &converted["messages"][1];
        assert_eq!(message["tool_calls"][0]["id"], "call_fixture");
        assert!(message.get("reasoning_content").is_none());
    }
}
