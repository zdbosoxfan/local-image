"""Cutout pixels, alpha safety, native precision, and portable project integration."""
import base64
import io
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import AsyncMock, patch
import zipfile

import numpy as np
from fastapi import HTTPException, UploadFile
from PIL import Image
import tifffile

HERE = Path(__file__).resolve().parents[1] / 'backend'
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(Path(__file__).resolve().parent / 'helpers'))
import backend_folder_save_test as fixtures
fixtures.V3 = HERE
from cutout_composite import (DEFAULT_SHADOW, DEFAULT_TRANSFORM, compose_native,
                              refine_alpha, inverse_selection, validate_cutout)
from qwen_image import qwen_canvas_size


def png64(image):
    stream = io.BytesIO()
    image.save(stream, format='PNG')
    return base64.b64encode(stream.getvalue()).decode('ascii')


class CutoutPixelTests(unittest.TestCase):
    def test_subject_translation_rotation_and_scale_keep_source_unchanged(self):
        raw = np.full((9, 9, 3), (220, 90, 40), dtype=np.uint8); original = raw.copy()
        alpha = Image.new('L', (9, 9)); alpha.putpixel((2, 4), 255)
        moved = compose_native(raw, alpha, None, DEFAULT_SHADOW, transform={**DEFAULT_TRANSFORM, 'offset_x': 2, 'offset_y': 1})
        self.assertEqual(moved[5, 4].tolist(), [220, 90, 40, 255]); self.assertEqual(moved[4, 2, 3], 0)
        rotated = compose_native(raw, alpha, None, DEFAULT_SHADOW, transform={**DEFAULT_TRANSFORM, 'rotation': 90})
        self.assertEqual(rotated[2, 4].tolist(), [220, 90, 40, 255])
        scaled = compose_native(raw, alpha, None, DEFAULT_SHADOW, transform={**DEFAULT_TRANSFORM, 'scale': 2})
        self.assertEqual(scaled[4, 0, 3], 255)
        np.testing.assert_array_equal(raw, original)

    def test_rotated_soft_edges_use_premultiplied_color_without_dark_halos(self):
        raw = np.zeros((32, 32, 3), dtype=np.uint8); raw[9:23, 9:23] = 255
        alpha = Image.new('L', (32, 32)); alpha.paste(255, (9, 9, 23, 23))
        transform = {**DEFAULT_TRANSFORM, 'scale': 1.2, 'rotation': 23, 'offset_x': 0.35}
        result = compose_native(raw, alpha, None, DEFAULT_SHADOW, transform=transform)
        partial = (result[..., 3] > 0) & (result[..., 3] < 255)
        self.assertTrue(np.any(partial)); self.assertTrue(np.all(result[partial, :3] == 255))
        white = compose_native(raw, alpha, '#ffffff', DEFAULT_SHADOW, transform=transform)
        self.assertTrue(np.all(white == 255))

    def test_shadow_follows_translated_subject_even_when_squashed(self):
        raw = np.full((48, 48, 3), (220, 90, 40), dtype=np.uint8)
        alpha = Image.new('L', (48, 48)); alpha.paste(255, (14, 12, 24, 25))
        shadow = {**DEFAULT_SHADOW, 'enabled': True, 'blur': 1, 'squeeze': 0.4, 'offset_x': 3, 'offset_y': 2}
        before = Image.fromarray(compose_native(raw, alpha, None, shadow))
        after = compose_native(raw, alpha, None, shadow, transform={**DEFAULT_TRANSFORM, 'offset_x': 4, 'offset_y': 3})
        expected = Image.new('RGBA', before.size); expected.paste(before, (4, 3))
        np.testing.assert_array_equal(after, np.asarray(expected))

    def test_inverse_selection_uses_display_coordinates_after_translation_and_rotation(self):
        selection = Image.new('L', (9, 9)); selection.putpixel((4, 2), 255)
        source = inverse_selection(selection, {**DEFAULT_TRANSFORM, 'rotation': 90})
        self.assertEqual(source.getpixel((2, 4)), 255)
        translated = Image.new('L', (9, 9)); translated.putpixel((6, 7), 255)
        source = inverse_selection(translated, {**DEFAULT_TRANSFORM, 'offset_x': 2, 'offset_y': 3})
        self.assertEqual(source.getpixel((4, 4)), 255)
    def test_qwen_work_size_caps_large_and_extreme_aspect_sources(self):
        for source in ((6000, 4000), (8256, 5504), (5000, 3000), (4096, 4096), (12000, 600), (12, 12)):
            width, height = qwen_canvas_size(source)
            self.assertLessEqual(width * height, 4194304)
            self.assertTrue(32 <= width <= 4096 and 32 <= height <= 4096)
            self.assertEqual(width % 32, 0); self.assertEqual(height % 32, 0)
            if min(source) >= 600:
                self.assertLess(abs((width / height) / (source[0] / source[1]) - 1), 0.04)

    def test_refinement_preserves_soft_edges_and_restore_is_not_destructive(self):
        alpha = Image.fromarray(np.array([[0, 128, 255]], dtype=np.uint8))
        selection = Image.fromarray(np.array([[255, 128, 0]], dtype=np.uint8))
        self.assertEqual(np.asarray(refine_alpha(alpha, selection, 'restore')).ravel().tolist(), [255, 192, 255])
        self.assertEqual(np.asarray(refine_alpha(alpha, selection, 'erase')).ravel().tolist(), [0, 64, 255])
        self.assertEqual(np.asarray(refine_alpha(alpha, selection, 'replace')).ravel().tolist(), [255, 128, 0])

    def test_shadow_changes_only_underlay_and_can_be_disabled(self):
        raw = np.full((8, 8, 3), (160, 80, 20), dtype=np.uint8)
        alpha = Image.new('L', (8, 8)); alpha.paste(255, (2, 2, 4, 4))
        shadow = {**DEFAULT_SHADOW, 'enabled': True, 'opacity': 0.5, 'blur': 0, 'offset_x': 2, 'offset_y': 1}
        result = compose_native(raw, alpha, None, shadow)
        self.assertEqual(result[2, 2].tolist(), [160, 80, 20, 255])
        self.assertEqual(result[3, 5].tolist(), [0, 0, 0, 128])
        shadow['enabled'] = False
        self.assertEqual(compose_native(raw, alpha, None, shadow)[3, 5, 3], 0)

    def test_native_sixteen_bit_opaque_foreground_is_exact(self):
        raw = np.array([[[12001, 32002, 64003], [65000, 12345, 42]]], dtype=np.uint16)
        alpha = Image.fromarray(np.array([[255, 0]], dtype=np.uint8))
        transparent = compose_native(raw, alpha, None, DEFAULT_SHADOW)
        self.assertEqual(transparent.dtype, np.uint16)
        self.assertEqual(transparent[0, 0].tolist(), [12001, 32002, 64003, 65535])
        self.assertEqual(transparent[0, 1, 3], 0)
        solid = compose_native(raw, alpha, '#ffffff', DEFAULT_SHADOW)
        self.assertEqual(solid[0, 1].tolist(), [65535, 65535, 65535, 65535])


