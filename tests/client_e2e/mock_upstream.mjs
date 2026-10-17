// Scripted synthetic upstream for the real-client compatibility suite.
//
// It speaks all three upstream formats (OpenAI Chat, OpenAI Responses, Anthropic Messages), both
// streaming and non-streaming, and behaves like a tiny deterministic "model": the scenario named
// in the first user message decides which tools it calls, in which order, and what it finally says.
// Every request is appended to a JSONL log so the harness can assert what the gateway forwarded.
//
// No external calls, no credentials, no command execution.
import fs from 'node:fs';
import http from 'node:http';
import { randomUUID } from 'node:crypto';

const port = Number(process.argv[2] ?? 18091);
const logPath = process.argv[3] ?? 'mock-upstream.jsonl';
const log = (entry) => fs.appendFileSync(logPath, JSON.stringify({ ts: Date.now(), ...entry }) + '\n');
const id = (prefix) => prefix + randomUUID().replaceAll('-', '').slice(0, 24);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// ---- scenario script -----------------------------------------------------------------------
// The prompt carries `e2e-scenario:<name> dir=<abs dir>`; the mock derives every tool call from it.
function parseScenario(text) {
  const match = /e2e-scenario:(\w+)\s+dir=(\S+)/.exec(text ?? '');
  return match ? { name: match[1], dir: match[2] } : null;
}

// A tool step is a neutral intent; `toolCall()` maps it onto whatever tool the client offers.
function steps(scenario) {
  const file = (n) => `${scenario.dir}/e2e_${n}.txt`;
  switch (scenario.name) {
    case 'write_file':
      return [{ kind: 'write', path: file(0), content: 'E2E_WRITE_0' }];
    case 'multi_tool':
      return [0, 1, 2].map((n) => ({ kind: 'write', path: file(n), content: `E2E_WRITE_${n}` }));
    case 'shell':
      return [{ kind: 'shell', command: `printf E2E_SHELL > "${file(0)}"` }];
    case 'text_only':
    case 'cancel':
    case 'cancel_drain':
    case 'err_400':
    case 'err_429':
    case 'err_500':
    case 'err_html':
    case 'err_stream':
    default:
      return [];
  }
}

const SHELL_TOOLS = ['shell', 'exec_command', 'shell_command', 'local_shell', 'Bash'];

function pickTool(tools, wanted) {
  return tools.find((tool) => wanted.includes(tool.name));
}

// Map a neutral step to a concrete tool name + JSON arguments, or null if the client has no such tool.
function toolCall(step, tools) {
  const shell = (command) => {
    const tool = pickTool(tools, SHELL_TOOLS);
    if (!tool) return null;
    const props = tool.schema?.properties ?? {};
    // Codex `shell` takes an argv array; `exec_command`/`shell_command` and Claude's Bash take a string.
    if (props.command?.type === 'array') return { name: tool.name, args: { command: ['bash', '-lc', command] } };
    if ('cmd' in props) return { name: tool.name, args: { cmd: command } };
    return { name: tool.name, args: { command } };
  };
  if (step.kind === 'shell') return shell(step.command);
  const write = pickTool(tools, ['Write']);
  if (write) return { name: write.name, args: { file_path: step.path, content: step.content } };
  // Clients without a write tool (Codex) write through the shell instead.
  return shell(`printf '%s' '${step.content}' > "${step.path}"`);
}

// ---- request inspection --------------------------------------------------------------------
function textOf(content) {
  if (typeof content === 'string') return content;
  if (!Array.isArray(content)) return '';
  return content.map((part) => part?.text ?? part?.content ?? '').filter((t) => typeof t === 'string').join('\n');
}

