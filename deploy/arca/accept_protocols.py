#!/usr/bin/env python3
"""Live gateway protocol/tool acceptance with an isolated deterministic upstream."""
import copy
import json
import pathlib
import os
import subprocess
import time
import urllib.error
import urllib.request
import uuid

from accept_control import BASE, ROOT, Client, OPENER, check

SCHEMA = {'type': 'object', 'properties': {'nonce': {'type': 'string'}}, 'required': ['nonce'], 'additionalProperties': False}


def websocket_probe(token, model, quota=False):
    environment = {**os.environ, 'ARCA_TEST_INFERENCE_KEY': token, 'ARCA_TEST_MODEL': model, 'ARCA_WS_EXPECT_QUOTA': '1' if quota else '0'}
    command = ['docker', 'run', '--rm', '--init', '--name', 'arca-ws-protocol-test-20261016',
               '--network', 'host', '--cpus=0.5', '--memory=192m', '--pids-limit=64', '--ulimit', 'core=0',
               '--cap-drop=ALL', '--security-opt', 'no-new-privileges',
               '-v', '/opt/arca-test-20261016/browser:/work:ro', '-w', '/work']
    for name in ['ARCA_TEST_INFERENCE_KEY', 'ARCA_TEST_MODEL', 'ARCA_WS_EXPECT_QUOTA']:
        command.extend(['-e', name])
    command.extend(['mcr.microsoft.com/playwright:v1.56.1-noble', 'node', '/work/ws_roundtrip.mjs'])
    result = subprocess.run(command, env=environment, capture_output=True, text=True, timeout=100)
    if result.returncode:
        try:
            detail = json.loads(result.stdout.strip()).get('reason', '')
        except (json.JSONDecodeError, AttributeError):
            detail = 'fixture process failed'
        allowed = ['WebSocket handshake failed', 'WebSocket turn timed out', 'Invalid WebSocket JSON event', 'fixture process failed']
        if detail not in allowed and not detail.startswith(('WebSocket handshake HTTP ', 'WebSocket protocol error code=')):
            detail = 'fixture assertion failed'
        raise AssertionError(('Synthetic WebSocket budget probe failed: ' if quota else 'Synthetic WebSocket tool round-trip failed: ') + detail)
    report = json.loads(result.stdout.strip())
    check(report['status'] == 'passed', 'real WebSocket budget enforcement' if quota else 'real WebSocket tool-result continuation')


def request_body(protocol, model):
    common = {'model': model, 'stream': False}
    if protocol == 'responses':
        return {**common, 'input': [{'role': 'user', 'content': 'Read the nonce'}], 'max_output_tokens': 64,
                'tools': [{'type': 'function', 'name': 'read_nonce', 'description': 'Read a nonce', 'parameters': SCHEMA}]}
    if protocol == 'chat':
        return {**common, 'messages': [{'role': 'user', 'content': 'Read the nonce'}], 'max_tokens': 64,
                'tools': [{'type': 'function', 'function': {'name': 'read_nonce', 'description': 'Read a nonce', 'parameters': SCHEMA}}]}
    return {**common, 'messages': [{'role': 'user', 'content': 'Read the nonce'}], 'max_tokens': 64,
            'tools': [{'name': 'read_nonce', 'description': 'Read a nonce', 'input_schema': SCHEMA}]}


def infer(token, protocol, body):
    path = {'responses': '/v1/responses', 'chat': '/v1/chat/completions', 'messages': '/v1/messages'}[protocol]
    headers = {'Content-Type': 'application/json', 'Authorization': 'Bearer ' + token, 'anthropic-version': '2023-06-01'}
    req = urllib.request.Request(BASE + path, json.dumps(body).encode(), headers, method='POST')
    try:
        response = OPENER.open(req, timeout=60)
    except urllib.error.HTTPError as error:
        response = error
    raw = response.read(4 * 1024 * 1024)
    if response.status != 200:
        raise AssertionError(f'Protocol {protocol} returned HTTP {response.status}')
    if body.get('stream'):
        check(response.headers.get('Content-Type', '').startswith('text/event-stream'), protocol + ' has actual SSE content type')
        records = []
        for line in raw.decode().splitlines():
            if line.startswith('data: ') and line[6:] != '[DONE]':
                records.append(json.loads(line[6:]))
        check(bool(records) and not any(r.get('type') == 'error' or 'error' in r for r in records), protocol + ' SSE has events and no error')
        return records
    return json.loads(raw)


