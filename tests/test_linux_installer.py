"""Exercise Linux install/upgrade/uninstall against disposable user directories."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


SCRIPTS = Path(__file__).resolve().parents[1] / 'packaging' / 'linux'


@unittest.skipUnless(sys.platform.startswith('linux'), 'Linux installer needs Linux shell utilities.')
class LinuxInstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='local-image-installer-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.home = self.root / 'home with spaces – café'
        self.home.mkdir()
        self.data = self.home / 'custom data'
        self.package = self.root / 'extracted download'
        self.package.mkdir()
        for name in ('install.sh', 'uninstall.sh', 'local-image.desktop'):
            shutil.copy2(SCRIPTS / name, self.package / name)
        self.version = self.package / 'VERSION'
        self.version.write_text('0.7.0-linux-test\n', encoding='utf-8')
        (self.package / 'icon.png').write_bytes(b'fixture-icon')
        self.write_program('local-image', 'original')
        self.write_program('backend/LocalRemoveBackend', 'backend')
        self.env = dict(os.environ, HOME=str(self.home), XDG_DATA_HOME=str(self.data),
                        XDG_CONFIG_HOME=str(self.home / 'config'),
                        XDG_CACHE_HOME=str(self.home / 'cache'),
                        XDG_STATE_HOME=str(self.home / 'state'))

    def write_program(self, name, identity):
        path = self.package / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('#!/bin/sh\nprintf "' + identity + ': %s\\n" "$@"\n', encoding='utf-8')
        path.chmod(0o755)

    @property
    def application(self):
        return self.data / 'local-image-app'

    @property
    def launcher(self):
        return self.home / '.local' / 'bin' / 'local-image'

    @property
    def desktop(self):
        return self.data / 'applications' / 'local-image.desktop'

    def run_script(self, name='install.sh', *, path=None, env=None, success=True):
        result = subprocess.run([str(path or self.package / name)],
                                env=env or self.env, text=True, capture_output=True, timeout=20)
        if success:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def create_data(self):
        files = ['state/settings.json', 'state/generation-library/result.png',
                 'models/retained-model.bin', 'webview/preferences.json']
        for name in files:
            path = self.data / 'local-image' / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(('keep ' + name).encode())
        return {name: (self.data / 'local-image' / name).read_bytes() for name in files}

    def assert_data_kept(self, expected):
        self.assertEqual({name: (self.data / 'local-image' / name).read_bytes()
                          for name in expected}, expected)

    def test_install_creates_working_launcher_and_desktop_entry(self):
        self.run_script()
        result = subprocess.run([str(self.launcher), 'a photo with spaces.png'],
                                env=self.env, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, 'original: a photo with spaces.png\n')
        self.assertTrue((self.application / 'current').is_symlink())
        self.assertTrue((self.application / 'current' / 'backend' / 'LocalRemoveBackend').is_file())
        entry = self.desktop.read_text(encoding='utf-8')
        self.assertIn('Name=Local Image\n', entry)
        self.assertIn('Terminal=false\n', entry)
        self.assertIn(f'Exec="{self.launcher}" %F\n', entry)
        if shutil.which('desktop-file-validate'):
            subprocess.run(['desktop-file-validate', str(self.desktop)], check=True, timeout=10)

    def test_upgrade_is_atomic_and_keeps_prior_release_and_user_data(self):
        expected = self.create_data()
        self.run_script()
        previous = (self.application / 'current').resolve()
        self.version.write_text('0.7.0-linux-next\n', encoding='utf-8')
        self.write_program('local-image', 'updated')
        self.run_script()
        self.assertNotEqual((self.application / 'current').resolve(), previous)
        self.assertTrue((previous / 'local-image').is_file())
        result = subprocess.run([str(self.launcher), 'hello'], env=self.env,
                                capture_output=True, text=True, check=True, timeout=10)
        self.assertEqual(result.stdout, 'updated: hello\n')
        self.assert_data_kept(expected)
        self.assertFalse((self.application / '.install-lock').exists())

    def test_failed_copy_keeps_current_release_and_cleans_stage(self):
        expected = self.create_data()
        self.run_script()
        previous = (self.application / 'current').resolve()
        fake_bin = self.root / 'failure bin'
        fake_bin.mkdir()
        (fake_bin / 'cp').write_text('#!/bin/sh\nexit 27\n', encoding='utf-8')
        (fake_bin / 'cp').chmod(0o755)
        env = dict(self.env, PATH=str(fake_bin) + os.pathsep + self.env['PATH'])
        self.run_script(env=env, success=False)
        self.assertEqual((self.application / 'current').resolve(), previous)
        self.assertEqual(list((self.application / 'releases').glob('.install-*')), [])
        self.assertFalse((self.application / '.install-lock').exists())
        self.assert_data_kept(expected)

    def test_first_install_copy_failure_allows_a_clean_retry(self):
        fake_bin = self.root / 'failure bin'
        fake_bin.mkdir()
        (fake_bin / 'cp').write_text('#!/bin/sh\nexit 27\n', encoding='utf-8')
        (fake_bin / 'cp').chmod(0o755)
        env = dict(self.env, PATH=str(fake_bin) + os.pathsep + self.env['PATH'])
        self.run_script(env=env, success=False)
        self.assertFalse((self.application / 'current').exists())
        self.assertFalse(self.launcher.exists())
        self.assertFalse(self.desktop.exists())
        self.assertFalse((self.application / '.install-lock').exists())
        self.run_script()
        self.assertTrue((self.application / 'current' / 'local-image').is_file())

    def test_uninstall_keeps_images_models_and_settings(self):
        expected = self.create_data()
        self.run_script()
        self.run_script(path=self.application / 'current' / 'uninstall.sh')
        self.assertFalse(self.application.exists())
        self.assertFalse(self.launcher.exists())
        self.assertFalse(self.desktop.exists())
        self.assert_data_kept(expected)

    def test_installed_uninstaller_uses_installed_location_after_xdg_change(self):
        expected = self.create_data()
        self.run_script()
        env = dict(self.env, XDG_DATA_HOME=str(self.root / 'different data'))
        self.run_script(path=self.application / 'current' / 'uninstall.sh', env=env)
        self.assertFalse(self.application.exists())
        self.assert_data_kept(expected)

    def test_uninstall_keeps_replacement_launchers_and_desktop_entries(self):
        self.run_script()
        self.launcher.write_text('another launcher', encoding='utf-8')
        self.desktop.write_text('[Desktop Entry]\nName=Other app\n', encoding='utf-8')
        self.run_script('uninstall.sh')
        self.assertEqual(self.launcher.read_text(), 'another launcher')
        self.assertIn('Name=Other app', self.desktop.read_text())
        self.assertFalse(self.application.exists())

    def test_relative_xdg_uses_the_standard_default(self):
        env = dict(self.env, XDG_DATA_HOME='relative data')
        self.run_script(env=env)
        self.assertTrue((self.home / '.local/share/local-image-app/current/local-image').is_file())
        self.assertFalse(self.data.exists())

    def test_quotes_dollars_backslashes_and_unicode_are_literal(self):
        self.home = self.root / "quoted 'home' $dollar `tick` \\slash %percent – café"
        self.home.mkdir()
        self.data = self.home / 'data spaces'
        self.env.update(HOME=str(self.home), XDG_DATA_HOME=str(self.data))
        self.run_script()
        result = subprocess.run([str(self.launcher), 'literal $argument'], env=self.env,
                                text=True, capture_output=True, check=True, timeout=10)
        self.assertEqual(result.stdout, 'original: literal $argument\n')
        if shutil.which('desktop-file-validate'):
            subprocess.run(['desktop-file-validate', str(self.desktop)], check=True, timeout=10)
        self.run_script('uninstall.sh')
        self.assertFalse(self.application.exists())

    def test_unmanaged_application_is_never_removed_or_overwritten(self):
        self.application.mkdir(parents=True)
        sentinel = self.application / 'other-user-file'
        sentinel.write_text('leave me', encoding='utf-8')
        self.run_script(success=False)
        self.run_script('uninstall.sh', success=False)
        self.assertEqual(sentinel.read_text(), 'leave me')
        self.assertFalse(self.launcher.exists())

    def test_application_symlink_is_never_followed(self):
        other = self.root / 'other application'
        other.mkdir()
        sentinel = other / 'keep'
        sentinel.write_text('leave me', encoding='utf-8')
        self.data.mkdir(parents=True)
        self.application.symlink_to(other, target_is_directory=True)
        self.run_script(success=False)
        self.run_script('uninstall.sh', success=False)
        self.assertEqual(sentinel.read_text(), 'leave me')
        self.assertTrue(self.application.is_symlink())

    def test_releases_symlink_is_never_followed(self):
        self.application.mkdir(parents=True)
        (self.application / 'INSTALLATION').write_text('local-image-linux-bundle-v1\n')
        other = self.root / 'other releases'
        other.mkdir()
        (self.application / 'releases').symlink_to(other, target_is_directory=True)
        self.run_script(success=False)
        self.assertEqual(list(other.iterdir()), [])

    def test_unmanaged_or_symlink_launcher_is_preserved(self):
        self.launcher.parent.mkdir(parents=True)
        other = self.root / 'another launcher'
        other.write_text('leave me', encoding='utf-8')
        self.launcher.symlink_to(other)
        self.run_script(success=False)
        self.assertTrue(self.launcher.is_symlink())
        self.assertEqual(other.read_text(), 'leave me')
        self.assertFalse(self.application.exists())

    def test_desktop_symlink_is_preserved(self):
        self.desktop.parent.mkdir(parents=True)
        other = self.root / 'another desktop'
        other.write_text('leave me', encoding='utf-8')
        self.desktop.symlink_to(other)
        self.run_script(success=False)
        self.assertTrue(self.desktop.is_symlink())
        self.assertEqual(other.read_text(), 'leave me')
        self.assertFalse(self.application.exists())

    def test_invalid_version_and_incomplete_download_are_rejected(self):
        self.version.write_text('../../outside\n', encoding='utf-8')
        self.run_script(success=False)
        self.assertFalse(self.application.exists())
        self.version.write_text('0.7.0\n', encoding='utf-8')
        (self.package / 'backend/LocalRemoveBackend').unlink()
        self.run_script(success=False)
        self.assertFalse(self.application.exists())

    def test_concurrent_install_lock_leaves_current_release_untouched(self):
        self.run_script()
        previous = (self.application / 'current').resolve()
        (self.application / '.install-lock').mkdir()
        self.run_script(success=False)
        self.run_script('uninstall.sh', success=False)
        self.assertEqual((self.application / 'current').resolve(), previous)


if __name__ == '__main__':
    unittest.main()
