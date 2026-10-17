#!/usr/bin/env python3
"""Real-client compatibility suite: Codex CLI and Claude Code against a scripted mock upstream.

Topology (everything on loopback, nothing leaves the machine):

    real client  ->  Arca gateway (disposable DB, memory runtime)  ->  mock_upstream.mjs

The clients run headless in isolated config/home directories and are pointed at the gateway with a
throwaway API key. The mock upstream plays a deterministic model that calls tools; the suite then
asserts on what the *real client* did (files written, final answer, exit status) and on what the
gateway forwarded to the upstream (format, model mapping, tool result linkage, cancellation).

Usage:
    tests/client_e2e/run.py [--clients codex,claude] [--upstreams chat,responses,messages]
                            [--scenarios write_file,multi_tool,...] [--gateway-bin PATH] [--keep]
"""
import argparse
import getpass
import json
import os
import pathlib
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
import uuid

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))

UPSTREAM_FORMAT = {'chat': 'openai:chat', 'responses': 'openai:responses', 'messages': 'claude:messages'}
SCENARIOS = ['text_only', 'write_file', 'multi_tool', 'shell', 'cancel', 'cancel_drain',
             'err_400', 'err_429', 'err_500', 'err_html', 'err_stream']
# What the mock upstream returns for each error scenario, and what the gateway must have recorded.
ERROR_EXPECT = {
    'err_400': {'status': 400, 'marker': 'E2E_UPSTREAM_ERROR_400', 'headers': {'x-request-id': 'upstream-req-e2e'}},
    'err_429': {'status': 429, 'marker': 'E2E_UPSTREAM_ERROR_429', 'headers': {'retry-after': '7'}},
    'err_500': {'status': 500, 'marker': 'E2E_UPSTREAM_ERROR_500', 'headers': {'x-request-id': 'upstream-req-e2e'}},
    'err_html': {'status': 502, 'marker': 'E2E_UPSTREAM_ERROR_HTML', 'headers': {}},
    # A 200 whose SSE fails half-way: the gateway records the failure with a synthesized status.
    'err_stream': {'status': None, 'marker': 'E2E_UPSTREAM_ERROR_STREAM', 'headers': {}},
}
RECORD_LEVEL_CACHE_SECONDS = 32  # the gateway caches the capture policy for 30s
CLIENT_PROTOCOL = {'codex': 'responses', 'claude': 'messages'}


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def wait_for(url, seconds, what):
    deadline = time.time() + seconds
    while time.time() < deadline:
        try:
            if OPENER.open(url, timeout=2).status == 200:
                return
        except (urllib.error.URLError, OSError):
            pass
        time.sleep(0.4)
    raise SystemExit(f'{what} did not become ready')


class Api:
    def __init__(self, base, token=None):
        self.base, self.token = base, token

    def request(self, method, path, body=None, statuses=(200,)):
        headers = {'Accept': 'application/json', 'X-Client-Device-Id': 'client-e2e'}
        if self.token:
            headers['Authorization'] = 'Bearer ' + self.token
        payload = None if body is None else json.dumps(body).encode()
        if payload is not None:
            headers['Content-Type'] = 'application/json'
        req = urllib.request.Request(self.base + path, payload, headers, method=method)
        try:
            response = OPENER.open(req, timeout=45)
        except urllib.error.HTTPError as error:
            response = error
        raw = response.read(4 * 1024 * 1024)
        if response.status not in statuses:
            raise SystemExit(f'{method} {path.split("?")[0]} -> HTTP {response.status}: {raw[:300]!r}')
        return json.loads(raw) if raw else None


