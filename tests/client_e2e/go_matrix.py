#!/usr/bin/env python3
"""Real Codex / Claude Code against every OpenCode Go model, through an `opencode_go` provider.

The upstream is `mock_upstream.mjs go`: it serves each model on its ONE documented native protocol,
refuses everything else exactly like the real service and validates requests strictly (see
go_mode.mjs). So a request the gateway shapes wrongly is rejected here the way it would be in
production, and a request sent to the wrong protocol shows up as a rejection too.

A case passes only when the real client finished its tool loop AND the upstream refused nothing.

    tests/client_e2e/go_matrix.py [--clients codex,claude] [--models glm-5.3,qwen3.8-max]
                                  [--scenarios text_only,write_file,multi_tool] [--keep]
"""
import argparse
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time
import collections

import run

HERE = pathlib.Path(__file__).resolve().parent
MODELS = json.loads(subprocess.check_output(
    ['node', '--input-type=module', '-e',
     "import { GO_MODELS, UNDOCUMENTED } from './go_mode.mjs'; console.log(JSON.stringify({ models: GO_MODELS, undocumented: [...UNDOCUMENTED] }))"],
    cwd=HERE, text=True))
PROTOCOL = {model: protocol for protocol, models in MODELS['models'].items() for model in models}
GO_SCENARIOS = ['text_only', 'write_file', 'multi_tool']


class GoStack(run.Stack):
    """The standard stack, but with one `opencode_go` provider instead of three custom ones."""

    def __init__(self, gateway_bin, keep, allow_training=True, cost_limit=None, directives=False):
        super().__init__(gateway_bin, keep)
        self.cost_limit = cost_limit
        self.directives = directives
        self.mock_extra = ['go']
        self.mock_env = {'GO_ALLOW_TRAINING': '1' if allow_training else '0'}

    def apply_cost_limit(self):
        """A finite personal budget makes every request go through the hard cost-limit estimate,
        which refuses (HTTP 500) any request whose cost it cannot bound."""
        admin = self.admin
        owner = admin.request('GET', '/api/users/me')['id']
        current = admin.request('GET', '/api/admin/system/configs/internal_access_policy', statuses=(200, 404)).get('value')
        value = json.loads(json.dumps(current)) if current else {'users': {}}
        value.setdefault('users', {})[owner] = {'unlimited': False, 'five_hour_budget_usd': self.cost_limit,
                                                'weekly_budget_usd': self.cost_limit, 'monthly_budget_usd': self.cost_limit,
                                                'models': {}, 'families': {}}
        admin.request('PUT', '/api/admin/system/configs/internal_access_policy', {'expected_value': current, 'value': value}, statuses=(200, 201))

    def provision_all(self):
        admin = self.admin
        if self.cost_limit:
            self.apply_cost_limit()
        if self.directives:
            # Production runs with this on: a model name ending like a directive (qwen3.8-max) must still be that model.
            admin.request('PUT', '/api/admin/system/configs/enable_model_directives', {'value': True}, statuses=(200, 201))
        provider = admin.request('POST', '/api/admin/providers/', {'name': 'OpenCode Go (mock)', 'provider_type': 'opencode_go', 'billing_type': 'pay_as_you_go', 'is_active': True, 'max_retries': 0}, statuses=(200, 201))
        pid = provider['id']
        self.provider_id = pid
        endpoints = admin.request('GET', f'/api/admin/endpoints/providers/{pid}/endpoints')
        endpoints = endpoints if isinstance(endpoints, list) else endpoints.get('endpoints', endpoints.get('items', []))
        self.endpoint_formats = sorted(e['api_format'] for e in endpoints)
        base = f'http://127.0.0.1:{self.mock_port}/inference/go/v1'
        for endpoint in endpoints:
            admin.request('PUT', f"/api/admin/endpoints/{endpoint['id']}", {'base_url': base}, statuses=(200, 201))
        admin.request('POST', f'/api/admin/endpoints/providers/{pid}/keys', {
            'api_formats': self.endpoint_formats, 'api_key': 'synthetic-not-a-real-key', 'name': 'Go key',
            'allowed_models': list(PROTOCOL), 'auto_fetch_models': False}, statuses=(200, 201))
        for model in PROTOCOL:
            global_model = admin.request('POST', '/api/admin/models/global', {
                'name': model, 'display_name': model, 'is_active': True, 'supported_capabilities': ['function_calling'],
                'default_tiered_pricing': {'tiers': [{'up_to': None, 'input_price_per_1m': 1, 'output_price_per_1m': 2}]},
                'config': {'function_calling': True, 'context_limit': 200000, 'output_limit': 8192}}, statuses=(200, 201))
            admin.request('POST', f'/api/admin/providers/{pid}/models', {'provider_model_name': model, 'global_model_id': global_model['id'], 'supports_function_calling': True, 'supports_streaming': True, 'is_active': True}, statuses=(200, 201))
            self.models[model] = model


