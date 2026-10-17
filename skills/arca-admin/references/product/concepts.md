# 核心概念

> 依据：已提交代码和仓库文档（核对于 2026-10-18，HEAD `266c45ed1`）。**多数条目来自读代码，没有在运行界面逐项验证**；标"实测"的是在运行的网关上复现过的。涉及生产状态的问题以实时读取为准。

## 0. 产品边界（先讲清楚，避免答错）

- Arca 是**团队内部**网关（Aether 的分支）。**钱包、充值、支付、推荐返利、套餐购买已停用**，对应接口在内部模式下返回 404。限额靠**按用户/分组的美元预算**。
- 内部模式开关 `ARCA_INTERNAL_MODE`，默认开启；关闭只是遗留的逃生口，不是受支持的部署方式。
- 受支持的客户端协议：`/v1/responses`（含 Codex）、`/v1/messages`（Claude）、`/v1/chat/completions`。Gemini 原生生成接口（`/v1beta/*`、`:generateContent`）返回 404（实测）；Gemini 仍可作为**上游**格式。
- 界面里仍能看到的已停用残留：基础配置里的"默认用户初始赠款"、"邀请返利"，通知项 `user_balance_low`，部分模块卡片。不要把它们讲成可用功能。
- 成熟度：`docs/ARCA.md` 和 README 写明内部模式、反馈、管理 MCP 仍在开发/验收阶段，没有在生产验证。被问到"能不能上生产"时如实这样说。

## 1. 供应商、端点、密钥、模型映射、公开模型

| 对象 | 含义 | 关键字段 |
| --- | --- | --- |
| 供应商 Provider | 一个上游厂商/账号族：OpenAI 兼容、Claude、Gemini，或模板（`codex`、`claude_code`、`gemini_cli`、`antigravity`、`vertex_ai`、`kiro`、`opencode_go`、自定义…） | `provider_priority`（数字小的先试）、`is_active`、`enable_format_conversion`、`keep_priority_on_conversion`、`concurrent_limit`、`max_retries`、`proxy`、`config`（含 `failover_rules`、`pool_advanced`…） |
| 端点 Endpoint | 一种接口 = (`api_format`, `base_url`) | `api_format`（`openai:chat`、`openai:responses`、`claude:messages`、`gemini:generate_content`、`openai:embedding`…）、`custom_path`、`header_rules`、`body_rules`、`format_acceptance_config`、`proxy` |
| 上游密钥 | 供应商上的一份凭据 | `auth_type`、加密的密钥、`api_formats`、`allowed_models`（null=全部，`[]`=无，列表=仅这些）、`internal_priority`、`rpm_limit`、`concurrent_limit`、健康/熔断状态 |
| 供应商模型 | 供应商真实的模型名，关联到公开模型 | `provider_model_name`、`provider_model_mappings[]`、价格覆盖、`is_active`、`is_available` |
| 公开模型 Global model | 客户端使用的名字 | `name`、`display_name`、`is_active`、`default_tiered_pricing`、`supported_capabilities`、`config.model_mappings`（正则别名） |

候选（candidate）= 对一次请求留下来的 (供应商, 端点, 密钥, 供应商模型名) 组合。控制台页面：供应商 `/admin/providers`、模型 `/admin/models`、调度策略 `/admin/routing`、号池 `/admin/pool`。写权限：`service_admin`、`admin`；`user_admin`、`audit_admin` 只读。

### 请求怎样选到上游

1. **鉴权和本地权限**：密钥是否有效、未锁定、未过期，供应商族、API 格式、模型是否被允许。失败直接 401/403，不进入路由。
2. **选路由策略**：请求头 `x-aether-scheduler-group` → 密钥的默认绑定 → 用户的默认绑定 → 用户所在分组的默认绑定 → 系统默认策略。没有任何绑定就直接用系统默认。找不到启用的策略返回 503「no enabled routing strategy is configured for this request」。
3. **解析策略**：从 `default_policy` 开始，叠加匹配的 `model_policies`，再按 (priority, id) 顺序应用启用的 `rules`（`restrict_models`、`restrict_providers`、`restrict_keys`、`set_scheduling`、`set_provider_priority`、`json_patch_body`、`patch_headers`…）。`restrict_models` 不匹配返回 400「requested model is not allowed by routing policy」。
4. **枚举候选**：密钥的 `api_formats` 要覆盖端点格式；供应商、端点、密钥、模型都启用；流式请求要求支持流式；模型名要能通过公开名、供应商模型名、映射名或正则别名解析；上游密钥 `allowed_models` 过滤；用户/密钥的供应商、格式、模型限制。
5. **跨格式检查**：客户端格式与候选格式不同时，只有开了格式转换才保留（见第 2 节），否则跳过原因是 `format_conversion_disabled`。
6. **运行时过滤**（跳过原因）：供应商额度、账号额度耗尽、OAuth 无效、供应商/密钥并发、`key_circuit_open`、健康分 0、`key_rpm_exhausted`；号池另有 `pool_cooldown`、`pool_cost_limit_reached`、`pool_account_blocked` 等。
7. **排序**后依次执行，带故障转移。

