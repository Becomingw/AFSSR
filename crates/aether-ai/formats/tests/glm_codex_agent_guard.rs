use aether_ai_formats::provider_compat::proxy::rules::{
    apply_local_body_rules_with_request_headers, body_rules_are_locally_supported,
};
use http::HeaderMap;
use serde_json::{json, Value};

fn rules() -> Value {
    serde_json::from_str(include_str!(
        "../../../../deploy/compat/glm-codex-agent-guard.json"
    ))
    .unwrap()
}

fn body() -> Value {
    json!({
        "model": "gpt-5.6-sol", "stream": true,
        "instructions": "Keep these original instructions unchanged.",
        "input": [{"role":"user","content":"Check the synthetic service."}],
        "tools": [{"type":"function","name":"check_service","parameters":{"type":"object"}}]
    })
}

fn apply(body: &mut Value, original: &Value, client: Option<&str>) {
    let mut headers = HeaderMap::new();
    if let Some(client) = client {
        headers.insert("user-agent", client.parse().unwrap());
    }
    assert!(apply_local_body_rules_with_request_headers(
        body,
        Some(&rules()),
        Some(original),
        Some(&headers)
    ));
}

#[test]
fn guard_is_supported_scoped_and_idempotent() {
    assert!(body_rules_are_locally_supported(Some(&rules())));
    let original = body();
    let mut request = original.clone();
    request["model"] = json!("glm-5.3-flash");
    apply(&mut request, &original, Some("codex_cli_rs/0.162.0"));
    assert_eq!(request["input"].as_array().unwrap().len(), 2);
    assert_eq!(request["input"][1]["role"], "developer");
    assert_eq!(request["instructions"], original["instructions"]);
    assert_eq!(request["tools"], original["tools"]);
    assert!(request.get("tool_choice").is_none());
    let once = request.clone();
    apply(&mut request, &original, Some("codex_cli_rs/0.162.0"));
    assert_eq!(request, once);
}

#[test]
fn guard_does_not_touch_other_clients_or_models() {
    for client in [None, Some("another-client/1.0")] {
        let original = body();
        let mut request = original.clone();
        apply(&mut request, &original, client);
        assert_eq!(request, original);
    }
    let mut original = body();
    original["model"] = json!("gpt-6-astra");
    let mut request = original.clone();
    apply(&mut request, &original, Some("codex_cli_rs/0.162.0"));
    assert_eq!(request, original);
}

#[test]
fn guard_respects_explicit_no_tools_and_plain_input() {
    for variant in 0..4 {
        let mut original = body();
        match variant {
            0 => original["tool_choice"] = json!("none"),
            1 => original["tools"] = json!([]),
            2 => {
                original.as_object_mut().unwrap().remove("tools");
            }
            _ => original["input"] = json!("Just answer the question."),
        }
        let mut request = original.clone();
        apply(&mut request, &original, Some("codex_cli_rs/0.162.0"));
        assert_eq!(request, original);
    }
}

#[test]
fn guard_preserves_tool_history_reasoning_and_approval_fields() {
    let mut original = body();
    original["input"] = json!([
        {"type":"reasoning","id":"rs_test","encrypted_content":"opaque","summary":[]},
        {"type":"custom_tool_call","call_id":"call_1","name":"exec","input":"test()"},
        {"type":"custom_tool_call_output","call_id":"call_1","output":"approval required"}
    ]);
    original["tool_choice"] = json!("auto");
    let mut request = original.clone();
    apply(&mut request, &original, Some("Codex Desktop"));
    assert_eq!(request["input"].as_array().unwrap().len(), 4);
    for index in 0..3 {
        assert_eq!(request["input"][index], original["input"][index]);
    }
    assert_eq!(request["tool_choice"], "auto");
}

fn lite_body() -> Value {
    let mut request = body();
    let tools = request.as_object_mut().unwrap().remove("tools").unwrap();
    request["input"].as_array_mut().unwrap().insert(
        0,
        json!({
            "type": "additional_tools", "role": "developer", "tools": tools
        }),
    );
    request
}

#[test]
fn lite_tools_are_lifted_before_the_agent_guard() {
    let original = lite_body();
    let mut request = original.clone();
    apply(&mut request, &original, Some("codex_cli_rs/0.162.0"));
    assert_eq!(request["tools"], original["input"][0]["tools"]);
    assert_eq!(request["input"][0], original["input"][1]);
    assert_eq!(request["input"].as_array().unwrap().len(), 2);
    assert_eq!(request["input"][1]["role"], "developer");
    let once = request.clone();
    apply(&mut request, &original, Some("codex_cli_rs/0.162.0"));
    assert_eq!(request, once);
}

#[test]
fn lite_conversion_preserves_namespaces_grammar_and_no_tool_choice() {
    let mut original = lite_body();
    original["input"][0]["tools"] = json!([{
        "type": "namespace", "name": "functions", "tools": [{
            "type": "custom", "name": "exec", "format": {
                "type": "grammar", "syntax": "lark", "definition": "start: /.+/"
            }
        }]
    }]);
    original["tool_choice"] = json!("none");
    let mut request = original.clone();
    apply(&mut request, &original, Some("codex_cli_rs/0.162.0"));
    assert_eq!(request["tools"], original["input"][0]["tools"]);
    assert_eq!(request["tool_choice"], "none");
    assert_eq!(request["input"], json!([original["input"][1]]));
}

#[test]
fn lite_conversion_does_not_overwrite_explicit_tools_or_other_clients() {
    for client in [None, Some("another-client/1.0")] {
        let original = lite_body();
        let mut request = original.clone();
        apply(&mut request, &original, client);
        assert_eq!(request, original);
    }
    let mut original = lite_body();
    original["tools"] = json!([]);
    let mut request = original.clone();
    apply(&mut request, &original, Some("Codex Desktop"));
    assert_eq!(request, original);
}

#[test]
fn lite_empty_catalog_does_not_inject_execution_instructions() {
    let mut original = lite_body();
    original["input"][0]["tools"] = json!([]);
    let mut request = original.clone();
    apply(&mut request, &original, Some("Codex Desktop"));
    assert_eq!(request["tools"], json!([]));
    assert_eq!(request["input"], json!([original["input"][1]]));
}
