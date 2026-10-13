use std::{
    collections::{HashMap, VecDeque},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::formats::{context::FormatContext, registry};

const HISTORY_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const MAX_HISTORY_ENTRIES: usize = 2_048;
const MAX_HISTORY_BYTES: usize = 64 * 1024 * 1024;
const MAX_HISTORY_ENTRY_BYTES: usize = 8 * 1024 * 1024;
const HISTORY_STORAGE_VERSION: u8 = 1;
const HISTORY_STORAGE_KEY_PREFIX: &str = "ai:responses:history:v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponseHistoryRecord {
    pub storage_key: String,
    pub payload: String,
    pub ttl: Duration,
}

#[derive(Serialize, Deserialize)]
struct PersistedResponseHistory {
    version: u8,
    response_id: String,
    scope_fingerprint: String,
    expires_at_unix_secs: u64,
    transcript: Vec<Value>,
}

#[derive(Clone)]
struct ResponseHistoryEntry {
    transcript: Vec<Value>,
    inserted_at: Instant,
    expires_at: Instant,
    size_bytes: usize,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct ResponseHistoryKey {
    scope: Option<String>,
    response_id: String,
}

#[derive(Default)]
struct ResponseHistoryStore {
    entries: HashMap<ResponseHistoryKey, ResponseHistoryEntry>,
    insertion_order: VecDeque<(ResponseHistoryKey, Instant)>,
    total_bytes: usize,
}

impl ResponseHistoryStore {
    fn remove(&mut self, key: &ResponseHistoryKey) {
        if let Some(entry) = self.entries.remove(key) {
            self.total_bytes = self.total_bytes.saturating_sub(entry.size_bytes);
        }
    }

    fn prune(&mut self, now: Instant) {
        let expired_keys = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.expires_at <= now)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in expired_keys {
            self.remove(&key);
        }

        while self.entries.len() > MAX_HISTORY_ENTRIES || self.total_bytes > MAX_HISTORY_BYTES {
            let Some((key, inserted_at)) = self.insertion_order.pop_front() else {
                break;
            };
            if self
                .entries
                .get(&key)
                .is_some_and(|entry| entry.inserted_at == inserted_at)
            {
                self.remove(&key);
            }
        }

        while let Some((key, inserted_at)) = self.insertion_order.front() {
            if self
                .entries
                .get(key)
                .is_some_and(|entry| entry.inserted_at == *inserted_at)
            {
                break;
            }
            self.insertion_order.pop_front();
        }
    }

    fn get(
        &mut self,
        response_id: &str,
        history_scope: Option<&str>,
        now: Instant,
    ) -> Option<Vec<Value>> {
        self.prune(now);
        self.entries
            .get(&response_history_key(response_id, history_scope))
            .map(|entry| entry.transcript.clone())
    }

    fn insert(
        &mut self,
        response_id: String,
        history_scope: Option<&str>,
        transcript: Vec<Value>,
        now: Instant,
        ttl: Duration,
    ) {
        if ttl.is_zero() {
            return;
        }
        let size_bytes = serde_json::to_vec(&transcript)
            .map(|bytes| bytes.len())
            .unwrap_or(MAX_HISTORY_ENTRY_BYTES.saturating_add(1));
        if size_bytes > MAX_HISTORY_ENTRY_BYTES {
            return;
        }
        let key = response_history_key(&response_id, history_scope);
        self.remove(&key);
        self.total_bytes = self.total_bytes.saturating_add(size_bytes);
        self.entries.insert(
            key.clone(),
            ResponseHistoryEntry {
                transcript,
                inserted_at: now,
                expires_at: now.checked_add(ttl).unwrap_or(now),
                size_bytes,
            },
        );
        self.insertion_order.push_back((key, now));
        self.prune(now);
    }
}

fn response_history_key(response_id: &str, history_scope: Option<&str>) -> ResponseHistoryKey {
    ResponseHistoryKey {
        scope: history_scope
            .map(str::trim)
            .filter(|scope| !scope.is_empty())
            .map(ToOwned::to_owned),
        response_id: response_id.to_string(),
    }
}

fn normalized_history_scope(history_scope: Option<&str>) -> Option<&str> {
    history_scope
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
}

fn sha256_hex(value: &[u8]) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn history_scope_fingerprint(history_scope: Option<&str>) -> String {
    sha256_hex(
        normalized_history_scope(history_scope)
            .unwrap_or("<unscoped>")
            .as_bytes(),
    )
}