### 排序与调度旋钮（策略的 `default_policy`，在 `/admin/routing` 编辑）

| 字段 | 默认 | 含义 |
| --- | --- | --- |
| `priority_mode` | `provider` | `provider`：先按供应商优先级再按密钥内部优先级；`global_key`：按密钥的 `global_priority_by_format`。数字小的优先 |
| `scheduling_mode` | `cache_affinity` | `cache_affinity` 同一用户密钥持续命中同一上游密钥以保持提示缓存（亲和键含 API Key、格式、模型，TTL 300 秒）；`load_balance` 负载均衡；`fixed_order` 固定顺序 |
| `keep_priority_on_conversion` | false | 格式不同的候选默认被排到同格式候选后面；设为 true（策略或供应商任一）则不降级。**只影响顺序，不决定是否允许转换** |
| `sticky_key_attempts` | 2 | 在第一个候选上总共尝试几次再换（界面"错误重试次数"）；0 或 1 表示不在同一密钥上重试 |
| `max_transfer_count` | 0（不限） | 最多切换几次 (供应商, 端点, 密钥)；同候选重试和被跳过的候选不计数 |
| `max_transfer_timeout_seconds` | 0（不限） | 从首次尝试起累计的最长时间；只在准备下一次尝试时检查，不会打断正在进行的调用 |
| `cancel_on_client_disconnect` | false | 见下 |
| `cyber_continue_failover` | false | 响应开始前遇到 Cyber Policy 拒绝时是否继续故障转移（界面"Cyber继续转移"） |
| `enable_cf_heartbeat` | false | 同步请求保持外层 HTTP 200，把错误放在响应体里；开启后 HTTP 状态不再是失败信号 |

- 预演：`arca_routing_preview`（`POST …/dry-run`）用合成请求评估，不发推理。发布：`arca_routing_publish` 生成新版本。绑定：`subject_type` 为 `user` / `api_key` / `user_group`，字段 `is_default`、`allow_explicit_select`。

### 故障转移规则

- `failover_rules.success_failover_patterns`：只对 HTTP 200 且响应体匹配的情况触发转移。`error_stop_patterns`：对 400–599 停止转移；同时有状态码和正则则两者都要匹配。正则是 Rust 语法，最多 64 条、每条 ≤4096 字节。全局规则先于供应商规则。
- 传输错误（连接/TLS/DNS/超时）默认继续转移，除非供应商设置 `stop_on_transport_errors`。
- **流式 200 的恢复窗口**：网关会缓冲协议前导事件；在**第一段真实内容发出前**出现结构化错误、提前 EOF、首字节超时或命中 200 正则，仍会转移。内容一旦开始发送，后面的错误就是终态，HTTP 状态不能再变。
- 客户端 4xx 默认也会故障转移（`openai:search` 除外）。
- 诊断事件：`routing_transfer_limit_reached`、`provider_transfer_limit_reached`、`local_stream_candidate_retry_scheduled`。详见 `docs/operations/routing-failover.md`。

### 客户端断开 `cancel_on_client_disconnect`

- `false`（默认）：客户端断开后请求继续跑完并正常结算。**取消的请求仍可能计费**（已实测：上游流被读完）。
- `true`：立即中断上游（实测约 2 秒内），不再计 token 费用。取消的请求会记为 499 `cancelled`，但计费状态可能仍是 `settled`。
- 按策略设置；Responses WebSocket 只会让进行中的那一轮结束。详见 `docs/operations/client-disconnect-policy.md`。

## 2. 格式转换

