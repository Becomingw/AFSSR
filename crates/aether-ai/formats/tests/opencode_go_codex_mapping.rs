use aether_ai_formats::formats::shared::stream_core::format_matrix::StreamingStandardFormatMatrix;
use aether_ai_formats::{convert_request, convert_response, FormatContext};
use serde_json::{json, Value};

fn body() -> Value {
    json!({"model":"gpt-6.1-sol", "stream":true, "store":false,
        "input":[{"id":"tools_fixture", "type":"additional_tools", "role":"developer", "tools":[
            {"type":"namespace", "name":"functions", "description":"Synthetic tools", "tools":[
                {"type":"custom", "name":"exec", "description":"Execute synthetic JavaScript",
                 "format":{"type":"grammar", "syntax":"lark", "definition":"start: /[a-z]+/"}}]}]},
            {"role":"developer", "content":[{"type":"input_text", "text":"Use the tools."}]},
            {"role":"user", "content":[{"type":"input_text", "text":"Read a nonce."}]}],
        "reasoning":{"effort":"medium", "context":"all_turns"}, "text":{"verbosity":"low"},
        "tool_choice":"auto", "parallel_tool_calls":true,
        "include":["reasoning.encrypted_content"], "prompt_cache_key":"synthetic", "client_metadata":{"session":"fixture"}})
}

fn context(body: &Value, format: &str, model: &str) -> FormatContext {
    FormatContext::default().with_mapped_model(model).with_upstream_stream(true)
        .with_report_context(json!({"provider_type":"opencode_go", "provider_api_format":format, "mapped_model":model,
            "client_api_format":"openai:responses", "needs_conversion":true, "original_request_body":body}))
}

#[test]
fn fresh_codex_lite_request_maps_to_chat_and_messages() {
    let body = body();
    for (format, model) in [
        ("openai:chat", "space-bunny"),
        ("claude:messages", "claude-haiku-5-5"),
    ] {
        let mapped = convert_request(
            "openai:responses",
            format,
            &body,
            &context(&body, format, model),
        )
        .unwrap();
        assert_eq!(mapped["model"], model);
        assert_eq!(mapped["tools"].as_array().unwrap().len(), 1);
        if format == "openai:chat" {
            assert!(mapped.get("verbosity").is_none());
            assert!(mapped["messages"]
                .to_string()
                .contains("Keep the final answer concise."));
            assert_eq!(mapped["tools"][0]["type"], "function");
            assert_eq!(
                mapped["tools"][0]["function"]["parameters"]["required"],
                json!(["input"])
            );
        } else {
            assert_eq!(
                mapped["tools"][0]["input_schema"]["required"],
                json!(["input"])
            );
            assert!(mapped["system"].to_string().contains("Use the tools."));
            assert!(mapped["messages"].to_string().contains("Read a nonce."));
        }
    }
}

#[test]
fn go_gemini_lite_custom_tools_round_trip() {
    let original = body();
    let ctx = context(
        &original,
        "gemini:generate_content",
        "gemini-4-argon-contributor",
    );
    let native = convert_request(
        "openai:responses",
        "gemini:generate_content",
        &original,
        &ctx,
    )
    .unwrap();
    assert_eq!(
        native["tools"][0]["functionDeclarations"][0]["name"],
        "exec"
    );
    assert!(native["tools"][0]["functionDeclarations"][0]["parametersJsonSchema"].is_object());
    assert!(native["systemInstruction"]
        .to_string()
        .contains("Keep the final answer concise."));
    let response = json!({"responseId":"gemini_fixture", "modelVersion":"gemini-4-argon-contributor",
    "candidates":[{"index":0,"finishReason":"STOP","content":{"role":"model","parts":[
        {"functionCall":{"name":"exec","args":{"input":"alpha"}},"thoughtSignature":"synthetic-signature"}
    ]}}]});
    let converted = convert_response(
        "gemini:generate_content",
        "openai:responses",
        &response,
        &ctx,
    )
    .unwrap();
    let call = converted["output"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["type"] == "custom_tool_call")
        .unwrap();
    assert_eq!(call["name"], "exec");
    assert_eq!(call["namespace"], "functions");
    assert_eq!(call["input"], "alpha");
    let mut replay = original.clone();
    replay["input"]
        .as_array_mut()
        .unwrap()
        .extend(converted["output"].as_array().unwrap().iter().cloned());
    replay["input"].as_array_mut().unwrap().push(json!({"type":"custom_tool_call_output","call_id":call["call_id"],"output":"synthetic-result"}));
    let replay_native =
        convert_request("openai:responses", "gemini:generate_content", &replay, &ctx).unwrap();
    assert!(replay_native["contents"]
        .to_string()
        .contains("synthetic-signature"));
    assert!(replay_native["contents"]
        .to_string()
        .contains("synthetic-result"));
    assert!(replay_native["contents"].to_string().contains("alpha"));

    use aether_ai_formats::formats::shared::stream_rewrite::maybe_build_ai_surface_stream_rewriter;
    let mut rewriter = maybe_build_ai_surface_stream_rewriter(ctx.report_context.as_ref()).unwrap();
    let chunk = format!("data: {response}\n\n");
    let wire = rewriter.push_chunk(chunk.as_bytes()).unwrap();
    let final_wire = rewriter.finish().unwrap();
    let mut wire = String::from_utf8(wire).unwrap();
    wire.push_str(&String::from_utf8(final_wire).unwrap());
    assert!(wire.contains("custom_tool_call"));
    assert!(wire.contains("\"namespace\":\"functions\""));
    assert!(wire.contains("response.completed"));
    let completed: Value = wire
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|event| event["type"] == "response.completed")
        .unwrap();
    let mut replay = original;
    replay["input"].as_array_mut().unwrap().extend(
        completed["response"]["output"]
            .as_array()
            .unwrap()
            .iter()
            .cloned(),
    );
    let replay_native =
        convert_request("openai:responses", "gemini:generate_content", &replay, &ctx).unwrap();
    assert!(replay_native["contents"]
        .to_string()
        .contains("synthetic-signature"));
}

#[test]
fn go_gemini_preserves_json_schema_instead_of_openapi_cleaning() {
    let schema = json!({"type":"object","properties":{
        "questions":{"type":"array","minItems":1,"maxItems":3,"items":{
            "type":"object","properties":{"choice":{"type":"integer","enum":[1,2]}},
            "required":["choice"],"additionalProperties":false}}
        },"required":["questions"],"additionalProperties":false});
    let original = json!({"model":"gpt-6.1-sol","input":"synthetic",
        "tools":[{"type":"function","name":"ask","parameters":schema}]});
    let native = convert_request(
        "openai:responses",
        "gemini:generate_content",
        &original,
        &context(
            &original,
            "gemini:generate_content",
            "gemini-4-argon-contributor",
        ),
    )
    .unwrap();
    assert_eq!(
        native["tools"][0]["functionDeclarations"][0]["parametersJsonSchema"],
        schema
    );
    assert!(native["tools"][0]["functionDeclarations"][0]
        .get("parameters")
        .is_none());
}

