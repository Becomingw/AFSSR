//! Synthetic native-wire probe. Credentials are supplied by a separate sender.
use std::io::{self, Read};

use aether_ai_formats::formats::shared::stream_rewrite::maybe_build_ai_surface_stream_rewriter;
use aether_ai_formats::{convert_request, convert_response, FormatContext};
use serde_json::{json, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).ok_or("request or stream mode is required")?;
    let model = args.get(2).ok_or("native Go model is required")?;
    let format = aether_ai_formats::provider_compat::opencode_go::model_api_format(model)
        .ok_or("unknown native Go protocol")?;
    let mut original: Value =
        serde_json::from_str(include_str!("../tests/fixtures/codex_lite_go.json"))?;
    if mode == "request-stdin" {
        let mut input = String::new();
        io::stdin()
            .take(2 * 1024 * 1024 + 1)
            .read_to_string(&mut input)?;
        if input.len() > 2 * 1024 * 1024 {
            return Err("synthetic request exceeds limit".into());
        }
        original = serde_json::from_str(&input)?;
    }
    original["store"] = json!(false);
    let report = json!({"provider_type":"opencode_go", "provider_api_format":format,
        "client_api_format":"openai:responses", "mapped_model":model,
        "needs_conversion":format != "openai:responses", "api_key_id":"synthetic-probe",
        "original_request_body":original});
    let context = FormatContext::default()
        .with_mapped_model(model)
        .with_upstream_stream(true)
        .with_history_scope("synthetic-probe")
        .with_report_context(report.clone());
    if mode == "helper-projections" {
        if format != "openai:chat" {
            return Err("helper probe requires a Go Chat model".into());
        }
        let mut cases = Vec::new();
        for (name, arguments) in [
            ("exec_command", json!({"cmd":"SYNTHETIC_LITERAL"})),
            (
                "apply_patch",
                json!({"patch":"*** Begin Patch\n*** End Patch"}),
            ),
            ("list_client_tools", json!({"query":"synthetic"})),
            (
                "call_client_tool",
                json!({"name":"synthetic_echo","arguments":{"literal":"\"; process.exit() //","__proto__":{"marker":"literal"}}}),
            ),
            (
                "call_client_tool",
                json!({"name":"not_advertised","arguments":{}}),
            ),
            ("apply_patch", json!({})),
        ] {
            let native = json!({"id":"chatcmpl_mock","model":model,"choices":[{"index":0,"finish_reason":"tool_calls","message":{"role":"assistant","content":null,
                "tool_calls":[{"id":"call_mock","type":"function","function":{"name":name,"arguments":arguments.to_string()}}]}}]});
            let response = convert_response(format, "openai:responses", &native, &context)?;
            let call = response["output"]
                .as_array()
                .ok_or("missing output")?
                .iter()
                .find(|item| item["type"] == "custom_tool_call")
                .ok_or("missing restored custom call")?;
            cases.push(json!({"helper":name,"arguments":arguments,"input":call["input"]}));
        }
        println!("{}", json!(cases));
        return Ok(());
    }
    if mode == "request" || mode == "request-stdin" {
        let mut native = if format == "openai:responses" {
            let mut native = original;
            native["model"] = json!(model);
            aether_ai_formats::provider_compat::opencode_go::adapt_native_responses_request_for_model(
                &mut native, model,
            ).map_err(|error| format!("{}: {}", error.field, error.reason))?;
            native
        } else {
            convert_request("openai:responses", format, &original, &context)?
        };
        if format == "openai:responses" {
            native["max_output_tokens"] = json!(4096);
        } else if format == "gemini:generate_content" {
            native["generationConfig"]["maxOutputTokens"] = json!(4096);
        } else {
            native["max_tokens"] = json!(4096);
        }
        println!("{native}");
        return Ok(());
    }
    if mode != "stream" {
        return Err("unexpected probe mode".into());
    }
    let mut rewriter =
        maybe_build_ai_surface_stream_rewriter(Some(&report)).ok_or("no stream rewrite plan")?;
    let mut buffer = [0; 4096];
    let mut bytes = 0;
    loop {
        let read = io::stdin().read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes += read;
        if bytes > 4 * 1024 * 1024 {
            return Err("probe input exceeds limit".into());
        }
        if let Err(error) = rewriter.push_chunk(&buffer[..read]) {
            println!(
                "{}",
                json!({"rewrite_passed":false,"bytes":bytes,
                "error":classify_error(&error.0)})
            );
            return Ok(());
        }
    }
    let result = rewriter.finish();
    println!(
        "{}",
        json!({"rewrite_passed":result.is_ok(),"bytes":bytes,
        "error":result.err().map(|error| classify_error(&error.0))})
    );
    Ok(())
}

fn classify_error(error: &str) -> &'static str {
    for known in [
        "Claude custom tool arguments are not valid JSON",
        "Claude custom tool arguments must contain only input",
        "Claude custom tool input must be a string",
        "client tool-search arguments must be valid JSON",
        "client tool-search arguments must be an object",
    ] {
        if error.contains(known) {
            return known;
        }
    }
    "other_stream_conversion_error"
}
