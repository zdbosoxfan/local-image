"""Setup routes retain the native credential and local-origin boundaries."""
import asyncio
from pathlib import Path
import sys
import types
import unittest
from unittest.mock import AsyncMock, patch

from fastapi import HTTPException
from pydantic import ValidationError
from starlette.requests import Request

HERE = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(HERE / 'backend'), str(HERE / 'tests' / 'helpers')]
from backend_settings_test import BackendSettingsTests
import managed_ai


class SetupRouteTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.fixture = BackendSettingsTests(methodName='runTest')
        self.fixture.setUp()
        self.imports = patch.dict(sys.modules, {'local_remove': self.fixture.app})
        self.imports.start()
        self.routes = types.ModuleType('setup_routes_under_test')
        self.routes.__file__ = str(HERE / 'backend' / 'setup_routes.py')
        exec(compile(Path(self.routes.__file__).read_text(), self.routes.__file__, 'exec'), self.routes.__dict__)
        self.routes.manager = managed_ai.SetupManager()
        self.routes.manager.status = AsyncMock(return_value={'job': None})

    def tearDown(self):
        self.imports.stop()
        self.fixture.tearDown()

    def request(self, native=False, origin='http://127.0.0.1:5000'):
        headers = [(b'host', b'127.0.0.1:5000'), (b'origin', origin.encode()),
                   (b'x-local-remove-token', self.fixture.app.CSRF.encode())]
        if native:
            headers.append((b'x-local-launcher', self.fixture.app.LAUNCHER_KEY.encode()))
        return Request({'type': 'http', 'scheme': 'http', 'method': 'POST', 'path': '/',
                        'query_string': b'', 'server': ('127.0.0.1', 5000), 'headers': headers})

    async def test_every_mutation_rejects_browser_token_and_cross_origin_native_requests(self):
        handlers = (
            (self.routes.setup_configure, self.routes.SetupConfiguration(model_directory=str(self.fixture.directory))),
            (self.routes.setup_install, self.routes.InstallDirectory(directory=str(self.fixture.directory))),
            (self.routes.setup_download_models, self.routes.NoOptions()),
            (self.routes.setup_start, self.routes.NoOptions()),
            (self.routes.setup_eject, self.routes.NoOptions()),
        )
        for handler, payload in handlers:
            for request in (self.request(), self.request(True, 'https://remote.example')):
                with self.assertRaises(HTTPException) as error:
                    await handler(request, payload)
                self.assertEqual(error.exception.status_code, 403)
        self.assertIsNone(self.routes.manager.job)

    async def test_local_status_is_readable_but_remote_origin_is_rejected(self):
        self.assertEqual(await self.routes.setup_status(self.request()), {'job': None})
        with self.assertRaises(HTTPException) as error:
            await self.routes.setup_status(self.request(origin='https://remote.example'))
        self.assertEqual(error.exception.status_code, 403)

    async def test_busy_generation_rejects_setup_without_launching_job(self):
        await self.fixture.main.generation_lock.acquire()
        try:
            with self.assertRaises(HTTPException) as error:
                await self.routes.setup_start(self.request(True), self.routes.NoOptions())
            self.assertEqual(error.exception.status_code, 409)
            self.assertIsNone(self.routes.manager.job)
        finally:
            self.fixture.main.generation_lock.release()

    async def test_native_job_holds_generation_lock_until_complete_and_blocks_duplicates(self):
        wait = asyncio.Event()
        async def start():
            await wait.wait()
        self.routes.manager.start = start
        await self.routes.setup_start(self.request(True), self.routes.NoOptions())
        self.assertTrue(self.fixture.main.generation_lock.locked())
        with self.assertRaises(HTTPException) as error:
            await self.routes.setup_eject(self.request(True), self.routes.NoOptions())
        self.assertEqual(error.exception.status_code, 409)
        wait.set()
        await self.routes.manager.task
        self.assertFalse(self.fixture.main.generation_lock.locked())
        self.assertEqual(self.routes.manager.job['status'], 'complete')

    async def test_native_configuration_updates_port_and_rejects_forged_candidate(self):
        await self.routes.setup_configure(self.request(True), self.routes.SetupConfiguration(
            model_directory=str(self.fixture.directory / 'models'), comfy_port=8189))
        self.assertEqual(self.fixture.engine.config.COMFY_PORT, 8189)
        with self.assertRaises(HTTPException) as error:
            await self.routes.setup_configure(self.request(True), self.routes.SetupConfiguration(installation_id='forged'))
        self.assertEqual(error.exception.status_code, 400)
        self.assertFalse(self.fixture.main.generation_lock.locked())

    def test_payloads_reject_extra_commands_and_paths(self):
        for model, payload in ((self.routes.NoOptions, {'command': 'arbitrary.exe'}),
                               (self.routes.SetupConfiguration, {'comfy_python': 'arbitrary.exe'}),
                               (self.routes.InstallDirectory, {'directory': str(self.fixture.directory), 'url': 'https://remote.example'})):
            with self.assertRaises(ValidationError):
                model(**payload)


if __name__ == '__main__':
    # Keep imported fixture classes out of this module's discovery.
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(SetupRouteTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    raise SystemExit(not result.wasSuccessful())
