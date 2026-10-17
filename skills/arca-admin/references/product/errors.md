# 报错目录（管理员视角）

> 依据：已提交代码（核对于 2026-10-18，HEAD `266c45ed1`）。"实测"= 在运行的网关上复现过，其余来自读代码。**来源**列：本地 = 网关自己的决定；上游 = 厂商返回（直通或被总结）。

## 先看这几件事

1. **谁产生的错误**：看响应头 `x-aether-execution-path`（`local_auth_denied`、`local_rate_limited`、`local_execution_runtime_miss`…）和 `x-aether-local-execution-runtime-miss-reason`（精确原因代码）。**用户看到的"没有可用提供商…"消息里，原因代码和跳过原因细节已被剥掉**，只在这个响应头、用量记录和候选记录里能看到。
2. **请求 ID** = 响应头 `x-trace-id`。用 `arca_request_diagnostics` 看各阶段证据，用 `arca_usage_records` 找记录。
3. **同一条件有两种 JSON 外形**：OpenAI 风格 `{"error":{"type":"http_error","message":…}}`；Claude 客户端（`/v1/messages`）是 `{"type":"error","error":{"type":<claude 类型>,…}}`（400/405/422 `invalid_request_error`，401 `authentication_error`，403 `permission_error`，404 `not_found_error`，413 `request_too_large`，429 `rate_limit_error`，503/529 `overloaded_error`，其余 `api_error`）。
4. **上游自己的 401/403/404/5xx 不会原样转给用户**，全部候选失败后统一变成 503 的网关消息。**不要把上游的 401/403/429 说成本地身份或额度问题。**

## 鉴权与密钥状态

| 情形 | 状态 | 消息 | 来源 | 原因与谁处理 |
| --- | --- | --- | --- | --- |
| 密钥不存在/用户被删或停用/密钥停用或过期 | 401 | 无效的API密钥 | 本地（实测） | 用户核对密钥；`user_admin` 重新启用用户/密钥或延长有效期 |
| 密钥被管理员锁定（仅非独立密钥） | 403 | 该密钥已被管理员锁定，请联系管理员 | 本地 | `user_admin` 解锁 |
| 密钥 IP 规则不允许当前来源 | **401** | API Key 不允许从当前 IP 访问: `<ip>` | 本地（实测） | 改密钥 IP 限制；IP 看起来不对时检查 `ARCA_TRUSTED_PROXY_CIDRS` |
| 全局 IP 黑名单 | 403 | 当前 IP 已被禁止访问 | 本地 | `admin` 在 `/admin/ip-security` 移除 |
| IP 规则存储不可用 | 503 | IP 访问控制暂时不可用 | 本地 | 基础设施（数据库/Redis），运维处理 |
| 没带密钥/认证上下文缺失，或密钥快照缺失、过期、读取失败 | 503 | 请求缺少有效的用户或 API Key 认证上下文，无法选择上游提供商 / 当前 API Key 的本地执行配置不存在或已过期… | 本地（前者实测） | 前者是请求没带密钥；后者多为缓存或数据库的临时问题，重试，仍不行找运维 |

## 权限与路由

| 情形 | 状态 | 消息 | 来源 | 原因与谁处理 |
| --- | --- | --- | --- | --- |
| 供应商族不被允许 | 403 | 当前用户、用户组或密钥的访问控制策略不允许访问 `<provider>` 提供商 | 本地 | `allowed_providers` 排除了该族（如 `openai`、`claude`）；`user_admin` 改分组/密钥 |
| API 格式不被允许 | 403 | …不允许访问 `<格式>` 格式 | 本地 | `allowed_api_formats` |
| 模型不被允许 | 403 | …不允许访问模型 `<model>` | 本地 | 生效的 `allowed_models`（用户模式、分组并集、密钥列表）里没有它、也没有它的后缀基础模型和别名。`null` 继承、`[]` 全拒。**改用户/分组的模型列表，不是改额度页** |
| 路由策略拒绝模型 | 400 | requested model is not allowed by routing policy | 本地 | 路由规则 `restrict_models`；`service_admin` |
| 路由策略缺失/无效 | 503 / 400 | no enabled routing strategy is configured for this request / invalid routing group config | 本地 | 默认策略被停用或不存在，或存储的配置无效；`service_admin` |
| 显式路由头有问题 | 403 | requested routing group was not found / is not enabled / is not allowed for this principal | 本地 | `x-aether-scheduler-group` 写错，或没有 `allow_explicit_select` 的绑定 |
| 调用了商业功能路径 | 404 | `{"detail":"Commercial features are not available in AFSSR internal gateway"}` | 本地 | 已停用的功能 |
| 已停用的 Gemini 生成路径 | 404 | `{"detail":"Use /v1/responses, /v1/messages, or /v1/chat/completions for model generation"}` | 本地（实测） | 客户端改用三种协议之一 |
| 路径不存在 | 404 | Route not found | 本地（实测） | URL 错误，常见是 `/v1` 写多或写少 |

