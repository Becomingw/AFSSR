# OpenAI Responses WebSocket probe

`aether-openai-responses-ws-probe` verifies the official OpenAI Responses
WebSocket protocol using standard API-key Bearer authentication. It sends two
sequential `response.create` warmups on one socket, chaining the second from
the first response ID with `previous_response_id`.

It shares its protocol-driving core with the Codex probe, but it does **not**
send Codex account headers or require Codex quota events. This makes it the
compatibility gate for Arca's standard Responses WebSocket adapter, rather
than a replacement for the Codex probe.

## Prerequisites

Use a dedicated API project and a model that your key can access. Keep values
only in your process environment or secret manager:

```bash
export AETHER_OPENAI_WS_PROBE_API_KEY='your-api-key'
export AETHER_OPENAI_WS_PROBE_MODEL='your-openai-model'
```

The default endpoint is the official Responses WebSocket endpoint:

```text
wss://api.openai.com/v1/responses
```

To test a compatible endpoint explicitly, set
`AETHER_OPENAI_WS_PROBE_URL` or pass `--url`. The endpoint must use `ws://` or
`wss://` and may not contain credentials, a query string, or a fragment. The
API key has no command-line flag and is never printed.

## Run

```bash
cargo run -p aether-gateway --bin aether-openai-responses-ws-probe
```

For an explicit endpoint and timeout:

```bash
cargo run -p aether-gateway --bin aether-openai-responses-ws-probe -- \
  --url 'wss://api.openai.com/v1/responses' \
  --timeout-secs 30
```

The probe uses `generate:false`, so the warmups prepare continuation state but
do not request model output. A successful JSON report contains
`"continuation_confirmed":true`; header and event arrays contain names only,
never credentials, response IDs, request bodies, or response bodies.

## Interpretation

Success establishes that this key, model, and endpoint support the Responses
WebSocket handshake plus an in-socket continuation. It does not establish
support for every model, tool, service tier, proxy path, or Arca provider
configuration. Treat a successful direct probe as a prerequisite before
enabling **Responses WebSocket mode** for the matching Arca provider.

For protocol details, see the official [WebSocket Mode guide](https://developers.openai.com/api/docs/guides/websocket-mode).
