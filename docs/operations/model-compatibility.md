# Model Compatibility Workbench (Batch 1)

## Evidence Boundary

`/admin/compatibility` records **offline synthetic protocol evidence**, not model
quality, provider availability, real client compatibility or HTTP connectivity.
No provider key lookup, HTTP inference, client process, endpoint URL or user prompt
is accepted by this module. Client/version and model names are operator-supplied
labels. An HTTP 200 is only an API operation success, never a compatibility pass.

Each immutable run records client (Codex / Claude Code / OpenCode / generic),
client_version, client_protocol, model, upstream_format, mapped_model, suite,
configuration fingerprint, timestamp and per-case statuses/reasons. It has no
aggregate real-model or real-client pass. `not_tested`, `unsupported` and unknown
states are never passes.

| Case | What Actually Runs |
| --- | --- |
| basic_request | Synthetic text request; converted model and text assertions |
| tool_definition | Function name and JSON schema preservation |
| tool_return | Tool-call ID, input and result linkage preservation |
| multi_turn_tools | Three synthetic call/result rounds, ordered content assertions |
| long_history | 64 alternating messages, each with 128 repeated synthetic words |
| image | Inline 1x1 PNG carrier conversion only; no image understanding |
| sync_response | Text and stop-reason conversion |
| tool_response | Function call ID, name, arguments and stop reason conversion |
| stream | Actual text SSE transform and terminal check, not just transcoder construction |
| cancel | `not_tested`: no live transport |
| compression | `not_tested`: no model/client compaction or context recovery |
| real_client | `not_tested`: no Codex, Claude Code or OpenCode executable launched |
| real_model | `not_tested`: no model request, key or spend |

The runner calls the existing `aether_ai_formats` `emit_request`,
`convert_request`, `parse_request`, `emit_response`, `convert_response`,
`parse_response`, `build_stream_transcoder` and `StreamingStandardFormatMatrix`.
Semantic assertions check content/tool/schema/terminal preservation, not just a
successful return value. Fixtures are generated from explicit canonical values,
not captured customer traffic. Format library rejections become `unsupported`
where classified as unsupported; other conversion or assertion failures become
`failed`, with fixed readable, non-payload reasons.

## Permissions And API

All four administrator roles may read. Only `service_admin` and `admin` (the
existing super-administrator role) may configure or run. `audit_admin` and
`user_admin` cannot run even this zero-cost suite. Ordinary users have no access.
No paid-test endpoint or mode is introduced. Existing provider model-test actions
remain a separate explicit operator workflow and are **not** imported as evidence.

| Method / Path | Scope | Input |
| --- | --- | --- |
| GET `/api/admin/compatibility` | `admin:compatibility:read` | No query parameters |
| POST `/api/admin/compatibility/profiles` | `admin:compatibility:write` | `{expected_revision, profile:{id:UUID,config}}` |
| POST `/api/admin/compatibility/runs` | `admin:compatibility:write` | `{expected_revision,profile_id:UUID,expected_fingerprint}` |

`config` contains exactly `client`, `client_version`, `client_protocol`, `model`,
`upstream_format`, `mapped_model`. Protocols are `openai:responses`, `openai:chat`,
`claude:messages`; Codex is restricted to Responses and Claude Code to Messages.
Labels are bounded identifiers; unknown fields, URLs and obvious bearer/key
labels are rejected. Do not put secrets into labels. Bodies are limited to 8 KiB.

Read then submit the exact returned revision/fingerprint. HTTP 409 means refresh
before retrying. The client does not silently retry. Failed persistence never
reports a saved result. Responses are `Cache-Control: no-store`.

MCP exposes the following exact allowlisted operations, using the same live
role/token checks and local dispatch as the admin API. There is no generic URL,
SQL, transport or paid-test execution:

| MCP Tool | Impact / Scope | Confirmation |
| --- | --- | --- |
| `afssr_compatibility_list` | Read / `admin:compatibility:read`; four administrator roles | No body, dry_run or confirmation accepted |
| `afssr_compatibility_profile_save` | Change / `admin:compatibility:write`; service/super only | Required body matches profile API above; default dry_run previews only |
| `afssr_compatibility_run` | Change / `admin:compatibility:write`; service/super only | Required body matches run API above; default dry_run previews only |

