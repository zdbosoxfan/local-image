"""Upscale snapshots, editable provenance, library storage and strict limits."""
import io
import json
from pathlib import Path
import sys
import types
import unittest
from unittest.mock import AsyncMock, patch

from fastapi import HTTPException
from PIL import Image
from pydantic import ValidationError

BACKEND = Path(__file__).resolve().parents[1] / 'backend'
sys.path[:0] = [str(BACKEND), str(Path(__file__).resolve().parent / 'helpers')]
import backend_folder_save_test as fixtures
fixtures.V3 = BACKEND
from test_generation_library import parameters
from upscale_metadata import validate_upscale_metadata, validate_upscale_size


class UpscaleTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.fixture = fixtures.FolderSaveTests(); self.fixture.setUp(); self.editor = self.fixture.app
        self.api = self.load('image_upscale'); self.api.ENABLE_UPSCALE = True
        self.library = self.load('generation_library')
        self.adapter = types.SimpleNamespace(run_seedvr2_image=AsyncMock(return_value=Image.new('RGB', (512, 512), 'green')),
            seedvr2_model_option=lambda info: {'id': 'fp16', 'available': bool(info), 'reason': '' if info else 'Missing model'})
        self.module_patch = patch.dict(sys.modules, {'seedvr2_image': self.adapter, 'generation_library': self.library})
        self.module_patch.start()

    def load(self, filename):
        module = types.ModuleType(filename + '_upscale_test'); module.__file__ = str(BACKEND / (filename + '.py'))
        sys.modules[module.__name__] = module; self.addCleanup(sys.modules.pop, module.__name__, None)
        with patch.dict(sys.modules, {'local_remove': self.editor}):
            exec(compile(Path(module.__file__).read_text(encoding='utf-8'), module.__file__, 'exec'), module.__dict__)
        return module

    def tearDown(self):
        self.module_patch.stop(); self.fixture.tearDown()

    def source(self, generated=False):
        source = self.fixture.images / 'source.png'
        image = Image.new('RGBA', (256, 256), (200, 50, 20, 0)); image.paste((200, 50, 20, 255), (50, 50, 200, 200)); image.save(source)
        data = self.fixture.bind(source)
        if generated:
            data = self.editor.read_session(data['id']); data['generation'] = parameters()
            self.editor.write_session(self.editor.folder(data['id']), data)
        return source, data, image

    def request(self, data, **kwargs):
        return self.api.UpscaleRequest(session_id=data['id'], revision=data['revision'], width=512, height=512, **kwargs)

    async def test_capability_remains_withheld_until_quality_flag_is_enabled(self):
        self.api.ENABLE_UPSCALE = False
        with patch.object(self.api.qwen_image, '_object_info', AsyncMock(return_value={'ready': {}})):
            status = await self.api.upscale_models(self.fixture.request())
        self.assertFalse(status['enabled']); self.assertTrue(status['model']['available'])
        self.assertIsNone(status['limits']['max_pixels'])
        self.assertIsNone(status['limits']['max_dimension'])
        self.assertEqual(status['limits']['dimension_step'], 2)
        _, data, _ = self.source()
        with self.assertRaises(HTTPException) as error:
            await self.api.upscale_image(self.fixture.request(), self.request(data))
        self.assertEqual(error.exception.status_code, 409); self.adapter.run_seedvr2_image.assert_not_called()

    async def test_seed_zero_preserves_alpha_source_and_original_generation_resolution_in_project_and_library(self):
        source, data, image = self.source(generated=True)
        original = source.read_bytes(); session_before = (self.editor.folder(data['id']) / 'session.json').read_bytes()
        result = await self.api.upscale_image(self.fixture.request(), self.request(data, seed=0))
        output = result['session']
        self.assertNotEqual(output['id'], data['id']); self.assertTrue(output['dirty']); self.assertFalse(output['can_return'])
        self.assertEqual(output['generation']['width'], 256)
        self.assertEqual((output['width'], output['height']), (512, 512)); self.assertEqual(output['upscale']['seed'], 0)
        self.assertEqual(self.adapter.run_seedvr2_image.await_args.kwargs, {'size': (512, 512), 'seed': 0})
        with Image.open(self.editor.folder(output['id']) / output['original']) as restored:
            self.assertEqual(restored.mode, 'RGBA')
            self.assertEqual(restored.getchannel('A').tobytes(), image.getchannel('A').resize((512, 512), Image.Resampling.LANCZOS).tobytes())
        self.assertEqual(source.read_bytes(), original)
        self.assertEqual((self.editor.folder(data['id']) / 'session.json').read_bytes(), session_before)
        project = self.fixture.images / 'upscaled.lremove'
        self.editor.write_project(self.editor.folder(output['id']), output, project)
        reopened = self.editor.import_project(project)['session']
        self.assertEqual(reopened['upscale'], output['upscale']); self.assertEqual(reopened['generation'], output['generation'])
        entries = self.library.listing()['items']; item = next(item for item in entries if item['id'] == output['id'])
        self.assertEqual(item['model'], 'seedvr2'); self.assertEqual(item['width'], 512)
        from_library = self.library.open_image(item['id'])['session']
        self.assertEqual(from_library['upscale'], output['upscale']); self.assertEqual(from_library['generation']['width'], 256)

    async def test_imported_image_can_upscale_without_generation_and_keeps_stock_credit(self):
        from test_stock_integration import credit
        _, data, _ = self.source()
        data = self.editor.read_session(data['id']); data['source_attribution'] = credit()
        self.editor.write_session(self.editor.folder(data['id']), data)
        result = (await self.api.upscale_image(self.fixture.request(), self.request(data)))['session']
        self.assertNotIn('generation', result); self.assertEqual(result['source_attribution'], credit())
        item = self.library.listing()['items'][0]
        self.assertIsNone(item['generation']); self.assertEqual(item['reference_attributions'], [credit()])
        reopened = self.library.open_image(item['id'])['session']
        self.assertNotIn('generation', reopened); self.assertEqual(reopened['reference_attributions'], [credit()])
        project = self.fixture.images / 'stock-upscale.lremove'
        self.editor.write_project(self.editor.folder(result['id']), result, project)
        self.assertEqual(self.editor.import_project(project)['session']['upscale'], result['upscale'])

    async def test_stale_revision_busy_gate_and_missing_csrf_never_call_model(self):
        _, data, _ = self.source()
        self.fixture.layer(data['id'])
        with self.assertRaises(HTTPException) as stale:
            await self.api.upscale_image(self.fixture.request(), self.request(data))
        self.assertEqual(stale.exception.status_code, 409)
        with self.assertRaises(HTTPException) as denied:
            await self.api.upscale_image(self.fixture.request(csrf=False), self.request(data))
        self.assertEqual(denied.exception.status_code, 403)
        lock = self.fixture.fixture.main.generation_lock
        await lock.acquire()
        try:
            with self.assertRaises(HTTPException) as busy:
                await self.api.upscale_image(self.fixture.request(), self.request(data))
            self.assertEqual(busy.exception.status_code, 409)
        finally:
            lock.release()
        self.adapter.run_seedvr2_image.assert_not_called()
        self.assertFalse(list(self.editor.ROOT.glob('local-image-upscale-*')))

    async def test_snapshots_current_repair_pixels_and_failure_leaves_source_editable(self):
        _, data, _ = self.source()
        edited = self.fixture.layer(data['id']); before = json.dumps(edited)
        async def failing(path, **kwargs):
            with Image.open(path) as image:
                self.assertEqual(image.getpixel((10, 10))[:3], (200, 210, 220))
            raise RuntimeError('GPU memory exhausted')
        self.adapter.run_seedvr2_image = failing
        with self.assertRaises(HTTPException) as error:
            await self.api.upscale_image(self.fixture.request(), self.request(edited))
        self.assertEqual(error.exception.status_code, 400)
        self.assertIn('GPU memory', error.exception.detail)
        self.assertFalse(self.fixture.fixture.main.generation_lock.locked())
        self.assertEqual(len(list(self.editor.SESSIONS.iterdir())), 1)
        self.assertEqual(json.dumps(self.editor.public(self.editor.read_session(data['id']))), before)

    async def test_invalid_size_aspect_path_and_dimensions_rejected(self):
        _, data, _ = self.source()
        for changes in ({'session_id': '../outside'}, {'width': 1}, {'height': -1}, {'width': 513}, {'seed': True}, {'revision': -1}):
            values = {'session_id': data['id'], 'revision': 0, 'width': 512, 'height': 512, **changes}
            with self.assertRaises(ValidationError):
                self.api.UpscaleRequest(**values)
        for output in ((256, 256), (512, 600)):
            with self.assertRaises(HTTPException) as error:
                await self.api.upscale_image(self.fixture.request(), self.api.UpscaleRequest(session_id=data['id'], revision=0, width=output[0], height=output[1]))
            self.assertEqual(error.exception.status_code, 400)
        validate_upscale_size((333, 250), (1024, 768))
        validate_upscale_size((1536, 864), (7680, 4320))
        with self.assertRaisesRegex(ValueError, 'even output dimensions'):
            validate_upscale_size((256, 256), (513, 513))
        self.adapter.run_seedvr2_image.assert_not_called()

    async def test_wrong_model_output_dimensions_cannot_create_result_or_library_copy(self):
        _, data, _ = self.source()
        self.adapter.run_seedvr2_image = AsyncMock(return_value=Image.new('RGB', (256, 256)))
        with self.assertRaises(HTTPException) as error:
            await self.api.upscale_image(self.fixture.request(), self.request(data))
        self.assertEqual(error.exception.status_code, 400)
        self.assertEqual(len(list(self.editor.SESSIONS.iterdir())), 1)
        self.assertFalse(self.fixture.fixture.main.generation_lock.locked())

    async def test_malformed_portable_upscale_metadata_is_rejected(self):
        value = {'model': 'seedvr2', 'variant': 'fp16', 'source_width': 256, 'source_height': 256, 'width': 512, 'height': 512, 'seed': 0}
        self.assertEqual(validate_upscale_metadata(value), value)
        for changes in ({'width': 0}, {'width': 513}, {'seed': True}, {'variant': 'fp8'}, {'path': 'C:/private.png'}, {'source_width': 1024}):
            with self.assertRaises(ValueError):
                validate_upscale_metadata({**value, **changes})


if __name__ == '__main__':
    unittest.main()