class Stack:
    """Disposable database, mock upstream and gateway, plus provisioned models and an API key."""

    def __init__(self, gateway_bin, keep):
        self.gateway_bin, self.keep = gateway_bin, keep
        self.tmp = pathlib.Path(tempfile.mkdtemp(prefix='arca-client-e2e-'))
        self.db = 'arca_client_e2e_' + uuid.uuid4().hex[:8]
        self.mock_port, self.gw_port = free_port(), free_port()
        self.base = f'http://127.0.0.1:{self.gw_port}'
        self.mock_log = self.tmp / 'mock-upstream.jsonl'
        self.procs = []
        self.models = {}
        self.token = None
        self.record_level_ready_at = 0.0

    def start(self):
        subprocess.run(['createdb', '-h', 'localhost', self.db], check=True)
        db_url = f'postgresql://{getpass.getuser()}@localhost/{self.db}'
        self.spawn('mock', ['node', str(HERE / 'mock_upstream.mjs'), str(self.mock_port), str(self.mock_log)], {})
        wait_for(f'http://127.0.0.1:{self.mock_port}/health', 15, 'mock upstream')
        secret = lambda ch: ch * 64  # noqa: E731  (synthetic, per-run throwaway values)
        admin_password = 'e2e-' + uuid.uuid4().hex
        self.spawn('gateway', [self.gateway_bin], {
            'APP_PORT': str(self.gw_port), 'AETHER_PUBLIC_BASE_URL': self.base, 'AETHER_BASE_DIR': str(self.tmp / 'gw'),
            'AETHER_DATABASE_DRIVER': 'postgres', 'AETHER_DATABASE_URL': db_url, 'DATABASE_URL': db_url,
            'AETHER_RUNTIME_BACKEND': 'memory', 'AETHER_GATEWAY_DATABASE_MODE': 'auto',
            'AETHER_GATEWAY_DEPLOYMENT_TOPOLOGY': 'single-node', 'AETHER_GATEWAY_NODE_ROLE': 'all',
            'AETHER_GATEWAY_DATA_ENCRYPTION_KEY': secret('a'), 'ENCRYPTION_KEY': secret('a'), 'JWT_SECRET_KEY': secret('b'),
            'ADMIN_USERNAME': 'e2e_admin', 'ADMIN_EMAIL': 'e2e-admin@example.invalid', 'ADMIN_PASSWORD': admin_password,
            'AETHER_TUNNEL_RELAY_AUTH_SECRET': secret('c'), 'AFSSR_INTERNAL_MODE': 'true', 'AETHER_UPDATE_STRATEGY': 'manual',
            'RUST_LOG': 'info', 'AETHER_LOG_FORMAT': 'json',
        })
        wait_for(self.base + '/health', 90, 'gateway')
        admin = Api(self.base)
        admin.token = admin.request('POST', '/api/auth/login', {'email': 'e2e-admin@example.invalid', 'password': admin_password, 'auth_type': 'local'})['access_token']
        self.admin = admin
        for upstream, api_format in UPSTREAM_FORMAT.items():
            self.provision(upstream, api_format)
        key = admin.request('POST', '/api/users/me/api-keys', {'name': 'client-e2e'}, statuses=(200, 201))
        self.token = key.get('key') or admin.request('GET', f'/api/users/me/api-keys/{key["id"]}?include_key=true')['key']
        # Keep bodies only for failed requests: this is what the error scenarios assert on, and it
        # shows that successful requests leave no body behind.
        admin.request('PUT', '/api/admin/system/configs/request_record_level', {'value': 'failures'}, statuses=(200, 201))
        self.record_level_ready_at = time.time() + RECORD_LEVEL_CACHE_SECONDS

    def provision(self, upstream, api_format):
        model = f'e2e-{upstream}'
        admin = self.admin
        provider = admin.request('POST', '/api/admin/providers/', {'name': 'Synthetic ' + model, 'provider_type': 'custom', 'billing_type': 'pay_as_you_go', 'is_active': True, 'max_retries': 0}, statuses=(200, 201))
        pid = provider['id']
        admin.request('PATCH', '/api/admin/providers/' + pid, {'enable_format_conversion': True})
        admin.request('POST', f'/api/admin/endpoints/providers/{pid}/endpoints', {'provider_id': pid, 'api_format': api_format, 'base_url': f'http://127.0.0.1:{self.mock_port}/v1', 'is_active': True}, statuses=(200, 201))
        admin.request('POST', f'/api/admin/endpoints/providers/{pid}/keys', {'api_formats': [api_format], 'api_key': 'synthetic-not-a-real-key', 'name': 'Synthetic', 'allowed_models': [model], 'auto_fetch_models': False}, statuses=(200, 201))
        global_model = admin.request('POST', '/api/admin/models/global', {
            'name': model, 'display_name': model, 'is_active': True, 'supported_capabilities': ['function_calling'],
            'default_tiered_pricing': {'tiers': [{'up_to': None, 'input_price_per_1m': 1, 'output_price_per_1m': 2}]},
            'config': {'function_calling': True, 'context_limit': 200000, 'output_limit': 8192}}, statuses=(200, 201))
        admin.request('POST', f'/api/admin/providers/{pid}/models', {'provider_model_name': model, 'global_model_id': global_model['id'], 'supports_function_calling': True, 'supports_streaming': True, 'is_active': True}, statuses=(200, 201))
        self.models[upstream] = model

    def set_cancel_policy(self, enabled):
        """Toggle `cancel_on_client_disconnect` on the system-default routing group and publish it."""
        groups = self.admin.request('GET', '/api/admin/routing/groups')['items']
        group = next(g for g in groups if g.get('is_system_default'))
        config = group['config_json']
        config.setdefault('default_policy', {})['cancel_on_client_disconnect'] = enabled
        self.admin.request('PATCH', '/api/admin/routing/groups/' + group['id'], {'config_json': config})
        self.admin.request('POST', f'/api/admin/routing/groups/{group["id"]}/publish', {})

    def spawn(self, name, argv, env):
        out = open(self.tmp / f'{name}.log', 'wb')
        proc = subprocess.Popen(argv, env={**os.environ, **env}, stdout=out, stderr=subprocess.STDOUT, start_new_session=True)
        self.procs.append(proc)

    def mock_events(self, since):
        if not self.mock_log.exists():
            return []
        lines = self.mock_log.read_text().splitlines()
        return [json.loads(line) for line in lines[since:]]

    def mock_mark(self):
        return len(self.mock_log.read_text().splitlines()) if self.mock_log.exists() else 0

    def stop(self):
        for proc in reversed(self.procs):
            try:
                os.killpg(proc.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        for proc in self.procs:
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGKILL)
        subprocess.run(['dropdb', '-h', 'localhost', '--force', '--if-exists', self.db], check=False)
        if self.keep:
            print(f'kept logs and mock traffic in {self.tmp}')
        else:
            shutil.rmtree(self.tmp, ignore_errors=True)


