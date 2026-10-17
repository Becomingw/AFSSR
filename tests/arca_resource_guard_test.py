#!/usr/bin/env python3
"""Deterministic tests of acceptance-job resource refusal boundaries."""
import importlib.util
import contextlib
import io
import os
import pathlib
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location('arca_guard', pathlib.Path(__file__).resolve().parents[1] / 'deploy/arca/resource_guard.py')
GUARD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GUARD)


class ResourceGuardTests(unittest.TestCase):
    def check(self, *, available=1024, load=0.5, cpus=2, free=8 * 1024**3, memory=640):
        return GUARD.validate_headroom(f'MemTotal: 2000000 kB\nMemAvailable: {available * 1024} kB\n', load, cpus, free, memory)

    def test_healthy_snapshot(self):
        self.assertEqual(self.check()['reserve_mib'], 256)

    def test_memory_boundary(self):
        self.check(available=896)
        with self.assertRaisesRegex(RuntimeError, 'RAM'):
            self.check(available=895)

    def test_swap_does_not_substitute_for_physical_headroom(self):
        with self.assertRaisesRegex(RuntimeError, 'RAM'):
            GUARD.validate_headroom('MemAvailable: 100000 kB\nSwapFree: 9000000 kB', 0, 2, 8 * 1024**3, 640)

    def test_unreadable_memory_is_not_a_pass(self):
        for text in ['', 'MemAvailable: unknown kB', 'MemAvailable: 900000 MB']:
            with self.subTest(text=text), self.assertRaisesRegex(RuntimeError, 'verify'):
                GUARD.validate_headroom(text, 0, 2, 8 * 1024**3, 640)

    def test_disk_boundary(self):
        self.check(free=4 * 1024**3)
        with self.assertRaisesRegex(RuntimeError, 'disk'):
            self.check(free=4 * 1024**3 - 1)

    def test_load_boundary(self):
        self.check(load=3)
        for load in [3.01, float('inf'), float('nan')]:
            with self.subTest(load=load), self.assertRaisesRegex(RuntimeError, 'load'):
                self.check(load=load)

    def test_invalid_limits_are_refused(self):
        for memory in [0, -1, 4097, 640.5]:
            with self.subTest(memory=memory), self.assertRaisesRegex(RuntimeError, 'bounded'):
                self.check(memory=memory)

    def test_unknown_cpu_count_is_refused(self):
        with self.assertRaisesRegex(RuntimeError, 'load'):
            self.check(cpus=None)

    def test_process_lock_refuses_overlap_but_not_a_stale_filename(self):
        previous = pathlib.Path.cwd()
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary).resolve() / 'source'
            root.mkdir()
            try:
                os.chdir(root)
                with mock.patch.object(GUARD, 'ROOT', root), \
                     mock.patch.object(pathlib.Path, 'read_text', return_value='MemAvailable: 2097152 kB'), \
                     mock.patch.object(GUARD.os, 'getloadavg', return_value=(0, 0, 0)), \
                     mock.patch.object(GUARD.os, 'cpu_count', return_value=2), \
                     mock.patch.object(GUARD.shutil, 'disk_usage', return_value=SimpleNamespace(free=8 * 1024**3)), \
                     contextlib.redirect_stdout(io.StringIO()):
                    with GUARD.heavy_test_slot(640):
                        with self.assertRaisesRegex(RuntimeError, 'still active'):
                            with GUARD.heavy_test_slot(640):
                                self.fail('Overlapping work was admitted')
                    self.assertTrue((root.parent / '.heavy-test.lock').is_file())
                    with GUARD.heavy_test_slot(640):
                        pass
            finally:
                os.chdir(previous)

    def test_wrong_workspace_is_refused(self):
        with self.assertRaisesRegex(RuntimeError, 'restricted'):
            with GUARD.heavy_test_slot(640):
                self.fail('Non-acceptance workspace was admitted')


if __name__ == '__main__':
    unittest.main()