- 跨格式转换覆盖四种标准格式：`openai:chat`、`openai:responses`、`claude:messages`、`gemini:generate_content`。其它格式（`openai:responses:compact`、`openai:realtime`、`codex:live`、`openai:search`、embedding/rerank 等）不跨族转换。
- **同格式**是直通：JSON 字段原样复制，再套模型覆盖、body 规则、模型后缀和流策略。**跨格式**是严格的、失败即拒绝：不支持或有损的字段会报错，不会静默丢弃（例如 Chat→Responses 会拦截 `n`、`stop`、`seed`、`logprobs`…）。
- **三者任一**就开启跨格式转换：① 供应商开关 `enable_format_conversion`（默认 false）；② 全局系统设置 `enable_format_conversion`（强制所有供应商开）；③ 端点的 `format_acceptance_config`（`{enabled, accept_formats, reject_formats}`，reject 优先）。都没开则候选被跳过，用户看到「没有可用提供商支持模型 X…」。
- **模型后缀**：`enable_model_directives` 开启后 `-high`、`-low`、`-xhigh`、`-max`、`-fast` 等后缀会被剥掉用于匹配，并映射成推理强度或服务层级；关闭时 `gpt-5-high` 只是个不存在的模型名。配额按去掉后缀的基础模型核算。
- **排查**：请求详情的"失败诊断"面板可以复制 schema v2 JSON（依赖已抓取的正文，即 `request_record_level=full`）；请求诊断的 `conversion` 阶段在转换失败时为 `failed/format_conversion_failed`。参考文档：`docs/api/format-*.md`、`docs/operations/conversion-failure-diagnostics.md`（`format-conversion-audit.md` 写于 2026-06-03，可能比代码旧）。
- **离线兼容性工作台**（`/admin/compatibility`）：跑真实的转换库在内置合成数据上，**只是离线合成证据**。不发 HTTP 请求、不用密钥、不启动客户端，所以不能说明模型质量、供应商可用性或真实 Codex/Claude Code 兼容性；`cancel`、`compression`、`real_client`、`real_model` 永远是"未测试"。真实客户端证据来自单独的本地套件 `tests/client_e2e/run.py`，没有接进工作台。

## 3. 额度、分组、模型权限、角色

### 额度（用户权益，`/admin/access-policies`，系统配置键 `internal_access_policy`）

- 全部是**美元**，不是请求数；遗留的 `*_requests` 键会被拒绝。省略某个上限 = 不限；`0` 是非法值不是"不限"；`unlimited:true` 不能和任何预算同时出现。
- 窗口：最近 5 小时（滚动，18000 秒）、按配置时区的日/周（周一起）/月。总额度支持 5 小时/周/月；子额度（模型、模型族）另支持日额度。每个有效分配最多 64 个窗口，超出则失败即拒绝。
- **生效的分配**（按顺序）：① `users` 里的个人条目**整体替换**所有继承（不是合并；个人省略的子额度也不回退到分组）；② 否则看用户所在分组：总额度逐窗口取**最大值**，任一分组 `unlimited` 则不限，子额度取并集、同模型同窗口取更大；③ 否则用 `default`；都没有则没有任何额度规则 = **不限**（`docs/ARCA.md` 说"未分配用户失败即拒绝"，与代码和 `internal-gateway.md` 不符，以代码为准，已实测：没配策略的用户不受限）。**建议配置一个 `default` 预算。**
- 子额度**计入**总额度，不是额外额度；一次请求要同时满足所有适用窗口。模型族 = 一组精确公开模型名，不是前缀匹配；成员重叠则所有匹配的池都得放行。
- 额度**不授予模型权限**，移除子额度也不会收回权限；`unlimited` 只豁免额度，不豁免鉴权、模型权限、IP、限速、并发。
- 内部模式下额度对**所有主体**生效，包括管理员和独立密钥（按所属用户计）；想让管理员不受限要显式给 `unlimited:true`。
- 核算：请求前**预估并预留**（待处理且未过期的预留也计入，预留 TTL 24 小时），完成后按实际费用结算。缺少可用定价或结算存储不可用时对有预算的用户**失败即拒绝**，表现为 500。
- 写入必须带 `expected_value`（CAS），冲突 409，不允许 DELETE。预览不保存：`arca_benefits_preview`。用户看自己的：`GET /api/users/me/entitlements`。工具：`arca_benefits_get` / `_preview` / `_set`。详见 `docs/operations/entitlement-preview.md`、`model-family-ui.md`、`internal-gateway.md`。

