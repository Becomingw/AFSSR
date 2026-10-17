#!/usr/bin/env python3
"""Create synthetic-only local acceptance credentials, never production values."""
import argparse
import json
import os
import pathlib
import secrets
import shutil
import socket

parser = argparse.ArgumentParser()
parser.add_argument('root', type=pathlib.Path)
args = parser.parse_args()
root = args.root.resolve()
if str(root) != '/opt/arca-test-20261016/source':
    raise SystemExit('This setup is restricted to the isolated Korean acceptance directory')
if shutil.disk_usage(root).free < 10 * 1024**3:
    raise SystemExit('Insufficient free disk for isolated acceptance')
for port in [15433, 16380, 18085]:
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', port))
environment = root / 'deploy/arca/.test.env'
values = {key: secrets.token_hex(32) for key in [
    'ARCA_TEST_DB_PASSWORD', 'ARCA_TEST_REDIS_PASSWORD', 'ARCA_TEST_ENCRYPTION_KEY',
    'ARCA_TEST_JWT_SECRET', 'ARCA_TEST_ADMIN_PASSWORD', 'ARCA_TEST_RELAY_SECRET',
]}
with os.fdopen(os.open(environment, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w') as output:
    output.write(''.join(f'{key}={value}\n' for key, value in values.items()))
(root / 'test-data').mkdir(mode=0o700, exist_ok=True)
print(json.dumps({'environment_created': True, 'ports_available': [15433, 16380, 18085], 'production_credentials_used': False}))
