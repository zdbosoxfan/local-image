"""Stop only the owned prompt, preserve completed images, and keep truthful state."""
import asyncio
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import AsyncMock, patch, call

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
import operation_progress as progress
import test_operation_progress as progress_cases
GRAPH = progress_cases.GRAPH


class CancellationStateTests(unittest.IsolatedAsyncioTestCase):
    async def test_submission_id_correlates_progress_and_rejects_previous_submission(self):
        first = 'aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa'
        second = 'bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb'
        with progress.operation('qwen', job_id=first):
            self.assertEqual(progress.snapshot()['job_id'], first)
        with progress.operation('qwen', job_id=second) as run:
            self.assertEqual(progress.snapshot()['job_id'], second)
            with self.assertRaisesRegex(ValueError, 'changed'):
                await progress.cancel(first)
            self.assertFalse(run.cancel_requested)
            state = await progress.cancel(second)
            self.assertTrue(state['cancellation_requested'])

    async def test_preparation_can_stop_without_ever_submitting_and_stale_id_cannot_stop_next_job(self):
        with self.assertRaises(progress.OperationCancelled):
            with progress.operation('qwen') as run:
                with self.assertRaisesRegex(ValueError, 'changed'):
                    await progress.cancel('not-this-job')
                self.assertFalse(run.cancel_requested)
                state = await progress.cancel(run.job_id)
                self.assertTrue(state['cancellation_requested']); self.assertTrue(state['cancelling'])
                self.assertFalse(state['can_cancel']); self.assertEqual(state['stage'], 'cancelling')
                run.check_cancelled()
        self.assertEqual(progress.snapshot()['stage'], 'cancelled')
        self.assertFalse(progress.snapshot()['active'])

    async def test_saving_and_completed_results_win_late_stop(self):
        with progress.operation('qwen') as run:
            run.prompt_id = 'owned'
            run.prevent_cancel(); run.update('saving')
            state = await progress.cancel(run.job_id)
            self.assertFalse(state['cancellation_requested']); self.assertFalse(state['can_cancel'])
            self.assertEqual(state['stage'], 'saving')
        state = await progress.cancel(run.job_id)
        self.assertFalse(state['cancellation_requested']); self.assertEqual(state['stage'], 'completed')

    async def test_failed_cancel_restores_observation_and_allows_retry(self):
        with progress.operation('qwen') as run:
            run.prompt_id = 'owned'; run.update('sampling', source='comfyui', value=1, maximum=8)
            run._cancel_handler = AsyncMock(side_effect=progress.CancellationUnavailable('Update backend'))
            with self.assertRaisesRegex(progress.CancellationUnavailable, 'Update'):
                await progress.cancel(run.job_id)
            self.assertFalse(run.cancel_requested); self.assertTrue(run.snapshot()['can_cancel'])
            self.assertEqual(run.stage, 'sampling'); self.assertTrue(run.active)

    async def test_result_wins_while_cancel_http_is_still_pending(self):
        for outcome in (False, True, OSError('late connection failure')):
            with self.subTest(outcome=outcome):
                entered, resume = asyncio.Event(), asyncio.Event()
                async def handler(prompt_id):
                    entered.set(); await resume.wait()
                    if isinstance(outcome, Exception): raise outcome
                    return outcome
                with progress.operation('qwen') as run:
                    run.prompt_id = 'ours'; run.update('sampling'); run._cancel_handler = handler
                    task = asyncio.create_task(progress.cancel(run.job_id)); await entered.wait()
                    run.prevent_cancel(); run.update('saving'); resume.set()
                    state = await task
                    self.assertFalse(state['cancellation_requested']); self.assertFalse(state['cancelling'])
                    self.assertFalse(run.cancel_dispatched); self.assertEqual(run.stage, 'saving')


