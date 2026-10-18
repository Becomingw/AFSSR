// "OpenCode Go" behaviour for the scripted upstream.
//
// Mirrors what OpenCode Go really does, as documented at https://opencode.ai/docs/go/ and as seen in
// production failures: every model is served by exactly ONE native protocol and anything else is
// refused, and each protocol's backend validates requests strictly. The error texts below are the
// real ones (copied from rejected requests), so a conversion bug in the gateway shows up here the
// same way it does against the real service.
//
// Not the real service: model quality, limits and billing are not modelled.

// Documented protocol per model (docs table + /v1/models). `omen-alpha` has no documented protocol;
// it is treated as chat here and flagged `undocumented` so tests can see the gateway does not rely
// on a guess.
export const GO_MODELS = {
  responses: ['gpt-6-luna', 'gpt-5.6-luna', 'grok-4.6', 'grok-4.7', 'muse-spark-1.2-contributor', 'muse-spark-1.3-contributor'],
  messages: ['claude-haiku-5-5', 'minimax-m2.5', 'minimax-m2.7', 'minimax-m3', 'qwen3.6-plus', 'qwen3.7-max', 'qwen3.7-plus', 'qwen3.8-max', 'qwen3.8-flash'],
  gemini: ['gemini-4-argon-contributor'],
  chat: ['beam-free', 'deepseek-v4-pro', 'deepseek-v4.1-flash', 'glm-5.1', 'glm-5.2', 'glm-5.3', 'glm-5.3-flash', 'hy3', 'hy4-preview', 'kimi-k2.6', 'kimi-k2.7-code', 'kimi-k3', 'longcat-2.0', 'longcat-2.5-preview-free', 'mimo-v2.5', 'mimo-v2.5-pro', 'mimo-v2.6-flash', 'mimo-v2.6-pro', 'omen-alpha', 'space-bunny', 'step-5-preview'],
};
export const UNDOCUMENTED = new Set(['omen-alpha']);
export const ALL_MODELS = Object.values(GO_MODELS).flat();
export const protocolOf = (model) => Object.entries(GO_MODELS).find(([, models]) => models.includes(model))?.[0] ?? null;
const OPENAI_BACKED = new Set(['gpt-6-luna', 'gpt-5.6-luna']);
// Grok/Muse refuse `custom` and `namespace` tools; only xAI documents a hosted `web_search` (and rejects
// OpenAI's redundant `external_web_access`). Muse is treated as function-only.
const FUNCTION_ONLY_RESPONSES = new Set(['grok-4.6', 'grok-4.7', 'muse-spark-1.2-contributor', 'muse-spark-1.3-contributor']);
const XAI_WEB_SEARCH = new Set(['grok-4.6', 'grok-4.7']);
const NEW_THINKING_MODELS = new Set(['qwen3.8-max', 'qwen3.8-flash', 'claude-haiku-5-5']);
const TRAINS_ON_DATA = (model) => model.endsWith('-contributor');
const CHAT_MESSAGE_FIELDS = new Set(['role', 'content', 'tool_calls', 'tool_call_id', 'reasoning_content', 'name', 'refusal']);
const ANTHROPIC_TOP_LEVEL = new Set(['model', 'messages', 'max_tokens', 'system', 'tools', 'tool_choice', 'stream', 'temperature', 'top_p', 'top_k', 'stop_sequences', 'thinking', 'metadata', 'output_config', 'service_tier']);

/** Maps a request path to { protocol, model } or null. Accepts the documented vendor-prefixed paths, the short ones and the legacy /zen/go ones. */
export function route(path) {
  const clean = path.replace(/^\/(inference|zen)\/go/, '').replace(/^\/(openai|anthropic|google)(?=\/)/, '');
  if (/\/v1\/chat\/completions$/.test(clean)) return { protocol: 'chat' };
  if (/\/v1\/responses$/.test(clean)) return { protocol: 'responses' };
  if (/\/v1\/messages$/.test(clean)) return { protocol: 'messages' };
  const gemini = /\/v1(?:beta)?\/models\/([^/:]+):(generateContent|streamGenerateContent)$/.exec(clean);
  if (gemini) return { protocol: 'gemini', model: decodeURIComponent(gemini[1]), stream: gemini[2] === 'streamGenerateContent' };
  return null;
}

