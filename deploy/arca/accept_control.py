#!/usr/bin/env python3
"""Real HTTP acceptance, restricted to the disposable Korean installation."""
import copy
import json
import pathlib
import secrets
import sys
import urllib.error
import urllib.request
import uuid

ROOT = pathlib.Path('/opt/arca-test-20261016/source')
BASE = 'http://127.0.0.1:18085'
PASSED = []
OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def check(condition, label):
    if not condition:
        raise AssertionError(label)
    PASSED.append(label)
    print(json.dumps({'passed': label}), flush=True)


class Client:
    def __init__(self, token=None):
        self.token = token
        self.device = str(uuid.uuid4())

    def request(self, method, path, body=None, statuses=(200,), headers=None):
        h = {'X-Client-Device-Id': self.device, 'Accept': 'application/json'}
        if self.token:
            h['Authorization'] = 'Bearer ' + self.token
        if headers:
            h.update(headers)
        payload = None if body is None else json.dumps(body).encode()
        if payload is not None:
            h['Content-Type'] = 'application/json'
        req = urllib.request.Request(BASE + path, payload, h, method=method)
        try:
            response = OPENER.open(req, timeout=45)
        except urllib.error.HTTPError as error:
            response = error
        raw = response.read(2 * 1024 * 1024)
        if response.status not in statuses:
            # Never include response bodies: successful key APIs contain secrets.
            raise AssertionError(f'{method} {path.split("?")[0]} status={response.status}, expected={statuses}')
        if not raw:
            return None
        try:
            return json.loads(raw)
        except json.JSONDecodeError:
            raise AssertionError(f'{method} {path.split("?")[0]} did not return JSON') from None

    def login(self, email, password):
        data = self.request('POST', '/api/auth/login', {'email': email, 'password': password, 'auth_type': 'local'})
        self.token = data['access_token']
        return self.request('GET', '/api/users/me')

    def rpc(self, method, params):
        response = self.request('POST', '/mcp', {'jsonrpc': '2.0', 'id': 1, 'method': method, 'params': params}, headers={'Accept': 'application/json, text/event-stream'})
        check('error' not in response, 'mcp RPC ' + method)
        return response['result']


def issue_create(client, title, visibility='public', admin=False):
    body = {
        'title': title, 'description': 'Synthetic acceptance, no production information.',
        'category': 'bug', 'client_request_id': str(uuid.uuid4()),
    }
    if visibility:
        body['visibility'] = visibility
    return client.request('POST', '/api/admin/feedback' if admin else '/api/feedback', body, statuses=(200, 201))['issue']


def command(client, issue, action, admin=False, statuses=(200,), **fields):
    return client.request('POST', ('/api/admin/feedback/' if admin else '/api/feedback/') + issue['id'] + '/commands', {
        'action': action, 'expected_version': issue['version'], 'client_request_id': str(uuid.uuid4()), **fields,
    }, statuses=statuses)


