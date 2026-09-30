"""Actual Comfy events, bounded observation, and history-completion fallback."""
import asyncio
import json
from pathlib import Path
import sys
import types
import unittest
from unittest.mock import AsyncMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
import operation_progress as progress

GRAPH = {'1': {'class_type': 'UNETLoader'}, '2': {'class_type': 'CLIPTextEncode'},
         '3': {'class_type': 'KSampler'}, '4': {'class_type': 'VAEDecode'},
         '5': {'class_type': 'SaveImage'}}


class ProgressTests(unittest.TestCase):
    def test_stages_are_node_specific_and_percent_is_only_sampler_steps(self):
        run = progress.OperationProgress(model='z-image-turbo')
        graph = progress.GraphProgress(run, GRAPH); graph.queued('ours')
        graph.accept({'type': 'executing', 'data': {'prompt_id': 'other', 'node': '3'}})
        self.assertEqual(run.stage, 'queued')
        for node, stage in [('1', 'loading'), ('2', 'conditioning'), ('3', 'sampling'), ('4', 'decoding'), ('5', 'saving')]:
            graph.accept({'type': 'executing', 'data': {'prompt_id': 'ours', 'node': node}})
            self.assertEqual(run.stage, stage)
            self.assertIsNone(run.progress)
        graph.accept({'type': 'progress', 'data': {'prompt_id': 'ours', 'node': '3', 'value': 2, 'max': 8}})
        self.assertEqual(run.progress, {'value': 2, 'max': 8, 'percent': 25.0})
        graph.accept({'type': 'executing', 'data': {'prompt_id': 'ours', 'node': None}})
        self.assertEqual(run.stage, 'saving'); self.assertTrue(run.active)
        self.assertIsNone(run.progress, 'Graph completion never promises an image before history and download')

    def test_prompt_queue_race_is_buffered_and_other_jobs_are_excluded(self):
        run = progress.OperationProgress(); graph = progress.GraphProgress(run, GRAPH)
        graph.accept({'type': 'executing', 'data': {'prompt_id': 'ours', 'node': '3'}})
        graph.accept({'type': 'execution_error', 'data': {'prompt_id': 'other', 'exception_message': 'unrelated'}})
        graph.queued('ours')
        self.assertEqual(run.stage, 'sampling'); self.assertIsNone(graph.error)
        for value, maximum in [(99, 8), (-1, 8), (True, 8), (1, 0), (float('nan'), 8), ('2', '8')]:
            graph.accept({'type': 'progress', 'data': {'prompt_id': 'ours', 'node': '3', 'value': value, 'max': maximum}})
            self.assertIsNone(run.progress)
        graph.accept({'type': 'progress', 'data': {'node': '3', 'value': 7, 'max': 8}})
        self.assertIsNone(run.progress, 'Events missing prompt identity cannot claim ownership')

    def test_context_success_error_and_context_reset(self):
        with progress.operation('qwen') as run:
            self.assertIs(progress.current(), run); self.assertTrue(progress.snapshot()['active'])
        self.assertIsNone(progress.current()); self.assertEqual(progress.snapshot()['stage'], 'completed')
        with self.assertRaisesRegex(RuntimeError, 'broken'):
            with progress.operation('qwen'):
                raise RuntimeError('broken')
        self.assertEqual(progress.snapshot()['stage'], 'error'); self.assertFalse(progress.snapshot()['active'])
        self.assertEqual(progress.snapshot()['error'], 'broken')


class Socket:
    def __init__(self, events=(), *, stall=False):
        self.events = list(events); self.stall = stall; self.closed = False
    def __aiter__(self): return self
    async def __anext__(self):
        if self.events:
            return self.events.pop(0)
        if self.stall:
            await asyncio.Future()
        raise StopAsyncIteration
    async def close(self): self.closed = True


class TransportProgressTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        source = Path(__file__).resolve().parents[1] / 'backend' / 'local_comfy_client.py'
        self.module = types.ModuleType('transport_progress_under_test')
        self.engine = types.SimpleNamespace(ComfyClient=type('ComfyClient', (), {}), config=types.SimpleNamespace(ws_url='ws://127.0.0.1:8188/ws'))
        with patch.dict(sys.modules, {'engine': self.engine}):
            exec(compile(source.read_text(encoding='utf-8'), str(source), 'exec'), self.module.__dict__)
        self.client = self.module.LocalComfyClient(); self.client.client_id = 'ours-client'
        self.client._queue_prompt = AsyncMock(return_value='ours')
        self.client._get_history = AsyncMock(return_value={'ours': {'status': {'completed': True}, 'outputs': {'5': {'images': [{}]}}}})
        self.client._fetch_image = AsyncMock(return_value=b'image-png')
        self.client._upload_image = AsyncMock()
        self.socket = Socket(stall=True)
        owner = self
        class Session:
            async def __aenter__(self): return self
            async def __aexit__(self, *args): pass
        self.http = patch.object(self.module.aiohttp, 'ClientSession', lambda **kwargs: Session()); self.http.start()
        async def open_socket(graph):
            return owner.socket, asyncio.create_task(owner.client._observe_progress(owner.socket, graph))
        self.client._open_progress = AsyncMock(side_effect=open_socket)
    def tearDown(self): self.http.stop()

    async def test_stalled_socket_cannot_delay_history_completion(self):
        with progress.operation('z-image-turbo') as run:
            result = await asyncio.wait_for(self.client.execute(GRAPH), timeout=.25)
        self.assertEqual(result, b'image-png'); self.assertTrue(self.socket.closed)
        self.client._queue_prompt.assert_awaited_once(); self.assertEqual(run.stage, 'completed')

    async def test_disconnect_uses_history_without_resubmission(self):
        self.socket = Socket()
        async def history(prompt_id):
            await asyncio.sleep(0)
            return {'ours': {'outputs': {'5': {'images': [{}]}}}}
        self.client._get_history.side_effect = history
        with progress.operation('qwen') as run:
            self.assertEqual(await self.client.execute(GRAPH), b'image-png')
            self.assertTrue(run.connection_lost)
        self.client._queue_prompt.assert_awaited_once(); self.client._fetch_image.assert_awaited_once()

    async def test_matching_socket_error_rejects_partial_history_output(self):
        for kind in ['execution_error', 'execution_interrupted']:
            with self.subTest(kind=kind):
                self.socket = Socket([json.dumps({'type': kind, 'data': {'prompt_id': 'ours', 'exception_message': 'GPU stopped'}})])
                async def history(prompt_id):
                    await asyncio.sleep(0)
                    return {'ours': {'outputs': {'5': {'images': [{}]}}}}
                self.client._get_history.side_effect = history
                with self.assertRaisesRegex(RuntimeError, 'GPU stopped'):
                    with progress.operation('qwen') as run:
                        await self.client.execute(GRAPH)
                self.assertEqual(run.stage, 'error'); self.assertTrue(self.socket.closed)
                self.client._fetch_image.assert_not_awaited()
        self.assertEqual(self.client._queue_prompt.await_count, 2)

    async def test_cancellation_closes_observer_and_keeps_single_submission(self):
        queued = asyncio.Event()
        async def history(prompt_id):
            queued.set(); await asyncio.Future()
        self.client._get_history.side_effect = history
        async def execute():
            with progress.operation('qwen'):
                await self.client.execute(GRAPH)
        task = asyncio.create_task(execute()); await queued.wait(); task.cancel()
        with self.assertRaises(asyncio.CancelledError): await task
        self.assertTrue(self.socket.closed); self.client._queue_prompt.assert_awaited_once()
        self.assertEqual(progress.snapshot()['stage'], 'cancelled')
        self.client._fetch_image.assert_not_awaited()

    async def test_open_observer_uses_no_ping_deadline_and_failure_is_optional(self):
        del self.client._open_progress
        connect = AsyncMock(return_value=self.socket)
        with patch.dict(sys.modules, {'engine': self.engine}), patch('websockets.connect', connect):
            with progress.operation('qwen') as run:
                self.assertEqual(await self.client.execute(GRAPH), b'image-png')
        self.assertIsNone(connect.await_args.kwargs['ping_interval']); self.assertEqual(connect.await_args.kwargs['open_timeout'], 1)
        connect.side_effect = OSError('no socket')
        with patch.dict(sys.modules, {'engine': self.engine}), patch('websockets.connect', connect):
            with progress.operation('qwen') as run:
                self.assertEqual(await self.client.execute(GRAPH), b'image-png'); self.assertTrue(run.connection_lost)
        self.assertEqual(self.client._queue_prompt.await_count, 2)


class ProgressRouteTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        import test_image_generation as generation_cases
        self.cases = generation_cases
        self.harness = generation_cases.GenerationTests(); self.harness.setUp()
    def tearDown(self): self.harness.tearDown()

    async def test_progress_requires_token_and_generation_context_finishes(self):
        from fastapi import HTTPException
        from PIL import Image
        with self.assertRaises(HTTPException) as denied:
            await self.harness.api.generation_progress(self.harness.fixture.request(csrf=False))
        self.assertEqual(denied.exception.status_code, 403)
        async def generate(*args, **kwargs):
            run = progress.current(); self.assertIsNotNone(run)
            run.update('sampling', source='comfyui', value=3, maximum=8)
            status = await self.harness.api.generation_progress(self.harness.fixture.request())
            self.assertTrue(status['active']); self.assertEqual(status['progress']['value'], 3)
            return Image.new('RGBA', (256, 256), 'green')
        with patch.object(self.cases.qwen_image, 'run_qwen_image', generate):
            await self.harness.api.generate_image(self.harness.fixture.request(), self.harness.payload())
        status = await self.harness.api.generation_progress(self.harness.fixture.request())
        self.assertFalse(status['active']); self.assertEqual(status['stage'], 'completed')

    async def test_capability_reads_cache_but_actual_generation_validates_fresh(self):
        import io
        from PIL import Image
        stream = io.BytesIO(); Image.new('RGBA', (256, 256), 'green').save(stream, format='PNG')
        with patch.object(self.cases.qwen_image, '_object_info', AsyncMock(return_value=self.cases.combined_inventory())) as info, patch.object(self.cases.qwen_image, '_execute_workflow', AsyncMock(return_value=stream.getvalue())):
            await self.harness.api.generation_models(self.harness.fixture.request())
            await self.harness.api.generation_models(self.harness.fixture.request())
            self.assertEqual(info.await_count, 1)
            await self.harness.api.generate_image(self.harness.fixture.request(), self.harness.payload())
            self.assertEqual(info.await_count, 2, 'Actual graph validation must not use the inventory cache')


if __name__ == '__main__': unittest.main()
