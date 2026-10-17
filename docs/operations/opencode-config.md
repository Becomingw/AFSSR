# OpenCode API-key config contract

`GET /api/opencode/config` accepts an Arca API key as `Authorization: Bearer <key>`.
It uses the same credential resolution, current key snapshot, user/group/key
effective policies, and candidate model filtering as `/v1/models`. A login session
or management token does not authorize this endpoint. It never returns a key.

The frontend and installer should consume this JSON directly, not CC Switch's
OpenCode deep-link payload, which may discard model metadata:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "aether": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "Arca",
      "options": { "baseURL": "https://gateway.example/v1" },
      "models": {
        "public-alias": { "id": "public-alias", "name": "Public Model" }
      }
    }
  }
}
```

- Fetch with the explicitly selected API key, without session axios interception
  or cookies (`credentials: 'omit'`). The selected model is authorized for this
  configuration only when its exact public alias is an own key of
  `provider.aether.models`. Do not fall back to a global/public model catalog.
- The SDK calls `openai:chat`. Candidate collection reuses the ordinary models
  directory's cross-format logic for Chat, Responses, Claude Messages and Gemini
  GenerateContent sources, including models the gateway serves via conversion.
  Image, embedding, rerank and compact-only sources are not exported. User and key
  protocol permissions must permit the client protocol `openai:chat`; otherwise a valid key receives
  `200` with `models: {}`. Provider/model permissions are also intersected.
  Source protocol permissions are not additionally required on the caller's key;
  source formats are used to filter upstream keys and scoped model mappings.
  Responses candidates do not trigger Codex-specific upstream catalog fetching.
  Standalone keys retain the gateway's existing standalone-policy semantics.
- Candidate endpoints/keys are filtered and deduplicated by public model name
  before the 512-model limit and per-model metadata lookup. Metadata comes only
  from the corresponding public global model, not provider/key overrides.
  If a configured public model reader no longer finds an active record, the
  model is omitted even if an older candidate snapshot still contains it.
- `id` and the map key are the public alias; `name` is its public display name
  when present. No source model names, mappings, provider IDs or secrets are
  returned. Names/displays containing `{env:`, `{file:` or `${` are excluded.
- Known global config booleans map `function_calling` to `tool_call`,
  `extended_thinking` to `reasoning`, and preserve `attachment`/`temperature`.
  Known supported-capability names can establish `true`, but absence is not
  treated as `false`. Explicit config booleans take precedence.
- Known `input_modalities`/`output_modalities` map to OpenCode `modalities`.
  Only `text`, `image`, `audio`, `video`, and `pdf` are accepted. Known vision
  support can establish text/image input; it does not imply attachment support.
- Positive integer `context_limit` and `output_limit` are exported together as
  `limit.context`/`limit.output`. Incomplete/invalid limits are omitted entirely.
  Safe known `family`/`release_date` strings are preserved.
- `cost` uses known global USD-per-million token prices only when a single
  unbounded tier supplies both input/output prices, without a nonzero per-request
  charge. Cache read/write prices are optional. Variable pricing is omitted,
  never flattened; missing values are never replaced with zero or infinity.
- `baseURL` reuses the existing public installation URL resolver:
  `ARCA_PUBLIC_BASE_URL`, then `PUBLIC_BASE_URL`, then trusted-proxy metadata.
  Untrusted request hosts do not set the returned URL. Configure the public base
  URL for external clients; the resolver's safe fallback is `http://localhost`.
- Missing/invalid/expired/locked/revoked keys and IP policy denial return `401`,
  following the existing models/IP handlers. Catalog read errors/timeouts/unavailable readers/size overflow
  return `503` with `error.code: "catalog_unavailable"`, not a successful empty
  replacement config. No upstream catalog fetch is performed.
- Every response through this route, including early auth failures, has
  `Cache-Control: private, no-store` and varies on `Authorization`.

Model properties follow the official
[OpenCode ProviderConfig schema](https://opencode.ai/config.json). Unknown
metadata is omitted. Runtime inference authorization remains authoritative if
permissions change after download.

## Local verification

```sh
cargo check -p aether-gateway --lib
cargo test -p aether-gateway --lib opencode -- --test-threads=1
RUST_MIN_STACK=33554432 cargo test -p aether-gateway --lib models -- --test-threads=1
```

Tests use synthetic credentials without printing request headers or keys. They
cover schema field projection, unknown/malformed metadata, safe names, per-key
and user restrictions, protocol denial, deduplication, session rejection,
expired/locked/deleted keys, IP denial and no-store behavior. No production
operations, frontend changes, or edits to `install.rs` are required.
Cross-format HTTP fixtures also cover chat-only callers seeing converted text
models, source-scoped mappings, non-conversation exclusion, provider/model policy
intersections and 600 candidate keys across multiple protocols deduplicating to
one public alias. Existing `/v1/models` query scope and Codex behavior are unchanged.