For both Change tools, call with `body` first, then submit the exact same body
with `dry_run:false` and the returned `confirmation`. Existing tickets are
single-use, expire after 300 seconds and bind the tool, exact arguments, live
role/user/key and permissions. A ticket is not human approval. Unknown fields,
nonempty queries, bodies over 8 KiB, malformed IDs/fingerprints, unsafe labels and
out-of-range revisions are rejected **before** preview/ticket creation. Strict
nested JSON schemas and the typed compatibility API both remain authoritative.
Read-only tokens cannot obtain or execute write previews; audit/user_admin remain
unable to write even with an asserted write scope. Revision/fingerprint/CAS
checks still run at execution time; 409 requires rereading, not blind retrying.

## Persistence, Retention And Staleness

Uses the existing PostgreSQL `system_configs` table at
`compatibility_workbench_v1`, with a versioned, strictly validated JSON snapshot.
Maximum 64 profiles and 200 immutable runs; running at capacity drops the oldest
run. There is deliberately **no production in-memory fallback**. No new schema
or migration is required. Generic system-config writes to this reserved key are
blocked; only the controlled workbench can write it. System backup/purge remains
under existing super-administrator controls; this is not an append-only audit log.

Writes compare the entire previously read JSON in an atomic SQL update. Initial
creation uses `INSERT ... ON CONFLICT DO NOTHING`; a lost race is a conflict, not
an overwrite. Revision checks also cover profile edits and test runs. No payload,
image bytes, upstream response or converter exception text is saved. Fixed result
reason strings are validated on read and before save. Invalid schema, status or
stored content fails closed without overwriting the snapshot.

Fingerprints are SHA-256 of the typed offline configuration, suite identifier
and a build-time digest of the conversion crate/workbench source and Cargo.lock.
Dirty source changes are included, not just Git HEAD. Editing any profile field
or changing the compiled converter/suite makes old runs stale. The API computes
staleness on every read; the UI shows `已过期` and offers freshness filtering.

**Fingerprint scope is deliberately `offline_profile_and_converter_source_only`.**
This batch does not select a deployed provider/endpoint/key and does not read its
configuration. It therefore never asserts freshness or passing status for actual
provider routing, credentials, transforms, timeouts, quotas or production model
capabilities. A future live-evidence feature must snapshot the relevant sanitized
deployment configuration itself, with server-computed fingerprints and explicit
authorization/spend controls. It must not reuse this offline fingerprint as live
evidence.

## Local Verification

From the repository root:

```sh
cargo check -p aether-gateway --lib
cargo test -p aether-gateway --lib api::compatibility::tests -- --nocapture
cargo test -p aether-gateway --lib mcp_compatibility_is_readonly
cd frontend
npm run test:run -- src/api/__tests__/compatibility.spec.ts src/views/admin/__tests__/Compatibility.spec.ts src/config/__tests__/adminRoles.spec.ts src/layouts/main-layout/__tests__/navigation.spec.ts
npm run type-check
```

The ignored PostgreSQL test requires an **empty disposable local** database named
`afssr_compatibility_test` and `AFSSR_COMPAT_TEST_DATABASE_URL`. It refuses other
hosts/database names or a pre-existing snapshot. It never uses `DATABASE_URL` or
repository `.env` and creates only a minimal local `system_configs` table:

```sh
AFSSR_COMPAT_TEST_DATABASE_URL=postgres://compat_test@127.0.0.1:55439/afssr_compatibility_test \
  cargo test -p aether-gateway --lib postgres_compatibility_roundtrip_restart_stale_and_atomic_cas -- --ignored --nocapture
```

This tests first-insert races, real conversion plus storage, reading via a new
gateway data state, stale revision/fingerprint handling, concurrent SQL CAS,
unsupported schema refusal and absence of fixture bodies/images in saved JSON.
Do not point it at any shared or production database.

Remaining real acceptance: client-version-specific native request traces, provider
behavior and authentication, tool streaming, transport cancellation, previous
response/session recovery, compaction, long-context model limits, image reasoning
and billed model execution. None was executed or claimed by this batch.

## Batch 1 File Map

New files:

- `apps/aether-gateway/src/api/compatibility/mod.rs`: controlled API dispatch and snapshot handling.
- `apps/aether-gateway/src/api/compatibility/schema.rs`: strict configuration/result contracts and fingerprints.
- `apps/aether-gateway/src/api/compatibility/runner.rs`: actual offline conversion and SSE semantic assertions.
- `apps/aether-gateway/src/api/compatibility/tests.rs`: protocol matrix, schema, role, HTTP, persistence-refusal and opt-in PostgreSQL tests.
- `apps/aether-gateway/src/data/state/compatibility.rs`: persistent-only gateway adapter.
- `frontend/src/api/compatibility.ts`: typed API and fail-closed status summary.
- `frontend/src/api/__tests__/compatibility.spec.ts`: API and unknown-state tests.
- `frontend/src/views/admin/Compatibility.vue`: dense filtered workbench.
- `frontend/src/views/admin/__tests__/Compatibility.spec.ts`: UI role, conflict, stale and failure-state tests.
- `docs/operations/model-compatibility.md`: this operational boundary and verification guide.