#[test]
fn go_muse_responses_functionizes_custom_tools_and_restores_identity() {
    use aether_ai_formats::provider_compat::opencode_go::{
        adapt_native_responses_request_for_model, restore_native_responses_payload,
    };
    for model in ["muse-spark-1.2-contributor", "muse-spark-1.3-contributor"] {
        let original = body();
        let ctx = context(&original, "openai:responses", model);
        let mut native = original.clone();
        adapt_native_responses_request_for_model(&mut native, model).unwrap();
        assert_eq!(native["tools"][0]["type"], "function");
        let mut response = json!({"id":"muse_fixture","status":"completed","output":[{
            "type":"function_call","id":"fc_fixture","call_id":"call_fixture","name":"exec","arguments":"{\"input\":\"alpha\"}"}]});
        assert!(restore_native_responses_payload(
            &mut response,
            ctx.report_context.as_ref().unwrap()
        )
        .unwrap());
        let call = &response["output"][0];
        assert_eq!(call["type"], "custom_tool_call");
        assert_eq!(call["namespace"], "functions");
        assert_eq!(call["input"], "alpha");
    }
}

#[test]
fn muse_13_root_combinators_keep_native_schema_and_unwrapped_arguments() {
    use aether_ai_formats::provider_compat::opencode_go::{
        adapt_native_responses_request_for_model, restore_native_responses_payload,
    };
    for operator in ["oneOf", "anyOf", "allOf"] {
        let schema = json!({operator:[{"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}]});
        let mut original = body();
        original["input"][0]["tools"] = json!([{"type":"namespace","name":"functions","description":"Synthetic tools","tools":[
            {"type":"function","name":"probe","description":"Read a synthetic value","parameters":schema}]}]);
        let ctx = context(&original, "openai:responses", "muse-spark-1.3-contributor");
        let mut native = original.clone();
        adapt_native_responses_request_for_model(&mut native, "muse-spark-1.3-contributor")
            .unwrap();
        assert_eq!(native["tools"][0]["parameters"], schema);
        let alias = native["tools"][0]["name"].clone();
        let mut response = json!({"id":"muse_union_fixture","status":"completed","output":[{
            "type":"function_call","id":"fc_union_fixture","call_id":"call_union_fixture",
            "name":alias,"arguments":"{\"label\":\"alpha\"}"}]});
        assert!(restore_native_responses_payload(
            &mut response,
            ctx.report_context.as_ref().unwrap()
        )
        .unwrap());
        let call = &response["output"][0];
        assert_eq!(call["namespace"], "functions");
        assert_eq!(call["name"], "probe");
        assert_eq!(call["arguments"], "{\"label\":\"alpha\"}");
        original["input"].as_array_mut().unwrap().extend([
            call.clone(),json!({"type":"function_call_output","call_id":"call_union_fixture","output":"synthetic_result"})
        ]);
        adapt_native_responses_request_for_model(&mut original, "muse-spark-1.3-contributor")
            .unwrap();
        let history = original["input"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["type"] == "function_call")
            .unwrap();
        assert_eq!(history["arguments"], call["arguments"]);
    }
}

#[test]
fn go_native_messages_union_profiles_preserve_the_original_schema() {
    let schema = json!({"oneOf":[{"type":"object","properties":{"label":{"type":"string"}},"required":["label"]},{"type":"object","properties":{"number":{"type":"integer"}},"required":["number"]}]});
    let mut original = body();
    original["input"][0]["tools"] = json!([{"type":"function","name":"probe","parameters":schema}]);
    for model in ["qwen3.7-max", "minimax-m2.5"] {
        let ctx = context(&original, "claude:messages", model);
        let native =
            convert_request("openai:responses", "claude:messages", &original, &ctx).unwrap();
        assert_eq!(native["tools"][0]["input_schema"]["oneOf"], schema["oneOf"]);
        assert!(native["tools"][0]["input_schema"]["properties"]
            .get("aether_input")
            .is_none());
        let response = json!({"id":"union_fixture","model":model,"type":"message","role":"assistant","content":[{"type":"tool_use","id":"call_fixture","name":"probe","input":{"label":"alpha"}}],"stop_reason":"tool_use"});
        let converted =
            convert_response("claude:messages", "openai:responses", &response, &ctx).unwrap();
        assert_eq!(converted["output"][0]["arguments"], "{\"label\":\"alpha\"}");
    }
    let native = convert_request(
        "openai:responses",
        "claude:messages",
        &original,
        &context(&original, "claude:messages", "claude-haiku-5-5"),
    )
    .unwrap();
    assert!(native["tools"][0]["input_schema"]["properties"]["aether_input"].is_object());
}

#[test]
fn go_gemini_keeps_basic_native_web_tools_and_rejects_unmapped_controls() {
    let mut original = json!({"model":"gpt-6.1-sol","stream":true,"store":false,
        "input":[{"role":"user","content":"Search for a synthetic documentation example."}],
        "tools":[{"type":"web_search"}]});
    let ctx = context(
        &original,
        "gemini:generate_content",
        "gemini-4-argon-contributor",
    );
    let native = convert_request(
        "openai:responses",
        "gemini:generate_content",
        &original,
        &ctx,
    )
    .unwrap();
    assert!(native["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool.get("googleSearch").is_some()));
    original["tools"][0]["external_web_access"] = json!(false);
    assert!(convert_request(
        "openai:responses",
        "gemini:generate_content",
        &original,
        &ctx
    )
    .is_err());
}

#[test]
fn native_responses_aliases_namespace_and_restores_sync_and_sse() {
    use aether_ai_formats::formats::shared::stream_rewrite::maybe_build_ai_surface_stream_rewriter;
    use aether_ai_formats::provider_compat::opencode_go::{
        adapt_native_responses_request, restore_native_responses_payload,
    };
    let original = body();
    let ctx = context(&original, "openai:responses", "gpt-6-luna")
        .report_context
        .unwrap();
    let mut request = original.clone();
    adapt_native_responses_request(&mut request).unwrap();
    assert_eq!(request["tools"][0]["type"], "custom");
    assert_eq!(request["tools"][0]["name"], "functions__exec");
    assert_eq!(request["input"][0]["role"], "developer");
    let call = json!({"type":"custom_tool_call", "id":"ctc_fixture", "call_id":"call_fixture", "name":"functions__exec", "input":"alpha"});
    let mut response = json!({"id":"resp_fixture", "output":[call.clone()]});
    assert!(restore_native_responses_payload(&mut response, &ctx).unwrap());
    assert_eq!(response["output"][0]["namespace"], "functions");
    assert_eq!(response["output"][0]["name"], "exec");
    let mut rewriter = maybe_build_ai_surface_stream_rewriter(Some(&ctx)).unwrap();
    for event in [
        json!({"type":"response.output_item.added", "item":call.clone()}),
        json!({"type":"response.output_item.done", "item":call}),
        json!({"type":"response.completed", "response":{"id":"resp_fixture", "status":"completed", "output":response["output"]}}),
    ] {
        let wire = format!("data: {event}\n\n");
        let output = String::from_utf8(rewriter.push_chunk(wire.as_bytes()).unwrap()).unwrap();
        assert!(output.contains("\"namespace\":\"functions\""));
        assert!(!output.contains("functions__exec"));
    }
    let mut next = original;
    next["input"]
        .as_array_mut()
        .unwrap()
        .extend(response["output"].as_array().unwrap().iter().cloned());
    adapt_native_responses_request(&mut next).unwrap();
    assert_eq!(
        next["input"].as_array().unwrap().last().unwrap()["name"],
        "functions__exec"
    );
}

#[test]
fn native_alias_collision_is_rejected_not_dispatched_to_wrong_tool() {
    use aether_ai_formats::provider_compat::opencode_go::adapt_native_responses_request;
    let mut request = body();
    request["tools"] =
        json!([{"type":"function", "name":"functions__exec", "parameters":{"type":"object"}}]);
    assert!(adapt_native_responses_request(&mut request)
        .unwrap_err()
        .reason
        .contains("collision"));
}

#[test]
fn native_responses_sync_omits_only_stream_transport_options() {
    use aether_ai_formats::provider_compat::opencode_go::adapt_native_responses_request_for_model;
    for model in ["gpt-6-luna", "grok-4.6"] {
        let mut request = body();
        request["stream"] = json!(false);
        request["stream_options"] = json!({"include_obfuscation": true});
        adapt_native_responses_request_for_model(&mut request, model).unwrap();
        assert!(request.get("stream_options").is_none());
        assert_eq!(request["reasoning"]["effort"], "medium");
        let mut streaming = body();
        streaming["stream_options"] = json!({"include_obfuscation": true});
        adapt_native_responses_request_for_model(&mut streaming, model).unwrap();
        assert_eq!(streaming["stream_options"]["include_obfuscation"], true);
    }
}

#[test]
fn grok_replay_omits_null_content_without_losing_encrypted_reasoning() {
    use aether_ai_formats::provider_compat::opencode_go::adapt_native_responses_request_for_model;
    for model in ["grok-4.6", "grok-4.7"] {
        let mut request = body();
        let null = json!({"type":"reasoning","id":"rs_signed","summary":[],
            "content":null,"encrypted_content":"synthetic-unchanged-ciphertext"});
        let array = json!({"type":"reasoning","id":"rs_array","summary":[],
            "content":[{"type":"reasoning_text","text":"synthetic"}],
            "encrypted_content":"synthetic-second-ciphertext"});
        request["input"]
            .as_array_mut()
            .unwrap()
            .extend([null, array.clone()]);
        adapt_native_responses_request_for_model(&mut request, model).unwrap();
        let input = request["input"].as_array().unwrap();
        let signed = input.iter().find(|i| i["id"] == "rs_signed").unwrap();
        assert!(signed.get("content").is_none());
        assert_eq!(
            signed["encrypted_content"],
            "synthetic-unchanged-ciphertext"
        );
        assert_eq!(signed["summary"], json!([]));
        assert_eq!(
            input.iter().find(|i| i["id"] == "rs_array").unwrap(),
            &array
        );
    }
}

#[test]
fn grok_native_function_bridge_restores_custom_tools_in_sync_sse_and_continuation() {
    use aether_ai_formats::formats::shared::stream_rewrite::maybe_build_ai_surface_stream_rewriter;
    use aether_ai_formats::formats::shared::sync_products::maybe_build_openai_responses_same_family_sync_body_from_normalized_payload;
    use aether_ai_formats::provider_compat::opencode_go::adapt_native_responses_request_for_model;
    let original = body();
    let mut ctx = context(&original, "openai:responses", "grok-4.6");
    ctx.report_context.as_mut().unwrap()["needs_conversion"] = json!(false);
    let report = ctx.report_context.as_ref().unwrap();
    let mut native = original.clone();
    adapt_native_responses_request_for_model(&mut native, "grok-4.6").unwrap();
    assert_eq!(native["tools"][0]["type"], "function");
    let name = native["tools"][0]["name"].clone();
    let call = json!({"type":"function_call", "id":"fc_grok_fixture", "status":"completed", "call_id":"call_grok_fixture", "name":name,"arguments":"{\"input\":\"alpha\"}"});
    let reasoning = json!({"type":"reasoning", "id":"rs_grok_fixture", "summary":[], "encrypted_content":"synthetic-grok-encrypted-content"});
    let upstream = json!({"id":"resp_grok_fixture", "object":"response", "model":"grok-4.6", "status":"completed", "output":[reasoning.clone(),call.clone()]});
    let sync = maybe_build_openai_responses_same_family_sync_body_from_normalized_payload(
        "openai_responses_sync_finalize",
        200,
        Some(report),
        Some(&upstream),
        None,
    )
    .unwrap()
    .unwrap();
    let sync_call = sync["output"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["type"] == "custom_tool_call")
        .unwrap();
    assert_eq!(sync_call["input"], "alpha");
    assert_eq!(sync_call["namespace"], "functions");
    let product = aether_ai_formats::formats::shared::sync_products::maybe_build_standard_sync_finalize_product_from_normalized_payload(
        "openai_responses_sync_finalize", 200, Some(report), Some(&upstream), None,
    ).unwrap().unwrap();
    let aether_ai_formats::formats::shared::sync_products::StandardSyncFinalizeNormalizedProduct::SuccessBody(sync) = product else {
        panic!("expected restored native success body");
    };
    assert_eq!(sync["output"][1]["type"], "custom_tool_call");
    assert_eq!(sync["output"][1]["input"], "alpha");
    assert_eq!(sync["output"][0], reasoning);
    let frames = [
        json!({"type":"response.created","response":{"id":"resp_grok_fixture", "model":"grok-4.6", "status":"in_progress", "output":[]}}),
        json!({"type":"response.output_item.added","output_index":0,"sequence_number":139,"item":reasoning.clone()}),
        json!({"type":"response.reasoning_summary_text.delta","output_index":0,"item_id":"rs_grok_fixture", "summary_index":0,"delta":"synthetic reasoning", "sequence_number":140}),
        json!({"type":"response.output_item.done","output_index":0,"item":reasoning}),
        json!({"type":"response.output_item.added","output_index":1,"item":{"type":"function_call", "id":"fc_grok_fixture", "call_id":"call_grok_fixture", "name":name, "arguments":""}}),
        json!({"type":"response.function_call_arguments.delta","item_id":"fc_grok_fixture", "output_index":1,"delta":"{\"input\":\"alpha\"}"}),
        json!({"type":"response.function_call_arguments.done","item_id":"fc_grok_fixture", "output_index":1,"arguments":"{\"input\":\"alpha\"}"}),
        json!({"type":"response.output_item.done","output_index":1,"item":call}),
        json!({"type":"response.completed","response":upstream}),
    ];
    let mut rewriter = maybe_build_ai_surface_stream_rewriter(Some(report)).unwrap();
    let mut wire = Vec::new();
    for frame in frames {
        wire.extend(
            rewriter
                .push_chunk(format!("data: {frame}\n\n").as_bytes())
                .unwrap(),
        );
    }
    wire.extend(rewriter.finish().unwrap());
    let text = String::from_utf8(wire).unwrap();
    let events: Vec<Value> = text
        .lines()
        .filter_map(|s| s.strip_prefix("data: "))
        .filter_map(|s| serde_json::from_str::<Value>(s).ok())
        .collect();
    let sequences: Vec<u64> = events
        .iter()
        .filter_map(|e| e["sequence_number"].as_u64())
        .collect();
    assert_eq!(sequences, (1..=events.len() as u64).collect::<Vec<_>>());
    assert!(
        text.contains("response.custom_tool_call_input.delta"),
        "{text}"
    );
    assert!(text.contains("\"namespace\":\"functions\""), "{text}");
    assert!(text.contains("synthetic-grok-encrypted-content"), "{text}");
    let completed: Value = text
        .lines()
        .filter_map(|s| s.strip_prefix("data: "))
        .filter_map(|s| serde_json::from_str::<Value>(s).ok())
        .find(|e| e["type"] == "response.completed")
        .unwrap();
    let restored = completed["response"]["output"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["type"] == "custom_tool_call")
        .unwrap();
    assert_eq!(restored["input"], "alpha");
    assert_eq!(restored["name"], "exec");
    let mut next = original;
    next["previous_response_id"] = json!("resp_grok_fixture");
    next["input"] = json!([{"type":"custom_tool_call_output", "call_id":"call_grok_fixture", "output":"nonce"}]);
    adapt_native_responses_request_for_model(&mut next, "grok-4.6").unwrap();
    assert_eq!(next["input"][0]["type"], "function_call_output");
    assert_eq!(next["input"][0]["call_id"], "call_grok_fixture");
}

#[test]
fn deepseek_root_unions_wrap_without_losing_constraints_or_replay_identity() {
    for model in [
        "deepseek-flash",
        "deepseek-v4-flash",
        "deepseek-v4.1-flash",
        "deepseek-v4-pro",
    ] {
        for operator in ["oneOf", "anyOf", "allOf"] {
            let schema = json!({operator:[{"type":"object","properties":{"label":{"type":"string","enum":["alpha"]}},"required":["label"],"additionalProperties":false}]});
            let mut original = body();
            original["input"][0]["tools"] =
                json!([{"type":"function","name":"probe","parameters":schema}]);
            let ctx = context(&original, "openai:chat", model);
            let mapped =
                convert_request("openai:responses", "openai:chat", &original, &ctx).unwrap();
            let parameters = &mapped["tools"][0]["function"]["parameters"];
            assert_eq!(parameters["type"], "object");
            assert_eq!(parameters["properties"]["aether_input"], schema);
            assert_eq!(parameters["required"], json!(["aether_input"]));
            assert_eq!(parameters["additionalProperties"], false);
            let native = json!({"id":"chatcmpl_deepseek_union","choices":[{"index":0,
                "message":{"role":"assistant","tool_calls":[{"id":"call_deepseek_union","type":"function",
                    "function":{"name":"probe","arguments":"{\"aether_input\":{\"label\":\"alpha\"}}"}}]},"finish_reason":"tool_calls"}]});
            let response =
                convert_response("openai:chat", "openai:responses", &native, &ctx).unwrap();
            let call = response["output"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["type"] == "function_call")
                .unwrap();
            assert_eq!(call["arguments"], "{\"label\":\"alpha\"}");
            original["input"].as_array_mut().unwrap().extend([call.clone(),
                json!({"type":"function_call_output","call_id":call["call_id"],"output":"synthetic"})]);
            let replay = convert_request(
                "openai:responses",
                "openai:chat",
                &original,
                &context(&original, "openai:chat", model),
            )
            .unwrap();
            let native_call = replay["messages"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|message| message["tool_calls"].as_array())
                .flatten()
                .find(|call| call["id"] == "call_deepseek_union")
                .unwrap();
            let args: Value =
                serde_json::from_str(native_call["function"]["arguments"].as_str().unwrap())
                    .unwrap();
            assert_eq!(args, json!({"aether_input":{"label":"alpha"}}));
        }
    }
}

#[test]
fn stored_chat_union_call_preserves_native_schema_and_history() {
    let mut original = body();
    original["store"] = json!(true);
    original["input"].as_array_mut().unwrap().remove(0);
    original["tools"] = json!([{"type":"function", "name":"probe", "parameters":{"oneOf":[
        {"type":"object","properties":{"label":{"type":"string"}}, "required":["label"], "additionalProperties":false},
        {"type":"object","properties":{"number":{"type":"integer"}}, "required":["number"], "additionalProperties":false}]}}]);
    let scope = "go-history-union-synthetic";
    let mut ctx = context(&original, "openai:chat", "space-bunny").with_history_scope(scope);
    ctx.report_context.as_mut().unwrap()["api_key_id"] = json!(scope);
    let native = json!({"id":"chatcmpl_union_previous_fixture", "model":"space-bunny", "choices":[{
        "index":0,"message":{"role":"assistant", "content":null,"tool_calls":[{
            "id":"call_union_fixture","type":"function", "function":{"name":"probe", "arguments":"{\"label\":\"alpha\"}"}}]}, "finish_reason":"tool_calls"}]});
    let response = convert_response("openai:chat", "openai:responses", &native, &ctx).unwrap();
    assert_eq!(response["output"][0]["arguments"], "{\"label\":\"alpha\"}");
    original["previous_response_id"] = response["id"].clone();
    original["input"] =
        json!([{"type":"function_call_output", "call_id":"call_union_fixture", "output":"nonce"}]);
    let next = convert_request("openai:responses", "openai:chat", &original, &ctx).unwrap();
    let history_call = next["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m.get("tool_calls").is_some())
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(
            history_call["tool_calls"][0]["function"]["arguments"]
                .as_str()
                .unwrap()
        )
        .unwrap(),
        json!({"label":"alpha"})
    );
    assert_eq!(
        next["tools"][0]["function"]["parameters"],
        original["tools"][0]["parameters"]
    );
}

#[test]
fn schema_wrapping_is_protocol_specific_not_shared_between_go_chat_and_messages() {
    for operator in ["oneOf", "anyOf", "allOf"] {
        let mut original = body();
        original["input"].as_array_mut().unwrap().remove(0);
        let schema = json!({operator:[{"type":"object","properties":{"label":{"type":"string"}},"required":["label"]}]});
        original["tools"] = json!([{"type":"function","name":"probe","parameters":schema}]);
        let chat = convert_request(
            "openai:responses",
            "openai:chat",
            &original,
            &context(&original, "openai:chat", "mimo-v2.5"),
        )
        .unwrap();
        assert_eq!(chat["tools"][0]["function"]["parameters"], schema);
        let messages = convert_request(
            "openai:responses",
            "claude:messages",
            &original,
            &context(&original, "claude:messages", "claude-haiku-5-5"),
        )
        .unwrap();
        assert_eq!(
            messages["tools"][0]["input_schema"]["properties"]["aether_input"],
            schema
        );
        assert_eq!(
            messages["tools"][0]["input_schema"]["required"],
            json!(["aether_input"])
        );
    }
}

#[test]
fn messages_preserve_structured_format_and_fit_thinking_budget() {
    let mut body = body();
    body["max_output_tokens"] = json!(2048);
    body["text"] = json!({"format":{"type":"json_schema", "name":"probe",
        "strict":true, "schema":{"type":"object", "properties":{"nonce":{"type":"string"}}, "required":["nonce"], "additionalProperties":false}}});
    for model in ["claude-haiku-5-5", "qwen3.8-max", "minimax-m3"] {
        let mapped = convert_request(
            "openai:responses",
            "claude:messages",
            &body,
            &context(&body, "claude:messages", model),
        )
        .unwrap();
        assert_eq!(
            mapped["output_config"]["format"]["schema"],
            body["text"]["format"]["schema"]
        );
        if model == "qwen3.8-max" {
            assert!(mapped.get("thinking").is_none());
            assert_eq!(mapped["output_config"]["effort"], "medium");
        } else if model != "claude-haiku-5-5" {
            assert!(mapped["thinking"]["budget_tokens"].as_u64().unwrap() < 2048);
        }
        body["reasoning"]["effort"] = json!("none");
        let mapped = convert_request(
            "openai:responses",
            "claude:messages",
            &body,
            &context(&body, "claude:messages", model),
        )
        .unwrap();
        assert_eq!(mapped["thinking"]["type"], "disabled");
        body["reasoning"]["effort"] = json!("medium");
    }
}

#[test]
fn qwen_38_explicit_budget_and_disabled_thinking_do_not_conflict_with_effort() {
    use aether_ai_formats::provider_compat::opencode_go::adapt_messages_request;
    let mut body = body();
    let source = json!({"thinking":{"type":"enabled", "budget_tokens":2048}, "output_config":{"effort":"medium"}});
    let mut mapped = source.clone();
    adapt_messages_request(&mut mapped, &source, "qwen3.8-flash");
    assert_eq!(mapped["thinking"]["budget_tokens"], 2048);
    assert!(mapped["output_config"].get("effort").is_none());
    body["reasoning"]["effort"] = json!("none");
    let mapped = convert_request(
        "openai:responses",
        "claude:messages",
        &body,
        &context(&body, "claude:messages", "qwen3.8-flash"),
    )
    .unwrap();
    assert_eq!(mapped["thinking"]["type"], "disabled");
    assert!(mapped["output_config"].get("effort").is_none());
}

#[test]
fn messages_signed_thinking_survives_all_turns_for_non_claude_model_names() {
    let mut body = body();
    let signed = json!({"type":"thinking", "thinking":"Synthetic reasoning", "signature":"synthetic-signature"});
    let upstream = json!({"id":"msg_qwen_fixture", "model":"qwen3.7-plus", "type":"message", "role":"assistant", "content":[signed.clone(),
        {"type":"tool_use", "id":"call_fixture", "name":"exec", "input":{"input":"alpha"}}], "stop_reason":"tool_use"});
    let mapped = convert_response(
        "claude:messages",
        "openai:responses",
        &upstream,
        &context(&body, "claude:messages", "qwen3.7-plus"),
    )
    .unwrap();
    body["input"]
        .as_array_mut()
        .unwrap()
        .extend(mapped["output"].as_array().unwrap().iter().cloned());
    body["input"].as_array_mut().unwrap().push(
        json!({"type":"custom_tool_call_output", "call_id":"call_fixture", "output":"nonce"}),
    );
    let next = convert_request(
        "openai:responses",
        "claude:messages",
        &body,
        &context(&body, "claude:messages", "qwen3.7-plus"),
    )
    .unwrap();
    assert!(next["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["content"].as_array())
        .flatten()
        .any(|b| b == &signed));
}

#[test]
fn captured_synthetic_codex_request_converts_for_each_protocol_family() {
    let body: Value = serde_json::from_str(include_str!("fixtures/codex_lite_go.json")).unwrap();
    for (format, model) in [
        ("openai:chat", "space-bunny"),
        ("claude:messages", "claude-haiku-5-5"),
    ] {
        let mapped = convert_request(
            "openai:responses",
            format,
            &body,
            &context(&body, format, model),
        )
        .unwrap_or_else(|error| panic!("{format}: {error}"));
        assert!(mapped["tools"].as_array().unwrap().len() >= 10);
    }
}

fn code_mode_body() -> Value {
    serde_json::from_str(include_str!("fixtures/codex_lite_go.json")).unwrap()
}

#[test]
fn code_mode_helpers_project_only_explicitly_advertised_client_capabilities() {
    let body = code_mode_body();
    for (format, model) in [
        ("openai:chat", "mimo-v2.6-flash"),
        ("claude:messages", "minimax-m2.7"),
    ] {
        let mapped = convert_request(
            "openai:responses",
            format,
            &body,
            &context(&body, format, model),
        )
        .unwrap();
        let names = mapped["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| {
                if format == "openai:chat" {
                    tool["function"]["name"].as_str().unwrap()
                } else {
                    tool["name"].as_str().unwrap()
                }
            })
            .collect::<Vec<_>>();
        assert!(names.contains(&"exec_command"), "{names:?}");
        assert!(names.contains(&"apply_patch"), "{names:?}");
        assert!(names.contains(&"list_client_tools"), "{names:?}");
        assert!(names.contains(&"call_client_tool"), "{names:?}");
        assert!(names
            .iter()
            .any(|name| name.contains("exec") && *name != "exec_command"));
    }
    let mut non_go = context(&body, "claude:messages", "claude-haiku-5-5");
    non_go.report_context.as_mut().unwrap()["provider_type"] = json!("claude");
    let mapped = convert_request("openai:responses", "claude:messages", &body, &non_go).unwrap();
    assert!(!mapped["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == "exec_command" || t["name"] == "apply_patch"));
}

#[test]
fn projected_helpers_restore_sync_and_replay_exact_native_arguments() {
    for (format, model) in [
        ("openai:chat", "mimo-v2.6-flash"),
        ("claude:messages", "minimax-m2.7"),
    ] {
        for (name, arguments) in [
            (
                "exec_command",
                json!({"cmd":"pwd", "max_output_tokens":1000}),
            ),
            (
                "apply_patch",
                json!({"patch":"*** Begin Patch\n*** Add File: nonce\n+\"literal\"\n*** End Patch"}),
            ),
            ("list_client_tools", json!({"query":"synthetic"})),
            (
                "call_client_tool",
                json!({"name":"synthetic_echo","arguments":{"literal":"opaque"}}),
            ),
        ] {
            let mut body = code_mode_body();
            let ctx = context(&body, format, model);
            let upstream = if format == "openai:chat" {
                json!({"id":"chatcmpl_helper", "model":model,"choices":[{"index":0,"message":{"role":"assistant","content":null,
                    "tool_calls":[{"id":"call_helper","type":"function","function":{"name":name,"arguments":arguments.to_string()}}]},"finish_reason":"tool_calls"}]})
            } else {
                json!({"id":"msg_helper", "model":model,"type":"message","role":"assistant","stop_reason":"tool_use",
                    "content":[{"type":"tool_use","id":"call_helper","name":name,"input":arguments}]})
            };
            let response = convert_response(format, "openai:responses", &upstream, &ctx).unwrap();
            let call = response["output"]
                .as_array()
                .unwrap()
                .iter()
                .find(|i| i["type"] == "custom_tool_call")
                .unwrap()
                .clone();
            assert_eq!(call["name"], "exec");
            assert_eq!(call["namespace"], "functions");
            let input = call["input"].as_str().unwrap();
            assert!(input.contains("AFSSR_CODE_MODE_HELPER_V1:"));
            if matches!(name, "exec_command" | "apply_patch") {
                assert!(input.contains(&format!("text(await tools.{name}(")));
            } else {
                assert!(input.contains("typeof ALL_TOOLS"));
            }
            body["input"].as_array_mut().unwrap().extend([call,
                json!({"type":"custom_tool_call_output","call_id":"call_helper","output":"synthetic result"})]);
            let next = convert_request(
                "openai:responses",
                format,
                &body,
                &context(&body, format, model),
            )
            .unwrap();
            let messages = next["messages"].as_array().unwrap();
            if format == "openai:chat" {
                let replay = messages
                    .iter()
                    .find(|m| m["tool_calls"][0]["id"] == "call_helper")
                    .unwrap();
                assert_eq!(replay["tool_calls"][0]["function"]["name"], name);
                assert_eq!(
                    serde_json::from_str::<Value>(
                        replay["tool_calls"][0]["function"]["arguments"]
                            .as_str()
                            .unwrap()
                    )
                    .unwrap(),
                    arguments
                );
            } else {
                let replay = messages
                    .iter()
                    .filter_map(|m| m["content"].as_array())
                    .flatten()
                    .find(|i| i["type"] == "tool_use" && i["id"] == "call_helper")
                    .unwrap();
                assert_eq!(replay["name"], name);
                assert_eq!(replay["input"], arguments);
            }
        }
    }
}

#[test]
fn projected_helper_stream_emits_client_custom_lifecycle_without_native_alias() {
    let body = code_mode_body();
    let ctx = context(&body, "openai:chat", "mimo-v2.6-flash");
    let frames = [
        json!({"id":"chatcmpl_helper_stream", "model":"mimo-v2.6-flash","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_helper","type":"function","function":{"name":"exec_command","arguments":""}}]},"finish_reason":null}]}),
        json!({"id":"chatcmpl_helper_stream", "choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"cmd\":\"pwd\"}"}}]},"finish_reason":null}]}),
        json!({"id":"chatcmpl_helper_stream", "choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}),
    ];
    let mut matrix = StreamingStandardFormatMatrix::default();
    let mut bytes = Vec::new();
    for frame in frames {
        bytes.extend(
            matrix
                .transform_line(
                    ctx.report_context.as_ref().unwrap(),
                    format!("data: {frame}\n").into_bytes(),
                )
                .unwrap(),
        );
    }
    bytes.extend(matrix.finish(ctx.report_context.as_ref().unwrap()).unwrap());
    let events = String::from_utf8(bytes)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    assert!(events
        .iter()
        .any(|e| e["type"] == "response.custom_tool_call_input.done"
            && e["input"].as_str().unwrap().contains("tools.exec_command")));
    for item in events.iter().filter_map(|e| e.get("item")) {
        assert_ne!(item["name"], "exec_command");
        if item["type"] == "custom_tool_call" {
            assert_eq!(item["namespace"], "functions");
            assert_eq!(item["name"], "exec");
        }
    }
}

