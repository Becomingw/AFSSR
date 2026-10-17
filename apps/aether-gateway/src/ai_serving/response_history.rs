use crate::ai_serving::{
    hydrate_response_history, normalize_api_format_alias, record_converted_response_history,
    response_history_is_loaded, response_history_storage_key, ResponseHistoryRecord,
};
use serde_json::Value;
use tracing::warn;

use crate::{AppState, GatewayError};

const RESPONSE_HISTORY_SECRET_PURPOSE: &str = "openai-response-history";

#[allow(clippy::too_many_arguments)]
pub(crate) async fn prepare_go_native_response_history(
    state: &AppState,
    request: &Value,
    client_api_format: &str,
    provider_api_format: &str,
    provider_type: &str,
    model: &str,
    scope: &str,
) -> Result<Option<Value>, GatewayError> {
    if !provider_type.eq_ignore_ascii_case("opencode_go")
        || normalize_api_format_alias(client_api_format) != "openai:responses"
        || normalize_api_format_alias(provider_api_format) != "openai:responses"
        || !crate::ai_serving::uses_gateway_response_history(model)
    {
        return Ok(None);
    }
    hydrate_openai_response_history(
        state,
        request,
        client_api_format,
        provider_api_format,
        scope,
    )
    .await?;
    crate::ai_serving::expand_go_native_response_history(request, model, Some(scope))
        .map(Some)
        .map_err(|_| GatewayError::Client {
            status: axum::http::StatusCode::BAD_REQUEST,
            message: "Go Muse response history is unavailable for this credential scope"
                .to_string(),
        })
}

pub(crate) async fn hydrate_openai_response_history(
    state: &AppState,
    request: &Value,
    client_api_format: &str,
    provider_api_format: &str,
    history_scope: &str,
) -> Result<(), GatewayError> {
    if normalize_api_format_alias(client_api_format) != "openai:responses"
        || !matches!(
            normalize_api_format_alias(provider_api_format).as_str(),
            "openai:chat" | "claude:messages" | "gemini:generate_content" | "openai:responses"
        )
    {
        return Ok(());
    }
    let Some(previous_response_id) = request
        .get("previous_response_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    if response_history_is_loaded(previous_response_id, Some(history_scope)) {
        return Ok(());
    }

    let storage_key = response_history_storage_key(previous_response_id, Some(history_scope));
    let runtime_state = state.runtime_state();
    let payload = runtime_state.kv_get(&storage_key).await.map_err(|error| {
        warn!(
            event_name = "openai_response_history_read_failed",
            log_type = "ops",
            backend = runtime_state.backend_kind().as_str(),
            error = ?error,
            "gateway failed to read shared OpenAI response history"
        );
        GatewayError::Internal("OpenAI response history lookup failed".to_string())
    })?;
    let Some(payload) = payload else {
        return Ok(());
    };
    let Some(payload) = crate::handlers::shared::open_runtime_secret_payload(
        state,
        RESPONSE_HISTORY_SECRET_PURPOSE,
        &payload,
    ) else {
        let _ = runtime_state.kv_delete(&storage_key).await;
        warn!(
            event_name = "openai_response_history_decryption_failed",
            log_type = "ops",
            backend = runtime_state.backend_kind().as_str(),
            "gateway rejected undecryptable shared OpenAI response history"
        );
        return Err(GatewayError::Internal(
            "OpenAI response history decryption failed".to_string(),
        ));
    };
    if let Err(error) =
        hydrate_response_history(previous_response_id, Some(history_scope), payload.as_str())
    {
        let _ = runtime_state.kv_delete(&storage_key).await;
        warn!(
            event_name = "openai_response_history_invalid",
            log_type = "ops",
            backend = runtime_state.backend_kind().as_str(),
            error = %error,
            "gateway rejected invalid shared OpenAI response history"
        );
        return Err(GatewayError::Internal(
            "OpenAI response history validation failed".to_string(),
        ));
    }
    Ok(())
}

pub(crate) async fn persist_response_history_record(
    state: &AppState,
    record: ResponseHistoryRecord,
) {
    let runtime_state = state.runtime_state();
    let Some(sealed_payload) = crate::handlers::shared::seal_runtime_secret_payload(
        state,
        RESPONSE_HISTORY_SECRET_PURPOSE,
        &record.payload,
    ) else {
        warn!(
            event_name = "openai_response_history_encryption_unavailable",
            log_type = "ops",
            backend = runtime_state.backend_kind().as_str(),
            "gateway refused to persist unencrypted OpenAI response history"
        );
        return;
    };
    if let Err(error) = runtime_state
        .kv_set(&record.storage_key, sealed_payload, Some(record.ttl))
        .await
    {
        warn!(
            event_name = "openai_response_history_write_failed",
            log_type = "ops",
            backend = runtime_state.backend_kind().as_str(),
            error = ?error,
            "gateway failed to persist shared OpenAI response history"
        );
    }
}

