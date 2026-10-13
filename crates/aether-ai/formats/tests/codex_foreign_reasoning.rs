use aether_ai_formats::formats::openai::responses::codex::apply_codex_openai_responses_special_body_edits;
use serde_json::{json, Value};

fn body() -> Value {
    json!({"model": "gpt-5.4", "input": [
        {"type": "message", "role": "user", "content": "Continue"},
        {"type": "reasoning", "id": "rs_resp_20261008234244", "summary": [],
         "encrypted_content": null, "content": [{"type": "reasoning_text", "text": "synthetic"}]},
        {"type": "custom_tool_call", "id": "ct_1", "call_id": "call_1", "name": "exec", "input": "test()"},
        {"type": "custom_tool_call_output", "call_id": "call_1", "output": "done"}
    ]})
}

fn apply(body: &mut Value, provider: &str) {
    apply_codex_openai_responses_special_body_edits(body, provider, "openai:responses", None, None);
}

#[test]
fn codex_removes_known_foreign_plaintext_without_losing_tool_pairs() {
    let original = body();
    let mut request = original.clone();
    apply(&mut request, "codex");
    assert_eq!(
        request["input"],
        json!([
            original["input"][0],
            original["input"][2],
            original["input"][3]
        ])
    );
    let once = request.clone();
    apply(&mut request, "codex");
    assert_eq!(request, once);
}

#[test]
fn native_glm_and_other_providers_are_unchanged() {
    for provider in ["custom", "openai", "deepseek", ""] {
        let original = body();
        let mut request = original.clone();
        apply(&mut request, provider);
        assert_eq!(request, original);
    }
}

#[test]
fn native_reasoning_and_opaque_state_are_preserved() {
    for (id, encrypted, content) in [
        (
            "rs_native",
            Value::Null,
            json!([{"type":"reasoning_text","text":"native"}]),
        ),
        (
            "rs_resp_vendor",
            json!("opaque-state"),
            json!([{"type":"reasoning_text","text":"vendor"}]),
        ),
        ("rs_resp_vendor", Value::Null, json!([])),
    ] {
        let mut request = body();
        request["input"][1]["id"] = json!(id);
        request["input"][1]["encrypted_content"] = encrypted;
        request["input"][1]["content"] = content;
        let original = request["input"].clone();
        apply(&mut request, "codex");
        assert_eq!(request["input"], original);
    }
}
