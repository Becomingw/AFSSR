# Agent skills

| Skill | For | What it does |
| --- | --- | --- |
| [`arca-admin`](arca-admin/SKILL.md) | administrators and support/ops agents | operate Arca through the MCP server, answer "how do I configure / what does this setting mean" from a product knowledge base, reply to user feedback |
| [`arca-user`](arca-user/SKILL.md) | ordinary users and their coding agents | get a key, find model ids, connect Codex / Claude Code / OpenCode, read quotas and errors, file useful feedback (needs no administrator rights and no MCP) |

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
| Unattended feedback worker | **仅讨论与反馈** | read and reply to feedback, change feedback status; cannot touch providers, models, users |
| Interactive operations | read/write (still bounded by your role) | the full change protocol in the skill |

Generic client configuration (tools are named `arca_*`; the old `afssr_*` names still work as deprecated aliases):

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

### Keeping the knowledge base accurate

```bash
python3 skills/arca-admin/scripts/gen_settings_defaults.py --check  # defaults match code; every setting has a glossary entry
python3 skills/sync_shared.py --check                                # shared documents are in sync
```

`arca-user` needs no setup beyond copying the folder (`cp -R skills/arca-user ~/.claude/skills/`).
