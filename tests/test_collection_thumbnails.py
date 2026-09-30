"""Real isolated native-collection thumbnails; no model calls or user files."""
import base64
import hashlib
import io
from pathlib import Path
import sys
import unittest

import numpy as np
from PIL import Image
import tifffile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
sys.path.insert(0, str(Path(__file__).resolve().parent / 'helpers'))
import backend_folder_save_test as fixtures


def encoded(image):
    stream = io.BytesIO()
    image.save(stream, format='PNG')
    return base64.b64encode(stream.getvalue()).decode('ascii')


def asset_hashes(directory):
    return {str(file.relative_to(directory)): hashlib.sha256(file.read_bytes()).hexdigest()
            for file in directory.rglob('*') if file.is_file()}


class CollectionThumbnailTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.fixture = fixtures.FolderSaveTests()
        self.fixture.setUp()
        self.app, self.request = self.fixture.app, self.fixture.request

    def tearDown(self):
        self.fixture.tearDown()

    async def open_stack(self, collection, entry):
        result = await self.app.open_collection_entry(collection['id'], entry['id'], self.request())
        data = result['session']
        return await self.app.enable_stack(data['id'], self.request(), self.app.MergeRequest(revision=data['revision']))

    def assert_jpeg(self, response):
        self.assertEqual(response.status_code, 200)
        self.assertEqual(response.media_type, 'image/jpeg')
        self.assertTrue(Path(response.path).read_bytes().startswith(b'\xff\xd8'))
        with Image.open(response.path) as image:
            image.load()
            self.assertEqual(image.format, 'JPEG')
            self.assertEqual(image.mode, 'RGB')
            self.assertEqual(image.size, (160, 120))
            self.assertEqual(image.info['icc_profile'], self.app.SRGB.tobytes())
            return image.copy()

    async def test_edited_opaque_stack_thumbnail_is_jpeg_and_revision_cached(self):
        source = self.fixture.make_image(size=(320, 240), color=(170, 50, 20))
        source_before = source.read_bytes()
        collection = await self.fixture.register()
        entry = collection['entries'][0]
        lazy = await self.app.collection_thumbnail(collection['id'], entry['id'], self.request())
        self.assert_jpeg(lazy)
        self.assertEqual(list(self.app.SESSIONS.glob('*/session.json')), [])
        data = await self.open_stack(collection, entry)
        data = await self.app.create_stack_layer(data['id'], self.request(), self.app.StackCreate(revision=data['revision'], name='Retouch'))
        stored = self.app.read_session(data['id'])
        self.assertEqual(self.app.render(stored).mode, 'RGBA')
        self.assertGreater(data['revision'], 0)
        root = self.app.folder(data['id'])
        before = asset_hashes(root)
        response = await self.app.collection_thumbnail(collection['id'], entry['id'], self.request())
        thumbnail = self.assert_jpeg(response)
        self.assertTrue(all(abs(actual - expected) <= 3 for actual, expected in zip(thumbnail.getpixel((80, 60)), (170, 50, 20))))
        self.assertNotEqual(Path(response.path), Path(lazy.path))
        self.assertFalse(Path(lazy.path).exists())
        repeated = await self.app.collection_thumbnail(collection['id'], entry['id'], self.request())
        self.assertEqual(Path(repeated.path), Path(response.path))
        self.assertEqual(asset_hashes(root), before)
        self.assertEqual(source.read_bytes(), source_before)

    async def test_transparent_native_stack_thumbnail_mattes_only_preview_and_keeps_export_guard(self):
        source = self.fixture.images / 'precision.tif'
        raw = np.full((240, 320, 4), (12001, 32002, 64003, 65535), dtype=np.uint16)
        profile = self.app.SRGB.tobytes()
        tifffile.imwrite(source, raw, photometric='rgb', extrasamples='unassalpha', metadata=None,
                         extratags=[(34675, 'B', len(profile), profile, False)])
        source_before = source.read_bytes()
        collection = await self.fixture.register()
        entry = collection['entries'][0]
        data = await self.open_stack(collection, entry)
        alpha = Image.new('L', (320, 240), 255)
        alpha.paste(0, (0, 0, 80, 240)); alpha.paste(128, (80, 0, 160, 240))
        data = await self.app.refine_cutout(data['id'], self.request(), self.app.CutoutRefine(revision=data['revision'], mask=encoded(alpha), operation='replace'))
        data = await self.app.update_stack_layer(data['id'], 'original', self.request(), self.app.StackUpdate(revision=data['revision'], visible=False))
        root = self.app.folder(data['id'])
        stored = self.app.read_session(data['id'])
        native_before, icc_before = self.app.native_pixels(stored, root)
        self.assertEqual(native_before.dtype, np.uint16)
        rendered = self.app.render(stored)
        self.assertEqual(rendered.getpixel((20, 120))[3], 0)
        self.assertEqual(rendered.getpixel((120, 120))[3], 128)
        before = asset_hashes(root)
        response = await self.app.collection_thumbnail(collection['id'], entry['id'], self.request())
        thumbnail = self.assert_jpeg(response)
        for original_x, thumbnail_x in [(20, 10), (120, 60), (250, 125)]:
            red, green, blue, opacity = rendered.getpixel((original_x, 120))
            expected = [round(channel * opacity / 255 + 255 - opacity) for channel in (red, green, blue)]
            self.assertTrue(all(abs(actual - desired) <= 3 for actual, desired in zip(thumbnail.getpixel((thumbnail_x, 60)), expected)), (thumbnail.getpixel((thumbnail_x, 60)), expected))
        with self.assertRaisesRegex(ValueError, 'JPEG cannot store transparency'):
            self.app.flatten(stored, self.fixture.images / 'must-not-export.jpg', allow_8bit=True)
        self.assertFalse((self.fixture.images / 'must-not-export.jpg').exists())
        native_after, icc_after = self.app.native_pixels(self.app.read_session(data['id']), root)
        np.testing.assert_array_equal(native_before, native_after)
        self.assertEqual(icc_before, icc_after)
        self.assertEqual(icc_after, profile)
        self.assertEqual(asset_hashes(root), before)
        self.assertEqual(source.read_bytes(), source_before)
        np.testing.assert_array_equal(tifffile.imread(source), raw)
        self.assertEqual(list((self.app.cache_dir() / 'thumbnails').glob('thumbnail-*.jpg')), [])


if __name__ == '__main__':
    unittest.main()