def call_from_result(protocol, result, streaming):
    if protocol == 'responses':
        if streaming:
            completed = next((r['response'] for r in reversed(result) if r.get('type') == 'response.completed'), None)
            check(completed is not None, 'Responses SSE terminates with response.completed')
            result = completed
        call = next(item for item in result['output'] if item['type'] == 'function_call')
        return call, call['name'], json.loads(call['arguments']), call['call_id']
    if protocol == 'chat':
        if streaming:
            calls = {}
            for record in result:
                for choice in record.get('choices', []):
                    for item in choice.get('delta', {}).get('tool_calls', []):
                        call = calls.setdefault(item['index'], {'id': '', 'type': 'function', 'function': {'name': '', 'arguments': ''}})
                        if item.get('id'):
                            call['id'] = item['id']
                        for field in ['name', 'arguments']:
                            call['function'][field] += item.get('function', {}).get(field, '')
            call = calls[0]
        else:
            call = result['choices'][0]['message']['tool_calls'][0]
        return call, call['function']['name'], json.loads(call['function']['arguments']), call['id']
    if streaming:
        call = None
        arguments = ''
        for event in result:
            if event.get('type') == 'content_block_start' and event.get('content_block', {}).get('type') == 'tool_use':
                call = event['content_block']
            if event.get('type') == 'content_block_delta' and event.get('delta', {}).get('type') == 'input_json_delta':
                arguments += event['delta'].get('partial_json', '')
        check(any(event.get('type') == 'message_stop' for event in result), 'Claude SSE has message_stop')
        if arguments:
            call['input'] = json.loads(arguments)
    else:
        call = next(item for item in result['content'] if item['type'] == 'tool_use')
    return call, call['name'], call['input'], call['id']


def continue_body(protocol, body, call, call_id):
    body = copy.deepcopy(body)
    if protocol == 'responses':
        body['input'].extend([call, {'type': 'function_call_output', 'call_id': call_id, 'output': 'nonce_read_success'}])
    elif protocol == 'chat':
        body['messages'].extend([{'role': 'assistant', 'content': None, 'tool_calls': [call]}, {'role': 'tool', 'tool_call_id': call_id, 'content': 'nonce_read_success'}])
    else:
        body['messages'].extend([{'role': 'assistant', 'content': [call]}, {'role': 'user', 'content': [{'type': 'tool_result', 'tool_use_id': call_id, 'content': 'nonce_read_success'}]}])
    return body


