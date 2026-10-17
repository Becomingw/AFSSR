import http from 'node:http';
import { randomUUID } from 'node:crypto';
import { WebSocketServer } from 'ws';

// Native Responses WS fixture, separate from the HTTP Chat conversion fixture.
const server = http.createServer((_request, response) => response.writeHead(404).end());
const sockets = new WebSocketServer({ noServer: true, maxPayload: 2 * 1024 * 1024 });
server.on('upgrade', (request, socket, head) => {
  if (request.url !== '/v1/responses' || request.headers.authorization !== 'Bearer synthetic-not-a-real-key') {
    socket.end('HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n');
    return;
  }
  sockets.handleUpgrade(request, socket, head, connection => sockets.emit('connection', connection));
});
sockets.on('connection', socket => {
  const previous = new Map();
  socket.on('message', raw => {
    let body;
    try { body = JSON.parse(raw.toString()); }
    catch { socket.close(1003); return; }
    if (body.type !== 'response.create' || !Array.isArray(body.input)) {
      socket.send(JSON.stringify({ type: 'error', status: 400, error: { code: 'invalid_request_error', message: 'Expected synthetic response.create' } }));
      return;
    }
    const output = body.input.find(item => item.type === 'function_call_output');
    if (output && (previous.get(body.previous_response_id) !== output.call_id || output.output !== 'nonce_read_success')) {
      socket.send(JSON.stringify({ type: 'error', status: 400, error: { code: 'invalid_request_error', message: 'Synthetic continuation did not match' } }));
      return;
    }
    const id = 'resp_' + randomUUID().replaceAll('-', '');
    const callId = 'call_' + randomUUID().replaceAll('-', '');
    const item = output
      ? { type: 'message', id: 'msg_' + randomUUID(), role: 'assistant', status: 'completed', content: [{ type: 'output_text', text: 'nonce_read_success verified', annotations: [] }] }
      : { type: 'function_call', id: 'fc_' + randomUUID(), call_id: callId, name: 'read_nonce', arguments: '{"nonce":"test"}', status: 'completed' };
    if (!output && !body.tools?.some(tool => tool.type === 'function' && tool.name === 'read_nonce')) {
      socket.close(1003); return;
    }
    previous.set(id, callId);
    const response = { id, object: 'response', created_at: Math.floor(Date.now() / 1000), model: body.model, status: 'completed', output: [item], error: null, incomplete_details: null, usage: { input_tokens: 10, output_tokens: 5, total_tokens: 15, input_tokens_details: { cached_tokens: 0 }, output_tokens_details: { reasoning_tokens: 0 } } };
    let sequence = 0;
    const send = event => socket.send(JSON.stringify({ sequence_number: sequence++, ...event }));
    send({ type: 'response.created', response: { ...response, status: 'in_progress', output: [], usage: null } });
    send({ type: 'response.output_item.added', output_index: 0, item: { ...item, ...(output ? {} : { arguments: '' }), status: 'in_progress' } });
    if (!output) {
      for (const delta of ['{"nonce":', '"test"}']) send({ type: 'response.function_call_arguments.delta', output_index: 0, item_id: item.id, delta });
      send({ type: 'response.function_call_arguments.done', output_index: 0, item_id: item.id, arguments: item.arguments });
    } else {
      send({ type: 'response.output_text.delta', output_index: 0, content_index: 0, item_id: item.id, delta: item.content[0].text });
      send({ type: 'response.output_text.done', output_index: 0, content_index: 0, item_id: item.id, text: item.content[0].text });
    }
    send({ type: 'response.output_item.done', output_index: 0, item });
    send({ type: 'response.completed', response });
    console.log(JSON.stringify({ synthetic: true, protocol: 'responses-websocket', continuation: Boolean(output) }));
  });
});
server.listen(18087, '0.0.0.0');