function inspect(format, body) {
  const userTexts = [];
  let results = 0;
  let tools = [];
  if (format === 'chat') {
    for (const m of body.messages ?? []) {
      if (m.role === 'user') userTexts.push(textOf(m.content));
      if (m.role === 'tool') results += 1;
    }
    tools = (body.tools ?? []).filter((t) => t.type === 'function').map((t) => ({ name: t.function?.name, schema: t.function?.parameters }));
  } else if (format === 'responses') {
    const input = typeof body.input === 'string' ? [{ role: 'user', content: body.input }] : body.input ?? [];
    for (const item of input) {
      if (item.role === 'user') userTexts.push(textOf(item.content));
      if (/_output$/.test(item.type ?? '')) results += 1;
    }
    tools = (body.tools ?? []).filter((t) => t.type === 'function').map((t) => ({ name: t.name, schema: t.parameters }));
  } else {
    for (const m of body.messages ?? []) {
      if (m.role !== 'user') continue;
      const blocks = Array.isArray(m.content) ? m.content : [{ type: 'text', text: m.content }];
      userTexts.push(textOf(blocks.filter((b) => b.type !== 'tool_result')));
      results += blocks.filter((b) => b.type === 'tool_result').length;
    }
    tools = (body.tools ?? []).map((t) => ({ name: t.name, schema: t.input_schema }));
  }
  const scenario = parseScenario(userTexts.find((t) => t.includes('e2e-scenario:')));
  return { scenario, results, tools, toolNames: tools.map((t) => t.name) };
}

// ---- reply planning ------------------------------------------------------------------------
function plan(info) {
  const script = info.scenario ? steps(info.scenario) : [];
  if (info.toolNames.length && info.results < script.length) {
    const call = toolCall(script[info.results], info.tools);
    if (call) return { type: 'tool', callId: id('call_'), ...call };
  }
  if (info.scenario?.name?.startsWith('err_')) return { type: 'error', kind: info.scenario.name };
  if (info.scenario?.name === 'cancel') return { type: 'slow', chunks: 150 };
  if (info.scenario?.name === 'cancel_drain') return { type: 'slow', chunks: 20 };
  const done = info.scenario ? `E2E_DONE rounds=${info.results}` : 'synthetic hello';
  return { type: 'text', text: done };
}

const USAGE = { input: 12, output: 7 };

// ---- chat ----------------------------------------------------------------------------------
function chat(body, reply, res) {
  const base = { id: id('chatcmpl_'), model: body.model, created: 1_790_000_000 };
  const toolCalls = reply.type === 'tool' ? [{ id: reply.callId, type: 'function', function: { name: reply.name, arguments: JSON.stringify(reply.args) } }] : undefined;
  const finish = toolCalls ? 'tool_calls' : 'stop';
  const usage = { prompt_tokens: USAGE.input, completion_tokens: USAGE.output, total_tokens: USAGE.input + USAGE.output };
  if (!body.stream) {
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ ...base, object: 'chat.completion', choices: [{ index: 0, message: { role: 'assistant', content: toolCalls ? null : reply.text, tool_calls: toolCalls }, finish_reason: finish }], usage }));
    return;
  }
  res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' });
  const send = (data) => res.write(`data: ${JSON.stringify({ ...base, object: 'chat.completion.chunk', ...data })}\n\n`);
  send({ choices: [{ index: 0, delta: { role: 'assistant' }, finish_reason: null }] });
  if (toolCalls) {
    const args = toolCalls[0].function.arguments;
    send({ choices: [{ index: 0, delta: { tool_calls: [{ index: 0, id: reply.callId, type: 'function', function: { name: reply.name, arguments: '' } }] }, finish_reason: null }] });
    const mid = Math.floor(args.length / 2);
    for (const part of [args.slice(0, mid), args.slice(mid)]) {
      send({ choices: [{ index: 0, delta: { tool_calls: [{ index: 0, function: { arguments: part } }] }, finish_reason: null }] });
    }
  } else {
    send({ choices: [{ index: 0, delta: { content: reply.text }, finish_reason: null }] });
  }
  send({ choices: [{ index: 0, delta: {}, finish_reason: finish }] });
  send({ choices: [], usage });
  res.end('data: [DONE]\n\n');
}

