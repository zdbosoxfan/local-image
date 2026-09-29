"""Hardware detection is informational and tolerates missing GPU utilities."""
from pathlib import Path
import asyncio
import subprocess
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
import hardware_guide as hardware


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


if __name__ == '__main__':
    unittest.main()
