# 系统设置词典

> 依据：已提交代码（核对于 2026-10-18，HEAD `266c45ed1`），以及控制台界面文案。**没有在运行中的界面逐项验证**；有疑问时读实时值（`arca_system_config`）并以代码为准。
> 默认值表见 [settings-defaults.md](settings-defaults.md)（由代码自动生成）。本文件每个条目只写含义、影响和风险；`scripts/gen_settings_defaults.py --check` 会检查代码里的每个键都有条目。

## 通用规则

- **生效时机**：系统配置改动不需重启，但有缓存：读取 30 秒新鲜、最长约 5 分钟陈旧；其它节点约 30 秒后看到；限流默认值再加约 15 秒，PII 再加约 5 秒。通过环境变量决定的模块可用性（`OAUTH_AVAILABLE`、`LDAP_AVAILABLE`、`S3_BACKUP_AVAILABLE` 等，默认都为开）和 `APP_TIMEZONE`（默认 Asia/Shanghai）需要**重启**。
- **写入校验**：只有部分键会校验（密码策略、模型后缀、通知/推送/PII 模块、SMTP host/from 等）；其余键接受任何 JSON，**类型错了会在读取时静默回退到代码默认值**，所以写完要回读确认。
- **敏感键**（`smtp_password`、`turnstile_secret_key`、`backup_s3_secret_access_key`、`module.server_chan_push.send_key`、`module.bark_push.device_key`）加密存储，读取只显示"已设置"，保存响应返回 `********`；没有加密密钥时写入返回 503。MCP 还会把名字含 password/secret/token、以 `_key` 结尾、或值里含 `sk-` / `ae-` / `Bearer ` 的内容显示成 `[REDACTED]`。
- **MCP 写入**：`arca_system_config_set` 是两步的 Destructive 操作（预览票据 300 秒有效），需要 `admin:system:admin`，即只有超级管理员；body 为 `{value, expected_value?, description?}`，只有 `internal_access_policy` 强制 `expected_value`。
- 键以 `internal.` 开头的是内部状态，列表里看不到。`compatibility_workbench_v1` 不可写（403）。
- **品牌残留**：几处默认值仍带 Aether 字样（`site_name`、`smtp_from_name`、PII 占位符前缀、S3 前缀）。前端只在界面上把旧名显示成 Arca，邮件等不会。
- **条目状态**：在用 / 已停用（商业功能，内部模式无效） / 无读取方（改了没有效果） / 品牌残留。

## 站点信息

控制台「系统设置 → 站点信息」。

### `site_name`

- **界面标签**：站点名称
- **含义**：显示在导航栏、登录页标题和邮件里。
- **影响与风险**：仅外观。默认值仍是 "Aether"：前端会把旧默认显示成 "Arca"，但邮件在没设置时仍写 "Aether"，建议显式设置。
- **状态**：在用 · 品牌残留

### `site_subtitle`

- **界面标签**：站点副标题
- **含义**：显示在导航栏品牌名下方。默认 "AI Gateway"。
- **影响与风险**：仅外观。
- **状态**：在用 · 品牌残留

## 用户与认证

控制台「系统设置 → 基础配置」以及「邮件配置」页的注册相关项。

### `default_user_initial_gift_usd`

- **界面标签**：默认用户初始赠款(美元)
- **含义**：旧版商业钱包的新用户赠款。
- **影响与风险**：内部模式下无效：钱包初始化强制为 0，界面也隐藏该字段。注意保存"基础配置"卡片仍会重写此键，不必在意。
- **状态**：已停用

### `password_policy_level`

- **界面标签**：密码策略
- **含义**：`weak`（≥6 字符）/`medium`（≥8 位，含字母和数字）/`strong`（≥8 位，含大小写字母、数字、特殊字符）。影响注册、创建用户、重置/修改密码的校验。
- **影响与风险**：只校验**新设置**的密码，不追溯已有密码。非法值写入会 400；密码另有 72 字节上限。
- **状态**：在用

### `turnstile_enabled`

