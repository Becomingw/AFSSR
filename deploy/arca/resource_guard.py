#!/usr/bin/env python3
"""Serialize heavy Korean acceptance jobs and refuse unsafe host headroom."""
import argparse
from contextlib import contextmanager
import fcntl
import json
import math
import os
import pathlib
import shutil
import subprocess

ROOT = pathlib.Path('/opt/arca-test-20261016/source')


def validate_headroom(meminfo, load, cpus, free_bytes, memory_mib):
    if not isinstance(memory_mib, int) or not 128 <= memory_mib <= 4096:
        raise RuntimeError('Expected a bounded container memory limit in MiB')
    values = dict(line.split(':', 1) for line in meminfo.splitlines() if ':' in line)
    available = values.get('MemAvailable', '').split()
    if len(available) != 2 or available[1] != 'kB' or not available[0].isdigit():
        raise RuntimeError('Cannot verify available host memory')
    available_mib = int(available[0]) // 1024
    if available_mib < memory_mib + 256:
        raise RuntimeError('Insufficient physical RAM headroom; do not start another heavy test')
    if not math.isfinite(load) or not cpus or load > max(2, cpus * 1.5):
        raise RuntimeError('Host load is too high for another heavy test')
    if free_bytes < 4 * 1024**3:
        raise RuntimeError('Less than 4 GiB disk headroom; do not start another heavy test')
    return {'available_mib': available_mib, 'container_limit_mib': memory_mib,
            'reserve_mib': 256, 'free_bytes': free_bytes, 'load_one_minute': load}


@contextmanager
def heavy_test_slot(memory_mib):
    if pathlib.Path.cwd().resolve() != ROOT:
        raise RuntimeError('Heavy test guard is restricted to the Korean acceptance source')
    descriptor = os.open(ROOT.parent / '.heavy-test.lock', os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    try:
        try:
            fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError('Another guarded heavy test is still active; wait for its actual result') from None
        snapshot = validate_headroom(pathlib.Path('/proc/meminfo').read_text(),
                                     os.getloadavg()[0], os.cpu_count(),
                                     shutil.disk_usage(ROOT).free, memory_mib)
        print(json.dumps({'resource_guard': 'passed', **snapshot}), flush=True)
        yield
    finally:
        os.close(descriptor)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--memory-mib', required=True, type=int)
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    if not command:
        parser.error('An explicit command is required')
    try:
        with heavy_test_slot(args.memory_mib):
            result = subprocess.run(command)
        raise SystemExit(result.returncode)
    except RuntimeError as error:
        print(json.dumps({'resource_guard': 'refused', 'reason': str(error)}), flush=True)
        raise SystemExit(1) from None