// ---- responses -----------------------------------------------------------------------------
function responses(body, reply, res) {
  const respId = id('resp_');
  const item = reply.type === 'tool'
    ? { type: 'function_call', id: id('fc_'), call_id: reply.callId, name: reply.name, arguments: JSON.stringify(reply.args), status: 'completed' }
    : { type: 'message', id: id('msg_'), role: 'assistant', status: 'completed', content: [{ type: 'output_text', text: reply.text, annotations: [] }] };
  const usage = { input_tokens: USAGE.input, output_tokens: USAGE.output, total_tokens: USAGE.input + USAGE.output, input_tokens_details: { cached_tokens: 0 }, output_tokens_details: { reasoning_tokens: 0 } };
  const finalResponse = { id: respId, object: 'response', created_at: 1_790_000_000, model: body.model, status: 'completed', output: [item], usage };
  if (!body.stream) {
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(JSON.stringify(finalResponse));
    return;
  }
  res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' });
  let seq = 0;
  const send = (type, data) => res.write(`event: ${type}\ndata: ${JSON.stringify({ type, sequence_number: seq++, ...data })}\n\n`);
  send('response.created', { response: { ...finalResponse, status: 'in_progress', output: [], usage: null } });
  if (item.type === 'function_call') {
    send('response.output_item.added', { output_index: 0, item: { ...item, arguments: '', status: 'in_progress' } });
    const mid = Math.floor(item.arguments.length / 2);
    for (const delta of [item.arguments.slice(0, mid), item.arguments.slice(mid)]) {
      send('response.function_call_arguments.delta', { output_index: 0, item_id: item.id, delta });
    }
    send('response.function_call_arguments.done', { output_index: 0, item_id: item.id, arguments: item.arguments });
    send('response.output_item.done', { output_index: 0, item });
  } else {
    send('response.output_item.added', { output_index: 0, item: { ...item, content: [], status: 'in_progress' } });
    send('response.content_part.added', { output_index: 0, content_index: 0, item_id: item.id, part: { type: 'output_text', text: '', annotations: [] } });
    send('response.output_text.delta', { output_index: 0, content_index: 0, item_id: item.id, delta: reply.text });
    send('response.output_text.done', { output_index: 0, content_index: 0, item_id: item.id, text: reply.text });
    send('response.content_part.done', { output_index: 0, content_index: 0, item_id: item.id, part: item.content[0] });
    send('response.output_item.done', { output_index: 0, item });
  }
  send('response.completed', { response: finalResponse });
  res.end();
}

// ---- messages ------------------------------------------------------------------------------
function messages(body, reply, res) {
  const msgId = id('msg_');
  const content = reply.type === 'tool'
    ? [{ type: 'tool_use', id: reply.callId.replace('call_', 'toolu_'), name: reply.name, input: reply.args }]
    : [{ type: 'text', text: reply.text }];
  const stop = reply.type === 'tool' ? 'tool_use' : 'end_turn';
  const message = { id: msgId, type: 'message', role: 'assistant', model: body.model, content, stop_reason: stop, stop_sequence: null, usage: { input_tokens: USAGE.input, output_tokens: USAGE.output } };
  if (!body.stream) {
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(JSON.stringify(message));
    return;
  }
  res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' });
  const send = (type, data) => res.write(`event: ${type}\ndata: ${JSON.stringify({ type, ...data })}\n\n`);
  send('message_start', { message: { ...message, content: [], stop_reason: null, usage: { input_tokens: USAGE.input, output_tokens: 0 } } });
  send('ping', {});
  if (content[0].type === 'tool_use') {
    send('content_block_start', { index: 0, content_block: { ...content[0], input: {} } });
    const json = JSON.stringify(content[0].input);
    const mid = Math.floor(json.length / 2);
    for (const part of [json.slice(0, mid), json.slice(mid)]) {
      send('content_block_delta', { index: 0, delta: { type: 'input_json_delta', partial_json: part } });
    }
  } else {
    send('content_block_start', { index: 0, content_block: { type: 'text', text: '' } });
    send('content_block_delta', { index: 0, delta: { type: 'text_delta', text: reply.text } });
  }
  send('content_block_stop', { index: 0 });
  send('message_delta', { delta: { stop_reason: stop, stop_sequence: null }, usage: { output_tokens: USAGE.output } });
  send('message_stop', {});
  res.end();
}