const fail = (protocol, status, message, type = 'invalid_request_error') => ({
  status,
  body: protocol === 'messages'
    ? { type: 'error', error: { type, message } }
    : protocol === 'gemini'
      ? { error: { code: status, message, status: 'INVALID_ARGUMENT' } }
      : { error: { message, type, code: null, param: null } },
});
const upstreamFailed = (protocol, tag, message) => fail(protocol, 400, `Upstream request failed: [${tag}] ${message}`);

const hasContent = (value) => Array.isArray(value) ? value.length > 0 : value != null;

/** Returns { status, body } when the real service would refuse this request, else null. */
export function validate(protocol, model, body, { allowTraining = true } = {}) {
  const native = protocolOf(model);
  if (!native) return fail(protocol, 404, `Model ${model} not found`, 'not_found_error');
  if (native !== protocol) return fail(protocol, 400, 'Model does not support this protocol.');
  if (TRAINS_ON_DATA(model) && !allowTraining) {
    return fail(protocol, 400, "Upstream request failed: This Go model trains on request data. Allow paid endpoints that train on request data in your workspace's Privacy settings to use it.");
  }
  if (protocol === 'chat') return validateChat(body);
  if (protocol === 'responses') return validateResponses(model, body);
  if (protocol === 'messages') return validateMessages(model, body);
  return validateGemini(body);
}

function validateChat(body) {
  const tools = body.tools ?? [];
  for (const [i, tool] of tools.entries()) {
    if (tool.type === 'custom') {
      const syntax = tool.custom?.format?.grammar?.syntax;
      if (tool.custom?.format?.type === 'grammar' && !['lark', 'regex'].includes(syntax)) {
        return upstreamFailed('chat', 'invalid_value', `tools[${i}]: custom.format.grammar.syntax must be "lark" or "regex", got "${syntax ?? ''}"`);
      }
      continue;
    }
    if (tool.type !== 'function') return upstreamFailed('chat', 'invalid_value', `tools[${i}].type: unsupported tool type "${tool.type}"`);
    const fn = tool.function ?? {};
    if (!fn.description) return upstreamFailed('chat', 'missing_required_parameter', `tools[${i}]: function.description is required`);
    const params = fn.parameters;
    if (!params || typeof params !== 'object' || !Object.keys(params).length) {
      return upstreamFailed('chat', 'bad_request_error', 'invalid params, function parameters is empty (2013)');
    }
    if (params.type !== 'object') return upstreamFailed('chat', 'invalid_request_error', `tools[${i}].function.parameters: schema must have type "object"`);
  }
  for (const [i, message] of (body.messages ?? []).entries()) {
    if (!['system', 'developer', 'user', 'assistant', 'tool'].includes(message.role)) {
      return upstreamFailed('chat', 'invalid_request_error', `messages[${i}].role: unsupported role "${message.role}"`);
    }
    // Real beam-free: "Unrecognized request argument supplied: messages[4].reasoning_parts."
    for (const key of Object.keys(message)) {
      if (!CHAT_MESSAGE_FIELDS.has(key)) return fail('chat', 400, `Upstream request failed: [unsupported_parameter] Unrecognized request argument supplied: messages[${i}].${key}.`);
    }
    if (message.role === 'tool' && !message.tool_call_id) return upstreamFailed('chat', 'invalid_request_error', `messages[${i}]: tool_call_id is required`);
  }
  return null;
}

function validateResponses(model, body) {
  const input = Array.isArray(body.input) ? body.input : [];
  if (OPENAI_BACKED.has(model)) {
    for (const [i, item] of input.entries()) {
      if (item?.type === 'reasoning' && hasContent(item.content) && (!Array.isArray(item.content) || item.content.length > 0)) {
        return fail('responses', 400, `Invalid 'input[${i}].content': array too long. Expected an array with maximum length 0, but got an array with length ${Array.isArray(item.content) ? item.content.length : 1} instead.`);
      }
    }
  }
  const functionOnly = FUNCTION_ONLY_RESPONSES.has(model);
  for (const [i, tool] of (body.tools ?? []).entries()) {
    if (tool.search_content_types !== undefined && tool.type !== 'web_search_preview') {
      return fail('responses', 400, '`tools[].search_content_types` is only supported for web_search_preview tools.');
    }
    if (tool.type === 'web_search' && tool.external_web_access !== undefined && XAI_WEB_SEARCH.has(model)) {
      return fail('responses', 400, 'Unknown parameter: tools[].external_web_access');
    }
    const supported = functionOnly ? ['function', ...(XAI_WEB_SEARCH.has(model) ? ['web_search'] : [])] : ['function', 'custom', 'web_search', 'web_search_preview'];
    if (!supported.includes(tool.type)) return fail('responses', 400, `Invalid value: '${tool.type}'. Supported values are: ${supported.map((t) => `'${t}'`).join(', ')}. (tools[${i}].type)`);
    if (tool.type === 'function' && !tool.name) return fail('responses', 400, `Missing required parameter: 'tools[${i}].name'.`);
  }
  for (const [i, item] of input.entries()) {
    if (item?.type === 'reasoning' && item.content === null && functionOnly) {
      return fail('responses', 400, `Invalid 'input[${i}].content': expected an array, got null.`);
    }
  }
  return null;
}

