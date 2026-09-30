"""Local job completion, timeout recovery and single-submission guarantees."""
import asyncio
from pathlib import Path
import sys
import types
import unittest
from unittest.mock import AsyncMock, patch

import aiohttp

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE / 'backend'))


class LocalComfyClientTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        # The inherited HTTP methods are mocked here; avoid initializing the
        # unrelated engine image cache and user profile during transport tests.
        self.module = types.ModuleType('local_comfy_client_under_test')
        source = HERE / 'backend' / 'local_comfy_client.py'
        with patch.dict(sys.modules, {'engine': types.SimpleNamespace(ComfyClient=type('ComfyClient', (), {}))}):
            exec(compile(source.read_text(encoding='utf-8'), str(source), 'exec'), self.module.__dict__)
        self.elapsed = 0.0
        self.sleeps = []
        self.closed = False
        owner = self
        class Session:
            async def __aenter__(self): return self
            async def __aexit__(self, *args): owner.closed = True
        self.module.aiohttp = types.SimpleNamespace(ClientSession=lambda **kwargs: Session(),
            ClientTimeout=aiohttp.ClientTimeout, ClientConnectionError=aiohttp.ClientConnectionError)
        async def sleep(delay):
            self.sleeps.append(delay)
            self.elapsed += delay
        self.module.asyncio = types.SimpleNamespace(sleep=sleep, TimeoutError=asyncio.TimeoutError)
        self.module.time = types.SimpleNamespace(monotonic=lambda: self.elapsed)
        self.client = self.module.LocalComfyClient()
        self.client._upload_image = AsyncMock(return_value='uploaded.png')
        self.client._queue_prompt = AsyncMock(return_value='job-123')
        self.client._get_history = AsyncMock()
        self.client._fetch_image = AsyncMock(return_value=b'finished-png')
        self.output = {'save-node': {'images': [{'filename': 'result.png'}]}}

    def complete(self):
        return {'job-123': {'status': {'completed': True}, 'outputs': self.output}}

    async def test_fast_completion_fetches_prompt_output_without_two_second_floor(self):
        self.client._get_history.side_effect = [{}, self.complete()]
        result = await self.client.execute({})
        self.assertEqual(result, b'finished-png')
        self.assertLess(self.elapsed, 0.5)
        self.client._queue_prompt.assert_awaited_once_with({})
        self.client._fetch_image.assert_awaited_once_with(self.output)
        self.assertTrue(self.closed)

    async def test_transient_history_failures_never_reupload_or_resubmit(self):
        workflow = {'source': {'class_type': 'LoadImage', 'inputs': {'image': 'source.png'}},
                    'mask': {'class_type': 'LoadImage', 'inputs': {'image': 'mask.png'}}}
        self.client._upload_image.side_effect = ['uploaded-source.png', 'uploaded-mask.png']
        self.client._get_history.side_effect = [asyncio.TimeoutError(), aiohttp.ClientConnectionError(),
            {'another-job': {'outputs': self.output}}, {}, self.complete()]
        result = await self.client.execute(workflow)
        self.assertEqual(result, b'finished-png')
        self.assertEqual(self.client._upload_image.await_args_list,
                         [unittest.mock.call(Path('source.png')), unittest.mock.call(Path('mask.png'))])
        self.client._queue_prompt.assert_awaited_once_with(workflow)
        self.assertEqual(workflow['source']['inputs']['image'], 'uploaded-source.png')
        self.assertEqual(workflow['mask']['inputs']['image'], 'uploaded-mask.png')
        self.assertEqual(self.client._get_history.await_args_list, [unittest.mock.call('job-123')] * 5)
        self.assertEqual(self.sleeps[:2], [2, 2])
        self.client._fetch_image.assert_awaited_once_with(self.output)
        self.assertTrue(self.closed)

    async def test_gpu_error_or_interruption_never_returns_partial_outputs(self):
        cases = [({'messages': [('execution_error', {'exception_message': 'out of memory'})]}, 'out of memory'),
                 ({'messages': [('execution_interrupted', {})]}, 'execution_interrupted'),
                 ({'status_str': 'error'}, 'could not complete')]
        for status, message in cases:
            with self.subTest(status=status):
                self.client._get_history.return_value = {'job-123': {'status': status, 'outputs': self.output}}
                with self.assertRaisesRegex(RuntimeError, message):
                    await self.client.execute({})
                self.client._fetch_image.assert_not_awaited()
                self.assertTrue(self.closed)
        self.assertEqual(self.client._queue_prompt.await_count, len(cases))

    async def test_completed_without_image_reports_error_instead_of_waiting(self):
        self.client._get_history.return_value = {'job-123': {'status': {'completed': True}, 'outputs': {}}}
        with self.assertRaisesRegex(RuntimeError, 'finished without an image'):
            await self.client.execute({})
        self.client._queue_prompt.assert_awaited_once()
        self.client._fetch_image.assert_not_awaited()
        self.assertEqual(self.sleeps, [])

    async def test_twenty_minute_timeout_keeps_original_queued_job(self):
        async def history(prompt_id):
            self.elapsed += 600
            return {}
        self.client._get_history.side_effect = history
        with self.assertRaisesRegex(TimeoutError, 'within 20 minutes'):
            await self.client.execute({})
        self.client._queue_prompt.assert_awaited_once()
        self.assertEqual(self.client._get_history.await_count, 2)
        self.client._fetch_image.assert_not_awaited()
        self.assertTrue(self.closed)

    async def test_upload_and_queue_failures_do_not_retry_submission(self):
        self.client._upload_image.side_effect = OSError('upload failed')
        with self.assertRaisesRegex(OSError, 'upload failed'):
            await self.client.execute({'source': {'class_type': 'LoadImage', 'inputs': {'image': 'source.png'}}})
        self.client._queue_prompt.assert_not_awaited()
        self.client._queue_prompt.side_effect = asyncio.TimeoutError()
        with self.assertRaises(asyncio.TimeoutError):
            await self.client.execute({})
        self.client._queue_prompt.assert_awaited_once()
        self.client._get_history.assert_not_awaited()
        self.client._fetch_image.assert_not_awaited()
        self.assertTrue(self.closed)

    async def test_output_download_failure_propagates_without_resubmission(self):
        self.client._get_history.return_value = self.complete()
        self.client._fetch_image.side_effect = aiohttp.ClientConnectionError('result connection lost')
        with self.assertRaisesRegex(aiohttp.ClientConnectionError, 'result connection lost'):
            await self.client.execute({})
        self.client._queue_prompt.assert_awaited_once()
        self.client._get_history.assert_awaited_once()
        self.client._fetch_image.assert_awaited_once()
        self.assertTrue(self.closed)


if __name__ == '__main__': unittest.main()
