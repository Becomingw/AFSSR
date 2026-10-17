# 排查食谱

按症状选路径。全部是只读调用，除非明确标了 **改**。工具名都带 `arca_` 前缀（下文省略）。
先宽后窄，每一步都把"事实 / 推断 / 未知"分开记。

## 1. 用户说"某个请求失败了"（有 request id）

1. `request_diagnostics`（path: `request_id`）：看八个阶段的状态和确定性摘要。
   - 阶段状态只有 known / failed / unknown。`known` **不代表该阶段成功**，`unknown` 是没有证据。
   - 没有 owner 记录、记录已清理、较旧记录都会是 unknown，如实说"没有证据"。
2. `usage_records`（query: `user_id`、`model`、`provider`、`status`、`limit`）：核对该用户同一时间段的请求结果、状态码、耗时。不返回正文。
3. 对照上游：`stats_errors`（错误分布）、`monitor_resilience` / `monitor_circuits`（熔断状态与历史）、`providers_health`。
4. 结论要区分来源：本地网关拒绝（鉴权、模型权限、额度）、网关转换失败、上游返回错误、网络/代理错误。**上游返回的 401/403/429 不是本地身份或额度问题。**

## 2. 用户说"用不了某个模型 / 没权限"

1. `users_get`（path: `user_id`）：用户当前的模型访问策略（`allowed_models`：`null` 继承分组、`[]` 全拒、列表覆盖）。
2. `groups_list`：用户所在分组的模型策略。
3. `benefits_preview`（body: `user_id`，可选 `api_key_id`）：有效的模型访问和 USD 额度，**不发推理、不保存**。
4. `users_keys`：该用户的推理 key 是否有模型/供应商/格式限制（只返回元数据）。
5. `models_get` / `models_routing`：这个公开模型是否启用、有哪些供应商、路由候选是否可用。
6. 如果是模型权限需要放开 → **改**：`users_update` 或 `groups_update`，走变更协议，先 `benefits_preview`。放开范围按用户申请的最小范围。

## 3. 额度 / 预算问题

1. `benefits_get`：读 `internal_access_policy`（总预算 `five_hour_budget_usd`/`weekly_budget_usd`/`monthly_budget_usd`，模型和模型族子池 `daily_budget_usd`）。个人额度**替换**分组额度；子池也计入总预算。
2. `benefits_preview`：看用户当前实际生效的额度。
3. `usage_stats` / `stats_users` / `usage_records`：看用量。
4. 要改 → **改**：`benefits_set`，body `{value, expected_value}`，`expected_value` 取自刚才 `benefits_get`。CAS 冲突就重读，别重试覆盖。先 `benefits_preview` 带 `config` 预演。

## 4. 上游异常（错误率升高 / 某供应商不可用）

1. `health_summary`、`health_status`、`health_providers`、`health_formats`：范围多大，是一个供应商、一个密钥还是一种 API 格式。
2. `providers_health`、`health_key`、`rpm_key`：看具体密钥/端点。
3. `pool_overview`、`pool_status`（账号池供应商）：冷却、活跃、热池状态。
4. `stats_errors`、`stats_provider_performance`、`stats_percentiles`：错误分布和延迟分位。
5. `monitor_circuits`：熔断状态转换时间线，对上事件时间。
6. 可做的处理（都是 **改**，影响在线流量，先说明再确认）：
   - `health_reset`：重置某密钥的健康/熔断历史（只在确认根因已消除后）。
   - `pool_clear_cooldown`：清单个账号冷却。
   - `providers_update` 设 `is_active:false`：临时摘除供应商（**优先停用，不要删除**）。
   - `routing_*`：调整路由要先 `routing_preview`（合成请求评估，不发推理）再 `routing_publish`。
7. 不要用 `model_test*` 去"验证上游"，那会用真实凭据发请求并可能产生费用；确需要时找人同意。

## 5. 模型兼容问题（客户端协议和上游格式）

1. `compatibility_list`：离线合成转换的结果，只证明协议转换没丢内容，**不代表真实客户端或真实模型已验证**。
2. `provider_models_get`、`endpoints_get`：看映射、上游格式、请求兼容规则。
3. `models_routing`：这个模型实际会路由到哪些候选。
4. 需要更多证据时，说明要在管理端开启 `request_record_level=full` 才能留下转换后发给上游的请求体（含用户内容，只在排查期间开）。这是 `system_config_set`，属于 Destructive 级别，需要人同意。

## 6. 慢 / 延迟

`stats_percentiles`、`stats_provider_performance`、`stats_time_series`、`usage_active`（当前活跃请求），对比不同供应商和时间窗口（`stats_comparison`）。

## 7. 谁在用 / 成本

`stats_users`、`stats_models`、`stats_keys`（不含密钥）、`stats_forecast`、`stats_savings`、`leaderboard_overview`。

## 8. 系统本身

`system_version`、`system_stats`、`monitor_system`、`modules_list`、`system_cleanup_preview`（只预览，不删除）。
更新、回滚、模块开关、LDAP、全局配置都是 Destructive：必须有人明确同意并确认维护窗口。
