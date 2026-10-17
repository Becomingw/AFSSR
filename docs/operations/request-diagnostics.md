# Request Diagnostics (Batch 1)

## Entry Points

- Ordinary users: **My Feedback** (`/dashboard/feedback`), enter a RequestID and explicitly read diagnostics. A request can be carried to a new feedback draft, not submitted automatically.
- Administrators: the request detail drawer, or **User Feedback** (`/admin/feedback`). The drawer can carry only the RequestID into the feedback page; select an existing issue and explicitly attach the summary to a reply.
- Download uses the same safe API projection, not the administrator's raw trace or HTTP capture.

## API And Permissions

- `GET /api/requests/{request_id}/diagnostics`: authenticated local session; reads only evidence whose usage owner and every candidate owner match the current user. Missing, unowned, conflicting-owner and foreign evidence produce the same `unknown` view. Inference keys do not grant access to this endpoint.
- `GET /api/admin/monitoring/diagnostics/{request_id}`: existing administrative role and management-token `admin:monitoring:read` checks, including permitted higher scope. It deliberately does not reuse the credential-bearing raw trace route's `admin` scope.
- Both reject query filters, validate bounded ASCII identifiers and return `Cache-Control: no-store`. Storage read errors return generic 503, never partial success or backend text.
- No new tables, capture streams, external services or LLM diagnosis are introduced. Only shallow usage audit and existing candidate evidence are read. No body blob is resolved and no provider catalog enrichment is necessary.

The v1 response contains `request_id`, `state`, `final_outcome`, `has_attempt_failures`, eight `stages` and a deterministic `summary`. Stage statuses are **known**, **failed**, or **unknown**. A known observation is not a successful stage gate: `owner_recorded` proves attribution, `conversion_configured` proves configuration, and `first_byte_recorded` proves only a recorded first byte. Evidence lists contain fixed source field names, not their values.

Final usage lifecycle takes precedence over stale candidates. A finalized, HTTP-2xx, error-free completed usage has `state=known`, `final_outcome=completed`, even after failed/skipped attempts. Those failures remain in stages and `has_attempt_failures`. Pending/streaming final usage has an unknown final result and `state=in_progress`. Without usage, only the latest started candidate can indicate active execution; earlier Pending candidates cannot override later attempts. Candidate errors alone do not establish a final request outcome, which remains `unknown`.

## Feedback And MCP

- User creation keeps the existing `POST /api/feedback` contract, adding optional `include_diagnostics: true`. A supplied `request_id` must belong to that user even when no summary is requested. An unavailable request yields 404. Without explicit opt-in, only the user's entered description is stored. With opt-in the server appends its own allowlisted summary.
- Existing user/admin feedback command endpoints accept `diagnostic_request_id` only for `action=reply`. Feedback visibility/write permission remains required. Administrative attachments additionally require monitoring read permission on the same principal/token. The server checks feedback visibility before reading diagnostics.
- Existing CAS `expected_version` and stable UUID `client_request_id` rules are unchanged. If evidence changes between uncertain retries, the regenerated summary can cause a conflict rather than overwrite or duplicate a mutation; reread the feedback before another explicit action.
- MCP exposes read-only `afssr_request_diagnostics` under monitoring read scope. `afssr_feedback_command` accepts `diagnostic_request_id` and retains its existing preview, exact-argument confirmation, feedback write and CAS protections. A feedback-write-only key cannot attach diagnostic evidence. Ordinary users continue to use their own-session HTTP API, not administrator MCP.

No history, raw prompts/responses, headers, upstream key names/IDs, tokens, target models, provider names, internal target URLs/IPs, raw errors or arbitrary metadata are returned in this safe projection. Unknown strings are not copied into diagnostic codes. Existing high-privilege raw trace access remains separate and unchanged.

## Evidence And Limitations

- Identity/model ACL/quota are not inferred from HTTP 401/403/429 or from a later successful dispatch. In particular upstream permission/rate-limit errors must not be called a local identity/entitlement failure. Local gates without persisted explicit evidence remain unknown; this batch does not add duplicate admission logging.
- Recognizes existing candidate conversion-build rejection, structured `failure_diagnostic.kind=request_conversion`, common API-format error types, proxy/transport errors and terminal/read/decode/delivery stream errors. A missing terminal event remains failure even with HTTP 200.
- A proxy transport error is not proof of a specific tunnel node outage. No node address, target URL or guessed failure cause is exposed.
- Older records, unrecognized errors, missing owners, purged records and non-final snapshots can remain unknown. The diagnosis is an evidence projection, not a claim that every gate was instrumented.
- Fixtures use the real stored-candidate/usage schema and existing executor error codes with synthetic identifiers and secret canaries, not production conversations. Tests cover common Chat/Responses/Claude/Gemini format-error shapes, stream interruption, missing/running evidence, success after fallback, stale candidates, ownership, admin/token scope, explicit feedback attachment and frontend stale-result cancellation.
- No inference replay, inference retry, automatic feedback creation, history forwarding or production issue creation occurs.

## Local Checks

```sh
cargo test -p aether-gateway --lib api::diagnostics
cargo test -p aether-gateway --lib api::feedback
cargo test -p aether-gateway --lib api::mcp
cd frontend
npm run test:run -- src/api/__tests__/diagnostics.spec.ts src/api/__tests__/feedback.spec.ts src/features/usage/components/__tests__/RequestDiagnosticPanel.spec.ts src/features/usage/components/__tests__/RequestDetailDrawer.pricing.spec.ts src/features/usage/components/__tests__/RequestDetailDrawer.body-capture.spec.ts src/views/shared/__tests__/Feedback.spec.ts
```