// A long text stream that the harness cancels half-way; reports whether the peer went away.
async function slowStream(format, body, res, requestId, chunks) {
  let closed = false;
  res.on('close', () => {
    if (!res.writableFinished) {
      closed = true;
      log({ kind: 'aborted', request_id: requestId });
    }
  });
  const sse = (event, data) => `event: ${event}\ndata: ${JSON.stringify({ type: event, ...data })}\n\n`;
  const frame = {
    chat: (text) => `data: ${JSON.stringify({ id: 'slow', object: 'chat.completion.chunk', model: body.model, choices: [{ index: 0, delta: { content: text }, finish_reason: null }] })}\n\n`,
    responses: (text) => sse('response.output_text.delta', { output_index: 0, content_index: 0, item_id: 'msg_slow', delta: text }),
    messages: (text) => sse('content_block_delta', { index: 0, delta: { type: 'text_delta', text } }),
  }[format];
  res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' });
  if (format === 'chat') {
    res.write(`data: ${JSON.stringify({ id: 'slow', object: 'chat.completion.chunk', model: body.model, choices: [{ index: 0, delta: { role: 'assistant' }, finish_reason: null }] })}\n\n`);
  }
  if (format === 'responses') {
    res.write(sse('response.created', { response: { id: 'resp_slow', object: 'response', model: body.model, status: 'in_progress', output: [] } }));
    res.write(sse('response.output_item.added', { output_index: 0, item: { type: 'message', id: 'msg_slow', role: 'assistant', content: [], status: 'in_progress' } }));
    res.write(sse('response.content_part.added', { output_index: 0, content_index: 0, item_id: 'msg_slow', part: { type: 'output_text', text: '', annotations: [] } }));
  }
  if (format === 'messages') {
    res.write(sse('message_start', { message: { id: 'msg_slow', type: 'message', role: 'assistant', model: body.model, content: [], usage: { input_tokens: 1, output_tokens: 0 } } }));
    res.write(sse('content_block_start', { index: 0, content_block: { type: 'text', text: '' } }));
  }
  log({ kind: 'stream_started', request_id: requestId });
  for (let i = 0; i < chunks && !closed; i += 1) {
    res.write(frame(`slow-${i} `));
    await sleep(200);
  }
  if (!closed) {
    res.end();
    log({ kind: 'stream_finished', request_id: requestId });
  }
}

// ---- scripted upstream failures ------------------------------------------------------------
// Bodies mimic what real providers send, with a marker (`E2E_UPSTREAM_ERROR_*`) the harness greps for.
function upstreamError(format, body, kind, res) {
  const detail = { chat: (m, t, c) => ({ error: { message: m, type: t, code: c, param: null } }),
    responses: (m, t, c) => ({ error: { message: m, type: t, code: c, param: null } }),
    messages: (m, t) => ({ type: 'error', error: { type: t, message: m } }) }[format];
  const extra = { 'x-request-id': 'upstream-req-e2e', 'x-ratelimit-remaining': '0' };
  if (kind === 'err_400') {
    res.writeHead(400, { 'content-type': 'application/json', ...extra }).end(JSON.stringify(detail('E2E_UPSTREAM_ERROR_400 unsupported parameter: reasoning_effort', 'invalid_request_error', 'unsupported_parameter')));
  } else if (kind === 'err_429') {
    res.writeHead(429, { 'content-type': 'application/json', 'retry-after': '7', ...extra }).end(JSON.stringify(detail('E2E_UPSTREAM_ERROR_429 rate limit reached', format === 'messages' ? 'rate_limit_error' : 'rate_limit_exceeded', 'rate_limit_exceeded')));
  } else if (kind === 'err_500') {
    res.writeHead(500, { 'content-type': 'application/json', ...extra }).end(JSON.stringify(detail('E2E_UPSTREAM_ERROR_500 internal failure', format === 'messages' ? 'api_error' : 'server_error', 'internal')));
  } else if (kind === 'err_html') {
    res.writeHead(502, { 'content-type': 'text/html' }).end('<html><body><h1>502 Bad Gateway</h1><p>E2E_UPSTREAM_ERROR_HTML</p></body></html>');
  } else {
    // 200 + SSE that fails half-way: the failure only exists in-band.
    res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' });
    if (format === 'chat') {
      res.write(`data: ${JSON.stringify({ id: 'x', object: 'chat.completion.chunk', model: body.model, choices: [{ index: 0, delta: { role: 'assistant', content: 'partial ' }, finish_reason: null }] })}\n\n`);
      res.write(`data: ${JSON.stringify({ error: { message: 'E2E_UPSTREAM_ERROR_STREAM overloaded', type: 'server_error' } })}\n\n`);
    } else if (format === 'responses') {
      res.write(`event: response.created\ndata: ${JSON.stringify({ type: 'response.created', response: { id: 'resp_x', object: 'response', model: body.model, status: 'in_progress', output: [] } })}\n\n`);
      res.write(`event: response.failed\ndata: ${JSON.stringify({ type: 'response.failed', response: { id: 'resp_x', status: 'failed', error: { code: 'server_error', message: 'E2E_UPSTREAM_ERROR_STREAM overloaded' } } })}\n\n`);
    } else {
      res.write(`event: message_start\ndata: ${JSON.stringify({ type: 'message_start', message: { id: 'msg_x', type: 'message', role: 'assistant', model: body.model, content: [], usage: { input_tokens: 1, output_tokens: 0 } } })}\n\n`);
      res.write(`event: error\ndata: ${JSON.stringify({ type: 'error', error: { type: 'overloaded_error', message: 'E2E_UPSTREAM_ERROR_STREAM overloaded' } })}\n\n`);
    }
    res.end();
  }
}

