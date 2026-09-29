"""Stock imports retain editable pixels, revisions, undo and portable credits."""
import asyncio
import base64
import io
import json
from pathlib import Path
import sys
import types
import unittest
from unittest.mock import AsyncMock, patch
import zipfile

from fastapi import HTTPException
import numpy as np
from PIL import Image

BACKEND = Path(__file__).resolve().parents[1] / 'backend'
sys.path[:0] = [str(BACKEND), str(Path(__file__).resolve().parent / 'helpers')]
import backend_folder_save_test as fixtures
fixtures.V3 = BACKEND
import qwen_image
from stock_attribution import validate_attribution, collect_attributions


def credit(asset='photo-1'):
    return {'provider': 'openverse', 'asset_id': asset, 'title': 'Beach at sunset', 'creator': 'Example Photographer',
            'creator_url': 'https://www.flickr.com/photos/example/',
            'source_url': 'https://www.flickr.com/photos/example/' + asset + '/',
            'license': 'CC BY 4.0', 'license_url': 'https://creativecommons.org/licenses/by/4.0/',
            'attribution': 'Beach at sunset by Example Photographer, CC BY 4.0.'}


def png(image):
    stream = io.BytesIO(); image.save(stream, format='PNG'); return stream.getvalue()


class StockIntegrationTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.fixture = fixtures.FolderSaveTests(); self.fixture.setUp(); self.editor = self.fixture.app

    def tearDown(self):
        self.fixture.tearDown()

    def project_roundtrip(self, data, name='stock.lremove'):
        path = self.fixture.images / name
        self.editor.write_project(self.editor.folder(data['id']), data, path)
        return self.editor.import_project(path)['session']

    async def test_image_import_keeps_rgba_credit_and_unsaved_project_state(self):
        image = Image.new('RGBA', (256, 256), (80, 120, 180, 255)); image.putpixel((0, 0), (80, 120, 180, 0))
        attribution = credit(); attribution['title'] = 'File: beach / sunset?.png'
        result = await self.editor.import_stock_image(png(image), attribution)
        data = result['session']
        self.assertEqual(result['target'], 'image'); self.assertEqual(data['source_attribution'], attribution)
        self.assertTrue(data['dirty']); self.assertFalse(data['can_return']); self.assertEqual(data['revision'], 1)
        self.assertNotIn('/', data['name']); self.assertNotIn(':', data['name'])
        with Image.open(self.editor.folder(data['id']) / data['original']) as original:
            np.testing.assert_array_equal(np.asarray(original), np.asarray(image))
        restored = self.project_roundtrip(data)
        self.assertEqual(restored['source_attribution'], attribution)
        self.assertFalse(list(self.editor.ROOT.glob('stock-import-*')))

    async def test_background_import_preserves_subject_transform_source_and_undo(self):
        source = self.fixture.make_image(size=(64, 48)); original = source.read_bytes()
        session = self.fixture.bind(source)
        selection = Image.new('L', (64, 48)); selection.paste(255, (16, 10, 40, 40))
        data = await self.editor.refine_cutout(session['id'], self.fixture.request(), self.editor.CutoutRefine(
            revision=0, mask=base64.b64encode(png(selection)).decode(), operation='replace'))
        data = await self.editor.update_cutout(data['id'], self.fixture.request(), self.editor.CutoutUpdate(
            revision=data['revision'], transform={'offset_x': 6, 'offset_y': -2, 'scale': .8, 'rotation': 12},
            shadow={'enabled': True, 'opacity': .45}))
        before = json.loads(json.dumps(data['cutout'])); layer_count = len(data['layers'])
        result = await self.editor.import_stock_image(png(Image.new('RGB', (96, 64), 'blue')), credit(),
                    target='background', session_id=data['id'], revision=data['revision'])
        after = result['session']
        self.assertEqual(after['id'], data['id']); self.assertEqual(after['revision'], data['revision'] + 1)
        for key in ('alpha', 'transform', 'shadow', 'feather'):
            self.assertEqual(after['cutout'][key], before[key])
        self.assertEqual(after['cutout']['background']['attribution'], credit())
        self.assertEqual(len(after['layers']), layer_count); self.assertEqual(source.read_bytes(), original)
        restored = self.project_roundtrip(after)
        self.assertEqual(restored['cutout']['background']['attribution'], credit())
        undone = await self.editor.undo_cutout(after['id'], self.fixture.request(), self.editor.MergeRequest(revision=after['revision']))
        self.assertEqual(undone['cutout'], before)
        redone = await self.editor.redo_cutout(after['id'], self.fixture.request(), self.editor.MergeRequest(revision=undone['revision']))
        self.assertEqual(redone['cutout']['background']['attribution'], credit())

    async def test_initial_background_import_respects_original_transparency(self):
        source = self.fixture.images / 'transparent.png'
        original = Image.new('RGBA', (32, 32), (230, 80, 20, 0)); original.paste((230, 80, 20, 255), (8, 8, 24, 24)); original.save(source)
        data = self.fixture.bind(source)
        result = await self.editor.import_stock_image(png(Image.new('RGB', (32, 32), 'blue')), credit(),
            target='background', session_id=data['id'], revision=0)
        output = self.editor.render(result['session'])
        self.assertEqual(output.getpixel((0, 0)), (0, 0, 255, 255))
        self.assertEqual(output.getpixel((16, 16)), (230, 80, 20, 255))
        self.assertTrue(result['session']['cutout_can_undo'])

    async def test_stale_background_response_never_replaces_newer_edit_or_creates_document(self):
        session = self.fixture.bind(self.fixture.make_image(size=(64, 48)))
        data = self.fixture.layer(session['id'])
        before = (self.editor.folder(data['id']) / 'session.json').read_bytes()
        count = len(list(self.editor.SESSIONS.iterdir()))
        with self.assertRaises(HTTPException) as failed:
            await self.editor.import_stock_image(png(Image.new('RGB', (32, 32), 'blue')), credit(),
                target='background', session_id=data['id'], revision=0)
        self.assertEqual(failed.exception.status_code, 409)
        self.assertEqual((self.editor.folder(data['id']) / 'session.json').read_bytes(), before)
        self.assertEqual(len(list(self.editor.SESSIONS.iterdir())), count)

    async def test_invalid_bytes_or_attribution_never_create_a_document(self):
        for contents, attribution in [(b'not a png', credit()), (png(Image.new('RGB', (16, 16))), {**credit(), 'source_url': 'javascript:alert(1)'}),
                                      (png(Image.new('RGB', (16, 16))), {**credit(), 'path': 'C:/private.png'})]:
            with self.assertRaises((ValueError, OSError)):
                await self.editor.import_stock_image(contents, attribution)
        self.assertEqual(list(self.editor.SESSIONS.iterdir()), [])

    async def test_stock_reference_credits_follow_generation_then_background_and_project(self):
        stock = (await self.editor.import_stock_image(png(Image.new('RGB', (256, 256), 'yellow')), credit()))['session']
        api = types.ModuleType('stock_generation_under_test'); api.__file__ = str(BACKEND / 'image_generation.py')
        sys.modules[api.__name__] = api
        try:
            with patch.dict(sys.modules, {'local_remove': self.editor}):
                exec(compile(Path(api.__file__).read_text(encoding='utf-8'), api.__file__, 'exec'), api.__dict__)
            with patch.object(qwen_image, 'run_qwen_image', AsyncMock(return_value=Image.new('RGB', (256, 256), 'green'))), \
                    patch.dict(sys.modules, {'generation_library': types.SimpleNamespace(add_generated=lambda image, session: session['id'])}):
                generated = (await api.generate_image(self.fixture.request(), api.GenerationRequest(prompt='An empty studio',
                    width=256, height=256, reference_session_ids=[stock['id']])) )['session']
            self.assertEqual(generated['reference_attributions'], [credit()])
            self.assertEqual(self.project_roundtrip(generated, 'generated-credit.lremove')['reference_attributions'], [credit()])
            target = await self.editor.import_stock_image(png(Image.new('RGB', (64, 48), 'blue')), credit('background-2'),
                target='background', session_id=stock['id'], revision=stock['revision'])
            data = await self.editor.use_generated_background(stock['id'], self.fixture.request(), self.editor.GeneratedBackground(
                revision=target['session']['revision'], generated_session_id=generated['id']))
            self.assertEqual(data['cutout']['background']['reference_attributions'], [credit()])
            self.assertNotIn('attribution', data['cutout']['background'])
            self.assertEqual(collect_attributions(data), [credit()])
            self.assertEqual(self.project_roundtrip(data, 'background-credit.lremove')['cutout']['background']['reference_attributions'], [credit()])
        finally:
            sys.modules.pop(api.__name__, None)

    async def test_project_rejects_attribution_with_filesystem_or_script_authority(self):
        data = (await self.editor.import_stock_image(png(Image.new('RGB', (32, 32), 'red')), credit()))['session']
        data['source_attribution']['license_url'] = 'file:///C:/secret.txt'
        with self.assertRaises(ValueError):
            self.project_roundtrip(data)
        for values in ({**credit(), 'creator_url': 'https://user:secret@example.com/credit'},
                       {**credit(), 'provider': 'untrusted'}, {**credit(), 'attribution': 'x' * 8001}):
            with self.assertRaises(ValueError):
                validate_attribution(values)


if __name__ == '__main__':
    unittest.main()
