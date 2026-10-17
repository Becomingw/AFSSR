use super::{
    hash_api_key, run_frontdoor_async_test, sample_models_candidate_row,
    sample_public_global_model, unrestricted_models_snapshot, InMemoryAuthApiKeySnapshotRepository,
    InMemoryGlobalModelReadRepository, InMemoryMinimalCandidateSelectionReadRepository,
};
use crate::data::GatewayDataState;
use crate::tests::{build_router_with_state, json, start_server, AppState, Arc, StatusCode};
use aether_data_contracts::repository::auth::AuthApiKeyWriteRepository;
use aether_data_contracts::repository::candidate_selection::StoredProviderModelMapping;

#[test]
fn gateway_opencode_conversation_sources_obey_client_chat_and_effective_model_permissions() {
    run_frontdoor_async_test("opencode-conversation-sources", async {
        let sources = [
            ("openai:chat", "text-chat"),
            ("openai:responses", "text-responses"),
            ("claude:messages", "text-claude"),
            ("gemini:generate_content", "text-gemini"),
        ];
        let expected = [
            "shared-alias",
            "text-chat",
            "text-claude",
            "text-codex",
            "text-gemini",
            "text-responses",
        ];
        let mut snapshot = unrestricted_models_snapshot("key", "user");
        let mut allowed = expected
            .iter()
            .map(|name| name.to_string())
            .collect::<Vec<_>>();
        allowed.extend(
            [
                "image-only",
                "embedding-only",
                "rerank-only",
                "compact-only",
                "provider-denied",
                "mapping-denied",
            ]
            .map(String::from),
        );
        snapshot.user_allowed_models = Some(
            allowed
                .iter()
                .cloned()
                .chain(["user-only".into()])
                .collect(),
        );
        snapshot.api_key_allowed_models =
            Some(allowed.into_iter().chain(["key-only".into()]).collect());
        snapshot.user_allowed_providers = Some(vec!["allowed-provider".into()]);
        snapshot.api_key_allowed_providers =
            Some(vec!["allowed-provider".into(), "denied-provider".into()]);
        snapshot.user_allowed_api_formats = Some(vec!["openai:chat".into()]);
        snapshot.api_key_allowed_api_formats = Some(vec!["/v1/chat/completions".into()]);

        let source_row = |format: &str, alias: &str| {
            let mut row = sample_models_candidate_row(
                "allowed-provider",
                "private-provider-name",
                format,
                alias,
                10,
            );
            row.endpoint_id = format!("source-endpoint-{format}");
            row.model_provider_model_name = "default-upstream-canary".into();
            let mapped_name = format!("mapped-upstream-canary-{format}");
            row.key_allowed_models = Some(vec![mapped_name.clone()]);
            row.model_provider_model_mappings = Some(vec![StoredProviderModelMapping {
                name: mapped_name,
                priority: 1,
                api_formats: Some(vec![format.into()]),
                endpoint_ids: Some(vec![row.endpoint_id.clone()]),
                operations: None,
            }]);
            row
        };
        let mut rows = Vec::new();
        for (format, alias) in sources {
            rows.push(source_row(format, alias));
            rows.push(source_row(format, "shared-alias"));
        }
        let mut codex = source_row("openai:responses", "text-codex");
        codex.provider_type = "codex".into();
        codex.key_auth_type = "oauth".into();
        rows.push(codex);
        for (format, alias) in [
            ("openai:image", "image-only"),
            ("openai:embedding", "embedding-only"),
            ("openai:rerank", "rerank-only"),
            ("openai:responses:compact", "compact-only"),
            ("claude:messages", "user-only"),
            ("gemini:generate_content", "key-only"),
        ] {
            rows.push(source_row(format, alias));
        }
        let mut provider_denied = source_row("claude:messages", "provider-denied");
        provider_denied.provider_id = "denied-provider".into();
        rows.push(provider_denied);
        let mut mapping_denied = source_row("claude:messages", "mapping-denied");
        mapping_denied
            .model_provider_model_mappings
            .as_mut()
            .expect("mapping")[0]
            .api_formats = Some(vec!["openai:chat".into()]);
        rows.push(mapping_denied);

        let auth = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![(
            Some(hash_api_key("synthetic-chat-only")),
            snapshot,
        )]));
        let data = GatewayDataState::with_minimal_candidate_selection_and_auth_for_tests(
            Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            auth,
        );
        let gateway = build_router_with_state(
            AppState::new()
                .expect("state")
                .with_data_state_for_tests(data),
        );
        let (url, handle) = start_server(gateway).await;
        let response = reqwest::Client::new()
            .get(format!("{url}/api/opencode/config"))
            .bearer_auth("synthetic-chat-only")
            .send()
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "private, no-store");
        let payload: serde_json::Value = response.json().await.expect("json");
        let models = payload["provider"]["aether"]["models"]
            .as_object()
            .expect("models");
        assert_eq!(
            models.keys().map(String::as_str).collect::<Vec<_>>(),
            expected
        );
        for alias in expected {
            assert_eq!(models[alias], json!({"id": alias, "name": alias}));
        }
        let serialized = payload.to_string();
        for private_value in [
            "upstream-canary",
            "source-endpoint",
            "private-provider-name",
            "synthetic-chat-only",
        ] {
            assert!(!serialized.contains(private_value), "private field leaked");
        }
        handle.abort();
    });
}

