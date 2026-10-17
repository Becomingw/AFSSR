#!/usr/bin/env python3
"""Export a read-only consistent logical backup with same-snapshot identity hashes."""
import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys

CRITICAL = ['users', 'api_keys', 'providers', 'provider_api_keys', 'provider_endpoints',
            'global_models', 'models', 'user_groups', 'user_group_members', 'user_group_memberships',
            'system_configs', 'management_tokens', 'proxy_nodes', 'announcements']


def digest(path):
    result = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(4 * 1024 * 1024), b''):
            result.update(block)
    return result.hexdigest()


def write_private(path, value):
    with os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w') as output:
        json.dump(value, output, indent=2)
        output.write('\n')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--container', required=True, choices=['aether-postgres', 'arca-acceptance-20261016-postgres-1', 'afssr-go-staging-postgres-1'])
    parser.add_argument('--destination', required=True, type=pathlib.Path)
    parser.add_argument('--test-database', action='store_true', help='Use only the isolated arca_backup_source fixture')
    args = parser.parse_args()
    destination = args.destination.resolve()
    production = args.container == 'aether-postgres'
    staging = args.container == 'afssr-go-staging-postgres-1'
    if (production or staging) and args.test_database:
        raise RuntimeError('Source backup cannot override its database')
    allowed_root = pathlib.Path('/mnt/cch-archiver-data/arca-rollout-20261016/backups' if production else '/opt/arca-test-20261016/backups')
    if destination.parent != allowed_root or destination.exists():
        raise RuntimeError('Destination must be a new protected backup directory under the permitted root')
    if production:
        source = subprocess.check_output(['findmnt', '-n', '-o', 'SOURCE', '--target', '/mnt/cch-archiver-data'], text=True).strip()
        if source != '/dev/sdb':
            raise RuntimeError('Required separate data disk is not mounted')
    if staging:
        info = json.loads(subprocess.check_output(['docker', 'inspect', args.container], text=True))[0]
        if info['Config'].get('Labels', {}).get('com.docker.compose.project') != 'afssr-go-staging':
            raise RuntimeError('Legacy rehearsal source is not the isolated staging project')
    destination.parent.mkdir(parents=True, mode=0o700, exist_ok=True)
    destination.mkdir(mode=0o700)
    docker_exec = ['docker', 'exec']
    if args.test_database:
        docker_exec.extend(['-e', 'POSTGRES_DB=arca_backup_source'])
    sql_command = [*docker_exec, '-i', args.container, 'sh', '-c',
                   'exec psql -X -qAt -w -v ON_ERROR_STOP=1 -U "${POSTGRES_USER:-postgres}" -d "${POSTGRES_DB:-aether}"']
    process = subprocess.Popen(sql_command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)

    def sql(statement):
        process.stdin.write(statement + '\n')
        process.stdin.flush()
        line = process.stdout.readline()
        if not line:
            raise RuntimeError('Read-only backup transaction ended unexpectedly')
        return json.loads(line)

    try:
        process.stdin.write("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY; SET LOCAL statement_timeout='60s'; SET LOCAL idle_in_transaction_session_timeout='30min'; SET LOCAL TIME ZONE 'UTC'; SET LOCAL DateStyle='ISO,YMD'; SET LOCAL extra_float_digits=3;\n")
        process.stdin.flush()
        header = sql("SELECT json_build_object('snapshot',pg_export_snapshot(),'database',current_database(),'database_bytes',pg_database_size(current_database()),'captured_at',clock_timestamp(),'read_only',current_setting('transaction_read_only'),'server_version_num',current_setting('server_version_num'));")
        if header['read_only'] != 'on':
            raise RuntimeError('Snapshot transaction is not read-only')
        if staging and header['database'] != 'afssr_staging':
            raise RuntimeError('Legacy rehearsal source database mismatch')
        minimum = header['database_bytes'] * (1.5 if production else 1) + 1024**3
        if shutil.disk_usage(destination).free < minimum:
            raise RuntimeError('Insufficient backup and independent restore headroom')
        tables = sql("SELECT COALESCE(json_agg(tablename ORDER BY tablename),'[]'::json) FROM pg_tables WHERE schemaname='public';")
        row_counts = {}
        for table in tables:
            identifier = '"' + table.replace('"', '""') + '"'
            row_counts[table] = sql('SELECT to_json(count(*)) FROM public.' + identifier + ';')
        hashes = {}
        for table in CRITICAL:
            if table in tables:
                hashes[table] = sql('SELECT json_build_object(\'rows\',count(*),\'rows_md5\',md5(COALESCE(string_agg(row_hash,\'\' ORDER BY row_hash),\'\'))) FROM (SELECT md5(row_to_json(t)::text) row_hash FROM public."' + table + '" t) x;')
        required = ['users', 'api_keys', 'providers', 'provider_api_keys', 'provider_endpoints'] if production or staging else []
        if not all(table in hashes for table in required):
            raise RuntimeError('Mandatory source tables are missing')
        partial = destination / 'database.dump.partial'
        with os.fdopen(os.open(partial, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'wb') as output:
            command = [*docker_exec, args.container, 'sh', '-c',
                       'exec pg_dump -w -U "${POSTGRES_USER:-postgres}" -d "${POSTGRES_DB:-aether}" -Fc --no-owner --no-privileges --compress=3 --lock-wait-timeout=5s --snapshot="$1"',
                       'arca-readonly-backup', header['snapshot']]
            result = subprocess.run(command, stdout=output, stderr=subprocess.PIPE)
            output.flush()
            os.fsync(output.fileno())
        if result.returncode:
            raise RuntimeError('Logical dump failed; partial is not a valid accepted backup')
        # Prove the exporting transaction is still alive before accepting the archive.
        check = sql("SELECT json_build_object('snapshot_alive',true,'read_only',current_setting('transaction_read_only'));")
        if not check['snapshot_alive'] or check['read_only'] != 'on':
            raise RuntimeError('Snapshot transaction did not survive dump')
        process.stdin.write('ROLLBACK;\n\\q\n')
        process.stdin.flush()
        process.communicate(timeout=15)
        if process.returncode:
            raise RuntimeError('Backup transaction did not close cleanly')
        archive = destination / 'database.dump'
        partial.rename(archive)
        write_private(destination / 'manifest.json', {**header, 'source_container': args.container, 'critical_tables': hashes,
                                                    'all_public_tables': tables, 'all_table_row_counts': row_counts,
                                                    'hash_session_timezone': 'UTC', 'dump_sha256': digest(archive),
                                                    'dump_bytes': archive.stat().st_size, 'full_restore_verified': False})
        print(json.dumps({'backup_created': True, 'destination': str(destination), 'dump_bytes': archive.stat().st_size,
                          'dump_sha256': digest(archive), 'critical_table_counts': {k: v['rows'] for k, v in hashes.items()},
                          'source_writes': False, 'full_restore_verified': False}), flush=True)
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        print(json.dumps({'backup_created': False, 'error_type': type(error).__name__, 'reason': str(error) if isinstance(error, RuntimeError) else 'Inspect protected backup diagnostics'}), flush=True)
        sys.exit(1)