pub fn response_history_storage_key(response_id: &str, history_scope: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(history_scope_fingerprint(history_scope));
    hasher.update([0]);
    hasher.update(response_id.trim().as_bytes());
    let digest = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{HISTORY_STORAGE_KEY_PREFIX}:{digest}")
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn response_history_store() -> &'static Mutex<ResponseHistoryStore> {
    static STORE: OnceLock<Mutex<ResponseHistoryStore>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(ResponseHistoryStore::default()))
}

pub fn response_history_is_loaded(response_id: &str, history_scope: Option<&str>) -> bool {
    response_history_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(response_id, history_scope, Instant::now())
        .is_some()
}

pub fn hydrate_response_history(
    response_id: &str,
    history_scope: Option<&str>,
    payload: &str,
) -> Result<(), String> {
    if payload.len() > MAX_HISTORY_ENTRY_BYTES {
        return Err("persisted response history exceeds the maximum entry size".to_string());
    }
    let persisted: PersistedResponseHistory = serde_json::from_str(payload)
        .map_err(|error| format!("invalid persisted response history: {error}"))?;
    if persisted.version != HISTORY_STORAGE_VERSION {
        return Err(format!(
            "unsupported response history version {}",
            persisted.version
        ));
    }
    if persisted.response_id != response_id.trim() {
        return Err(
            "persisted response history id does not match the requested response".to_string(),
        );
    }
    if persisted.scope_fingerprint != history_scope_fingerprint(history_scope) {
        return Err("persisted response history scope does not match the requester".to_string());
    }
    let now_unix_secs = current_unix_secs();
    let remaining_ttl = persisted
        .expires_at_unix_secs
        .checked_sub(now_unix_secs)
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
        .ok_or_else(|| "persisted response history has expired".to_string())?;
    response_history_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(
            response_id.trim().to_string(),
            history_scope,
            persisted.transcript,
            Instant::now(),
            remaining_ttl.min(HISTORY_TTL),
        );
    Ok(())
}