## 找不到模型 / 没有候选 / 全部失败

| 情形 | 状态 | 用户看到的消息 | 来源 | 原因与谁处理 |
| --- | --- | --- | --- | --- |
| 没有候选支持该模型（`candidate_list_empty`） | **503（不是 404）** | 没有可用提供商支持模型 `<m>` 的同步/流式请求 | 本地（实测） | 模型未知、没有关联任何供应商、供应商/端点/密钥未启用、上游密钥 `allowed_models` 排除、密钥没开该 API 格式、用户限制。`service_admin` 查模型页/映射/密钥；原因代码在响应头 |
| 有候选但全部被跳过（`all_candidates_skipped`） | 503 | 同上 | 本地 | 跳过原因只在用量/候选记录里：`format_conversion_disabled`、`provider_inactive`、`endpoint_inactive`、`key_inactive`、`key_api_format_disabled`、`key_model_disabled`、`mapped_model_missing`、`pool_*`、`*_concurrency_limit_reached`、`transport_*_unsupported`、`provider_request_body_build_failed` |
| 所有执行过的候选都失败（`execution_runtime_candidates_exhausted`） | 503 | 已尝试所有本地执行候选提供商，但没有任何候选成功完成请求 | 本地对上游失败的总结（实测） | 候选都报错、超时，或上游返回 5xx/401/403。**用量详情和候选记录里有上游真实状态和错误体**（`upstream_response`）。`service_admin` 查上游健康、密钥、代理 |
| 最后一个候选返回 429 | 429（保留状态） | 上游渠道达到限流或额度限制（HTTP 429），请稍后重试 | 本地文字 + 上游状态（实测） | 厂商限流；等待或加密钥。**不是用户的 Arca 额度** |
| 最后一个候选返回 400 / 422 | 实测 400 最终是 **503**（「已尝试所有本地执行候选…」）；代码里还有 400/422 保留状态并替换为「上游拒绝了请求格式或模型参数（HTTP 400）…」「上游无法处理此模型或工具请求（HTTP 422）…」的分支 | 视情况 | 本地文字 + 上游状态 | 参数、转换或能力不匹配：检查格式转换、模型后缀、body 规则、映射。**上游原始错误体在候选记录里**；上游 400 还会被同一候选重试一次（实测，同一请求在上游出现两次），属于现有行为 |
| 候选评估未完成 / 无法构建执行计划 | 503 | 本地执行候选评估未完成… / 找到了候选提供商，但无法为本次…请求构建本地执行计划。请检查端点路径、认证方式、Header/Body 规则和格式转换配置 | 本地 | 端点路径、认证、规则、转换配置；`service_admin` |
| 规划阶段超时 | 504 | 当前 AI 请求在本地执行规划阶段超时，请稍后重试 | 本地 | 过载或数据库慢；运维 |
| 所有候选的请求体转换失败 | 503 | 上游请求体转换失败 | 本地 | 转换限制或路由 JSON 补丁导致；诊断的 `conversion` 阶段为 `format_conversion_failed` |
| 路由 JSON 补丁作用于二进制/空体 | 400 | routing provider_request body patch cannot be applied to a binary or empty upstream body | 本地 | 规则配置错误 |

## 限流、额度、容量

