"""Portable installation/configuration: temporary files, no live units or workers."""
import importlib.machinery
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'bin'))
import li_config


def load(name, path):
    spec = importlib.util.spec_from_loader(name, importlib.machinery.SourceFileLoader(name, str(path)))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


installer = load('queue_installer', ROOT / 'install.py')
dashboard = load('portable_dashboard', ROOT / 'dashboard/dashboard.py')


class PortableTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.dev = self.root / 'state with spaces'
        self.lanes = self.root / 'lanes'
        self.repo = self.root / 'repo with spaces'
        self.repo.mkdir()
        (self.repo / 'Cargo.toml').write_text('[workspace]\n')
        (self.repo / '.git').mkdir()
        (self.repo / 'crates').mkdir()
        env = patch.dict(os.environ, {'LI_DEV_DIR': str(self.dev), 'LI_REPO': str(self.repo), 'LI_BUILD_STATE_DIR': str(self.lanes)})
        env.start(); self.addCleanup(env.stop)
        self.idle = patch.object(installer, 'assert_idle')
        self.idle.start(); self.addCleanup(self.idle.stop)
        # A regression must fail, never invoke the real service manager/process tool.
        self.commands = patch.object(installer.subprocess, 'run', side_effect=AssertionError('unexpected process/service action'))
        self.commands.start(); self.addCleanup(self.commands.stop)

    def test_preflight_is_read_only(self):
        with patch('sys.stdout', new=io.StringIO()):
            self.assertEqual(installer.main(['--dev-dir', str(self.dev)]), 0)
        self.assertFalse(self.dev.exists())
        self.assertFalse(self.lanes.exists())

    def test_fresh_install_and_upgrade_keep_state_and_only_owned_files(self):
        first = installer.apply(self.dev)
        jobs = json.loads((self.dev / 'jobs.json').read_text())
        self.assertEqual(jobs['dispatch_mode'], 'coordinator')
        self.assertTrue(jobs['paused'])
        self.assertEqual(jobs['jobs'], [])
        (self.dev / 'unrelated').write_text('keep me')
        jobs['jobs'] = [{'name': 'manual', 'status': 'running', 'dispatch_mode': 'coordinator'}]
        (self.dev / 'jobs.json').write_text(json.dumps(jobs))
        before = (self.dev / 'jobs.json').read_bytes()
        hold = (self.dev / 'queue-hold').read_bytes()
        second = installer.apply(self.dev)
        self.assertNotEqual(first, second)
        self.assertEqual((self.dev / 'jobs.json').read_bytes(), before)
        self.assertEqual((self.dev / 'queue-hold').read_bytes(), hold)
        self.assertEqual((self.dev / 'unrelated').read_text(), 'keep me')
        expected = json.loads((self.dev / installer.MANIFEST).read_text())
        for name in installer.FILES:
            self.assertEqual(installer.digest(self.dev / name), expected[name])
            self.assertEqual((self.dev / name).read_bytes(), (ROOT / name).read_bytes())
        self.assertEqual((self.dev / 'bin/li-land').stat().st_mode & 0o777, 0o755)
        self.assertEqual((self.dev / 'bin/li_state.py').stat().st_mode & 0o777, 0o644)

    def test_unknown_existing_bytes_refused_without_change(self):
        target = self.dev / 'bin/li-test-runner'
        target.parent.mkdir(parents=True)
        target.write_bytes(b'unknown edited tool')
        before = sorted(p.relative_to(self.dev) for p in self.dev.rglob('*'))
        with self.assertRaisesRegex(RuntimeError, 'Unreviewed'):
            installer.apply(self.dev)
        self.assertEqual(target.read_bytes(), b'unknown edited tool')
        self.assertEqual(sorted(p.relative_to(self.dev) for p in self.dev.rglob('*')), before)

    def test_reviewed_existing_bytes_backed_up_not_silently_adopted(self):
        installer.apply(self.dev)
        target = self.dev / 'bin/li-test-runner'
        target.write_bytes(b'reviewed local change')
        reviewed = self.root / 'reviewed.json'
        reviewed.write_text(json.dumps({'bin/li-test-runner': installer.digest(target)}))
        backup = installer.apply(self.dev, reviewed=reviewed)
        self.assertEqual((backup / 'bin/li-test-runner').read_bytes(), b'reviewed local change')
        self.assertEqual(target.read_bytes(), (ROOT / 'bin/li-test-runner').read_bytes())

    def test_missing_hold_and_invalid_jobs_refuse(self):
        installer.apply(self.dev)
        (self.dev / 'queue-hold').unlink()
        with self.assertRaisesRegex(RuntimeError, 'held'):
            installer.apply(self.dev)
        (self.dev / 'queue-hold').touch()
        (self.dev / 'jobs.json').write_text('{"jobs":false}')
        with self.assertRaisesRegex(RuntimeError, 'invalid'):
            installer.apply(self.dev)

    def test_symlinked_owned_file_and_state_root_refused(self):
        outside = self.root / 'outside'
        outside.mkdir()
        self.dev.symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'symlink'):
            installer.apply(self.dev)
        self.assertEqual(list(outside.iterdir()), [])
        self.dev.unlink()
        (self.dev / 'bin').mkdir(parents=True)
        target = outside / 'original'
        target.write_text('keep')
        (self.dev / 'bin/li-test-runner').symlink_to(target)
        with self.assertRaisesRegex(RuntimeError, 'symlink'):
            installer.verify(self.dev)
        self.assertEqual(target.read_text(), 'keep')

    def test_runtime_inside_repo_and_relative_or_invalid_repo_refused(self):
        with self.assertRaisesRegex(ValueError, 'outside'):
            installer.apply(self.repo / 'runtime')
        with patch.dict(os.environ, {'LI_REPO': ''}):
            with self.assertRaises(ValueError): li_config.repository()
        with patch.dict(os.environ, {'LI_REPO': str(self.root / 'missing')}):
            with self.assertRaises(ValueError): li_config.repository()
        with self.assertRaises(ValueError): li_config.absolute('relative', 'path')

    def test_build_state_inside_checkout_refuses_before_any_runtime_write(self):
        lanes = self.repo / 'lane-state'
        before = sorted(p.relative_to(self.repo) for p in self.repo.rglob('*'))
        with patch.dict(os.environ, {'LI_BUILD_STATE_DIR': str(lanes)}):
            with self.assertRaisesRegex(ValueError, 'outside'):
                installer.apply(self.dev)
        self.assertFalse(self.dev.exists(), 'no installation lock directory may be created')
        self.assertFalse(lanes.exists(), 'no scheduler directory/lock may be created')
        self.assertEqual(sorted(p.relative_to(self.repo) for p in self.repo.rglob('*')), before)
        alias = self.root / 'repo-alias'
        alias.symlink_to(self.repo, target_is_directory=True)
        with patch.dict(os.environ, {'LI_BUILD_STATE_DIR': str(alias / 'lane-state')}):
            with self.assertRaisesRegex(ValueError, 'symlink'):
                installer.apply(self.dev)
        self.assertFalse(self.dev.exists())
        self.assertFalse(lanes.exists())
        self.assertEqual(sorted(p.relative_to(self.repo) for p in self.repo.rglob('*')), before)

    def test_installed_config_has_no_author_repo_fallback(self):
        with patch.object(li_config, 'source_repository', return_value=None), patch.dict(os.environ, {}, clear=True):
            self.assertIsNone(li_config.repository(required=False))
            with self.assertRaisesRegex(ValueError, 'require LI_REPO'): li_config.repository()

    def test_optional_unit_is_generated_but_never_activated(self):
        unit = self.root / 'units' / 'li-dashboard.service'
        installer.apply(self.dev, unit_output=unit, repo=self.repo)
        text = unit.read_text()
        self.assertIn('Environment="LI_REPO=' + str(self.repo) + '"', text)
        self.assertIn('"' + str(self.dev / 'dashboard/dashboard.py') + '"', text)
        self.assertNotIn('@SCRIPT@', text)
        self.assertEqual(installer.quote_unit('a%/b$c', executable=True), '"a%%/b$$c"')
        unit.write_text('another owner')
        with self.assertRaisesRegex(RuntimeError, 'different unit'):
            installer.apply(self.dev, unit_output=unit, repo=self.repo)
        self.assertEqual(unit.read_text(), 'another owner')

    def test_busy_scheduler_lease_refuses_replacement(self):
        import fcntl
        installer.apply(self.dev)
        marker = self.lanes / 'pending.test'
        with marker.open('w') as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            before = (self.dev / installer.MANIFEST).read_bytes()
            with self.assertRaisesRegex(RuntimeError, 'busy'):
                installer.apply(self.dev)
            self.assertEqual((self.dev / installer.MANIFEST).read_bytes(), before)

    def test_partial_atomic_write_leaves_original_and_no_temporary(self):
        target = self.root / 'sentinel'
        target.write_bytes(b'old')
        with patch.object(installer.os, 'fsync', side_effect=OSError('injected sync failure')):
            with self.assertRaisesRegex(OSError, 'injected'):
                installer.atomic_bytes(target, b'new')
        self.assertEqual(target.read_bytes(), b'old')
        self.assertFalse(list(self.root.glob('.sentinel-*')))

    def test_dashboard_records_refuse_symlink_redirection_into_checkout(self):
        runtime = self.dev / 'dashboard'
        runtime.mkdir(parents=True)
        target = self.repo / 'sentinel'
        target.write_text('original')
        record = runtime / 'requests.jsonl'
        record.symlink_to(target)
        with self.assertRaisesRegex(ValueError, 'symlink'):
            dashboard.append_record(record, {'text': 'do not write'})
        self.assertEqual(target.read_text(), 'original')
        record.unlink()
        dashboard.append_record(record, {'text': 'request only'})
        self.assertEqual(json.loads(record.read_text()), {'text': 'request only'})

    def test_dashboard_runtime_paths_and_no_phantom_worktree(self):
        with patch.object(dashboard, 'DEV', str(self.dev)), patch.object(dashboard, 'RECOVERY', ''):
            self.dev.mkdir()
            (self.dev / 'jobs.json').write_text('{"jobs":[{"name":"job","status":"queued"}]}')
            rows, _ = dashboard.tasks({1: ('cargo test', '')})
            self.assertFalse(rows[0]['building'])
            self.assertIsNone(dashboard.coordinator({}))
        loaded = load('configured_dashboard', ROOT / 'dashboard/dashboard.py')
        self.assertEqual(Path(loaded.REQ), self.dev / 'dashboard/requests.jsonl')
        self.assertEqual(Path(loaded.LANES), self.lanes)
        self.assertFalse(self.repo in Path(loaded.REQ).parents)
        with patch.object(loaded, 'procs', return_value={}), patch.object(loaded, 'lanes', return_value=([], [])):
            self.assertTrue(loaded.collect_status()['scheduler']['paused'])


class DisabledEntrypointTests(unittest.TestCase):
    def test_all_legacy_entrypoints_refuse_before_any_side_effect(self):
        # Only fixed shell stubs run; no model, service manager, Git, or Cargo.
        for name in ['codex-launch.sh', 'bin/li-land', 'bin/li-rework', 'bin/li-sweep-targets', 'bin/update-gate.sh']:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                sentinel = root / 'sentinel'
                sentinel.write_text('unchanged')
                env = {'HOME': directory, 'LI_DEV_DIR': str(root / 'state'), 'PATH': '/nonexistent'}
                result = subprocess.run(['/bin/sh', str(ROOT / name), str(sentinel)], cwd=root, env=env, capture_output=True, text=True, timeout=2)
                self.assertEqual(result.returncode, 2)
                self.assertIn('held', result.stderr)
                self.assertEqual(list(root.iterdir()), [sentinel])
                self.assertEqual(sentinel.read_text(), 'unchanged')


if __name__ == '__main__':
    unittest.main()
