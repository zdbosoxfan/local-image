"""Library copies and deletion cannot remove editor work or original files."""
import io
import hashlib
import json
from pathlib import Path
import sys
import types
import unittest
from unittest.mock import AsyncMock, patch
import uuid

from fastapi import HTTPException
from PIL import Image
from pydantic import ValidationError

BACKEND = Path(__file__).resolve().parents[1] / 'backend'
sys.path[:0] = [str(BACKEND), str(Path(__file__).resolve().parent / 'helpers')]
import backend_folder_save_test as fixtures
fixtures.V3 = BACKEND
import qwen_image


def parameters():
    return {'model': 'qwen', 'variant': 'int8', 'prompt': 'A transparent red object', 'negative_prompt': '',
            'width': 256, 'height': 256, 'seed': 0, 'transparent': True, 'steps': 25, 'guidance': 1,
            'denoise': None, 'reference_count': 0, 'loras': []}


class GenerationLibraryTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.fixture = fixtures.FolderSaveTests(); self.fixture.setUp(); self.editor = self.fixture.app
        self.library = self.load('generation_library', 'generation_library_isolated')

    def load(self, filename, name):
        module = types.ModuleType(name); module.__file__ = str(BACKEND / (filename + '.py'))
        sys.modules[name] = module
        self.addCleanup(sys.modules.pop, name, None)
        with patch.dict(sys.modules, {'local_remove': self.editor}):
            exec(compile(Path(module.__file__).read_text(encoding='utf-8'), module.__file__, 'exec'), module.__dict__)
        return module

    def tearDown(self):
        self.fixture.tearDown()

    def generated(self, name='generated.png'):
        image = Image.new('RGBA', (256, 256), (200, 30, 40, 0))
        image.paste((200, 30, 40, 255), (50, 40, 220, 230))
        path = self.fixture.images / name; image.save(path)
        result = self.editor.create_session(path, name)
        data = self.editor.read_session(result['id']); data.update(generation=parameters(), revision=1)
        self.editor.write_session(self.editor.folder(data['id']), data)
        return image, data

    async def test_migrates_only_generated_originals_and_counts_actual_library_files(self):
        self.fixture.bind(self.fixture.make_image(size=(256, 256)))
        image, data = self.generated()
        self.fixture.layer(data['id'])
        result = await self.library.list_library(self.fixture.request())
        self.assertEqual(result['count'], 1)
        item = result['items'][0]
        self.assertEqual(item['id'], data['id']); self.assertEqual(item['generation']['seed'], 0)
        self.assertEqual(item['model'], 'qwen'); self.assertEqual((item['width'], item['height']), image.size)
        actual = sum(path.stat().st_size for path in self.library.root_directory().rglob('*') if path.is_file())
        self.assertEqual(result['bytes'], actual); self.assertEqual(item['bytes'], actual)
        response = await self.library.thumbnail(data['id'], self.fixture.request())
        with Image.open(io.BytesIO(response.body)) as thumbnail:
            self.assertLessEqual(thumbnail.width, 480); self.assertEqual(thumbnail.getchannel('A').getextrema(), (0, 255))
        with Image.open(self.library.entry_directory(data['id']) / 'image.png') as stored:
            self.assertEqual(stored.getpixel((10, 10)), image.getpixel((10, 10)), 'Retouch edits must not alter the saved generated result')

    async def test_open_creates_independent_editable_document_and_clear_preserves_work_and_project(self):
        _, data = self.generated()
        self.library.listing()
        opened = (await self.library.open_library_image(data['id'], self.fixture.request()))['session']
        self.assertNotEqual(opened['id'], data['id']); self.assertTrue(opened['dirty']); self.assertFalse(opened['can_return'])
        self.assertEqual(opened['generation'], data['generation'])
        edited = self.fixture.layer(opened['id'])
        project = self.fixture.images / 'saved.lremove'
        self.editor.write_project(self.editor.folder(edited['id']), edited, project)
        before_project = project.read_bytes()
        before_session = (self.editor.folder(edited['id']) / 'session.json').read_bytes()
        original = self.fixture.images / 'generated.png'; before_original = original.read_bytes()
        result = await self.library.delete_library_images(self.fixture.request(), self.library.DeleteImages(all=True))
        self.assertEqual(result['count'], 0); self.assertEqual(result['bytes'], 0)
        self.assertEqual(result['deleted'], [data['id']]); self.assertGreater(result['freed_bytes'], 0)
        self.assertEqual(project.read_bytes(), before_project)
        self.assertEqual((self.editor.folder(edited['id']) / 'session.json').read_bytes(), before_session)
        self.assertEqual(original.read_bytes(), before_original)
        self.assertEqual(self.editor.render(edited).getpixel((10, 10)), (200, 210, 220))
        reopened = self.editor.import_project(project)['session']
        self.assertEqual(reopened['generation'], data['generation'])

    async def test_deleted_items_do_not_reappear_after_restart_or_next_generated_image(self):
        _, first = self.generated()
        self.library.listing()
        self.library.delete_images(self.library.DeleteImages(ids=[first['id']]))
        restarted = self.load('generation_library', 'generation_library_restarted')
        self.assertEqual(restarted.listing()['count'], 0)
        image, second = self.generated('new.png')
        restarted.add_generated(image, second)
        self.assertEqual([item['id'] for item in restarted.listing()['items']], [second['id']])
        self.assertTrue(self.editor.folder(first['id']).is_dir())

    async def test_write_guard_strict_ids_and_batch_preflight_prevent_partial_deletion(self):
        _, data = self.generated(); self.library.listing()
        for values in ({}, {'all': True, 'ids': [data['id']]}, {'ids': ['../sessions']}, {'ids': [data['id'], data['id']]}, {'all': 1}):
            with self.assertRaises(ValidationError):
                self.library.DeleteImages(**values)
        for call in (self.library.delete_library_images(self.fixture.request(csrf=False), self.library.DeleteImages(all=True)),
                     self.library.open_library_image(data['id'], self.fixture.request(csrf=False))):
            with self.assertRaises(HTTPException) as error:
                await call
            self.assertEqual(error.exception.status_code, 403)
        with self.assertRaises(HTTPException):
            self.library.delete_images(self.library.DeleteImages(ids=[data['id'], str(uuid.uuid4())]))
        self.assertTrue((self.library.entry_directory(data['id']) / 'image.png').is_file())
        with self.assertRaises(ValueError):
            self.library.open_image('../sessions')

    async def test_unexpected_files_and_linked_paths_are_never_deleted(self):
        _, first = self.generated(); _, second = self.generated('second.png')
        self.library.listing()
        directory = self.library.entry_directory(second['id'])
        sentinel = directory / 'user-project.lremove'; sentinel.write_bytes(b'preserve this')
        with self.assertRaises(ValueError):
            self.library.delete_images(self.library.DeleteImages(ids=[first['id'], second['id']]))
        self.assertTrue((self.library.entry_directory(first['id']) / 'image.png').exists())
        self.assertEqual(sentinel.read_bytes(), b'preserve this')
        linked = self.library.linked
        with patch.object(self.library, 'linked', side_effect=lambda path: path == directory or linked(path)):
            with self.assertRaises(ValueError):
                self.library.open_image(second['id'])
        self.assertEqual(sentinel.read_bytes(), b'preserve this')

    async def test_corrupted_metadata_remains_counted_and_clear_can_remove_owned_files(self):
        _, data = self.generated(); initial = self.library.listing()
        directory = self.library.entry_directory(data['id'])
        (directory / 'entry.json').write_text('{}')
        broken = self.library.listing()
        self.assertEqual(broken['count'], 0); self.assertGreater(broken['bytes'], 0); self.assertTrue(broken['warning'])
        with self.assertRaises(ValueError):
            self.library.open_image(data['id'])
        self.assertEqual(self.library.delete_images(self.library.DeleteImages(all=True))['bytes'], 0)
        self.assertGreater(initial['bytes'], 0)

    async def test_changed_image_hash_rejected_without_creating_session(self):
        _, data = self.generated(); self.library.listing()
        image = self.library.entry_directory(data['id']) / 'image.png'
        image.write_bytes(b'changed')
        before = len(list(self.editor.SESSIONS.iterdir()))
        with self.assertRaisesRegex(ValueError, 'changed or is damaged'):
            self.library.open_image(data['id'])
        self.assertEqual(len(list(self.editor.SESSIONS.iterdir())), before)

    async def test_mismatched_png_dimensions_rejected_even_with_recomputed_hash(self):
        _, data = self.generated(); self.library.listing()
        directory = self.library.entry_directory(data['id'])
        Image.new('RGB', (512, 512)).save(directory / 'image.png')
        record = json.loads((directory / 'entry.json').read_text())
        record['sha256'] = hashlib.sha256((directory / 'image.png').read_bytes()).hexdigest()
        (directory / 'entry.json').write_text(json.dumps(record))
        before = len(list(self.editor.SESSIONS.iterdir()))
        with self.assertRaisesRegex(ValueError, 'dimensions do not match'):
            self.library.open_image(data['id'])
        self.assertEqual(len(list(self.editor.SESSIONS.iterdir())), before)

    async def test_failed_migration_write_is_retryable_and_cleans_staging(self):
        _, data = self.generated()
        with patch.object(Image.Image, 'save', side_effect=OSError('Disk full')):
            with self.assertRaisesRegex(OSError, 'Disk full'):
                self.library.listing()
        self.assertFalse((self.editor.ROOT / 'generation-library-v1.json').exists())
        self.assertEqual(list(self.library.root_directory().iterdir()), [])
        self.assertEqual(self.library.listing()['items'][0]['id'], data['id'])

    async def test_generation_library_failure_still_returns_successful_editable_result(self):
        api = self.load('image_generation', 'generation_api_library_failure')
        image, _ = self.generated()
        with patch.object(qwen_image, 'run_qwen_image', AsyncMock(return_value=image)), \
                patch.dict(sys.modules, {'generation_library': self.library}), \
                patch.object(self.library, 'add_generated', side_effect=OSError('Disk full')):
            result = await api.generate_image(self.fixture.request(), api.GenerationRequest(prompt='A red object', width=256, height=256, transparent=True))
        self.assertIn('Disk full', result['library_warning']); self.assertTrue(result['session']['dirty'])
        self.assertTrue(self.editor.folder(result['session']['id']).is_dir())
        self.assertFalse(self.fixture.fixture.main.generation_lock.locked())


if __name__ == '__main__':
    unittest.main()