#[test]
fn projected_helpers_do_not_shadow_explicit_client_function_names() {
    let mut body = code_mode_body();
    body["tools"] = json!([{"type":"function","name":"exec_command","parameters":{"type":"object","properties":{"literal":{"type":"string"}}}}]);
    let mapped = convert_request(
        "openai:responses",
        "openai:chat",
        &body,
        &context(&body, "openai:chat", "mimo-v2.6-flash"),
    )
    .unwrap();
    let matches = mapped["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["function"]["name"] == "exec_command")
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1);
    assert!(matches[0]["function"]["parameters"]["properties"]
        .get("literal")
        .is_some());
}

#[test]
fn invalid_projected_helper_returns_a_safe_tool_error_without_breaking_sse() {
    let mut original = code_mode_body();
    let ctx = context(&original, "openai:chat", "mimo-v2.5");
    let upstream = json!({"id":"chatcmpl_invalid_helper","choices":[{"index":0,
        "message":{"role":"assistant","tool_calls":[{"id":"call_invalid_patch","type":"function",
        "function":{"name":"apply_patch","arguments":"{}"}}]},"finish_reason":"tool_calls"}]});
    let response = convert_response("openai:chat", "openai:responses", &upstream, &ctx).unwrap();
    let call = response["output"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["type"] == "custom_tool_call")
        .unwrap();
    assert!(call["input"].as_str().unwrap().contains("throw new Error("));
    assert!(call["input"]
        .as_str()
        .unwrap()
        .contains("No tool was executed"));
    assert!(!call["input"]
        .as_str()
        .unwrap()
        .contains("tools.apply_patch("));
    original["input"].as_array_mut().unwrap().push(call.clone());
    original["input"].as_array_mut().unwrap().push(json!({"type":"custom_tool_call_output",
        "call_id":call["call_id"],"output":"Invalid arguments for apply_patch: provide a patch string."}));
    let replay = convert_request(
        "openai:responses",
        "openai:chat",
        &original,
        &context(&original, "openai:chat", "mimo-v2.5"),
    )
    .unwrap();
    let native = replay["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["tool_calls"].as_array())
        .flatten()
        .find(|call| call["id"] == "call_invalid_patch")
        .unwrap();
    assert_eq!(native["function"]["name"], "apply_patch");
    assert_eq!(native["function"]["arguments"], "{}");
    let mut matrix = StreamingStandardFormatMatrix::default();
    let mut bytes = Vec::new();
    for frame in [
        json!({"id":"chatcmpl_invalid_helper","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_invalid_patch","type":"function","function":{"name":"apply_patch","arguments":"{}"}}]},"finish_reason":null}]}),
        json!({"id":"chatcmpl_invalid_helper","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}),
    ] {
        bytes.extend(
            matrix
                .transform_line(
                    ctx.report_context.as_ref().unwrap(),
                    format!("data: {frame}\n").into_bytes(),
                )
                .unwrap(),
        );
    }
    bytes.extend(matrix.finish(ctx.report_context.as_ref().unwrap()).unwrap());
    let stream = String::from_utf8(bytes).unwrap();
    assert!(stream.contains("response.completed"));
    assert!(stream.contains("AFSSR_CODE_MODE_HELPER_ERROR_V1:"));
    assert!(!stream.contains("response.failed"));
}

