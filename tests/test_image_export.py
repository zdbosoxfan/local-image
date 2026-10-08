"""Actual single-image backgrounds, scaled exports and generated composition layers."""
import base64
import copy
import io
from pathlib import Path
import sys
import unittest

import numpy as np
from fastapi import HTTPException
from PIL import Image
from pydantic import ValidationError
import tifffile

sys.path.insert(0, str(Path(__file__).parent / 'helpers'))
import backend_folder_save_test as fixtures


class ImageExportTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.fixture = fixtures.FolderSaveTests(); self.fixture.setUp()
        self.app = self.fixture.app; self.request = self.fixture.request

    def tearDown(self): self.fixture.tearDown()

    async def cutout(self, native=False):
        if native:
            source = self.fixture.images / 'native.tif'
            tifffile.imwrite(source, np.full((32, 40, 3), (12001, 32002, 64003), np.uint16), photometric='rgb', metadata=None)
        else: source = self.fixture.make_image()
        data = self.fixture.bind(source)
        data = await self.app.enable_stack(data['id'], self.request(), self.app.MergeRequest(revision=0))
        mask = Image.new('L', (40, 32)); mask.paste(255, (10, 8, 28, 25))
        buffer = io.BytesIO(); mask.save(buffer, format='PNG')
        data = await self.app.refine_cutout(data['id'], self.request(), self.app.CutoutRefine(revision=0, mask=base64.b64encode(buffer.getvalue()).decode(), operation='replace'))
        return await self.app.update_stack_layer(data['id'], 'original', self.request(), self.app.StackUpdate(revision=data['revision'], visible=False))

    async def test_white_background_is_below_cutout_preserves_mask_and_native_precision_and_undo(self):
        for native in (False, True):
            before = await self.cutout(native)
            result = await self.app.add_white_background(before['id'], self.request(), self.app.WhiteBackground(revision=before['revision'], layer_id=before['selected_layer_id']))
            layers = result['layer_stack']; self.assertEqual([layer['kind'] for layer in layers], ['original', 'image', 'cutout'])
            self.assertEqual(layers[1]['name'], 'White background'); self.assertEqual(layers[2], before['layer_stack'][1])
            pixels, _ = self.app.native_pixels(self.app.read_session(result['id']), self.app.folder(result['id']))
            maximum = 65535 if native else 255
            self.assertEqual(pixels[0, 0].tolist(), [maximum] * 4)
            self.assertEqual(pixels[12, 14].tolist(), ([12001, 32002, 64003] if native else [40, 60, 80]) + [maximum])
            undone = await self.app.undo_stack(result['id'], self.request(), self.app.MergeRequest(revision=result['revision']))
            self.assertEqual(undone['layer_stack'], before['layer_stack'])
            redone = await self.app.redo_stack(result['id'], self.request(), self.app.MergeRequest(revision=undone['revision']))
            self.assertEqual(redone['layer_stack'], result['layer_stack'])

    async def test_legacy_cutout_also_accepts_plain_white(self):
        data = self.fixture.bind(self.fixture.make_image())
        mask = Image.new('L', (40, 32)); mask.paste(255, (10, 8, 28, 25))
        buffer = io.BytesIO(); mask.save(buffer, format='PNG')
        data = await self.app.refine_cutout(data['id'], self.request(), self.app.CutoutRefine(revision=0, mask=base64.b64encode(buffer.getvalue()).decode(), operation='replace'))
        result = await self.app.add_white_background(data['id'], self.request(), self.app.WhiteBackground(revision=data['revision']))
        self.assertEqual(self.app.render(self.app.read_session(result['id'])).getpixel((0, 0)), (255, 255, 255, 255))

    async def test_browser_export_uses_chosen_name_and_size_without_resizing_document(self):
        data = await self.cutout()
        result = await self.app.save(data['id'], self.request(), self.app.SaveRequest(revision=data['revision'], format='png', filename='Catalog.png', width=20, height=16))
        self.assertEqual(result['name'], 'Catalog.png')
        download = await self.app.download(data['id'], self.request())
        self.assertIn('Catalog.png', download.headers['content-disposition'])
        with Image.open(download.path) as output:
            self.assertEqual(output.size, (20, 16)); self.assertEqual(output.getpixel((0, 0))[3], 0)
        stored = self.app.read_session(data['id']); self.assertEqual((stored['width'], stored['height'], stored['revision']), (40, 32, data['revision']))

    async def test_native_export_has_real_destination_no_overwrite_and_revision_guard(self):
        data = await self.cutout(True)
        values = dict(path=str(self.fixture.images), revision=data['revision'], format='tif', filename='Product.tif', width=20, height=16)
        with self.assertRaises(HTTPException) as failure:
            await self.app.export_image_folder(data['id'], self.request(), self.app.FolderImageExport(**values))
        self.assertEqual(failure.exception.status_code, 403)
        result = await self.app.export_image_folder(data['id'], self.request(native=True), self.app.FolderImageExport(**values))
        self.assertIsNone(result['download']); pixels = tifffile.imread(self.fixture.images / 'Product.tif')
        self.assertEqual(pixels.dtype, np.uint16); self.assertEqual(pixels.shape, (16, 20, 4))
        before = (self.fixture.images / 'Product.tif').read_bytes()
        with self.assertRaises(HTTPException) as failure:
            await self.app.export_image_folder(data['id'], self.request(native=True), self.app.FolderImageExport(**values))
        self.assertEqual(failure.exception.status_code, 409); self.assertEqual((self.fixture.images / 'Product.tif').read_bytes(), before)
        values.update(filename='Stale.tif', revision=0)
        with self.assertRaises(HTTPException) as failure:
            await self.app.export_image_folder(data['id'], self.request(native=True), self.app.FolderImageExport(**values))
        self.assertEqual(failure.exception.status_code, 409); self.assertFalse((self.fixture.images / 'Stale.tif').exists())

    def test_invalid_names_and_sizes_cannot_escape_folder_or_resize_source(self):
        for change in ({'filename':'../outside.png'}, {'filename':'CON.png'}, {'filename':'name.jpg'}, {'width':0}, {'width':32769}, {'width':16000,'height':16000}, {'height':None}, {'mode':'overwrite'}):
            with self.subTest(change=change), self.assertRaises(ValidationError):
                self.app.SaveRequest(**dict(dict(revision=0,format='png',filename='Valid.png',width=40,height=32), **change))

    async def test_generated_result_is_new_top_layer_and_existing_composition_survives(self):
        data = await self.cutout(); before = copy.deepcopy(data['layer_stack'])
        source = self.fixture.bind(self.fixture.make_image('Generated.png', (120, 180, 90), (80, 64)))
        result = await self.app.use_generated_layer(data['id'], self.request(), self.app.GeneratedBackground(revision=data['revision'], generated_session_id=source['id']))
        self.assertEqual(result['id'], data['id']); self.assertEqual(result['layer_stack'][:-1], before)
        layer = result['layer_stack'][-1]; self.assertEqual(layer['generated_session_id'], source['id']); self.assertEqual(layer['id'], result['selected_layer_id'])
        self.assertEqual((result['width'],result['height']), (40,32))
        pixels, _ = self.app.native_pixels(self.app.read_session(data['id']), self.app.folder(data['id']))
        self.assertEqual(pixels[15,20].tolist(), [120,180,90,255])
        duplicate = await self.app.use_generated_layer(data['id'], self.request(), self.app.GeneratedBackground(revision=result['revision'], generated_session_id=source['id']))
        self.assertEqual(duplicate['revision'], result['revision']); self.assertEqual(duplicate['layer_stack'], result['layer_stack'])
        project = self.app.folder(data['id']) / 'composition.lremove'
        self.app.write_project(self.app.folder(data['id']), self.app.read_session(data['id']), project)
        reopened = self.app.import_project(project)['session']
        self.assertEqual(reopened['layer_stack'][-1]['generated_session_id'], source['id'])
        self.assertEqual(reopened['layer_stack'][-1]['source'], layer['source'])
        undone = await self.app.undo_stack(data['id'], self.request(), self.app.MergeRequest(revision=result['revision']))
        self.assertEqual(undone['layer_stack'], before)


if __name__ == '__main__': unittest.main()