function validateMessages(model, body) {
  for (const key of Object.keys(body)) {
    if (!ANTHROPIC_TOP_LEVEL.has(key)) return upstreamFailed('messages', 'invalid_request_error', `${key}: Extra inputs are not permitted`);
  }
  for (const [i, tool] of (body.tools ?? []).entries()) {
    if (tool.type && tool.type !== 'custom') {
      return upstreamFailed('messages', 'invalid_request_error', `tools.${i}: Input tag '${tool.type}' found using 'type' does not match any of the expected tags: 'custom'`);
    }
    const schema = tool.input_schema;
    if (!schema || typeof schema !== 'object') return upstreamFailed('messages', 'invalid_request_error', `tools.${i}.custom.input_schema: Field required`);
    if (['oneOf', 'allOf', 'anyOf'].some((k) => k in schema)) {
      return upstreamFailed('messages', 'invalid_request_error', `tools.${i}.custom.input_schema: input_schema does not support oneOf, allOf, or anyOf at the top level`);
    }
    if (schema.type !== 'object') return upstreamFailed('messages', 'invalid_request_error', `tools.${i}.custom.input_schema.type: Input should be 'object'`);
  }
  if (body.thinking?.type === 'enabled' && NEW_THINKING_MODELS.has(model)) {
    return upstreamFailed('messages', 'invalid_request_error', '"thinking.type.enabled" is not supported for this model. Use "thinking.type.adaptive" and "output_config.effort" to control thinking behavior.');
  }
  for (const [i, message] of (body.messages ?? []).entries()) {
    if (!['user', 'assistant'].includes(message.role)) {
      return upstreamFailed('messages', 'invalid_request_error', `messages.${i}.role: Input should be 'user' or 'assistant'`);
    }
    for (const [j, block] of (Array.isArray(message.content) ? message.content : []).entries()) {
      if (block.type === 'thinking' && !block.signature) {
        return upstreamFailed('messages', 'invalid_request_error', `messages.${i}.content.${j}.thinking.signature: Field required`);
      }
      if (block.type === 'text' && typeof block.text === 'string' && !block.text.length && message.role === 'user') {
        return upstreamFailed('messages', 'invalid_request_error', `messages.${i}.content.${j}.text: text content blocks must be non-empty`);
      }
    }
  }
  return null;
}

const GEMINI_SCHEMA_KEYS = new Set(['type', 'format', 'description', 'nullable', 'enum', 'properties', 'required', 'items', 'minItems', 'maxItems', 'minimum', 'maximum', 'anyOf', 'title', 'minLength', 'maxLength', 'pattern', 'propertyOrdering']);
function geminiSchemaProblem(schema, path) {
  if (!schema || typeof schema !== 'object') return null;
  for (const key of Object.keys(schema)) {
    if (!GEMINI_SCHEMA_KEYS.has(key)) return `Unknown name "${key}" at '${path}'`;
  }
  for (const [name, sub] of Object.entries(schema.properties ?? {})) {
    const problem = geminiSchemaProblem(sub, `${path}.properties[${name}]`);
    if (problem) return problem;
  }
  return geminiSchemaProblem(schema.items, `${path}.items`);
}
function validateGemini(body) {
  for (const [t, tool] of (body.tools ?? []).entries()) {
    for (const [d, declaration] of (tool.functionDeclarations ?? []).entries()) {
      const problem = geminiSchemaProblem(declaration.parameters, `tools[${t}].function_declarations[${d}].parameters`);
      if (problem) return fail('gemini', 400, `Invalid JSON payload received. ${problem}`);
    }
  }
  return null;
}

export const modelsListing = () => ({ object: 'list', data: ALL_MODELS.map((id) => ({ id, object: 'model', created: 1792325150, owned_by: 'opencode' })) });