pub(crate) async fn persist_converted_response_history(
    state: &AppState,
    report_context: &Value,
    response: Option<&Value>,
) {
    let Some(response) = response else {
        return;
    };
    if let Some(record) = record_converted_response_history(report_context, response) {
        persist_response_history_record(state, record).await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use aether_crypto::DEVELOPMENT_ENCRYPTION_KEY;
    use aether_runtime_state::{MemoryRuntimeStateConfig, RuntimeState};
    use serde_json::json;
    use sha2::{Digest, Sha256};

    use super::{
        hydrate_openai_response_history, persist_response_history_record, ResponseHistoryRecord,
    };
    use crate::{ai_serving::response_history_storage_key, data::GatewayDataState, AppState};

    fn response_history_test_state() -> AppState {
        AppState::new()
            .expect("test state should build")
            .with_data_state_for_tests(
                GatewayDataState::disabled()
                    .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY),
            )
            .with_runtime_state(Arc::new(RuntimeState::memory(
                MemoryRuntimeStateConfig::default(),
            )))
    }

    fn response_history_payload(response_id: &str, scope: &str, marker: &str) -> String {
        let expires_at_unix_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .saturating_add(3600);
        json!({
            "version": 1,
            "response_id": response_id,
            "scope_fingerprint": format!("{:x}", Sha256::digest(scope.trim().as_bytes())),
            "expires_at_unix_secs": expires_at_unix_secs,
            "transcript": [{"type": "message", "content": marker}],
        })
        .to_string()
    }

    #[tokio::test]
    async fn response_history_is_encrypted_at_rest_and_hydrates() {
        let state = response_history_test_state();
        let response_id = "resp_gateway_encrypted_history_v1";
        let scope = "response-history-encrypted-scope";
        let marker = "private-response-history-marker";
        let storage_key = response_history_storage_key(response_id, Some(scope));
        let payload = response_history_payload(response_id, scope, marker);

        persist_response_history_record(
            &state,
            ResponseHistoryRecord {
                storage_key: storage_key.clone(),
                payload,
                ttl: Duration::from_secs(6 * 60 * 60),
            },
        )
        .await;

        let stored = state
            .runtime_kv_get(&storage_key)
            .await
            .expect("history lookup should succeed")
            .expect("history should be persisted");
        assert!(crate::handlers::shared::runtime_secret_payload_is_sealed(
            &stored
        ));
        assert!(!stored.contains(marker));

        hydrate_openai_response_history(
            &state,
            &json!({"previous_response_id": response_id}),
            "openai:responses",
            "openai:chat",
            scope,
        )
        .await
        .expect("encrypted history should hydrate");
        assert!(crate::ai_serving::response_history_is_loaded(
            response_id,
            Some(scope)
        ));
    }

    #[tokio::test]
    async fn claude_response_history_hydrates_encrypted_scoped_storage() {
        let state = response_history_test_state();
        let response_id = "resp_gateway_claude_encrypted_history_v1";
        let scope = "response-history-claude-scope";
        let storage_key = response_history_storage_key(response_id, Some(scope));
        persist_response_history_record(
            &state,
            ResponseHistoryRecord {
                storage_key,
                payload: response_history_payload(response_id, scope, "claude-signed-history"),
                ttl: Duration::from_secs(3600),
            },
        )
        .await;
        hydrate_openai_response_history(
            &state,
            &json!({"previous_response_id": response_id}),
            "openai:responses",
            "claude:messages",
            scope,
        )
        .await
        .expect("Claude history must hydrate from shared encrypted storage");
        assert!(crate::ai_serving::response_history_is_loaded(
            response_id,
            Some(scope)
        ));
        assert!(!crate::ai_serving::response_history_is_loaded(
            response_id,
            Some("unrelated-scope")
        ));
    }

    #[tokio::test]
    async fn muse_preparation_expands_encrypted_history_only_for_owned_scope() {
        let state = response_history_test_state();
        let id = "resp_gateway_muse_preparation_v1";
        let scope = "muse-preparation-scope";
        persist_response_history_record(
            &state,
            ResponseHistoryRecord {
                storage_key: response_history_storage_key(id, Some(scope)),
                payload: response_history_payload(id, scope, "synthetic prior turn"),
                ttl: Duration::from_secs(3600),
            },
        )
        .await;
        let request = json!({"previous_response_id":id,"include":["reasoning.encrypted_content"],
            "input":[{"role":"user","content":"synthetic next turn"}]});
        let prepared = super::prepare_go_native_response_history(
            &state,
            &request,
            "openai:responses",
            "openai:responses",
            "opencode_go",
            "muse-spark-1.3-contributor",
            scope,
        )
        .await
        .unwrap()
        .unwrap();
        assert!(prepared.get("previous_response_id").is_none());
        assert_eq!(prepared["input"].as_array().unwrap().len(), 2);
        assert_eq!(prepared["include"], request["include"]);
        let error = super::prepare_go_native_response_history(
            &state,
            &request,
            "openai:responses",
            "openai:responses",
            "opencode_go",
            "muse-spark-1.3-contributor",
            "wrong-scope",
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            crate::GatewayError::Client {
                status: axum::http::StatusCode::BAD_REQUEST,
                ..
            }
        ));
        for (provider, model) in [
            ("custom", "muse-spark-1.3-contributor"),
            ("opencode_go", "gpt-6-luna"),
        ] {
            assert!(super::prepare_go_native_response_history(
                &state,
                &request,
                "openai:responses",
                "openai:responses",
                provider,
                model,
                scope
            )
            .await
            .unwrap()
            .is_none());
        }
    }

    #[tokio::test]
    async fn response_history_reader_rejects_and_deletes_legacy_plaintext() {
        let state = response_history_test_state();
        let response_id = "resp_gateway_legacy_history_v1";
        let scope = "response-history-legacy-scope";
        let storage_key = response_history_storage_key(response_id, Some(scope));
        let payload = response_history_payload(response_id, scope, "legacy-private-history");
        state
            .runtime_kv_setex(&storage_key, &payload, 6 * 60 * 60)
            .await
            .expect("legacy history should store");

        let result = hydrate_openai_response_history(
            &state,
            &json!({"previous_response_id": response_id}),
            "openai:responses",
            "openai:chat",
            scope,
        )
        .await;
        assert!(result.is_err());
        assert!(!crate::ai_serving::response_history_is_loaded(
            response_id,
            Some(scope)
        ));
        assert!(state
            .runtime_kv_get(&storage_key)
            .await
            .expect("history lookup should succeed")
            .is_none());
    }
}
