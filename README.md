<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/hero-dark.png" />
    <img src="docs/assets/hero-light.png" alt="Arca — 自托管 AI 网关" width="100%" />
  </picture>
</p>

<p align="center">
  一个入口，统一接入 Claude、OpenAI、Gemini 与各类 CLI 客户端——鉴权、转换、调度、配额，都在你自己的服务器上。
</p>

<p align="center">
  <img src="https://img.shields.io/badge/deploy-Docker%20%C2%B7%20Linux%20%C2%B7%20macOS-6B625B?style=flat-square" alt="Deploy" />
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-non--commercial-6B625B?style=flat-square" alt="License: Non-Commercial" /></a>
  <a href="https://github.com/Becomingw/Arca/stargazers"><img src="https://img.shields.io/github/stars/Becomingw/Arca?style=flat-square&color=C8A27A" alt="GitHub stars" /></a>
</p>

<p align="center">
  <b>中文</b> · <a href="README.en.md">English</a> · <a href="docs/configuration.md">配置</a> · <a href="docs/ARCA.md">版本说明</a>
</p>

<br />

> **One endpoint, every model.**
> 模型的来源越来越多：官方 API、订阅账号、各家兼容服务——协议不同，额度不同，稳定性也不同。
> 客户端只想用熟悉的 OpenAI、Claude、Gemini 接口，运维只想要一个可控、可审计的入口。
> Arca 站在两者之间，把选路、转换、限额和计量收在同一个地方。

<p align="center">
  <img src="docs/assets/overview.jpg" alt="Arca 运维总览：QPS、SLA、首字延迟、并发与队列" width="88%" />
</p>

## 为什么是 Arca

<table>
  <tr>
    <td width="33%" valign="top">
      <h3>协议无关</h3>
      OpenAI Chat / Responses、Claude Messages、Gemini <code>generateContent</code> 都是一等入口，并在三者之间双向转换，覆盖流式、工具调用与推理内容。同协议请求原生直通，不做多余改写。
    </td>
    <td width="33%" valign="top">
      <h3>稳定可控</h3>
      按优先级、健康度与缓存亲和选路，失败自动重试并切换候选；订阅类上游用号池管理，额度与状态实时可见，不可用账号自动摘除。
    </td>
    <td width="33%" valign="top">
      <h3>自托管</h3>
      一个 Rust 二进制同时提供 API 与前端，依赖 PostgreSQL，Redis 可选。数据与密钥都在你自己的环境里，由你生成、由你保管。
    </td>
  </tr>
</table>

## 功能

### 统一接入与协议转换

- **多协议入口**：OpenAI Chat / Responses / Realtime、Claude Messages、Gemini `generateContent`，以及 Embeddings、Rerank、图像、视频任务与 Gemini Files。
- **双向转换**：请求与响应在 OpenAI、Claude、Gemini 之间互转；转换契约与字段覆盖矩阵公开在 [docs/api](docs/api/)。
- **按模型限定接口**：在模型映射上指定允许的接口类型与端点，例如让某个上游模型只走 Gemini 原生接口，避开不够完善的兼容层。
- **模型目录与映射**：全局模型与各提供商的真实模型名解耦，支持别名、阶梯定价与调用指令。

### 调度与稳定性

- **智能选路**：优先级、健康度、缓存亲和共同决定候选顺序，同协议候选优先。
- **故障转移**：上游失败时自动重试并切换候选，客户端断开行为可按策略配置，见[路由与故障转移](docs/operations/routing-failover.md)。
- **账号池**：为订阅类上游维护账号池，跟踪额度与健康状态，自动摘除不可用账号。
- **健康监控**：端点、模型、提供商三个视角查看可用率、耗时、首字延迟与速度。

<p align="center">
  <img src="docs/assets/models.jpg" alt="模型管理：定价、提供商数量与状态" width="49%" />
  <img src="docs/assets/health.jpg" alt="健康监控：端点、模型、提供商三个视角" width="49%" />