Narrow edits to existing/shared files (other agents' changes retained):

- `apps/aether-gateway/Cargo.toml`: existing SHA-256 dependency also available to build script.
- `apps/aether-gateway/build.rs`: deterministic converter/source fingerprint input.
- `apps/aether-gateway/src/api/mod.rs`: module registration.
- `apps/aether-gateway/src/data/state/mod.rs`: persistent adapter registration.
- `apps/aether-gateway/src/control/route/admin.rs`: three exact compatibility endpoints.
- `apps/aether-gateway/src/control/management_token_permissions.rs`: dedicated compatibility scope/catalog keys.
- `apps/aether-gateway/src/roles.rs`: four-role reads; service/super writes only.
- `apps/aether-gateway/src/handlers/proxy/local.rs`: local dispatch, retaining preview and diagnostics branches.
- `apps/aether-gateway/src/api/mcp/catalog.rs`: read/list and two controlled Change tools, with strict bounded schemas.
- `apps/aether-gateway/src/api/mcp/execution.rs`: compatibility body validation before preview, preserving the shared confirmation mechanism.
- `apps/aether-gateway/src/api/mcp/tests.rs`: role/scope/readonly/body/confirmation tests.
- `apps/aether-gateway/src/handlers/admin/system/shared/configs.rs`: reserved-key generic-write refusal.
- `crates/aether-data/runtime/src/backend/maintenance.rs`: JSON CAS dispatch with no disabled-backend success.
- `crates/aether-data/runtime/src/backend/system/postgres.rs`: atomic initial insert / JSON snapshot CAS update.
- `frontend/src/config/adminRoles.ts`: workbench visible to four roles.
- `frontend/src/router/routes/admin.ts`: lazy workbench route.
- `frontend/src/layouts/main-layout/navigation.ts`: workbench navigation item.

Changes remain local. No production deployment or production migration was performed.

## Recorded Local Validation (2026-10-16)

- `cargo test -p aether-gateway --lib api::compatibility::tests -- --nocapture`: **6 passed, 0 failed, 1 ignored**. The nine protocol directions actually execute semantic conversion/SSE assertions; they are not mocked HTTP tests.
- Built test executable `target/debug/deps/aether_gateway-2a501f6e605d9007 mcp_compatibility_is_readonly --nocapture`: **1 passed**.
- Same executable with `mcp_catalog_routes_are_registered_and_names_unique --nocapture`: **1 passed**. These reused the existing binary to avoid adding concurrent Cargo builds.
- Frontend four-file command above: **17 passed**; final focused compatibility UI/API rerun: **7 passed**.
- `npm run type-check`: **passed**.
- Targeted ESLint for the four new frontend source/test files: **passed, no warnings**.
- Headless browser visual QA at 1440 and 390 px, including inline profile editing: no document horizontal overflow or page errors. All API traffic was synthetic/intercepted and external requests blocked. Screenshots are temporary `/tmp/afssr-compatibility-{1440,390}.png` and `/tmp/afssr-compatibility-edit-{1440,390}.png`, not live evidence.
- `git diff --check` on compatibility-owned files and targeted persistence edits: **passed**.
- Final unified API/MCP regression: **47 passed, 0 failed, 1 ignored**; the ignored PostgreSQL test was run separately as described below.
- PostgreSQL roundtrip/restart/stale-fingerprint/concurrent-CAS integration: **1 passed** on the final build. New-instance startup initially hit macOS shared-memory exhaustion; verification instead used an already initialized, dedicated loopback test server with a newly created empty `afssr_compatibility_test` database. The test refused pre-existing snapshots. Its temporary database was removed and the test server stopped afterward. No production database or kernel settings were touched.

### Controlled MCP Follow-Up

`afssr_compatibility_profile_save` and `afssr_compatibility_run` are included in the
final passing API/MCP regression. Their tests verify role/scope checks, strict
body validation before preview, exact ticket binding and single-use confirmation.
Read-only assertions explicitly allow these two Change operations but continue
to forbid bodies/confirmations on the Read tool. No real model, client or inference
transport was exercised. Temporary module Vite servers were stopped; the consolidated
synthetic frontend preview remains on 5188.
