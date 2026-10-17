#!/usr/bin/env python3
"""Collect only Cargo-produced test executables for resource-limited remote execution."""
import hashlib
import json
import pathlib
import shutil
import subprocess

root = pathlib.Path('/build/arca-artifacts')
tests = root / 'tests'
tests.mkdir(exist_ok=True)
manifest = {}
for line in (root / 'test-artifacts.jsonl').read_text().splitlines():
    record = json.loads(line)
    executable = record.get('executable')
    if record.get('reason') != 'compiler-artifact' or not record.get('profile', {}).get('test') or not executable:
        continue
    source = pathlib.Path(executable)
    if not source.is_relative_to('/build/target'):
        raise RuntimeError('Unexpected test artifact path')
    destination = tests / source.name
    shutil.copy2(source, destination)
    subprocess.run(['strip', '--strip-debug', destination], check=True)
    manifest[destination.name] = {
        'package': record['package_id'], 'target': record['target']['name'],
        'kind': record['target']['kind'], 'sha256': hashlib.sha256(destination.read_bytes()).hexdigest(),
        'bytes': destination.stat().st_size,
    }
if not any(item['target'] == 'aether_gateway' for item in manifest.values()):
    raise RuntimeError('Gateway library tests missing')
(root / 'test-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps({'test_binaries': len(manifest), 'total_bytes': sum(item['bytes'] for item in manifest.values())}))