#[test]
fn gateway_opencode_catalog_uses_selected_api_key_and_safe_public_metadata() {
    run_frontdoor_async_test("opencode-catalog", async {
        let mut first = unrestricted_models_snapshot("first-key", "same-user");
        first.user_allowed_models = Some(vec![
            "public-alias".into(),
            "second-alias".into(),
            "provider-denied".into(),
            "source-denied".into(),
            "inactive".into(),
            "inactive-global".into(),
        ]);
        first.api_key_allowed_models = Some(vec![
            "public-alias".into(),
            "outside-user-policy".into(),
            "provider-denied".into(),
            "source-denied".into(),
            "inactive".into(),
            "inactive-global".into(),
        ]);
        first.user_allowed_providers = Some(vec!["allowed-provider".into()]);
        first.api_key_allowed_providers =
            Some(vec!["allowed-provider".into(), "denied-provider".into()]);
        first.user_allowed_api_formats = Some(vec!["/v1/chat/completions".into()]);
        first.api_key_allowed_api_formats =
            Some(vec!["openai:chat".into(), "claude:messages".into()]);
        let mut second = first.clone();
        second.api_key_id = "second-key".into();
        second.api_key_allowed_models = Some(vec!["second-alias".into()]);

        let auth = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![
            (Some(hash_api_key("synthetic-first")), first),
            (Some(hash_api_key("synthetic-second")), second),
        ]));
        let row = |provider: &str, name: &str| {
            let mut row = sample_models_candidate_row(provider, "custom", "openai:chat", name, 10);
            row.model_provider_model_name = "upstream-mapping-canary".into();
            row.model_provider_model_mappings = None;
            row
        };
        let mut source_denied = row("allowed-provider", "source-denied");
        source_denied.key_allowed_models = Some(vec!["unrelated-upstream-model".into()]);
        let mut inactive = row("allowed-provider", "inactive");
        inactive.model_is_active = false;
        let candidates = Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(vec![
            row("allowed-provider", "public-alias"),
            row("allowed-provider", "public-alias"),
            row("allowed-provider", "second-alias"),
            row("allowed-provider", "outside-user-policy"),
            row("denied-provider", "provider-denied"),
            source_denied,
            inactive,
            row("allowed-provider", "inactive-global"),
            sample_models_candidate_row(
                "allowed-provider",
                "custom",
                "openai:responses",
                "responses-only",
                10,
            ),
        ]));
        let mut metadata =
            sample_public_global_model("global-public-alias", "public-alias", "Public Model", true);
        metadata.default_price_per_request = None;
        metadata.config = Some(json!({
            "context_limit": 128000, "output_limit": 16000, "function_calling": true,
            "extended_thinking": true, "vision": true, "attachment": false,
            "input_modalities": ["text", "image"], "output_modalities": ["text"],
            "apiKey": "secret-canary", "model_mapping": "upstream-mapping-canary"
        }));
        let mut second_metadata =
            sample_public_global_model("global-second-alias", "second-alias", "second-alias", true);
        second_metadata.config = None;
        second_metadata.supported_capabilities = None;
        let data =
            GatewayDataState::with_minimal_candidate_selection_and_auth_for_tests(candidates, auth)
                .with_global_model_reader(Arc::new(InMemoryGlobalModelReadRepository::seed(vec![
                    metadata,
                    second_metadata,
                    sample_public_global_model(
                        "global-inactive-global",
                        "inactive-global",
                        "Disabled Global Model",
                        false,
                    ),
                ])));
        let gateway = build_router_with_state(
            AppState::new()
                .expect("state")
                .with_data_state_for_tests(data),
        );
        let (url, handle) = start_server(gateway).await;
        let client = reqwest::Client::new();
        let response = client
            .get(format!("{url}/api/opencode/config?client_version=ignored"))
            .bearer_auth("synthetic-first")
            .header("anthropic-version", "2023-06-01")
            .header("cookie", "session=ignored")
            .send()
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "private, no-store");
        assert_eq!(response.headers()["vary"], "Authorization");
        let payload: serde_json::Value = response.json().await.expect("json");
        let provider = &payload["provider"]["aether"];
        assert_eq!(payload["$schema"], "https://opencode.ai/config.json");
        assert_eq!(provider["npm"], "@ai-sdk/openai-compatible");
        assert!(provider["options"]["baseURL"]
            .as_str()
            .expect("base URL")
            .ends_with("/v1"));
        let models = provider["models"].as_object().expect("models");
        assert_eq!(
            models.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["public-alias"]
        );
        assert_eq!(models["public-alias"]["name"], "Public Model");
        assert_eq!(
            models["public-alias"]["limit"],
            json!({"context": 128000, "output": 16000})
        );
        assert_eq!(models["public-alias"]["tool_call"], true);
        assert_eq!(models["public-alias"]["reasoning"], true);
        assert_eq!(
            models["public-alias"]["cost"],
            json!({"input": 3.0, "output": 15.0})
        );
        let serialized = payload.to_string();
        for canary in [
            "synthetic-first",
            "secret-canary",
            "upstream-mapping-canary",
            "allowed-provider",
            "first-key",
            "same-user",
        ] {
            assert!(!serialized.contains(canary), "private field leaked");
        }

        let response = client
            .get(format!("{url}/api/opencode/config"))
            .bearer_auth("synthetic-second")
            .send()
            .await
            .expect("second request");
        assert_eq!(response.status(), StatusCode::OK);
        let payload: serde_json::Value = response.json().await.expect("json");
        assert_eq!(
            payload["provider"]["aether"]["models"],
            json!({"second-alias": {"id": "second-alias", "name": "second-alias"}})
        );
        handle.abort();
    });
}

