# AFSSR Source Snapshot

AFSSR is a downstream fork of https://github.com/fawney19/Aether.
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

AFSSR does not yet publish an image registry or release-update endpoint.
Do not install an upstream Aether image expecting AFSSR changes.
The inherited local-source Dockerfile builds the frontend and backend:

```sh
docker build --build-arg AETHER_BUILD_VERSION=afssr-dev \
  -f Dockerfile.app.local -t afssr:local .
```

Choose that image explicitly in your own Compose configuration. Generate new
installation secrets locally using the original setup instructions. Do not
replace secrets when upgrading an existing installation. This publication does
not rebuild or change the running production service.

Upstream publishing workflows must be reviewed and retargeted before use here.
The initial upload does not configure AFSSR releases or automatic deployment.

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