### 模型权限 ACL（与额度分开）

- 在用户（`/admin/users`）、分组、单个密钥上设置。存储为模式 + 列表：`inherit`、`unrestricted`、`specific`、`deny_all`。API/MCP 的 `allowed_models`：`null` → inherit，`[]` → deny_all，列表 → specific。
- 内部模式用户解析：个人 `specific` → 该列表；个人 `deny_all` → 空；其它个人模式 → 各分组策略的并集。分组并集：任一分组 `unrestricted` → 不限；`specific` 取并集；`deny_all` 不增加；`inherit` 忽略；**不属于任何分组的用户 = 不限**。
- 请求时放行条件：在列表里，或其后缀基础模型在（后缀功能开启时），或经别名解析到允许的公开模型。否则 403。
- 密钥限制：用户的密钥与用户策略取**交集**，只能收窄；**独立密钥**（`/admin/keys`）上 `[]` 表示**不限**（与用户上的 `[]` 相反）。
- 内部模式下供应商和格式策略归分组所有：用户更新里同时带 `allowed_models` 和 `allowed_providers` 会 400，请在分组或密钥上设供应商/格式限制。
- 旧数据里的 JSON `null` 会报「contains JSON null; use SQL NULL for an unset policy」，需要迁移 `20260908000000_normalize_legacy_policy_nulls.sql`（见 `docs/operations/legacy-policy-null-upgrade.md`）。

### 角色

| 角色 | 标签 | 能做 |
| --- | --- | --- |
| `admin` | 超级管理员 | 一切；只有它能指派管理员角色、管理访问令牌、改全局认证/安全设置、整站导入导出、系统设置、模块、LDAP/OAuth |
| `audit_admin` | 审计管理员 | 所有普通读取；无写入、不能查看凭据。例外：反馈创建/回复/状态（`admin:feedback:write`）；可读兼容性工作台 |
| `user_admin` | 用户及权益管理员 | 用户、分组、权益、用户模型权限、独立密钥、公告；模型/供应商只读 |
| `service_admin` | 服务管理员 | 供应商、端点、上游密钥/模型、号池、路由、代理/隧道、异步任务、自适应并发；可运行兼容性工作台 |

- 管理令牌权限 = 令牌 scope ∩ 用户当前角色。被委派的管理员不能降级、停用、删除或重置其他管理员；最后一个超级管理员受保护（「不能降级或停用最后一个管理员账户」）。
- 角色拆分迁移 `20261015000200_split_admin_roles.sql` 要先执行才能指派新角色。

## 4. 模型目录

- 外部目录 = models.dev 快照，**只有显式刷新才会创建或替换**（模型对话框里的刷新按钮 → `POST /api/admin/models/external/refresh`；MCP `arca_models_external_refresh`）。首次刷新前 `GET /api/admin/models/external` 返回 503；没有按时间过期。快照存系统配置 `internal.external_models_catalog_snapshot`（对列表隐藏）加运行时缓存，重启后保留。
- 刷新先抓取替换数据（抓取失败保留旧快照），再按每页 200 个更新已有公开模型；渠道价格覆盖、别名、权限、启用状态、自定义设置**不会被重置**；缺失或不支持的价格不会覆盖已有价格。
- 关联/导入：导入供应商模型、保存密钥的允许模型都走同一个导入器（精确 id、带供应商前缀的 id、固定来源，或唯一明确的官方来源，不做模糊匹配）；只增不减，取消选择不会删除。匹配不上的模型用 32,768 上下文 / 4,096 输出的占位值创建，**默认禁用并标记待定价**——未知价格不会当作免费。
- **孤儿清理**：没有任何供应商关联的公开模型会被打上 `config.orphaned_since`，在 `global_model_orphan_retention_days`（默认 30，0 关闭）后仍无关联才删除；每 6 小时扫描一次，重新关联会清除标记。**一个月后"消失"的模型多半是它。**
- 配额子额度键是精确公开模型名，**重命名公开模型不会重命名配额条目和旧账本**。

## 5. 号池、自适应并发、健康、熔断

