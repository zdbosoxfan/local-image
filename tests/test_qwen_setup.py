"""Qwen download selection, job lifecycle and native authorization; no downloads."""
import asyncio
import hashlib
import os
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
import backend_settings_test as helper
import qwen_download_catalog as catalog
import generation_download_catalog as generation_catalog


class QwenSetupTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.fixture = helper.BackendSettingsTests(methodName='runTest')
        self.fixture.setUp()
        self.imports = patch.dict(sys.modules, {'local_remove': self.fixture.app})
        self.imports.start()
        self.routes = types.ModuleType('qwen_setup_under_test')
        self.routes.__file__ = str(HERE / 'backend' / 'qwen_setup.py')
        exec(compile(Path(self.routes.__file__).read_text(), self.routes.__file__, 'exec'), self.routes.__dict__)
        self.root = self.fixture.directory / 'qwen-models'
        self.routes.model_directory = lambda: self.root

    async def asyncTearDown(self):
        if self.routes.manager.active:
            self.routes.manager.task.cancel()
            try:
                await self.routes.manager.task
            except asyncio.CancelledError:
                pass

    def tearDown(self):
        self.imports.stop()
        self.fixture.tearDown()

    def request(self, native=False, origin='http://127.0.0.1:5000'):
        headers = [(b'host', b'127.0.0.1:5000'), (b'origin', origin.encode())]
        if native:
            headers.append((b'x-local-launcher', self.fixture.app.LAUNCHER_KEY.encode()))
        return Request({'type': 'http', 'scheme': 'http', 'method': 'POST', 'path': '/',
                        'query_string': b'', 'server': ('127.0.0.1', 5000), 'headers': headers})

    def small_catalog(self):
        body = b'verified test model'
        def artifact(folder, name):
            return {'folder': folder, 'name': name, 'bytes': len(body),
                    'sha256': hashlib.sha256(body).hexdigest(), 'url': 'https://huggingface.co/test/model/resolve/fixed/' + name}
        vae = artifact('vae', 'shared.safetensors')
        return {'int8': (artifact('diffusion_models', 'compact.safetensors'), vae),
                'bf16': (artifact('diffusion_models', 'full.safetensors'), vae)}, body

    async def test_download_requires_native_and_same_origin_authority(self):
        for request in (self.request(), self.request(True, 'https://remote.example')):
            with self.assertRaises(HTTPException) as error:
                await self.routes.qwen_download_begin(request, self.routes.QwenDownloadRequest(variant='int8'))
            self.assertEqual(error.exception.status_code, 403)
        self.assertIsNone(self.routes.manager.task)

    async def test_status_is_read_only_local_and_has_size_and_license_information(self):
        status = await self.routes.qwen_download_status(self.request())
        self.assertFalse(status['running'])
        self.assertEqual(status['phase'], 'idle')
        self.assertGreater(status['variants'][1]['total_bytes'], status['variants'][0]['total_bytes'])
        self.assertIn('noncommercial', status['license_note'])
        self.assertIn('QwenLM/Qwen-Image-2.1', status['license_url'])
        self.assertFalse(self.root.exists())
        with self.assertRaises(HTTPException):
            await self.routes.qwen_download_status(self.request(origin='https://remote.example'))

    def test_request_cannot_supply_arbitrary_download_url_or_destination(self):
        for payload in ({'variant': 'int4'}, {'variant': 'int8', 'url': 'https://untrusted.example'},
                        {'variant': 'bf16', 'directory': 'C:/elsewhere'}):
            with self.assertRaises(ValidationError):
                self.routes.QwenDownloadRequest(**payload)

    async def test_selected_files_only_and_progress_survives_request_completion(self):
        files, body = self.small_catalog()
        self.routes.QWEN_FILES = files
        gate = asyncio.Event()
        calls = []
        async def download(artifact, target, progress):
            calls.append((artifact, target))
            progress(5, artifact['bytes'])
            await gate.wait()
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(body)
            progress(artifact['bytes'], artifact['bytes'])
        self.routes.download_verified = download
        status = await self.routes.qwen_download_begin(self.request(True), self.routes.QwenDownloadRequest(variant='int8'))
        self.assertTrue(status['running'])
        await asyncio.sleep(0)
        self.assertEqual(self.routes.manager.status()['downloaded_bytes'], 5)
        self.assertTrue(self.fixture.main.generation_lock.locked())
        with self.assertRaises(HTTPException) as error:
            await self.routes.qwen_download_begin(self.request(True), self.routes.QwenDownloadRequest(variant='bf16'))
        self.assertEqual(error.exception.status_code, 409)
        gate.set()
        await self.routes.manager.task
        status = self.routes.manager.status()
        self.assertFalse(status['running'])
        self.assertEqual(status['phase'], 'complete')
        self.assertEqual(status['progress'], 1.0)
        self.assertEqual([item[0]['name'] for item in calls], ['compact.safetensors', 'shared.safetensors'])
        self.assertTrue(all(target.is_relative_to(self.root) for _, target in calls))
        self.assertFalse(self.fixture.main.generation_lock.locked())
        self.assertFalse((self.root / 'diffusion_models' / 'full.safetensors').exists())

    async def test_existing_mismatched_file_preserved_and_error_retained(self):
        files, body = self.small_catalog()
        self.routes.QWEN_FILES = files
        target = self.root / 'diffusion_models' / 'compact.safetensors'
        target.parent.mkdir(parents=True)
        target.write_bytes(b'wrong')
        await self.routes.qwen_download_begin(self.request(True), self.routes.QwenDownloadRequest(variant='int8'))
        await self.routes.manager.task
        status = self.routes.manager.status()
        self.assertEqual(status['phase'], 'error')
        self.assertIn('already exists', status['error'])
        self.assertEqual(target.read_bytes(), b'wrong')
        self.assertFalse(self.fixture.main.generation_lock.locked())

    async def test_existing_correct_files_verified_without_network_or_overwrite(self):
        files, body = self.small_catalog()
        self.routes.QWEN_FILES = files
        for artifact in files['int8']:
            target = self.root / artifact['folder'] / artifact['name']
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(body)
        with patch('managed_ai.aiohttp.ClientSession', side_effect=AssertionError('Existing models must not redownload')):
            await self.routes.qwen_download_begin(self.request(True), self.routes.QwenDownloadRequest(variant='int8'))
            await self.routes.manager.task
        self.assertEqual(self.routes.manager.status()['phase'], 'complete')
        self.assertTrue(self.routes.manager.status()['variants'][0]['installed'])

    async def test_download_revalidates_saved_protected_model_folder_before_network(self):
        protected = self.fixture.directory / 'Program Files'; protected.mkdir()
        target = protected / 'model store'
        self.routes.model_directory = lambda: target
        with (patch.dict(os.environ, {'ProgramFiles': str(protected)}),
              patch.object(self.routes, 'download_verified', AsyncMock()) as download):
            await self.routes.qwen_download_begin(self.request(True), self.routes.QwenDownloadRequest(variant='int8'))
            await self.routes.manager.task
        self.assertIn('outside Program Files', self.routes.manager.status()['error'])
        download.assert_not_awaited()
        self.assertFalse(target.exists())
        self.assertFalse(self.fixture.main.generation_lock.locked())

    async def test_busy_generation_rejects_download_without_job(self):
        await self.fixture.main.generation_lock.acquire()
        try:
            with self.assertRaises(HTTPException) as error:
                await self.routes.qwen_download_begin(self.request(True), self.routes.QwenDownloadRequest(variant='int8'))
            self.assertEqual(error.exception.status_code, 409)
            self.assertIsNone(self.routes.manager.task)
        finally:
            self.fixture.main.generation_lock.release()

    def test_catalog_urls_pin_publisher_revision_and_have_integrity_metadata(self):
        for files in catalog.QWEN_FILES.values():
            self.assertEqual(len(files), 3)
            for artifact in files:
                self.assertIn('/resolve/' + catalog.REVISION + '/', artifact['url'])
                self.assertNotIn('/main/', artifact['url'])
                self.assertEqual(len(artifact['sha256']), 64)
                self.assertGreater(artifact['bytes'], 0)

    async def test_generation_status_reports_both_models_and_incremental_sizes(self):
        files, body = self.small_catalog()
        self.routes.Z_IMAGE_FILES = {'bf16': files['bf16']}
        shared = files['bf16'][1]
        path = self.root / shared['folder'] / shared['name']
        path.parent.mkdir(parents=True)
        path.write_bytes(body)
        status = await self.routes.qwen_download_status(self.request())
        z = next(model for model in status['models'] if model['id'] == 'z-image-turbo')
        self.assertEqual(z['variants'][0]['missing_bytes'], len(body))
        self.assertEqual(z['variants'][0]['total_bytes'], len(body) * 2)
        self.assertIn('Apache', z['license_note'])
        self.assertEqual(status['variants'], status['models'][0]['variants'])

    async def test_z_download_uses_shared_lock_and_selected_catalog(self):
        files, body = self.small_catalog()
        self.routes.Z_IMAGE_FILES = {'bf16': files['bf16']}
        calls = []
        async def download(item, target, progress):
            calls.append(item['name'])
            progress(item['bytes'], item['bytes'])
        self.routes.download_verified = download
        payload = self.routes.GenerationDownloadRequest(model='z-image-turbo', variant='bf16')
        status = await self.routes.generation_download_begin(self.request(True), payload)
        self.assertEqual(status['model'], 'z-image-turbo')
        self.assertTrue(self.fixture.main.generation_lock.locked())
        with self.assertRaises(HTTPException) as error:
            await self.routes.qwen_download_begin(self.request(True), self.routes.QwenDownloadRequest(variant='int8'))
        self.assertEqual(error.exception.status_code, 409)
        await self.routes.manager.task
        self.assertEqual(calls, ['full.safetensors', 'shared.safetensors'])
        self.assertEqual(self.routes.manager.status()['phase'], 'complete')
        self.assertFalse(self.fixture.main.generation_lock.locked())

    async def test_native_selected_model_folder_is_used_by_generation_download(self):
        from app_paths import model_directory
        from managed_ai import configure
        selected = self.fixture.directory / 'user-selected shared models'
        configure(model_directory=str(selected))
        self.routes.model_directory = model_directory
        files, body = self.small_catalog(); self.routes.Z_IMAGE_FILES = {'bf16': files['bf16']}
        targets = []
        async def download(item, target, progress):
            targets.append(target)
            target.parent.mkdir(parents=True, exist_ok=True); target.write_bytes(body)
            progress(item['bytes'], item['bytes'])
        self.routes.download_verified = download
        await self.routes.generation_download_begin(self.request(True), self.routes.GenerationDownloadRequest(model='z-image-turbo', variant='bf16'))
        await self.routes.manager.task
        self.assertEqual(targets, [selected / item['folder'] / item['name'] for item in files['bf16']])
        self.assertEqual(self.routes.manager.status()['model_directory'], str(selected))
        self.assertEqual(self.routes.manager.status()['phase'], 'complete')
        self.assertFalse(self.root.exists(), 'The previous model folder must not receive selected-folder downloads')

    async def test_generic_route_requires_native_authority_and_supported_variant(self):
        payload = self.routes.GenerationDownloadRequest(model='z-image-turbo', variant='bf16')
        with self.assertRaises(HTTPException) as error:
            await self.routes.generation_download_begin(self.request(), payload)
        self.assertEqual(error.exception.status_code, 403)
        unsupported = self.routes.GenerationDownloadRequest(model='z-image-turbo', variant='int8')
        with self.assertRaises(HTTPException) as error:
            await self.routes.generation_download_begin(self.request(True), unsupported)
        self.assertEqual(error.exception.status_code, 422)
        self.assertFalse(self.fixture.main.generation_lock.locked())
        self.assertIsNone(self.routes.manager.task)
        for extra in ({'model': 'unknown', 'variant': 'bf16'},
                      {'model': 'qwen', 'variant': 'bf16', 'url': 'https://remote.example'}):
            with self.assertRaises(ValidationError):
                self.routes.GenerationDownloadRequest(**extra)

    def test_z_catalog_pins_official_template_components(self):
        files = generation_catalog.Z_IMAGE_FILES['bf16']
        self.assertEqual([item['name'] for item in files],
                         ['z_image_turbo_bf16.safetensors', 'qwen_3_4b.safetensors', 'ae.safetensors'])
        for item in files:
            self.assertIn('/resolve/' + generation_catalog.REVISION + '/split_files/', item['url'])
            self.assertEqual(len(item['sha256']), 64)
            self.assertGreater(item['bytes'], 0)

    async def test_flux_generation_presets_are_separate_from_removal_base_and_locked_to_precision(self):
        dev = self.routes.GenerationDownloadRequest(model='flux2-dev', variant='fp8')
        klein = self.routes.GenerationDownloadRequest(model='flux2-klein-4b', variant='bf16')
        self.assertEqual(dev.model, 'flux2-dev'); self.assertEqual(klein.variant, 'bf16')
        self.assertEqual(generation_catalog.FLUX_KLEIN_FILES['bf16'][0]['name'], 'flux-2-klein-4b.safetensors')
        self.assertNotIn('base', generation_catalog.FLUX_KLEIN_FILES['bf16'][0]['name'])
        for payload in (self.routes.GenerationDownloadRequest(model='flux2-dev', variant='bf16'),
                        self.routes.GenerationDownloadRequest(model='qwen', variant='fp8')):
            with self.assertRaises(HTTPException) as error:
                await self.routes.generation_download_begin(self.request(True), payload)
            self.assertEqual(error.exception.status_code, 422)
        self.assertIsNone(self.routes.manager.task)

    def test_flux_catalog_pinned_and_accepts_existing_compatible_vae(self):
        for catalog in (generation_catalog.FLUX_DEV_FILES, generation_catalog.FLUX_KLEIN_FILES, generation_catalog.FLUX_KLEIN_9B_FILES):
            for files in catalog.values():
                for artifact in files:
                    self.assertNotIn('/main/', artifact['url'])
                    self.assertEqual(len(artifact['sha256']), 64)
        target = self.root / 'vae' / 'test.safetensors'
        target.parent.mkdir(parents=True); target.write_bytes(b'compatible')
        self.assertTrue(self.routes._matching_size(target, {'bytes': 1, 'compatible_existing': ({'bytes': 10},)}))

    def test_klein_9b_catalog_retains_shared_vae_and_hides_legacy_dev_from_browse(self):
        files = generation_catalog.FLUX_KLEIN_9B_FILES['fp8']
        self.assertEqual(files[0]['name'], 'flux-2-klein-9b-fp8.safetensors')
        self.assertEqual(files[1]['name'], 'qwen_3_8b_fp8mixed.safetensors')
        self.assertEqual(files[0]['access_url'], generation_catalog.KLEIN_9B_ACCESS_URL)
        self.assertIs(files[2], generation_catalog.FLUX_VAE)
        self.assertIn('Non-Commercial', self.routes._license('flux2-klein-9b')[1])
        ids = [item['id'] for item in self.routes.manager.status()['models']]
        self.assertIn('flux2-klein-9b', ids)
        self.assertNotIn('flux2-dev', ids)

    async def test_hidream_is_hidden_and_rejected_without_starting_a_download(self):
        self.assertNotIn('hidream-o1', [x['id'] for x in self.routes.manager.status()['models']])
        with self.assertRaises(ValidationError):
            self.routes.GenerationDownloadRequest(model='hidream-o1', variant='fp8')
        with self.assertRaises(HTTPException) as error:
            await self.routes._download_begin(self.request(True), 'hidream-o1', 'fp8')
        self.assertEqual(error.exception.status_code, 422)
        with self.assertRaises(self.routes.SetupError):
            self.routes.manager.begin('fp8', model='hidream-o1')
        self.assertIsNone(self.routes.manager.task)
        self.assertFalse(self.fixture.main.generation_lock.locked())

    async def test_seedvr2_download_verifies_both_files_in_selected_directory(self):
        files = self.routes.SEEDVR2_FILES['fp16']
        calls = []
        async def download(artifact, target, progress):
            calls.append((artifact, target))
            progress(artifact['bytes'], artifact['bytes'])
        self.routes.download_verified = download
        status = await self.routes.generation_download_begin(self.request(True),
            self.routes.GenerationDownloadRequest(model='seedvr2', variant='fp16'))
        self.assertTrue(status['running'])
        await self.routes.manager.task
        self.assertEqual(calls, [(item, self.root / item['folder'] / item['name']) for item in files])
        self.assertEqual(self.routes.manager.status()['phase'], 'complete')
        self.assertFalse(self.fixture.main.generation_lock.locked())
        self.assertNotIn('seedvr2', [x['id'] for x in self.routes.manager.status()['models']])
        with self.assertRaises(HTTPException) as error:
            await self.routes.generation_download_begin(self.request(True),
                self.routes.GenerationDownloadRequest(model='seedvr2', variant='fp8'))
        self.assertEqual(error.exception.status_code, 422)

    async def test_ernie_base_only_downloads_exact_preset_and_reuses_shared_vae(self):
        files = self.routes.ERNIE_FILES['bf16']
        calls = []
        async def download(artifact, target, progress):
            calls.append((artifact, target))
            progress(artifact['bytes'], artifact['bytes'])
        self.routes.download_verified = download
        await self.routes.generation_download_begin(self.request(True),
            self.routes.GenerationDownloadRequest(model='ernie-image', variant='bf16'))
        await self.routes.manager.task
        self.assertEqual(calls, [(item, self.root / item['folder'] / item['name']) for item in files])
        self.assertEqual(len(calls), 3)
        self.assertFalse(any('enhancer' in item['name'] for item in files))
        self.assertEqual(files[2]['sha256'], generation_catalog.FLUX_VAE['sha256'])
        self.assertEqual(files[2]['compatible_existing'], generation_catalog.FLUX_VAE['compatible_existing'])
        self.assertEqual(self.routes.manager.status()['phase'], 'complete')
        self.assertFalse(self.fixture.main.generation_lock.locked())
        with self.assertRaises(HTTPException) as error:
            await self.routes.generation_download_begin(self.request(True),
                self.routes.GenerationDownloadRequest(model='ernie-image', variant='fp8'))
        self.assertEqual(error.exception.status_code, 422)


if __name__ == '__main__':
    unittest.main()
