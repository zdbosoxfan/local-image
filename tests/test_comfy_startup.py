"""Check startup with real local HTTP responses, never the user's ComfyUI."""
import asyncio
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
_temporary = tempfile.TemporaryDirectory(prefix='local-image-startup-')
_environment = patch.dict(os.environ, {'LOCAL_IMAGE_DATA_DIR': str(Path(_temporary.name) / 'profile')})
_environment.start()
from engine import ComfyClient, config

# The profile override is needed while the backend modules import, but must not
# leak into other test modules: unittest discover imports every module first.
_environment.stop()


def setUpModule():
    _environment.start()


def tearDownModule():
    _environment.stop()
    _temporary.cleanup()


class ComfyStartupTests(unittest.IsolatedAsyncioTestCase):
    async def probe(self, answer):
        async def handler(reader, writer):
            try:
                await reader.readuntil(b'\r\n\r\n')
                if answer:
                    writer.write(b'HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n')
                    await writer.drain()
                else:
                    await reader.read()  # Wait for the bounded client to disconnect.
            finally:
                writer.close()
                await writer.wait_closed()
        server = await asyncio.start_server(handler, '127.0.0.1', 0)
        port = server.sockets[0].getsockname()[1]
        try:
            with patch.object(config, 'COMFY_PORT', port), patch.object(config, 'COMFY_HOST', '127.0.0.1'):
                return await asyncio.wait_for(ComfyClient.check_health(), timeout=5)
        finally:
            server.close()
            await server.wait_closed()

    async def test_responsive_ai_is_detected(self):
        self.assertTrue(await self.probe(answer=True))

    async def test_unresponsive_ai_does_not_hold_first_launch(self):
        started = time.monotonic()
        self.assertFalse(await self.probe(answer=False))
        self.assertLess(time.monotonic() - started, 4.8)


if __name__ == '__main__':
    unittest.main()