#[test]
fn gateway_opencode_rejects_session_missing_invalid_expired_and_revoked_credentials() {
    run_frontdoor_async_test("opencode-auth", async {
        let active = unrestricted_models_snapshot("active-key", "user");
        let mut expired = unrestricted_models_snapshot("expired-key", "user");
        expired.api_key_expires_at_unix_secs = Some(1);
        let mut ip_denied = unrestricted_models_snapshot("ip-denied-key", "user");
        ip_denied.api_key_ip_rules = Some(vec!["192.0.2.1".into()]);
        let auth = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![
            (Some(hash_api_key("synthetic-active")), active),
            (Some(hash_api_key("synthetic-expired")), expired),
            (Some(hash_api_key("synthetic-ip-denied")), ip_denied),
        ]));
        let data = GatewayDataState::with_minimal_candidate_selection_and_auth_for_tests(
            Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(
                Vec::new(),
            )),
            auth.clone(),
        );
        let gateway = build_router_with_state(
            AppState::new()
                .expect("state")
                .with_data_state_for_tests(data),
        );
        let (url, handle) = start_server(gateway).await;
        let endpoint = format!("{url}/api/opencode/config");
        let client = reqwest::Client::new();
        let now = chrono::Utc::now();
        let session = crate::local_auth_token::create_local_auth_token(
            crate::local_auth_token::LocalAuthTokenType::Access,
            serde_json::Map::from_iter([
                ("user_id".into(), json!("user")),
                ("role".into(), json!("admin")),
                ("session_id".into(), json!("session")),
                ("created_at".into(), json!(now.to_rfc3339())),
            ]),
            now + chrono::Duration::hours(1),
        )
        .expect("synthetic session token");
        for token in [
            None,
            Some("synthetic-invalid"),
            Some("synthetic-expired"),
            Some(session.as_str()),
        ] {
            let mut request = client
                .get(&endpoint)
                .header("cookie", format!("session={session}"));
            if let Some(token) = token {
                request = request.bearer_auth(token);
            }
            let response = request.send().await.expect("request");
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert_eq!(response.headers()["cache-control"], "private, no-store");
        }
        let warm = client
            .get(&endpoint)
            .bearer_auth("synthetic-active")
            .send()
            .await
            .expect("warm request");
        assert_eq!(warm.status(), StatusCode::OK);
        assert_eq!(warm.headers()["cache-control"], "private, no-store");
        let ip_response = client
            .get(&endpoint)
            .bearer_auth("synthetic-ip-denied")
            .send()
            .await
            .expect("IP-denied request");
        assert_eq!(ip_response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(ip_response.headers()["cache-control"], "private, no-store");
        let ip_payload: serde_json::Value = ip_response.json().await.expect("IP error JSON");
        assert!(ip_payload.to_string().contains("IP"));
        assert!(auth
            .set_user_api_key_locked("user", "active-key", true)
            .await
            .expect("lock"));
        let revoked = client
            .get(&endpoint)
            .bearer_auth("synthetic-active")
            .send()
            .await
            .expect("revoked request");
        assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(revoked.headers()["cache-control"], "private, no-store");
        assert!(auth
            .delete_user_api_key("user", "active-key")
            .await
            .expect("delete"));
        let deleted = client
            .get(&endpoint)
            .bearer_auth("synthetic-active")
            .send()
            .await
            .expect("deleted request");
        assert_eq!(deleted.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(deleted.headers()["cache-control"], "private, no-store");
        handle.abort();
    });
}