- **界面标签**：注册人机验证
- **含义**：开启后，注册和发送邮箱验证码前需要通过 Cloudflare Turnstile。
- **影响与风险**：只保护注册和发验证码，**不保护登录**。开启但没配密钥会让注册返回 503；服务器无法访问 Cloudflare 时同样无法注册。
- **状态**：在用

### `turnstile_site_key`

- **界面标签**：Turnstile Site Key
- **含义**：Turnstile 前端站点密钥。
- **影响与风险**：公开值，但 MCP 读取时会被脱敏，无法通过 MCP 核对。
- **状态**：在用

### `turnstile_secret_key`

- **界面标签**：Turnstile Secret Key
- **含义**：Turnstile 服务端密钥，加密存储。
- **影响与风险**：敏感：读取只显示是否已设置。填错会使注册失败。
- **状态**：在用

### `turnstile_allowed_hostnames`

- **界面标签**：允许的 Hostname
- **含义**：Turnstile 校验通过后，额外要求 Cloudflare 返回的 hostname 在此列表内；留空不额外校验。
- **影响与风险**：拼写错误会拦住所有注册。
- **状态**：在用

### `auto_delete_expired_keys`

- **界面标签**：自动删除过期 Key
- **含义**：每日清理任务对已过期的 API 密钥：开启时**删除**，关闭时仅**禁用**。密钥自己的"到期自动删除"设置优先。
- **影响与风险**：开启会造成数据丢失；只有 `enable_auto_cleanup` 为真时才会运行，下次 03:00 生效。
- **状态**：在用

## 邮件 (SMTP)

控制台「邮件配置」页。SMTP 用于注册验证码、通知邮件和反馈 @ 提醒邮件。host 或发件邮箱为空时，邮件功能视为未配置。

### `smtp_host`

- **界面标签**：SMTP 服务器地址
- **含义**：邮件服务器地址（不能含换行，≤255 字节）。
- **影响与风险**：修改 host/端口/用户名/加密方式后，**已保存的密码会失效**（密码与这些字段绑定），需要重新输入密码。
- **状态**：在用

### `smtp_port`

- **界面标签**：SMTP 端口
- **含义**：常用 587（TLS/STARTTLS）、465（SSL）、25（无加密）。
- **影响与风险**：同上，改动后需重填密码。
- **状态**：在用

### `smtp_user`

- **界面标签**：SMTP 用户名
- **含义**：通常是邮箱地址。
- **影响与风险**：同上。
- **状态**：在用

### `smtp_password`

- **界面标签**：SMTP 密码
- **含义**：邮箱密码或应用专用密码，加密存储，与 host/port/user/tls/ssl 绑定。
- **影响与风险**：敏感。保存前必须先配好 host 和 user，否则 400。绑定字段改变后发信会报"无法解密已保存的 SMTP 密码"。
- **状态**：在用

### `smtp_use_tls`

- **界面标签**：加密方式（TLS/STARTTLS）
- **含义**：界面是一个"加密方式"下拉：SSL（隐式加密）/ TLS（STARTTLS）/ 无加密，分别对应 `smtp_use_ssl` 和 `smtp_use_tls`。
- **影响与风险**："无加密"会明文发送账号密码。Gmail 等推荐 SSL。
- **状态**：在用

### `smtp_use_ssl`

- **界面标签**：加密方式（SSL）
- **含义**：见 `smtp_use_tls`。
- **影响与风险**：同上。
- **状态**：在用

### `smtp_from_email`

- **界面标签**：发件人邮箱
- **含义**：邮件显示的发件地址。
- **影响与风险**：为空则邮件功能视为"未配置"。
- **状态**：在用

### `smtp_from_name`

- **界面标签**：发件人名称
- **含义**：邮件显示的发件人名。默认 "Aether"，运行时兜底是 "Arca"。
- **影响与风险**：同时是邮件应用名取值链的最后一环。建议显式设置。
- **状态**：在用 · 品牌残留

### `email_suffix_mode`

