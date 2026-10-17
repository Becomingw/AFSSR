use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use serde_json::{json, Value};

const PREFIX: &str = "// AFSSR_CODE_MODE_HELPER_V1:";
const ERROR_PREFIX: &str = "// AFSSR_CODE_MODE_HELPER_ERROR_V1:";
const MAX_BYTES: usize = 1024 * 1024;
const PATCH_DESCRIPTION: &str = "Edit files using the client's explicitly advertised tools.apply_patch helper. The patch argument is one string containing the complete Codex patch, not a unified diff or a JSON object. Start with *** Begin Patch and end with *** End Patch. Each operation starts with *** Add File: path, *** Update File: path, or *** Delete File: path. For Add File, prefix every content line with +. For Update File, use @@ (optionally followed by a context anchor), then prefix unchanged lines with a space, removed lines with -, and added lines with +. Do not include ---/+++ file headers or @@ -old,+new line-number ranges. A rename is *** Move to: new_path immediately after an Update File header. End-of-file context uses *** End of File. Paths are relative to the current workspace unless the client allows an absolute path. For example, the patch string to create example.txt containing hello is:\n*** Begin Patch\n*** Add File: example.txt\n+hello\n*** End Patch\nDo not wrap the patch in Markdown fences. Do not use shell commands to write files. Use the actual task content and path, not this example's literal values.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CodeModeHelper {
    ExecCommand,
    ApplyPatch,
    ListTools,
    CallTool,
}

