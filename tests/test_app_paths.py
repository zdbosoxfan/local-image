import json
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
import app_paths


class UserStorageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='local-remove-layout-')
        self.root = Path(self.temporary.name)
        self.environment = patch.dict(os.environ, {'LOCAL_REMOVE_DATA_DIR': str(self.root / 'User Data')})
        self.environment.start()

    def tearDown(self):
        self.environment.stop()
        self.temporary.cleanup()

    def test_windows_profile_default_does_not_use_working_directory(self):
        with patch.dict(os.environ, {'LOCALAPPDATA': str(self.root / 'Local AppData')}):
            with patch.dict(os.environ):
                os.environ.pop('LOCAL_REMOVE_DATA_DIR')
                self.assertEqual(app_paths.data_root(), self.root / 'Local AppData' / 'Local Image')

    @unittest.skipIf(os.name == 'nt', 'POSIX platform layout')
    def test_linux_profile_uses_absolute_xdg_or_home_without_temporary_browser_storage(self):
        with patch.dict(os.environ), patch.object(app_paths.sys, 'platform', 'linux'), patch.object(Path, 'home', return_value=self.root):
            for key in ('LOCAL_REMOVE_DATA_DIR', 'LOCAL_IMAGE_DATA_DIR', 'LOCALAPPDATA'):
                os.environ.pop(key, None)
            os.environ['XDG_DATA_HOME'] = str(self.root / 'Native Data')
            self.assertEqual(app_paths.data_root(), self.root / 'Native Data' / 'local-image')
            os.environ['XDG_DATA_HOME'] = 'relative-folder'
            self.assertEqual(app_paths.data_root(), self.root / '.local' / 'share' / 'local-image')

    @unittest.skipIf(os.name == 'nt', 'POSIX platform layout')
    def test_macos_profile_uses_application_support(self):
        with patch.dict(os.environ), patch.object(app_paths.sys, 'platform', 'darwin'), patch.object(Path, 'home', return_value=self.root):
            for key in ('LOCAL_REMOVE_DATA_DIR', 'LOCAL_IMAGE_DATA_DIR', 'LOCALAPPDATA'):
                os.environ.pop(key, None)
            self.assertEqual(app_paths.data_root(), self.root / 'Library' / 'Application Support' / 'Local Image')

    def test_profile_identity_preserves_native_logical_path(self):
        # A Store/MSIX parent can redirect a physical handle under Packages.
        # Native GetFullPath and runtime identity must use the same logical path.
        expected = self.root / 'User Data'
        with patch.object(Path, 'resolve', side_effect=AssertionError('Physical path lookup')):
            self.assertEqual(app_paths.data_root(), expected)
            with patch.dict(os.environ, {'LOCALAPPDATA': str(self.root / 'Local AppData')}):
                with patch.dict(os.environ):
                    os.environ.pop('LOCAL_REMOVE_DATA_DIR')
                    self.assertEqual(app_paths.data_root(), self.root / 'Local AppData' / 'Local Image')

    def test_legacy_profile_is_adopted_without_copying_or_overriding_current_profile(self):
        base = self.root / 'Other User AppData'
        legacy, current = base / 'Local Remove', base / 'Local Image'
        legacy.mkdir(parents=True)
        (legacy / 'config.json').write_text('{"comfy_port": 8189}')
        with patch.dict(os.environ, {'LOCALAPPDATA': str(base)}):
            with patch.dict(os.environ):
                os.environ.pop('LOCAL_REMOVE_DATA_DIR')
                os.environ.pop('LOCAL_IMAGE_DATA_DIR', None)
                self.assertEqual(app_paths.data_root(), legacy)
                self.assertEqual(app_paths.read_config()['comfy_port'], 8189)
                (current / 'state').mkdir(parents=True)
                self.assertEqual(app_paths.data_root(), current)
        self.assertEqual((legacy / 'config.json').read_text(), '{"comfy_port": 8189}')

    def test_selected_model_and_portable_runtime_folders_are_independent_of_installation(self):
        app_paths.write_config({'model_directory': str(self.root / 'Models on another drive'),
                               'managed_ai_directory': str(self.root / 'Portable AI'), 'setup_mode': 'portable'})
        with patch.dict(os.environ):
            os.environ.pop('LOCAL_IMAGE_MODELS_DIR', None)
            os.environ.pop('LOCAL_REMOVE_MODELS_DIR', None)
            os.environ.pop('LOCAL_IMAGE_AI_DIR', None)
            self.assertEqual(app_paths.model_directory(), self.root / 'Models on another drive')
            self.assertEqual(app_paths.managed_ai_dir(), self.root / 'Portable AI')
        self.assertEqual(app_paths.read_config()['setup_mode'], 'portable')

    def test_gallery_audience_preference_requires_real_boolean_and_survives_other_updates(self):
        app_paths.write_config({'lora_show_adult_content':True})
        self.assertTrue(app_paths.read_config()['lora_show_adult_content'])
        app_paths.write_config({'comfy_port':8189})
        self.assertTrue(app_paths.read_config()['lora_show_adult_content'])
        app_paths.write_config({'lora_show_adult_content':'true'})
        self.assertNotIn('lora_show_adult_content',app_paths.read_config())

    def test_workflow_is_copied_once_and_user_changes_survive(self):
        resources = self.root / 'Read Only Install'
        resources.mkdir()
        (resources / 'workflow.json').write_text('{"initial": true}')
        with patch.object(app_paths, 'RESOURCE_DIR', resources):
            configured = app_paths.workflow_file()
            self.assertTrue(configured.is_relative_to(app_paths.data_root()))
            configured.write_text('{"customized": true}')
            self.assertEqual(json.loads(app_paths.workflow_file().read_text()), {'customized': True})
            self.assertEqual(json.loads((resources / 'workflow.json').read_text()), {'initial': True})

    def test_cache_cleanup_never_touches_recovery_data_or_projects(self):
        app_paths.prepare_user_folders()
        recovery = app_paths.state_dir() / 'unsaved.lremove'
        recovery.write_bytes(b'recoverable layers')
        folder = app_paths.cache_dir() / 'thumbnails'
        old = folder / 'old.jpg'
        old.write_bytes(b'x' * 32)
        os.utime(old, (time.time() - 9 * 86400,) * 2)
        recent = folder / 'recent.jpg'
        recent.write_bytes(b'x' * 32)
        other = folder / 'keep.lremove'
        other.write_bytes(b'not a thumbnail')
        app_paths.prune_thumbnails(max_bytes=40)
        self.assertFalse(old.exists())
        self.assertTrue(recent.exists())
        self.assertEqual(recovery.read_bytes(), b'recoverable layers')
        self.assertTrue(other.exists())

    def test_invalid_config_falls_back_and_valid_connection_reloads(self):
        app_paths.prepare_user_folders()
        config = app_paths.data_root() / 'config.json'
        config.write_text('{bad json')
        self.assertEqual(app_paths.read_config()['comfy_port'], 8188)
        config.write_text(json.dumps({'comfy_port': True, 'model_directory': 123}))
        self.assertEqual(app_paths.read_config(), {'comfy_port': 8188, 'model_directory': ''})
        model_root = self.root / 'Models elsewhere'
        config.write_text(json.dumps({'comfy_port': 8189, 'model_directory': str(model_root)}))
        self.assertEqual(app_paths.read_config()['comfy_port'], 8189)
        with patch.dict(os.environ):
            os.environ.pop('LOCAL_REMOVE_MODELS_DIR', None)
            self.assertEqual(app_paths.model_directory(), model_root)


if __name__ == '__main__':
    unittest.main()
