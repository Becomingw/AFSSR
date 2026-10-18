# Real-client compatibility suite

Runs the **actual Codex CLI and Claude Code binaries**, headless, through the Arca gateway to a
scripted mock upstream, and asserts on what the clients did and on what the gateway forwarded.

```
real client  ->  Arca gateway (disposable Postgres DB, memory runtime)  ->  mock_upstream.mjs
```

Everything stays on loopback. The clients get an isolated `HOME`, `CODEX_HOME` and
`CLAUDE_CONFIG_DIR` and a throwaway gateway API key, so they never see your real config,
credentials or hooks. No real model or provider is contacted and nothing costs money.

## Run

Prerequisites: `node`, a local PostgreSQL (`createdb`/`dropdb` on `PATH`), the `codex` and `claude`
CLIs, and a built gateway (`cargo build -p aether-gateway`).

```bash
python3 tests/client_e2e/run.py                                   # full matrix: 2 clients x 3 upstreams x 11 scenarios
python3 tests/client_e2e/run.py --clients claude --upstreams chat  # a slice
python3 tests/client_e2e/run.py --scenarios multi_tool --keep      # keep logs + captured upstream traffic
```

`--keep` prints a temp directory with the gateway log, each client's output and
`mock-upstream.jsonl` (every request the upstream received, including full bodies). Use it to capture
fixtures for the offline suite when a client version changes its request shape.

## Matrix

Codex speaks OpenAI Responses and Claude Code speaks Anthropic Messages; each is run against an
upstream of every format (`openai:chat`, `openai:responses`, `claude:messages`), so same-format
pass-through and both directions of format conversion are exercised.

| Scenario | What the mock upstream does | What is asserted |
| --- | --- | --- |
| `text_only` | answers with plain text | the answer reaches the client, clean exit |
| `write_file` | one tool call (Claude `Write`, Codex `exec_command`) | the real client executed the tool and the file exists; the tool result went back upstream |
| `multi_tool` | three sequential tool calls | three rounds, files written in order, results sent back round by round |
| `shell` | one shell command | the command ran in the client's own shell tool |
| `cancel` | a long stream; the client gets Ctrl-C mid-stream | with `cancel_on_client_disconnect` on, the gateway closes the upstream stream |
| `cancel_drain` | same, default policy | the gateway keeps draining the upstream stream so usage can still settle |
| `err_400` `err_429` `err_500` `err_html` | the upstream rejects with a JSON error / rate limit / server error / HTML 502 page | the trace keeps the upstream status, useful headers (`retry-after`, upstream `x-request-id`) and body verbatim, the gateway's classification and URL; the failed request keeps the converted request that was sent (`request_record_level=failures`) |
| `err_stream` | a 200 whose SSE fails half-way | the in-band error message is kept on the attempt and the error body with the failed request |

Every non-cancel case also checks, on the request the gateway forwarded upstream:

- the client's system prompt/instructions arrived in the upstream's native slot;
- the client's signature tools survived conversion with their parameters (`exec_command.cmd`,
  `Write.file_path/content`, `Bash.command`, `Read.file_path`);
- every tool result refers to a tool call present in the same request, one to one;
- the model name was mapped, no request hit an unknown upstream path;
- the gateway's usage accounting matches the upstream's numbers exactly (request count, input and
  output tokens).

## OpenCode Go matrix

`go_matrix.py` runs real Codex and Claude Code against an `opencode_go` provider whose upstream
(`mock_upstream.mjs ... go`, rules in `go_mode.mjs`) behaves like OpenCode Go: every model is served
on exactly one native protocol, anything else gets "Model does not support this protocol.", the
backends validate strictly (tool shapes, extra fields, role names, reasoning replay) with the error
texts seen in production, and every reply carries reasoning. A case passes only when the client
finished its tool loop and the upstream refused nothing.

    python3 tests/client_e2e/go_matrix.py [--clients codex,claude] [--models glm-5.3] [--scenarios multi_tool]

`go_probe.sh` is the fast, conversion-only version: it converts the captured real Codex / Claude Code
bodies (`crates/aether-ai/formats/tests/fixtures/real_clients/`) for every Go model and runs them
through the same validators (`go_validate.mjs`) in seconds. Refresh a fixture by running
`CAPTURE_DIR=<dir> python3 run.py --upstreams responses,messages --scenarios text_only`.

The rules in `go_mode.mjs` are a model of the service, not the service: where the docs or a probe
do not settle a point the mock follows the gateway's documented behaviour. The live acceptance on a
real key is what settles it.

## Not covered

- Real model behaviour (the mock is scripted) and real provider quirks: auth, rate limits, field
  allow-lists. That needs a paid smoke test against a real channel.
- Interactive TUI modes, `/compact` and context compression, MCP servers, sub-agents, images,
  reasoning/thinking round trips, WebSocket transport for Codex, and multi-provider failover.
  The scenario hook in `mock_upstream.mjs` (`steps()`) is the place to add them.
- Client versions other than the ones installed locally; results are only evidence for those.

The offline workbench at `/admin/compatibility` is separate: it records synthetic conversion
evidence and still reports `real_client` as `not_tested`.
