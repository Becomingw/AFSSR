# 客户端接入

> 核对于 2026-10-18。

下面的配置都已经用真实客户端对网关跑通过（Codex CLI 0.141、Claude Code 2.1.259，经由本仓库的 `tests/client_e2e`）。客户端升级后配置项可能变化，以客户端自己的文档为准。

`<ARCA_URL>` 是网关地址（不带结尾斜杠），`<模型ID>` 是 `/v1/models` 或控制台「模型目录」里的精确 ID。

## 0. 先验证密钥和模型

```bash
export ARCA_API_KEY='sk-…'    # 你的密钥，放环境变量，不要写进文件或聊天
curl -s <ARCA_URL>/v1/models -H "Authorization: Bearer $ARCA_API_KEY"
```

返回 `data[].id` 列表说明地址和密钥都对。`401` 是密钥问题，`Route not found` 是地址问题。再用一个很小的请求确认模型可用：

```bash
curl -s <ARCA_URL>/v1/chat/completions \
  -H "Authorization: Bearer $ARCA_API_KEY" -H 'Content-Type: application/json' \
  -d '{"model":"<模型ID>","messages":[{"role":"user","content":"ping"}],"max_tokens":16}'
```

## 1. Codex CLI

Codex 使用 Responses 协议。编辑 `~/.codex/config.toml`：

```toml
model_provider = "arca"
model = "<模型ID>"

[model_providers.arca]
name = "Arca"
base_url = "<ARCA_URL>/v1"
wire_api = "responses"
env_key = "ARCA_API_KEY"
```

然后 `export ARCA_API_KEY='sk-…'` 再启动 `codex`。

- `base_url` **要带** `/v1`，`wire_api` 必须是 `"responses"`。
- 密钥通过 `env_key` 指向的环境变量传入，不写进配置文件。
- 控制台「API 密钥」页的"一键安装并配置 CLI"会自动写入配置（该方式会把密钥以明文放进 `config.toml`，权限 0600）。想避免明文就用上面的手动配置。

## 2. Claude Code

Claude Code 使用 Messages 协议。设置环境变量（可以放进 shell 配置，或 `~/.claude/settings.json` 的 `env` 字段）：

```bash
export ANTHROPIC_BASE_URL="<ARCA_URL>"          # 注意：不带 /v1
export ANTHROPIC_AUTH_TOKEN="$ARCA_API_KEY"     # 也可以用 ANTHROPIC_API_KEY，两种都被接受
export ANTHROPIC_MODEL="<模型ID>"
export ANTHROPIC_DEFAULT_SONNET_MODEL="<模型ID>"
export ANTHROPIC_DEFAULT_OPUS_MODEL="<模型ID>"
export ANTHROPIC_DEFAULT_HAIKU_MODEL="<模型ID>"
export ANTHROPIC_SMALL_FAST_MODEL="<模型ID>"
```

- `ANTHROPIC_BASE_URL` **不要**带 `/v1`，否则会出现 `Route not found`。
- 后四个变量建议都设：Claude Code 会用"小模型"发后台请求（例如生成标题）。如果那个模型名在 Arca 里不存在，这些请求会因"没有可用提供商支持模型"失败。
- 网关两种认证头都接受（`Authorization: Bearer` 和 `x-api-key`）。

## 3. OpenCode

OpenCode 的模型列表可以从网关同步：

```bash
curl -s <ARCA_URL>/api/opencode/config -H "Authorization: Bearer $ARCA_API_KEY"
```

返回完整的 OpenCode 配置片段（`provider` 下一个 OpenAI 兼容的 provider，`baseURL` 为 `<ARCA_URL>/v1`，`models` 是你的密钥当前可用的模型，含上下文和输出上限）。把它合并进 `~/.config/opencode/opencode.json`，重启 OpenCode。

- 这个接口**只认 API 密钥**（Bearer），登录会话不行。
- 控制台的"一键安装并配置 CLI"在 Linux/macOS 上会自动合并并生成同步脚本 `python3 "$HOME/.aether/opencode-sync.py"`。Windows 请使用 WSL。
- 如果同目录有 `opencode.jsonc`，合并会失败，先处理掉它。
- 管理员新增模型后，重新同步即可看到。

## 4. 任意 OpenAI 兼容客户端（含 Cline 等）

仓库没有给这类客户端做专门适配和测试，按通用方式填写：

- API 提供商：OpenAI Compatible
- Base URL：`<ARCA_URL>/v1`
- API Key：你的密钥
- 模型：`<模型ID>`

也可以直接用三种协议的原生接口：

| 协议 | 路径 | 认证头 |
| --- | --- | --- |
| OpenAI Chat | `POST /v1/chat/completions` | `Authorization: Bearer` 或 `x-api-key` |
| OpenAI Responses | `POST /v1/responses`（HTTP；也支持 WebSocket） | `Authorization: Bearer` |
| Anthropic Messages | `POST /v1/messages`、`/v1/messages/count_tokens` | `x-api-key` 或 `Authorization: Bearer`，并带 `anthropic-version` |
| 模型列表 | `GET /v1/models` | 同上；带 `x-api-key` + `anthropic-version` 返回 Claude 格式 |

另有 `/v1/embeddings`、`/v1/rerank` 等，是否可用取决于管理员给你的密钥开放了哪些格式。

## 5. Gemini CLI

**目前不可用。** 控制台和安装器里仍然能看到 Gemini CLI 的选项，但网关已经停用 Gemini 原生的生成接口（`/v1beta/…:generateContent`），请求会返回 404「Use /v1/responses, /v1/messages, or /v1/chat/completions for model generation」。不要建议用户使用它；如需 Gemini 系模型，请用 OpenAI 兼容方式或其他客户端，并向管理员确认有哪些 Gemini 模型可用。

## 6. 常见配置错误

| 现象 | 原因 | 修复 |
| --- | --- | --- |
| 404 `Route not found` | 地址的 `/v1` 写多或写少了 | Codex、OpenAI 兼容：`<ARCA_URL>/v1`；Claude Code：`<ARCA_URL>` |
| 401 无效的API密钥 | 密钥错、被禁用或删除；密钥有 IP 限制而当前出口不在名单里 | 核对密钥；到「API 密钥」页看状态和 IP 限制 |
| 503 没有可用提供商支持模型 X | 模型 ID 写错，或该模型当前没有可用上游 | 用 `/v1/models` 核对精确 ID |
| Claude Code 主流程正常、偶发后台请求失败 | 小模型变量没设或指向不存在的模型 | 设置 `ANTHROPIC_SMALL_FAST_MODEL` 和 `ANTHROPIC_DEFAULT_*_MODEL` |
| Codex 报 wire_api / 协议错误 | `wire_api` 不是 `"responses"` | 改成 `"responses"` |
| 工具调用或图片异常 | 协议转换对某些功能不完全等价 | 换成与客户端协议同族的模型；带请求 ID 反馈 |
