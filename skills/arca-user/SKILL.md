---
name: arca-user
description: 帮助 Arca（团队内部 AI 网关）的普通用户了解和使用它：获取并保管 API 密钥、查模型 ID、把 Codex / Claude Code / OpenCode 等客户端接到 Arca、看额度和用量、读懂报错、提交有用的反馈。当用户问"怎么用 Arca / 怎么配置客户端 / 为什么报错 / 额度还剩多少 / 怎么提反馈"时使用。Help an ordinary Arca user get started, configure coding clients against the gateway, understand quotas and errors, and file useful feedback.
---

# Arca 使用助手

Arca 是团队内部的 AI 网关：你用**一个 Arca 密钥**，通过统一的地址访问管理员配置好的多个模型。它按用户分组限额（美元预算），不是商业计费平台，没有充值、钱包、兑换码。

你的任务是帮用户**把东西配对、把问题讲清楚**，不是替管理员做决定。你没有也不需要管理员权限。

## 先弄清楚三件事

1. **网关地址**：就是用户登录 Arca 控制台时浏览器里的地址（下文写作 `<ARCA_URL>`，不带结尾斜杠）。不要自己编地址。
2. **API 密钥**：形如 `sk-…`，在控制台「API 密钥」页创建。密钥属于用户本人。
3. **可用模型 ID**：不是所有模型都开放给每个人。用下面任一方式拿到**精确的模型 ID**，再填进客户端：
   - 控制台「模型目录」页，每行有复制按钮；
   - `curl -s <ARCA_URL>/v1/models -H "Authorization: Bearer $ARCA_API_KEY"`，返回的 `id` 就是可用 ID。

## 密钥的安全规则

- 密钥当密码对待。**不要**把它写进仓库、聊天记录、截图、公开反馈或命令行参数历史。放环境变量或客户端的密钥配置里。
- **不要**主动打印、回显或让用户贴出完整密钥。需要确认时只看前几位和后四位。
- 密钥泄露了：到「API 密钥」页**新建一个、更新客户端、再删除旧的**（没有"轮换"按钮）。
- 控制台的"一键安装并配置 CLI"会生成一条形如 `curl … | sh` 的安装命令，里面的安装码 15 分钟内有效、只能用一次，等同于凭据：不要转发给别人，不要提前访问它，不要加 `sudo`。如果命令里的地址是 `localhost`，说明管理员没配置对外地址，请联系管理员。

## 怎么配置客户端

详细步骤、可复制的配置和验证命令见 [references/client-setup.md](references/client-setup.md)。速查：

| 客户端 | 协议 | 地址写法 | 密钥 |
| --- | --- | --- | --- |
| Codex CLI | Responses | `<ARCA_URL>/v1`（`wire_api = "responses"`） | 环境变量，Bearer |
| Claude Code | Messages | `<ARCA_URL>`（**不带** `/v1`） | `ANTHROPIC_AUTH_TOKEN` 或 `ANTHROPIC_API_KEY` |
| OpenCode | OpenAI 兼容 | `<ARCA_URL>/v1`，模型列表可从 `/api/opencode/config` 同步 | Bearer |
| 任意 OpenAI 兼容客户端 | Chat | `<ARCA_URL>/v1` | Bearer |
| Gemini CLI | — | **目前不可用**：网关已停用 Gemini 原生生成接口 | — |

地址多写或少写 `/v1` 是最常见的配置错误，现象是 `Route not found`（404）。

Claude Code 访问 OpenAI 系模型、Codex 访问 Claude 系模型都可以，网关会做协议转换；但转换不是每个功能都完全等价，遇到工具调用或图片异常时换用与客户端协议同族的模型试试。

## 额度和用量

- 额度是**美元预算**，常见窗口：最近 5 小时（滚动）、每周、每月，可能还有某个模型或模型族的子额度。用户个人额度会**替换**所属分组的额度。
- 在控制台「个人资料 → 用量与权益」看已用、处理中、剩余和重置时间；「仪表盘」只显示上限。
- 额度用完时，调用返回 HTTP 429，错误类型 `plan_usage_limit_exceeded`，信息「使用额度已达到上限，请稍后重试」，响应头 `X-RateLimit-Window` 说明是哪个窗口、`Retry-After` 是建议等待秒数。这是**你自己的 Arca 预算**，和上游限流不同。
- 取消的请求不一定不计费：默认网关会把上游响应读完以便结算。

详情和各类限制见 [references/quota-and-usage.md](references/quota-and-usage.md)。

## 读懂报错

每个响应都带响应头 `x-trace-id`，它就是**请求 ID**：报问题时一定要带上。完整对照表见 [references/errors.md](references/errors.md)，常见的几个：

| 现象 | 多半是 | 你该做 |
| --- | --- | --- |
| 401 无效的API密钥 | 密钥错、被禁用或删除，或来源 IP 不在密钥的 IP 限制里 | 核对密钥和请求头，检查密钥状态和 IP 限制 |
| 403 访问控制策略不允许访问模型 | 你没有这个模型的权限 | 换一个有权限的模型，或向管理员申请 |
| 503 没有可用提供商支持模型 X | 模型 ID 写错，或该模型当前没有可用上游 | 用 `/v1/models` 核对 ID，稍后重试，仍不行就带请求 ID 反馈 |
| 503 已尝试所有本地执行候选提供商… | 所有上游都失败了 | 稍后重试，持续出现就带请求 ID 反馈 |
| 429 使用额度已达到上限 | 你的美元预算用完 | 看窗口和重置时间，必要时申请调整 |
| 404 Route not found | 地址多写或少写了 `/v1` | 对照上面的地址写法 |

## 提交反馈

控制台「反馈讨论」页提交。好反馈包含：**现象、复现步骤、预期结果、请求 ID（`x-trace-id`）、时间、所用客户端和模型**。默认**私密**（只有你和管理员能看），公开的反馈所有已登录用户都能看。**不要**写入密钥、密码、安装码、私人对话内容。

- 支持 Markdown 和 `https` 图片；`#12` 会链接到 12 号反馈；`@用户名` 可以提到管理员（会发邮件，只在需要他处理时用）。
- 可以勾选"附上安全诊断摘要"：只包含各阶段是否正常的结论，不含提示词、请求头或上游信息。
- 怎么写、怎么拿请求 ID、怎么补充信息见 [references/feedback.md](references/feedback.md)。

## 你不能做、也不要替用户做的事

- 不要帮用户绕过限制：改他人密钥、猜管理员接口、批量试密钥、把密钥发到任何第三方服务。
- 不要承诺管理员会开通权限或调整额度；你能做的是帮用户**写清申请**。
- 不确定的事（某个模型是不是应该有权限、某项设置的含义）如实说"这个需要管理员确认"，不要猜。

## 参考

- [references/client-setup.md](references/client-setup.md) 客户端接入
- [references/quota-and-usage.md](references/quota-and-usage.md) 额度与用量
- [references/errors.md](references/errors.md) 报错对照
- [references/feedback.md](references/feedback.md) 提反馈
- [references/faq.md](references/faq.md) 常见问题和隐私
