"""Live GPU telemetry stays truthful, bounded, and shared across visible controls."""
import asyncio
from contextlib import nullcontext
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import AsyncMock, patch

from fastapi import HTTPException

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
sys.path.insert(0, str(Path(__file__).resolve().parent / 'helpers'))
import hardware_guide as hardware
import backend_settings_test as setting_fixtures

GIB = 1024**3


class GPUUsageParsingTests(unittest.TestCase):
    def test_driver_reports_real_utilization_and_mib_in_bytes(self):
        devices = hardware.parse_nvidia_usage('GPU-one, NVIDIA RTX 5090, 1536, 32768, 74\n'
                                              'GPU-two, "GPU, another", 0, 8192, 0\n')
        self.assertEqual(devices[0]['vram_used_bytes'], int(1.5 * GIB))
        self.assertEqual(devices[0]['vram_total_bytes'], 32 * GIB)
        self.assertEqual(devices[0]['utilization_percent'], 74)
        self.assertEqual(devices[1]['name'], 'GPU, another')
        self.assertEqual(devices[1]['utilization_percent'], 0)
        self.assertEqual(devices[1]['vram_used_bytes'], 0)

    def test_missing_and_invalid_metrics_are_unknown_never_zero(self):
        for sample in ('[N/A]', 'nan', 'inf', '-1', 'wrong'):
            device = hardware.parse_nvidia_usage(f'GPU-one, GPU, {sample}, {sample}, {sample}')[0]
            self.assertIsNone(device['vram_used_bytes'])
            self.assertIsNone(device['vram_total_bytes'])
            self.assertIsNone(device['utilization_percent'])
        impossible = hardware.parse_nvidia_usage('GPU-one, GPU, 8193, 8192, 101')[0]
        self.assertIsNone(impossible['vram_used_bytes'])
        self.assertIsNone(impossible['utilization_percent'])
        self.assertEqual(hardware.parse_nvidia_usage('no fields\n, GPU, 1, 2, 3\nGPU-one,,1,2,3'), [])

    def test_driver_query_has_timeout_and_restores_external_environment(self):
        result = subprocess.CompletedProcess([], 0, 'GPU-one, GPU, 1, 8192, 3\n')
        environment = {'PATH': '/usr/bin', 'LD_LIBRARY_PATH': '/host-libraries'}
        with patch('managed_ai.external_process_environment', return_value=nullcontext(environment)), \
             patch.object(hardware.subprocess, 'run', return_value=result) as run:
            self.assertEqual(len(hardware.nvidia_usage()), 1)
        arguments = run.call_args
        self.assertEqual(arguments.kwargs['timeout'], 1)
        self.assertEqual(arguments.kwargs['env'], environment)
        self.assertNotIn('shell', arguments.kwargs)
        self.assertIn('--query-gpu=uuid,name,memory.used,memory.total,utilization.gpu', arguments.args[0])

    def test_missing_or_timed_out_driver_remains_nonfatal(self):
        for error in (FileNotFoundError(), subprocess.TimeoutExpired('nvidia-smi', 1)):
            with patch.object(hardware.subprocess, 'run', side_effect=error):
                self.assertEqual(hardware.nvidia_usage(), [])

    def test_backend_primary_is_first_without_assuming_cuda_index_is_physical(self):
        driver = hardware.parse_nvidia_usage('GPU-one, First GPU, 1024, 8192, 10\n'
                                             'GPU-two, Second GPU, 2048, 16384, 75\n')
        stats = {'devices': [{'name': 'cuda:0 Second GPU : cudaMallocAsync', 'type': 'cuda', 'index': 0,
                              'vram_total': 16 * GIB, 'vram_free': 14 * GIB}]}
        merged = hardware.merge_gpu_usage(driver, stats)
        self.assertEqual(merged[0]['id'], 'GPU-two')
        self.assertTrue(merged[0]['is_backend_device'])
        self.assertEqual(merged[0]['utilization_percent'], 75)
        self.assertFalse(merged[1]['is_backend_device'])
        self.assertFalse(driver[1]['is_backend_device'], 'Merging must not mutate another cached snapshot.')

    def test_identical_models_do_not_guess_the_backend_driver_index(self):
        driver = hardware.parse_nvidia_usage('GPU-one, Same GPU, 1024, 8192, 10\n'
                                             'GPU-two, Same GPU, 2048, 8192, 75\n')
        stats = {'devices': [{'name': 'cuda:0 Same GPU : cudaMallocAsync', 'type': 'cuda', 'index': 0,
                              'vram_total': 8 * GIB, 'vram_free': 6 * GIB}]}
        merged = hardware.merge_gpu_usage(driver, stats)
        self.assertEqual(merged[0]['id'], 'comfy:cuda:0')
        self.assertTrue(merged[0]['is_backend_device'])
        self.assertIsNone(merged[0]['utilization_percent'])
        self.assertEqual(merged[0]['vram_used_bytes'], 2 * GIB)

    def test_comfy_reusable_cache_is_still_allocated_device_memory(self):
        stats = {'devices': [{'name': 'AMD GPU', 'type': 'cuda', 'index': 1,
                              'vram_total': 16 * GIB, 'vram_free': 14 * GIB, 'torch_vram_free': 2 * GIB}]}
        device = hardware.merge_gpu_usage([], stats)[0]
        self.assertEqual(device['vram_used_bytes'], 4 * GIB)
        self.assertEqual(device['memory_scope'], 'device')
        self.assertIsNone(device['utilization_percent'])

    def test_unknown_memory_and_shared_system_ram_are_not_fake_vram(self):
        for stats, scope in (({'devices': [{'name': 'GPU', 'type': 'cuda', 'vram_total': 'unknown'}]}, 'backend'),
                             ({'devices': [{'name': 'Apple GPU', 'type': 'mps', 'vram_total': 32 * GIB,
                                            'vram_free': 24 * GIB}]}, 'shared')):
            device = hardware.merge_gpu_usage([], stats)[0]
            self.assertIsNone(device['vram_used_bytes'])
            self.assertIsNone(device['vram_total_bytes'])
            self.assertEqual(device['memory_scope'], scope)
        self.assertEqual(hardware.merge_gpu_usage([], {'devices': [{'type': 'cpu'}, None]}), [])


class GPUUsagePollingTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.cache_patch = patch.object(hardware, '_usage_cache', None)
        self.task_patch = patch.object(hardware, '_usage_task', None)
        self.config_patch = patch.object(hardware, 'read_config', return_value={'comfy_port': 8188})
        self.cache_patch.start()
        self.task_patch.start()
        self.config = self.config_patch.start()
        self.addCleanup(self.cache_patch.stop)
        self.addCleanup(self.task_patch.stop)
        self.addCleanup(self.config_patch.stop)

    async def test_concurrent_consumers_share_a_probe_and_short_cache(self):
        started = asyncio.Event()
        finish = asyncio.Event()
        async def stats(port):
            started.set()
            await finish.wait()
            return {'devices': []}
        with patch.object(hardware, 'nvidia_usage', return_value=[]) as driver, \
             patch.object(hardware, '_usage_system_stats', side_effect=stats) as probe:
            first = asyncio.create_task(hardware.hardware_usage())
            await started.wait()
            second = asyncio.create_task(hardware.hardware_usage())
            await asyncio.sleep(0)
            finish.set()
            snapshots = await asyncio.gather(first, second)
            self.assertEqual(snapshots[0], snapshots[1])
            self.assertEqual(await hardware.hardware_usage(), snapshots[0])
        self.assertEqual(driver.call_count, 1)
        self.assertEqual(probe.call_count, 1)
        self.assertEqual(snapshots[0]['devices'], [])
        self.assertTrue(snapshots[0]['comfy_connected'])
        self.assertEqual(snapshots[0]['refresh_after_ms'], 2000)
        self.assertGreater(snapshots[0]['sampled_at'], 0)

    async def test_disconnected_request_cannot_cancel_another_consumers_sample(self):
        started = asyncio.Event()
        finish = asyncio.Event()
        async def stats(port):
            started.set()
            await finish.wait()
            return None
        with patch.object(hardware, 'nvidia_usage', return_value=[]) as driver, \
             patch.object(hardware, '_usage_system_stats', side_effect=stats):
            first = asyncio.create_task(hardware.hardware_usage())
            await started.wait()
            second = asyncio.create_task(hardware.hardware_usage())
            first.cancel()
            with self.assertRaises(asyncio.CancelledError):
                await first
            finish.set()
            snapshot = await second
        self.assertEqual(driver.call_count, 1)
        self.assertFalse(snapshot['comfy_connected'])
        self.assertEqual(snapshot['devices'], [])

    async def test_cache_expiry_and_backend_port_changes_refresh(self):
        with patch.object(hardware, 'nvidia_usage', return_value=[]) as driver, \
             patch.object(hardware, '_usage_system_stats', return_value=None):
            await hardware.hardware_usage()
            hardware._usage_cache = (8188, 0, hardware._usage_cache[2])
            await hardware.hardware_usage()
            self.config.return_value = {'comfy_port': 8189}
            await hardware.hardware_usage()
        self.assertEqual(driver.call_count, 3)

    async def test_comfy_connection_failure_does_not_hide_driver_metrics(self):
        devices = hardware.parse_nvidia_usage('GPU-one, GPU, 1024, 8192, 40\n')
        with patch.object(hardware, 'nvidia_usage', return_value=devices), \
             patch.object(hardware.aiohttp, 'ClientSession', side_effect=hardware.aiohttp.ClientError()):
            snapshot = await hardware.hardware_usage()
        self.assertFalse(snapshot['comfy_connected'])
        self.assertEqual(snapshot['devices'][0]['utilization_percent'], 40)


class GPUUsageGuardTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.fixture = setting_fixtures.BackendSettingsTests(methodName='runTest')
        self.fixture.setUp()
        self.modules = patch.dict(sys.modules, {'local_remove': self.fixture.app})
        self.modules.start()

    def tearDown(self):
        self.modules.stop()
        self.fixture.tearDown()

    async def test_telemetry_is_guarded_before_touching_hardware(self):
        with patch.object(hardware, 'hardware_usage', new_callable=AsyncMock) as usage:
            with self.assertRaises(HTTPException) as rejected:
                await hardware.get_hardware_usage(self.fixture.request(origin='https://remote.example'))
            self.assertEqual(rejected.exception.status_code, 403)
            usage.assert_not_awaited()
            usage.return_value = {'devices': []}
            self.assertEqual(await hardware.get_hardware_usage(self.fixture.request()), {'devices': []})
            usage.assert_awaited_once()


if __name__ == '__main__':
    unittest.main()
