# OpenCode Go Provider

This Arca provider uses API keys, not OpenCode account OAuth. It reuses Arca's
standard conversion, proxy/tunnel selection, metering, and response-history paths.
It does not change existing `custom` providers or production database records.

## Setup (key only)

1. Create a provider of type `opencode_go`. Nothing else is configured by hand: the four
   endpoints (`openai:responses`, `openai:chat`, `claude:messages`,
   `gemini:generate_content`), the base URL and format conversion are fixed by the
   preset. The provider drawer shows them read-only, hides the conversion switches and
   the key form has no API-format picker.
2. Add the Go API key. The server always gives a Go key every protocol (whatever the
   request says), and right after the key is saved the console lists the key's models
   and registers every one of them (global model + provider model), so Codex and
   Claude Code can use any Go model by its own name.
3. Each request is sent on the model's native protocol: the gateway picks Responses,
   Chat, Messages or Google by model, converting whatever the client speaks.
   Downstream aliases still work through an explicit model mapping; the alias is not
   sent upstream.
4. Select the existing proxy/tunnel option if needed. The adapter does not force
   all requests through tunnels or modify the system proxy.

Default base URL: `https://opencode.ai/inference/go/v1`.
Legacy `https://opencode.ai/zen/go/v1` endpoints remain supported. Existing custom
Go providers retain automatic session-header support for either official URL,
but automatic model protocol filtering requires the dedicated provider type.
Provider type cannot be changed in the normal editor: add a new disabled provider
for staging rather than rewriting the production provider in SQL.

| Native protocol | Examples | Suffix |
| --- | --- | --- |
| Responses | GPT Luna, Grok, Muse Spark | `/responses` |
| Chat Completions | GLM, Kimi, DeepSeek, MiMo, Space Bunny | `/chat/completions` |
| Messages | Claude Haiku, MiniMax, Qwen | `/messages` |
| Google GenerateContent | Gemini 4 Argon Contributor | `/models/{model}:generateContent` (stream: `:streamGenerateContent?alt=sse`) |