class CancellationTransportTests(unittest.IsolatedAsyncioTestCase):
    setUp = progress_cases.TransportProgressTests.setUp
    tearDown = progress_cases.TransportProgressTests.tearDown

    async def test_atomic_cancel_sends_only_the_owned_prompt_and_is_idempotent(self):
        self.client._control_request = AsyncMock(return_value=(200, {'cancelled': True}))
        with progress.operation('qwen') as run:
            run.prompt_id = 'our-prompt'; run._cancel_handler = self.client._cancel_prompt
            await progress.cancel(run.job_id); await progress.cancel(run.job_id)
            self.assertTrue(run.cancel_dispatched)
        self.client._control_request.assert_awaited_once_with('POST', '/api/jobs/our-prompt/cancel', {})

    async def test_legacy_queue_deletes_only_our_pending_prompt_and_leaves_other_work_running(self):
        self.client._control_request = AsyncMock(side_effect=[(404, {}),
            (200, {'queue_running': [[0, 'other']], 'queue_pending': [[1, 'ours'], [2, 'another']]}),
            (200, {}), (200, {'queue_running': [[0, 'other']], 'queue_pending': [[2, 'another']]})])
        self.assertTrue(await self.client._cancel_prompt('ours'))
        self.assertEqual(self.client._control_request.await_args_list, [
            call('POST', '/api/jobs/ours/cancel', {}), call('GET', '/queue'),
            call('POST', '/queue', {'delete': ['ours']}), call('GET', '/queue')])

    async def test_legacy_running_prompt_never_uses_global_interrupt(self):
        self.client._control_request = AsyncMock(side_effect=[(404, {}),
            (200, {'queue_running': [[0, 'ours']], 'queue_pending': [[1, 'other']]})])
        with self.assertRaisesRegex(progress.CancellationUnavailable, 'Update'):
            await self.client._cancel_prompt('ours')
        self.assertEqual(self.client._control_request.await_count, 2)
        self.assertNotIn('/interrupt', str(self.client._control_request.await_args_list))

    async def test_stop_during_queue_submission_cancels_once_id_is_known_and_closes_observer(self):
        submitting, resume = asyncio.Event(), asyncio.Event()
        async def queue(workflow):
            submitting.set(); await resume.wait(); return 'ours'
        self.client._queue_prompt.side_effect = queue
        self.client._cancel_prompt = AsyncMock(return_value=True)
        self.client._get_history.return_value = {}
        self.client._queue_status = AsyncMock(return_value=(False, False))
        async def execute():
            with progress.operation('qwen'):
                return await self.client.execute(GRAPH)
        task = asyncio.create_task(execute()); await submitting.wait()
        state = await progress.cancel(progress.snapshot()['job_id'])
        self.assertTrue(state['cancelling']); resume.set()
        with self.assertRaises(progress.OperationCancelled): await task
        self.client._cancel_prompt.assert_awaited_once_with('ours')
        self.client._queue_prompt.assert_awaited_once(); self.client._fetch_image.assert_not_awaited()
        self.assertEqual(progress.snapshot()['stage'], 'cancelled'); self.assertTrue(self.socket.closed)

    async def test_history_interrupted_after_stop_rejects_partial_outputs(self):
        entered, resume = asyncio.Event(), asyncio.Event()
        async def history(prompt_id):
            entered.set(); await resume.wait()
            return {'ours': {'status': {'status_str': 'error', 'messages': [['execution_interrupted', {}]]}, 'outputs': {'5': {'images': [{}]}}}}
        self.client._get_history.side_effect = history
        self.client._cancel_prompt = AsyncMock(return_value=True)
        async def execute():
            with progress.operation('qwen'):
                return await self.client.execute(GRAPH)
        task = asyncio.create_task(execute()); await entered.wait()
        await progress.cancel(progress.snapshot()['job_id']); resume.set()
        with self.assertRaises(progress.OperationCancelled): await task
        self.client._fetch_image.assert_not_awaited()
        self.assertEqual(progress.snapshot()['stage'], 'cancelled')

    async def test_successful_history_preserves_image_when_stop_was_too_late(self):
        entered, resume = asyncio.Event(), asyncio.Event()
        async def history(prompt_id):
            entered.set(); await resume.wait()
            return {'ours': {'status': {'completed': True, 'status_str': 'success'}, 'outputs': {'5': {'images': [{}]}}}}
        self.client._get_history.side_effect = history
        self.client._cancel_prompt = AsyncMock(return_value=False)
        async def execute():
            with progress.operation('qwen'):
                return await self.client.execute(GRAPH)
        task = asyncio.create_task(execute()); await entered.wait()
        state = await progress.cancel(progress.snapshot()['job_id'])
        self.assertFalse(state['cancellation_requested']); resume.set()
        self.assertEqual(await task, b'image-png')
        self.assertEqual(progress.snapshot()['stage'], 'completed')
        self.client._fetch_image.assert_awaited_once()