# ---- client drivers ------------------------------------------------------------------------
def client_env(home):
    """A minimal environment so the client cannot see the user's real config, credentials or hooks."""
    keep = {k: os.environ[k] for k in ('PATH', 'LANG', 'LC_ALL', 'TMPDIR', 'SHELL') if k in os.environ}
    return {**keep, 'HOME': str(home), 'NO_PROXY': '127.0.0.1,localhost', 'no_proxy': '127.0.0.1,localhost'}


def prompt_for(scenario, workdir):
    if scenario == 'text_only':
        return 'Reply with a short greeting. e2e-scenario:text_only dir=' + str(workdir)
    return f'e2e-scenario:{scenario} dir={workdir}'


def build_command(client, stack, upstream, home, workdir, prompt):
    model = stack.models[upstream]
    env = client_env(home)
    if client == 'codex':
        codex_home = home / '.codex'
        codex_home.mkdir(parents=True, exist_ok=True)
        (codex_home / 'config.toml').write_text(
            f'model = "{model}"\nmodel_provider = "arca"\n'
            '[model_providers.arca]\nname = "Arca"\n'
            f'base_url = "{stack.base}/v1"\nenv_key = "ARCA_E2E_KEY"\nwire_api = "responses"\n')
        env.update({'CODEX_HOME': str(codex_home), 'ARCA_E2E_KEY': stack.token})
        argv = ['codex', 'exec', '--json', '--skip-git-repo-check', '--dangerously-bypass-approvals-and-sandbox', '-C', str(workdir), prompt]
    else:
        config = home / '.claude'
        config.mkdir(parents=True, exist_ok=True)
        env.update({
            'CLAUDE_CONFIG_DIR': str(config), 'ANTHROPIC_BASE_URL': stack.base, 'ANTHROPIC_API_KEY': stack.token,
            'ANTHROPIC_MODEL': model, 'ANTHROPIC_SMALL_FAST_MODEL': model, 'ANTHROPIC_DEFAULT_HAIKU_MODEL': model,
            'ANTHROPIC_DEFAULT_SONNET_MODEL': model, 'ANTHROPIC_DEFAULT_OPUS_MODEL': model,
            'DISABLE_TELEMETRY': '1', 'DISABLE_AUTOUPDATER': '1', 'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC': '1',
        })
        argv = ['claude', '-p', prompt, '--output-format', 'json', '--model', model, '--permission-mode', 'bypassPermissions', '--no-session-persistence']
    return argv, env