impl CodeModeHelper {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::ExecCommand => "exec_command",
            Self::ApplyPatch => "apply_patch",
            Self::ListTools => "list_client_tools",
            Self::CallTool => "call_client_tool",
        }
    }

    pub(crate) fn advertised(tool: &Value) -> Vec<Self> {
        let description = tool["description"].as_str().unwrap_or_default();
        if tool["name"] != "exec"
            || tool["type"] != "custom"
            || tool["format"]["type"] != "grammar"
            || tool["format"]["syntax"] != "lark"
            || !tool["format"]["definition"]
                .as_str()
                .is_some_and(|grammar| {
                    grammar.contains("pragma_source") && grammar.contains("SOURCE:")
                })
        {
            return Vec::new();
        }
        [
            Self::ExecCommand,
            Self::ApplyPatch,
            Self::ListTools,
            Self::CallTool,
        ]
        .into_iter()
        .filter(|helper| match helper {
            Self::ExecCommand => {
                description.contains("exec_command(args:")
                    && description.contains("tools.exec_command")
                    && description.contains("sandbox_permissions")
            }
            Self::ApplyPatch => {
                (description.contains("apply_patch(input: string)")
                    && description.contains("tools.apply_patch"))
                    || (description.contains("ALL_TOOLS")
                        && description.contains(
                            "All nested tools are available on the global `tools` object",
                        )
                        && description.contains("tools.exec_command"))
            }
            Self::ListTools | Self::CallTool => {
                description.contains("ALL_TOOLS")
                    && description
                        .contains("All nested tools are available on the global `tools` object")
            }
        })
        .collect()
    }

    pub(crate) fn definition(self) -> Value {
        let (description, parameters) = match self {
            Self::ExecCommand => ("Run a shell command using the client's explicitly advertised tools.exec_command helper. The client applies its normal permissions and approval checks. Use apply_patch for file edits.", json!({
                "type":"object", "required":["cmd"], "additionalProperties":false,
                "properties":{
                    "cmd":{"type":"string"}, "workdir":{"type":"string"},
                    "justification":{"type":"string"}, "shell":{"type":"string"},
                    "login":{"type":"boolean"}, "tty":{"type":"boolean"},
                    "max_output_tokens":{"type":"integer","minimum":0},
                    "yield_time_ms":{"type":"integer","minimum":0},
                    "sandbox_permissions":{"type":"string","enum":["use_default","require_escalated"]},
                    "prefix_rule":{"type":"array","items":{"type":"string"}}
                }
            })),
            Self::ApplyPatch => (PATCH_DESCRIPTION, json!({
                "type":"object", "properties":{"patch":{"type":"string"}},
                "required":["patch"], "additionalProperties":false
            })),
            Self::ListTools => ("Search the client's available tool catalog by name or description. Pass a query such as apply_patch; returns matching tool names and descriptions, capped at 20 results. This is discovery, not execution. Use call_client_tool to invoke a discovered tool without writing JavaScript.", json!({
                "type":"object","properties":{"query":{"type":"string"}},
                "required":["query"],"additionalProperties":false
            })),
            Self::CallTool => ("Invoke an existing client tool by its exact name from list_client_tools. Pass arguments as a JSON object for structured tools, or a string for freeform tools such as apply_patch. No JavaScript is required. The client checks its available tool catalog and applies normal permissions and approvals. Never invent a tool name. Prefer exec_command for shell commands and apply_patch for editing files.", json!({
                "type":"object","properties":{"name":{"type":"string"},"arguments":{"type":["object","string"]}},
                "required":["name","arguments"],"additionalProperties":false
            })),
        };
        json!({"type":"function","name":self.name(),"description":description,"parameters":parameters})
    }

    fn validate(self, arguments: &Value) -> Result<(), &'static str> {
        let object = arguments
            .as_object()
            .ok_or("Code Mode helper arguments must be an object")?;
        let valid = match self {
            Self::ApplyPatch => object.len() == 1 && arguments["patch"].is_string(),
            Self::ListTools => object.len() == 1 && arguments["query"].is_string(),
            Self::CallTool => {
                object.len() == 2
                    && arguments["name"].as_str().is_some_and(|name| {
                        !name.is_empty()
                            && !matches!(name, "__proto__" | "prototype" | "constructor")
                    })
                    && (arguments["arguments"].is_object() || arguments["arguments"].is_string())
                    && js_safe_numbers(&arguments["arguments"])
            }
            Self::ExecCommand => {
                arguments["cmd"].is_string()
                    && object.iter().all(|(key, value)| match key.as_str() {
                        "cmd" | "workdir" | "justification" | "shell" => value.is_string(),
                        "login" | "tty" => value.is_boolean(),
                        "max_output_tokens" | "yield_time_ms" => value.as_u64().is_some(),
                        "sandbox_permissions" => {
                            matches!(value.as_str(), Some("use_default" | "require_escalated"))
                        }
                        "prefix_rule" => value
                            .as_array()
                            .is_some_and(|items| items.iter().all(Value::is_string)),
                        _ => false,
                    })
            }
        };
        if valid {
            Ok(())
        } else {
            Err("Code Mode helper arguments violate the advertised schema")
        }
    }

    pub(crate) fn encode(self, arguments: &str) -> Result<String, &'static str> {
        if arguments.len() > MAX_BYTES {
            return Err("Code Mode helper arguments exceed limit");
        }
        let raw = arguments;
        let arguments: Value = match serde_json::from_str(arguments) {
            Ok(arguments) => arguments,
            Err(_) => return Ok(self.error_projection(raw)),
        };
        if self.validate(&arguments).is_err() {
            return Ok(self.error_projection(raw));
        }
        let metadata = json!({"helper":self.name(),"arguments":arguments});
        let call = match self {
            Self::ExecCommand => format!("tools.exec_command({arguments})"),
            Self::ApplyPatch => format!("tools.apply_patch({})", arguments["patch"]),
            Self::ListTools => format!("(async () => {{ if (typeof ALL_TOOLS === \"undefined\") throw new Error(\"Client does not expose a tool catalog\"); const query = {}.toLowerCase(); const matches = ALL_TOOLS.filter(t => (String(t.name) + \" \" + String(t.description || \"\")).toLowerCase().includes(query)).map(t => ({{name:t.name,description:t.description || \"\"}})); return {{count:matches.length,tools:matches.slice(0,20)}}; }})()",arguments["query"]),
            Self::CallTool => format!("(async () => {{ const name = {}; if (typeof ALL_TOOLS === \"undefined\" || !ALL_TOOLS.some(t => t.name === name) || typeof tools[name] !== \"function\") throw new Error(\"Client does not advertise the requested tool\"); return await tools[name](JSON.parse({})); }})()",arguments["name"],json!(arguments["arguments"].to_string())),
        };
        // The marker makes history reversible; the client, never the gateway,
        // executes this JSON-escaped invocation under its existing permissions.
        let guard = if self == Self::ApplyPatch {
            "if (typeof ALL_TOOLS !== \"undefined\" && !ALL_TOOLS.some(t => t.name === \"apply_patch\")) throw new Error(\"Client does not advertise apply_patch\");\n"
        } else {
            ""
        };
        Ok(format!(
            "{PREFIX}{}\n{guard}text(await {call});",
            STANDARD_NO_PAD.encode(metadata.to_string())
        ))
    }

    fn error_projection(self, arguments: &str) -> String {
        let expected = match self {
            Self::ExecCommand => "provide a cmd string and only the declared option fields",
            Self::ApplyPatch => "provide exactly one patch string, starting with *** Begin Patch and ending with *** End Patch",
            Self::ListTools => "provide exactly one query string",
            Self::CallTool => "provide an advertised name string and arguments as an object or string, without extra fields or unsafe integer values",
        };
        let message = format!("Invalid arguments for {}: {expected}. No tool was executed. Retry with the documented parameters.", self.name());
        let metadata = json!({"helper":self.name(),"arguments":arguments});
        // Reject execution but preserve a reversible call. The client returns
        // this error as a tool result so a malformed call cannot tear down SSE.
        format!(
            "{ERROR_PREFIX}{}\nthrow new Error({});",
            STANDARD_NO_PAD.encode(metadata.to_string()),
            json!(message)
        )
    }

    pub(crate) fn decode(input: &str) -> Option<(Self, String)> {
        if input.len() > MAX_BYTES * 3 {
            return None;
        }
        let first = input.lines().next()?;
        let is_error = first.starts_with(ERROR_PREFIX);
        let encoded = first.strip_prefix(if is_error { ERROR_PREFIX } else { PREFIX })?;
        let bytes = STANDARD_NO_PAD.decode(encoded).ok()?;
        let metadata: Value = serde_json::from_slice(&bytes).ok()?;
        if metadata.as_object()?.len() != 2 {
            return None;
        }
        let helper = match metadata["helper"].as_str()? {
            "exec_command" => Self::ExecCommand,
            "apply_patch" => Self::ApplyPatch,
            "list_client_tools" => Self::ListTools,
            "call_client_tool" => Self::CallTool,
            _ => return None,
        };
        let arguments = if is_error {
            metadata.get("arguments")?.as_str()?.to_string()
        } else {
            metadata.get("arguments")?.to_string()
        };
        // A marker alone is not trusted: require the entire invocation to match.
        (helper.encode(&arguments).ok()? == input).then_some((helper, arguments))
    }
}

