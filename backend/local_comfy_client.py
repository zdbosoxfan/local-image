"""Comfy client that tolerates long local model-loading and sampling stalls."""
import asyncio
import json
import logging
from pathlib import Path
import time

import aiohttp
from engine import ComfyClient
from operation_progress import current, GraphProgress

logger = logging.getLogger('Engine')


class LocalComfyClient(ComfyClient):
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
            socket = observer = None
            try:
                for node in workflow.values():
                    if node.get('class_type') == 'LoadImage':
                        node['inputs']['image'] = await self._upload_image(Path(node['inputs']['image']))
                if graph:
                    socket, observer = await self._open_progress(graph)
                prompt_id = await self._queue_prompt(workflow)
                if graph:
                    graph.queued(prompt_id)
                logger.info('Local image job queued: %s', prompt_id)
                started = time.monotonic()
                deadline = started + 20 * 60
                while time.monotonic() < deadline:
                    if graph and graph.error:
                        raise graph.error
                    try:
                        history = await self._get_history(prompt_id)
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
                                raise RuntimeError('Image operation failed: ' + details.get('exception_message', kind))
                        if status.get('status_str') == 'error':
                            raise RuntimeError('The GPU could not complete this image operation.')
                        if item.get('outputs'):
                            if run:
                                run.update('saving')
                            return await self._fetch_image(item['outputs'])
                        if status.get('completed'):
                            raise RuntimeError('The image operation finished without an image.')
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