class Result:
    def __init__(self, name):
        self.name, self.failures, self.notes = name, [], []

    def check(self, condition, label):
        if not condition:
            self.failures.append(label)
        return condition


def run_case(stack, client, upstream, scenario, timeout):
    result = Result(f'{client} -> {upstream} / {scenario}')
    workdir = pathlib.Path(tempfile.mkdtemp(prefix='work-', dir=stack.tmp))
    home = pathlib.Path(tempfile.mkdtemp(prefix='home-', dir=stack.tmp))
    argv, env = build_command(client, stack, upstream, home, workdir, prompt_for(scenario, workdir))
    if scenario.startswith('cancel'):
        stack.set_cancel_policy(scenario == 'cancel')
    if scenario.startswith('err_'):
        time.sleep(max(0.0, stack.record_level_ready_at - time.time()))
    case_started_at = time.time()
    mark = stack.mock_mark()
    usage_before = usage_totals(stack, f'e2e-{upstream}')
    started = time.time()
    proc = subprocess.Popen(argv, cwd=workdir, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True)
    cancelled = False
    if scenario.startswith('cancel'):
        deadline = time.time() + timeout
        while time.time() < deadline and not any(e['kind'] == 'stream_started' for e in stack.mock_events(mark)):
            time.sleep(0.2)
        time.sleep(1.0)  # let a few chunks through before the user "presses Ctrl-C"
        if proc.poll() is None:
            os.killpg(proc.pid, signal.SIGINT)
            cancelled = True
    # Clients retry failed requests with backoff; for error scenarios the gateway's record is the
    # subject, so give the client a short window and then stop it. (They retry for a long time.)
    client_timeout = min(timeout, 15) if scenario.startswith('err_') else timeout
    try:
        out, err = proc.communicate(timeout=client_timeout)
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        out, err = proc.communicate()
        if not scenario.startswith('err_'):
            result.check(False, f'client timed out after {timeout}s')
    if scenario.startswith('cancel'):
        # `cancel`: the gateway should notice the vanished client and close the upstream connection.
        # `cancel_drain` (default policy): it keeps draining the upstream so usage can still settle.
        want = 'aborted' if scenario == 'cancel' else 'stream_finished'
        deadline = time.time() + 12
        while time.time() < deadline and not any(e['kind'] == want for e in stack.mock_events(mark)):
            time.sleep(0.3)
    events = stack.mock_events(mark)
    requests = [e for e in events if e['kind'] == 'request' and e.get('scenario') == scenario]
    result.notes.append(f'{time.time() - started:.1f}s, {len(requests)} scripted upstream request(s)')
    (stack.tmp / f'{client}-{upstream}-{scenario}.out').write_text(out + '\n--- stderr ---\n' + err)
    assert_case(result, scenario, client, upstream, workdir, out, err, proc.returncode, requests, events, cancelled)
    if scenario.startswith('err_'):
        check_recorded_error(stack, result, upstream, scenario, case_started_at, requests)
    elif not scenario.startswith('cancel') and requests:
        check_forwarded(result, client, upstream, requests)
        check_usage(stack, result, client, upstream, requests, usage_before)
    return result


