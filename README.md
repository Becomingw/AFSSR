# Arca

基于 [Aether](https://github.com/fawney19/Aether) 的下游修改版，由
[Mengfei Wang / Becomingw](https://github.com/Becomingw) 维护。
原始 Aether 及其贡献者的版权和署名保留，本仓库不是 Aether 官方版本。

- 完整源码包含后端、前端、隧道以及 Codex/Haiku/GLM 兼容修改和测试。
- `haiku-compat6-20261013` 标签对应已部署并通过公网合成验收的源码；
  `main` 还包含尚未部署的本地 GLM 历史兼容修改。
- 继续使用原 [非商业许可证](LICENSE)，改名不改变许可条件。
- 本仓库目前仅发布源码，没有发布 Arca 的预构建镜像。下方文档中指向上游 Aether
  的克隆、安装脚本、镜像和更新地址不包含这里的修改，请先阅读 [Arca 版本说明](docs/ARCA.md)。

## 使用文档（承自 Aether）

<p align="center">
  <img src="frontend/public/arca.svg" width="120" height="120" alt="Arca Logo">
</p>

<h1 align="center">Arca</h1>

<p align="center">
  <strong>一站式 AI 基础设施平台</strong><br>
  支持 Claude / OpenAI / Gemini 及其 CLI 客户端的统一接入、格式转换、正/反向代理, 致力于成为用户驱动AI服务的底座
</p>
<p align="center">
  <a href="#简介">简介</a> •
  <a href="#部署">部署</a> •
  <a href="#api-文档">API 文档</a> •
  <a href="#环境变量">环境变量</a> •
  <a href="#qa">Q&A</a>
</p>


---

## 简介

Arca 是一个自托管的 AI API 网关，为团队和个人提供多租户管理、智能负载均衡、成本配额控制和健康监控能力。通过统一的 API 入口，可以无缝对接 Claude、OpenAI、Gemini 等主流 AI 服务及其 CLI 工具。

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/architecture/architecture-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="docs/architecture/architecture-light.svg">
    <img src="docs/architecture/architecture-light.svg" width="680" alt="Arca Architecture">
  </picture>
</p>

页面预览: https://becomingw.github.io/Arca/

## 部署

### Docker Compose（推荐：预构建镜像）

```bash
# 1. 克隆代码
git clone https://github.com/fawney19/Aether.git
cd Aether

# 2. 配置环境变量
cp .env.example .env
# .env 包含数据库、JWT 和数据加密密钥，先限制为仅当前用户可读写
chmod 600 .env
# 生成 JWT / 加密 / Postgres / Redis 独立随机密钥，并填入 .env
./generate_keys.sh
# 编辑 .env 设置 ADMIN_PASSWORD

# 3. Docker 部署 / 更新（PostgreSQL + Redis）
docker compose pull && docker compose up -d
```

### 一键安装（PostgreSQL + Redis）

```bash
git clone https://github.com/fawney19/Aether.git
cd Aether
curl -fsSL https://raw.githubusercontent.com/fawney19/Aether/main/install.sh | sudo bash -s -- --mode compose
```

原生 Linux systemd / macOS launchd 安装需先准备 PostgreSQL，将连接串通过 `DATABASE_URL` 传给安装进程，并选择 `--mode single-node`；不再自动创建本地数据库文件。

### Nightly（每日 main 构建）

Nightly workflow 每天从 `main` 的固定 commit 构建并发布滚动的 GitHub Release `nightly`，同时推送多架构 GHCR 镜像 `ghcr.io/fawney19/aether:nightly`。Nightly 是预发布版本，适合验证最新代码，不保证与正式版相同的稳定性。滚动 Release 需要仓库保持关闭 GitHub Release immutability。

安装最新 nightly（PostgreSQL + Redis）：

```bash
curl -fsSL https://raw.githubusercontent.com/fawney19/Aether/main/install.sh | sudo bash -s -- --mode compose --channel nightly
```

Docker Compose 用户可在部署目录的 `.env` 中设置 `APP_IMAGE=ghcr.io/fawney19/aether:nightly`，然后运行 `./update.sh` 获取下一次 nightly。二进制部署请沿用已有 PostgreSQL 环境配置，并使用 `--mode single-node --channel nightly` 重新运行安装脚本升级；当前管理后台的在线更新列表只跟踪正式版/RC/Beta，不会自动提示下一次 nightly。

## 本地开发

依赖 Docker、Rust toolchain、Node.js 和 make。
首次启动前需要在 `.env` 中设置 `ADMIN_PASSWORD`，用于创建本地管理员。

```bash
make dev
```

`make dev` 会同时启动后端 `aether-gateway` 和前端 `frontend` 的 Vite dev server。需要单独启动时可使用 `make dev-backend` 或 `make dev-frontend`。
Postgres / Redis 本地依赖未就绪时，`make dev` 会自动执行 `docker compose up -d postgres redis`。
`make dev` 会先完成后端编译，再开始计算服务健康检查超时。数据库 schema 和必要的派生数据准备也会在启动时自动完成；通常不需要手动区分 migration 与 backfill。升级不会主动重写或清除已有业务历史记录，新写入会直接遵循当前的数据持久化策略。排查或部署前预执行时可使用：

```bash
make db-status
make db-prepare
```

## Codex 远程协同

`aether-vscodex/` 是独立的 VS Code Codex 协同模块：同步模式跟随 VS Code 官方 Codex 面板当前会话且不另起进程；异步模式使用独立 app-server，让浏览器自行列出、恢复、新建和切换会话。两种模式都能从本机 URL 或 Aether 云端查看输出、发送消息和处理授权，模块内的 Vue 前端提供中英文界面。

安装、云端配对和安全边界请参阅 [`aether-vscodex/README.md`](aether-vscodex/README.md)。

## Arca Tunnel (可选)

Arca Tunnel 是配套的正向代理节点，部署在海外 VPS 上，为墙内的 Arca 实例中转 API 流量。

- Docker Compose 部署或下载预编译二进制直接运行
- 提供 macOS/Linux 与 Windows 一键脚本，自动下载最新 `tunnel-v*` 制品并向现有 `aether-tunnel.toml` 追加 `[[servers]]`
- 通过 `aether-tunnel setup` 完成交互式配置，自动注册为系统服务
- 详细文档见 [apps/aether-tunnel/README.md](apps/aether-tunnel/README.md)

## API 文档

- Embeddings: [OpenAI compatible `POST /v1/embeddings`](docs/api/embeddings.md)
- Rerank: [OpenAI/Jina compatible `POST /v1/rerank`](docs/api/rerank.md)
- Responses WebSocket mode: [protocol and Arca behavior](docs/WebSocket-Mode.md)
- WebSocket probes: [Codex](docs/operations/codex-responses-websocket-probe.md) · [OpenAI Responses](docs/operations/openai-responses-websocket-probe.md)

## 环境变量

- `APP_PORT`：`aether-gateway` 唯一监听端口，固定绑定 `0.0.0.0:${APP_PORT}`
- `DATABASE_URL`：PostgreSQL 连接串，例如 `postgresql://USER:PASSWORD@HOST:5432/aether`
- `AETHER_GATEWAY_DATA_POSTGRES_MIN_CONNECTIONS` / `AETHER_GATEWAY_DATA_POSTGRES_MAX_CONNECTIONS`：数据库连接池手动覆盖值；未配置时 PostgreSQL 按每核 `4` 条自动推导，总池范围为 `32-100`。该预算按进程计算，多实例部署应按数据库连接上限显式分配
- `AETHER_GATEWAY_MAX_IN_FLIGHT_REQUESTS`：单实例请求并发上限；未配置时按 CPU 自动推导（基础范围 `512-65536`），低文件描述符预算时会进一步下调
- `AETHER_GATEWAY_REQUEST_BODY_BUFFER_BUDGET_MB`：单实例同时读取和解压请求体的加权内存预算，默认 `256MB`
- `AETHER_GATEWAY_REQUEST_BODY_READ_TIMEOUT_MS`：可选的请求体完整读取超时；默认或显式设为 `0` 时关闭，非零值限制在 `1000-600000ms`
- `AETHER_MAX_REQUEST_BODY_MB`：单请求解压后请求体上限，默认 `256MB`；显式设为 `0` 表示不再收紧默认值，但仍受 `256MB` 安全硬上限约束
- `AETHER_MAX_INTERNAL_BUFFERED_BODY_MB`：heartbeat、管理探测等内部整包响应体上限，默认 `64MB`；显式设为 `0` 表示不再收紧默认值，但仍受 `256MB` 安全硬上限约束
- `AETHER_TUNNEL_NODE_STATUS_QUEUE_CAPACITY`：隧道节点状态上报队列容量，默认 `1024`；满载时拒绝新事件，避免控制面故障导致无界内存增长
- `AETHER_TUNNEL_RELAY_ALLOW_PRIVATE_TARGETS`：跨网关 owner relay 解析到私有/保留地址时的显式运维开关，默认关闭；仅当多网关 relay URL 是受控的内网 HTTPS 地址时设置为 `true`。它不改变普通 provider 请求的 DNS/代理策略，也不允许明文 HTTP 非 loopback relay
- `AETHER_TUNNEL_RELAY_PRIVATE_HOST_ALLOWLIST`：更窄的 owner relay 私网例外，填写逗号分隔的精确主机名（例如 `gateway-a.internal,gateway-b.internal`，忽略大小写和末尾点）；仅这些主机解析出的私有地址会被允许，并且请求仍使用解析后地址 pin。不要填写通配符或 `.internal` 这类后缀
- `AETHER_INTERNAL_GATEWAY_AUTH_SECRET`：旧版 `/api/internal/gateway/*` 高权限控制面的独立 HMAC 密钥，至少 `32` 字节；未配置时该控制面返回 `404`。不要复用 JWT、数据加密或 tunnel relay 密钥，多节点必须使用同一值及共享 Redis 防重放
- `AETHER_GATEWAY_SECURITY_CACHE_TTL_MS`：IP 黑白名单本地缓存时间，默认 `1000ms`，写操作会主动失效相关缓存
- `AETHER_MAX_REDACTED_SYNC_RESPONSE_BODY_MB`：PII 恢复同步响应缓冲上限，默认 `64MB`；显式设为 `0` 表示不再收紧默认值，但仍受 `256MB` 安全硬上限约束
- `REDIS_URL`：Redis 连接串；仅 Postgres + Redis 的 Docker Compose 部署需要配置
- `AETHER_RUNTIME_BACKEND=memory|redis`：运行时缓存/协调后端。配置 Redis 时使用 `redis`，否则使用 `memory`；多节点部署和需要跨 gateway 重启恢复 OpenAI Responses continuation history 的部署必须使用共享 Redis
- `AETHER_GATEWAY_DATABASE_MODE=auto|verify-only`：数据库启动策略，默认 `auto`，自动完成挂起的 schema migration 和 backfill；`verify-only` 仅检查并在数据库落后时拒绝启动
- `AETHER_GATEWAY_AUTO_PREPARE_DATABASE`：旧版兼容开关；新配置请使用 `AETHER_GATEWAY_DATABASE_MODE`
- `JWT_SECRET_KEY` / `ENCRYPTION_KEY`：认证和敏感数据加密所需密钥
- `AETHER_BACKUP_ENCRYPTION_KEY`：推荐的 S3 备份独立加密密钥；缺省回退到 `ENCRYPTION_KEY`。新备份使用带 key ID 的 AES-256-GCM v2 envelope，轮换前必须保留旧密钥
- `API_KEY_PREFIX`：用户和管理员新建 API Key 时使用的前缀，默认 `sk`
- `ADMIN_USERNAME` / `ADMIN_PASSWORD` / `ADMIN_EMAIL`：首次启动时自举首个本地管理员；`install.sh` 会提示输入管理员密码
- `CORS_ORIGINS` / `CORS_ALLOW_CREDENTIALS`：前端跨域来源控制；如果要跨域带登录 Cookie，`CORS_ORIGINS` 不能写 `*`
- `RUST_LOG`：Rust 日志过滤，例如 `aether_gateway=info`、`aether_gateway=debug,sqlx=warn`
- `DB_PASSWORD` / `REDIS_PASSWORD`：Docker Compose 后端密码，首次安装时分别随机生成；手工部署必须替换示例占位值，不要互相复用

### S3 备份离线恢复

先从 S3 下载完整的 `.json.zst.aes256gcm` 对象，再使用原始的完整 S3 object key 做认证解密。恢复工具只验证并输出本地 JSON，不会直接写数据库；数据库导入仍应在维护窗口通过管理端完成。

```bash
AETHER_BACKUP_ENCRYPTION_KEY='原备份密钥' \
  cargo run -p aether-gateway --bin aether-backup-restore -- \
  --input ./backup.json.zst.aes256gcm \
  --object-key 'aether/backups/aether-data-backup-20260822-010000.json.zst.aes256gcm' \
  --output ./restored-backup.json
```

工具默认拒绝覆盖，输出采用原子写并在 Unix 上设置为 `0600`；Unix 可用 `--overwrite` 原子替换，Windows 为避免非原子删除窗口会要求选择新输出路径。密钥不能作为命令行参数。可使用 `AETHER_BACKUP_ENCRYPTION_KEY`、兼容用 `AETHER_GATEWAY_DATA_ENCRYPTION_KEY` / `ENCRYPTION_KEY`、受保护的 `--key-file`，或 `AETHER_BACKUP_KEYRING_FILE`。Keyring JSON 格式为 `{"version":1,"keys":["当前或历史 v2 secret"],"legacy_v1":["旧 v1 secret"]}`；条目也可写成 `{"secret":"..."}`（兼容字段名 `key`）。也可由 `AETHER_BACKUP_HISTORICAL_KEYS_JSON` 提供同一结构。密钥文件必须是非符号链接的普通文件，Unix 下权限需为 `0600` 或更严格。

默认限制密文为 `512MiB`、解压后 JSON 为 `1GiB`，可通过受限的 `--max-encrypted-mib` / `--max-json-mib` 调整。网关最多扫描同一备份前缀下 10,000 个对象，并且不会自动删除 S3 对象：`backup_s3_retention_count` 只用于报告超出保留数量的清理候选。旧明文备份在创建并验证加密副本后仍会保留，必须通过 bucket lifecycle 或支持版本条件的外部清理工具移除；启用 Versioning 时还需清理 noncurrent versions，Object Lock/retention 可能阻止物理删除。

---

## 许可证

本项目采用 [Aether 非商业开源许可证](LICENSE)。允许个人学习、教育研究、非盈利组织及企业内部非盈利性质的使用；禁止用于盈利目的。商业使用请联系获取商业许可。

## 联系原作者（Aether，商业许可咨询）

<p align="center">
  <img src="docs/author/qq_qrcode.jpg" width="200" alt="QQ二维码">
  &nbsp;&nbsp;&nbsp;&nbsp;
  <img src="docs/author/qrcode_1770574997172.jpg" width="200" alt="QQ群二维码">
</p>

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=Becomingw/Arca&type=date&legend=top-left)](https://www.star-history.com/?repos=Becomingw%2FArca&type=date&legend=top-left)