class CutoutRouteTests(unittest.IsolatedAsyncioTestCase):
    setUp = fixtures.FolderSaveTests.setUp
    tearDown = fixtures.FolderSaveTests.tearDown
    request = fixtures.FolderSaveTests.request
    make_image = fixtures.FolderSaveTests.make_image
    bind = fixtures.FolderSaveTests.bind

    async def manual(self, session=None):
        session = session or self.bind(self.make_image(size=(16, 12)))
        mask = Image.new('L', (16, 12)); mask.paste(255, (4, 3, 12, 10))
        return await self.app.refine_cutout(session['id'], self.request(),
            self.app.CutoutRefine(revision=session['revision'], mask=png64(mask), operation='replace'))

    async def test_manual_cutout_real_preview_export_alpha_and_disable(self):
        data = await self.manual()
        self.assertEqual(data['revision'], 1)
        response = await self.app.preview(data['id'], self.request(), full=True)
        with Image.open(response.path) as image:
            self.assertEqual(image.mode, 'RGBA')
            self.assertEqual(image.getpixel((0, 0))[3], 0)
            self.assertEqual(image.getpixel((6, 6)), (40, 60, 80, 255))
        target = self.images / 'transparent.png'
        self.app.flatten(data, target)
        with Image.open(target) as image:
            self.assertEqual(image.getchannel('A').getextrema(), (0, 255))
        with self.assertRaisesRegex(ValueError, 'JPEG cannot store transparency'):
            self.app.flatten(data, self.images / 'bad.jpg')
        disabled = await self.app.update_cutout(data['id'], self.request(), self.app.CutoutUpdate(revision=1, enabled=False))
        self.assertEqual(self.app.render(disabled).mode, 'RGB')
        self.assertEqual(disabled['cutout']['alpha'], data['cutout']['alpha'])

    async def test_generated_alpha_preserves_source_rgb_and_rejects_opaque_output(self):
        data = self.bind(self.make_image(size=(16, 12)))
        rgba = Image.new('RGBA', (8, 6), (250, 10, 220, 0)); rgba.paste((250, 10, 220, 255), (2, 1, 6, 5))
        with patch('qwen_image.run_qwen_image', AsyncMock(return_value=rgba)) as model:
            result = await self.app.remove_background(data['id'], self.request(), self.app.CutoutRequest(revision=0, variant='bf16'))
        self.assertEqual(model.await_args.kwargs['variant'], 'bf16')
        self.assertEqual(self.app.render(result).getpixel((6, 6))[:3], (40, 60, 80))
        with patch('qwen_image.run_qwen_image', AsyncMock(return_value=Image.new('RGBA', (16, 12), 'red'))):
            with self.assertRaises(HTTPException) as failure:
                await self.app.remove_background(data['id'], self.request(), self.app.CutoutRequest(revision=1))
        self.assertEqual(failure.exception.status_code, 400)
        self.assertEqual(self.app.read_session(data['id'])['revision'], 1)

    async def test_project_roundtrip_keeps_cutout_background_shadow_and_no_folder_authority(self):
        data = await self.manual()
        source = self.make_image('background.png', (70, 90, 120), (31, 19))
        with source.open('rb') as stream:
            data = await self.app.upload_background(data['id'], self.request(), data['revision'], UploadFile(stream, filename=source.name))
        data = await self.app.update_cutout(data['id'], self.request(), self.app.CutoutUpdate(
            revision=data['revision'], feather=0.5, shadow={'enabled': True, 'blur': 1, 'offset_x': 2}))
        expected = np.asarray(self.app.render(data))
        project = self.images / 'cutout.lremove'
        self.app.write_project(self.app.folder(data['id']), data, project)
        with zipfile.ZipFile(project) as archive:
            manifest = json.loads(archive.read('manifest.json'))
            self.assertEqual(manifest['version'], 2)
            self.assertNotIn(str(self.images), json.dumps(manifest))
            self.assertIn(data['cutout']['alpha'], archive.namelist())
            self.assertIn(data['cutout']['background']['asset'], archive.namelist())
        restored = self.app.import_project(project)['session']
        self.assertEqual(restored['cutout'], data['cutout'])
        np.testing.assert_array_equal(np.asarray(self.app.render(restored)), expected)
        self.assertFalse(restored['can_return'])

    async def test_16bit_tiff_cutout_export_keeps_precision_and_alpha(self):
        source = self.images / 'native.tif'
        raw = np.full((12, 16, 3), (12001, 32002, 64003), dtype=np.uint16)
        tifffile.imwrite(source, raw, photometric='rgb')
        data = await self.manual(self.bind(source))
        target = self.images / 'cutout.tif'
        self.app.flatten(data, target)
        result = tifffile.imread(target)
        self.assertEqual(result.dtype, np.uint16)
        self.assertEqual(result[5, 6].tolist(), [12001, 32002, 64003, 65535])
        self.assertEqual(result[0, 0, 3], 0)

    async def test_background_library_native_guard_and_copied_portable_asset(self):
        data = await self.manual()
        self.make_image('library.png', (80, 20, 50))
        payload = self.app.OpenLocal(path=str(self.images))
        with self.assertRaises(HTTPException) as auth:
            await self.app.register_background_folder(self.request(), payload)
        self.assertEqual(auth.exception.status_code, 403)
        library = await self.app.register_background_folder(self.request(native=True), payload)
        self.assertNotIn(str(self.images), json.dumps(library))
        entry = next(x for x in library['entries'] if x['name'] == 'library.png')
        thumbnail = await self.app.background_thumbnail(library['id'], entry['id'], self.request())
        self.assertTrue(Path(thumbnail.path).is_file())
        result = await self.app.apply_library_background(data['id'], self.request(), self.app.LibraryBackground(
            revision=data['revision'], library_id=library['id'], entry_id=entry['id']))
        self.assertEqual(self.app.render(result).getpixel((0, 0)), (80, 20, 50, 255))
        with self.assertRaises(HTTPException):
            await self.app.apply_library_background(data['id'], self.request(), self.app.LibraryBackground(
                revision=result['revision'], library_id=library['id'], entry_id='../outside'))

    async def test_revision_csrf_and_project_metadata_reject_invalid_assets(self):
        data = await self.manual()
        for request, revision, status in [(self.request(csrf=False), 1, 403), (self.request(), 0, 409)]:
            with self.assertRaises(HTTPException) as error:
                await self.app.update_cutout(data['id'], request, self.app.CutoutUpdate(revision=revision, enabled=False))
            self.assertEqual(error.exception.status_code, status)
        with self.assertRaises(HTTPException) as error:
            await self.app.update_cutout(data['id'], self.request(), self.app.CutoutUpdate(revision=1, background={'asset': '../outside.png'}))
        self.assertEqual(error.exception.status_code, 400)
        invalid = json.loads(json.dumps(data)); invalid['cutout']['alpha'] = '../outside.png'
        with self.assertRaisesRegex(ValueError, 'alpha asset'):
            self.app.write_project(self.app.folder(data['id']), invalid, self.images / 'bad.lremove')

    async def test_generated_background_calls_empty_background_task_and_preserves_foreground(self):
        data = await self.manual()
        with patch('qwen_image.run_qwen_image', AsyncMock(return_value=Image.new('RGB', (16, 12), (15, 35, 55)))) as model:
            result = await self.app.generate_background(data['id'], self.request(), self.app.CutoutRequest(revision=1, prompt='Warm studio', variant='int8'))
        self.assertEqual(model.await_args.args[0], None)
        self.assertEqual(model.await_args.kwargs['task'], 'background')
        self.assertEqual(self.app.render(result).getpixel((0, 0)), (15, 35, 55, 255))
        self.assertEqual(self.app.render(result).getpixel((6, 6)), (40, 60, 80, 255))

    async def test_qwen_repair_layers_keep_cutout_separate_and_outside_pixels_exact(self):
        data = await self.manual()
        mask = Image.new('L', (16, 12)); mask.paste(255, (5, 4, 8, 7))
        output = Image.new('RGBA', (16, 12), (40, 60, 80, 255)); output.paste((200, 210, 220, 255), (5, 4, 8, 7))
        with patch('qwen_image.run_qwen_removal', AsyncMock(return_value=output)) as model:
            result = await self.app.remove(data['id'], self.request(), self.app.RemoveRequest(
                revision=data['revision'], mask=png64(mask), model='qwen', variant='bf16'))
        self.assertEqual(model.await_args.kwargs['variant'], 'bf16')
        rendered = self.app.render(result)
        self.assertEqual(rendered.getpixel((6, 5)), (200, 210, 220, 255))
        self.assertEqual(rendered.getpixel((9, 5)), (40, 60, 80, 255))
        self.assertEqual(rendered.getpixel((0, 0))[3], 0)
        self.assertEqual(result['cutout']['alpha'], data['cutout']['alpha'])

    async def test_existing_png_alpha_matches_preview_and_export(self):
        source = self.images / 'alpha.png'
        original = Image.new('RGBA', (16, 12), (40, 60, 80, 255)); original.paste((40, 60, 80, 0), (5, 4, 7, 6)); original.save(source)
        data = await self.manual(self.bind(source))
        rendered = self.app.render(data)
        target = self.images / 'alpha-export.png'; self.app.flatten(data, target)
        with Image.open(target) as exported:
            np.testing.assert_array_equal(np.asarray(rendered), np.asarray(exported))
        self.assertEqual(rendered.getpixel((5, 4))[3], 0)
        for response in (await self.app.base_display(data['id'], self.request()),
                         await self.app.preview(data['id'], self.request(), original=True, full=True)):
            with Image.open(response.path) as compared:
                self.assertEqual(compared.mode, 'RGBA')
                self.assertEqual(compared.getpixel((5, 4))[3], 0)
                self.assertEqual(compared.getpixel((0, 0)), (40, 60, 80, 255))

    async def test_large_photo_requests_work_size_and_recovers_original_alpha_size(self):
        data = self.bind(self.make_image(size=(5000, 1000)))
        output = Image.new('RGBA', (640, 128), (200, 150, 100, 0)); output.paste((200, 150, 100, 255), (160, 32, 480, 112))
        with patch('qwen_image.run_qwen_image', AsyncMock(return_value=output)) as model:
            result = await self.app.remove_background(data['id'], self.request(), self.app.CutoutRequest(revision=0))
        working = model.await_args.kwargs['size']
        self.assertLessEqual(working[0] * working[1], 4194304)
        self.assertLessEqual(max(working), 4096)
        with Image.open(self.app.folder(data['id']) / result['cutout']['alpha']) as alpha:
            self.assertEqual(alpha.size, (5000, 1000)); self.assertEqual(alpha.getextrema(), (0, 255))
        with patch('qwen_image.run_qwen_image', AsyncMock(return_value=Image.new('RGB', working, 'blue'))) as model:
            await self.app.generate_background(data['id'], self.request(), self.app.CutoutRequest(revision=1, prompt='Empty blue studio'))
        self.assertEqual(model.await_args.kwargs['size'], working)

    async def test_cutout_undo_redo_retains_repair_layers_and_resets_redo_on_new_edit(self):
        data = await self.manual()
        first_alpha = data['cutout']['alpha']
        mask = Image.new('L', (16, 12)); mask.paste(255, (5, 4, 8, 7))
        refined = await self.app.refine_cutout(data['id'], self.request(), self.app.CutoutRefine(
            revision=data['revision'], mask=png64(mask), operation='erase'))
        self.assertEqual(self.app.render(refined).getpixel((6, 5))[3], 0)
        self.assertTrue(refined['cutout_can_undo']); self.assertNotIn('cutout_undo', refined)
        # Retouch happens between cutout edits, but stays intact when undoing a cutout.
        repaired = fixtures.FolderSaveTests.layer(self, data['id'])
        undone = await self.app.undo_cutout(data['id'], self.request(), self.app.MergeRequest(revision=repaired['revision']))
        self.assertEqual(undone['cutout']['alpha'], first_alpha)
        self.assertEqual(undone['layers'], repaired['layers'])
        self.assertEqual(self.app.render(undone).getpixel((6, 5))[3], 255)
        self.assertTrue(undone['cutout_can_redo'])
        redone = await self.app.redo_cutout(data['id'], self.request(), self.app.MergeRequest(revision=undone['revision']))
        self.assertEqual(redone['cutout']['alpha'], refined['cutout']['alpha'])
        undone = await self.app.undo_cutout(data['id'], self.request(), self.app.MergeRequest(revision=redone['revision']))
        changed = await self.app.update_cutout(data['id'], self.request(), self.app.CutoutUpdate(revision=undone['revision'], feather=1))
        self.assertFalse(changed['cutout_can_redo'])
        while changed['cutout_can_undo']:
            changed = await self.app.undo_cutout(data['id'], self.request(), self.app.MergeRequest(revision=changed['revision']))
        self.assertNotIn('cutout', changed)
        self.assertEqual(changed['layers'], repaired['layers'])

    async def test_disabled_cutout_project_preserves_alpha_and_later_retouch(self):
        data = await self.manual(self.bind(self.make_image(size=(32, 24))))
        alpha_asset = data['cutout']['alpha']
        disabled = await self.app.update_cutout(data['id'], self.request(), self.app.CutoutUpdate(revision=1, enabled=False))
        repaired = fixtures.FolderSaveTests.layer(self, data['id'], color=(220, 170, 120))
        project = self.images / 'disabled-cutout.lremove'
        self.app.write_project(self.app.folder(data['id']), repaired, project)
        restored = self.app.import_project(project)['session']
        self.assertFalse(restored['cutout']['enabled'])
        self.assertEqual(restored['cutout']['alpha'], alpha_asset)
        self.assertEqual(restored['layers'], repaired['layers'])
        shown = await self.app.update_cutout(restored['id'], self.request(), self.app.CutoutUpdate(revision=restored['revision'], enabled=True))
        self.assertEqual(self.app.render(shown).getpixel((11, 11))[:3], (220, 170, 120))
        with Image.open(self.app.folder(shown['id']) / alpha_asset) as alpha:
            self.assertEqual(self.app.render(shown).getpixel((11, 11))[3], alpha.getpixel((11, 11)))
        self.assertEqual(self.app.render(shown).getpixel((0, 0))[3], 0)

    async def test_subject_transform_project_roundtrip_undo_and_live_preview_assets(self):
        data = await self.manual()
        original_bytes = (self.app.folder(data['id']) / data['original']).read_bytes()
        alpha_bytes = (self.app.folder(data['id']) / data['cutout']['alpha']).read_bytes()
        transform = {'offset_x': 2, 'offset_y': -1, 'scale': 0.75, 'rotation': 30}
        moved = await self.app.update_cutout(data['id'], self.request(), self.app.CutoutUpdate(revision=1, transform=transform))
        self.assertEqual(moved['cutout']['transform'], transform)
        self.assertEqual(tuple(moved['cutout_bounds']), (4, 3, 12, 10))
        root = self.app.folder(data['id'])
        self.assertEqual((root / data['original']).read_bytes(), original_bytes)
        self.assertEqual((root / data['cutout']['alpha']).read_bytes(), alpha_bytes)
        foreground = await self.app.cutout_foreground(data['id'], self.request())
        background = await self.app.cutout_background_preview(data['id'], self.request())
        with Image.open(foreground.path) as fg, Image.open(background.path) as bg:
            self.assertEqual(fg.getchannel('A').getbbox(), (4, 3, 12, 10))
            self.assertIsNone(bg.getchannel('A').getbbox())
        project = self.images / 'transformed.lremove'; self.app.write_project(root, moved, project)
        restored = self.app.import_project(project)['session']
        self.assertEqual(restored['cutout']['transform'], transform)
        np.testing.assert_array_equal(np.asarray(self.app.render(restored)), np.asarray(self.app.render(moved)))
        undone = await self.app.undo_cutout(data['id'], self.request(), self.app.MergeRequest(revision=2))
        self.assertEqual(undone['cutout']['transform'], DEFAULT_TRANSFORM)

    async def test_refining_moved_subject_edits_inverse_mapped_original_alpha(self):
        data = await self.manual()
        moved = await self.app.update_cutout(data['id'], self.request(), self.app.CutoutUpdate(revision=1, transform={'offset_x': 3, 'offset_y': 1}))
        selection = Image.new('L', (16, 12)); selection.paste(255, (8, 5, 11, 8))
        refined = await self.app.refine_cutout(data['id'], self.request(), self.app.CutoutRefine(
            revision=moved['revision'], mask=png64(selection), operation='erase'))
        with Image.open(self.app.folder(data['id']) / refined['cutout']['alpha']) as alpha:
            self.assertEqual(alpha.getpixel((6, 5)), 0)
            self.assertEqual(alpha.getpixel((9, 6)), 255)
        self.assertEqual(self.app.render(refined).getpixel((9, 6))[3], 0)

    async def test_sixteen_bit_transformed_export_keeps_native_color(self):
        source = self.images / 'transform-native.tif'
        raw = np.full((12, 16, 3), (12001, 32002, 64003), dtype=np.uint16)
        tifffile.imwrite(source, raw, photometric='rgb')
        data = await self.manual(self.bind(source))
        moved = await self.app.update_cutout(data['id'], self.request(), self.app.CutoutUpdate(revision=1, transform={'offset_x': 2, 'offset_y': 1}))
        target = self.images / 'moved-native.tif'; self.app.flatten(moved, target)
        result = tifffile.imread(target)
        self.assertEqual(result.dtype, np.uint16)
        self.assertEqual(result[6, 8].tolist(), [12001, 32002, 64003, 65535])
        self.assertEqual(result[3, 4, 3], 0)

    async def test_early_v2_project_without_transform_defaults_to_identity(self):
        data = await self.manual(); root = self.app.folder(data['id'])
        path = self.images / 'early.lremove'; self.app.write_project(root, data, path)
        with zipfile.ZipFile(path) as archive:
            contents = {name: archive.read(name) for name in archive.namelist()}
        manifest = json.loads(contents['manifest.json']); manifest['cutout'].pop('transform')
        contents['manifest.json'] = json.dumps(manifest).encode()
        with zipfile.ZipFile(path, 'w') as archive:
            for name, content in contents.items():
                archive.writestr(name, content)
        restored = self.app.import_project(path)['session']
        self.assertEqual(restored['cutout']['transform'], DEFAULT_TRANSFORM)
        np.testing.assert_array_equal(np.asarray(self.app.render(restored)), np.asarray(self.app.render(data)))

    async def test_generated_background_transfer_preserves_alpha_and_undo(self):
        subject = await self.manual()
        path = self.images / 'generated.png'
        background = Image.new('RGBA', (16, 12), (60, 120, 190, 128))
        background.putpixel((0, 0), (0, 0, 0, 0)); background.save(path)
        generated = self.bind(path)
        result = await self.app.use_generated_background(subject['id'], self.request(),
            self.app.GeneratedBackground(revision=subject['revision'], generated_session_id=generated['id']))
        self.assertEqual(result['revision'], subject['revision'] + 1)
        self.assertEqual(result['cutout']['transform'], subject['cutout']['transform'])
        with Image.open(self.app.folder(subject['id']) / result['cutout']['background']['asset']) as copied:
            np.testing.assert_array_equal(np.asarray(copied), np.asarray(background))
        self.assertEqual(self.app.read_session(generated['id'])['revision'], generated['revision'])
        restored = await self.app.undo_cutout(subject['id'], self.request(), self.app.MergeRequest(revision=result['revision']))
        self.assertEqual(restored['cutout']['background']['mode'], subject['cutout']['background']['mode'])

    async def test_generated_background_transfer_checks_revision_auth_and_target(self):
        subject = await self.manual()
        generated = self.bind(self.make_image(name='second.png'))
        for request, revision, source_id, status in [
                (self.request(csrf=False), subject['revision'], generated['id'], 403),
                (self.request(), 0, generated['id'], 409),
                (self.request(), subject['revision'], subject['id'], 400)]:
            with self.assertRaises(HTTPException) as caught:
                await self.app.use_generated_background(subject['id'], request,
                    self.app.GeneratedBackground(revision=revision, generated_session_id=source_id))
            self.assertEqual(caught.exception.status_code, status)


if __name__ == '__main__':
    unittest.main()
