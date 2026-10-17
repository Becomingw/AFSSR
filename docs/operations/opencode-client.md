# OpenCode Client Integration

This is the downstream OpenCode client integration. The upstream OpenCode Go
provider is a separate feature; see [opencode-go.md](opencode-go.md).

## Model Catalog

`GET /api/opencode/config` accepts the same Bearer API key as inference. It returns
an OpenCode `provider.aether` configuration without a credential. Its model IDs
are gateway aliases, filtered by the user's and API key's effective permissions
and the Chat Completions protocol used by `@ai-sdk/openai-compatible`.
The exact response, filtering and error contract is documented in
[opencode-config.md](opencode-config.md).

Only explicitly configured, publicly safe model metadata is exported: display
names, context/output limits, supported capabilities/modalities, and representable
token prices. Missing values remain unknown; aliases do not imply capabilities.
Backend provider URLs, upstream names, credentials and routing rules are not
client metadata. A configuration export does not bypass admission or quota checks.

The API Keys page's CC Switch dialog offers an **OpenCode configuration** download
after selecting OpenCode. The downloaded `opencode.json` contains the selected
client key and all models that key may use. Treat it as a credential, not a public
share link. The Linux installer merges this catalog into the user's existing
configuration; see [linux-client-setup.md](linux-client-setup.md).

OpenCode consumes the resulting native configuration, including model metadata.
This is not a claim that an unmodified OpenCode client automatically polls arbitrary
gateway model endpoints. Run the installed sync command after catalog changes.
Price display is an estimate; gateway metering remains authoritative. Tiered and
per-request billing cannot always be represented by OpenCode's flat token rates.

## CC Switch Compatibility

Verified against official **v4.0.13**, released October 15, 2026:

- [Release](https://github.com/farion1231/cc-switch/releases/tag/v4.0.13)
- [Official site-integration guide](https://github.com/farion1231/cc-switch/blob/v4.0.13/docs/guides/deeplink-site-integration-guide-zh.md)
- [Parser](https://github.com/farion1231/cc-switch/blob/v4.0.13/src-tauri/src/deeplink/parser.rs)
- [Provider import implementation](https://github.com/farion1231/cc-switch/blob/v4.0.13/src-tauri/src/deeplink/provider.rs)

Links use `ccswitch://v1/import`, `resource=provider`, UTF-8 Base64URL content
followed by URLSearchParams encoding, and explicit case-sensitive `apiFormat`:

| Target | Format |
| --- | --- |
| Claude Code | `anthropic` |
| Codex, Grok Build | `openai_responses` |
| Gemini CLI | `gemini_native` |
| OpenCode, OpenClaw, Hermes, Pi | `openai_chat` |

The format describes the client-to-gateway protocol, not the selected upstream's
protocol. The gateway remains responsible for supported conversions.

Claude's three model slots and Codex's provider-scoped TOML are included as inline
JSON configuration. Address, key and selected model also remain URL parameters.
Older CC Switch versions may ignore `apiFormat`; v4.0.10 or newer is recommended.
The official confirmation dialog controls activation; importing is not unattended
or deduplicated, and repeated imports can create duplicate provider cards.

CC Switch v4.0.13 **rebuilds OpenCode configuration with only the selected model**.
Its deep links do not preserve a complete `models` map, context limits, capabilities
or pricing from inline config. We deliberately do not attach metadata that it
would silently discard. Use the full OpenCode configuration download or installer
for the complete catalog. `configUrl` is not supported upstream and is not emitted.

Usage scripts are emitted only for HTTPS gateways because CC Switch requires a
same-origin HTTPS query. The script uses the provider key by default and a gateway
root `usageBaseUrl`, not the `/v1` inference base. No browser cookies are required.
Links are created on explicit user action, are not stored or rendered in page
markup, and must never be sent to analytics or logs. Base64 is not encryption.