- **界面标签**：后缀限制模式
- **含义**：`none` 不限制 / `whitelist` 仅允许列出的邮箱后缀 / `blacklist` 拒绝列出的后缀。
- **影响与风险**：白名单里没有自己公司的域名会拦住所有注册。匹配的是 `@` 后的精确域名（小写）；列表为空视为不限制。
- **状态**：在用

### `email_suffix_list`

- **界面标签**：邮箱后缀列表
- **含义**：逗号分隔，例如 `gmail.com, outlook.com`。
- **影响与风险**：同上。
- **状态**：在用

## 请求记录

控制台「系统设置 → 请求记录」。以已提交代码为准（`failures` 级别尚未提交，不要当成已上线）。

### `request_record_level`

- **界面标签**：记录详细程度
- **含义**：界面选项 `basic` / `headers` / `full`。**只有精确的 `full` 才会保存请求/响应正文**；`headers` 和任何拼写错误都按 `basic` 处理。旧名 `request_log_level` 作为后备别名。
- **影响与风险**：`full` 会存提示词和响应（可能含个人信息），并把流式缓冲提高到每流最多 64 MiB（basic 约 5 MiB）。OAuth 令牌交换的流量永远不抓。请求头里只有凭据类（Authorization、Cookie、x-api-key 等）被写成 `[redacted]`，其它头（含自定义头）原样保存（已实测）；`docs/operations/usage-header-capture.md` 说"不脱敏"，已与代码不符。写入不校验，拼错等于 basic。30 秒缓存。排查上游兼容问题时只临时打开。
- **状态**：在用

### `sensitive_headers`

- **界面标签**：敏感请求头
- **含义**：界面写"这些请求头会被脱敏"。
- **影响与风险**：**代码里没有读取方**：真正的脱敏用的是写死的凭据类请求头列表（authorization、cookie、set-cookie、x-api-key、api-key、x-goog-api-key 等），改这个设置没有任何效果；想让某个自定义头也被脱敏，目前做不到。
- **状态**：无读取方

### `max_request_body_size`

- **界面标签**：—
- **含义**：旧版请求体大小上限。
- **影响与风险**：无读取方，被测试明确忽略；完整抓取不设上限。
- **状态**：无读取方

### `max_response_body_size`

- **界面标签**：—
- **含义**：旧版响应体大小上限。
- **影响与风险**：同上。
- **状态**：无读取方

## 数据保留与清理

控制台「系统设置 → 请求记录清理策略」。用量清理每天 03:00 运行（`APP_TIMEZONE` 时区），设置在运行时读取，**改动在下次运行时生效**。手动"立即清理"不受 `enable_auto_cleanup` 影响。数值非法时静默回退到默认值。

### `enable_auto_cleanup`

- **界面标签**：启用自动清理
- **含义**：总开关：用量、审计、请求候选、代理指标的自动清理都受它控制。
- **影响与风险**：关闭后表会无限增长。界面开关立即保存。
- **状态**：在用

### `detail_log_retention_days`

- **界面标签**：详细记录保留天数
- **含义**：超过后压缩 body 字段。默认 7。
- **影响与风险**：设为 0 会在下次运行时把所有正文压缩。也是候选记录保留天数的兜底值。
- **状态**：在用

### `compressed_log_retention_days`

- **界面标签**：压缩记录保留天数
- **含义**：超过后删除 body 字段。默认 30。
- **影响与风险**：正文不可恢复。
- **状态**：在用

### `header_retention_days`

- **界面标签**：请求头保留天数
- **含义**：超过后清空 headers 字段。默认 90。
- **影响与风险**：—
- **状态**：在用

### `log_retention_days`

- **界面标签**：请求记录保存天数
- **含义**：超过后删除整条记录。默认 365。
- **影响与风险**：用量记录没有最小下限：设为 0 会在下次 03:00 删除所有早于"现在"的记录，请格外小心。
- **状态**：在用

### `cleanup_batch_size`