这些都是**每个上游密钥、每种 API 格式**的状态。查看：健康监控 `/admin/health-monitor`、供应商抽屉、号池 `/admin/pool`、请求详情的候选记录；MCP 工具 `health_summary`、`monitor_circuits`、`pool_overview`、`pool_status`、`stats_errors`。写入需要 `service_admin`/`admin`。

- **健康分**（0–1）：成功置 1.0；计入的失败置 0.75（成功正则触发重试）/0.6（5xx）/0.7（其它），之后每次连续失败再减 0.15，下限 0.2；HTTP 400 和"停止"类不计。分档：<0.5 低、<0.8 降级、其余健康。供应商摘要里的"端点健康度"是该格式下启用密钥分数的平均；灰色/null 表示**没有数据**，不是 100%。它是当前密钥状态，不是窗口成功率或主动探测。
- **熔断**（非号池供应商）：连续失败 8 次打开（原因 `consecutive_failures_8`），被跳过为 `key_circuit_open`；探测间隔加倍，最长 32 分钟，半开探测。号池供应商不用熔断。健康分 0 也会被跳过。**只在确认根因消除后**用 `arca_health_reset`。
- **自适应 RPM**：密钥**没设** `rpm_limit` 时，网关从 429 和响应头学习限额（初始 50、最小 5、最大 10000、每 30 分钟探测 +5、429 后冷却 5 分钟、置信度 >0.6 才执行）。**显式设置 `rpm_limit` 会关闭学习。**
- **并发**：`concurrent_limit` 可设在供应商、上游密钥和调用方 API Key 上。
- **号池**：供应商 `config.pool_advanced` 存在即为号池（很多账号，如 Codex/Kiro 订阅）。分发预设（互斥）：`cache_affinity`、`lru`、`single_account`（单号优先）、`load_balance`；可叠加的预设：`free_first`、`team_first`、`plus_first`、`pro_first`、额度均衡、额度重置优先、优先级优先、健康优先、延迟优先、成本优先。高级设置：自适应热池、账号自检（默认 60 分钟、并发 4）、异常自动清除、跳过额度耗尽账号、429 冷却（默认 300 秒，范围 10–3600）、529 冷却（默认 30 秒，范围 5–600）。单个账号冷却可用 `arca_pool_clear_cooldown` 清除。

## 6. 代理 / 隧道节点

- 代理节点是**供应商流量的出口**（不是客户端入口）：**手动**（HTTP/SOCKS URL，可带加密凭据）或**隧道**（VPS 上的 `aether-tunnel` 代理主动通过 WebSocket 连回网关，不需要入站端口）。页面 `/admin/proxy-nodes`。
- 什么时候需要：供应商端点从网关所在地区不可达或被屏蔽，或要求出口 IP 在某地区。
- **出口选择**（第一个命中的生效）：上游密钥的代理 → 端点代理 → 供应商代理 → 系统默认 `system_proxy_node_id`（null = 直连）。每个 `proxy` 是 `{enabled, node_id | url, mode, label}`，`enabled:false` 会跳过。显式配置的节点或 URL 解析不了会产生"不可用"快照，**不会静默回退直连**。节点必须 `status == online` 且接受新隧道。
- 系统代理影响供应商请求、余额查询、OAuth 刷新，不影响内部接口；隧道本地节点在 `cache_affinity` 排序中优先于远端节点。多网关部署通过 `ARCA_TUNNEL_RELAY_*` 转发，`ARCA_TUNNEL_RELAY_AUTH_SECRET` 必填（≥32 字节，与 JWT/加密密钥不同）。
- 诊断里的 `proxy_error` 对应"隧道"阶段，但**代理传输错误不能证明某个具体节点宕机**。
- **注意**：隧道的 README、安装脚本和一键更新逻辑仍指向上游 `github.com/fawney19/Aether` 的发布。Arca 没有发布隧道版本；`deploy/arca/README.md` 说明不要重新注册现有 agent。让任何人执行这些一键命令之前先提醒。

## 7. 请求记录与诊断（已提交行为）

