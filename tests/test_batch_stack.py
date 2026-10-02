"""Real CPU batch snapshots of modern stacks; isolated files, no model execution."""
import io
import json
from copy import deepcopy
from pathlib import Path
import sys
import unittest
import zipfile
from unittest.mock import AsyncMock, patch

import numpy as np
import tifffile
from fastapi import HTTPException
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
import test_batch_tools as batch_fixtures
from test_layer_stack import encoded


class BatchStackTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.case = batch_fixtures.BatchTests()
        self.case.setUp()
        self.editor, self.batch, self.request = self.case.editor, self.case.batch, self.case.request

    async def asyncTearDown(self):
        await self.case.asyncTearDown()

    async def stack(self, native=False):
        if native:
            source = self.case.fixture.images / 'precision.tif'
            rgb = (np.arange(32 * 40 * 3, dtype=np.uint16).reshape(32, 40, 3) * 7 + 12001)
            alpha = np.full((32, 40, 1), 65535, dtype=np.uint16)
            raw = np.concatenate((rgb, alpha), axis=2)
            profile = self.editor.SRGB.tobytes()
            tifffile.imwrite(source, raw, photometric='rgb', extrasamples='unassalpha', metadata=None,
                             extratags=[(34675, 'B', len(profile), profile, False)])
        else:
            source = self.case.fixture.make_image(size=(40, 32), color=(170, 50, 20))
            raw, profile = None, None
        data = self.case.fixture.bind(source)
        data = await self.editor.enable_stack(data['id'], self.request(), self.editor.MergeRequest(revision=data['revision']))
        return data, source, raw, profile

    async def update(self, data, layer, **change):
        return await self.editor.update_stack_layer(data['id'], layer, self.request(), self.editor.StackUpdate(revision=data['revision'], **change))

    async def cutout(self, data, box=(10, 8, 28, 25)):
        mask = Image.new('L', (40, 32)); mask.paste(255, box)
        return await self.editor.refine_cutout(data['id'], self.request(), self.editor.CutoutRefine(revision=data['revision'], mask=encoded(mask), operation='replace'))

    async def test_background_removal_without_treatment_exports_subject_only(self):
        data, _, _, _ = await self.stack()
        data = await self.cutout(data)
        source = self.editor.read_session(data['id'])
        source['layer_stack'][0]['visible'] = True
        self.editor.write_session(self.editor.folder(data['id']), source)
        queue = await self.case.create([source], format='png', prepare_cutouts=True)
        value = await self.case.finish(queue['id'])
        self.assertEqual([item['status'] for item in value['items']], ['ready'], value['items'])
        item = value['items'][0]
        root = self.batch.own_directory('jobs', value['id']) / item['id']
        snapshot = json.loads((root / 'snapshot.json').read_text())
        image = self.editor.render(snapshot, root_override=root)
        self.assertEqual(image.getpixel((0, 0))[3], 0)
        self.assertEqual(image.getpixel((15, 15))[3], 255)
        self.assertEqual(self.editor.read_session(data['id'])['layer_stack'], source['layer_stack'])

    async def test_white_and_file_backgrounds_match_reviewed_export(self):
        for mode, color in [('white', (255, 255, 255)), ('image', (15, 35, 80))]:
            data, _, _, _ = await self.stack()
            data = await self.cutout(data)
            background = encoded(Image.new('RGB', (12, 9), color))
            queue = await self.case.create([self.editor.read_session(data['id'])], format='png', prepare_cutouts=True,
                                           background_mode=mode, background_image=background if mode == 'image' else None)
            value = await self.case.finish(queue['id'])
            self.assertEqual([item['status'] for item in value['items']], ['ready'], value['items'])
            root = self.batch.own_directory('jobs', value['id']) / value['items'][0]['id']
            snapshot = json.loads((root / 'snapshot.json').read_text())
            self.assertEqual(self.editor.render(snapshot, root_override=root).getpixel((0, 0)), (*color, 255))
            exported = await self.export(value)
            with zipfile.ZipFile(self.batch.own_directory('jobs', exported['id']) / 'exports.zip') as archive:
                image_name = next(name for name in archive.namelist() if name.endswith('.png'))
                with Image.open(io.BytesIO(archive.read(image_name))) as image:
                    self.assertEqual(image.convert('RGBA').getpixel((0, 0)), (*color, 255))
            self.assertTrue((await self.batch.delete_job(exported['id'], self.request()))['deleted'])

    async def test_missing_cutout_is_prepared_without_a_saved_treatment(self):
        data, source, _, _ = await self.stack()
        before = source.read_bytes()
        rgba = Image.new('RGBA', (40, 32), (0, 0, 0, 0)); rgba.paste((1, 2, 3, 255), (10, 8, 28, 25))
        with patch('qwen_image.run_qwen_image', AsyncMock(return_value=rgba)) as model:
            queue = await self.case.create([self.editor.read_session(data['id'])], format='png', prepare_cutouts=True)
            value = await self.case.finish(queue['id'])
        self.assertEqual([item['status'] for item in value['items']], ['ready'], value['items'])
        self.assertEqual(model.await_count, 1)
        root = self.batch.own_directory('jobs', value['id']) / value['items'][0]['id']
        snapshot = json.loads((root / 'snapshot.json').read_text())
        self.assertEqual(self.editor.render(snapshot, root_override=root).getpixel((0, 0))[3], 0)
        self.assertEqual(source.read_bytes(), before)
        self.assertNotIn('cutout', self.editor.read_session(data['id']))

    async def test_background_removal_keeps_repairs_above_the_subject(self):
        data, _, _, _ = await self.stack(); data = await self.cutout(data)
        data = await self.editor.create_stack_layer(data['id'], self.request(), self.editor.StackCreate(revision=data['revision'], name='Repair'))
        self.case.fixture.layer(data['id'], color=(25, 200, 60))
        source = self.editor.read_session(data['id'])
        source['layer_stack'][-1]['patch_ids'] = [source['layers'][-1]['id']]
        self.editor.write_session(self.editor.folder(data['id']), source)
        queue = await self.case.create([source], format='png', prepare_cutouts=True)
        value = await self.case.finish(queue['id'])
        self.assertEqual(value['items'][0]['status'], 'ready', value['items'])
        root = self.batch.own_directory('jobs', value['id']) / value['items'][0]['id']
        snapshot = json.loads((root / 'snapshot.json').read_text())
        result = self.editor.render(snapshot, root_override=root)
        self.assertEqual(result.getpixel((11, 11)), (25, 200, 60, 255))
        self.assertEqual(result.getpixel((0, 0))[3], 0)
        self.assertEqual(self.editor.read_session(data['id'])['layer_stack'], source['layer_stack'])

    async def prepare(self, data, format='png'):
        created = await self.case.create([self.editor.read_session(data['id'])], format=format)
        value = await self.case.finish(created['id'])
        self.assertEqual([item['status'] for item in value['items']], ['ready'], value['items'])
        return value

    async def export(self, job):
        await self.batch.export_zip(job['id'], self.request(), self.batch.ExportSelection(item_ids=[job['items'][0]['id']]))
        value = await self.case.finish(job['id'])
        self.assertEqual([item['status'] for item in value['items']], ['exported'], value['items'])
        self.assertTrue(value.get('archive_ready'))
        return value

    async def test_stacked_cutout_image_and_repair_snapshots_export_exact_pixels_without_touching_editor(self):
        data, source, _, _ = await self.stack()
        data = await self.editor.create_stack_layer(data['id'], self.request(), self.editor.StackCreate(revision=data['revision'], name='Dust cleanup'))
        retouch = data['selected_layer_id']
        selection = Image.new('L', (40, 32)); selection.paste(255, (2, 2, 6, 6))
        data = await self.editor.remove(data['id'], self.request(), self.editor.RemoveRequest(revision=data['revision'], model='heal', heal_method='telea', target_layer_id=retouch, mask=encoded(selection)))
        data = await self.cutout(data); first = data['selected_layer_id']
        data = await self.update(data, 'original', visible=False)
        data = await self.update(data, retouch, visible=False)
        root = self.editor.folder(data['id'])
        data = self.editor.add_stack_image(root, self.editor.read_session(data['id']), Image.new('RGBA', (40, 32), (0, 0, 255, 255)), 'Blue background', below=first)
        data = await self.update(data, first, opacity=.75, transform={'offset_x': 2, 'offset_y': 1, 'rotation': 12})
        data = await self.cutout(data, (2, 2, 8, 8)); hidden = data['selected_layer_id']
        data = await self.update(data, hidden, visible=False, locked=True)
        stored = self.editor.read_session(data['id'])
        original_bytes, editor_bytes = source.read_bytes(), (root / 'session.json').read_bytes()
        expected = self.case.fixture.images / 'expected.png'
        self.editor.flatten(stored, expected, True)
        prepared = await self.prepare(data)
        directory = self.batch.own_directory('jobs', prepared['id']) / prepared['items'][0]['id']
        snapshot = json.loads((directory / 'snapshot.json').read_text())
        self.assertEqual(snapshot['layer_stack'], stored['layer_stack'])
        self.assertEqual(snapshot['layers'], stored['layers'])
        for name in self.editor.stack_model.assets(stored['layer_stack']):
            self.assertEqual((directory / name).read_bytes(), (root / name).read_bytes(), name)
        exported = await self.export(prepared)
        with zipfile.ZipFile(self.batch.own_directory('jobs', exported['id']) / 'exports.zip') as archive:
            output = archive.read(exported['items'][0]['output_name'])
        with Image.open(io.BytesIO(output)) as actual, Image.open(expected) as wanted:
            np.testing.assert_array_equal(np.asarray(actual), np.asarray(wanted))
        self.assertEqual(source.read_bytes(), original_bytes)
        self.assertEqual((root / 'session.json').read_bytes(), editor_bytes)

    async def test_native_16bit_alpha_and_icc_survive_reviewed_stack_export(self):
        data, source, raw, profile = await self.stack(native=True)
        data = await self.cutout(data); lid = data['selected_layer_id']
        data = await self.update(data, 'original', visible=False)
        data = await self.update(data, lid, transform={'offset_x': 2})
        original = source.read_bytes()
        expected = self.case.fixture.images / 'expected.tif'
        self.editor.flatten(self.editor.read_session(data['id']), expected)
        prepared = await self.prepare(data, format='original')
        exported = await self.export(prepared)
        output = self.batch.own_directory('jobs', exported['id']) / 'exports' / exported['items'][0]['output_name']
        pixels = tifffile.imread(output)
        self.assertEqual(pixels.dtype, np.uint16)
        np.testing.assert_array_equal(pixels, tifffile.imread(expected))
        self.assertEqual(pixels[12, 16].tolist(), raw[12, 14].tolist())
        self.assertEqual(int(pixels[0, 0, 3]), 0)
        with tifffile.TiffFile(output) as image:
            self.assertEqual(image.pages[0].tags[34675].value, profile)
        self.assertEqual(exported['items'][0]['export_bit_depth'], 16)
        self.assertEqual(source.read_bytes(), original)

    async def test_cache_clear_accepts_exact_stack_asset_names_but_rejects_other_files(self):
        data, source, _, _ = await self.stack()
        data = await self.cutout(data)
        prepared = await self.prepare(data)
        queue_root = self.batch.own_directory('jobs', prepared['id'])
        item_root = queue_root / prepared['items'][0]['id']
        unexpected = item_root / ('stack-' + '0' * 32 + '-source.py')
        unexpected.write_text('This unrecognized file must not be removed.')
        with self.assertRaises(HTTPException) as denied:
            await self.batch.delete_job(prepared['id'], self.request())
        self.assertEqual(denied.exception.status_code, 400)
        self.assertTrue(unexpected.is_file())
        unexpected.unlink()
        original = source.read_bytes()
        result = await self.batch.delete_job(prepared['id'], self.request())
        self.assertTrue(result['deleted'])
        self.assertFalse(queue_root.exists())
        self.assertEqual(source.read_bytes(), original)
        self.assertTrue((self.editor.folder(data['id']) / 'session.json').is_file())

    async def check_stack_treatment(self, mode):
        data, source, _, _ = await self.stack()
        data = await self.cutout(data); target_id = data['selected_layer_id']
        data = await self.update(data, 'original', visible=False)
        root = self.editor.folder(data['id'])
        data = self.editor.add_stack_image(root, self.editor.read_session(data['id']), Image.new('RGBA', (40, 32), (0, 0, 255, 255)), 'Existing image', below=target_id)
        existing_image_id = data['selected_layer_id']
        data = await self.cutout(data, (2, 2, 8, 8)); other_id = data['selected_layer_id']
        data = await self.update(data, other_id, visible=False, locked=True)
        # The compatibility mirror points to the first mask, not the last node.
        data = await self.update(data, target_id, opacity=.9)
        original = self.editor.read_session(data['id'])
        original_bytes, editor_bytes = source.read_bytes(), (root / 'session.json').read_bytes()
        treatment_source = self.case.image('Legacy treatment.png')
        treatment_source['cutout']['feather'] = 2
        credit = {'provider': 'openverse', 'asset_id': 'studio', 'title': 'Studio', 'creator': 'Photographer', 'creator_url': 'https://example.com/creator', 'source_url': 'https://example.com/image', 'license': 'CC BY 4.0', 'license_url': 'https://creativecommons.org/licenses/by/4.0/', 'attribution': 'Studio by Photographer, CC BY 4.0.'}
        if mode == 'color': treatment_source['cutout']['background'] = {'mode': 'color', 'color': '#113399'}
        saved = await self.case.treatment(treatment_source, background=credit if mode == 'image' else None)
        created = await self.case.create([original], treatment_id=saved['id'], format='png')
        prepared = await self.case.finish(created['id'])
        self.assertEqual(prepared['items'][0]['status'], 'ready', prepared['items'][0])
        directory = self.batch.own_directory('jobs', created['id']) / prepared['items'][0]['id']
        snapshot = json.loads((directory / 'snapshot.json').read_text())
        target = next(node for node in snapshot['layer_stack'] if node['id'] == target_id)
        requested = snapshot['cutout']
        self.assertEqual(target['transform'], requested['transform'])
        self.assertEqual(target['cutout']['feather'], requested['feather'])
        self.assertEqual(target['cutout']['shadow'], requested['shadow'])
        self.assertEqual(target['cutout']['background']['mode'], 'transparent')
        self.assertEqual(target['opacity'], .9)
        old_ids = [node['id'] for node in original['layer_stack']]
        self.assertEqual([node['id'] for node in snapshot['layer_stack'] if node['id'] in old_ids], old_ids)
        for node in original['layer_stack']:
            if node['id'] != target_id:
                self.assertEqual(next(value for value in snapshot['layer_stack'] if value['id'] == node['id']), node)
        additions = [node for node in snapshot['layer_stack'] if node['id'] not in old_ids]
        self.assertEqual(len(additions), 0 if mode == 'transparent' else 1)
        if additions:
            self.assertEqual(additions[0]['kind'], 'image')
            self.assertEqual(snapshot['layer_stack'].index(additions[0]) + 1, snapshot['layer_stack'].index(target))
            if mode == 'image': self.assertEqual(additions[0]['attribution'], credit)
        # Compare actual pixels against the existing compositor's declared
        # per-node operations. Existing non-target image data stays unchanged.
        raw, icc, _ = self.editor.decode_original(root / original['original'])
        expected = np.zeros((*raw.shape[:2], 4), dtype=raw.dtype)
        for node in original['layer_stack']:
            if not node['visible'] or node['discarded']: continue
            value = deepcopy(node)
            if node['id'] == target_id:
                if mode != 'transparent':
                    color = (17, 51, 153, 255) if mode == 'color' else (10, 40, 90, 255)
                    expected = self.editor.stack_model.over(expected, np.full((*raw.shape[:2], 4), color, dtype=raw.dtype))
                value['transform'] = deepcopy(requested['transform'])
                value['cutout']['shadow'] = deepcopy(requested['shadow'])
                value['cutout']['feather'] = requested['feather']
            expected = self.editor.stack_model.over(expected, self.editor.stack_model.native_layer(original, root, value, raw, icc, self.editor.decode_original))
        actual = self.editor.render(snapshot, root_override=directory)
        np.testing.assert_array_equal(np.asarray(actual), np.asarray(self.editor.stack_model.display(expected, icc)))
        exported = await self.export(prepared)
        output = self.batch.own_directory('jobs', exported['id']) / 'exports' / exported['items'][0]['output_name']
        with Image.open(output) as image:
            np.testing.assert_array_equal(np.asarray(image), expected)
        if mode == 'image': self.assertEqual(exported['items'][0]['credits'], [credit])
        self.assertEqual(source.read_bytes(), original_bytes)
        self.assertEqual((root / 'session.json').read_bytes(), editor_bytes)
        self.assertEqual(next(node for node in snapshot['layer_stack'] if node['id'] == existing_image_id)['visible'], True)

    async def test_transparent_treatment_targets_matching_cutout_and_preserves_other_nodes(self):
        await self.check_stack_treatment('transparent')

    async def test_color_treatment_adds_ordinary_background_layer_and_exports_declared_pixels(self):
        await self.check_stack_treatment('color')

    async def test_image_treatment_preserves_credits_and_exports_declared_pixels(self):
        await self.check_stack_treatment('image')

    async def check_qwen_stack_preparation(self, native=False, repair=False):
        data, source, raw, profile = await self.stack(native=native)
        if repair:
            data = await self.editor.create_stack_layer(data['id'], self.request(), self.editor.StackCreate(revision=data['revision'], name='Repair before cutout'))
            selection = Image.new('L', (40, 32)); selection.paste(255, (2, 2, 6, 6))
            data = await self.editor.remove(data['id'], self.request(), self.editor.RemoveRequest(revision=data['revision'], model='heal', heal_method='telea', target_layer_id=data['selected_layer_id'], mask=encoded(selection)))
        original = self.editor.read_session(data['id'])
        credit = {'provider': 'openverse', 'asset_id': 'own-product', 'title': 'Product', 'creator': 'Photographer', 'creator_url': 'https://example.com/creator', 'source_url': 'https://example.com/product', 'license': 'CC BY 4.0', 'license_url': 'https://creativecommons.org/licenses/by/4.0/', 'attribution': 'Product by Photographer, CC BY 4.0.'}
        original['source_attribution'] = credit
        root = self.editor.folder(data['id']); self.editor.write_session(root, original)
        original = self.editor.read_session(data['id'])
        source_bytes, editor_bytes = source.read_bytes(), (root / 'session.json').read_bytes()
        native_before, original_profile = self.editor.native_pixels(original, root)
        recipe = self.case.image('Identity treatment.png', size=(40, 32), bounds=(10, 8, 28, 25))
        saved = await self.batch.save_treatment(self.request(), self.batch.SaveTreatment(name='Identity treatment', session_id=recipe['id'], revision=recipe['revision'], format='original'))
        rgba = Image.new('RGBA', (40, 32), (0, 0, 0, 0)); rgba.paste((1, 2, 3, 255), (10, 8, 28, 25))
        with patch('qwen_image.run_qwen_image', AsyncMock(return_value=rgba)) as model:
            created = await self.case.create([original], treatment_id=saved['id'], prepare_cutouts=True)
            prepared = await self.case.finish(created['id'])
        self.assertEqual(prepared['items'][0]['status'], 'ready', prepared['items'][0])
        self.assertEqual(model.await_count, 1)
        directory = self.batch.own_directory('jobs', created['id']) / prepared['items'][0]['id']
        snapshot = json.loads((directory / 'snapshot.json').read_text())
        added = [node for node in snapshot['layer_stack'] if node['id'] not in {item['id'] for item in original['layer_stack']}]
        self.assertEqual(len(added), 1); self.assertEqual(added[0]['kind'], 'cutout')
        self.assertEqual([node['id'] for node in snapshot['layer_stack'][:-1]], [node['id'] for node in original['layer_stack']])
        for before, after in zip(original['layer_stack'], snapshot['layer_stack']):
            expected = dict(before)
            if before['visible'] and not before['discarded']: expected['visible'] = False
            self.assertEqual(after, expected)
        with Image.open(directory / added[0]['cutout']['alpha']) as alpha:
            np.testing.assert_array_equal(np.asarray(alpha), np.asarray(rgba.getchannel('A')))
        np.testing.assert_array_equal(tifffile.imread(directory / added[0]['source']), native_before)
        with tifffile.TiffFile(directory / added[0]['source']) as image:
            if original_profile: self.assertEqual(image.pages[0].tags[34675].value, original_profile)
        self.assertIn(credit, added[0]['reference_attributions'])
        exported = await self.export(prepared)
        output = self.batch.own_directory('jobs', created['id']) / 'exports' / exported['items'][0]['output_name']
        if native:
            pixels = tifffile.imread(output)
            self.assertEqual(pixels.dtype, np.uint16)
            self.assertEqual(pixels[12, 16].tolist(), raw[12, 16].tolist())
            self.assertEqual(int(pixels[0, 0, 3]), 0)
            with tifffile.TiffFile(output) as image: self.assertEqual(image.pages[0].tags[34675].value, profile)
        else:
            with Image.open(output) as image:
                self.assertEqual(image.getpixel((16, 12)), (170, 50, 20, 255), 'Only model alpha is used; source colors stay native')
                self.assertEqual(image.getpixel((0, 0))[3], 0)
        self.assertEqual(exported['items'][0]['credits'], [credit])
        self.assertEqual(source.read_bytes(), source_bytes)
        self.assertEqual((root / 'session.json').read_bytes(), editor_bytes)
        await self.batch.delete_job(created['id'], self.request())
        self.assertFalse(directory.exists())

    async def test_controlled_qwen_prepares_an_original_only_stack_as_detached_cutout(self):
        await self.check_qwen_stack_preparation()

    async def test_controlled_qwen_preserves_native_16bit_repair_pixels_profile_and_credits(self):
        await self.check_qwen_stack_preparation(native=True, repair=True)

    async def test_ambiguous_existing_disabled_cutout_fails_before_model_request(self):
        data, _, _, _ = await self.stack(); data = await self.cutout(data)
        data = await self.update(data, data['selected_layer_id'], visible=False)
        recipe = await self.case.treatment(self.case.image('Recipe.png'))
        rgba = Image.new('RGBA', (40, 32), (0, 0, 0, 0)); rgba.paste((1, 2, 3, 255), (10, 8, 28, 25))
        with patch('qwen_image.run_qwen_image', AsyncMock(return_value=rgba)) as model:
            created = await self.case.create([self.editor.read_session(data['id'])], treatment_id=recipe['id'], prepare_cutouts=True)
            finished = await self.case.finish(created['id'])
        self.assertEqual(finished['items'][0]['status'], 'failed')
        self.assertIn('cutout', finished['items'][0]['error'].lower())
        model.assert_not_awaited()


if __name__ == '__main__':
    unittest.main(verbosity=2)