#[test]
fn invalid_helper_history_preserves_client_error_content_and_malformed_arguments() {
    for model in ["longcat-2.0", "mimo-v2.5", "deepseek-v4-flash"] {
        for arguments in [
            "{}",
            "{\"name\":\"apply_patch\",\"arguments\":*** Begin Patch}",
        ] {
            let mut original = code_mode_body();
            let ctx = context(&original, "openai:chat", model);
            let upstream = json!({"id":"chatcmpl_invalid_history","choices":[{"index":0,
                "message":{"role":"assistant","tool_calls":[{"id":"call_invalid_history","type":"function",
                "function":{"name":"call_client_tool","arguments":arguments}}]},"finish_reason":"tool_calls"}]});
            let response =
                convert_response("openai:chat", "openai:responses", &upstream, &ctx).unwrap();
            let call = response["output"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["type"] == "custom_tool_call")
                .unwrap();
            original["input"].as_array_mut().unwrap().push(call.clone());
            original["input"].as_array_mut().unwrap().push(json!({
                "type":"custom_tool_call_output","call_id":call["call_id"],
                "output":[{"type":"input_text","text":"Error: Invalid arguments. No tool was executed."}]
            }));
            let replay = convert_request(
                "openai:responses",
                "openai:chat",
                &original,
                &context(&original, "openai:chat", model),
            )
            .unwrap();
            let messages = replay["messages"].as_array().unwrap();
            let native = messages
                .iter()
                .filter_map(|message| message["tool_calls"].as_array())
                .flatten()
                .find(|call| call["id"] == "call_invalid_history")
                .unwrap();
            assert_eq!(native["function"]["name"], "call_client_tool");
            let replay_arguments: Value =
                serde_json::from_str(native["function"]["arguments"].as_str().unwrap()).unwrap();
            if serde_json::from_str::<Value>(arguments).is_ok() {
                assert_eq!(native["function"]["arguments"], arguments);
            } else {
                assert_eq!(
                    replay_arguments,
                    json!({"_afssr_rejected_arguments":arguments})
                );
            }
            let feedback = messages
                .iter()
                .find(|message| message["role"] == "tool")
                .unwrap();
            assert!(feedback["content"]
                .to_string()
                .contains("No tool was executed"));
        }
    }
}