- **界面标签**：每批次清理记录数
- **含义**：单次清理批大小，避免长时间占用数据库。默认 1000。
- **影响与风险**：过大会长时间锁表。也是候选和代理指标批大小的兜底值。
- **状态**：在用

### `audit_log_retention_days`

- **界面标签**：审计日志保留天数
- **含义**：超过后删除审计日志。默认 30。
- **影响与风险**：有效最小值 7 天。属于安全证据，谨慎缩短。
- **状态**：在用

### `request_candidates_retention_days`

- **界面标签**：候选记录保留天数
- **含义**：清理 request_candidates（每次对上游尝试的记录）。默认 30。
- **影响与风险**：有效最小值 3 天。从未保存过时，运行时实际回退到 `detail_log_retention_days`（7），不是界面显示的 30。故障转移和诊断依赖这些记录。
- **状态**：在用

### `request_candidates_cleanup_batch_size`

- **界面标签**：候选记录清理批次
- **含义**：候选记录大表的单批删除数。默认 5000。
- **影响与风险**：未保存时回退到 `cleanup_batch_size`。
- **状态**：在用

### `proxy_node_metrics_1m_retention_days`

- **界面标签**：代理 1m 指标保留天数
- **含义**：代理节点分钟级指标保留。默认 30，有效范围 1–365。
- **影响与风险**：—
- **状态**：在用

### `proxy_node_metrics_1h_retention_days`

- **界面标签**：代理 1h 指标保留天数
- **含义**：小时级聚合保留。默认 180，不能短于 1m 的保留天数，上限 1095。
- **影响与风险**：—
- **状态**：在用

### `proxy_node_metrics_cleanup_batch_size`

- **界面标签**：代理指标清理批次
- **含义**：单批删除数，限制在 1–50000。默认 5000。
- **影响与风险**：—
- **状态**：在用

### `enable_db_maintenance`

- **界面标签**：（无界面）
- **含义**：每周日 05:00 对 usage、request_candidates、audit_logs 做 `VACUUM ANALYZE`。默认开启。
- **影响与风险**：关闭会导致表膨胀。
- **状态**：在用 · 无界面

## 定时任务与 OAuth 刷新

控制台「系统设置 → 定时任务」。

### `enable_provider_checkin`

- **界面标签**：Provider 自动签到
- **含义**：每天自动执行已配置了运维集成的供应商签到（并发 3）。
- **影响与风险**：没配置供应商运维集成则没有效果。
- **状态**：在用

### `provider_checkin_time`

- **界面标签**：签到时间
- **含义**：`HH:MM`，默认 `01:05`；格式无效时回退到默认。
- **影响与风险**：计划每轮只读一次，**新时间要等旧时间点跑完一轮后才生效**（或重启）。
- **状态**：在用

### `enable_oauth_token_refresh`

- **界面标签**：OAuth Token 自动刷新
- **含义**：后台每 60 秒检查，主动刷新 120 秒内即将过期的 OAuth 密钥。
- **影响与风险**：关闭后不再主动刷新；按需刷新是否保留未验证，过期的 OAuth 密钥可能失败。
- **状态**：在用

## 代理节点

控制台「系统设置 → 网络代理」。

### `system_proxy_node_id`

- **界面标签**：默认代理节点
- **含义**：未单独配置代理的供应商使用的出站代理，覆盖大模型 API 请求、余额查询、OAuth 刷新；不影响系统内部接口。取值顺序：密钥 → 端点 → 供应商 → 这里。`null` 表示直连。
- **影响与风险**：选了离线节点会得到"代理不可用"，可能中断所有上游流量。删除该节点会自动清空此项。
- **状态**：在用

### `external_models_proxy_node_id`

- **界面标签**：（模型目录页）
- **含义**：拉取外部模型目录（models.dev）时使用的代理节点。
- **影响与风险**：**不能**通过通用系统配置接口或 MCP 读写（返回 400），请用模型目录页的配置，或 `arca_models_external_config` / `arca_models_external_set`。
- **状态**：在用 · 受限

## 格式转换、模型后缀与路由开关