fn js_safe_numbers(value: &Value) -> bool {
    match value {
        Value::Number(number) if number.is_i64() => number
            .as_i64()
            .is_some_and(|value| value.unsigned_abs() <= 9_007_199_254_740_991),
        Value::Number(number) if number.is_u64() => number
            .as_u64()
            .is_some_and(|value| value <= 9_007_199_254_740_991),
        Value::Array(items) => items.iter().all(js_safe_numbers),
        Value::Object(items) => items.values().all(js_safe_numbers),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_helper_advertises_operation_grammar_without_changing_payload() {
        let definition = CodeModeHelper::ApplyPatch.definition();
        let description = definition["description"].as_str().unwrap();
        for required in [
            "*** Begin Patch",
            "*** End Patch",
            "*** Add File:",
            "*** Update File:",
            "*** Delete File:",
            "*** Move to:",
            "*** End of File",
            "not a unified diff",
            "---/+++",
        ] {
            assert!(description.contains(required), "missing {required}");
        }
        let arguments =
            json!({"patch":"*** Begin Patch\n*** Add File: output.txt\n+literal\n*** End Patch"});
        let encoded = CodeModeHelper::ApplyPatch
            .encode(&arguments.to_string())
            .unwrap();
        assert_eq!(
            CodeModeHelper::decode(&encoded),
            Some((CodeModeHelper::ApplyPatch, arguments.to_string()))
        );
        assert_eq!(definition["parameters"]["required"], json!(["patch"]));
    }

    #[test]
    fn helper_payload_is_reversible_and_never_interpolates_commands() {
        for (helper, arguments) in [
            (
                CodeModeHelper::ExecCommand,
                json!({"cmd":"printf 'a\"b'; echo $(whoami)\n", "sandbox_permissions":"require_escalated", "justification":"test"}),
            ),
            (
                CodeModeHelper::ApplyPatch,
                json!({"patch":"*** Begin Patch\n*** Add File: nonce\n+\"literal\"\n*** End Patch"}),
            ),
        ] {
            let encoded = helper.encode(&arguments.to_string()).unwrap();
            assert_eq!(
                CodeModeHelper::decode(&encoded),
                Some((helper, arguments.to_string()))
            );
            assert!(CodeModeHelper::decode(&(encoded.clone() + "\nprocess.exit()")).is_none());
            assert!(encoded.contains("text(await tools."));
        }
    }

    #[test]
    fn undeclared_or_invalid_arguments_fail_closed_without_reflection() {
        for arguments in [
            json!({"cmd":"secret","unknown":true}),
            json!({"cmd":42}),
            json!({"cmd":"x","tty":"true"}),
            json!({"cmd":"x","sandbox_permissions":"bypass"}),
        ] {
            let error = CodeModeHelper::ExecCommand
                .encode(&arguments.to_string())
                .unwrap();
            assert!(error.starts_with(ERROR_PREFIX));
            assert!(!error.contains("tools.exec_command("));
            assert!(!error.contains("secret"));
            assert_eq!(
                CodeModeHelper::decode(&error),
                Some((CodeModeHelper::ExecCommand, arguments.to_string()))
            );
        }
        for raw in [
            "{}",
            "{\"input\":\"patch\"}",
            "not valid JSON; process.exit()",
        ] {
            let projected = CodeModeHelper::ApplyPatch.encode(raw).unwrap();
            assert!(projected.starts_with(ERROR_PREFIX));
            assert!(!projected.contains("tools.apply_patch("));
            assert_eq!(
                CodeModeHelper::decode(&projected),
                Some((CodeModeHelper::ApplyPatch, raw.to_string()))
            );
            assert!(CodeModeHelper::decode(&(projected + "\nprocess.exit()")).is_none());
        }
        assert!(CodeModeHelper::ApplyPatch
            .encode(&"x".repeat(MAX_BYTES + 1))
            .is_err());
    }

    #[test]
    fn catalog_helpers_keep_arguments_opaque_and_require_client_advertisement() {
        for (helper, arguments) in [
            (
                CodeModeHelper::ListTools,
                json!({"query":"\"; process.exit() //"}),
            ),
            (
                CodeModeHelper::CallTool,
                json!({"name":"exec_command","arguments":{"cmd":"echo literal"}}),
            ),
            (
                CodeModeHelper::CallTool,
                json!({"name":"apply_patch","arguments":"*** Begin Patch\n*** End Patch"}),
            ),
        ] {
            let encoded = helper.encode(&arguments.to_string()).unwrap();
            assert_eq!(
                CodeModeHelper::decode(&encoded),
                Some((helper, arguments.to_string()))
            );
            assert!(encoded.contains("typeof ALL_TOOLS"));
            assert!(CodeModeHelper::decode(&(encoded + "\nprocess.exit()")).is_none());
        }
        for arguments in [
            json!({"name":"constructor","arguments":{}}),
            json!({"name":"exec_command","arguments":null}),
            json!({"name":"exec_command","arguments":{},"extra":true}),
            json!({"name":"synthetic_echo","arguments":{"nested":[9007199254740993u64]}}),
        ] {
            assert!(CodeModeHelper::CallTool
                .encode(&arguments.to_string())
                .unwrap()
                .starts_with(ERROR_PREFIX));
        }
    }
}
