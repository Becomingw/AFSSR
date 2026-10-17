# 配置参考

本文档列出 Arca 网关常用的环境变量，以及 S3 备份的离线恢复方法。完整的示例见仓库根目录的 [`.env.example`](../.env.example)。

## 环境变量

> 配置项统一使用 `ARCA_` 前缀。旧的 `AETHER_*` / `AFSSR_*` 名称仍可使用，启动时会打印一次弃用警告；同名时 `ARCA_*` 优先。构建参数 `AETHER_BUILD_VERSION` / `AETHER_BUILD_TYPE` 与测试用的 `*_TEST_DATABASE_URL` 暂保持原名。

- `APP_PORT`：`aether-gateway` 唯一监听端口，固定绑定 `0.0.0.0:${APP_PORT}`
- `DATABASE_URL`：PostgreSQL 连接串，例如 `postgresql://USER:PASSWORD@HOST:5432/aether`
- `ARCA_GATEWAY_DATA_POSTGRES_MIN_CONNECTIONS` / `ARCA_GATEWAY_DATA_POSTGRES_MAX_CONNECTIONS`：数据库连接池手动覆盖值；未配置时 PostgreSQL 按每核 `4` 条自动推导，总池范围为 `32-100`。该预算按进程计算，多实例部署应按数据库连接上限显式分配
- `ARCA_GATEWAY_MAX_IN_FLIGHT_REQUESTS`：单实例请求并发上限；未配置时按 CPU 自动推导（基础范围 `512-65536`），低文件描述符预算时会进一步下调
- `ARCA_GATEWAY_REQUEST_BODY_BUFFER_BUDGET_MB`：单实例同时读取和解压请求体的加权内存预算，默认 `256MB`
- `ARCA_GATEWAY_REQUEST_BODY_READ_TIMEOUT_MS`：可选的请求体完整读取超时；默认或显式设为 `0` 时关闭，非零值限制在 `1000-600000ms`
- `ARCA_MAX_REQUEST_BODY_MB`：单请求解压后请求体上限，默认 `256MB`；显式设为 `0` 表示不再收紧默认值，但仍受 `256MB` 安全硬上限约束
- `ARCA_MAX_INTERNAL_BUFFERED_BODY_MB`：heartbeat、管理探测等内部整包响应体上限，默认 `64MB`；显式设为 `0` 表示不再收紧默认值，但仍受 `256MB` 安全硬上限约束
- `ARCA_TUNNEL_NODE_STATUS_QUEUE_CAPACITY`：隧道节点状态上报队列容量，默认 `1024`；满载时拒绝新事件，避免控制面故障导致无界内存增长
- `ARCA_TUNNEL_RELAY_ALLOW_PRIVATE_TARGETS`：跨网关 owner relay 解析到私有/保留地址时的显式运维开关，默认关闭；仅当多网关 relay URL 是受控的内网 HTTPS 地址时设置为 `true`。它不改变普通 provider 请求的 DNS/代理策略，也不允许明文 HTTP 非 loopback relay
- `ARCA_TUNNEL_RELAY_PRIVATE_HOST_ALLOWLIST`：更窄的 owner relay 私网例外，填写逗号分隔的精确主机名（例如 `gateway-a.internal,gateway-b.internal`，忽略大小写和末尾点）；仅这些主机解析出的私有地址会被允许，并且请求仍使用解析后地址 pin。不要填写通配符或 `.internal` 这类后缀
- `ARCA_INTERNAL_GATEWAY_AUTH_SECRET`：旧版 `/api/internal/gateway/*` 高权限控制面的独立 HMAC 密钥，至少 `32` 字节；未配置时该控制面返回 `404`。不要复用 JWT、数据加密或 tunnel relay 密钥，多节点必须使用同一值及共享 Redis 防重放
- `ARCA_GATEWAY_SECURITY_CACHE_TTL_MS`：IP 黑白名单本地缓存时间，默认 `1000ms`，写操作会主动失效相关缓存
- `ARCA_MAX_REDACTED_SYNC_RESPONSE_BODY_MB`：PII 恢复同步响应缓冲上限，默认 `64MB`；显式设为 `0` 表示不再收紧默认值，但仍受 `256MB` 安全硬上限约束
- `REDIS_URL`：Redis 连接串；仅 Postgres + Redis 的 Docker Compose 部署需要配置
- `ARCA_RUNTIME_BACKEND=memory|redis`：运行时缓存/协调后端。配置 Redis 时使用 `redis`，否则使用 `memory`；多节点部署和需要跨 gateway 重启恢复 OpenAI Responses continuation history 的部署必须使用共享 Redis
- `ARCA_GATEWAY_DATABASE_MODE=auto|verify-only`：数据库启动策略，默认 `auto`，自动完成挂起的 schema migration 和 backfill；`verify-only` 仅检查并在数据库落后时拒绝启动
- `ARCA_GATEWAY_AUTO_PREPARE_DATABASE`：旧版兼容开关；新配置请使用 `ARCA_GATEWAY_DATABASE_MODE`
- `JWT_SECRET_KEY` / `ENCRYPTION_KEY`：认证和敏感数据加密所需密钥
- `ARCA_BACKUP_ENCRYPTION_KEY`：推荐的 S3 备份独立加密密钥；缺省回退到 `ENCRYPTION_KEY`。新备份使用带 key ID 的 AES-256-GCM v2 envelope，轮换前必须保留旧密钥
- `API_KEY_PREFIX`：用户和管理员新建 API Key 时使用的前缀，默认 `sk`
- `ADMIN_USERNAME` / `ADMIN_PASSWORD` / `ADMIN_EMAIL`：首次启动时自举首个本地管理员；`install.sh` 会提示输入管理员密码
- `CORS_ORIGINS` / `CORS_ALLOW_CREDENTIALS`：前端跨域来源控制；如果要跨域带登录 Cookie，`CORS_ORIGINS` 不能写 `*`
- `RUST_LOG`：Rust 日志过滤，例如 `aether_gateway=info`、`aether_gateway=debug,sqlx=warn`
- `DB_PASSWORD` / `REDIS_PASSWORD`：Docker Compose 后端密码，首次安装时分别随机生成；手工部署必须替换示例占位值，不要互相复用