| 情形 | 状态 | 消息与响应头 | 来源 | 原因与谁处理 |
| --- | --- | --- | --- | --- |
| 美元预算用完 | 429 `plan_usage_limit_exceeded` | 使用额度已达到上限，请稍后重试。`error.details`：`metric`（`actual_cost_usd`）、`window`（`rolling_5h`/`calendar_day`/`calendar_week`/`calendar_month`）、`limit`、`retry_after`；响应头 `Retry-After`、`X-RateLimit-Limit`、`X-RateLimit-Remaining: 0`、`X-RateLimit-Scope: plan`、`X-RateLimit-Metric`、`X-RateLimit-Window`。Claude 客户端的响应体没有 `details` | 本地（实测） | 分组/用户/子额度在该窗口用完（预留也计入）。`user_admin` 先 `arca_benefits_preview` 再调整预算或设 `unlimited`；5 小时窗口逐步释放 |
| 用户/密钥每分钟请求数 | 429 `rate_limit_exceeded` | 请求过于频繁，请稍后重试；`X-RateLimit-Scope: user\|key` | 本地 | 系统 `rate_limit_per_minute`（0 = 不限）或用户/密钥自己的上限 |
| 调用方 API Key 并发上限 | 503（按代码路径） | 当前调用方 API Key 并发请求数已达上限，请稍后重试 | 本地 | 密钥 `concurrent_limit`；状态码未在运行中核实 |
| 所有上游账号都到并发/RPM 上限 | 429 | 所有可用上游账号当前均已达到并发或 RPM 上限，请稍后重试 | 本地 | 增加密钥或提高 `rpm_limit`（自适应学到的限额可能偏低） |
| 网关过载 | 503 `overloaded` | 服务繁忙，请稍后重试（含 `details {gate, limit}`） | 本地 | `ARCA_GATEWAY_MAX_IN_FLIGHT_REQUESTS` 已满；扩容或调整 |
| 准入队列超时 | 429 + `Retry-After: 1` | gateway admission queue timed out | 本地 | 同上 |
| 额度评估需要模型 | 400 | a model is required to evaluate internal usage budgets | 本地（实测） | 请求没带 `model` |
| 额度后端故障/缺定价/策略 JSON 坏 | 500 | internal server error | 本地 | 对有预算的用户失败即拒绝：模型没有定价、结算存储不可用或导入了无效策略。按 `x-trace-id` 查日志；`service_admin` 补定价。**不要盲目设成 unlimited 来"修"它** |

## 请求体与协议

| 情形 | 状态 | 消息 | 处理 |
| --- | --- | --- | --- |
| 请求体过大 | 413 | Request body exceeds `<N>` bytes | `ARCA_MAX_REQUEST_BODY_MB`（默认 256，硬上限 256） |
| 解压后过大 | 413 | Decoded request body with Content-Encoding `<enc>` exceeds `<N>` bytes | 同上 |
| 编码/分帧错误 | 400 | Unsupported request Content-Encoding / Failed to decode request body / Invalid request body framing | 客户端问题 |
| 请求体缓冲预算耗尽 | 503 | Request body buffering capacity is temporarily exhausted | `ARCA_GATEWAY_REQUEST_BODY_BUFFER_BUDGET_MB` |
| 读取超时/失败 | 408 / 400 | Request body read timed out… / Failed to read request body | 慢客户端；`ARCA_GATEWAY_REQUEST_BODY_READ_TIMEOUT_MS` |
| 上下文超长 | 上游状态 | 上游原文 | 模型上下文窗口，不是网关限制 |

## 上游与转换

| 情形 | 状态 | 消息 | 说明 |
| --- | --- | --- | --- |
| 上游响应无法用客户端 Responses 格式表示（OpenCode Go） | 502 | Upstream response cannot be represented in the client Responses format | 供应商/模型特性 |
| 上游响应过大/无法解码 | 502 `upstream_error` | Upstream response too large / Failed to decode upstream response | 上游发了超大或损坏的响应 |
| 收到 HTTP 响应前的传输错误，且已停止转移 | 502 `server_error` | Upstream transport failed before an HTTP response was received | 供应商设了 `stop_on_transport_errors`；查代理/隧道和端点 URL |
| 上游报错且转移已停止（命中停止规则、不可重试、仅一个候选） | 上游状态 | 上游错误体，必要时重塑成客户端格式 | 厂商侧原因（额度、鉴权、参数）。**上游的 401/403/429 不是本地问题** |
| 流式在内容开始后中断 | HTTP 200 已发出 | 流里的 error 事件或被截断 | 状态不能再变；诊断的 `stream` 阶段 |
| 客户端断开 | 记录为 499 `cancelled` | 无响应 | 由 `cancel_on_client_disconnect` 决定是否继续上游 |
| 执行回环保护 | 508 | Gateway detected an execution runtime request loop back into the local frontdoor | 端点 `base_url` 指向了网关自己 |
| 网关控制/代理不可用 | 502 | gateway proxy unavailable / gateway control unavailable | 运维 |
| 管理员保护 | 400 | 不能降级或停用最后一个管理员账户 / 不能删除最后一个管理员账户 | 最后一个超级管理员受保护 |

## 谁来处理

- **用户自己**：无效密钥、没带 model、请求体/编码问题。
- **`user_admin`**：密钥状态和 IP、模型/供应商/格式权限、额度、限速。
- **`service_admin`**：路由、候选为空或被跳过、上游全部失败、并发/RPM、转换配置、代理。
- **`admin`**：全局 IP 黑名单。
- **运维**：数据库/Redis、过载、规划超时、缓冲预算、回环、网关不可用。
- **厂商**：上游失败、上游限流、上下文超长、流中断。