def main():
    if pathlib.Path.cwd().resolve() != ROOT:
        raise AssertionError('Restricted to isolated Korean test directory')
    env = dict(line.split('=', 1) for line in (ROOT / 'deploy/arca/.test.env').read_text().splitlines() if line)
    admin = Client()
    me = admin.login('arca-accept-admin@example.invalid', env['ARCA_TEST_ADMIN_PASSWORD'])
    check(me['role'] == 'admin', 'bootstrap administrator login with real database')
    suffix = uuid.uuid4().hex[:8]
    clients = {'admin': admin}
    users = {'admin': me}
    for role in ['user', 'audit_admin', 'user_admin', 'service_admin', 'other']:
        actual_role = 'user' if role == 'other' else role
        email = f'accept-{role}-{suffix}@example.invalid'
        password = secrets.token_urlsafe(24) + 'aA1!'
        user = admin.request('POST', '/api/admin/users', {'username': f'accept_{role}_{suffix}', 'email': email, 'password': password, 'role': actual_role}, statuses=(200, 201))
        client = Client()
        logged = client.login(email, password)
        check(logged['id'] == user['id'] and logged['role'] == actual_role, 'create and authenticate ' + role)
        clients[role], users[role] = client, user

    for role, client in clients.items():
        is_admin = role not in ['user', 'other']
        issue = issue_create(client, 'Role acceptance ' + role, admin=is_admin)
        check(issue['owner_id'] == users[role]['id'], 'feedback ownership ' + role)
    clients['user'].request('GET', '/api/admin/users', statuses=(401, 403))
    clients['audit_admin'].request('POST', '/api/admin/users', {}, statuses=(403,))
    clients['service_admin'].request('POST', '/api/admin/users', {}, statuses=(403,))
    clients['user_admin'].request('POST', '/api/admin/providers', {}, statuses=(403,))
    clients['user_admin'].request('PUT', '/api/admin/users/' + me['id'], {'role': 'user'}, statuses=(403,))
    check(True, 'server-side delegated role boundaries and administrator protection')

    owner, other = clients['user'], clients['other']
    owner.request('POST', '/api/feedback', {
        'title': 'Private visibility refusal acceptance',
        'description': 'Synthetic acceptance, no production information.',
        'category': 'bug', 'visibility': 'private', 'client_request_id': str(uuid.uuid4()),
    }, statuses=(400,))
    public = issue_create(owner, 'Public lifecycle acceptance')
    public_id = public['id']
    other.request('GET', '/api/feedback/' + public_id)
    Client().request('GET', '/api/feedback/' + public_id, statuses=(401, 403))
    payload = {'action': 'reply', 'expected_version': public['version'], 'client_request_id': str(uuid.uuid4()), 'content': 'Public synthetic reply', 'visibility': 'public'}
    path = '/api/feedback/' + public_id + '/commands'
    first = other.request('POST', path, payload)
    replay = other.request('POST', path, payload)
    check(replay['replayed'] and first['issue']['version'] == replay['issue']['version'], 'feedback command retries are idempotent')
    public = first['issue']
    other.request('POST', path, {**payload, 'client_request_id': str(uuid.uuid4())}, statuses=(409,))
    private_marker = 'private-' + uuid.uuid4().hex
    command(admin, public, 'reply', admin=True, content=private_marker, visibility='private', statuses=(400,))
    visible = other.request('GET', '/api/feedback/' + public_id)
    check(private_marker not in json.dumps(visible), 'a refused private reply is never published')
    command(owner, public, 'set_status', status='closed', statuses=(403,))
    command(other, public, 'set_status', status='resolved', comment='Cannot resolve another issue', statuses=(403,))
    command(owner, public, 'set_status', status='resolved', statuses=(400,))
    public = command(owner, public, 'set_status', status='resolved', comment='Synthetic final solution', visibility='public')['issue']
    check(public['status'] == 'resolved', 'owner resolves with required final solution')
    public = command(admin, public, 'set_status', admin=True, status='closed', comment='Verified closure')['issue']
    check(public['status'] == 'closed', 'administrator closes resolved issue')
    public = command(admin, public, 'set_status', admin=True, status='open', comment='Reopen for verification')['issue']
    check(public['status'] == 'open', 'administrator reopens issue')
    check(True, 'feedback privacy lifecycle CAS and unauthenticated protection')

    group = admin.request('POST', '/api/admin/user-groups', {'name': 'Acceptance ' + suffix, 'allowed_models_mode': 'specific', 'allowed_models': ['accept-a']}, statuses=(200, 201))
    uid = users['user']['id']
    admin.request('PUT', '/api/admin/users/' + uid, {'group_ids': [group['id']]})
    config_path = '/api/admin/system/configs/internal_access_policy'
    previous = admin.request('GET', config_path, statuses=(200, 404)).get('value')
    config = {'timezone': 'Asia/Shanghai', 'groups': {group['id']: {'unlimited': False, 'five_hour_budget_usd': 5, 'weekly_budget_usd': 30, 'monthly_budget_usd': 100, 'models': {'accept-a': {'daily_budget_usd': 2}}, 'families': {'astra': {'weekly_budget_usd': 10}}}}, 'users': {}, 'model_families': {'astra': {'name': 'Astra', 'models': ['accept-a', 'accept-b']}}}
    admin.request('PUT', config_path, {'value': config, 'expected_value': previous})
    ent = owner.request('GET', '/api/users/me/entitlements')
    check(ent['enabled'] and not ent['unlimited'], 'real account inherited budget read')
    windows = ent['windows']
    check(any(w['period'] == 'five_hour' and w['limit_usd'] == 5 and not w.get('model') and not w.get('family') for w in windows), 'rolling five-hour total pool')
    check(any(w['period'] == 'weekly' and w['limit_usd'] == 30 and not w.get('model') and not w.get('family') for w in windows), 'calendar weekly total pool')
    check(any(w.get('family') == 'astra' and w['limit_usd'] == 10 for w in windows), 'model family sub-pool is additional bound')
    proposed = copy.deepcopy(config)
    proposed['users'][uid] = {'unlimited': True, 'models': {}}
    preview = admin.request('POST', '/api/admin/access-preview', {'user_id': uid, 'config': proposed})
    check(preview['changed'] and preview['proposed']['source'] == 'personal' and preview['proposed']['allocation']['unlimited'], 'personal unlimited allocation overrides group in preview')
    check(not owner.request('GET', '/api/users/me/entitlements')['unlimited'], 'preview does not mutate effective budgets')
    admin.request('PUT', config_path, {'value': proposed, 'expected_value': config})
    admin.request('PUT', config_path, {'value': config, 'expected_value': config}, statuses=(409,))
    check(owner.request('GET', '/api/users/me/entitlements')['unlimited'], 'personal unlimited allocation effective and stale policy rejected')
    zero_defaults = copy.deepcopy(proposed)
    zero = {'unlimited': False, 'five_hour_budget_usd': 0, 'weekly_budget_usd': 0, 'monthly_budget_usd': 0, 'models': {}}
    zero_defaults['default'] = zero
    zero_defaults['groups'][group['id']] = zero
    admin.request('PUT', '/api/admin/users/' + users['other']['id'], {'group_ids': [group['id']]})
    admin.request('PUT', config_path, {'value': zero_defaults, 'expected_value': proposed})
    try:
        check(owner.request('GET', '/api/users/me/entitlements')['unlimited'], 'copied personal unlimited overrides zero default group')
        new_entitlements = other.request('GET', '/api/users/me/entitlements')
        check(not new_entitlements['unlimited'] and len(new_entitlements['windows']) == 3 and all(w['limit_usd'] == 0 for w in new_entitlements['windows']), 'new user inherits actual zero budgets even through default group')
    finally:
        admin.request('PUT', config_path, {'value': proposed, 'expected_value': zero_defaults})
    admin.request('DELETE', config_path, statuses=(400, 403, 405))
    check(True, 'policy cannot be removed through generic config deletion')

    for role in ['user', 'audit_admin', 'user_admin', 'service_admin', 'admin']:
        client = clients[role]
        client.request('GET', '/api/users/me')
        client.request('GET', '/api/users/me/usage/heatmap')
        client.request('GET', '/api/users/me/sessions')
        client.request('GET', '/api/users/me/client-config')
        report = client.request('GET', '/api/leaderboard?preset=this_week&tz_offset_minutes=480')
        check(isinstance(report.get('boards'), list), 'leaderboard self-service ' + role)
    report = owner.request('GET', '/api/leaderboard?preset=this_week&tz_offset_minutes=480')
    check(report['masked'], 'ordinary user leaderboard masked')
    report = admin.request('GET', '/api/admin/stats/leaderboard/overview?preset=this_week&tz_offset_minutes=480')
    check(not report['masked'], 'administrator leaderboard unmasked')
    check(True, 'profile heatmap sessions client config and leaderboard real APIs')

    announcement = clients['user_admin'].request('POST', '/api/announcements', {'title': 'Acceptance ' + suffix, 'content': 'Synthetic announcement', 'type': 'important', 'priority': 10, 'is_pinned': True, 'requires_ack': True}, statuses=(200, 201))
    required = owner.request('GET', '/api/announcements/users/me/required-unread')
    check(any(i['id'] == announcement['id'] for i in required['items']), 'pinned required announcement delivery')
    owner.request('PATCH', '/api/announcements/' + announcement['id'] + '/read-status')
    required = owner.request('GET', '/api/announcements/users/me/required-unread')
    check(not any(i['id'] == announcement['id'] for i in required['items']), 'announcement acknowledgement persists')

    compatibility = admin.request('GET', '/api/admin/compatibility')
    profile = {'id': str(uuid.uuid4()), 'config': {'client': 'codex', 'client_version': 'acceptance', 'client_protocol': 'openai:responses', 'model': 'accept-a', 'upstream_format': 'claude:messages', 'mapped_model': 'accept-a'}}
    compatibility = clients['service_admin'].request('POST', '/api/admin/compatibility/profiles', {'expected_revision': compatibility['revision'], 'profile': profile}, statuses=(200, 201))
    saved = next(p for p in compatibility['profiles'] if p['id'] == profile['id'])
    compatibility = clients['service_admin'].request('POST', '/api/admin/compatibility/runs', {'expected_revision': compatibility['revision'], 'profile_id': saved['id'], 'expected_fingerprint': saved['config_fingerprint']}, statuses=(200, 201))
    check(any(r['run']['profile_id'] == saved['id'] for r in compatibility['runs']), 'offline compatibility matrix persisted with exact fingerprint')

    owner.request('GET', '/api/mcp/keys', statuses=(401, 403))
    for role in ['admin', 'audit_admin', 'user_admin', 'service_admin']:
        client = clients[role]
        key = client.request('POST', '/api/mcp/keys', {'name': 'Acceptance ' + suffix, 'read_only': False, 'expires_in_days': 1, 'permissions': ['admin:feedback:read', 'admin:feedback:write']}, statuses=(200, 201), headers={'X-AFSSR-MCP-Key-Management': '1'})
        mcp = Client(key['token'])
        init = mcp.rpc('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name': 'arca-isolated-acceptance', 'version': '1'}})
        check('tools' in init['capabilities'], 'MCP initialize ' + role)
        tools = []
        cursor = None
        while True:
            listing = mcp.rpc('tools/list', {'cursor': cursor} if cursor else {})
            tools.extend(tool['name'] for tool in listing['tools'])
            cursor = listing.get('nextCursor')
            if not cursor:
                break
        check('arca_feedback_create' in tools and 'arca_providers_update' not in tools, 'MCP key scope filtering ' + role)
        args = {'body': {'title': 'MCP acceptance ' + role, 'description': 'Synthetic feedback only', 'category': 'suggestion', 'client_request_id': str(uuid.uuid4())}}
        preview = mcp.rpc('tools/call', {'name': 'arca_feedback_create', 'arguments': args})
        check(not preview.get('isError') and bool(preview.get('structuredContent', {}).get('confirmation')), 'MCP feedback dry-run ' + role)
        args.update({'dry_run': False, 'confirmation': preview['structuredContent']['confirmation']})
        result = mcp.rpc('tools/call', {'name': 'arca_feedback_create', 'arguments': args})
        check(not result.get('isError'), 'MCP confirmed feedback create ' + role)
        reused = mcp.rpc('tools/call', {'name': 'arca_feedback_create', 'arguments': args})
        check(reused.get('isError'), 'MCP confirmation single-use ' + role)
        client.request('DELETE', '/api/mcp/keys/' + key['data']['id'], statuses=(200, 204), headers={'X-AFSSR-MCP-Key-Management': '1'})
        mcp.request('POST', '/mcp', {'jsonrpc': '2.0', 'id': 1, 'method': 'tools/list', 'params': {}}, statuses=(401, 403), headers={'Accept': 'application/json, text/event-stream'})
        check(True, 'MCP immediate key revocation ' + role)

    for path in ['/api/billing/plans', '/api/users/me/referral', '/api/recharge']:
        owner.request('GET', path, statuses=(404, 410))
    check(True, 'retired commercial endpoints unavailable')
    print(json.dumps({'suite': 'control-real-http', 'passed': len(PASSED), 'status': 'passed', 'production_credentials_used': False}), flush=True)


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        print(json.dumps({'suite': 'control-real-http', 'status': 'failed', 'passed': len(PASSED), 'error_type': type(error).__name__, 'reason': str(error) if isinstance(error, AssertionError) else 'See the synthetic-only server diagnostics'}), flush=True)
        sys.exit(1)