## S3 备份离线恢复

先从 S3 下载完整的 `.json.zst.aes256gcm` 对象，再使用原始的完整 S3 object key 做认证解密。恢复工具只验证并输出本地 JSON，不会直接写数据库；数据库导入仍应在维护窗口通过管理端完成。

```bash
ARCA_BACKUP_ENCRYPTION_KEY='原备份密钥' \
  cargo run -p aether-gateway --bin aether-backup-restore -- \
  --input ./backup.json.zst.aes256gcm \
  --object-key 'aether/backups/aether-data-backup-20260822-010000.json.zst.aes256gcm' \
  --output ./restored-backup.json
```

工具默认拒绝覆盖，输出采用原子写并在 Unix 上设置为 `0600`；Unix 可用 `--overwrite` 原子替换，Windows 为避免非原子删除窗口会要求选择新输出路径。密钥不能作为命令行参数。可使用 `ARCA_BACKUP_ENCRYPTION_KEY`、兼容用 `ARCA_GATEWAY_DATA_ENCRYPTION_KEY` / `ENCRYPTION_KEY`、受保护的 `--key-file`，或 `ARCA_BACKUP_KEYRING_FILE`。Keyring JSON 格式为 `{"version":1,"keys":["当前或历史 v2 secret"],"legacy_v1":["旧 v1 secret"]}`；条目也可写成 `{"secret":"..."}`（兼容字段名 `key`）。也可由 `ARCA_BACKUP_HISTORICAL_KEYS_JSON` 提供同一结构。密钥文件必须是非符号链接的普通文件，Unix 下权限需为 `0600` 或更严格。

默认限制密文为 `512MiB`、解压后 JSON 为 `1GiB`，可通过受限的 `--max-encrypted-mib` / `--max-json-mib` 调整。网关最多扫描同一备份前缀下 10,000 个对象，并且不会自动删除 S3 对象：`backup_s3_retention_count` 只用于报告超出保留数量的清理候选。旧明文备份在创建并验证加密副本后仍会保留，必须通过 bucket lifecycle 或支持版本条件的外部清理工具移除；启用 Versioning 时还需清理 noncurrent versions，Object Lock/retention 可能阻止物理删除。