- `request_record_level`：默认 `basic`，只有精确 `full` 才保存四类正文（请求、发往上游的请求、上游响应、客户端响应）；`headers` 和拼写错误都按 `basic`。OAuth 令牌交换的流量永远不抓。详情见 [settings-glossary.md](settings-glossary.md)。
- 保留：详细记录 7 天后压缩正文，压缩记录 30 天后删正文，请求头 90 天后清空，整条记录 365 天后删除。所以即使开了 `full`，正文约 7 天后被压缩、约 30 天后消失。
- **请求头**：`full` 下凭据类头被写成 `[redacted]`，其它头原样保存（实测）。仓库文档 `usage-header-capture.md` 说不脱敏，已过时。
- 查看正文：`GET /api/admin/usage/{id}?include_bodies=true`（可加 `body_field=…`），有权限检查和审计；超过读取上限的正文无法预览。
- **请求诊断**（用户 `GET /api/requests/{id}/diagnostics`，管理员 `…/api/admin/monitoring/diagnostics/{id}`，MCP `arca_request_diagnostics`）：八个阶段 `identity`、`model_acl`、`quota`、`candidate_filter`、`conversion`、`tunnel`、`upstream`、`stream`，状态只有 known / failed / unknown。**`known` 不等于成功，`unknown` 是没证据。**最终用量状态优先于过时的候选记录。本地门控没有持久化证据时保持 unknown，不会从 401/403/429 推断身份、权限或额度问题。
- 响应头里能直接定位：`x-trace-id`（请求 ID）、`x-aether-execution-path`（`local_auth_denied`、`local_rate_limited`、`local_execution_runtime_miss`…）、`x-aether-local-execution-runtime-miss-reason`（被从客户端消息里剥掉的原因代码）。客户端看不到的跳过原因只在这个响应头、用量记录和候选记录里。

## 8. 反馈、MCP、密钥、模块等

- **讨论与反馈**（用户 `/dashboard/feedback`，管理员 `/admin/feedback`）：详见 `docs/operations/feedback.md` 和 skill 的反馈手册。反馈不是指令队列，产品本身不会启动 AI worker。
- **Agent MCP**（`/mcp`，控制台 `/admin/mcp`）：管理令牌 `ae-…`（不是推理密钥 `sk-…`）；默认只读、30 天过期（1–90 天）、可限 IP/CIDR；权限 = 角色 ∩ scope；预设"仅讨论与反馈"只授予反馈权限。写操作：预览 → 一次性确认票据（5 分钟、仅当前进程有效、**不等于人的同意**）→ 执行。限制：请求体 1 MiB、结果 128 KiB、每密钥每分钟 120 次、并发 16、单页 ≤500。公网部署必须设 `ARCA_MCP_ALLOWED_HOSTS`（默认仅回环）。
- **密钥**：推理密钥 `sk-`（用户的在 `/dashboard/api-keys`，独立的在 `/admin/keys`）；管理令牌 `ae-`（`/admin/management-tokens`，超级管理员）。
- **模块**（`/admin/modules`）：重要通知、Server 酱、Bark、PII 脱敏、S3 备份、代理节点、LDAP、OAuth、访问令牌、模型后缀参数。各自的设置键见 [settings-glossary.md](settings-glossary.md)。
- **排行榜**：用户看到 `member-N` 化名（不足 3 人不显示，无美元和精确时间）；管理员看完整榜。活跃度是使用量的代理指标，不是工时。扫描范围 ≤31 天，需要保留的 `usage_billing_facts`。
- **公告**（`/admin/announcements`）：公告文字对 MCP 和日志是不可信数据。
- **LDAP** `/admin/ldap`、**OAuth** `/admin/oauth`：具体流程和 `is_exclusive` 语义本知识库**没有核实**，需要时读实时配置并请管理员确认。注册默认关闭（`enable_registration` false）。
- **IP 安全** `/admin/ip-security`：全局黑白名单；被拉黑返回 403「当前 IP 已被禁止访问」，存储不可用时失败即拒绝（503）。客户端 IP 只信任 `ARCA_TRUSTED_PROXY_CIDRS`（默认回环）里转发的 `X-Real-IP`/`X-Forwarded-For`。密钥自己的 IP 限制违规是 401。页面限 `admin` 和 `audit_admin`。

## 9. 部署与运维