def main():
    if pathlib.Path.cwd().resolve() != ROOT:
        raise AssertionError('Restricted to disposable Korean acceptance')
    env = dict(line.split('=', 1) for line in (ROOT / 'deploy/arca/.test.env').read_text().splitlines() if line)
    admin = Client()
    actor = admin.login('arca-accept-admin@example.invalid', env['ARCA_TEST_ADMIN_PASSWORD'])
    suffix = uuid.uuid4().hex[:8]
    model = 'accept-protocol-' + suffix
    ws_model = model + '-ws'
    for public_model, api_format, base_url in [(model, 'openai:chat', 'http://mock-upstream:18086/v1'), (ws_model, 'openai:responses', 'http://mock-responses:18087/v1')]:
        provider = admin.request('POST', '/api/admin/providers/', {'name': 'Synthetic ' + public_model, 'provider_type': 'custom', 'billing_type': 'pay_as_you_go', 'is_active': True, 'max_retries': 0}, statuses=(200, 201))
        pid = provider['id']
        provider_options = {'enable_format_conversion': True}
        if api_format == 'openai:responses':
            provider_options['config'] = {'responses_websocket': {'enabled': True}}
        admin.request('PATCH', '/api/admin/providers/' + pid, provider_options)
        admin.request('POST', '/api/admin/endpoints/providers/' + pid + '/endpoints', {'provider_id': pid, 'api_format': api_format, 'base_url': base_url, 'is_active': True}, statuses=(200, 201))
        admin.request('POST', '/api/admin/endpoints/providers/' + pid + '/keys', {'api_formats': [api_format], 'api_key': 'synthetic-not-a-real-key', 'name': 'Synthetic', 'allowed_models': [public_model], 'auto_fetch_models': False}, statuses=(200, 201))
        global_model = admin.request('POST', '/api/admin/models/global', {'name': public_model, 'display_name': 'Acceptance model', 'default_tiered_pricing': {'tiers': [{'up_to': None, 'input_price_per_1m': 1, 'output_price_per_1m': 2}]}, 'supported_capabilities': ['function_calling'], 'config': {'function_calling': True, 'context_limit': 32768, 'output_limit': 4096}, 'is_active': True}, statuses=(200, 201))
        admin.request('POST', '/api/admin/providers/' + pid + '/models', {'provider_model_name': public_model, 'global_model_id': global_model['id'], 'supports_function_calling': True, 'supports_streaming': True, 'is_active': True}, statuses=(200, 201))
    key = admin.request('POST', '/api/users/me/api-keys', {'name': 'Protocol acceptance ' + suffix}, statuses=(200, 201))
    token = key.get('key') or admin.request('GET', '/api/users/me/api-keys/' + key['id'] + '?include_key=true')['key']
    inference = Client(token)
    catalog = inference.request('GET', '/api/opencode/config')
    check(model in catalog['provider']['aether']['models'], 'OpenCode inference-key catalog includes active public model')
    metadata = catalog['provider']['aether']['models'][model]
    check(metadata.get('tool_call') is True and metadata.get('limit') == {'context': 32768, 'output': 4096}, 'OpenCode exposes native model capability and limit metadata')
    admin.request('GET', '/api/opencode/config', statuses=(401,))

    for protocol in ['chat', 'responses', 'messages']:
        for streaming in [False, True]:
            body = request_body(protocol, model)
            body['stream'] = streaming
            result = infer(token, protocol, body)
            call, name, args, call_id = call_from_result(protocol, result, streaming)
            check(name == 'read_nonce' and args == {'nonce': 'test'} and bool(call_id), protocol + (' SSE' if streaming else ' sync') + ' tool identity and JSON arguments')
            continued = continue_body(protocol, body, call, call_id)
            final = infer(token, protocol, continued)
            check('nonce_read_success verified' in json.dumps(final), protocol + (' SSE' if streaming else ' sync') + ' tool result continuation')
    check(True, 'three client protocols execute actual two-round tool flow')
    websocket_probe(token, ws_model)

    # An explicit personal budget must apply equally to HTTP and WebSocket turns.
    policy_path = '/api/admin/system/configs/internal_access_policy'
    previous = admin.request('GET', policy_path)['value']
    check(isinstance(previous, dict), 'control suite installed a restorable test policy')
    bounded = copy.deepcopy(previous)
    bounded.setdefault('users', {})[actor['id']] = {'unlimited': False, 'five_hour_budget_usd': 0.000001, 'models': {}}
    admin.request('PUT', policy_path, {'value': bounded, 'expected_value': previous})
    try:
        inference.request('POST', '/v1/responses', request_body('responses', model), statuses=(429,))
        check(True, 'explicit personal money budget rejects oversized HTTP reservation')
        websocket_probe(token, ws_model, quota=True)
    finally:
        admin.request('PUT', policy_path, {'value': previous, 'expected_value': bounded})

    zero = copy.deepcopy(previous)
    zero.setdefault('users', {})[actor['id']] = {'unlimited': False, 'five_hour_budget_usd': 0, 'models': {}}
    admin.request('PUT', policy_path, {'value': zero, 'expected_value': previous})
    try:
        inference.request('POST', '/v1/responses', request_body('responses', model), statuses=(429,))
        check(True, 'explicit zero budget rejects paid HTTP reservation rather than returning 500')
        websocket_probe(token, ws_model, quota=True)
        check(True, 'explicit zero budget rejects paid WebSocket reservation')
    finally:
        admin.request('PUT', policy_path, {'value': previous, 'expected_value': zero})

    # The mock fails all unknown URLs; a completed request is not a fake readiness response.
    time.sleep(2)
    usage = admin.request('GET', '/api/users/me/usage?limit=30')
    check(model in json.dumps(usage), 'real protocol calls recorded in personal usage')
    admin.request('DELETE', '/api/users/me/api-keys/' + key['id'], statuses=(200, 204))
    inference.request('GET', '/api/opencode/config', statuses=(401,))
    print(json.dumps({'suite': 'protocol-real-http-sse', 'status': 'passed', 'upstreams': ['synthetic-chat-http', 'synthetic-responses-websocket'], 'client_protocols': 3, 'tool_round_trips': 6, 'websocket_tool_round_trips': 1, 'http_and_websocket_budget_enforced': True, 'real_model_claim': False}), flush=True)


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        print(json.dumps({'suite': 'protocol-real-http-sse', 'status': 'failed', 'error_type': type(error).__name__, 'reason': str(error) if isinstance(error, AssertionError) else 'Inspect synthetic-only fixture diagnostics'}), flush=True)
        raise SystemExit(1) from None