The protocol table is a dated snapshot, not an availability guarantee. The catalog
is fetched live. A model that is not in the table follows the same family rule
OpenCode's own client uses: `gpt-*`, `grok-*`, `muse-*` are Responses, `claude-*`,
`minimax-*`, `qwen*` are Messages, `gemini-*` is Google, everything else is Chat. A
wrong guess is a deterministic 400 for that one model ("Model does not support this
protocol"), never a wrong answer; add the model to the table once its protocol is
confirmed. An explicit `api_formats` or `endpoint_ids` model mapping still wins. Do
not use the `opencode-go/` client prefix as the upstream model ID. Go
compact/image/embedding endpoints are not automatically added or claimed to work.

### What is not forwarded

Go backends validate strictly, so a request is never sent with something the target
cannot execute. The client's own tools (shell, patch, plan, ...) are always kept.

- **Hosted web search** (Codex `web_search`): never forwarded to a Go model. Chat has no
  such protocol, Messages (Anthropic's `web_search_20250305` is an unknown tag there) and
  Gemini 4 (cannot mix a hosted tool with function declarations) cannot run it, only xAI
  documents one on Responses and accepting the declaration is not proof a search runs,
  and a hosted tool cannot be priced under a hard cost limit (every request of a
  cost-limited account would be refused). It is dropped together with a forced web
  `tool_choice` and replayed `web_search_call` items, instead of refusing the request.
- **Claude Code extensions on Messages**: `context_management`, `builtin_tools`,
  `container`, `mcp_servers`, `betas` and any non-`custom` tool type are removed.
- **Claude Code's `WebSearch`/`WebFetch`** are plain client functions. On Gemini they
  are not turned into Google's built-in search/URL tools.

## Account settings the gateway cannot change

Some Go models are refused by the service itself until the workspace that owns the key allows
them (Console, Privacy settings). The gateway reports the service's own text:

- "This Go model requires Global regions. Select Global in your workspace's Privacy settings"
  (seen on DeepSeek, GPT Luna);
- "This Go model trains on request data. Allow paid endpoints that train on request data ..."
  (the `*-contributor` models: Muse Spark, Gemini 4 Argon).

With several keys, a request that one workspace refuses fails over to a key whose workspace
allows it, so enable the setting on every key you want to serve those models. Per-model monthly
limits answer "Go usage limit exceeded" (HTTP 429) and fail over the same way.

## Headers and Authentication

- Chat, Responses and Messages inference use `Authorization: Bearer <provider key>`.
- Google GenerateContent uses `x-goog-api-key`, never a credential URL query.
- Legacy Messages uses `x-api-key`; legacy Chat/Responses and catalog GETs use
  Bearer. Downstream credentials are never substituted for provider credentials.
- Preserve an explicitly provided `x-opencode-session`. Otherwise derive it
  from a client session header or `prompt_cache_key`, hashed with the downstream
  credential scope. The hash format stays compatible with the previous adapter.
- Without conversation identity, use a per-request nonce rather than merging
  unrelated conversations. Clients should provide stable session identity.
- Preserve a non-generic client User-Agent, otherwise identify the gateway as
  `Arca-Go-Adapter/1.0`. Never pretend to be the OpenCode client.
- Explicit administrator header rules run after the adapter. Existing protected
  auth headers and hop-by-hop filtering still apply.

## Codex Protocol Bridges

The existing Responses converter preserves custom tools, namespaces, tool search,
tool outputs, signed thinking carriers, SSE, and stored response continuation.
For a function schema containing root `oneOf`, `anyOf`, or `allOf`, the converter
places the complete schema under a required `aether_input` object. This preserves
constraints rather than flattening unions or deleting tools.

The gateway unwraps provider arguments before returning them to the client and
rewraps tool calls when replaying history. Streaming wrapper JSON is buffered and
validated before emitting original function arguments; tool search restores its
object arguments. Ordinary tools remain unchanged. This correction applies to
the existing Responses-to-Messages adapter, not solely to Go.

Local JSON pointer `$ref` values are relocated to the nested schema. Scoped IDs,
anchors, external references and dynamic/recursive references are rejected with
an explicit conversion error when wrapping is required. Grammar custom tools
still rely on model adherence, not a constrained grammar decoder. Claude native
text web search is not a guarantee that every Go Messages model supports it.
Dedicated Go Chat endpoints now use the same bidirectional client-tool bridge:
Lite prefix IDs, namespaces, custom/Lark text payloads, client tool search and
loaded tools are represented as function calls and restored in sync/SSE results.
The model must still follow the supplied grammar; functionization is not a grammar
decoder. Native web search has no Chat equivalent; the hosted declaration is dropped (see
"What is not forwarded") and the client's own tools stay usable. Ordinary non-Go Chat providers are unaffected.
Most Go Chat profiles retain root union schemas natively. Native MiMo comparison
tests show that an unnecessary wrapper produces invalid nested values while the
original schema works. The verified `deepseek-flash`, `deepseek-v4-flash`,
`deepseek-v4.1-flash` and `deepseek-v4-pro` profiles instead require an object root:
their complete root combinators use the reversible `aether_input` carrier, retaining
constraints and replay identities. All twelve native model/combinator probes
passed after the operator enabled Global regions. Other Chat profiles are not
changed by this exception. Messages keeps its existing protocol-specific profiles.

Runtime `reasoning.context=all_turns` consumes the replay-policy selector only
after expanding the scoped transcript. Signed Messages thinking remains in
history. Other replay policies are rejected if they cannot be represented.
System/developer message content is lifted into Messages system instructions,
not discarded. JSON Schema output maps to `output_config.format`; enabled
thinking budgets stay below the output token limit and `none` maps to disabled
thinking. Actual model support for these controls must still be tested.

On Go Chat, verbosity is expressed as a final-answer instruction, not an unknown
wire parameter. This does not suppress tool execution. Go native Responses
flattens namespaces to stable `namespace__name` aliases and restores namespace
identity on output items and terminal snapshots. Alias collisions fail closed.
Qwen 3.8 disallows simultaneous `thinking` and `output_config.effort`: explicit
budgets/disabled thinking use `thinking`, otherwise effort uses `output_config`.
Native field isolation confirmed both controls individually work while their
combination returns 400; Qwen 3.7 and MiniMax do not share this restriction.

Grok 4.6/4.7 and Muse Spark 1.2/1.3 native Responses accept function tools but
reject custom tools. Their dedicated profiles use the bidirectional client-tool bridge
on the native Responses wire protocol, not a different upstream endpoint. Custom
inputs, namespace identity, client tool search and wrapped schemas are restored
in sync/SSE responses. Raw native reasoning items and encrypted content are
preserved during tool-event rewriting; sequence numbers remain monotonic.
GPT Luna retains native custom tools. Native clients should supply current tool
definitions when continuing a stored response so client identity can be restored.

Go Qwen3.7 Max and MiniMax M2.5 retain original root union schemas on Messages.
Native probes verified all three operators with exact arguments; an unnecessary
Claude-specific wrapper confused Qwen's returned arguments. Haiku and other
unverified Messages profiles keep their existing wrapper.

Go Gemini uses the existing Google converter with the bidirectional client-tool
bridge. Lite definitions are lifted before scoped history expansion; system and
developer text are moved to systemInstruction instead of being dropped. Verbosity
is an instruction preference. Function definitions use parametersJsonSchema to
preserve full nested schemas, including the required root-union object carrier.
Opaque Gemini tool thought signatures survive synchronous and SSE round trips.
A hosted web declaration alone is retained; unmapped controls or a forced native web
tool choice without any client tool still fail explicitly. When client tools are
present the hosted declaration is dropped, because Gemini 4 cannot mix the two.

For the explicitly advertised Codex JavaScript `functions.exec` contract, Go's
functionized protocols additionally expose JSON `exec_command` and `apply_patch`
projections. These calls are restored as JavaScript invocations of client tools;
the gateway never executes a command or applies a patch. Shell projection requires
the client's helper declaration. Deferred patch execution checks the client's
`ALL_TOOLS` catalog before invoking the exact `apply_patch` helper. Missing helpers
remain client errors, not guessed command execution. Existing tools are preserved,
explicit native names are never shadowed, and normal client approvals still apply.
An exact-match versioned marker allows structured replay of generated invocations;
arbitrary JavaScript and modified markers remain original custom tool history.
The patch helper documents Codex Add/Update/Delete/Move operations and examples;
unified-diff headers and line-number hunks are not valid Codex patch syntax.
When the client explicitly advertises `ALL_TOOLS`, `list_client_tools` and
`call_client_tool` additionally permit bounded discovery and JSON/string tool
arguments without model-generated JavaScript. Calls require an exact match in
the client's tool catalog; missing catalogs, unknown names and prototype names
fail closed. They execute only through client tools with normal approvals, never
on the gateway. Their versioned history markers preserve the exact original
arguments. No tools are deleted to force a model to choose these projections.
Malformed JSON in a rejected projected call is not sent back as malformed native
history. Its exact original text is carried under `_afssr_rejected_arguments` in a
valid JSON object, alongside the client's existing error result. This is historical
data for a call that was never executed, not a new tool invocation or a repaired
command. Valid JSON arguments and normal successful calls retain their original
representation. Native LongCat controls confirmed malformed history fails before
generation, while this carrier permits a subsequent valid patch call.
Arguments are parsed as JSON rather than interpolated as object literals, so
special property names remain data. Integers outside JavaScript's safe range are
rejected rather than silently rounded.
This bridge does not change non-Go providers or GPT's native custom-tool profile.
MiMo 2.5/2.6 may return the original Code Mode freeform input directly inside a
typed function call's argument field instead of its JSON carrier. For these exact
Go models and an explicitly advertised Codex Code Mode custom tool, the bridge
preserves that opaque input in sync/SSE. Other custom tools remain strict. No code
is extracted from assistant text, guessed, or executed by the gateway. An empty
JavaScript block is still a no-op, not proof of successful model tool use.

Grok's native web-search *output* items (statuses, sources) are still understood when a
provider returns them, but the hosted tool is no longer requested (see "What is not
forwarded").

Native Muse 1.3 probes accept root `oneOf`, `anyOf` and `allOf` tool schemas without
an object carrier. Its dedicated profile preserves those schemas instead of adding
a Claude-style wrapper; tool identities, history and custom-tool functionization
remain intact. Muse 1.2 and Grok retain their separately validated profiles.

The tested DeepSeek Go upstream rejects strict JSON Schema response output. This
is distinct from tool parameter schemas; no silent downgrade to ordinary JSON is
implemented. MiMo 2.5 empty/repeated Code Mode calls and incomplete workflows are
not classified as successful compatibility merely because the stream is valid.

Upstream 400/422/429 failures are no longer presented as a generic local-plan 503
when a real failed candidate has been recorded. Safe fixed client messages are
used rather than reflecting upstream bodies; provider 401/403 does not become a
false downstream authentication error. Runtime conversion diagnostics use the
planner's scoped history/policy instead of blaming unrelated pure-only fields.

Sync history is saved after restoring client tool identities, avoiding double
schema wrappers during `previous_response_id` replay. Go Chat `store=true` uses
scoped gateway history, not Chat's distinct persistence/training flag. Credential
headers are redacted at the shared usage persistence boundary; the outgoing
authentication headers remain unchanged.

## Acceptance and Isolation

Run the protocol, transport, scheduler and catalog tests:

```sh
cargo test --locked -p aether-ai-formats -p aether-provider-transport \
  -p aether-scheduler-core -p aether-model-fetch
cargo check --locked -p aether-gateway
```

Use the separate Korean staging deployment for live acceptance. Do not mount HK
production volumes or reuse its database, Redis or encryption secrets. A catalog
GET or HTTP 200 does not prove tool execution: test a synthetic call, return a
synthetic result, then verify continuation and completion, including streaming.
Live credentials, inference costs and deployment are separate authorization
scopes. This source change performs no production migration or deployment.

## Sources (Checked 2026-10-14, Asia/Taipei)

- [Official Go documentation](https://opencode.ai/v2/docs/console/go/)
- [Live Go catalog](https://opencode.ai/inference/go/v1/models)
- [Official inference authentication](https://opencode.ai/v2/docs/console/api/inference/)
- [Legacy Go forwarding implementation](https://github.com/anomalyco/opencode/blob/23e3493ab2efb6b8c00725cfd652b7f946af5343/packages/console/app/src/lib/inference-proxy.ts)
- [xAI native web search](https://docs.x.ai/developers/tools/web-search)
