use aether_ai_formats::formats::shared::stream_core::format_matrix::StreamingStandardFormatMatrix;
use aether_ai_formats::{convert_request, convert_response, FormatContext};
use serde_json::{json, Value};

fn function(name: &str) -> Value {
    json!({"type":"function", "name":name, "description":"Read a synthetic value",
        "parameters":{"type":"object", "properties":{"label":{"type":"string"}}, "required":["label"], "additionalProperties":false}})
}

fn custom(name: &str) -> Value {
    json!({"type":"custom", "name":name, "description":"Submit an exact synthetic payload",
        "format":{"type":"grammar", "syntax":"lark", "definition":"start: /[a-z]+/"}})
}

fn namespace(name: &str, tools: Vec<Value>) -> Value {
    json!({"type":"namespace", "name":name, "description":"Synthetic tools", "tools":tools})
}

fn request(tools: Vec<Value>, lite: bool) -> Value {
    let mut body = json!({"model":"gpt-5.6-terra", "input":[{"role":"user","content":"Call probe for alpha"}],
        "max_output_tokens":2048, "reasoning":{"effort":"high"}, "parallel_tool_calls":false});
    if lite {
        body["input"].as_array_mut().unwrap().insert(
            0,
            json!({"type":"additional_tools", "role":"developer", "tools":tools}),
        );
    } else {
        body["tools"] = json!(tools);
    }
    body
}

fn context(body: &Value) -> FormatContext {
    FormatContext::default().with_mapped_model("claude-haiku-5-5").with_report_context(json!({
        "provider_api_format":"claude:messages", "client_api_format":"openai:responses", "needs_conversion":true,
        "original_request_body":body,
    }))
}

fn upstream_response(name: &str, input: Value) -> Value {
    json!({"id":"msg_haiku_fixture", "type":"message", "role":"assistant", "model":"claude-haiku-5-5",
        "content":[{"type":"tool_use", "id":"toolu_probe", "name":name, "input":input}],
        "stop_reason":"tool_use", "usage":{"input_tokens":12,"output_tokens":5}})
}

fn search_tool() -> Value {
    json!({"type":"tool_search", "execution":"client", "description":"Find synthetic tools",
        "parameters":{"type":"object","properties":{"query":{"type":"string"}},
        "required":["query"],"additionalProperties":false}})
}

fn web_result() -> Value {
    json!({"type":"web_search_tool_result","tool_use_id":"srvtoolu_web_fixture","caller":{"type":"direct"},"content":[{
        "type":"web_search_result","url":"https://example.com","title":"Example Domain","page_age":null,"encrypted_content":"synthetic-encrypted-result"}]})
}

fn web_body() -> Value {
    request(
        vec![
            json!({"type":"web_search","external_web_access":true,"search_content_types":["text"]}),
        ],
        false,
    )
}

#[test]
fn native_web_sync_response_preserves_server_actor_and_encrypted_result_history() {
    let body = web_body();
    let response = json!({"id":"msg_web_sync_fixture","type":"message","role":"assistant","model":"claude-haiku-5-5",
        "content":[{"type":"server_tool_use","id":"srvtoolu_web_fixture","name":"web_search","caller":{"type":"direct"},"input":{"query":"example.com"}},
        web_result(), {"type":"text","text":"Example Domain","citations":[{"type":"web_search_result_location",
        "cited_text":"Different source wording","url":"https://example.com","title":"Example Domain","encrypted_index":"synthetic-index"}]}],
        "stop_reason":"end_turn","usage":{"input_tokens":12,"output_tokens":8}});
    let converted = convert_response(
        "claude:messages",
        "openai:responses",
        &response,
        &context(&body),
    )
    .unwrap();
    assert!(!converted["output"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["type"] == "function_call_output"));
    let search = converted["output"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["type"] == "web_search_call")
        .unwrap();
    assert_eq!(search["action"]["sources"][0]["url"], "https://example.com");
    let mut next = body.clone();
    next["input"]
        .as_array_mut()
        .unwrap()
        .extend(converted["output"].as_array().unwrap().iter().cloned());
    next["input"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"user","content":"Continue"}));
    let native = convert_request(
        "openai:responses",
        "claude:messages",
        &next,
        &context(&next),
    )
    .unwrap();
    let blocks = native["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["content"].as_array())
        .flatten()
        .collect::<Vec<_>>();
    assert!(blocks
        .iter()
        .any(|b| b["type"] == "server_tool_use" && b["id"] == "srvtoolu_web_fixture"));
    assert!(blocks.iter().any(|b| b["type"] == "web_search_tool_result"
        && b["content"][0]["encrypted_content"] == "synthetic-encrypted-result"));
    assert!(native["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["content"]
            .as_array()
            .is_some_and(|c| c.iter().any(|b| b["type"] == "web_search_tool_result")))
        .all(|m| m["role"] == "assistant"));
}

