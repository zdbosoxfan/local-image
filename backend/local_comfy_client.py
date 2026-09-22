"""Comfy client that tolerates long local model-loading and sampling stalls."""
import asyncio
import logging
from pathlib import Path
import time

import aiohttp
from engine import ComfyClient

logger = logging.getLogger('Engine')


class LocalComfyClient(ComfyClient):
    async def execute(self, workflow):
        # Model loading/driver stalls can starve Comfy's websocket pongs. Query
        # job history instead; never resubmit a job after a transient timeout.
        timeout = aiohttp.ClientTimeout(total=120, connect=20)
        async with aiohttp.ClientSession(timeout=timeout) as session:
            self.session = session
            for node in workflow.values():
                if node.get('class_type') == 'LoadImage':
                    node['inputs']['image'] = await self._upload_image(Path(node['inputs']['image']))
            prompt_id = await self._queue_prompt(workflow)
            logger.info('Local removal queued: %s', prompt_id)
            deadline = time.monotonic() + 20 * 60
            while time.monotonic() < deadline:
                try:
                    history = await self._get_history(prompt_id)
                except (aiohttp.ClientConnectionError, asyncio.TimeoutError):
                    await asyncio.sleep(2)
                    continue
                item = history.get(prompt_id)
                if item:
                    status = item.get('status', {})
                    for kind, details in status.get('messages', []):
                        if kind in ('execution_error', 'execution_interrupted'):
                            raise RuntimeError('Removal failed: ' + details.get('exception_message', kind))
                    if status.get('status_str') == 'error':
                        raise RuntimeError('The GPU could not complete this removal.')
                    if item.get('outputs'):
                        return await self._fetch_image(item['outputs'])
                    if status.get('completed'):
                        raise RuntimeError('The removal finished without an image.')
                await asyncio.sleep(2)
            raise TimeoutError('The GPU did not finish this removal within 20 minutes.')
