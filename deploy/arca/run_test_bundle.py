#!/usr/bin/env python3
"""Execute verified Linux tests on KR, with real isolated PostgreSQL fixtures."""
import hashlib
import argparse
import json
import os
import pathlib
import re
import subprocess
import sys
import time
from resource_guard import heavy_test_slot

ROOT = pathlib.Path('/opt/arca-test-20261016/source')
IMAGE = 'sha256:5f72c7b5bd616308ccfd2e74d6be16fb06364e5eecbb815fe9dc6ab9761d2111'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--partial', action='store_true')
    parser.add_argument('--run-id', default='')
    args = parser.parse_args()
    if args.run_id and not re.fullmatch(r'[a-zA-Z0-9_-]{1,40}', args.run_id):
        raise RuntimeError('Invalid test evidence run identifier')
    if pathlib.Path.cwd().resolve() != ROOT:
        raise RuntimeError('Restricted to disposable Korean acceptance')
    env = dict(line.split('=', 1) for line in (ROOT / 'deploy/arca/.test.env').read_text().splitlines() if line)
    url = 'postgresql://postgres:' + env['ARCA_TEST_DB_PASSWORD'] + '@127.0.0.1:15433/afssr_feedback_test'
    # Docker reads these values from this process; the command line never contains them.
    child_env = {**os.environ, **{key: url for key in ['AFSSR_FEEDBACK_TEST_DATABASE_URL', 'AFSSR_ACCOUNT_USAGE_TEST_DATABASE_URL', 'AFSSR_ACTIVITY_TEST_DATABASE_URL']}}
    child_env['AFSSR_COMPAT_TEST_DATABASE_URL'] = url.rsplit('/', 1)[0] + '/afssr_compatibility_test'
    partial = args.partial
    artifact_root = ROOT / 'arca-artifacts' / 'partial' if partial else ROOT / 'arca-artifacts'
    manifest = json.loads((artifact_root / 'test-manifest.json').read_text())
    required = {'aether_gateway', 'aether_data', 'aether_data_postgres', 'internal_gateway_protocols'}
    if partial:
        required.remove('aether_gateway')
    if not required.issubset({record['target'] for record in manifest.values()}):
        raise RuntimeError('Missing required test targets')
    results = []
    log_name = 'rust-partial' if partial else 'rust'
    if args.run_id:
        log_name += '-' + args.run_id
    log_root = ROOT.parent / 'logs' / log_name
    log_root.mkdir(parents=True, exist_ok=True)

    def execute(name, record, extra=()):
        artifact = artifact_root / 'tests' / name
        if artifact.parent != artifact_root / 'tests' or not artifact.is_file():
            raise RuntimeError('Invalid test artifact')
        if hashlib.sha256(artifact.read_bytes()).hexdigest() != record['sha256']:
            raise RuntimeError('Test artifact digest mismatch')
        log = log_root / (name + ('-integration' if extra else '') + '.log')
        if log.exists():
            raise RuntimeError('Refusing to overwrite prior test evidence')
        command = ['docker', 'run', '--rm', '--init', '--name', 'arca-rust-acceptance-20261016',
                   '--network', 'host', '--cpus', '1', '--memory', '640m', '--memory-swap', '1024m',
                   '--pids-limit', '256', '--ulimit', 'core=0', '--security-opt', 'no-new-privileges',
                   '--user', 'postgres', '-v', str(ROOT) + ':/build:ro',
                   '--tmpfs', '/run/arca-tests:rw,nosuid,nodev,size=32m,mode=0700,uid=999,gid=999',
                   '-w', '/run/arca-tests',
                   '-e', 'RUST_MIN_STACK=16777216', '-e', 'RUST_BACKTRACE=0',
                   '-e', 'AETHER_REQUIRE_LOCAL_POSTGRES_TESTS=1',
                   '-e', 'AETHER_INITDB_BIN=/usr/lib/postgresql/15/bin/initdb',
                   '-e', 'AETHER_POSTGRES_BIN=/usr/lib/postgresql/15/bin/postgres',
                   '-e', 'AETHER_PG_CTL_BIN=/usr/lib/postgresql/15/bin/pg_ctl']
        for key in ['AFSSR_FEEDBACK_TEST_DATABASE_URL', 'AFSSR_ACCOUNT_USAGE_TEST_DATABASE_URL', 'AFSSR_ACTIVITY_TEST_DATABASE_URL', 'AFSSR_COMPAT_TEST_DATABASE_URL']:
            command.extend(['-e', key])
        if record['target'] == 'aether_data_schema':
            # Existing schema tests write fixtures under their manifest's target dir.
            (ROOT / 'crates/aether-data/schema/target').mkdir(exist_ok=True)
            command.extend(['--tmpfs', '/build/crates/aether-data/schema/target:rw,nosuid,nodev,size=16m,mode=1777'])
        command.extend(['--entrypoint', str(pathlib.PurePosixPath('/build') / artifact.relative_to(ROOT)), IMAGE, '--test-threads=1', *extra])
        started = time.monotonic()
        with log.open('xb') as output:
            result = subprocess.run(command, env=child_env, stdout=output, stderr=subprocess.STDOUT)
        text = log.read_text(errors='replace')
        summary = [line for line in text.splitlines() if line.startswith('test result:')]
        results.append({'target': record['target'], 'binary': name, 'integration': bool(extra),
                        'exit_code': result.returncode, 'seconds': round(time.monotonic() - started, 2),
                        'summary': summary, 'log_sha256': hashlib.sha256(log.read_bytes()).hexdigest()})
        print(json.dumps(results[-1]), flush=True)
        if result.returncode or not summary or not all('test result: ok.' in line for line in summary):
            raise RuntimeError('Test target failed; inspect synthetic-only log ' + str(log))
        if extra and not any('1 passed; 0 failed;' in line for line in summary):
            raise RuntimeError('Required integration case did not actually execute')

    for name, record in sorted(manifest.items(), key=lambda item: (item[1]['target'] == 'aether_gateway', item[1]['target'], item[0])):
        if 'example' in record['kind']:
            continue
        execute(name, record)
    name, record = next((name, record) for name, record in manifest.items() if record['target'] == 'aether_data_postgres' and record['kind'] == ['lib'])
    # All three new PG cases share a dedicated guarded DB and independent schemas.
    for test in ['feedback_postgres_migration_persistence_cas_and_feed', 'personal_usage_ledger_isolation_expiry_models_and_boundaries', 'leaderboard_activity_postgres_buckets_timezone_and_boundaries']:
        # Each invocation needs its own retained log.
        alias = name + '-' + test
        destination = artifact_root / 'tests' / alias
        if not destination.exists():
            os.link(artifact_root / 'tests' / name, destination)
        execute(alias, record, [test, '--ignored'])
    if not partial:
        name, record = next((name, record) for name, record in manifest.items() if record['target'] == 'aether_gateway' and record['kind'] == ['lib'])
        test = 'postgres_compatibility_roundtrip_restart_stale_and_atomic_cas'
        alias = name + '-compatibility-postgres'
        destination = artifact_root / 'tests' / alias
        if not destination.exists():
            os.link(artifact_root / 'tests' / name, destination)
        execute(alias, record, [test, '--ignored'])
    report = {'suite': 'linux-rust-korea', 'status': 'partial_passed' if partial else 'passed', 'results': results}
    (log_root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'suite': report['suite'], 'status': report['status'], 'test_executions': len(results)}), flush=True)


if __name__ == '__main__':
    try:
        with heavy_test_slot(640):
            main()
    except Exception as error:
        print(json.dumps({'suite': 'linux-rust-korea', 'status': 'failed', 'error_type': type(error).__name__, 'reason': str(error) if isinstance(error, RuntimeError) else 'Inspect the isolated test diagnostics'}), flush=True)
        sys.exit(1)