// ---- server --------------------------------------------------------------------------------
// Match by suffix: the gateway may or may not insert a `/v1` depending on the endpoint's base URL.
const formatOf = (path) => (path.endsWith('/chat/completions') ? 'chat' : path.endsWith('/responses') ? 'responses' : path.endsWith('/messages') ? 'messages' : null);
const KEPT_HEADERS = ['user-agent', 'anthropic-beta', 'anthropic-version', 'originator', 'openai-beta', 'x-api-key', 'authorization', 'x-stainless-lang', 'session_id', 'x-claude-code-session-id'];

const server = http.createServer(async (req, res) => {
  const path = (req.url ?? '').split('?')[0];
  if (req.method === 'GET' && path === '/health') {
    res.writeHead(200, { 'content-type': 'application/json' }).end('{"synthetic":true}');
    return;
  }
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    if (size > 8 * 1024 * 1024) { res.writeHead(413).end(); return; }
    chunks.push(chunk);
  }
  let body = {};
  try { body = JSON.parse(Buffer.concat(chunks).toString() || '{}'); } catch { res.writeHead(400).end(); return; }

  if (path.endsWith('/messages/count_tokens')) {
    res.writeHead(200, { 'content-type': 'application/json' }).end('{"input_tokens":10}');
    return;
  }
  const format = formatOf(path);
  const requestId = id('req_');
  const headers = Object.fromEntries(KEPT_HEADERS.filter((h) => req.headers[h] !== undefined).map((h) => [h, /^(authorization|x-api-key)$/.test(h) ? '[redacted]' : req.headers[h]]));
  if (!format || req.method !== 'POST') {
    log({ kind: 'unknown', request_id: requestId, method: req.method, path });
    res.writeHead(404, { 'content-type': 'application/json' }).end('{"error":{"message":"synthetic upstream: unknown path"}}');
    return;
  }
  const info = inspect(format, body);
  const reply = plan(info);
  log({ kind: 'request', request_id: requestId, format, path, model: body.model, stream: !!body.stream, scenario: info.scenario?.name ?? null, tools: info.toolNames, tool_results: info.results, reply: reply.type === 'tool' ? { tool: reply.name } : { [reply.type]: true }, headers, body });
  if (reply.type === 'error') { upstreamError(format, body, reply.kind, res); return; }
  if (reply.type === 'slow') { await slowStream(format, body, res, requestId, reply.chunks); return; }
  ({ chat, responses, messages })[format](body, reply, res);
});
server.listen(port, '127.0.0.1', () => log({ kind: 'listening', port }));
