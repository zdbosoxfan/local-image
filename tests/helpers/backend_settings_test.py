"""Isolated standalone-model routing tests; no installed data or GPU jobs are touched."""
import asyncio
import base64
import copy
import io
import os
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import AsyncMock, patch

import numpy as np
from fastapi import HTTPException
from PIL import Image
from pydantic import ValidationError
from starlette.requests import Request


INSTALLED = Path(__file__).resolve().parents[2] / 'backend'
SOURCE = INSTALLED / 'local_remove.py'
sys.path.insert(0, str(INSTALLED))


def png64(image):
    stream = io.BytesIO()
    image.save(stream, format='PNG')
    return base64.b64encode(stream.getvalue()).decode('ascii')


class BackendSettingsTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='local-remove-settings-test-')
        self.directory = Path(self.temporary.name)
        self.data_patch = patch.dict(os.environ, {'LOCAL_REMOVE_DATA_DIR': str(self.directory)})
        self.data_patch.start()
        self.options = [
            {'id': 'klein', 'label': 'FLUX.2 Klein 4B Object Removal',
             'description': 'Current removal model', 'available': True},
        ]
        self.engine = types.ModuleType('engine')
        self.engine.config = types.SimpleNamespace(http_url='http://127.0.0.1:1')
        self.engine.run_inpaint = AsyncMock(side_effect=AssertionError('RapidRAW route was called'))
        self.models = types.ModuleType('local_removal_models')
        self.models.model_options = lambda: copy.deepcopy(self.options)
        self.models.run_local_removal = AsyncMock(return_value={
            'x': 4, 'y': 4,
            'color': png64(Image.new('RGB', (4, 4), (200, 210, 220))),
            'mask': png64(Image.new('L', (4, 4), 255)),
        })
        self.main = types.ModuleType('main')
        self.main.generation_lock = asyncio.Lock()
        self.modules_patch = patch.dict(sys.modules, {
            'engine': self.engine, 'local_removal_models': self.models, 'main': self.main,
        })
        self.modules_patch.start()
        # Use the real local-origin guard, with the engine dependency isolated.
        quality = types.ModuleType('quality_settings')
        quality.__file__ = str(INSTALLED / 'quality_settings.py')
        sys.modules['quality_settings'] = quality
        exec(compile(Path(quality.__file__).read_text(encoding='utf-8'), quality.__file__, 'exec'), quality.__dict__)
        self.app = self.load_backend('backend_under_test')

    def tearDown(self):
        self.modules_patch.stop()
        self.data_patch.stop()
        self.temporary.cleanup()

    def load_backend(self, name):
        module = types.ModuleType(name)
        # __file__ controls ROOT; this guarantees import-time files stay in the isolated fixture.
        module.__file__ = str(self.directory / 'local_remove.py')
        sys.modules[name] = module
        exec(compile(SOURCE.read_text(encoding='utf-8'), str(SOURCE), 'exec'), module.__dict__)
        return module

    def request(self, *, token=True, origin='http://127.0.0.1:5000'):
        headers = [(b'host', b'127.0.0.1:5000'), (b'origin', origin.encode('ascii'))]
        if token:
            headers.append((b'x-local-remove-token', self.app.CSRF.encode('ascii')))
        return Request({'type': 'http', 'scheme': 'http', 'method': 'PATCH',
                        'path': '/', 'query_string': b'', 'server': ('127.0.0.1', 5000),
                        'headers': headers})

    def session(self):
        source = self.directory / 'source.png'
        Image.new('RGB', (12, 12), (20, 30, 40)).save(source)
        return self.app.create_session(source, source.name)

    def remove_payload(self, model=None):
        mask = Image.new('L', (12, 12), 0)
        mask.paste(255, (4, 4, 8, 8))
        return self.app.RemoveRequest(mask=png64(mask), revision=0, model=model)

    async def test_default_and_atomic_persistence_survive_reload(self):
        initial = await self.app.get_settings(self.request())
        self.assertEqual(initial['model'], 'klein')
        self.assertEqual([x['id'] for x in initial['models']], ['klein', 'heal'])
        result = await self.app.update_settings(self.request(), self.app.RemovalSettings(model='klein'))
        self.assertEqual(result['model'], 'klein')
        reloaded = self.load_backend('backend_reloaded')
        self.assertEqual(reloaded.read_settings(), {'model': 'klein'})
        self.assertEqual(list(self.app.ROOT.glob('settings-*.tmp')), [])

    async def test_old_qwen_setting_migrates_to_flux(self):
        (self.app.ROOT / 'settings.json').write_text('{"model":"qwen"}', encoding='utf-8')
        self.assertEqual(self.app.read_settings(), {'model': 'klein'})
        with self.assertRaises(ValidationError):
            self.app.RemovalSettings(model='qwen')
        # Per-request Qwen removal is a supported choice in the Retouch tools.
        self.assertEqual(self.app.RemoveRequest(mask='unused', revision=0, model='qwen').model, 'qwen')

    async def test_unknown_models_are_rejected(self):
        with self.assertRaises(ValidationError):
            self.app.RemovalSettings(model='untrusted-workflow')
        with self.assertRaises(ValidationError):
            self.app.RemoveRequest(mask='unused', revision=0, model='untrusted-workflow')

    async def test_settings_writes_require_csrf_and_local_origin(self):
        payload = self.app.RemovalSettings(model='klein')
        for request in (self.request(token=False), self.request(origin='https://outside.example')):
            with self.assertRaises(HTTPException) as rejected:
                await self.app.update_settings(request, payload)
            self.assertEqual(rejected.exception.status_code, 403)
        self.assertEqual(self.app.read_settings()['model'], 'klein')

    async def test_unavailable_models_do_not_overwrite_settings(self):
        self.options[0].update(available=False, reason='FLUX model files are still downloading.')
        with self.assertRaises(HTTPException) as rejected:
            await self.app.update_settings(self.request(), self.app.RemovalSettings(model='klein'))
        self.assertEqual(rejected.exception.status_code, 409)
        self.assertIn('downloading', rejected.exception.detail)
        self.assertEqual(self.app.read_settings()['model'], 'klein')
        session = self.session()
        with self.assertRaises(HTTPException) as removal:
            await self.app.remove(session['id'], self.request(), self.remove_payload('klein'))
        self.assertEqual(removal.exception.status_code, 409)
        self.models.run_local_removal.assert_not_called()

    async def test_backend_down_preserves_flux_choice_and_reason(self):
        await self.app.update_settings(self.request(), self.app.RemovalSettings(model='klein'))
        self.options[0].update(available=False, reason='FLUX model files are missing.')
        before = (self.app.ROOT / 'settings.json').read_bytes()
        with patch.object(self.app.aiohttp, 'ClientSession', side_effect=OSError('offline')):
            result = await self.app.status(self.request())
        self.assertFalse(result['ready'])
        self.assertEqual(result['model_id'], 'klein')
        self.assertEqual(result['model'], 'FLUX.2 Klein 4B Object Removal')
        self.assertIn('missing', result['reason'])
        self.assertEqual((self.app.ROOT / 'settings.json').read_bytes(), before)
        self.assertEqual((await self.app.get_settings(self.request()))['model'], 'klein')

    async def test_explicit_model_routes_standalone_and_preserves_layers(self):
        session = self.session()
        result = await self.app.remove(session['id'], self.request(), self.remove_payload('klein'))
        self.assertEqual(self.models.run_local_removal.await_args.args[3], 'klein')
        self.engine.run_inpaint.assert_not_called()
        self.assertEqual(self.app.read_settings()['model'], 'klein')
        layer = result['layers'][0]
        self.assertEqual((layer['model'], layer['model_label']), ('klein', 'FLUX.2 Klein 4B Object Removal'))
        self.assertEqual(result['revision'], 1)
        rendered = np.asarray(self.app.render(result))
        np.testing.assert_array_equal(rendered[0, 0], (20, 30, 40))
        np.testing.assert_array_equal(rendered[5, 5], (200, 210, 220))
        hidden = await self.app.update_layer(session['id'], layer['id'], self.request(),
                                             self.app.LayerUpdate(visible=False))
        np.testing.assert_array_equal(np.asarray(self.app.render(hidden)),
                                      np.full((12, 12, 3), (20, 30, 40), dtype=np.uint8))

    async def test_queued_request_uses_saved_flux_model(self):
        session = self.session()
        await self.main.generation_lock.acquire()
        task = asyncio.create_task(self.app.remove(session['id'], self.request(), self.remove_payload()))
        await asyncio.sleep(0)
        self.assertFalse(task.done())
        await self.app.update_settings(self.request(), self.app.RemovalSettings(model='klein'))
        self.main.generation_lock.release()
        result = await task
        self.assertEqual(self.models.run_local_removal.await_args.args[3], 'klein')
        self.assertEqual(result['layers'][0]['model'], 'klein')
        self.assertEqual(self.app.read_settings()['model'], 'klein')

    async def test_flux_route_keeps_tiff_precision_profile_and_original(self):
        source = self.directory / 'source16.tif'
        raw = (np.arange(12 * 12 * 3, dtype=np.uint16).reshape(12, 12, 3) * 101 + 3)
        profile = self.app.SRGB.tobytes()
        self.app.tifffile.imwrite(source, raw, photometric='rgb', metadata=None,
                                  extratags=[(34675, 'B', len(profile), profile, False)])
        original_bytes = source.read_bytes()
        session = self.app.create_session(source, source.name)
        result = await self.app.remove(session['id'], self.request(), self.remove_payload('klein'))
        flattened = self.directory / 'flattened.tif'
        self.app.flatten(result, flattened)
        with self.app.tifffile.TiffFile(flattened) as image:
            flat = image.asarray()
            self.assertEqual(image.pages[0].tags['InterColorProfile'].value, profile)
        self.assertEqual(flat.dtype, np.uint16)
        outside = np.ones((12, 12), dtype=bool)
        outside[4:8, 4:8] = False
        np.testing.assert_array_equal(flat[outside], raw[outside])
        self.assertEqual(source.read_bytes(), original_bytes)
        result['layers'][0]['visible'] = False
        self.app.flatten(result, flattened)
        np.testing.assert_array_equal(self.app.tifffile.imread(flattened), raw)

    async def test_full_previews_keep_native_dimensions_without_cache_collisions(self):
        source = self.directory / 'wide-photo.png'
        Image.new('RGB', (4000, 100), (20, 30, 40)).save(source)
        session = self.app.create_session(source, source.name)
        paths = []
        for original in (False, True):
            for full in (False, True):
                response = await self.app.preview(session['id'], self.request(), original=original, full=full)
                paths.append(Path(response.path))
                with Image.open(response.path) as image:
                    self.assertEqual(image.size, (4000, 100) if full else (3000, 75))
                self.assertEqual(Path(response.path).name.startswith('full-'), full)
        self.assertEqual(len(set(paths)), 4)
        self.assertEqual(self.app.read_session(session['id'])['revision'], 0)


if __name__ == '__main__':
    unittest.main(verbosity=2)
