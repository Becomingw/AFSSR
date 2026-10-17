<div align="center">

<img src="frontend/public/arca.svg" width="96" height="96" alt="Arca">

# Arca

**自托管 AI 网关 —— 一个入口，统一接入 Claude、OpenAI、Gemini 与各类 CLI 客户端**

[![License: Non-Commercial](https://img.shields.io/badge/license-non--commercial-orange)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.95-b7410e?logo=rust)](rust-toolchain.toml)
[![Vue](https://img.shields.io/badge/vue-3-42b883?logo=vuedotjs&logoColor=white)](frontend/package.json)
[![PostgreSQL](https://img.shields.io/badge/postgresql-required-336791?logo=postgresql&logoColor=white)](docker-compose.yml)

[简体中文](README.md) · [English](README.en.md)

[特性](#特性) · [架构](#架构) · [快速开始](#快速开始) · [文档](#文档) · [参与贡献](#参与贡献) · [许可证](#许可证与致谢)

</div>

---

## 简介

Arca 是一个用 Rust 编写的自托管 AI API 网关，附带 Vue 管理控制台。它把多家上游模型服务收拢到同一个入口之后，向下游提供统一的鉴权、协议转换、智能调度、配额与用量管理。

- **对下游**：客户端继续使用熟悉的 OpenAI、Claude、Gemini 协议和 CLI 工具，无需改动。
- **对上游**：官方 API、订阅账号池、第三方兼容服务可以混合接入，由网关负责选路、重试与故障转移。
- **对运维**：一个 Rust 二进制同时提供 API 与前端静态资源，依赖 PostgreSQL，Redis 可选。

> Arca 是 [Aether](https://github.com/fawney19/Aether) 的下游分支，由 [Becomingw](https://github.com/Becomingw) 维护，原作者的版权与署名予以保留。详见[许可证与致谢](#许可证与致谢)。

## 特性

| 方向 | 能力 |
| --- | --- |
| **统一接入** | 同时提供 OpenAI Chat / Responses / Realtime、Claude Messages、Gemini `generateContent` 等协议入口；支持 Embeddings、Rerank、图像、视频任务与 Gemini Files |
| **协议转换** | 在 OpenAI、Claude、Gemini 之间双向转换请求与响应，覆盖流式输出、工具调用与推理内容；同协议请求原生直通，不做多余改写 |
| **智能调度** | 按优先级、健康度与缓存亲和选择上游 Key；失败自动重试并切换候选；支持按模型限定可用的接口类型与端点 |
| **账号池** | 对订阅类上游维护账号池，跟踪额度与健康状态，自动摘除不可用账号 |
| **多租户与配额** | 用户、用户组、API Key 三级管理；模型授权、滚动/日历请求上限、共享月度成本预算；钱包、计费方案与支付网关 |
| **管理控制台** | 提供商与端点、模型目录与映射、路由策略、用户与 Key、用量与成本分析、审计日志、IP 黑白名单、系统设置，中英文界面 |
| **可观测** | 请求级用量与耗时记录、请求诊断、请求体查看、Prometheus 指标、缓存与性能分析 |
| **安全** | 敏感数据加密存储、PII 脱敏与恢复、请求体与并发预算、出站 DNS/代理策略、LDAP 与 OAuth 登录 |
| **集成** | 内置管理 MCP 端点（用户绑定的管理 Key，写操作需预览确认）；Linux 客户端一键接入 Codex CLI / Claude Code；可选的反向 Tunnel 代理节点 |

## 架构

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/architecture/architecture-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="docs/architecture/architecture-light.svg">
    <img src="docs/architecture/architecture-light.svg" width="680" alt="Arca 架构：多种客户端协议经网关统一鉴权、调度、转换后到达上游">
  </picture>
</p>

请求进入网关后依次经过：鉴权与准入 → 模型解析 → 候选选择与排序 → 协议转换（如需要）→ 上游执行与故障转移 → 用量记录。

### 上游类型

官方 API 之外，网关内置以下固定提供商模板：`codex`、`claude_code`、`gemini_cli`、`antigravity`、`vertex_ai`、`kiro`、`chatgpt_web`、`grok`、`windsurf`、`opencode_go`，并支持任意 OpenAI / Claude / Gemini 兼容的自定义端点。

### 仓库结构

```text
apps/
  aether-gateway/   网关主程序：HTTP/WebSocket 入口、调度执行、管理 API、内置 MCP
  aether-tunnel/    Tunnel 代理节点，部署在海外 VPS 上为网关中转出站流量
crates/             领域库：协议格式、调度、数据访问、计费、用量、OAuth、运行时等
frontend/           管理控制台（Vue 3 + TypeScript + Vite）
deploy/             Compose 变体、验收脚本与兼容性模板
docs/               协议、运维手册与审计文档
```

> 目录与 crate 名称仍保留继承自上游的 `aether-*` 前缀，这不影响使用；重命名会破坏已部署实例，暂不进行。

## 快速开始

### 前置条件

Docker 与 Docker Compose；PostgreSQL 与 Redis 由仓库内的 Compose 文件一并拉起。

### 1. 构建镜像

Arca 目前**没有发布预构建镜像或 Release 产物**，需要自行构建：

```bash
git clone https://github.com/Becomingw/Arca.git
cd Arca
docker build --build-arg AETHER_BUILD_VERSION=arca-dev \
  -f Dockerfile.app.local -t arca:local .
```

### 2. 配置

```bash
cp .env.example .env
chmod 600 .env          # .env 包含数据库、JWT 与加密密钥
./generate_keys.sh      # 生成 JWT / 加密 / Postgres / Redis 独立随机密钥并写入 .env
# 编辑 .env：至少设置 ADMIN_PASSWORD，并设置 APP_IMAGE=arca:local
```

### 3. 启动

```bash
docker compose up -d
```

服务默认监听 `8084` 端口。打开 `http://localhost:8084`，使用 `.env` 中的 `ADMIN_USERNAME` / `ADMIN_PASSWORD` 登录，然后在管理控制台依次添加提供商、模型与 API Key。

升级现有部署时不要替换已生成的密钥。数据库结构会在启动时自动迁移，需要先检查时可使用 `make db-status`。

### 本地开发

依赖 Docker、Rust（版本见 [`rust-toolchain.toml`](rust-toolchain.toml)）、Node.js 与 make。首次启动前在 `.env` 中设置 `ADMIN_PASSWORD`。

```bash
make dev            # 后端 + 前端 Vite dev server；缺少 Postgres/Redis 时自动拉起
make dev-backend    # 仅后端
make dev-frontend   # 仅前端
make db-status      # 查看数据库迁移状态
make db-prepare     # 预先执行迁移与回填
```

## 文档

| 主题 | 文档 |
| --- | --- |
| 配置与环境变量、S3 备份恢复 | [docs/configuration.md](docs/configuration.md) |
| Arca 版本说明与自建镜像 | [docs/ARCA.md](docs/ARCA.md) |
| Linux 客户端一键接入 | [docs/operations/linux-client-setup.md](docs/operations/linux-client-setup.md) |
| 内部网关模式（用户/组授权） | [docs/operations/internal-gateway.md](docs/operations/internal-gateway.md) |
| 管理 MCP | [docs/operations/agent-mcp.md](docs/operations/agent-mcp.md) |
| 路由与故障转移 | [docs/operations/routing-failover.md](docs/operations/routing-failover.md) |
| 模型兼容性测试 | [docs/operations/model-compatibility.md](docs/operations/model-compatibility.md) |
| Responses WebSocket 模式 | [docs/WebSocket-Mode.md](docs/WebSocket-Mode.md) |
| Embeddings / Rerank | [embeddings](docs/api/embeddings.md) · [rerank](docs/api/rerank.md) |
| 格式转换契约与字段覆盖 | [passthrough 契约](docs/api/format-passthrough-contract.md) · [字段矩阵](docs/api/format-field-coverage-matrix.md) · [接口定义](docs/api/provider-interface-definitions.md) |
| Redis 运行时手册 | [docs/operations/redis-runtime-runbook.md](docs/operations/redis-runtime-runbook.md) |
| TLS 指纹采集 | [docs/operations/tls-fingerprint-capture.md](docs/operations/tls-fingerprint-capture.md) |
| Tunnel 代理节点 | [apps/aether-tunnel/README.md](apps/aether-tunnel/README.md) |
| 全部运维文档 | [docs/operations/](docs/operations/) |

## 状态

- 项目仍在快速演进，行为与接口可能变化；升级前请阅读 [docs/ARCA.md](docs/ARCA.md) 中的版本说明。
- 目前仅发布源码。部署流程、安装脚本与 Nightly 中指向上游 Aether 的部分不包含 Arca 的修改。
- 配置项统一使用 `ARCA_` 前缀；旧的 `AETHER_*` / `AFSSR_*` 仍兼容，启动时会打印弃用警告。

## 参与贡献

欢迎提交 Issue 与 Pull Request。开发环境、检查与测试命令见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 许可证与致谢

Arca 基于 [Aether](https://github.com/fawney19/Aether) 开发，原 Aether 及其贡献者的版权与署名保留，本仓库不是 Aether 官方版本。

本项目沿用原有的 [Aether 非商业开源许可证](LICENSE)：允许个人学习研究、教育机构、非盈利组织及企业内部非盈利性质的使用；**禁止用于盈利目的**，分发须使用相同许可条款，且不得再许可。更名不改变许可条件，也不授予商业使用权。

需要商业许可请联系原版权持有人（Aether 作者）：

<details>
<summary>原作者联系方式</summary>

<p align="center">
  <img src="docs/author/qq_qrcode.jpg" width="200" alt="QQ 二维码">
  &nbsp;&nbsp;&nbsp;&nbsp;
  <img src="docs/author/qrcode_1770574997172.jpg" width="200" alt="QQ 群二维码">
</p>

</details>
