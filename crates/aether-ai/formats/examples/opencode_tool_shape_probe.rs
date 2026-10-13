use aether_ai_formats::{convert_request, FormatContext};
use serde_json::{json, Value};

fn main() {
    let function = json!({
        "type": "function", "name": "probe",
        "description": "Read a synthetic value",
        "parameters": {"type": "object", "properties": {"label": {"type": "string"}},
                       "required": ["label"], "additionalProperties": false}
    });
    let custom = json!({
        "type": "custom", "name": "probe",
        "description": "Read a synthetic value; pass alpha as text",
        "format": {"type": "text"}
    });
    let namespace = |tool: &Value| {
        json!({
            "type": "namespace", "name": "functions",
            "description": "Available diagnostic tools", "tools": [tool]
        })
    };
    for (case, tools, lite) in [
        ("function", json!([function]), false),
        ("namespace_function", json!([namespace(&function)]), false),
        ("namespace_custom", json!([namespace(&custom)]), false),
        ("lite_namespace", json!([namespace(&function)]), true),
    ] {
        let mut body = json!({
            "model": "gpt-6.1-sol", "stream": false,
            "input": [{"role": "user", "content": "Call the probe tool for alpha"}]
        });
        if lite {
            body["input"].as_array_mut().unwrap().insert(
                0,
                json!({
                    "type": "additional_tools", "role": "developer", "tools": tools
                }),
            );
        } else {
            body["tools"] = tools;
        }
        for target in ["openai:chat", "claude:messages"] {
            let context = FormatContext::default().with_mapped_model("space-bunny");
            let result = match convert_request("openai:responses", target, &body, &context) {
                Ok(converted) => json!({"tools": converted.get("tools")}),
                Err(error) => json!({"error": error.to_string()}),
            };
            println!(
                "{}",
                json!({"case": case, "target": target, "result": result})
            );
        }
    }
}