def run_case(stack, client, model, scenario, timeout):
    workdir = pathlib.Path(tempfile.mkdtemp(prefix='work-', dir=stack.tmp))
    home = pathlib.Path(tempfile.mkdtemp(prefix='home-', dir=stack.tmp))
    argv, env = run.build_command(client, stack, model, home, workdir, run.prompt_for(scenario, workdir))
    mark = stack.mock_mark()
    started = time.time()
    proc = subprocess.Popen(argv, cwd=workdir, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True)
    timed_out = False
    try:
        out, err = proc.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        run.os.killpg(proc.pid, run.signal.SIGKILL)
        out, err = proc.communicate()
        timed_out = True
    events = stack.mock_events(mark)
    accepted = [e for e in events if e['kind'] == 'request' and e.get('scenario') == scenario]
    rejected = [e for e in events if e['kind'] == 'rejected']
    problems = []
    if timed_out:
        problems.append(f'client timed out after {timeout}s')
    elif proc.returncode != 0:
        problems.append(f'client exit code {proc.returncode}')
    expected = {'text_only': 0, 'write_file': 1, 'multi_tool': 3}[scenario]
    if scenario == 'text_only':
        if 'E2E_DONE rounds=0' not in out:
            problems.append('final answer missing')
    else:
        if f'rounds={expected}' not in out:
            problems.append(f'tool loop did not finish ({expected} round(s) expected)')
        for n in range(expected):
            path = workdir / f'e2e_{n}.txt'
            if not (path.exists() and path.read_text() == f'E2E_WRITE_{n}'):
                problems.append(f'tool call {n} did not write its file')
    if not accepted:
        problems.append('upstream accepted no request')
    wrong_protocol = [e for e in accepted if e['format'] != PROTOCOL[model]]
    if wrong_protocol:
        problems.append(f"sent to {sorted({e['format'] for e in wrong_protocol})} instead of the native {PROTOCOL[model]}")
    (stack.tmp / f'go-{client}-{model}-{scenario}.out').write_text(out + '\n--- stderr ---\n' + err)
    return {'client': client, 'model': model, 'scenario': scenario, 'native': PROTOCOL[model], 'seconds': round(time.time() - started, 1),
            'problems': problems, 'rejected': [{'format': e['format'], 'status': e['status'], 'message': e['message']} for e in rejected],
            'accepted': len(accepted)}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--clients', default='codex,claude')
    parser.add_argument('--models', default=','.join(PROTOCOL))
    parser.add_argument('--scenarios', default=','.join(GO_SCENARIOS))
    parser.add_argument('--gateway-bin', default=str(run.ROOT / 'target/debug/aether-gateway'))
    parser.add_argument('--timeout', type=int, default=120)
    parser.add_argument('--directives', action='store_true', help='enable model directives like production does')
    parser.add_argument('--cost-limit', type=float, help='give the test user a finite budget (hard cost limit active), USD')
    parser.add_argument('--keep', action='store_true')
    parser.add_argument('--json', type=pathlib.Path, help='write all results here')
    args = parser.parse_args()
    for tool in ('node', 'psql', 'createdb', 'dropdb'):
        if not shutil.which(tool):
            raise SystemExit(f'missing required tool: {tool}')
    models = args.models.split(',')
    unknown = [m for m in models if m not in PROTOCOL]
    if unknown:
        raise SystemExit(f'unknown Go model(s): {unknown}')

    stack = GoStack(args.gateway_bin, args.keep, cost_limit=args.cost_limit, directives=args.directives)
    results = []
    try:
        stack.start()
        print(f'provider endpoints: {stack.endpoint_formats}', flush=True)
        for client in args.clients.split(','):
            for model in models:
                for scenario in args.scenarios.split(','):
                    result = run_case(stack, client, model, scenario, args.timeout)
                    results.append(result)
                    clean = not result['problems'] and not result['rejected']
                    print(f"{'PASS' if clean else 'FAIL'}  {client:6} {model:28} {scenario:10} native={result['native']:9} {result['seconds']}s", flush=True)
                    for problem in result['problems']:
                        print(f'        - {problem}', flush=True)
                    for rejection in result['rejected']:
                        print(f"        ! upstream refused ({rejection['format']} {rejection['status']}): {rejection['message'][:140]}", flush=True)
    finally:
        stack.stop()
    failed = [r for r in results if r['problems'] or r['rejected']]
    reasons = collections.Counter(r['message'][:90] for result in results for r in result['rejected'])
    print(f'\n{len(results) - len(failed)}/{len(results)} cases clean')
    if reasons:
        print('upstream refusals by reason:')
        for message, count in reasons.most_common():
            print(f'  {count:4}  {message}')
    if args.json:
        args.json.write_text(json.dumps(results, indent=2))
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
