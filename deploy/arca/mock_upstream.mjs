import http from 'node:http';
import { randomUUID } from 'node:crypto';

// Disposable synthetic endpoint. No external calls, credentials or command execution.
const server = http.createServer(async (req, res) => {
  if (req.method === 'GET' && req.url === '/health') {
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end('{"synthetic":true}');
    return;
  }
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    if (size > 2 * 1024 * 1024) {
      res.writeHead(413).end();
      return;
    }
    chunks.push(chunk);
  }
  let body;
  try { body = JSON.parse(Buffer.concat(chunks).toString()); }
  catch { res.writeHead(400).end(); return; }
  if (!req.url?.endsWith('/chat/completions') || !Array.isArray(body.messages)) {
    res.writeHead(400, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ error: { type: 'invalid_request_error', message: 'Expected synthetic Chat Completions request' } }));
    return;
  }
  const continuation = body.messages.some(m => m.role === 'tool' && String(m.content).includes('nonce_read_success'));
  const tool = (body.tools ?? []).find(t => t.type === 'function' && t.function?.name);
  const toolCall = !continuation && tool
    ? { id: 'call_' + randomUUID().replaceAll('-', ''), type: 'function', function: { name: tool.function.name, arguments: '{"nonce":"test"}' } }
    : undefined;
  const base = { id: 'chatcmpl_synthetic_' + randomUUID().replaceAll('-', ''), model: body.model, created: 172800 };
  const message = toolCall
    ? { role: 'assistant', content: null, tool_calls: [toolCall] }
    : { role: 'assistant', content: continuation ? 'nonce_read_success verified' : 'synthetic hello' };
  const usage = { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 };
  console.log(JSON.stringify({ synthetic: true, path: req.url, stream: !!body.stream, tools: (body.tools ?? []).length, continuation }));
  if (!body.stream) {
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ ...base, object: 'chat.completion', choices: [{ index: 0, message, finish_reason: toolCall ? 'tool_calls' : 'stop' }], usage }));
    return;
  }
  res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache', connection: 'keep-alive' });
  const send = data => res.write(`data: ${JSON.stringify({ ...base, object: 'chat.completion.chunk', ...data })}\n\n`);
  send({ choices: [{ index: 0, delta: { role: 'assistant' }, finish_reason: null }] });
  if (toolCall) {
    send({ choices: [{ index: 0, delta: { tool_calls: [{ index: 0, id: toolCall.id, type: 'function', function: { name: toolCall.function.name, arguments: '' } }] }, finish_reason: null }] });
    for (const argumentsPart of ['{"nonce":', '"test"}']) {
      send({ choices: [{ index: 0, delta: { tool_calls: [{ index: 0, function: { arguments: argumentsPart } }] }, finish_reason: null }] });
    }
  } else {
    send({ choices: [{ index: 0, delta: { content: message.content }, finish_reason: null }] });
  }
  send({ choices: [{ index: 0, delta: {}, finish_reason: toolCall ? 'tool_calls' : 'stop' }] });
  send({ choices: [], usage });
  res.end('data: [DONE]\n\n');
});
server.listen(18086, '0.0.0.0');