def check_usage(stack, result, client, upstream, requests, before):
    """The gateway must account every upstream call: request count and token totals."""
    want = (before[0] + len(requests), before[1] + 12 * len(requests), before[2] + 7 * len(requests))
    deadline, got = time.time() + 10, None
    while time.time() < deadline:
        got = usage_totals(stack, f'e2e-{upstream}')
        if got == want:
            break
        time.sleep(0.5)
    result.check(got == want, f'usage accounting is exact (requests, input, output tokens): wanted {want}, got {got}')


def check_recorded_error(stack, result, upstream, scenario, since, requests):
    """The gateway must keep the upstream's real error and the request it was a reply to."""
    want = ERROR_EXPECT[scenario]
    records = stack.admin.request('GET', '/api/admin/usage/records?limit=50')['records']
    failed = [r for r in records if r.get('model') == f'e2e-{upstream}' and r.get('status') == 'failed']
    if not result.check(bool(failed), 'a failed usage record exists for the request'):
        return
    record = failed[0]  # newest first
    detail = stack.admin.request('GET', f'/api/admin/usage/{record["id"]}?include_bodies=true')
    trace = stack.admin.request('GET', '/api/admin/monitoring/trace/' + detail['request_id'])
    # An in-band stream failure is recorded on a candidate that reached the upstream "successfully"
    # (HTTP 200), so evidence is: failed attempts, or attempts that carry an error message.
    attempts = [c for c in trace['candidates'] if c['status'] == 'failed' or (c.get('error_message') and c['status'] != 'skipped')]
    result.check(bool(attempts), 'the trace has attempts that carry the failure')
    if not attempts:
        return
    attempt = attempts[-1]
    extra = attempt.get('extra_data') or {}
    upstream_response = extra.get('upstream_response') or {}
    result.check(bool(upstream_response), 'the attempt keeps the upstream response')
    if scenario == 'err_stream':
        # Only the parsed message of an in-band error is guaranteed; the raw event is kept for some
        # upstream formats only (see docs/operations/request-diagnostics.md).
        result.check(want['marker'] in (attempt.get('error_message') or ''), 'the in-band upstream error message is kept on the attempt')
    else:
        result.check(upstream_response.get('status_code') == want['status'], f'upstream status {want["status"]} recorded (got {upstream_response.get("status_code")})')
        result.check(want['marker'] in json.dumps(upstream_response.get('body')), 'the upstream error body is kept verbatim')
        headers = {k.lower(): v for k, v in (upstream_response.get('headers') or {}).items()}
        for name, value in want['headers'].items():
            result.check(headers.get(name) == value, f'upstream header {name} kept')
        flow = extra.get('error_flow') or {}
        result.check(bool(flow), 'the gateway recorded how it classified the error')
        result.check(want['marker'] in (attempt.get('error_message') or '') + json.dumps(flow) + json.dumps(upstream_response.get('body')), 'the upstream message is recoverable from the attempt')
    result.check(bool(extra.get('upstream_url')), 'the upstream URL is recorded')
    # Failures-only capture: the request that was sent upstream is kept for failed requests.
    # Same-format passthrough stores one copy of the request: the client's, which is what went upstream.
    sent = detail.get('provider_request_body') or (detail.get('request_body') if detail.get('has_format_conversion') is False else None)
    result.check(isinstance(sent, dict) and sent.get('model') == f'e2e-{upstream}', 'the converted request sent upstream is kept for the failed request')
    if upstream == 'messages':
        result.check(isinstance(sent, dict) and 'messages' in sent, 'the kept upstream request is in the upstream (Messages) format')
    elif upstream == 'responses':
        result.check(isinstance(sent, dict) and 'input' in sent, 'the kept upstream request is in the upstream (Responses) format')
    else:
        result.check(isinstance(sent, dict) and 'messages' in sent, 'the kept upstream request is in the upstream (Chat) format')