#[test]
fn grok_projected_helper_roundtrip_uses_native_responses_without_losing_reasoning() {
    use aether_ai_formats::provider_compat::opencode_go::{
        adapt_native_responses_request_for_model, restore_native_responses_payload,
    };
    let original = code_mode_body();
    let report = context(&original, "openai:responses", "grok-4.6")
        .report_context
        .unwrap();
    let mut request = original.clone();
    adapt_native_responses_request_for_model(&mut request, "grok-4.6").unwrap();
    assert!(request["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == "exec_command"));
    let arguments = json!({"cmd":"pwd"});
    let reasoning = json!({"type":"reasoning", "id":"rs_helper", "summary":[], "encrypted_content":"synthetic-unchanged"});
    let mut response = json!({"id":"resp_helper", "output":[reasoning.clone(),
        {"type":"function_call","id":"fc_helper","call_id":"call_helper","name":"exec_command","arguments":arguments.to_string()}]});
    restore_native_responses_payload(&mut response, &report).unwrap();
    assert_eq!(response["output"][0], reasoning);
    assert_eq!(response["output"][1]["type"], "custom_tool_call");
    assert_eq!(response["output"][1]["name"], "exec");
    request = original;
    request["input"]
        .as_array_mut()
        .unwrap()
        .extend(response["output"].as_array().unwrap().iter().cloned());
    request["input"].as_array_mut().unwrap().push(
        json!({"type":"custom_tool_call_output","call_id":"call_helper","output":"synthetic"}),
    );
    adapt_native_responses_request_for_model(&mut request, "grok-4.6").unwrap();
    let replay = request["input"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["call_id"] == "call_helper" && i["type"] == "function_call")
        .unwrap();
    assert_eq!(replay["name"], "exec_command");
    assert_eq!(
        serde_json::from_str::<Value>(replay["arguments"].as_str().unwrap()).unwrap(),
        arguments
    );
}

#[test]
fn projected_helper_marker_cannot_redirect_an_unrelated_custom_tool() {
    let mut original = code_mode_body();
    let ctx = context(&original, "openai:chat", "space-bunny");
    let upstream = json!({"id":"chatcmpl_marker","choices":[{"index":0,"message":{"role":"assistant","tool_calls":[
        {"id":"call_marker","type":"function","function":{"name":"exec_command","arguments":"{\"cmd\":\"pwd\"}"}}]},"finish_reason":"tool_calls"}]});
    let response = convert_response("openai:chat", "openai:responses", &upstream, &ctx).unwrap();
    let mut call = response["output"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["type"] == "custom_tool_call")
        .unwrap()
        .clone();
    call["input"] = json!(format!(
        "{}\ntext(\"tampered\");",
        call["input"].as_str().unwrap()
    ));
    original["input"].as_array_mut().unwrap().push(call.clone());
    let replay = convert_request(
        "openai:responses",
        "openai:chat",
        &original,
        &context(&original, "openai:chat", "space-bunny"),
    )
    .unwrap();
    let replay_call = replay["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["tool_calls"][0]["id"] == "call_marker")
        .unwrap();
    assert_ne!(
        replay_call["tool_calls"][0]["function"]["name"],
        "exec_command"
    );
    let arguments: Value = serde_json::from_str(
        replay_call["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(arguments["input"], call["input"]);
}

#[test]
fn mimo_explicit_code_mode_payload_is_opaque_not_extracted_from_chat_text() {
    let original = code_mode_body();
    for model in ["mimo-v2.5", "mimo-v2.6-flash", "mimo-v2.6-pro"] {
        let ctx = context(&original, "openai:chat", model);
        let native = convert_request("openai:responses", "openai:chat", &original, &ctx).unwrap();
        let alias = native["tools"][0]["function"]["name"].clone();
        for raw in ["text(await tools.exec_command({cmd: \"pwd\"}));", "{}"] {
            let upstream = json!({"id":"chatcmpl_mimo_raw","choices":[{"index":0,"message":{"role":"assistant","content":"This is not a tool payload.","tool_calls":[
                {"id":"call_raw","type":"function","function":{"name":alias,"arguments":raw}}]},"finish_reason":"tool_calls"}]});
            let response =
                convert_response("openai:chat", "openai:responses", &upstream, &ctx).unwrap();
            let call = response["output"]
                .as_array()
                .unwrap()
                .iter()
                .find(|i| i["type"] == "custom_tool_call")
                .unwrap();
            assert_eq!(call["input"], raw);
            assert_eq!(call["namespace"], "functions");
            assert_eq!(call["name"], "exec");
            let strict = context(&original, "openai:chat", "space-bunny");
            assert!(
                convert_response("openai:chat", "openai:responses", &upstream, &strict).is_err()
            );
        }
        let generic = body();
        let generic_context = context(&generic, "openai:chat", model);
        let native = convert_request(
            "openai:responses",
            "openai:chat",
            &generic,
            &generic_context,
        )
        .unwrap();
        let upstream = json!({"id":"chatcmpl_mimo_generic","choices":[{"index":0,"message":{"role":"assistant","tool_calls":[
            {"id":"call_generic","type":"function","function":{"name":native["tools"][0]["function"]["name"],"arguments":"alpha"}}]},"finish_reason":"tool_calls"}]});
        assert!(convert_response(
            "openai:chat",
            "openai:responses",
            &upstream,
            &generic_context
        )
        .is_err());
    }
}

#[test]
fn mimo_raw_code_mode_stream_keeps_the_exact_typed_payload() {
    let original = code_mode_body();
    let ctx = context(&original, "openai:chat", "mimo-v2.6-flash");
    let native = convert_request("openai:responses", "openai:chat", &original, &ctx).unwrap();
    let raw = "text(await tools.exec_command({cmd: \"pwd\"}));";
    let frames = [
        json!({"id":"chatcmpl_mimo_raw_stream","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_raw_stream","type":"function","function":{"name":native["tools"][0]["function"]["name"],"arguments":""}}]},"finish_reason":null}]}),
        json!({"id":"chatcmpl_mimo_raw_stream","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":raw}}]},"finish_reason":null}]}),
        json!({"id":"chatcmpl_mimo_raw_stream","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}),
    ];
    let mut matrix = StreamingStandardFormatMatrix::default();
    let mut bytes = Vec::new();
    for frame in frames {
        bytes.extend(
            matrix
                .transform_line(
                    ctx.report_context.as_ref().unwrap(),
                    format!("data: {frame}\n").into_bytes(),
                )
                .unwrap(),
        );
    }
    bytes.extend(matrix.finish(ctx.report_context.as_ref().unwrap()).unwrap());
    let events = String::from_utf8(bytes)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    let done = events
        .iter()
        .find(|e| e["type"] == "response.completed")
        .unwrap();
    assert_eq!(done["response"]["output"][0]["type"], "custom_tool_call");
    assert_eq!(done["response"]["output"][0]["input"], raw);
    assert!(events
        .iter()
        .any(|e| e["type"] == "response.custom_tool_call_input.done" && e["input"] == raw));
}

