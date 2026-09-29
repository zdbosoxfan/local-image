"""Folder and source-save integration tests using isolated files; no GPU jobs."""
import asyncio
import base64
import io
import json
from pathlib import Path
import sys
import time
import unittest
from unittest.mock import patch
import uuid

import numpy as np
from fastapi import HTTPException
from PIL import Image, ImageOps
from pydantic import ValidationError
from starlette.requests import Request

V3 = Path(__file__).resolve().parents[2] / 'backend'
sys.path.insert(0, str(Path(__file__).resolve().parent))
import backend_settings_test as fixtures


class FolderSaveTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.fixture = fixtures.BackendSettingsTests()
        with patch.object(fixtures, 'SOURCE', V3 / 'local_remove.py'):
            self.fixture.setUp()
        self.app = self.fixture.app
        self.images = self.fixture.directory / 'chosen photos'
        self.images.mkdir()

    def tearDown(self):
        self.fixture.tearDown()

    def request(self, *, native=False, csrf=True, origin='http://127.0.0.1:5000'):
        headers = [(b'host', b'127.0.0.1:5000'), (b'origin', origin.encode())]
        if csrf:
            headers.append((b'x-local-remove-token', self.app.CSRF.encode()))
        if native:
            headers.append((b'x-local-launcher', self.app.LAUNCHER_KEY.encode()))
        return Request({'type': 'http', 'scheme': 'http', 'method': 'POST', 'path': '/',
                        'query_string': b'', 'server': ('127.0.0.1', 5000), 'headers': headers})

    def make_image(self, name='photo.png', color=(40, 60, 80), size=(40, 32)):
        source = self.images / name
        Image.new('RGB', size, color).save(source)
        return source

    def bind(self, source):
        return self.app.create_session(source, source.name, source.resolve())

    def layer(self, sid, color=(200, 210, 220)):
        data = self.app.read_session(sid)
        root = self.app.folder(sid)
        lid = uuid.uuid4().hex
        Image.new('RGB', (4, 4), color).save(root / (lid + '-color.png'))
        Image.new('L', (4, 4), 255).save(root / (lid + '-mask.png'))
        data['layers'].append({'id': lid, 'name': 'Heal 1', 'visible': True, 'discarded': False,
                               'x': 10, 'y': 10, 'color': lid + '-color.png', 'mask': lid + '-mask.png'})
        data['revision'] += 1
        self.app.write_session(root, data)
        return self.app.public(data)

    async def save(self, session, mode='unique', output='original'):
        return await self.app.save(session['id'], self.request(),
                                   self.app.SaveRequest(mode=mode, revision=session['revision'], format=output))

    async def register(self):
        result = await self.app.register_folder(self.request(native=True, csrf=False),
                                                self.app.OpenLocal(path=str(self.images)))
        return result['collection']

    async def test_folder_registration_is_lazy_natural_sorted_and_nonrecursive(self):
        for name in ('photo10.png', 'photo2.png', 'photo1.png'):
            (self.images / name).write_bytes(b'not decoded until opened')
        (self.images / 'notes.txt').write_text('ignore')
        sub = self.images / 'nested'; sub.mkdir()
        (sub / 'nested.png').write_bytes(b'ignore')
        with (patch.object(self.app, 'decode_original', side_effect=AssertionError('eager decode')),
              patch.object(self.app, 'file_hash', side_effect=AssertionError('eager hash'))):
            collection = await self.register()
        self.assertEqual([item['name'] for item in collection['entries']], ['photo1.png', 'photo2.png', 'photo10.png'])
        self.assertTrue(all(item['session_id'] is None for item in collection['entries']))
        self.assertEqual(list(self.app.SESSIONS.glob('*/session.json')), [])
        self.assertNotIn(str(self.images), json.dumps(collection))
        reread = await self.app.collection(collection['id'], self.request())
        self.assertEqual(reread, collection)

    async def test_native_auth_and_csrf_only_allow_previously_registered_entries(self):
        self.make_image()
        for request in (self.request(), self.request(native=True, origin='https://outside.example')):
            with self.assertRaises(HTTPException) as failure:
                await self.app.register_folder(request, self.app.OpenLocal(path=str(self.images)))
            self.assertEqual(failure.exception.status_code, 403)
        collection = await self.register()
        entry = collection['entries'][0]
        with self.assertRaises(HTTPException) as failure:
            await self.app.open_collection_entry(collection['id'], entry['id'], self.request(csrf=False))
        self.assertEqual(failure.exception.status_code, 403)
        for unknown in (str(uuid.uuid4()), '../outside.png'):
            with self.assertRaises(HTTPException) as failure:
                await self.app.open_collection_entry(collection['id'], unknown, self.request())
            self.assertEqual(failure.exception.status_code, 404)
        opened = await self.app.open_collection_entry(collection['id'], entry['id'], self.request())
        self.assertTrue(opened['session']['can_return'])
        self.assertEqual(opened['session']['source_name'], 'photo.png')
        self.assertNotIn('source_path', opened['session'])

    async def test_open_files_deduplicates_without_expanding_other_folder_images(self):
        first = self.make_image('shot10.png')
        second = self.make_image('shot2.png')
        self.make_image('unselected.png')
        response = await self.app.open_files(self.request(native=True, csrf=False),
                                            self.app.OpenFiles(paths=[str(first), str(second), str(first)]))
        self.assertEqual([item['name'] for item in response['collection']['entries']], ['shot2.png', 'shot10.png'])

    async def test_revisit_reuses_latest_matching_session_and_retains_dirty_layers(self):
        source = self.make_image()
        collection = await self.register()
        eid = collection['entries'][0]['id']
        first = await self.app.open_collection_entry(collection['id'], eid, self.request())
        edited = self.layer(first['session']['id'])
        newest = self.bind(source)
        newest = self.layer(newest['id'], (1, 2, 3))
        corrupt = self.app.SESSIONS / str(uuid.uuid4()); corrupt.mkdir()
        (corrupt / 'session.json').write_text('{ broken json')
        reopened = await self.app.open_collection_entry(collection['id'], eid, self.request())
        self.assertEqual(reopened['session']['id'], newest['id'])
        self.assertNotEqual(reopened['session']['id'], edited['id'])
        self.assertEqual(len(reopened['session']['layers']), 1)
        self.assertTrue(reopened['session']['dirty'])
        before = source.read_bytes()
        refreshed = await self.register()
        self.assertEqual(refreshed['id'], collection['id'])
        self.assertEqual(refreshed['entries'][0]['id'], eid)
        self.assertEqual(source.read_bytes(), before)

    def test_equal_clock_ticks_preserve_session_edit_order_when_reopening(self):
        source = self.make_image()
        with patch.object(self.app.time, 'time', return_value=1_800_000_000.0):
            first = self.bind(source)
            second = self.bind(source)
            self.assertGreater(second['modified'], first['modified'])
            self.assertEqual(self.app.reuse_or_create_session(source)['id'], second['id'])
            edited_first = self.layer(first['id'])
            self.assertGreater(edited_first['modified'], second['modified'])
            self.assertEqual(self.app.reuse_or_create_session(source)['id'], first['id'])
            self.assertEqual(len(self.app.reuse_or_create_session(source)['layers']), 1)

    async def test_bad_entry_does_not_block_other_photos(self):
        (self.images / 'bad.png').write_bytes(b'broken')
        self.make_image('good.png')
        collection = await self.register()
        with self.assertRaises(HTTPException):
            await self.app.open_collection_entry(collection['id'], collection['entries'][0]['id'], self.request())
        good = await self.app.open_collection_entry(collection['id'], collection['entries'][1]['id'], self.request())
        self.assertEqual(good['session']['name'], 'good.png')
        self.assertIn('error', good['collection']['entries'][0])

    async def test_unique_collision_and_later_overwrite_keep_original_binding_and_layers(self):
        source = self.make_image('photo.JPEG')
        original_bytes = source.read_bytes()
        session = self.layer(self.bind(source)['id'])
        occupied = self.images / 'photo-removed.JPEG'
        occupied.write_bytes(b'existing file must survive')
        first = await self.save(session)
        self.assertEqual(first['name'], 'photo-removed-2.JPEG')
        first_bytes = (self.images / first['name']).read_bytes()
        second = await self.save(first['session'])
        self.assertEqual(second['name'], 'photo-removed-3.JPEG')
        self.assertEqual(source.read_bytes(), original_bytes)
        self.assertEqual(occupied.read_bytes(), b'existing file must survive')
        self.assertFalse(second['session']['dirty'])
        self.assertTrue(second['session']['saved'])
        self.assertEqual(second['session']['source_name'], source.name)
        overwritten = await self.save(second['session'], 'overwrite')
        self.assertEqual(overwritten['name'], source.name)
        self.assertNotEqual(source.read_bytes(), original_bytes)
        self.assertEqual((self.images / first['name']).read_bytes(), first_bytes)
        data = self.app.read_session(session['id'])
        self.assertEqual((self.app.folder(session['id']) / data['original']).read_bytes(), original_bytes)
        self.assertEqual(len(data['layers']), 1)
        reopened = await self.app.open_local(self.request(native=True), self.app.OpenLocal(path=str(source)))
        self.assertEqual(reopened['id'], session['id'])

    async def test_unique_publish_handles_collision_at_commit_without_placeholder(self):
        source = self.make_image()
        session = self.layer(self.bind(source)['id'])
        real_link = self.app.os.link
        attempted = []
        def racing_link(temporary, destination):
            attempted.append(Path(destination))
            if len(attempted) == 1:
                Path(destination).write_bytes(b'concurrent writer')
            return real_link(temporary, destination)
        with patch.object(self.app.os, 'link', side_effect=racing_link):
            result = await self.save(session)
        self.assertEqual(result['name'], 'photo-removed-2.png')
        self.assertEqual(attempted[0].read_bytes(), b'concurrent writer')
        with Image.open(self.images / result['name']) as image:
            self.assertEqual(image.format, 'PNG')
        self.assertEqual(list(self.images.glob('.local-remove-*')), [])

    async def test_stale_revision_and_changed_source_cannot_be_overwritten(self):
        source = self.make_image()
        session = self.layer(self.bind(source)['id'])
        with self.assertRaises(ValidationError):
            self.app.SaveRequest(mode='unique')
        before = source.read_bytes()
        with self.assertRaises(HTTPException) as stale:
            await self.app.save(session['id'], self.request(), self.app.SaveRequest(mode='overwrite', revision=0))
        self.assertEqual(stale.exception.status_code, 409)
        self.assertEqual(source.read_bytes(), before)
        source.write_bytes(b'external change')
        with self.assertRaises(HTTPException) as changed:
            await self.save(session, 'overwrite')
        self.assertEqual(changed.exception.status_code, 409)
        self.assertEqual(source.read_bytes(), b'external change')

    async def test_hash_is_rechecked_after_render_before_overwrite(self):
        source = self.make_image()
        session = self.layer(self.bind(source)['id'])
        render = self.app.flatten
        def change_during_render(*args):
            render(*args)
            source.write_bytes(b'changed after initial hash')
        with patch.object(self.app, 'flatten', side_effect=change_during_render):
            with self.assertRaises(HTTPException) as conflict:
                await self.save(session, 'overwrite')
        self.assertEqual(conflict.exception.status_code, 409)
        self.assertEqual(source.read_bytes(), b'changed after initial hash')
        self.assertEqual(list(self.images.glob('.local-remove-*')), [])
        self.assertTrue(self.app.public(self.app.read_session(session['id']))['dirty'])

    async def test_concurrent_sessions_cannot_overwrite_each_others_save(self):
        source = self.make_image()
        first = self.layer(self.bind(source)['id'], (50, 70, 90))
        second = self.layer(self.bind(source)['id'], (230, 220, 210))
        outcomes = await asyncio.gather(self.save(first, 'overwrite'), self.save(second, 'overwrite'), return_exceptions=True)
        self.assertEqual(sum(isinstance(item, dict) for item in outcomes), 1)
        failures = [item for item in outcomes if isinstance(item, HTTPException)]
        self.assertEqual(len(failures), 1)
        self.assertEqual(failures[0].status_code, 409)

    async def test_collection_saved_state_and_layer_toggle_dirty_state(self):
        self.make_image()
        collection = await self.register()
        opened = await self.app.open_collection_entry(collection['id'], collection['entries'][0]['id'], self.request())
        session = self.layer(opened['session']['id'])
        response = await self.save(session)
        self.assertTrue(response['collection']['entries'][0]['saved'])
        self.assertFalse(response['collection']['entries'][0]['dirty'])
        toggled = await self.app.update_layer(session['id'], session['layers'][0]['id'], self.request(),
                                             self.app.LayerUpdate(visible=False))
        self.assertTrue(toggled['dirty'])
        state = await self.app.collection(collection['id'], self.request())
        self.assertTrue(state['entries'][0]['dirty'])
        self.assertFalse(state['entries'][0]['saved'])

    async def test_jpeg_exif_orientation_dimensions_capture_data_and_icc_survive(self):
        source = self.images / 'rotated.jpg'
        exif = Image.Exif()
        exif[274] = 6
        exif[271] = 'Test camera'
        exif[34665] = {36867: '2024:01:02 03:04:05', 40962: 20, 40963: 40}
        profile = self.app.SRGB.tobytes()
        Image.new('RGB', (20, 40), (50, 70, 90)).save(source, exif=exif, icc_profile=profile)
        session = self.bind(source)
        result = await self.save(session)
        with Image.open(self.images / result['name']) as image:
            self.assertEqual(image.size, (40, 20))
            saved = image.getexif()
            self.assertEqual(saved[274], 1)
            self.assertEqual(saved[271], 'Test camera')
            self.assertEqual((saved[256], saved[257]), (40, 20))
            self.assertEqual(saved.get_ifd(34665)[36867], '2024:01:02 03:04:05')
            self.assertEqual((saved.get_ifd(34665)[40962], saved.get_ifd(34665)[40963]), (40, 20))
            self.assertEqual(image.info['icc_profile'], profile)

    async def test_tiff_precision_and_explicit_8bit_export_formats(self):
        source = self.images / 'precision.TIFF'
        raw = np.full((32, 40, 3), (30003, 41007, 50011), np.uint16)
        profile = self.app.SRGB.tobytes()
        self.app.tifffile.imwrite(source, raw, photometric='rgb', metadata=None,
                                  extratags=[(34675, 'B', len(profile), profile, False)])
        session = self.layer(self.bind(source)['id'])
        original_bytes = source.read_bytes()
        result = await self.save(session)
        self.assertEqual(result['name'], 'precision-removed.TIFF')
        self.assertEqual(result['bit_depth'], 16)
        with self.app.tifffile.TiffFile(self.images / result['name']) as image:
            output = image.asarray()
            self.assertEqual(output.dtype, np.uint16)
            self.assertEqual(image.pages[0].tags['InterColorProfile'].value, profile)
        outside = np.ones(raw.shape[:2], bool); outside[10:14, 10:14] = False
        np.testing.assert_array_equal(output[outside], raw[outside])
        for selected, expected in [('png', 'PNG'), ('jpg', 'JPEG'), ('webp', 'WEBP'), ('tif', 'TIFF')]:
            converted = await self.save(session, output=selected)
            with Image.open(self.images / converted['name']) as image:
                self.assertEqual(image.format, expected)
                self.assertEqual(image.size, (40, 32))
                self.assertEqual(image.info['icc_profile'], profile)
            self.assertEqual(converted['bit_depth'], 16 if selected == 'tif' else 8)
        self.assertEqual(source.read_bytes(), original_bytes)
        with self.assertRaises(HTTPException) as mismatch:
            await self.save(session, 'overwrite', 'png')
        self.assertEqual(mismatch.exception.status_code, 400)
        self.assertIn('Save Unique', mismatch.exception.detail)
        self.assertEqual(source.read_bytes(), original_bytes)

    async def test_webp_export_is_lossless_and_browser_upload_cannot_write_to_source(self):
        source = self.make_image()
        unbound = self.app.create_session(source, source.name)
        with self.assertRaises(HTTPException):
            await self.save(unbound)
        result = await self.app.save(unbound['id'], self.request(),
                                     self.app.SaveRequest(revision=0, format='webp'))
        self.assertEqual(result['mode'], 'export')
        self.assertIn('ext=webp', result['download'])
        response = await self.app.download(unbound['id'], self.request(), ext='webp')
        with Image.open(response.path) as image:
            self.assertEqual(image.format, 'WEBP')
            np.testing.assert_array_equal(np.asarray(image), np.asarray(Image.open(source)))

    async def test_legacy_capture_one_save_payload_remains_supported(self):
        source = self.make_image()
        session = self.layer(self.bind(source)['id'])
        response = await self.app.save(session['id'], self.request(), self.app.SaveRequest(return_to_source=True))
        self.assertTrue(response['returned'])
        self.assertEqual(response['name'], source.name)
        downloaded = await self.app.save(session['id'], self.request(), self.app.SaveRequest(return_to_source=False))
        self.assertEqual(downloaded['format'], 'png')
        self.assertIsNotNone(downloaded['download'])

    async def test_explicit_original_export_retains_input_format_while_legacy_download_stays_png(self):
        for name, expected, ext in [('capture.jpeg','JPEG','jpeg'), ('capture.webp','WEBP','webp')]:
            with self.subTest(name=name):
                source = self.make_image(name)
                # Browser-upload copies also know their input format without a source binding.
                session = self.app.create_session(source, source.name)
                result = await self.app.save(session['id'], self.request(),
                                             self.app.SaveRequest(return_to_source=False, revision=0, format='original'))
                self.assertTrue(result['download'].endswith('ext='+ext))
                response = await self.app.download(session['id'], self.request(), ext=ext)
                with Image.open(response.path) as image:
                    self.assertEqual(image.format, expected)
                legacy = await self.app.save(session['id'], self.request(), self.app.SaveRequest(return_to_source=False))
                self.assertEqual(legacy['format'], 'png')

    async def test_authorized_lazy_thumbnail_is_small_and_updates_after_edit(self):
        self.make_image(size=(640, 480))
        collection = await self.register()
        eid = collection['entries'][0]['id']
        first = await self.app.collection_thumbnail(collection['id'], eid, self.request())
        with Image.open(first.path) as image:
            self.assertEqual(image.size, (160, 120))
        self.assertEqual(list(self.app.SESSIONS.glob('*/session.json')), [])
        opened = await self.app.open_collection_entry(collection['id'], eid, self.request())
        self.layer(opened['session']['id'])
        second = await self.app.collection_thumbnail(collection['id'], eid, self.request())
        self.assertNotEqual(Path(first.path), Path(second.path))
        with self.assertRaises(HTTPException) as failure:
            await self.app.collection_thumbnail(collection['id'], str(uuid.uuid4()), self.request())
        self.assertEqual(failure.exception.status_code, 404)

    async def test_malformed_collection_and_stale_session_summary_fail_cleanly(self):
        malformed_id = str(uuid.uuid4())
        (self.app.COLLECTIONS / (malformed_id + '.json')).write_text(json.dumps({'id': malformed_id, 'entries': 'broken'}))
        with self.assertRaises(HTTPException) as failure:
            await self.app.collection(malformed_id, self.request())
        self.assertEqual(failure.exception.status_code, 404)
        self.make_image()
        collection = await self.register()
        data = self.app.read_collection(collection['id'])
        missing = self.app.SESSIONS / str(uuid.uuid4()); missing.mkdir()
        (missing / 'session.json').write_text('[]')
        data['entries'][0]['session_id'] = missing.name
        self.app.write_collection(data)
        summary = await self.app.collection(collection['id'], self.request())
        self.assertIsNone(summary['entries'][0]['session_id'])
        self.assertFalse(summary['entries'][0]['dirty'])
        self.assertEqual(await self.app.sessions(self.request()), [])

    async def test_merged_snapshot_preserves_16bit_and_heal_uses_merged_generated_pixels(self):
        source = self.images / 'merge16.tif'
        raw = np.full((32, 40, 3), (30003, 41007, 50011), np.uint16)
        profile = self.app.SRGB.tobytes()
        self.app.tifffile.imwrite(source, raw, photometric='rgb', metadata=None,
                                  extratags=[(34675, 'B', len(profile), profile, False)])
        source_before = source.read_bytes()
        generated = self.layer(self.bind(source)['id'], (0, 0, 0))
        generated_lid = generated['layers'][0]['id']
        prior_preview = np.asarray(self.app.render(generated))
        flat = self.fixture.directory / 'before-merge.tif'
        self.app.flatten(generated, flat)
        prior_native = self.app.tifffile.imread(flat)
        merged = await self.app.merge_visible(generated['id'], self.request(), self.app.MergeRequest(revision=generated['revision']))
        merged_layer = merged['layers'][-1]
        self.assertEqual(merged_layer['kind'], 'snapshot')
        self.assertEqual(merged_layer['model_label'], 'Merged visible')
        self.assertTrue(merged['layers'][0]['visible'])
        self.assertTrue(merged['dirty'])
        np.testing.assert_array_equal(np.asarray(self.app.render(merged)), prior_preview)
        self.app.flatten(merged, flat)
        np.testing.assert_array_equal(self.app.tifffile.imread(flat), prior_native)
        # Underlying layer changes are covered by the visible flattened snapshot.
        underneath_hidden = await self.app.update_layer(merged['id'], generated_lid, self.request(), self.app.LayerUpdate(visible=False))
        np.testing.assert_array_equal(np.asarray(self.app.render(underneath_hidden)), prior_preview)
        snapshot_hidden = await self.app.update_layer(merged['id'], merged_layer['id'], self.request(), self.app.LayerUpdate(visible=False))
        self.app.flatten(snapshot_hidden, flat)
        np.testing.assert_array_equal(self.app.tifffile.imread(flat), raw)
        restored = await self.app.update_layer(merged['id'], generated_lid, self.request(), self.app.LayerUpdate(visible=True))
        restored = await self.app.update_layer(merged['id'], merged_layer['id'], self.request(), self.app.LayerUpdate(visible=True))
        selection = Image.new('L', (40, 32)); selection.paste(255, (8, 8, 16, 16))
        healed = await self.app.remove(merged['id'], self.request(), self.app.RemoveRequest(
            revision=restored['revision'], mask=fixtures.png64(selection), model='heal'))
        self.assertEqual(healed['layers'][-1]['model'], 'heal')
        self.assertGreater(np.asarray(self.app.render(healed))[12, 12].mean(), 130)
        self.app.flatten(healed, flat)
        healed_native = self.app.tifffile.imread(flat)
        np.testing.assert_array_equal(healed_native[np.asarray(selection) == 0], prior_native[np.asarray(selection) == 0])
        hidden = await self.app.update_layer(merged['id'], healed['layers'][-1]['id'], self.request(), self.app.LayerUpdate(visible=False))
        np.testing.assert_array_equal(np.asarray(self.app.render(hidden)), prior_preview)
        self.assertEqual(source.read_bytes(), source_before)

    async def test_merge_stale_revision_and_render_failure_leave_layers_unchanged(self):
        session = self.layer(self.bind(self.make_image())['id'])
        with self.assertRaises(HTTPException) as stale:
            await self.app.merge_visible(session['id'], self.request(), self.app.MergeRequest(revision=0))
        self.assertEqual(stale.exception.status_code, 409)
        root = self.app.folder(session['id'])
        before = (root / 'session.json').read_bytes()
        files_before = {path.name for path in root.iterdir()}
        with patch.object(self.app, 'flatten', side_effect=RuntimeError('synthetic disk error')):
            with self.assertRaises(HTTPException):
                await self.app.merge_visible(session['id'], self.request(), self.app.MergeRequest(revision=session['revision']))
        self.assertEqual((root / 'session.json').read_bytes(), before)
        self.assertEqual({path.name for path in root.iterdir()}, files_before)


if __name__ == '__main__':
    unittest.main(verbosity=2)