#[test]
fn gateway_opencode_counts_distinct_public_models_not_candidate_keys() {
    run_frontdoor_async_test("opencode-dedup", async {
        let mut snapshot = unrestricted_models_snapshot("key", "user");
        snapshot.api_key_allowed_api_formats = Some(vec!["openai:chat".into()]);
        let auth = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![(
            Some(hash_api_key("synthetic-key")),
            snapshot,
        )]));
        let formats = [
            "openai:chat",
            "openai:responses",
            "claude:messages",
            "gemini:generate_content",
        ];
        let rows = (0..600).map(|index| {
            let mut row = sample_models_candidate_row(
                "provider",
                "custom",
                formats[index % formats.len()],
                "public-alias",
                10,
            );
            row.key_id = format!("upstream-key-{index}");
            row
        });
        let data = GatewayDataState::with_minimal_candidate_selection_and_auth_for_tests(
            Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            auth,
        );
        let gateway = build_router_with_state(
            AppState::new()
                .expect("state")
                .with_data_state_for_tests(data),
        );
        let (url, handle) = start_server(gateway).await;
        let response = reqwest::Client::new()
            .get(format!("{url}/api/opencode/config"))
            .bearer_auth("synthetic-key")
            .send()
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let payload: serde_json::Value = response.json().await.expect("json");
        assert_eq!(
            payload["provider"]["aether"]["models"],
            json!({"public-alias": {"id": "public-alias", "name": "public-alias"}})
        );
        handle.abort();
    });
}