fn request_input_items(request: &Value) -> Vec<Value> {
    match request.get("input") {
        Some(Value::Array(items)) => items.clone(),
        Some(Value::String(text)) if !text.is_empty() => vec![json!({
            "type": "message",
            "role": "user",
            "content": text,
        })],
        _ if request.get("messages").and_then(Value::as_array).is_some() => {
            registry::convert_request(
                "openai:chat",
                "openai:responses",
                request,
                &FormatContext::default(),
            )
            .ok()
            .and_then(|converted| converted.get("input").and_then(Value::as_array).cloned())
            .unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

fn history_conversion_enabled(report_context: &Value) -> bool {
    report_context
        .get("needs_conversion")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && report_context
            .get("client_api_format")
            .and_then(Value::as_str)
            .is_some_and(|format| format.eq_ignore_ascii_case("openai:responses"))
        && report_context
            .get("provider_api_format")
            .and_then(Value::as_str)
            .is_some_and(|format| {
                format.eq_ignore_ascii_case("openai:chat")
                    || format.eq_ignore_ascii_case("claude:messages")
            })
}

pub(crate) fn expand_previous_response_for_chat(
    request: &Value,
    history_scope: Option<&str>,
) -> Result<Value, String> {
    let Some(previous_response_id) = request
        .get("previous_response_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(request.clone());
    };
    let mut store = response_history_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(mut transcript) = store.get(previous_response_id, history_scope, Instant::now())
    else {
        return Err(format!(
            "response history not found for previous_response_id {previous_response_id}"
        ));
    };
    transcript.extend(request_input_items(request));
    let mut expanded = request.clone();
    let Some(object) = expanded.as_object_mut() else {
        return Err("OpenAI Responses request must be a JSON object".to_string());
    };
    object.remove("previous_response_id");
    object.insert("input".to_string(), Value::Array(transcript));
    Ok(expanded)
}

pub fn record_converted_response_history(
    report_context: &Value,
    response: &Value,
) -> Option<ResponseHistoryRecord> {
    if !history_conversion_enabled(report_context)
        || response.get("status").and_then(Value::as_str) != Some("completed")
    {
        return None;
    }
    let response_id = response
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let request = report_context.get("original_request_body")?;
    let history_scope = report_context
        .get("api_key_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|scope| !scope.is_empty());
    let mut transcript = if request.get("messages").and_then(Value::as_array).is_some() {
        Vec::new()
    } else {
        request
            .get("previous_response_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .and_then(|previous_response_id| {
                response_history_store()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .get(previous_response_id, history_scope, Instant::now())
            })
            .unwrap_or_default()
    };
    let mut current_input = request_input_items(request);
    if report_context
        .get("provider_api_format")
        .and_then(Value::as_str)
        == Some("claude:messages")
    {
        // Tool definitions belong to the current request, not conversation
        // messages. Keeping Lite envelopes here duplicates them on continuation.
        current_input.retain(|item| item["type"] != "additional_tools");
    }
    transcript.extend(current_input);
    if let Some(output) = response.get("output").and_then(Value::as_array) {
        transcript.extend(output.iter().cloned());
    }
    let expires_at_unix_secs = current_unix_secs().saturating_add(HISTORY_TTL.as_secs());
    let payload = serde_json::to_string(&PersistedResponseHistory {
        version: HISTORY_STORAGE_VERSION,
        response_id: response_id.to_string(),
        scope_fingerprint: history_scope_fingerprint(history_scope),
        expires_at_unix_secs,
        transcript: transcript.clone(),
    })
    .ok()?;
    if payload.len() > MAX_HISTORY_ENTRY_BYTES {
        return None;
    }
    response_history_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(
            response_id.to_string(),
            history_scope,
            transcript,
            Instant::now(),
            HISTORY_TTL,
        );
    Some(ResponseHistoryRecord {
        storage_key: response_history_storage_key(response_id, history_scope),
        payload,
        ttl: HISTORY_TTL,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::formats::{context::FormatContext, registry::convert_request};

    use super::{
        expand_previous_response_for_chat, hydrate_response_history,
        record_converted_response_history, response_history_key, response_history_storage_key,
        response_history_store,
    };

    fn conversion_report_context(original_request_body: serde_json::Value) -> serde_json::Value {
        json!({
            "needs_conversion": true,
            "client_api_format": "openai:responses",
            "provider_api_format": "openai:chat",
            "original_request_body": original_request_body,
        })
    }

    #[test]
    fn expands_previous_response_with_assistant_call_before_tool_output() {
        let report_context = conversion_report_context(json!({
            "model": "deepseek-v4-flash",
            "input": [{"role": "user", "content": "scan the repository"}]
        }));
        record_converted_response_history(
            &report_context,
            &json!({
                "id": "resp_history_expand_test_1",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "id": "fc_history_expand_test_1",
                    "call_id": "call_history_expand_test_1",
                    "name": "security_scan",
                    "arguments": "{\"depth\":\"deep\"}"
                }]
            }),
        );

        let expanded = expand_previous_response_for_chat(
            &json!({
                "model": "deepseek-v4-flash",
                "previous_response_id": "resp_history_expand_test_1",
                "input": [{
                    "type": "function_call_output",
                    "call_id": "call_history_expand_test_1",
                    "output": "manifest-created"
                }]
            }),
            None,
        )
        .expect("stored previous response should expand");

        assert!(expanded.get("previous_response_id").is_none());
        assert_eq!(expanded["input"][1]["type"], "function_call");
        assert_eq!(expanded["input"][1]["id"], "fc_history_expand_test_1");
        assert_eq!(
            expanded["input"][1]["call_id"],
            "call_history_expand_test_1"
        );
        assert_eq!(expanded["input"][2]["type"], "function_call_output");
        assert_eq!(
            expanded["input"][2]["call_id"],
            "call_history_expand_test_1"
        );
    }

    #[test]
    fn restores_persisted_history_after_local_cache_reset() {
        let report_context = json!({
            "needs_conversion": true,
            "client_api_format": "openai:responses",
            "provider_api_format": "openai:chat",
            "api_key_id": "distributed-history-key-a",
            "original_request_body": {
                "model": "deepseek-v4-flash",
                "input": [{"role": "user", "content": "inspect the repository"}]
            }
        });
        let record = record_converted_response_history(
            &report_context,
            &json!({
                "id": "resp_distributed_history_1",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "id": "fc_distributed_history_1",
                    "call_id": "call_distributed_history_1",
                    "name": "inspect_repository",
                    "arguments": "{}"
                }]
            }),
        )
        .expect("completed conversion should produce a persistence record");
        assert_eq!(
            record.storage_key,
            response_history_storage_key(
                "resp_distributed_history_1",
                Some("distributed-history-key-a")
            )
        );
        assert!(!record.storage_key.contains("distributed-history-key-a"));
        assert!(!record.storage_key.contains("resp_distributed_history_1"));

        response_history_store()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&response_history_key(
                "resp_distributed_history_1",
                Some("distributed-history-key-a"),
            ));
        let continuation = json!({
            "model": "deepseek-v4-flash",
            "previous_response_id": "resp_distributed_history_1",
            "input": [{
                "type": "function_call_output",
                "call_id": "call_distributed_history_1",
                "output": "inspection-complete"
            }]
        });
        assert!(expand_previous_response_for_chat(
            &continuation,
            Some("distributed-history-key-a")
        )
        .is_err());

        hydrate_response_history(
            "resp_distributed_history_1",
            Some("distributed-history-key-a"),
            &record.payload,
        )
        .expect("another instance should hydrate the persisted transcript");
        let expanded =
            expand_previous_response_for_chat(&continuation, Some("distributed-history-key-a"))
                .expect("hydrated history should support the continuation");
        assert_eq!(
            expanded["input"][1]["call_id"],
            "call_distributed_history_1"
        );
        assert_eq!(
            expanded["input"][2]["call_id"],
            "call_distributed_history_1"
        );
    }

    #[test]
    fn restores_history_recorded_from_redacted_chat_messages() {
        record_converted_response_history(
            &conversion_report_context(json!({
                "model": "deepseek-v4-flash",
                "messages": [
                    {"role": "user", "content": "scan the redacted repository"},
                    {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call_redacted_previous_1",
                            "type": "function",
                            "function": {
                                "name": "security_scan",
                                "arguments": "{\"depth\":\"quick\"}"
                            }
                        }]
                    },
                    {
                        "role": "tool",
                        "tool_call_id": "call_redacted_previous_1",
                        "content": "quick-scan-complete"
                    }
                ]
            })),
            &json!({
                "id": "resp_history_redacted_test_1",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "id": "fc_history_redacted_test_1",
                    "call_id": "call_redacted_next_1",
                    "name": "deep_scan",
                    "arguments": "{\"scope\":\"changed-files\"}"
                }]
            }),
        );

        let expanded = expand_previous_response_for_chat(
            &json!({
                "model": "deepseek-v4-flash",
                "previous_response_id": "resp_history_redacted_test_1",
                "input": [{
                    "type": "function_call_output",
                    "call_id": "call_redacted_next_1",
                    "output": "deep-scan-complete"
                }]
            }),
            None,
        )
        .expect("redacted Chat history should expand");
        let input = expanded["input"]
            .as_array()
            .expect("expanded input should be an array");

        assert_eq!(input.len(), 5);
        assert_eq!(input[1]["type"], "function_call");
        assert_eq!(input[1]["call_id"], "call_redacted_previous_1");
        assert_eq!(input[2]["type"], "function_call_output");
        assert_eq!(input[2]["call_id"], "call_redacted_previous_1");
        assert_eq!(input[3]["id"], "fc_history_redacted_test_1");
        assert_eq!(input[3]["call_id"], "call_redacted_next_1");
        assert_eq!(input[4]["type"], "function_call_output");
        assert_eq!(input[4]["call_id"], "call_redacted_next_1");
    }

    #[test]
    fn isolates_response_history_by_api_key_scope() {
        let report_context = json!({
            "needs_conversion": true,
            "client_api_format": "openai:responses",
            "provider_api_format": "openai:chat",
            "api_key_id": "key-history-scope-a",
            "original_request_body": {
                "model": "deepseek-v4-flash",
                "input": [{"role": "user", "content": "private request"}]
            }
        });
        record_converted_response_history(
            &report_context,
            &json!({
                "id": "resp_history_scope_test_1",
                "status": "completed",
                "output": [{"type": "message", "role": "assistant", "content": []}]
            }),
        );
        let continuation = json!({
            "model": "deepseek-v4-flash",
            "previous_response_id": "resp_history_scope_test_1",
            "input": "continue"
        });

        let owner_result = convert_request(
            "openai:responses",
            "openai:chat",
            &continuation,
            &FormatContext::default().with_history_scope("key-history-scope-a"),
        );
        assert!(owner_result.is_ok());

        let other_user_error = convert_request(
            "openai:responses",
            "openai:chat",
            &continuation,
            &FormatContext::default().with_history_scope("key-history-scope-b"),
        )
        .expect_err("another API key must not recover scoped history");
        assert!(matches!(
            other_user_error,
            crate::formats::context::FormatError::UnsupportedField { ref field, .. }
                if field == "previous_response_id"
        ));
    }

    #[test]
    fn converts_all_forty_two_tools_and_forces_late_tools() {
        let tools = (0..42)
            .map(|index| {
                json!({
                    "type": "function",
                    "name": format!("security_tool_{index:02}"),
                    "description": format!("Security tool {index}"),
                    "parameters": {
                        "type": "object",
                        "properties": {"path": {"type": "string"}},
                        "required": ["path"]
                    }
                })
            })
            .collect::<Vec<_>>();

        for forced_index in [18usize, 41usize] {
            let body = json!({
                "model": "deepseek-v4-flash",
                "input": [{"role": "user", "content": "run the selected scan"}],
                "tools": tools,
                "tool_choice": {
                    "type": "function",
                    "name": format!("security_tool_{forced_index:02}")
                }
            });
            let converted = convert_request(
                "openai:responses",
                "openai:chat",
                &body,
                &FormatContext::default(),
            )
            .expect("all Responses tools should convert to Chat");
            let converted_tools = converted["tools"]
                .as_array()
                .expect("chat tools should be an array");

            assert_eq!(converted_tools.len(), 42);
            for (index, tool) in converted_tools.iter().enumerate() {
                assert_eq!(
                    tool["function"]["name"],
                    json!(format!("security_tool_{index:02}"))
                );
            }
            assert_eq!(
                converted["tool_choice"]["function"]["name"],
                json!(format!("security_tool_{forced_index:02}"))
            );
        }
    }

    #[test]
    fn restores_two_consecutive_tool_call_rounds() {
        let first_request = json!({
            "model": "deepseek-v4-flash",
            "input": [{"role": "user", "content": "perform a deep scan"}],
            "parallel_tool_calls": true
        });
        record_converted_response_history(
            &conversion_report_context(first_request.clone()),
            &json!({
                "id": "resp_history_multiturn_test_1",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "id": "fc_history_multiturn_test_1",
                    "call_id": "call_discovery_manifest_1",
                    "name": "create_discovery_manifest",
                    "arguments": "{\"root\":\"src\"}"
                }]
            }),
        );

        let second_request = json!({
            "model": "deepseek-v4-flash",
            "previous_response_id": "resp_history_multiturn_test_1",
            "input": [{
                "type": "function_call_output",
                "call_id": "call_discovery_manifest_1",
                "output": "manifest-1"
            }],
            "parallel_tool_calls": true
        });
        let second_chat = convert_request(
            "openai:responses",
            "openai:chat",
            &second_request,
            &FormatContext::default(),
        )
        .expect("first continuation should convert");
        assert_eq!(second_chat["messages"][1]["role"], "assistant");
        assert_eq!(
            second_chat["messages"][1]["tool_calls"][0]["id"],
            "call_discovery_manifest_1"
        );
        assert_eq!(second_chat["messages"][2]["role"], "tool");
        assert_eq!(
            second_chat["messages"][2]["tool_call_id"],
            "call_discovery_manifest_1"
        );

        record_converted_response_history(
            &conversion_report_context(second_request.clone()),
            &json!({
                "id": "resp_history_multiturn_test_2",
                "status": "completed",
                "output": [
                    {
                        "type": "function_call",
                        "id": "fc_history_multiturn_test_2a",
                        "call_id": "call_deep_scan_2a",
                        "name": "deep_scan",
                        "arguments": "{\"manifest\":\"manifest-1\"}"
                    },
                    {
                        "type": "function_call",
                        "id": "fc_history_multiturn_test_2b",
                        "call_id": "call_audit_2b",
                        "name": "audit_results",
                        "arguments": "{\"manifest\":\"manifest-1\"}"
                    }
                ]
            }),
        );

        let third_chat = convert_request(
            "openai:responses",
            "openai:chat",
            &json!({
                "model": "deepseek-v4-flash",
                "previous_response_id": "resp_history_multiturn_test_2",
                "input": [
                    {
                        "type": "function_call_output",
                        "call_id": "call_deep_scan_2a",
                        "output": "scan-complete"
                    },
                    {
                        "type": "function_call_output",
                        "call_id": "call_audit_2b",
                        "output": "audit-complete"
                    }
                ]
            }),
            &FormatContext::default(),
        )
        .expect("second continuation should convert");
        let messages = third_chat["messages"]
            .as_array()
            .expect("chat messages should be an array");
        assert_eq!(messages.len(), 6);
        assert_eq!(messages[3]["role"], "assistant");
        assert_eq!(messages[3]["tool_calls"].as_array().map(Vec::len), Some(2));
        assert_eq!(messages[3]["tool_calls"][0]["id"], "call_deep_scan_2a");
        assert_eq!(messages[3]["tool_calls"][1]["id"], "call_audit_2b");
        assert_eq!(messages[4]["tool_call_id"], "call_deep_scan_2a");
        assert_eq!(messages[5]["tool_call_id"], "call_audit_2b");
    }
}