### `enable_format_conversion`

- **界面标签**：全局格式转换
- **含义**：开启后，**所有**供应商都被强制允许跨格式请求（覆盖各供应商自己的开关）。
- **影响与风险**：会覆盖"供应商单独关闭"的设置，可能改变行为和成本；写入会清除调度和传输缓存。
- **状态**：在用

### `enable_model_directives`

- **界面标签**：模型后缀参数（模块）
- **含义**：允许用模型名后缀（如 `-high`、`-fast`）覆盖推理参数或服务层级。关闭时 `gpt-5-high` 只是个字面模型名。
- **影响与风险**：开启后配额按**去掉后缀的基础模型名**核算。
- **状态**：在用

### `model_directives`

- **界面标签**：推理参数映射
- **含义**：按 API 端点配置后缀及其请求参数映射。`null` 使用默认。
- **影响与风险**：格式错误会 400。
- **状态**：在用

### `cyber_continue_failover`

- **界面标签**：（路由策略页）
- **含义**：系统设置键本身**没有读取方**。真正生效的是路由策略里的"Cyber继续转移"（响应开始前遇到 Cyber Policy 错误时是否继续故障转移）。
- **影响与风险**：改系统键无效；要改请到「路由策略」页或用 `arca_routing_update`。
- **状态**：无读取方（旧键）

## 通知与推送模块

控制台「模块管理」下的「通知服务」「Server 酱推送」「Bark 推送」。模块启用 API 会检查环境可用性和配置有效性，但直接写系统配置或 MCP 写入**不会**做这些检查。目前只有 `provider_quota_alert` 有触发来源。

### `module.important_notification.enabled`

- **界面标签**：启用通知服务
- **含义**：通知服务总开关。
- **影响与风险**：关闭则不发任何告警。
- **状态**：在用

### `module.important_notification.email_enabled`

- **界面标签**：邮件渠道
- **含义**：是否通过邮件发送通知；需要启用、收件人非空、SMTP 可用。
- **影响与风险**：—
- **状态**：在用

### `module.important_notification.email_recipients`

- **界面标签**：管理员收件人
- **含义**：收件邮箱，字符串或数组（≤128）。
- **影响与风险**：邮箱以明文存储。
- **状态**：在用

### `module.important_notification.default_channel`

- **界面标签**：全局推送服务
- **含义**：`all` / `email` / `server_chan` / `bark`。
- **影响与风险**：—
- **状态**：在用

### `module.important_notification.items`

- **界面标签**：通知项
- **含义**：内置三项：`provider_quota_alert`（号池额度不足）、`provider_pool_abnormal`（号池异常）、`user_balance_low`（用户余额不足）。每项可单独指定渠道和模板。
- **影响与风险**：后两项没有找到触发来源；`user_balance_low` 属于已停用的钱包功能。最多 128 项，模板 ≤8000 字符。
- **状态**：在用（部分无触发）

### `module.server_chan_push.enabled`

- **界面标签**：启用 Server 酱推送
- **含义**：Server 酱渠道开关。
- **影响与风险**：—
- **状态**：在用

### `module.server_chan_push.send_key`

- **界面标签**：SendKey
- **含义**：Server 酱密钥。
- **影响与风险**：敏感，读取显示已设置。固定请求 `sctapi.ftqq.com`。
- **状态**：在用

### `module.server_chan_push.template`

- **界面标签**：模板内容
- **含义**：推送模板，变量 `{title}`、`{body}`。
- **影响与风险**：≤256 KiB。
- **状态**：在用

### `module.bark_push.enabled`

- **界面标签**：启用 Bark 推送
- **含义**：Bark 渠道开关。
- **影响与风险**：—
- **状态**：在用

### `module.bark_push.device_key`

- **界面标签**：Device Key
- **含义**：Bark 设备密钥。
- **影响与风险**：敏感，且与 `server_url` 绑定：改了服务器地址必须重新填 Key。
- **状态**：在用

### `module.bark_push.server_url`