#[test]
fn gateway_opencode_filters_protocol_policy_and_dynamic_public_names() {
    run_frontdoor_async_test("opencode-safe-config", async {
        let mut denied = unrestricted_models_snapshot("denied-key", "user");
        denied.user_allowed_api_formats = Some(vec!["claude:messages".into()]);
        denied.api_key_allowed_api_formats = Some(vec!["openai:chat".into()]);
        let mut responses_only = unrestricted_models_snapshot("responses-key", "user");
        responses_only.api_key_allowed_api_formats = Some(vec!["openai:responses".into()]);
        let auth = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![
            (Some(hash_api_key("synthetic-denied")), denied),
            (Some(hash_api_key("synthetic-responses")), responses_only),
            (
                Some(hash_api_key("synthetic-safe")),
                unrestricted_models_snapshot("safe-key", "user"),
            ),
        ]));
        let names = [
            "safe-alias",
            "{env:SECRET}",
            "public-{file:/etc/passwd}",
            "${HOME}",
            "bad-display",
        ];
        let rows = names
            .iter()
            .map(|name| sample_models_candidate_row("provider", "custom", "openai:chat", name, 10));
        let metadata = sample_public_global_model(
            "global-bad-display",
            "bad-display",
            "Read {file:/etc/passwd}",
            true,
        );
        let mut metadata_models = names
            .iter()
            .filter(|name| **name != "bad-display")
            .map(|name| {
                let mut model = sample_public_global_model(
                    &format!("global-{name}"),
                    name,
                    if *name == "safe-alias" {
                        "safe-alias"
                    } else {
                        "Unsafe Alias"
                    },
                    true,
                );
                model.config = None;
                model.supported_capabilities = None;
                model
            })
            .collect::<Vec<_>>();
        metadata_models.push(metadata);
        let data = GatewayDataState::with_minimal_candidate_selection_and_auth_for_tests(
            Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            auth,
        )
        .with_global_model_reader(Arc::new(InMemoryGlobalModelReadRepository::seed(
            metadata_models,
        )));
        let gateway = build_router_with_state(
            AppState::new()
                .expect("state")
                .with_data_state_for_tests(data),
        );
        let (url, handle) = start_server(gateway).await;
        let client = reqwest::Client::new();
        for (token, expected) in [
            ("synthetic-denied", json!({})),
            ("synthetic-responses", json!({})),
            (
                "synthetic-safe",
                json!({"safe-alias": {"id": "safe-alias", "name": "safe-alias"}}),
            ),
        ] {
            let response = client
                .get(format!("{url}/api/opencode/config"))
                .bearer_auth(token)
                .send()
                .await
                .expect("request");
            assert_eq!(response.status(), StatusCode::OK);
            let payload: serde_json::Value = response.json().await.expect("json");
            assert_eq!(payload["provider"]["aether"]["models"], expected);
        }
        handle.abort();
    });
}

#[test]
fn gateway_opencode_fails_closed_without_catalog_reader() {
    run_frontdoor_async_test("opencode-unavailable", async {
        let auth = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![(
            Some(hash_api_key("synthetic-key")),
            unrestricted_models_snapshot("key", "user"),
        )]));
        let data = GatewayDataState::with_auth_api_key_reader_for_tests(auth);
        let gateway = build_router_with_state(
            AppState::new()
                .expect("state")
                .with_data_state_for_tests(data),
        );
        let (url, handle) = start_server(gateway).await;
        let response = reqwest::Client::new()
            .get(format!("{url}/api/opencode/config"))
            .bearer_auth("synthetic-key")
            .send()
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()["cache-control"], "private, no-store");
        let payload: serde_json::Value = response.json().await.expect("json");
        assert_eq!(payload["error"]["code"], "catalog_unavailable");
        assert!(payload.get("provider").is_none());
        handle.abort();
    });
}