#[test]
fn grok_native_web_parameters_are_not_claude_web_parameters() {
    use aether_ai_formats::provider_compat::opencode_go::adapt_native_responses_request_for_model;
    let mut original = body();
    let web = json!({"type":"web_search","external_web_access":true,"filters":{"allowed_domains":["example.com"]},"search_content_types":["text","image"]});
    original["tools"] = json!([web.clone()]);
    original["tool_choice"] = json!({"type":"web_search"});
    let mut native = original.clone();
    adapt_native_responses_request_for_model(&mut native, "grok-4.6").unwrap();
    let search = native["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["type"] == "web_search")
        .unwrap();
    let mut expected = web;
    expected
        .as_object_mut()
        .unwrap()
        .remove("external_web_access");
    assert_eq!(search, &expected);
    assert_eq!(native["tool_choice"], original["tool_choice"]);
    assert!(native["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["type"] == "function"));
    original["tools"][0]["external_web_access"] = json!(false);
    assert!(adapt_native_responses_request_for_model(&mut original, "grok-4.6").is_err());
}

#[test]
fn grok_native_web_lifecycle_and_sources_survive_client_custom_tool_rewriting() {
    use aether_ai_formats::formats::shared::stream_rewrite::maybe_build_ai_surface_stream_rewriter;
    use aether_ai_formats::provider_compat::opencode_go::adapt_native_responses_request_for_model;
    let mut original = body();
    original["tools"] = json!([{"type":"web_search","external_web_access":true}]);
    let report = context(&original, "openai:responses", "grok-4.6")
        .report_context
        .unwrap();
    let mut native = original.clone();
    adapt_native_responses_request_for_model(&mut native, "grok-4.6").unwrap();
    let name = native["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["type"] == "function")
        .unwrap()["name"]
        .clone();
    let web = json!({"type":"web_search_call","id":"ws_fixture","status":"completed","action":{"type":"search","query":"synthetic","sources":[{"type":"url","url":"https://example.com/"}]}});
    let call = json!({"type":"function_call","id":"fc_web_fixture","call_id":"call_web_fixture","name":name,"arguments":"{\"input\":\"alpha\"}","status":"completed"});
    let frames = [
        json!({"type":"response.created","response":{"id":"resp_web_fixture","model":"grok-4.6","output":[],"status":"in_progress"}}),
        json!({"type":"response.output_item.added","output_index":0,"item":{"type":"web_search_call","id":"ws_fixture","status":"in_progress"}}),
        json!({"type":"response.web_search_call.searching","output_index":0,"item_id":"ws_fixture","sequence_number":200}),
        json!({"type":"response.output_item.done","output_index":0,"item":web.clone()}),
        json!({"type":"response.output_item.added","output_index":1,"item":{"type":"function_call","id":"fc_web_fixture","call_id":"call_web_fixture","name":name,"arguments":""}}),
        json!({"type":"response.output_item.done","output_index":1,"item":call.clone()}),
        json!({"type":"response.completed","response":{"id":"resp_web_fixture","model":"grok-4.6","status":"completed","output":[web.clone(),call]}}),
    ];
    let mut rewriter = maybe_build_ai_surface_stream_rewriter(Some(&report)).unwrap();
    let mut bytes = Vec::new();
    for frame in frames {
        bytes.extend(
            rewriter
                .push_chunk(format!("data: {frame}\n\n").as_bytes())
                .unwrap(),
        );
    }
    bytes.extend(rewriter.finish().unwrap());
    let events = String::from_utf8(bytes)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    assert!(events
        .iter()
        .any(|e| e["type"] == "response.web_search_call.searching"));
    let done = events
        .iter()
        .find(|e| e["type"] == "response.completed")
        .unwrap();
    assert_eq!(done["response"]["output"][0], web);
    assert_eq!(done["response"]["output"][1]["type"], "custom_tool_call");
    assert_eq!(done["response"]["output"][1]["input"], "alpha");
    let sequences = events
        .iter()
        .filter_map(|e| e["sequence_number"].as_u64())
        .collect::<Vec<_>>();
    assert!(sequences.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn go_chat_custom_tool_sync_roundtrip_preserves_payload_and_namespace() {
    let mut body = body();
    let ctx = context(&body, "openai:chat", "space-bunny");
    let mapped = convert_request("openai:responses", "openai:chat", &body, &ctx).unwrap();
    let name = mapped["tools"][0]["function"]["name"].clone();
    let payload = "text(\"nonce\");\n";
    let upstream = json!({"id":"chatcmpl_fixture", "model":"space-bunny", "choices":[{
        "index":0, "message":{"role":"assistant", "content":null, "tool_calls":[{
            "id":"call_fixture", "type":"function", "function":{"name":name,
            "arguments":json!({"input":payload}).to_string()}}]}, "finish_reason":"tool_calls"}]});
    let response = convert_response("openai:chat", "openai:responses", &upstream, &ctx).unwrap();
    let call = response["output"][0].clone();
    assert_eq!(call["type"], "custom_tool_call");
    assert_eq!(call["namespace"], "functions");
    assert_eq!(call["name"], "exec");
    assert_eq!(call["input"], payload);
    body["input"].as_array_mut().unwrap().extend([
        call,
        json!({"type":"custom_tool_call_output", "call_id":"call_fixture", "output":"nonce"}),
    ]);
    let next = convert_request(
        "openai:responses",
        "openai:chat",
        &body,
        &context(&body, "openai:chat", "space-bunny"),
    )
    .unwrap();
    let messages = next["messages"].as_array().unwrap();
    assert!(messages.iter().any(|m| m["role"] == "tool"
        && m["content"] == "nonce"
        && m["tool_call_id"] == "call_fixture"));
    let expected_arguments = json!({"input":payload}).to_string();
    assert!(messages
        .iter()
        .any(|m| m["tool_calls"][0]["function"]["arguments"].as_str()
            == Some(expected_arguments.as_str())));
}

#[test]
fn go_chat_custom_tool_stream_restores_all_events() {
    let body = body();
    let ctx = context(&body, "openai:chat", "space-bunny");
    let mapped = convert_request("openai:responses", "openai:chat", &body, &ctx).unwrap();
    let name = mapped["tools"][0]["function"]["name"].clone();
    let mut matrix = StreamingStandardFormatMatrix::default();
    let frames = [
        json!({"id":"chatcmpl_stream_fixture", "model":"space-bunny", "choices":[{"index":0, "delta":{"role":"assistant", "tool_calls":[{"index":0,"id":"call_stream_fixture","type":"function", "function":{"name":name,"arguments":""}}]}, "finish_reason":null}]}),
        json!({"id":"chatcmpl_stream_fixture", "model":"space-bunny", "choices":[{"index":0, "delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"input\":\"alpha\"}"}}]}, "finish_reason":null}]}),
        json!({"id":"chatcmpl_stream_fixture", "model":"space-bunny", "choices":[{"index":0, "delta":{}, "finish_reason":"tool_calls"}]}),
    ];
    let mut output = Vec::new();
    for frame in frames {
        output.extend(
            matrix
                .transform_line(
                    ctx.report_context.as_ref().unwrap(),
                    format!("data: {frame}\n").into_bytes(),
                )
                .unwrap(),
        );
    }
    output.extend(matrix.finish(ctx.report_context.as_ref().unwrap()).unwrap());
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("custom_tool_call"), "{text}");
    assert!(text.contains("\"namespace\":\"functions\""), "{text}");
    assert!(
        text.contains("response.custom_tool_call_input.done"),
        "{text}"
    );
    assert!(!text.contains("aether_input"));
}

#[test]
fn go_bridge_does_not_silently_discard_web_or_unknown_replay_policy() {
    let mut body = body();
    body["reasoning"]["context"] = json!("current_turn");
    let error = convert_request(
        "openai:responses",
        "openai:chat",
        &body,
        &context(&body, "openai:chat", "space-bunny"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("context"));
    body["reasoning"]["context"] = json!("all_turns");
    body["tools"] = json!([{"type":"web_search"}]);
    assert!(convert_request(
        "openai:responses",
        "openai:chat",
        &body,
        &context(&body, "openai:chat", "space-bunny")
    )
    .unwrap_err()
    .to_string()
    .contains("web search"));
}
