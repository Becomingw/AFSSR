# 管理员 FAQ

> 核对于 2026-10-18（HEAD `266c45ed1`）。先查词典/概念，再读实时值，再回答。这里是被问得最多的问题和**指路**，不重复全文。

## 配置类

**怎么让用户只能用某几个模型？**
在用户或分组上设模型列表（`arca_users_update` / `arca_groups_update` 的 `allowed_models`；`null` 继承、`[]` 全拒、列表为白名单）。额度页不授予也不收回模型权限。用户的密钥与用户策略取交集，只能收窄。见 [concepts.md](concepts.md) 第 3 节。

**怎么给一个用户额外额度？**
`internal_access_policy` 的 `users` 里加个人条目——它**整体替换**分组额度，不是叠加，所以要把该用户需要的所有窗口都写全。先 `arca_benefits_preview`（`config` 里放完整的拟议策略），再 `arca_benefits_set`（带 `expected_value`）。

**新注册的用户能用什么？**
注册默认关闭（`enable_registration`）。开了之后，自助注册用户加入 `default_user_group_id` 分组，权限和额度由该分组决定；没有任何分组的用户在内部模式下模型权限和额度都是**不限**，建议配置默认分组和 `default` 预算。

**怎么新增一个模型？**
供应商页给端点和密钥开放该模型 → 模型管理里创建公开模型并关联供应商模型。保存密钥的允许模型会自动匹配 models.dev 目录并加入关联；没有匹配到的会以"待定价、禁用"创建，补上价格后再启用。目录只在手动刷新时更新。

**为什么某个模型一个月后没了？**
孤儿清理：没有任何供应商关联超过 `global_model_orphan_retention_days`（默认 30 天）的公开模型会被删除。

**怎么开启跨格式转换？**
三者任一：供应商开关、全局 `enable_format_conversion`（强制所有供应商）、端点的 `format_acceptance_config`。不开则相关候选被跳过。详见 concepts 第 2 节。

**什么时候该开 `request_record_level=full`？**
只在排查上游兼容问题时临时开：它保存提示词和响应（可能含个人信息），并抬高流式缓冲。开了之后约 30 秒生效，旧请求不会补抓；7 天后正文被压缩、30 天后被删。排查完关回 `basic`。

**邮件发不出去？**
检查 `smtp_host`、`smtp_from_email` 是否为空（空即视为未配置）；改过 host/端口/用户名/加密方式后要**重新输入密码**；`smtp_use_tls` 与 `smtp_use_ssl` 要和端口匹配（587 用 TLS，465 用 SSL）。

**注册时验证码/人机验证有问题？**
`turnstile_enabled` 打开但没配密钥会让注册 503；`require_email_verification` 打开但 SMTP 没配好没人能注册；邮箱白名单里没有自己公司的域名会拦住所有人。

## 使用与排错

**用户说"用不了某个模型"**
走 [diagnosis.md](../diagnosis.md) 第 2 节：`users_get` → `groups_list` → `benefits_preview` → `models_routing`。注意 403（权限）和 503（没有可用上游）是两件事。

**用户说"一直 503"**
503 有很多种：没带密钥、没有候选支持该模型、所有候选失败、网关过载。看响应头 `x-aether-local-execution-runtime-miss-reason`，再用 `arca_request_diagnostics` 和 `arca_usage_records` 对号入座。见 [errors.md](errors.md)。

**用户说"额度没用完却 429"**
看 `X-RateLimit-Scope`：`plan` 是 Arca 预算（看 `window` 判断哪个窗口，预留也计入）；`user`/`key` 是每分钟请求数；没有这个头且文字提到"上游渠道"是厂商限流。

**Gemini CLI 为什么不能用？**
网关停用了 Gemini 原生生成接口。控制台和安装器里仍列着它，但会 404。让用户改用 OpenAI 兼容方式。

**取消的请求为什么还扣额度？**
`cancel_on_client_disconnect` 默认 false，网关会把上游响应读完并结算。到调度策略里打开可立即中断。

## 运维与部署

**能用控制台"在线更新"吗？** 不要。自更新器只信任上游 `fawney19/Aether` 的发布。用 `./update.sh` 或自己构建镜像。

**S3 备份为什么越来越多？** `backup_s3_retention_count` 只统计不删除，要用存储桶的生命周期规则。

**改了 `ENCRYPTION_KEY` 会怎样？** 已存的供应商密钥全部无法解密，需要重新录入；备份也无法恢复，直到换回旧密钥。

**一键安装命令里是 `localhost`？** 没设置 `ARCA_PUBLIC_BASE_URL`。

## 如实说"不知道"的范围

LDAP 的 `is_exclusive` 语义、OAuth 登录细节、各模块的完整注册表、生产环境当前的实际设置和部署状态：本知识库没有核实，读实时配置并请管理员确认，不要猜。