def assert_case(result, scenario, client, upstream, workdir, out, err, code, requests, events, cancelled):
    result.check(bool(requests), 'the upstream received at least one scripted request')
    result.check(all(r['format'] == upstream for r in requests), f'every request reached the {upstream} upstream format')
    result.check(all(r['model'] == f'e2e-{upstream}' for r in requests), 'the upstream saw the mapped model name')
    result.check(not any(e['kind'] == 'unknown' for e in events), 'no request hit an unknown upstream path')
    if scenario.startswith('cancel'):
        result.check(cancelled, 'the client was interrupted mid-stream')
        aborted = any(e['kind'] == 'aborted' for e in events)
        if scenario == 'cancel':
            result.check(aborted, 'cancel_on_client_disconnect: the gateway dropped the upstream stream after the client left')
        else:
            result.check(not aborted and any(e['kind'] == 'stream_finished' for e in events), 'default policy: the gateway drained the upstream stream to completion')
        return
    if scenario.startswith('err_'):
        return  # the client's own error handling is not the subject; see check_recorded_error
    result.check(code == 0, f'client exited cleanly (exit code {code})')
    if scenario == 'text_only':
        result.check('E2E_DONE rounds=0' in out, 'the final answer reached the client')
        return
    expected = {'write_file': 1, 'multi_tool': 3, 'shell': 1}[scenario]
    result.check('E2E_DONE' in out, 'the client printed the final answer after the tool loop')
    result.check(f'rounds={expected}' in out, f'the final answer reports {expected} tool round(s)')
    rounds = sorted({r['tool_results'] for r in requests})
    result.check(rounds[:expected + 1] == list(range(expected + 1)), f'tool results were sent back round by round (saw {rounds})')
    if scenario == 'shell':
        result.check((workdir / 'e2e_0.txt').read_text() == 'E2E_SHELL' if (workdir / 'e2e_0.txt').exists() else False, 'the shell command ran and wrote its file')
    else:
        for n in range(expected):
            path = workdir / f'e2e_{n}.txt'
            result.check(path.exists() and path.read_text() == f'E2E_WRITE_{n}', f'tool call {n} wrote e2e_{n}.txt through the real client')


# ---- what the gateway forwarded upstream ---------------------------------------------------
def tool_schemas(fmt, body):
    tools = body.get('tools') or []
    if fmt == 'chat':
        return {t['function']['name']: t['function'].get('parameters') or {} for t in tools if t.get('type') == 'function'}
    if fmt == 'responses':
        return {t['name']: t.get('parameters') or {} for t in tools if t.get('type') == 'function'}
    return {t['name']: t.get('input_schema') or {} for t in tools}


def has_system_prompt(fmt, body):
    """The client's system prompt / instructions must reach the upstream in its native slot."""
    def text(value):
        if isinstance(value, str):
            return value.strip()
        return ' '.join(part.get('text', '') for part in value or [] if isinstance(part, dict)).strip()
    if fmt == 'chat':
        return any(m.get('role') in ('system', 'developer') and text(m.get('content')) for m in body.get('messages', []))
    if fmt == 'responses':
        return bool(text(body.get('instructions'))) or any(i.get('role') in ('system', 'developer') and text(i.get('content')) for i in body.get('input', []) if isinstance(i, dict))
    return bool(text(body.get('system')))


