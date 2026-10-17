use aether_ai_formats::{convert_request, convert_response, FormatContext};
use serde_json::{json, Value};

const CLIENTS: [&str; 3] = ["openai:responses", "claude:messages", "openai:chat"];
const UPSTREAMS: [&str; 4] = [
    "openai:responses",
    "claude:messages",
    "openai:chat",
    "gemini:generate_content",
];

fn request(format: &str) -> Value {
    let schema =
        json!({"type":"object","properties":{"nonce":{"type":"string"}},"required":["nonce"]});
    match format {
        "openai:responses" => {
            json!({"model":"fixture","input":[{"role":"user","content":"Read the nonce"}],"max_output_tokens":64,"tools":[{"type":"function","name":"read_nonce","description":"Read a nonce","parameters":schema}]})
        }
        "openai:chat" => {
            json!({"model":"fixture","messages":[{"role":"user","content":"Read the nonce"}],"max_tokens":64,"tools":[{"type":"function","function":{"name":"read_nonce","description":"Read a nonce","parameters":schema}}]})
        }
        "claude:messages" => {
            json!({"model":"fixture","messages":[{"role":"user","content":"Read the nonce"}],"max_tokens":64,"tools":[{"name":"read_nonce","description":"Read a nonce","input_schema":schema}]})
        }
        _ => unreachable!(),
    }
}

fn response(format: &str) -> Value {
    match format {
        "openai:responses" => {
            json!({"id":"resp_fixture","object":"response","status":"completed","model":"fixture","output":[{"type":"function_call","id":"item_1","call_id":"call_1","name":"read_nonce","arguments":"{\"nonce\":\"test\"}","status":"completed"}],"usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15}})
        }
        "openai:chat" => {
            json!({"id":"chat_fixture","object":"chat.completion","created":1,"model":"fixture","choices":[{"index":0,"finish_reason":"tool_calls","message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"read_nonce","arguments":"{\"nonce\":\"test\"}"}}]}}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}})
        }
        "claude:messages" => {
            json!({"id":"msg_fixture","type":"message","role":"assistant","model":"fixture","content":[{"type":"tool_use","id":"call_1","name":"read_nonce","input":{"nonce":"test"}}],"stop_reason":"tool_use","usage":{"input_tokens":10,"output_tokens":5}})
        }
        "gemini:generate_content" => {
            json!({"responseId":"gemini_fixture","modelVersion":"fixture","candidates":[{"index":0,"finishReason":"STOP","content":{"role":"model","parts":[{"functionCall":{"name":"read_nonce","args":{"nonce":"test"}}}]}}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5,"totalTokenCount":15}})
        }
        _ => unreachable!(),
    }
}

#[test]
fn three_internal_client_protocols_round_trip_function_calls_from_four_upstream_formats() {
    for client in CLIENTS {
        for upstream in UPSTREAMS {
            let original = request(client);
            let context = FormatContext::default().with_mapped_model("fixture").with_report_context(json!({
                "client_api_format":client,"provider_api_format":upstream,"original_request_body":original,
            }));
            let mapped = convert_request(client, upstream, &original, &context)
                .unwrap_or_else(|e| panic!("{client} -> {upstream}: {e}"));
            assert!(
                mapped.to_string().contains("read_nonce"),
                "{client} -> {upstream} lost tool"
            );
            let converted = convert_response(upstream, client, &response(upstream), &context)
                .unwrap_or_else(|e| panic!("{upstream} -> {client}: {e}"));
            let (name, args) = match client {
                "openai:responses" => {
                    let call = converted["output"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|item| item["type"] == "function_call")
                        .unwrap();
                    (
                        call["name"].as_str().unwrap(),
                        serde_json::from_str::<Value>(call["arguments"].as_str().unwrap()).unwrap(),
                    )
                }
                "openai:chat" => {
                    let call = &converted["choices"][0]["message"]["tool_calls"][0]["function"];
                    (
                        call["name"].as_str().unwrap(),
                        serde_json::from_str::<Value>(call["arguments"].as_str().unwrap()).unwrap(),
                    )
                }
                "claude:messages" => {
                    let call = converted["content"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|item| item["type"] == "tool_use")
                        .unwrap();
                    (call["name"].as_str().unwrap(), call["input"].clone())
                }
                _ => unreachable!(),
            };
            assert_eq!(name, "read_nonce", "{upstream} -> {client}");
            assert_eq!(args, json!({"nonce":"test"}), "{upstream} -> {client}");
            let mut continuation = original.clone();
            match client {
                "openai:responses" => {
                    let calls = converted["output"].as_array().unwrap();
                    let call = calls
                        .iter()
                        .find(|item| item["type"] == "function_call")
                        .unwrap();
                    continuation["input"]
                        .as_array_mut()
                        .unwrap()
                        .push(call.clone());
                    continuation["input"].as_array_mut().unwrap().push(json!({"type":"function_call_output","call_id":call["call_id"],"output":"nonce_read_success"}));
                }
                "openai:chat" => {
                    let message = &converted["choices"][0]["message"];
                    continuation["messages"]
                        .as_array_mut()
                        .unwrap()
                        .push(message.clone());
                    continuation["messages"].as_array_mut().unwrap().push(json!({"role":"tool","tool_call_id":message["tool_calls"][0]["id"],"content":"nonce_read_success"}));
                }
                "claude:messages" => {
                    let content = &converted["content"];
                    let call = content
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|item| item["type"] == "tool_use")
                        .unwrap();
                    continuation["messages"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"role":"assistant","content":content}));
                    continuation["messages"].as_array_mut().unwrap().push(json!({"role":"user","content":[{"type":"tool_result","tool_use_id":call["id"],"content":"nonce_read_success"}]}));
                }
                _ => unreachable!(),
            }
            let next_context = FormatContext::default().with_mapped_model("fixture").with_report_context(json!({"client_api_format":client,"provider_api_format":upstream,"original_request_body":continuation}));
            let next = convert_request(client, upstream, &continuation, &next_context)
                .unwrap_or_else(|e| panic!("{client} -> {upstream} tool result replay: {e}"));
            assert!(
                next.to_string().contains("nonce_read_success"),
                "{client} -> {upstream} lost tool result"
            );
        }
    }
}
