<div align="center">

<img src="frontend/public/arca.svg" width="96" height="96" alt="Arca">

# Arca

**A self-hosted AI gateway — one entry point for Claude, OpenAI, Gemini and their CLI clients**

[![License: Non-Commercial](https://img.shields.io/badge/license-non--commercial-orange)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.95-b7410e?logo=rust)](rust-toolchain.toml)
[![Vue](https://img.shields.io/badge/vue-3-42b883?logo=vuedotjs&logoColor=white)](frontend/package.json)
[![PostgreSQL](https://img.shields.io/badge/postgresql-required-336791?logo=postgresql&logoColor=white)](docker-compose.yml)

[简体中文](README.md) · [English](README.en.md)

[Features](#features) · [Architecture](#architecture) · [Quick start](#quick-start) · [Documentation](#documentation) · [Contributing](#contributing) · [License](#license-and-acknowledgements)

</div>

---

## Overview

Arca is a self-hosted AI API gateway written in Rust, with a Vue admin console. It puts several upstream model providers behind one entry point and gives downstream users unified authentication, protocol conversion, intelligent scheduling, quotas and usage tracking.

- **Downstream**: clients keep using the OpenAI, Claude and Gemini protocols and their CLI tools unchanged.
- **Upstream**: official APIs, subscription account pools and third-party compatible services can be mixed; the gateway handles routing, retries and failover.
- **Operations**: a single Rust binary serves both the API and the static frontend. It needs PostgreSQL; Redis is optional.

> Arca is a downstream fork of [Aether](https://github.com/fawney19/Aether), maintained by [Becomingw](https://github.com/Becomingw). The original authors' copyright and attribution are preserved. See [License and acknowledgements](#license-and-acknowledgements).

## Features

| Area | Capabilities |
| --- | --- |
| **Unified access** | OpenAI Chat / Responses / Realtime, Claude Messages and Gemini `generateContent` entry points; Embeddings, Rerank, image and video tasks, Gemini Files |
| **Protocol conversion** | Two-way request and response conversion between OpenAI, Claude and Gemini, including streaming, tool calls and reasoning content; same-protocol requests pass through natively |
| **Scheduling** | Upstream key selection by priority, health and cache affinity; automatic retry and candidate failover; per-model restriction of allowed interface types and endpoints |
| **Account pools** | Pools for subscription-style upstreams that track quota and health and drop unavailable accounts |
| **Multi-tenancy and quotas** | Users, groups and API keys; model grants, rolling and calendar request caps, shared monthly cost budgets; wallets, billing plans and payment gateways |
| **Admin console** | Providers and endpoints, model catalog and mappings, routing profiles, users and keys, usage and cost analysis, audit logs, IP allow/deny lists, system settings; Chinese and English UI |
| **Observability** | Per-request usage and timing, request diagnostics, request body viewer, Prometheus metrics, cache and performance analysis |
| **Security** | Encrypted storage of sensitive data, PII redaction and restoration, body-size and concurrency budgets, outbound DNS/proxy policy, LDAP and OAuth sign-in |
| **Integrations** | Built-in admin MCP endpoint (user-bound admin keys, preview-and-confirm for writes); one-step Linux setup for Codex CLI / Claude Code; optional Tunnel proxy node |

## Architecture

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/architecture/architecture-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="docs/architecture/architecture-light.svg">
    <img src="docs/architecture/architecture-light.svg" width="680" alt="Arca architecture: client protocols are authenticated, scheduled and converted by the gateway before reaching upstreams">
  </picture>
</p>

A request passes through: authentication and admission → model resolution → candidate selection and ranking → protocol conversion (if needed) → upstream execution with failover → usage recording.

### Upstream types

Besides official APIs, the gateway ships fixed provider templates for `codex`, `claude_code`, `gemini_cli`, `antigravity`, `vertex_ai`, `kiro`, `chatgpt_web`, `grok`, `windsurf` and `opencode_go`, and supports any OpenAI-, Claude- or Gemini-compatible custom endpoint.

### Repository layout

```text
apps/
  aether-gateway/   gateway binary: HTTP/WebSocket entry, scheduling and execution, admin API, built-in MCP
  aether-tunnel/    Tunnel proxy node, deployed on an overseas VPS to relay outbound traffic
crates/             domain libraries: protocol formats, scheduling, data access, billing, usage, OAuth, runtime
frontend/           admin console (Vue 3 + TypeScript + Vite)
deploy/             Compose variants, acceptance scripts and compatibility templates
docs/               protocol notes, runbooks and audits
```

> Directory and crate names keep the `aether-*` prefix inherited from upstream. This does not affect usage; renaming would break existing deployments, so it is not done for now.

## Quick start

### Prerequisites

Docker and Docker Compose. The repository's Compose files start PostgreSQL and Redis for you.

### 1. Build the image

Arca does **not yet publish prebuilt images or release artifacts**, so build it yourself:

```bash
git clone https://github.com/Becomingw/Arca.git
cd Arca
docker build --build-arg AETHER_BUILD_VERSION=arca-dev \
  -f Dockerfile.app.local -t arca:local .
```

### 2. Configure

```bash
cp .env.example .env
chmod 600 .env          # .env holds database, JWT and encryption keys
./generate_keys.sh      # generates independent random JWT / encryption / Postgres / Redis secrets into .env
# Edit .env: set at least ADMIN_PASSWORD, and set APP_IMAGE=arca:local
```

### 3. Start

```bash
docker compose up -d
```

The service listens on port `8084` by default. Open `http://localhost:8084`, sign in with `ADMIN_USERNAME` / `ADMIN_PASSWORD` from `.env`, then add providers, models and API keys in the admin console.

When upgrading an existing deployment, do not replace the generated secrets. The database schema migrates automatically on start; run `make db-status` first if you want to inspect it.

### Local development

Requires Docker, Rust (version pinned in [`rust-toolchain.toml`](rust-toolchain.toml)), Node.js and make. Set `ADMIN_PASSWORD` in `.env` before the first start.

```bash
make dev            # backend + frontend Vite dev server; starts Postgres/Redis if missing
make dev-backend    # backend only
make dev-frontend   # frontend only
make db-status      # show database migration status
make db-prepare     # run migrations and backfills ahead of time
```

## Documentation

| Topic | Document |
| --- | --- |
| Configuration, environment variables, S3 backup restore | [docs/configuration.md](docs/configuration.md) (Chinese) |
| Arca release notes and building your own image | [docs/ARCA.md](docs/ARCA.md) |
| One-step Linux client setup | [docs/operations/linux-client-setup.md](docs/operations/linux-client-setup.md) |
| Internal gateway mode (user/group grants) | [docs/operations/internal-gateway.md](docs/operations/internal-gateway.md) |
| Admin MCP | [docs/operations/agent-mcp.md](docs/operations/agent-mcp.md) |
| Routing and failover | [docs/operations/routing-failover.md](docs/operations/routing-failover.md) |
| Model compatibility testing | [docs/operations/model-compatibility.md](docs/operations/model-compatibility.md) |
| Responses WebSocket mode | [docs/WebSocket-Mode.md](docs/WebSocket-Mode.md) |
| Embeddings / Rerank | [embeddings](docs/api/embeddings.md) · [rerank](docs/api/rerank.md) |
| Format conversion contract and coverage | [passthrough contract](docs/api/format-passthrough-contract.md) · [field matrix](docs/api/format-field-coverage-matrix.md) · [interface definitions](docs/api/provider-interface-definitions.md) |
| Redis runtime runbook | [docs/operations/redis-runtime-runbook.md](docs/operations/redis-runtime-runbook.md) |
| TLS fingerprint capture | [docs/operations/tls-fingerprint-capture.md](docs/operations/tls-fingerprint-capture.md) |
| Tunnel proxy node | [apps/aether-tunnel/README.md](apps/aether-tunnel/README.md) (Chinese) |
| All operations docs | [docs/operations/](docs/operations/) |

## Status

- The project evolves quickly; behavior and interfaces may change. Read the release notes in [docs/ARCA.md](docs/ARCA.md) before upgrading.
- Only source code is published for now. Parts of the install script and Nightly flow that point at upstream Aether do not include Arca's changes.
- Configuration uses the `ARCA_` prefix. Legacy `AETHER_*` / `AFSSR_*` names still work and print a deprecation warning on start.

## Contributing

Issues and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) (Chinese) for the development setup, checks and test commands.

## License and acknowledgements

Arca is a downstream fork of [Aether](https://github.com/fawney19/Aether) and keeps its [non-commercial license](LICENSE); the original authors' and contributors' copyright and attribution are preserved. This repository is not an official Aether release, and renaming does not change the license terms.
