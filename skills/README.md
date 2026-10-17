# Agent skills

## arca-admin

Teaches an agent to operate Arca through its embedded MCP server: query and diagnose, make
controlled changes with preview/confirm, and triage and reply to user feedback.

### Install

Copy the folder to where your agent looks for skills, for example:

```bash
cp -R skills/arca-admin ~/.claude/skills/        # Claude Code, all projects
cp -R skills/arca-admin .claude/skills/          # Claude Code, this project only
cp -R skills/arca-admin ~/.codex/skills/         # Codex
```

### Connect the MCP server

In the console open **Agent MCP** (`/admin/mcp`) and create a key for the signed-in administrator.
Use two keys with different authority:

| Use | Key preset | Can do |
| --- | --- | --- |
| Unattended feedback worker | **仅用户反馈** | read and reply to feedback, change feedback status; cannot touch providers, models, users |
| Interactive operations | read/write (still bounded by your role) | the full change protocol in the skill |

Generic client configuration (the tools keep their `afssr_` prefix whatever you name the server):

```json
{
  "mcpServers": {
    "arca": {
      "url": "https://gateway.example.org/mcp",
      "headers": { "Authorization": "Bearer <KEY>" }
    }
  }
}
```

Set `ARCA_MCP_ALLOWED_HOSTS` for a reverse-proxied hostname. Details: `docs/operations/agent-mcp.md`.

### Keeping it accurate

`references/tool-map.md` is generated from the MCP operation catalog:

```bash
python3 skills/arca-admin/scripts/gen_tool_map.py          # regenerate after changing catalog.rs
python3 skills/arca-admin/scripts/gen_tool_map.py --check  # exit 1 if stale
```
