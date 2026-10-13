use aether_ai_formats::provider_compat::proxy::rules::apply_local_body_rules;
use aether_ai_formats::{convert_request, FormatContext};
use serde_json::{json, Value};

fn main() {
    let rules: Value = serde_json::from_str(include_str!(
        "../../../../deploy/compat/haiku-adaptive-body-rules.json"
    ))
    .unwrap();
    let original = json!({
        "model": "gpt-5.6-terra", "max_output_tokens": 512,
        "reasoning": {"effort": "high"},
        "input": [{"role": "user", "content": "Write a short Python function"}]
    });
    let context = FormatContext::default().with_mapped_model("claude-haiku-5-5");
    let mut converted =
        convert_request("openai:responses", "claude:messages", &original, &context).unwrap();
    assert_eq!(converted["thinking"]["type"], "enabled");
    let effort = converted["output_config"]["effort"].clone();
    assert!(apply_local_body_rules(
        &mut converted,
        Some(&rules),
        Some(&original)
    ));
    assert_eq!(converted["thinking"]["type"], "adaptive");
    assert!(converted["thinking"].get("budget_tokens").is_none());
    assert_eq!(converted["output_config"]["effort"], effort);

    for original in [
        json!({"model": "space-bunny", "thinking": {"type": "enabled", "budget_tokens": 4096}}),
        json!({"model": "claude-haiku-5-5", "thinking": {"type": "disabled"}}),
        json!({"model": "claude-haiku-5-5", "messages": []}),
        json!({"model": "claude-haiku-5-5", "thinking": {"type": "adaptive"},
               "output_config": {"effort": "low"}}),
    ] {
        let mut body = original.clone();
        assert!(apply_local_body_rules(&mut body, Some(&rules), None));
        assert_eq!(body, original);
    }
    let mut repeated = converted.clone();
    assert!(apply_local_body_rules(&mut repeated, Some(&rules), None));
    assert_eq!(repeated, converted);
    println!(
        "{}",
        json!({"checks_passed": 6, "thinking": converted["thinking"],
                         "output_config": converted["output_config"]})
    );
}