- **界面标签**：服务器地址
- **含义**：官方或自建 Bark 服务，默认 `https://api.day.app`，保存时去掉末尾斜杠。
- **影响与风险**：非 https 或内网地址默认被拦截，局域网自建需要环境变量 `AETHER_BARK_ALLOW_HTTP` / `AETHER_BARK_ALLOW_PRIVATE_TARGETS` 并重启。
- **状态**：在用

### `module.bark_push.template`

- **界面标签**：模板内容
- **含义**：Bark 推送模板。
- **影响与风险**：—
- **状态**：在用

## 敏感信息保护 (PII 脱敏)

控制台「敏感信息保护」页。发给供应商前把聊天消息里的敏感信息换成占位符，返回客户端前还原。需要**两道开关**：系统开关 `module.chat_pii_redaction.enabled`，加上用户或密钥各自的"敏感信息保护"设置（默认关）。支持 OpenAI Chat、Responses、Claude Messages。映射缓存在 Redis。

### `module.chat_pii_redaction.enabled`

- **界面标签**：启用敏感信息保护
- **含义**：系统级总开关。
- **影响与风险**：约 5–35 秒生效。开启后上游请求会强制 `accept-encoding: identity`。
- **状态**：在用

### `module.chat_pii_redaction.rules`

- **界面标签**：替换类型配置
- **含义**：12 条系统预置规则（邮箱、手机号、证件、银行卡、IP、API Key、令牌、JWT 等）加自定义规则，每条是正则。
- **影响与风险**：正则必须能编译。`api_key` 等宽泛规则可能过度替换；MCP 会把含 `sk-` 的正则脱敏显示。
- **状态**：在用

### `module.chat_pii_redaction.cache_ttl_seconds`

- **界面标签**：多轮上下文缓存
- **含义**：只能是 300（5 分钟，默认）或 3600（1 小时）。
- **影响与风险**：越长，长对话里占位符越稳定，Redis 状态越多。
- **状态**：在用

### `module.chat_pii_redaction.placeholder_prefix`

- **界面标签**：占位符前缀
- **含义**：供应商侧看到的占位符前缀，仅字母数字下划线，保存时转大写。默认 "AETHER"。
- **影响与风险**：供应商会在提示词里看到这个前缀。
- **状态**：在用 · 品牌残留

## S3 备份

控制台「S3 备份」页。后台每分钟检查一次，到点（`APP_TIMEZONE` 时区）执行。备份先 zstd 压缩，再用**网关加密密钥**做 AES-256-GCM 加密（`*.json.zst.aes256gcm`）。**丢失加密密钥 = 备份无法恢复。** 启用时校验：endpoint 必须是 https 且不含凭据、查询串或片段。

### `backup_s3_enabled`

- **界面标签**：自动备份
- **含义**：自动备份总开关。
- **影响与风险**：配置不完整时每次运行记录 `config_invalid` 并跳过。
- **状态**：在用

### `backup_s3_scope`

- **界面标签**：备份范围
- **含义**：`config`（配置数据）/ `users`（用户数据）/ `data`（完整备份，默认）。
- **影响与风险**：`users`/`data` 包含用户记录。
- **状态**：在用

### `backup_s3_endpoint`

- **界面标签**：Endpoint
- **含义**：S3 兼容存储的 https 地址。
- **影响与风险**：必须 https，无凭据。
- **状态**：在用

### `backup_s3_region`

- **界面标签**：Region
- **含义**：默认 `auto`。
- **影响与风险**：—
- **状态**：在用

### `backup_s3_user_agent`

- **界面标签**：User-Agent
- **含义**：默认 `rclone/v1.68.0`，用于兼容部分对象存储。
- **影响与风险**：—
- **状态**：在用

### `backup_s3_bucket`

- **界面标签**：Bucket
- **含义**：存储桶名。
- **影响与风险**：界面占位符仍是 `aether-backups`。
- **状态**：在用 · 品牌残留

### `backup_s3_prefix`

