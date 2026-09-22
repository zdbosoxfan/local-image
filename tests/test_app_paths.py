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
                self.assertEqual(app_paths.data_root(), self.root / 'Local AppData' / 'Local Remove')

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