</p>

### 多租户、配额与成本

- **用户、用户组、API Key** 三级管理，按模型授权，支持 IP 黑白名单与审计日志。
- **配额**：滚动与日历请求上限、共享月度成本预算；未分配额度的用户默认拒绝，见[内部网关模式](docs/operations/internal-gateway.md)。
- **钱包与计费**：计费方案、钱包、邀请奖励与支付网关，成本与缓存节省分析。

### 运维与集成

- **管理控制台**：提供商与端点、模型、调度策略、用户与 Key、用量与成本、系统设置，中英文界面。
- **管理 MCP**：内置 `/mcp` 端点，使用绑定用户的管理 Key，写操作需预览确认，见 [Agent MCP](docs/operations/agent-mcp.md)。
- **Linux 一键接入**：在 API 密钥页生成一次性命令，把 Codex CLI / Claude Code 配到服务器上，见[接入指南](docs/operations/linux-client-setup.md)。
- **Tunnel 代理节点**：部署在海外 VPS 上，为网关中转出站流量，见 [Tunnel 文档](apps/aether-tunnel/README.md)。
- **可观测**：请求级用量与耗时、请求诊断、请求体查看、Prometheus 指标。

### 安全

- 上游凭据与敏感配置加密存储，备份使用带密钥标识的加密格式，支持[离线恢复](docs/configuration.md#s3-备份离线恢复)。
- 请求体与并发预算、出站 DNS 与代理策略、PII 脱敏与恢复、LDAP 与 OAuth 登录。

## 架构

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/architecture/architecture-dark.svg" />
    <img src="docs/architecture/architecture-light.svg" alt="Arca 架构：客户端经网关完成鉴权、解析、调度、转换与计量后到达上游" width="100%" />
  </picture>
</p>

请求依次经过：鉴权与准入 → 模型解析 → 候选选择与故障转移 → 协议转换（如需要）→ 用量与计费。官方 API 之外，网关内置 `codex`、`claude_code`、`gemini_cli`、`antigravity`、`vertex_ai`、`kiro`、`chatgpt_web`、`grok`、`windsurf`、`opencode_go` 等提供商模板，也支持任意 OpenAI / Claude / Gemini 兼容的自定义端点。

## 快速开始

Arca 目前**没有发布预构建镜像或 Release 产物**，需要自行构建。需要 Docker 与 Docker Compose，PostgreSQL 和 Redis 由仓库内的 Compose 文件一并拉起。

```bash
# 1. 获取代码并构建镜像
git clone https://github.com/Becomingw/Arca.git
cd Arca
docker build --build-arg AETHER_BUILD_VERSION=arca-dev \
  -f Dockerfile.app.local -t arca:local .

# 2. 配置：生成独立随机密钥，并设置 ADMIN_PASSWORD 与 APP_IMAGE=arca:local
cp .env.example .env && chmod 600 .env
./generate_keys.sh

# 3. 启动
docker compose up -d
```

打开 `http://localhost:8084`，用 `.env` 中的 `ADMIN_USERNAME` / `ADMIN_PASSWORD` 登录，依次添加提供商、模型与 API Key 即可。升级时不要替换已生成的密钥；数据库结构会在启动时自动迁移，想先检查可运行 `make db-status`。

更多细节见 [配置参考](docs/configuration.md) · [版本说明与自建镜像](docs/ARCA.md) · [Linux 客户端接入](docs/operations/linux-client-setup.md)。

<details>
<summary><b>想先看看界面？前端演示模式</b></summary>
<br />

前端自带一套模拟数据，无需后端即可浏览管理控制台：

```bash
cd frontend
npm ci
VITE_DEMO_MODE=true npm run dev
```

打开终端里给出的地址，先退出默认的演示用户，再用 `admin@demo.arca.io` / `demo123` 登录管理员视角。所有数据均为模拟，不会产生真实调用。

</details>

## 文档

| 主题 | 文档 |
| --- | --- |
| 配置、环境变量、S3 备份恢复 | [docs/configuration.md](docs/configuration.md) |
| 版本说明与自建镜像 | [docs/ARCA.md](docs/ARCA.md) |
| 内部网关模式 · 管理 MCP · 路由与故障转移 | [internal-gateway](docs/operations/internal-gateway.md) · [agent-mcp](docs/operations/agent-mcp.md) · [routing-failover](docs/operations/routing-failover.md) |
| 模型兼容性测试 · 请求诊断 | [model-compatibility](docs/operations/model-compatibility.md) · [request-diagnostics](docs/operations/request-diagnostics.md) |
| 协议：Embeddings · Rerank · Responses WebSocket | [embeddings](docs/api/embeddings.md) · [rerank](docs/api/rerank.md) · [WebSocket-Mode](docs/WebSocket-Mode.md) |
| 格式转换契约与字段覆盖 | [passthrough 契约](docs/api/format-passthrough-contract.md) · [字段矩阵](docs/api/format-field-coverage-matrix.md) · [接口定义](docs/api/provider-interface-definitions.md) |
| 运维手册 | [Redis 运行时](docs/operations/redis-runtime-runbook.md) · [TLS 指纹](docs/operations/tls-fingerprint-capture.md) · [全部](docs/operations/) |

## 开发

<p>
  <img src="https://img.shields.io/badge/Rust-1.95-DEA584?style=flat-square&logo=rust&logoColor=black" alt="Rust" />
  <img src="https://img.shields.io/badge/Vue-3-42B883?style=flat-square&logo=vuedotjs&logoColor=white" alt="Vue 3" />
  <img src="https://img.shields.io/badge/TypeScript-5-3178C6?style=flat-square&logo=typescript&logoColor=white" alt="TypeScript" />
  <img src="https://img.shields.io/badge/PostgreSQL-required-336791?style=flat-square&logo=postgresql&logoColor=white" alt="PostgreSQL" />
</p>

```bash
make dev            # 后端 + 前端 Vite dev server；缺少 Postgres/Redis 时自动拉起
make dev-backend    # 仅后端
make dev-frontend   # 仅前端
make db-status      # 查看数据库迁移状态
```

```text
arca/
├── apps/
│   ├── aether-gateway/   网关主程序：HTTP/WebSocket 入口、调度执行、管理 API、内置 MCP
│   └── aether-tunnel/    Tunnel 代理节点
├── crates/               领域库：协议格式、调度、数据访问、计费、用量、OAuth、运行时
├── frontend/             管理控制台（Vue 3 + TypeScript + Vite）
├── deploy/               Compose 变体、验收脚本与兼容性模板
└── docs/                 协议、运维手册与审计文档
```

> 目录与 crate 名称仍保留继承自上游的 `aether-*` 前缀，重命名会破坏已部署实例，暂不进行。配置项统一使用 `ARCA_` 前缀，旧的 `AETHER_*` / `AFSSR_*` 仍兼容并会打印弃用警告。

开发环境、检查与测试命令见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 参与贡献

欢迎 Issue 与 PR。较大的改动请先开 issue 对齐方向；提交前请运行 CONTRIBUTING 中列出的检查，并保持改动聚焦。

## 许可证与致谢

Arca 是 [Aether](https://github.com/fawney19/Aether) 的下游分支，沿用其[非商业开源许可证](LICENSE)，原作者与贡献者的版权和署名保留。本仓库不是 Aether 官方版本，更名也不改变许可条件。

<p align="center">
  <a href="https://www.star-history.com/#Becomingw/Arca&Date">
    <img src="https://api.star-history.com/svg?repos=Becomingw/Arca&type=Date" alt="Star History" width="70%" />
  </a>
</p>
