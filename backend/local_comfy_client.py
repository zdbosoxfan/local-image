"""Comfy client that tolerates long local model-loading and sampling stalls."""
import asyncio
import json
import logging
from pathlib import Path
import time
from urllib.parse import quote

import aiohttp
from engine import ComfyClient
from operation_progress import current, GraphProgress, OperationCancelled, CancellationUnavailable

logger = logging.getLogger('Engine')


class LocalComfyClient(ComfyClient):
    async def _control_request(self, method, path, body=None):
        # A separate short connection keeps Stop responsive even when the
        # image/history connection is waiting through a driver stall.
        async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=4)) as session:
            kwargs = {'json': body} if body is not None else {}
            async with session.request(method, self._control_url + path, allow_redirects=False, **kwargs) as response:
                data = await response.json() if response.content_type == 'application/json' else {}
                return response.status, data

    async def _queue_status(self, prompt_id):
        status, queue = await self._control_request('GET', '/queue')
        if status != 200 or not isinstance(queue, dict):
            raise RuntimeError('The AI backend did not return a valid image queue.')
        def contains(key):
            rows = queue.get(key)
            if not isinstance(rows, list) or any(not isinstance(row, (list, tuple)) or len(row) < 2 or not isinstance(row[1], str) for row in rows):
                raise RuntimeError('The AI backend did not return a valid image queue.')
            return any(row[1] == prompt_id for row in rows)
        return contains('queue_running'), contains('queue_pending')

    async def _cancel_prompt(self, prompt_id):
        # This endpoint atomically tests ownership inside Comfy's queue lock.
        # Never use the global /interrupt: older servers ignore prompt_id and
        # could stop a different application's job in the completion race.
        status, data = await self._control_request('POST', '/api/jobs/' + quote(prompt_id, safe='') + '/cancel', {})
        if status == 200:
            if not isinstance(data, dict) or type(data.get('cancelled')) is not bool:
                raise RuntimeError('The AI backend did not confirm whether the image was stopped.')
            return data['cancelled']
        if status not in (404, 405):
            raise RuntimeError('The AI backend could not stop this image. Check its connection and try again.')
        running, pending = await self._queue_status(prompt_id)
        if pending:
            deleted, _ = await self._control_request('POST', '/queue', {'delete': [prompt_id]})
            if deleted != 200:
                raise RuntimeError('The AI backend could not remove the queued image.')
            running, pending = await self._queue_status(prompt_id)
            if not running and not pending:
                return True
        if running or pending:
            raise CancellationUnavailable('Update the AI backend to enable safely stopping a running image. Progress is still being monitored.')
        return False

    async def _open_progress(self, graph):
        """Optional observer, independent of HTTP completion and its deadline."""
        from engine import config
        import websockets
        try:
            # No ping deadline: a GPU/driver stall must not interrupt the job
            # when ComfyUI's event loop temporarily cannot send websocket pongs.
            socket = await websockets.connect(f'{config.ws_url}?clientId={self.client_id}',
                open_timeout=1, close_timeout=.25, ping_interval=None, max_size=1024 * 1024)
        except Exception:
            graph.run.connection_lost = True
            return None, None
        return socket, asyncio.create_task(self._observe_progress(socket, graph))

    async def _observe_progress(self, socket, graph):
        try:
            async for message in socket:
                if isinstance(message, str):
                    try:
                        graph.accept(json.loads(message))
                    except (ValueError, TypeError):
                        continue
        except asyncio.CancelledError:
            raise
        except Exception:
            graph.run.connection_lost = True
        else:
            graph.run.connection_lost = True

    async def _close_progress(self, socket, observer):
        if observer is not None:
            observer.cancel()
            try:
                await observer
            except asyncio.CancelledError:
                pass
        if socket is not None:
            try:
                await asyncio.wait_for(socket.close(), timeout=.5)
            except Exception:
                pass

    async def execute(self, workflow):
        # Model loading/driver stalls can starve Comfy's websocket pongs. Query
        # job history instead; never resubmit a job after a transient timeout.
        timeout = aiohttp.ClientTimeout(total=120, connect=20)
        async with aiohttp.ClientSession(timeout=timeout) as session:
            self.session = session
            run = current()
            graph = GraphProgress(run, workflow) if run else None
            if run:
                from engine import config
                self._control_url = config.http_url
                run._cancel_handler = self._cancel_prompt
            socket = observer = None
            try:
                if run:
                    run.check_cancelled()
                for node in workflow.values():
                    if run:
                        run.check_cancelled()
                    if node.get('class_type') == 'LoadImage':
                        node['inputs']['image'] = await self._upload_image(Path(node['inputs']['image']))
                if graph:
                    socket, observer = await self._open_progress(graph)
                    run.check_cancelled()
                prompt_id = await self._queue_prompt(workflow)
                if graph:
                    graph.queued(prompt_id)
                    if run.cancel_requested:
                        try:
                            await run.dispatch_cancel()
                        except Exception as error:
                            # Stop may have arrived before queue submission
                            # returned. If an older/unreachable backend cannot
                            # stop it, retain the observer and original request.
                            run.cancel_error = str(error)[:300]
                            logger.warning('Could not stop image job %s: %s', prompt_id, error)
                logger.info('Local image job queued: %s', prompt_id)
                started = time.monotonic()
                deadline = started + 20 * 60
                while time.monotonic() < deadline:
                    if graph and graph.error:
                        raise graph.error
                    try:
                        history = await asyncio.wait_for(self._get_history(prompt_id), timeout=4)
                    except (aiohttp.ClientConnectionError, asyncio.TimeoutError):
                        await asyncio.sleep(2)
                        continue
                    if graph and graph.error:
                        raise graph.error
                    item = history.get(prompt_id)
                    if item:
                        status = item.get('status', {})
                        for kind, details in status.get('messages', []):
                            if kind in ('execution_error', 'execution_interrupted'):
                                if kind == 'execution_interrupted' and run and run.cancel_requested:
                                    raise OperationCancelled('Image operation cancelled.')
                                raise RuntimeError('Image operation failed: ' + details.get('exception_message', kind))
                        if status.get('status_str') == 'error':
                            raise RuntimeError('The GPU could not complete this image operation.')
                        if item.get('outputs'):
                            if run:
                                run.prevent_cancel()
                                run.update('saving')
                            return await self._fetch_image(item['outputs'])
                        if status.get('completed'):
                            raise RuntimeError('The image operation finished without an image.')
                    if run and run.cancel_dispatched:
                        try:
                            running, pending = await self._queue_status(prompt_id)
                        except (aiohttp.ClientError, asyncio.TimeoutError, RuntimeError):
                            await asyncio.sleep(.5)
                            continue
                        if not running and not pending:
                            # A dequeued prompt has no history. A completion
                            # race may populate history just after the first
                            # read, so read once more before discarding output.
                            try:
                                history = await asyncio.wait_for(self._get_history(prompt_id), timeout=4)
                            except (aiohttp.ClientConnectionError, asyncio.TimeoutError):
                                await asyncio.sleep(.5)
                                continue
                            item = history.get(prompt_id)
                            status = item.get('status', {}) if item else {}
                            for kind, details in status.get('messages', []):
                                if kind == 'execution_interrupted':
                                    raise OperationCancelled('Image operation cancelled.')
                                if kind == 'execution_error':
                                    raise RuntimeError('Image operation failed: ' + details.get('exception_message', kind))
                            if status.get('status_str') == 'error':
                                raise RuntimeError('The GPU could not complete this image operation.')
                            if item and item.get('outputs'):
                                run.prevent_cancel(); run.update('saving')
                                return await self._fetch_image(item['outputs'])
                            raise OperationCancelled('Image operation cancelled.')
                    # Fast warm jobs should not wait behind a two-second poll.
                    # Ease off for loading/sampling jobs that are still running.
                    elapsed = time.monotonic() - started
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        break
                    interval = 0.15 if elapsed < 10 else 0.5 if elapsed < 60 else 1.0
                    await asyncio.sleep(min(interval, remaining))
                raise TimeoutError('The GPU did not finish this image operation within 20 minutes.')
            finally:
                if graph:
                    await self._close_progress(socket, observer)
                    run._cancel_handler = None
