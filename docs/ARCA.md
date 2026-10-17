# Arca Source Snapshot

Arca (previously published as AFSSR) is a downstream fork of https://github.com/fawney19/Aether.
Maintainer of this fork and its modifications: Mengfei Wang (Becomingw).
Original Aether contributors retain their copyright. The original LICENSE is
unchanged; this publication does not grant commercial-use permission.

## Source Versions

- Upstream base: `361952ada9e3d01ac180846cee497cd1f55ad1c3`.
- Tag `haiku-compat6-20261013`: the source modifications used for the deployed
  `0.7.19-haiku-compat6` gateway, without local-only development drafts.
- `main`: the tagged compatibility implementation plus local GLM foreign-
  reasoning handling, associated tests, synthetic examples and rule templates.
  That additional reasoning helper has not been deployed to production.

The deployed implementation includes direct Responses-to-Claude conversion,
custom and namespaced tools, client-side tool search, Lite tool envelopes,
streaming tool/result continuation, scoped response history, Haiku adaptive
thinking, automatic OpenCode Go session headers, and native Claude text web
search with sources, citations and preserved search-result context.

Mixed text/image web-search declarations use Claude's native text capability.
Image-only search, arbitrary hosted tools and cross-provider opaque reasoning
are not universally supported. Custom grammar descriptions are not native
grammar-constrained decoding.

## Verification

Local internal-gateway development (2026-10-15) adds user/group model allocations,
rolling/calendar request caps, shared monthly cost budgets and commercial UI/API
retirement. It is not deployed on HK or Korean staging. Read the
[internal gateway runbook](operations/internal-gateway.md) before enabling it;
unallocated users fail closed. This development status is distinct from the
historical production/source snapshots below.

Local development now includes a dedicated `opencode_go` API-key provider with
native model protocol routing, current/legacy Go headers and reversible Claude
root-combinator tool schemas. See [the Go runbook](operations/opencode-go.md).
These changes are not part of the production `haiku-compat6-20261013` tag and
have not yet passed live acceptance on the separate Korean staging server.

The deployed snapshot passed 906 format-library tests and 23 Haiku mapping
tests. Eight public API scenarios made 18 successful inference requests,
including real web search -> client tool -> result -> final answer.
These were synthetic tests; they are not a guarantee for every Codex session.

Run the additional source tests before deploying `main`:

```sh
cargo test --locked -p aether-ai-formats --lib --tests
cargo clippy --locked -p aether-ai-formats --all-targets -- -D warnings
cargo fmt --all --check
```

## Build Your Own Image

Arca does not yet publish an image registry or release-update endpoint.
Do not install an upstream Aether image expecting Arca changes.
The inherited local-source Dockerfile builds the frontend and backend:

```sh
docker build --build-arg AETHER_BUILD_VERSION=arca-dev \
  -f Dockerfile.app.local -t arca:local .
```

Choose that image explicitly in your own Compose configuration. Generate new
installation secrets locally using the original setup instructions. Do not
replace secrets when upgrading an existing installation. This publication does
not rebuild or change the running production service.

Upstream publishing workflows must be reviewed and retargeted before use here.
The initial upload does not configure Arca releases or automatic deployment.

## Compatibility Templates

`deploy/compat/` contains historical development examples, not a production
installer. Some packaging examples refer to earlier compatibility iterations.
SQL examples use the fixture endpoint UUID
`00000000-0000-4000-8000-000000000001`; substitute a deliberately selected
endpoint only after inspecting the current schema and expected old rule value.
Prefer the supported management API over database writes. Preserve CAS checks,
take a protected backup, and never run these SQL examples blindly.

Production endpoint snapshots, database data, real environment files,
credentials, user histories, binaries and container archives are not included.

## Agent MCP

The embedded administrative MCP endpoint is `/mcp`; the console entry is
`/admin/mcp`. It uses user-bound management API keys with live role/scope checks,
default read-only issuance and preview/confirmation for writes. Public deployment
requires an explicit Host allowlist. See [Agent MCP operations](operations/agent-mcp.md)
for coverage, authentication, proxy configuration, limits and verification.

## User Feedback

User feedback is an issue-style workflow with private ownership, shared administrator
replies, resolve/close/reopen actions and an append-only history. MCP exposes the
same administrative workflow, with feedback-only key scopes, incremental polling,
version checks and idempotent commands. See [Feedback operations](operations/feedback.md).

## Usage Leaderboard

Model popularity, user volume rankings, estimated activity, hourly leaders and
daily early birds are available in the administration console and as a masked
user view. The MCP exposes the administrator summary through `admin:stats:read`.
See [Leaderboard operations](operations/leaderboard.md) for metric definitions,
privacy boundaries and retention limitations.