- **界面标签**：Prefix
- **含义**：对象前缀，默认 `aether/backups/`。
- **影响与风险**：不能含空段、`.`、`..` 或反斜杠。默认前缀和文件名主干仍是 Aether 命名。
- **状态**：在用 · 品牌残留

### `backup_s3_access_key_id`

- **界面标签**：Access Key ID
- **含义**：访问密钥 ID。
- **影响与风险**：导出配置时被省略。
- **状态**：在用

### `backup_s3_secret_access_key`

- **界面标签**：Secret Access Key
- **含义**：访问密钥。
- **影响与风险**：敏感，读取显示已设置。
- **状态**：在用

### `backup_s3_path_style`

- **界面标签**：（无明显标签）
- **含义**：是否使用 path-style 寻址，默认 true。
- **影响与风险**：部分存储商选错会 403/404。
- **状态**：在用

### `backup_s3_compression`

- **界面标签**：压缩格式
- **含义**：目前只支持 `zstd`。
- **影响与风险**：其它值会让备份失败。
- **状态**：在用

### `backup_s3_schedule_unit`

- **界面标签**：周期单位
- **含义**：`hours` / `days` / `weeks` / `months`。
- **影响与风险**：—
- **状态**：在用

### `backup_s3_schedule_interval`

- **界面标签**：周期间隔
- **含义**：≥1。
- **影响与风险**：相位按纪元对齐，不是"启用后过 N 个周期"。
- **状态**：在用

### `backup_s3_schedule_minute`

- **界面标签**：执行分钟
- **含义**：0–59。
- **影响与风险**：—
- **状态**：在用

### `backup_s3_schedule_hour`

- **界面标签**：执行小时
- **含义**：0–23，默认 3。
- **影响与风险**：—
- **状态**：在用

### `backup_s3_schedule_weekday`

- **界面标签**：星期
- **含义**：1–7，周一=1。
- **影响与风险**：—
- **状态**：在用

### `backup_s3_schedule_month_day`

- **界面标签**：每月日期
- **含义**：1–31。
- **影响与风险**：设为 29–31 会跳过较短的月份。
- **状态**：在用

### `backup_s3_retention_count`

- **界面标签**：最多保留备份数
- **含义**：默认 7。
- **影响与风险**：**只统计"超出的备份数"，不会删除任何对象**（唯一的删除来自旧版明文备份的迁移）。请用存储桶的生命周期规则做过期清理。
- **状态**：在用 · 仅计数

### `backup_s3_last_slot`

- **界面标签**：（内部）
- **含义**：调度去重用的内部状态。
- **影响与风险**：不要手动修改。
- **状态**：内部

## 不在默认值表中的键

这些键由代码读取，但不在 `admin_system_config_default_value` 的默认值列表里（未保存时按各自的代码默认值）。

### `internal_access_policy`

- **界面标签**：用户权益（页面）
- **含义**：**用户分组限额的核心配置**：`{timezone, default, groups{id→分配}, users{id→分配}, model_families{id→{name, models[]}}}`。分配 = `{unlimited, five_hour_budget_usd, weekly_budget_usd, monthly_budget_usd, models{…}, families{…}}`。个人分配**替换**分组分配；多个分组取最宽松的；都没有则用 `default`。窗口：最近 5 小时滚动，加按配置时区计的日/周（周一起）/月。没有预算规则的分配表示不限，不是封锁。未知字段会被拒绝，`unlimited` 不能和预算同时出现，每个分配最多 15 个子池、总共最多 32 个模型族。
- **影响与风险**：写入**必须带 `expected_value`**（上次读到的值，CAS），否则 428/409；不允许删除（405）。立即生效，无缓存。请用 `arca_benefits_get` / `arca_benefits_preview` / `arca_benefits_set`，先预览再保存。

### `enable_registration`

- **界面标签**：开放用户注册
- **含义**：允许新用户自助注册。默认关闭（未设置时为 false）。
- **影响与风险**：开启后任何能访问控制台的人都能自助创建用户，并加入 `default_user_group_id` 对应分组（决定其额度）。

### `default_user_group_id`