class CancellationRouteTests(unittest.IsolatedAsyncioTestCase):
    setUp = progress_cases.ProgressRouteTests.setUp
    tearDown = progress_cases.ProgressRouteTests.tearDown

    async def post(self, payload, headers=None):
        # Exercise FastAPI's complete ASGI route (body validation, auth and
        # HTTP serialization) without adding a production httpx dependency.
        from fastapi import FastAPI
        app = FastAPI(); app.include_router(self.harness.api.router)
        body = json.dumps(payload).encode()
        values = {'host': '127.0.0.1:5000', 'origin': 'http://127.0.0.1:5000',
                  'content-type': 'application/json', 'x-local-remove-token': self.harness.editor.CSRF,
                  **(headers or {})}
        scope = {'type': 'http', 'http_version': '1.1', 'method': 'POST', 'scheme': 'http',
                 'path': '/api/local-remove/generation/cancel', 'root_path': '', 'query_string': b'',
                 'server': ('127.0.0.1', 5000), 'client': ('127.0.0.1', 1234),
                 'headers': [(name.encode(), value.encode()) for name, value in values.items()]}
        messages = []
        async def receive(): return {'type': 'http.request', 'body': body, 'more_body': False}
        async def send(message): messages.append(message)
        await app(scope, receive, send)
        status = next(message['status'] for message in messages if message['type'] == 'http.response.start')
        response = b''.join(message.get('body', b'') for message in messages if message['type'] == 'http.response.body')
        return status, json.loads(response)

    async def test_http_wrong_token_origin_cross_site_and_malformed_body_cannot_stop_job(self):
        with progress.operation('qwen') as run:
            for headers in ({'x-local-remove-token': 'wrong'}, {'origin': 'https://outside.example'}, {'sec-fetch-site': 'cross-site'}):
                status, _ = await self.post({'job_id': run.job_id}, headers)
                self.assertEqual(status, 403); self.assertFalse(run.cancel_requested)
            for body in ({'job_id': '../other-job'}, {'job_id': run.job_id, 'prompt_id': 'other'}):
                status, _ = await self.post(body)
                self.assertEqual(status, 422); self.assertFalse(run.cancel_requested)
            status, body = await self.post({'job_id': run.job_id})
            self.assertEqual(status, 200); self.assertTrue(body['cancellation_requested'])

    async def test_http_old_job_cannot_cancel_a_new_generation(self):
        with progress.operation('qwen') as old:
            pass
        with progress.operation('z-image-turbo') as new:
            status, _ = await self.post({'job_id': old.job_id})
            self.assertEqual(status, 409); self.assertFalse(new.cancel_requested)
            status, body = await self.post({'job_id': new.job_id})
            self.assertEqual(status, 200); self.assertEqual(body['job_id'], new.job_id)
            self.assertTrue(new.cancel_requested)

    async def test_stop_requires_csrf_and_rejects_stale_operation(self):
        from fastapi import HTTPException
        with progress.operation('qwen') as run:
            payload = self.harness.api.CancelRequest(job_id=run.job_id)
            with self.assertRaises(HTTPException) as denied:
                await self.harness.api.cancel_generation(self.harness.fixture.request(csrf=False), payload)
            self.assertEqual(denied.exception.status_code, 403); self.assertFalse(run.cancel_requested)
            stale = self.harness.api.CancelRequest(job_id='00000000-0000-0000-0000-000000000000')
            with self.assertRaises(HTTPException) as conflict:
                await self.harness.api.cancel_generation(self.harness.fixture.request(), stale)
            self.assertEqual(conflict.exception.status_code, 409); self.assertFalse(run.cancel_requested)
            state = await self.harness.api.cancel_generation(self.harness.fixture.request(), payload)
            self.assertTrue(state['cancellation_requested'])

    async def test_cancelled_generation_returns_409_without_publishing_a_session(self):
        from fastapi import HTTPException
        async def cancelled(*args, **kwargs):
            run = progress.current()
            await self.harness.api.cancel_generation(self.harness.fixture.request(), self.harness.api.CancelRequest(job_id=run.job_id))
            run.check_cancelled()
        before = set(self.harness.editor.SESSIONS.iterdir())
        with patch.object(self.cases.qwen_image, 'run_qwen_image', cancelled):
            with self.assertRaises(HTTPException) as stopped:
                await self.harness.api.generate_image(self.harness.fixture.request(), self.harness.payload())
        self.assertEqual(stopped.exception.status_code, 409)
        self.assertEqual(stopped.exception.detail, 'Image operation cancelled.')
        self.assertEqual(set(self.harness.editor.SESSIONS.iterdir()), before)
        self.assertEqual(progress.snapshot()['stage'], 'cancelled')
        self.assertFalse(self.harness.fixture.fixture.main.generation_lock.locked())


class UpscaleCancellationTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        import test_image_upscale
        self.harness = test_image_upscale.UpscaleTests(); self.harness.setUp()

    def tearDown(self):
        self.harness.tearDown()

    async def test_upscale_is_observable_and_stop_preserves_its_source_without_publication(self):
        from fastapi import HTTPException
        _, source, _ = self.harness.source()
        before = self.harness.editor.read_session(source['id'])
        async def cancelled(*args, **kwargs):
            run = progress.current()
            self.assertEqual(run.model, 'seedvr2'); self.assertEqual(run.operation, 'upscale')
            await progress.cancel(run.job_id); run.check_cancelled()
        self.harness.adapter.run_seedvr2_image = cancelled
        with self.assertRaises(HTTPException) as stopped:
            await self.harness.api.upscale_image(self.harness.fixture.request(), self.harness.request(source))
        self.assertEqual(stopped.exception.status_code, 409)
        self.assertEqual(stopped.exception.detail, 'Image operation cancelled.')
        self.assertEqual(self.harness.editor.read_session(source['id']), before)
        self.assertEqual(len(list(self.harness.editor.SESSIONS.iterdir())), 1)
        self.assertFalse(self.harness.fixture.fixture.main.generation_lock.locked())
        self.assertEqual(progress.snapshot()['stage'], 'cancelled')


if __name__ == '__main__': unittest.main()