#[test]
fn native_web_stream_completes_with_sources_citations_and_roundtrippable_history() {
    let body = web_body();
    let report = context(&body).report_context.unwrap();
    let mut matrix = StreamingStandardFormatMatrix::default();
    let events = vec![
        json!({"type":"message_start","message":{"id":"msg_web_stream_fixture","model":"claude-haiku-5-5","usage":{"input_tokens":12}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"server_tool_use","id":"srvtoolu_web_fixture","name":"web_search","input":{}}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"query\":\"example.com\"}"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"content_block_start","index":1,"content_block":web_result()}),
        json!({"type":"content_block_stop","index":1}),
        json!({"type":"content_block_start","index":2,"content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"Example Domain"}}),
        json!({"type":"content_block_delta","index":2,"delta":{"type":"citations_delta","citation":{"type":"web_search_result_location","url":"https://example.com","title":"Example Domain","cited_text":"Different source wording","encrypted_index":"synthetic-index"}}}),
        json!({"type":"content_block_stop","index":2}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":8}}),
        json!({"type":"message_stop"}),
    ];
    let mut bytes = Vec::new();
    for event in events {
        bytes.extend(
            matrix
                .transform_line(&report, format!("data: {event}\n").into_bytes())
                .unwrap(),
        );
    }
    bytes.extend(matrix.finish(&report).unwrap());
    let emitted = String::from_utf8(bytes)
        .unwrap()
        .lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .filter(|v| *v != "[DONE]")
        .map(|v| serde_json::from_str::<Value>(v).unwrap())
        .collect::<Vec<_>>();
    assert!(!emitted
        .iter()
        .any(|e| matches!(e["type"].as_str(), Some("error" | "response.failed"))));
    let final_response = &emitted
        .iter()
        .find(|e| e["type"] == "response.completed")
        .unwrap()["response"];
    let search = final_response["output"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["type"] == "web_search_call")
        .unwrap();
    assert_eq!(search["action"]["sources"][0]["url"], "https://example.com");
    let text = final_response["output"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["type"] == "message")
        .unwrap();
    assert_eq!(text["content"][0]["annotations"][0]["end_index"], 14);
    let mut next = body.clone();
    next["input"]
        .as_array_mut()
        .unwrap()
        .extend(final_response["output"].as_array().unwrap().iter().cloned());
    let native = convert_request(
        "openai:responses",
        "claude:messages",
        &next,
        &context(&next),
    )
    .unwrap();
    assert!(native["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["content"]
            .as_array()
            .is_some_and(|c| c.iter().any(|b| b["type"] == "web_search_tool_result"))));
}

#[test]
fn native_web_context_rejects_unpaired_results_and_non_claude_targets() {
    let body = web_body();
    let response = json!({"id":"msg_bad_web","type":"message","role":"assistant","model":"claude-haiku-5-5",
        "content":[web_result()],"stop_reason":"end_turn","usage":{"input_tokens":2,"output_tokens":2}});
    assert!(convert_response(
        "claude:messages",
        "openai:responses",
        &response,
        &context(&body)
    )
    .is_err());
    assert!(
        convert_response("claude:messages", "openai:chat", &response, &context(&body)).is_err()
    );
}

#[test]
fn native_web_result_only_accepts_known_direct_caller_metadata() {
    let body = web_body();
    for (key, value) in [
        (
            "caller",
            json!({"type":"code_execution_20250825","tool_id":"other"}),
        ),
        ("unknown", json!(true)),
    ] {
        let mut result = web_result();
        result[key] = value;
        let response = json!({"id":"msg_web_metadata","type":"message","role":"assistant","model":"claude-haiku-5-5",
            "content":[{"type":"server_tool_use","id":"srvtoolu_web_fixture","name":"web_search","input":{"query":"example.com"}},result],
            "stop_reason":"end_turn","usage":{"input_tokens":2,"output_tokens":2}});
        assert!(convert_response(
            "claude:messages",
            "openai:responses",
            &response,
            &context(&body)
        )
        .is_err());
    }
}

#[test]
fn codex_web_search_maps_to_a_real_server_tool_not_a_client_function() {
    use aether_ai_formats::formats::shared::standard_normalize::build_cross_format_openai_responses_request_body;
    for (lite, content_types) in [false, true].into_iter().flat_map(|lite| {
        [
            json!(["text"]),
            json!(["text", "image"]),
            json!(["image", "text"]),
            json!([]),
            json!(null),
        ]
        .into_iter()
        .map(move |types| (lite, types))
    }) {
        let body = request(
            vec![
                namespace("functions", vec![custom("probe")]),
                json!({"type":"web_search","external_web_access":true,"search_content_types":content_types}),
            ],
            lite,
        );
        let converted = build_cross_format_openai_responses_request_body(
            &body,
            "claude-haiku-5-5",
            "openai:responses",
            "claude:messages",
            true,
        )
        .unwrap();
        let web = converted["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "web_search")
            .unwrap();
        assert_eq!(
            web,
            &json!({"type":"web_search_20250305","name":"web_search"})
        );
        assert!(web.get("input_schema").is_none());
        assert!(converted.get("builtin_tools").is_none());
        assert!(converted.get("web_search_options").is_none());
        assert_eq!(converted["tools"].as_array().unwrap().len(), 2);
    }
}

#[test]
fn web_search_constraints_are_preserved_or_explicitly_rejected() {
    let location = json!({"type":"approximate","country":"US"});
    let web = json!({"type":"web_search", "filters":{"allowed_domains":["example.com"]},"user_location":location});
    let body = request(vec![web], false);
    let converted = convert_request(
        "openai:responses",
        "claude:messages",
        &body,
        &context(&body),
    )
    .unwrap();
    assert_eq!(
        converted["tools"][0]["allowed_domains"],
        json!(["example.com"])
    );
    assert_eq!(converted["tools"][0]["user_location"], location);
    for web in [
        json!({"type":"web_search","external_web_access":false}),
        json!({"type":"web_search","search_content_types":["image"]}),
        json!({"type":"web_search","search_content_types":["text","unknown"]}),
        json!({"type":"web_search","search_content_types":"text"}),
        json!({"type":"web_search","unknown":true}),
    ] {
        let body = request(vec![web], false);
        assert!(convert_request(
            "openai:responses",
            "claude:messages",
            &body,
            &context(&body)
        )
        .is_err());
    }
}

#[test]
fn additional_tools_accepts_protocol_id_but_rejects_unknown_metadata() {
    for id in [json!(null), json!("additional_fixture")] {
        let mut body = request(vec![function("probe")], true);
        body["input"][0]["id"] = id;
        assert!(convert_request(
            "openai:responses",
            "claude:messages",
            &body,
            &context(&body)
        )
        .is_ok());
        body["input"][0]["unknown"] = json!(true);
        assert!(convert_request(
            "openai:responses",
            "claude:messages",
            &body,
            &context(&body)
        )
        .is_err());
    }
}

#[test]
fn client_tool_search_roundtrips_call_and_loads_returned_namespace() {
    for lite in [false, true] {
        let body = request(vec![search_tool()], lite);
        let converted = convert_request(
            "openai:responses",
            "claude:messages",
            &body,
            &context(&body),
        )
        .unwrap();
        assert_eq!(converted["tools"][0]["name"], "aether_tool_search");
        let response = convert_response(
            "claude:messages",
            "openai:responses",
            &upstream_response("aether_tool_search", json!({"query":"probe"})),
            &context(&body),
        )
        .unwrap();
        let call = response["output"][0].clone();
        assert_eq!(call["type"], "tool_search_call");
        assert_eq!(call["execution"], "client");
        assert_eq!(call["arguments"], json!({"query":"probe"}));
        assert!(call.get("name").is_none());
        let mut continued = body.clone();
        continued["input"].as_array_mut().unwrap().extend([
            call,
            json!({"type":"tool_search_output",
            "execution":"client", "call_id":"toolu_probe", "status":"completed",
            "tools":[namespace("discovered", vec![function("probe")])]}),
        ]);
        let converted = convert_request(
            "openai:responses",
            "claude:messages",
            &continued,
            &context(&continued),
        )
        .unwrap();
        assert_eq!(converted["tools"].as_array().unwrap().len(), 2);
        let blocks = converted["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|m| m["content"].as_array())
            .flatten()
            .collect::<Vec<_>>();
        assert!(blocks
            .iter()
            .any(|b| b["type"] == "tool_use" && b["name"] == "aether_tool_search"));
        assert!(blocks.iter().any(|b| b["type"] == "tool_result"
            && b["content"]
                .as_str()
                .unwrap_or_default()
                .contains("loaded_tools")));
        let next = convert_response(
            "claude:messages",
            "openai:responses",
            &upstream_response("probe", json!({"label":"alpha"})),
            &context(&continued),
        )
        .unwrap();
        assert_eq!(next["output"][0]["namespace"], "discovered");
    }
}

#[test]
fn client_tool_search_sse_emits_object_arguments_and_matching_completed_item() {
    let body = request(vec![search_tool()], true);
    let (events, _) = streamed(
        &body,
        "aether_tool_search",
        &json!({"query":"probe"}),
        "msg_search_fixture",
    );
    let added = events
        .iter()
        .find(|e| e["type"] == "response.output_item.added")
        .unwrap();
    let done = events
        .iter()
        .find(|e| e["type"] == "response.output_item.done")
        .unwrap();
    let completed = events
        .iter()
        .find(|e| e["type"] == "response.completed")
        .unwrap();
    assert_eq!(added["item"]["type"], "tool_search_call");
    assert_eq!(done["item"]["arguments"], json!({"query":"probe"}));
    assert_eq!(done["item"]["execution"], "client");
    assert_eq!(done["item"]["id"], added["item"]["id"]);
    assert_eq!(completed["response"]["output"][0], done["item"]);
    assert!(!events.iter().any(|e| e["type"]
        .as_str()
        .unwrap()
        .starts_with("response.function_call_arguments.")));
}

#[test]
fn hosted_tool_search_and_alias_collisions_fail_closed() {
    let mut hosted = search_tool();
    hosted["execution"] = json!("server");
    for tools in [
        vec![hosted],
        vec![search_tool(), function("aether_tool_search")],
    ] {
        let body = request(tools, false);
        assert!(convert_request(
            "openai:responses",
            "claude:messages",
            &body,
            &context(&body)
        )
        .is_err());
    }
}

#[test]
fn production_request_builder_preserves_responses_tools_instead_of_using_chat() {
    use aether_ai_formats::formats::shared::standard_normalize::build_cross_format_openai_responses_request_body;
    for lite in [false, true] {
        for stream in [false, true] {
            for tools in [
                vec![search_tool()],
                vec![custom("probe")],
                vec![namespace("functions", vec![custom("probe")])],
                vec![namespace("functions", vec![function("probe")])],
            ] {
                let mut body = request(tools, lite);
                body["stream"] = json!(stream);
                body["store"] = json!(false);
                body["include"] = json!(["reasoning.encrypted_content"]);
                body["text"] = json!({"format":{"type":"text"}, "verbosity":"medium"});
                body["prompt_cache_key"] = json!("synthetic-codex-session");
                let converted = build_cross_format_openai_responses_request_body(
                    &body,
                    "claude-haiku-5-5",
                    "openai:responses",
                    "claude:messages",
                    stream,
                )
                .expect("production request builder must use the audited Responses mapping");
                assert_eq!(converted["tools"].as_array().unwrap().len(), 1);
                assert!(matches!(
                    converted["tools"][0]["name"].as_str(),
                    Some("probe" | "aether_tool_search")
                ));
                assert_eq!(converted["thinking"], json!({"type":"adaptive"}));
                assert_eq!(converted["output_config"]["effort"], "high");
                assert_eq!(converted["stream"], json!(stream));
                assert!(converted.get("input").is_none());
            }
        }
    }
}

#[test]
fn client_tool_search_previous_response_loads_tools_before_history_conversion() {
    use aether_ai_formats::formats::shared::standard_normalize::build_cross_format_openai_responses_request_body_with_model_directives_and_history_scope;
    let scope = "client-search-previous-regression";
    let mut body = request(vec![search_tool()], true);
    body["store"] = json!(true);
    body["input"][0]["id"] = json!("additional_tools_fixture");
    let mut ctx = context(&body).with_history_scope(scope);
    ctx.report_context.as_mut().unwrap()["api_key_id"] = json!(scope);
    let first = convert_response(
        "claude:messages",
        "openai:responses",
        &upstream_response("aether_tool_search", json!({"query":"probe"})),
        &ctx,
    )
    .unwrap();
    body["previous_response_id"] = first["id"].clone();
    body["input"] = json!([body["input"][0].clone(), {"type":"tool_search_output", "execution":"client",
        "call_id":"toolu_probe", "tools":[namespace("discovered", vec![custom("probe")])]}]);
    let continued =
        build_cross_format_openai_responses_request_body_with_model_directives_and_history_scope(
            &body,
            "claude-haiku-5-5",
            "openai:responses",
            "claude:messages",
            true,
            false,
            Some(scope),
        )
        .unwrap();
    assert_eq!(continued["tools"].as_array().unwrap().len(), 2);
    let blocks = continued["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["content"].as_array())
        .flatten()
        .collect::<Vec<_>>();
    assert!(blocks
        .iter()
        .any(|b| b["type"] == "tool_use" && b["name"] == "aether_tool_search"));
    assert!(blocks.iter().any(|b| b["type"] == "tool_result"));
    let mut loaded_context = context(&body).with_history_scope(scope);
    loaded_context.report_context.as_mut().unwrap()["api_key_id"] = json!(scope);
    let second = convert_response(
        "claude:messages",
        "openai:responses",
        &upstream_response("probe", json!({"input":"alpha"})),
        &loaded_context,
    )
    .unwrap();
    assert_eq!(second["output"][0]["namespace"], "discovered");
    let mut third = request(vec![search_tool()], true);
    third["previous_response_id"] = second["id"].clone();
    let mut third_context = context(&third).with_history_scope(scope);
    third_context.report_context.as_mut().unwrap()["api_key_id"] = json!(scope);
    let restored = convert_response(
        "claude:messages",
        "openai:responses",
        &upstream_response("probe", json!({"input":"alpha"})),
        &third_context,
    )
    .unwrap();
    assert_eq!(restored["output"][0]["namespace"], "discovered");
    assert_eq!(restored["output"][0]["type"], "custom_tool_call");
}

#[test]
fn production_lite_previous_response_preserves_current_tools_and_pairing() {
    use aether_ai_formats::formats::shared::standard_normalize::build_cross_format_openai_responses_request_body_with_model_directives_and_history_scope;
    let scope = "haiku-lite-previous-synthetic-scope";
    let mut body = request(vec![namespace("functions", vec![custom("probe")])], true);
    body["store"] = json!(true);
    let report = json!({"provider_api_format":"claude:messages", "client_api_format":"openai:responses",
        "needs_conversion":true, "original_request_body":body, "api_key_id":scope});
    let context = FormatContext::default()
        .with_mapped_model("claude-haiku-5-5")
        .with_history_scope(scope)
        .with_report_context(report);
    let first = convert_response(
        "claude:messages",
        "openai:responses",
        &upstream_response("probe", json!({"input":"alpha"})),
        &context,
    )
    .unwrap();
    let call = first["output"][0].clone();
    let tools_prefix = body["input"][0].clone();
    body["previous_response_id"] = first["id"].clone();
    body["input"] = json!([tools_prefix, {"type":"custom_tool_call_output",
        "call_id":call["call_id"], "output":"stored-nonce"}]);
    let continued =
        build_cross_format_openai_responses_request_body_with_model_directives_and_history_scope(
            &body,
            "claude-haiku-5-5",
            "openai:responses",
            "claude:messages",
            false,
            false,
            Some(scope),
        )
        .expect("Lite definitions must be lifted before expanding previous-response history");
    assert_eq!(continued["tools"].as_array().unwrap().len(), 1);
    let blocks = continued["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|message| message["content"].as_array())
        .flatten()
        .collect::<Vec<_>>();
    assert!(blocks
        .iter()
        .any(|block| block["type"] == "tool_use" && block["input"] == json!({"input":"alpha"})));
    assert!(blocks
        .iter()
        .any(|block| block["type"] == "tool_result" && block["content"] == "stored-nonce"));
}

#[test]
fn haiku_parameters_and_namespace_functions_map_without_losing_schema() {
    let body = request(vec![namespace("functions", vec![function("probe")])], false);
    let converted = convert_request(
        "openai:responses",
        "claude:messages",
        &body,
        &context(&body),
    )
    .unwrap();
    assert_eq!(converted["model"], "claude-haiku-5-5");
    assert_eq!(converted["thinking"], json!({"type":"adaptive"}));
    assert_eq!(converted["output_config"]["effort"], "high");
    assert_eq!(converted["max_tokens"], 2048);
    assert_eq!(converted["tools"][0]["name"], "probe");
    assert_eq!(
        converted["tools"][0]["input_schema"],
        function("probe")["parameters"]
    );
    assert_eq!(converted["tool_choice"]["disable_parallel_tool_use"], true);
    let downstream = convert_response(
        "claude:messages",
        "openai:responses",
        &upstream_response("probe", json!({"label":"alpha"})),
        &context(&body),
    )
    .unwrap();
    assert_eq!(downstream["output"][0]["namespace"], "functions");
    assert_eq!(downstream["output"][0]["name"], "probe");
    assert_eq!(downstream["output"][0]["call_id"], "toolu_probe");
}

#[test]
fn lite_custom_tools_round_trip_exact_payload_and_result() {
    let mut body = request(vec![namespace("functions", vec![custom("probe")])], true);
    body["tool_choice"] = json!({"type":"custom", "namespace":"functions", "name":"probe"});
    let converted = convert_request(
        "openai:responses",
        "claude:messages",
        &body,
        &context(&body),
    )
    .unwrap();
    assert_eq!(
        converted["tools"][0]["input_schema"]["required"],
        json!(["input"])
    );
    assert!(converted["tools"][0]["description"]
        .as_str()
        .unwrap()
        .contains("start: /[a-z]+/"));
    assert_eq!(converted["tool_choice"]["name"], "probe");
    let payload = "alpha\n\"quoted\"\\utf8: 中文";
    let downstream = convert_response(
        "claude:messages",
        "openai:responses",
        &upstream_response("probe", json!({"input":payload})),
        &context(&body),
    )
    .unwrap();
    let call = downstream["output"][0].clone();
    assert_eq!(call["type"], "custom_tool_call");
    assert_eq!(call["input"], payload);
    assert_eq!(call["namespace"], "functions");
    body["input"].as_array_mut().unwrap().extend([
        call,
        json!({"type":"custom_tool_call_output","call_id":"toolu_probe","output":"nonce-123"}),
    ]);
    let continued = convert_request(
        "openai:responses",
        "claude:messages",
        &body,
        &context(&body),
    )
    .unwrap();
    assert_eq!(
        continued["messages"][1]["content"][0]["input"],
        json!({"input":payload})
    );
    assert_eq!(continued["messages"][1]["content"][0]["name"], "probe");
    assert_eq!(
        continued["messages"][2]["content"][0]["tool_use_id"],
        "toolu_probe"
    );
    assert_eq!(
        continued["messages"][2]["content"][0]["content"],
        "nonce-123"
    );
}

#[test]
fn aliases_are_collision_safe_and_stable_across_history() {
    let mut body = request(
        vec![
            function("probe"),
            namespace("first", vec![function("probe")]),
            namespace("second", vec![custom("probe")]),
        ],
        false,
    );
    let converted = convert_request(
        "openai:responses",
        "claude:messages",
        &body,
        &context(&body),
    )
    .unwrap();
    let names = converted["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["probe", "first__probe", "second__probe"]);
    let call = convert_response(
        "claude:messages",
        "openai:responses",
        &upstream_response("second__probe", json!({"input":"alpha"})),
        &context(&body),
    )
    .unwrap()["output"][0]
        .clone();
    assert_eq!(call["namespace"], "second");
    assert_eq!(call["name"], "probe");
    body["input"].as_array_mut().unwrap().push(call);
    let continued = convert_request(
        "openai:responses",
        "claude:messages",
        &body,
        &context(&body),
    )
    .unwrap();
    assert_eq!(
        continued["messages"][1]["content"][0]["name"],
        "second__probe"
    );
}

fn streamed(
    body: &Value,
    name: &str,
    input: &Value,
    id: &str,
) -> (Vec<Value>, StreamingStandardFormatMatrix) {
    let report = context(body).report_context.unwrap();
    let mut matrix = StreamingStandardFormatMatrix::default();
    let encoded = input.to_string();
    let events = vec![
        json!({"type":"message_start", "message":{"id":id,"model":"claude-haiku-5-5","usage":{"input_tokens":12}}}),
        json!({"type":"content_block_start", "index":0,"content_block":{"type":"tool_use","id":"toolu_probe","name":name,"input":{}}}),
        json!({"type":"content_block_delta", "index":0,"delta":{"type":"input_json_delta","partial_json":&encoded[..1]}}),
        json!({"type":"content_block_delta", "index":0,"delta":{"type":"input_json_delta","partial_json":&encoded[1..]}}),
        json!({"type":"content_block_stop", "index":0}),
        json!({"type":"message_delta", "delta":{"stop_reason":"tool_use"}, "usage":{"output_tokens":5}}),
        json!({"type":"message_stop"}),
    ];
    let mut output = Vec::new();
    for event in events {
        output.extend(
            matrix
                .transform_line(&report, format!("data: {event}\n").into_bytes())
                .unwrap(),
        );
    }
    output.extend(matrix.finish(&report).unwrap());
    let parsed = String::from_utf8(output)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|data| *data != "[DONE]")
        .map(|data| serde_json::from_str(data).unwrap())
        .collect();
    (parsed, matrix)
}

#[test]
fn custom_sse_events_restore_identity_input_and_completion() {
    let body = request(vec![namespace("functions", vec![custom("probe")])], true);
    let payload = "alpha\n\"escaped\"\\";
    let (events, mut matrix) = streamed(
        &body,
        "probe",
        &json!({"input":payload}),
        "msg_custom_stream_fixture",
    );
    let added = events
        .iter()
        .find(|e| e["type"] == "response.output_item.added")
        .unwrap();
    assert_eq!(added["item"]["type"], "custom_tool_call");
    assert_eq!(added["item"]["namespace"], "functions");
    let delta = events
        .iter()
        .find(|e| e["type"] == "response.custom_tool_call_input.delta")
        .unwrap();
    assert_eq!(delta["delta"], payload);
    let done = events
        .iter()
        .find(|e| e["type"] == "response.custom_tool_call_input.done")
        .unwrap();
    assert_eq!(done["input"], payload);
    assert_eq!(done["item_id"], added["item"]["id"]);
    assert_eq!(done["call_id"], "toolu_probe");
    assert!(!events.iter().any(|e| e["type"]
        .as_str()
        .unwrap()
        .starts_with("response.function_call_arguments.")));
    let item_done = events
        .iter()
        .find(|e| e["type"] == "response.output_item.done")
        .unwrap();
    let completed = events
        .iter()
        .find(|e| e["type"] == "response.completed")
        .unwrap();
    assert_eq!(completed["response"]["output"][0], item_done["item"]);
    assert_eq!(completed["response"]["output"][0]["input"], payload);
    assert!(matrix.take_response_history_record().is_some());
    let continued = json!({"model":"gpt-5.6-terra","tools":[namespace("functions",vec![custom("probe")])],
        "previous_response_id":completed["response"]["id"], "input":[{"type":"custom_tool_call_output","call_id":"toolu_probe","output":"nonce"}]});
    let converted = convert_request(
        "openai:responses",
        "claude:messages",
        &continued,
        &context(&continued),
    )
    .unwrap();
    assert_eq!(
        converted["messages"][1]["content"][0]["input"]["input"],
        payload
    );
    assert_eq!(converted["messages"][2]["content"][0]["content"], "nonce");
}

#[test]
fn function_sse_events_keep_schema_identity_and_arguments() {
    let body = request(vec![namespace("functions", vec![function("probe")])], true);
    let (events, _) = streamed(
        &body,
        "probe",
        &json!({"label":"alpha"}),
        "msg_function_stream_fixture",
    );
    let completed = events
        .iter()
        .find(|e| e["type"] == "response.completed")
        .unwrap();
    let item = &completed["response"]["output"][0];
    assert_eq!(item["type"], "function_call");
    assert_eq!(item["namespace"], "functions");
    assert_eq!(
        serde_json::from_str::<Value>(item["arguments"].as_str().unwrap()).unwrap(),
        json!({"label":"alpha"})
    );
}

#[test]
fn malformed_custom_wrapper_is_not_silently_returned_as_a_function() {
    let body = request(vec![custom("probe")], false);
    assert!(convert_response(
        "claude:messages",
        "openai:responses",
        &upstream_response("probe", json!({"wrong":"alpha"})),
        &context(&body)
    )
    .is_err());
}

#[test]
fn unrelated_models_keep_legacy_thinking_and_chat_namespace_contract() {
    let body = request(vec![namespace("functions", vec![function("probe")])], false);
    let converted = convert_request(
        "openai:responses",
        "claude:messages",
        &body,
        &FormatContext::default().with_mapped_model("claude-haiku-4-5"),
    )
    .unwrap();
    assert_eq!(converted["thinking"]["type"], "enabled");
    assert!(converted["thinking"]["budget_tokens"].is_number());
    let chat = convert_request(
        "openai:responses",
        "openai:chat",
        &body,
        &FormatContext::default(),
    )
    .unwrap();
    assert_eq!(chat["tools"][0]["function"]["name"], "probe");
}

#[test]
fn signed_thinking_survives_sync_tool_continuation() {
    let mut body = request(vec![namespace("functions", vec![function("probe")])], false);
    let mut upstream = upstream_response("probe", json!({"label":"alpha"}));
    let signed = json!({"type":"thinking","thinking":"Synthetic reasoning fixture", "signature":"fixture-signature-not-a-credential"});
    upstream["content"]
        .as_array_mut()
        .unwrap()
        .insert(0, signed.clone());
    let converted = convert_response(
        "claude:messages",
        "openai:responses",
        &upstream,
        &context(&body),
    )
    .unwrap();
    assert!(converted["output"][0]["encrypted_content"]
        .as_str()
        .unwrap()
        .starts_with("aether-claude-thinking-v1:"));
    body["input"]
        .as_array_mut()
        .unwrap()
        .extend(converted["output"].as_array().unwrap().iter().cloned());
    body["input"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"function_call_output","call_id":"toolu_probe","output":"nonce"}));
    let continued = convert_request(
        "openai:responses",
        "claude:messages",
        &body,
        &context(&body),
    )
    .unwrap();
    assert_eq!(continued["messages"][1]["content"][0], signed);
    assert_eq!(continued["messages"][1]["content"][1]["type"], "tool_use");
}

#[test]
fn signed_thinking_survives_stream_tool_continuation() {
    let body = request(vec![namespace("functions", vec![function("probe")])], false);
    let report = context(&body).report_context.unwrap();
    let mut matrix = StreamingStandardFormatMatrix::default();
    let events = vec![
        json!({"type":"message_start","message":{"id":"msg_thinking_stream","model":"claude-haiku-5-5"}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Synthetic reasoning fixture"}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"fixture-signature"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_probe","name":"probe","input":{}}}),
        json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"label\":\"alpha\"}"}}),
        json!({"type":"content_block_stop","index":1}),
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5}}),
        json!({"type":"message_stop"}),
    ];
    let mut output = Vec::new();
    for event in events {
        output.extend(
            matrix
                .transform_line(&report, format!("data: {event}\n").into_bytes())
                .unwrap(),
        );
    }
    let sse = String::from_utf8(output).unwrap();
    let completed = sse
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|data| serde_json::from_str::<Value>(data).ok())
        .find(|event| event["type"] == "response.completed")
        .unwrap();
    let mut continued = body;
    continued["input"].as_array_mut().unwrap().extend(
        completed["response"]["output"]
            .as_array()
            .unwrap()
            .iter()
            .cloned(),
    );
    continued["input"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"function_call_output","call_id":"toolu_probe","output":"nonce"}));
    let converted = convert_request(
        "openai:responses",
        "claude:messages",
        &continued,
        &context(&continued),
    )
    .unwrap();
    assert_eq!(
        converted["messages"][1]["content"][0],
        json!({"type":"thinking","thinking":"Synthetic reasoning fixture","signature":"fixture-signature"})
    );
}

#[test]
fn haiku_none_effort_disables_thinking_instead_of_enabling_it() {
    let mut body = request(vec![function("probe")], false);
    body["reasoning"]["effort"] = json!("none");
    let converted = convert_request(
        "openai:responses",
        "claude:messages",
        &body,
        &context(&body),
    )
    .unwrap();
    assert_eq!(converted["thinking"]["type"], "disabled");
}
