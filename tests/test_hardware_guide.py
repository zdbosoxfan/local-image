"""Hardware detection is informational and tolerates missing GPU utilities."""
from pathlib import Path
import asyncio
import json
import subprocess
import sys
import unittest
from unittest.mock import patch
from fastapi import HTTPException
from pydantic import ValidationError

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
sys.path.insert(0, str(Path(__file__).resolve().parent / 'helpers'))
import hardware_guide as hardware
import backend_settings_test as setting_fixtures


class HardwareGuideTests(unittest.TestCase):
    def test_driver_capacities_do_not_confuse_mib_with_bytes(self):
        devices = hardware.parse_nvidia_devices('NVIDIA GeForce RTX 5090, 32607\n"GPU, second", 8192\n')
        self.assertEqual(devices[0]['vram_gb'], 31.8)
        self.assertEqual(devices[1]['name'], 'GPU, second')
        self.assertEqual(devices[1]['vram_gb'], 8)

    def test_unknown_and_invalid_capacities_remain_unknown(self):
        self.assertEqual(hardware.parse_nvidia_devices('GPU, [N/A]\nGPU, nan\nGPU, -1\nGPU, inf\n'), [])

    def test_missing_or_hung_driver_is_nonfatal(self):
        for error in (FileNotFoundError(), subprocess.TimeoutExpired('nvidia-smi', 4)):
            with patch.object(hardware.subprocess, 'run', side_effect=error):
                self.assertEqual(hardware.nvidia_devices(), [])

    def test_hardware_api_omits_retired_hidream_even_when_detection_is_offline(self):
        with patch.object(hardware, 'read_config', return_value={'comfy_port': 8188}), \
             patch.object(hardware.aiohttp, 'ClientSession', side_effect=hardware.aiohttp.ClientError()), \
             patch.object(hardware, 'nvidia_devices', return_value=[]):
            status = asyncio.run(hardware.hardware_status())
        ids = [profile['id'] for profile in status['profiles']]
        self.assertNotIn('hidream-o1', ids)
        self.assertNotIn('flux2-dev', ids)
        self.assertIn('qwen-int8', ids)
        self.assertIn('seedvr2', ids)


class HardwarePreferenceTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.fixture = setting_fixtures.BackendSettingsTests(methodName='runTest')
        self.fixture.setUp()
        self.modules = patch.dict(sys.modules, {'local_remove': self.fixture.app})
        self.modules.start()

    def tearDown(self):
        self.modules.stop()
        self.fixture.tearDown()

    async def test_preference_survives_disk_reload_and_preserves_other_settings(self):
        path = self.fixture.directory / 'config.json'
        path.write_text(json.dumps({'comfy_port': 8189, 'model_directory': '/models', 'unrelated': 'keep'}))
        self.assertEqual(await hardware.get_hardware_preference(self.fixture.request()), {'dont_show_again': None})
        for value in (True, False):
            result = await hardware.set_hardware_preference(self.fixture.request(), hardware.HardwarePreference(dont_show_again=value))
            self.assertEqual(result, {'dont_show_again': value})
            self.assertEqual(await hardware.get_hardware_preference(self.fixture.request()), result)
            saved = json.loads(path.read_text())
            self.assertEqual(saved['hardware_guide_dismissed'], value)
            self.assertEqual(saved['comfy_port'], 8189)
            self.assertEqual(saved['model_directory'], '/models')
            self.assertEqual(saved['unrelated'], 'keep')

    async def test_preference_routes_require_local_origin_and_page_token(self):
        payload = hardware.HardwarePreference(dont_show_again=True)
        for request in (self.fixture.request(token=False), self.fixture.request(origin='https://remote.example')):
            handlers = [(hardware.set_hardware_preference, (request, payload))]
            if request.headers['origin'] == 'https://remote.example':
                handlers.append((hardware.get_hardware_preference, (request,)))
            for handler, arguments in handlers:
                with self.assertRaises(HTTPException) as rejected:
                    await handler(*arguments)
                self.assertEqual(rejected.exception.status_code, 403)
        self.assertFalse((self.fixture.directory / 'config.json').exists())

    def test_preference_payload_cannot_change_other_settings(self):
        with self.assertRaises(ValidationError):
            hardware.HardwarePreference(dont_show_again=True, comfy_port=9000)


if __name__ == '__main__':
    unittest.main()