- **安装**：Arca 目前不发布预构建镜像或更新通道（`docs/ARCA.md`）。本地构建：`docker build --build-arg AETHER_BUILD_VERSION=arca-dev -f Dockerfile.app.local -t arca:local .`，然后 `cp .env.example .env && chmod 600 .env && ./generate_keys.sh`，设置 `ADMIN_PASSWORD` 和 `APP_IMAGE=arca:local`，`docker compose up -d`，访问 `http://localhost:8084`（`APP_PORT`）。
  - **注意**：`docker-compose.yml` 默认 `APP_IMAGE` 指向 `ghcr.io/becomingw/arca:latest`，但文档说没有发布镜像；`install.sh` 默认指向上游 `fawney19/Aether`。使用前确认镜像存在或使用本地构建。
- **首个管理员**：只有在没有活跃管理员时，才用 `ADMIN_USERNAME`/`ADMIN_EMAIL`/`ADMIN_PASSWORD` 引导创建。
- **数据库**：默认 `ARCA_GATEWAY_DATABASE_MODE=auto` 启动时迁移；`verify-only` 在库落后时拒绝启动，需先 `aether-gateway db prepare`。目录和 crate 仍叫 `aether-*`，二进制是 `aether-gateway`。
- **Redis**：自带的 Redis 不持久化。多节点必须共享 Redis（`ARCA_RUNTIME_BACKEND=redis`；`memory` 只适合单进程开发）。Responses→Chat 的续接历史存 Redis 6 小时、每条最多 8 MiB，Redis 重启会丢（除非开 AOF）。
- **环境变量规则**：用 `ARCA_*`。启动时把每个 `ARCA_X` 复制到 `AETHER_X` 和 `AFSSR_X`，同名时 ARCA 优先并打印警告；只有旧名的继续生效并打印弃用警告。无前缀变量（`APP_PORT`、`DATABASE_URL`、`REDIS_URL`、`JWT_SECRET_KEY`、`ENCRYPTION_KEY`、`API_KEY_PREFIX`、`ADMIN_*`、`CORS_*`、`RUST_LOG`、`DB_PASSWORD`、`REDIS_PASSWORD`）不做别名。完整清单见 `docs/configuration.md` 和 `.env.example`。关键的几个：
  - `JWT_SECRET_KEY` 改了会让所有人退出登录；`ENCRYPTION_KEY` 改了会让已存的供应商密钥全部无法解密，必须重新录入；`ARCA_BACKUP_ENCRYPTION_KEY` 建议独立设置（缺省回退到 `ENCRYPTION_KEY`），旧值要保留以便恢复。
  - `ARCA_PUBLIC_BASE_URL`：一键安装命令和反馈邮件里的链接用它；不设则安装命令里会是 `localhost`。
  - `ARCA_GATEWAY_MAX_IN_FLIGHT_REQUESTS`（自动 512–65536）、`ARCA_MAX_REQUEST_BODY_MB`（默认 256，硬上限 256）、`ARCA_GATEWAY_REQUEST_BODY_BUFFER_BUDGET_MB`（256）。
- **更新策略**：本分支为**手动**（`ARCA_UPDATE_STRATEGY=manual`）。**不要使用控制台的"在线更新"**：自更新器只信任上游 `github.com/fawney19/Aether` 的发布。Compose 部署用 `./update.sh`（拉取 `APP_IMAGE` 并只重建 `app`；选项 `--mode local-build`、`--no-pull`、`--force-recreate`、`--prepare`）。升级前先做受保护的数据库备份并保留旧镜像，不要替换密钥。回滚时旧应用无法解析金额策略，要先在维护窗口内恢复兼容的 `internal_access_policy` 快照，不要重新生成密钥或做反向迁移。
- **备份**：S3 备份（设置见词典）加密后上传；`backup_s3_retention_count` **不会删除**对象，要用存储桶生命周期规则。离线恢复工具 `aether-backup-restore` 只解密校验到本地 JSON，不写数据库；导入在维护窗口内从 `/admin/system` 的数据管理操作。反馈表包含在整库 PostgreSQL 备份里，JSON 导出不是反馈导出。升级前请 `pg_dump`。
- **健康与日志**：`GET /health`、`GET /readyz`、容器自检 `aether-gateway --healthcheck`；`GET /_gateway/metrics` 需要 `admin:monitoring:read`。`RUST_LOG`、`ARCA_LOG_FORMAT`（pretty/json）、`ARCA_LOG_DESTINATION`。用 `x-trace-id` 在日志里找请求；内部错误对客户端隐藏，只在日志里带 `error_fingerprint`。管理员审计 `/admin/audit-logs`（保留 30 天）。