- **界面标签**：（用户分组页）
- **含义**：自助注册用户加入的分组；未设置时用内置默认分组（若存在）。
- **影响与风险**：指向已删除的分组会让注册报错。设置它还会把没有其它分组的非管理员用户加入该组。写入会清除认证缓存。请在用户分组页操作。

### `require_email_verification`

- **界面标签**：需要邮箱验证
- **含义**：注册时必须验证邮箱。默认 false。
- **影响与风险**：开了但 SMTP 没配好，没人能注册。

### `rate_limit_per_minute`

- **界面标签**：默认速率限制 (请求/分钟)
- **含义**：未单独配置的用户和独立密钥共用的每分钟请求上限；0 = 不限（默认）。
- **影响与风险**：太低会让所有人 429。约 15–30 秒生效；运行时状态异常时默认"失败即拒绝"，除非环境变量 `RATE_LIMIT_FAIL_OPEN=true`。

### `registration_privacy_policy_enabled`

- **界面标签**：注册隐私政策确认
- **含义**：开启后注册必须确认当前版本的隐私政策。另有 `registration_privacy_policy_version`（默认 "1"）、`_format`（markdown/html）、`_content`。
- **影响与风险**：版本变化会要求重新确认；选 html 时内容原样展示给用户，只粘贴可信 HTML。

### `referral_enabled`

- **界面标签**：邀请返利（已停用）
- **含义**：旧版返利配置，另有 `referral_reward_mode`、`referral_recharge_percent`、`referral_headcount_amount_usd`、`referral_headcount_trigger`。
- **影响与风险**：内部模式下直接返回空，界面也隐藏，写入无效果。

### `email_app_name`

- **界面标签**：（无界面）
- **含义**：邮件里的应用名，优先于 `site_name`。
- **影响与风险**：只能通过原始 API / MCP 设置。

### `enable_stats_aggregation`

- **界面标签**：（无界面）
- **含义**：每日/每小时统计聚合开关，默认开。
- **影响与风险**：关闭后仪表盘统计不再更新。

### `pending_request_timeout_minutes`

- **界面标签**：（无界面）
- **含义**：处于 pending 超过 N 分钟的用量记录被标记为失败，默认 10，每 5 分钟检查。
- **影响与风险**：太小会把长流式请求误标失败。

### `module.ldap.enabled / module.oauth.enabled`

- **界面标签**：LDAP 认证 / OAuth 登录
- **含义**：两个登录模块的启用开关，具体配置在专用页面和表里。同时受环境变量 `LDAP_AVAILABLE` / `OAUTH_AVAILABLE` 约束。
- **影响与风险**：请用模块开关（会校验配置），别直接改原始键。

### `module.management_tokens.enabled / module.proxy_nodes.enabled / module.gemini_files.enabled`

- **界面标签**：访问令牌 / 代理节点 / 文件缓存
- **含义**：只用于模块状态展示，没有找到实际的门控读取方。
- **影响与风险**：—

### `module.payment_gateways.enabled / module.referral.enabled`

- **界面标签**：支付配置 / 邀请返利（已停用）
- **含义**：商业功能模块开关。
- **影响与风险**：内部模式下无效；前端过滤用的名字与后端不一致，「支付配置」卡片可能仍出现（未在运行界面核实）。

### `module.antigravity.bearer_bridge`

- **界面标签**：（无界面）
- **含义**：用 Google bearer 令牌映射到固定用户和密钥，认证 `antigravity:v1internal` 请求。
- **影响与风险**：**安全敏感**：`allow_unverified_google_bearer=true` 且白名单为空时，任何 bearer 都会通过。写入会清除认证缓存。

### `default_provider / default_model / enable_usage_tracking`

- **界面标签**：旧版设置
- **含义**：旧版 `/api/admin/system/settings` 的字段。
- **影响与风险**：运行时没有读取方；`enable_usage_tracking=false` 并不会停止用量记录。

## 待补

（暂无。新增的键先在这里用反引号列出，再补完整条目。）