def call_result_ids(fmt, body):
    """(ids of tool calls in the history, ids the tool results refer to)."""
    calls, results = [], []
    if fmt == 'chat':
        for m in body.get('messages', []):
            calls += [c['id'] for c in m.get('tool_calls') or []]
            if m.get('role') == 'tool':
                results.append(m.get('tool_call_id'))
    elif fmt == 'responses':
        for item in body.get('input', []):
            if not isinstance(item, dict):
                continue
            if item.get('type') in ('function_call', 'custom_tool_call', 'local_shell_call'):
                calls.append(item.get('call_id'))
            if str(item.get('type', '')).endswith('_output'):
                results.append(item.get('call_id'))
    else:
        for m in body.get('messages', []):
            for block in m['content'] if isinstance(m.get('content'), list) else []:
                if block.get('type') == 'tool_use':
                    calls.append(block['id'])
                if block.get('type') == 'tool_result':
                    results.append(block.get('tool_use_id'))
    return calls, results


# Each client's signature tool and the parameters that must survive format conversion.
PRIMARY_TOOLS = {'codex': {'exec_command': ['cmd']}, 'claude': {'Write': ['file_path', 'content'], 'Bash': ['command'], 'Read': ['file_path']}}


def check_forwarded(result, client, upstream, requests):
    first, last = requests[0], requests[-1]
    result.check(has_system_prompt(upstream, first['body']), 'the client system prompt reached the upstream in its native slot')
    schemas = tool_schemas(upstream, first['body'])
    for tool, params in PRIMARY_TOOLS[client].items():
        present = tool in schemas
        result.check(present, f'tool {tool} survived conversion to {upstream}')
        if present:
            props = schemas[tool].get('properties', {})
            result.check(all(p in props for p in params), f'tool {tool} kept its parameters {params}')
    if not last['tool_results']:
        return
    calls, results = call_result_ids(upstream, last['body'])
    result.check(bool(results) and len(set(results)) == len(results), 'tool results carry distinct ids')
    result.check(set(results) <= set(calls), 'every tool result refers to a tool call present in the same request')
    result.check(len(calls) == len(results) == last['tool_results'], 'tool calls and results pair up one to one')


def usage_totals(stack, model):
    summary = stack.admin.request('GET', '/api/users/me/usage?limit=1').get('summary_by_model', [])
    row = next((r for r in summary if r['model'] == model), None)
    return (row['requests'], row['input_tokens'], row['output_tokens']) if row else (0, 0, 0)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--clients', default='codex,claude')
    parser.add_argument('--upstreams', default='chat,responses,messages')
    parser.add_argument('--scenarios', default=','.join(SCENARIOS))
    parser.add_argument('--gateway-bin', default=str(ROOT / 'target/debug/aether-gateway'))
    parser.add_argument('--timeout', type=int, default=180)
    parser.add_argument('--keep', action='store_true', help='keep logs and mock traffic in the temp directory')
    args = parser.parse_args()
    for tool in ('node', 'psql', 'createdb', 'dropdb'):
        if not shutil.which(tool):
            raise SystemExit(f'missing required tool: {tool}')
    clients = args.clients.split(',')
    for client in clients:
        if not shutil.which(client):
            raise SystemExit(f'client not installed: {client}')
    if not pathlib.Path(args.gateway_bin).exists():
        raise SystemExit(f'gateway binary not found: {args.gateway_bin} (cargo build -p aether-gateway)')

    stack = Stack(args.gateway_bin, args.keep)
    results = []
    try:
        stack.start()
        for client in clients:
            for upstream in args.upstreams.split(','):
                for scenario in args.scenarios.split(','):
                    result = run_case(stack, client, upstream, scenario, args.timeout)
                    results.append(result)
                    status = 'PASS' if not result.failures else 'FAIL'
                    print(f'{status}  {result.name}  ({"; ".join(result.notes)})', flush=True)
                    for failure in result.failures:
                        print(f'        - {failure}', flush=True)
    finally:
        stack.stop()
    failed = [r for r in results if r.failures]
    print(f'\n{len(results) - len(failed)}/{len(results)} cases passed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
