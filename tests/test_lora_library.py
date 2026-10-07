"""Live discovery contract, trusted download boundaries and model-bound adapters."""
import asyncio
import hashlib
import json
from pathlib import Path
import struct
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


class LoraLibraryTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.fixture = helper.BackendSettingsTests(methodName='runTest'); self.fixture.setUp()
        self.imports = patch.dict(sys.modules, {'local_remove': self.fixture.app,
            'qwen_setup': types.SimpleNamespace(manager=types.SimpleNamespace(active=False))})
        self.imports.start()
        self.module = types.ModuleType('lora_library_under_test')
        self.module.__file__ = str(HERE / 'backend' / 'lora_library.py')
        exec(compile(Path(self.module.__file__).read_text(encoding='utf-8'), self.module.__file__, 'exec'), self.module.__dict__)
        self.module.model_card = AsyncMock(return_value={})
        self.root = self.fixture.directory / 'models'
        self.module.model_directory = lambda: self.root
        self.module.state_dir = lambda: self.fixture.directory / 'state'
        header = json.dumps({'transformer.block.lora_A.weight': {'dtype': 'F32', 'shape': [1], 'data_offsets': [0, 4]}}).encode()
        self.body = struct.pack('<Q', len(header)) + header + b'\0' * 4
        self.digest = hashlib.sha256(self.body).hexdigest()

    async def asyncTearDown(self):
        if self.module.manager.active:
            self.module.manager.task.cancel()
            try:
                await self.module.manager.task
            except asyncio.CancelledError:
                pass

    def tearDown(self):
        self.imports.stop(); self.fixture.tearDown()

    def request(self, native=False, origin='http://127.0.0.1:5000'):
        headers = [(b'host', b'127.0.0.1:5000'), (b'origin', origin.encode())]
        if native:
            headers.append((b'x-local-launcher', self.fixture.app.LAUNCHER_KEY.encode()))
        return Request({'type': 'http', 'scheme': 'http', 'method': 'POST', 'path': '/',
                        'query_string': b'', 'server': ('127.0.0.1', 5000), 'headers': headers})

    def payload(self, **changes):
        return self.module.LoraDownloadRequest(**{'model': 'z-image-turbo', 'repo_id': 'author/adapter',
            'filename': 'model.safetensors', 'revision': 'a' * 40, **changes})

    def remote(self, match='declared'):
        async def files(model, repo, revision=None):
            return {'repo_id': repo, 'revision': revision, 'compatibility': match,
                    'files': [{'filename': 'model.safetensors', 'bytes': len(self.body), 'sha256': self.digest}],
                    'supported': True, 'warning': '', 'license': 'apache-2.0'}
        self.module.repository_files = files

    async def test_native_and_origin_required_for_download(self):
        for request in (self.request(), self.request(True, 'https://remote.example')):
            with self.assertRaises(HTTPException) as error:
                await self.module.download_begin(request, self.payload())
            self.assertEqual(error.exception.status_code, 403)
        self.assertIsNone(self.module.manager.task)

    def test_no_arbitrary_origins_paths_revisions_or_extra_payload(self):
        for repo in ('https://host/adapter', '../adapter', 'a/b/c', 'a/..', 'a\\b'):
            with self.assertRaises(ValueError): self.module.check_repo(repo)
        for name in ('../model.safetensors', '/model.safetensors', 'a\\b.safetensors', 'C:/x.safetensors', 'model.pt'):
            with self.assertRaises(ValueError): self.module.check_filename(name)
        with self.assertRaises(ValueError): self.module.check_revision('main')
        with self.assertRaises(ValidationError): self.payload(url='https://remote.example')
        with self.assertRaises(ValidationError): self.payload(allow_unverified='yes')

    def test_base_version_boundaries_and_explicit_klein_compatibility(self):
        check = self.module.compatibility
        self.assertEqual(check('qwen', {'cardData': {'base_model': 'Qwen/Qwen-Image'}}), 'incompatible')
        self.assertEqual(check('qwen', {'cardData': {'base_model': 'Qwen/Qwen-Image-2.1'}}), 'declared')
        self.assertEqual(check('z-image-turbo', {'cardData': {'base_model': 'Tongyi-MAI/Z-Image'}}), 'incompatible')
        self.assertEqual(check('flux2-klein-4b', {'cardData': {'base_model': 'black-forest-labs/FLUX.2-klein-base-4B'}}), 'declared')
        self.assertEqual(check('flux2-klein-4b', {'cardData': {'base_model': 'black-forest-labs/FLUX.2-klein-9B'}}), 'incompatible')
        self.assertEqual(check('flux2-klein-9b', {'cardData': {'base_model': 'black-forest-labs/FLUX.2-klein-base-9B'}}), 'declared')
        self.assertEqual(check('flux2-klein-9b', {'cardData': {'base_model': 'black-forest-labs/FLUX.2-klein-4B'}}), 'incompatible')
        self.assertEqual(check('hidream-o1', {'cardData': {'base_model': 'HiDream-ai/HiDream-O1-Image'}}), 'declared')
        self.assertEqual(check('hidream-o1', {'cardData': {'base_model': 'HiDream-ai/HiDream-I1-Full'}}), 'incompatible')
        self.assertEqual(check('hidream-o1', {'cardData': {'base_model': 'HiDream-ai/HiDream-O1-Image-Dev'}}), 'incompatible')
        self.assertEqual(check('qwen', {}), 'unverified')

    def test_publisher_metadata_handles_nulls_without_mutating_base_list(self):
        self.assertEqual(self.module.compatibility('qwen', {'cardData': [], 'tags': None}), 'unverified')
        metadata = {'cardData': {'base_model': ['Qwen/Qwen-Image-2.1']},
                    'tags': ['base_model:adapter:Qwen/Qwen-Image-2.1']}
        self.assertEqual(self.module.compatibility('qwen', metadata), 'declared')
        self.assertEqual(metadata['cardData']['base_model'], ['Qwen/Qwen-Image-2.1'])

    async def test_search_is_live_sorted_and_excludes_declared_wrong_base(self):
        calls = []
        async def hub(path, params=None):
            calls.append((path, params))
            return [{'id': 'a/new', 'tags': ['lora'], 'lastModified': '2026-09-29', 'cardData': {'base_model': 'Qwen/Qwen-Image-2.1'}},
                    {'id': 'a/old', 'tags': ['lora'], 'lastModified': '2026-09-20'},
                    {'id': 'a/wrong', 'tags': ['lora'], 'cardData': {'base_model': 'Qwen/Qwen-Image'}},
                    {'id': 'a/fullmodel', 'tags': ['text-to-image']}]
        self.module.hub_json = hub
        first = await self.module.search_hub('qwen')
        previous = len(calls)
        second = await self.module.search_hub('qwen')
        self.assertGreater(len(calls), previous)
        self.assertEqual([item['repo_id'] for item in first['results']], ['a/new', 'a/old'])
        self.assertTrue(first['checked_at']); self.assertTrue(second['checked_at'])
        self.assertTrue(all(call[1]['sort'] == 'lastModified' and call[1]['direction'] == '-1' for call in calls))

    async def test_search_ignores_malformed_rows_and_nullable_publisher_fields(self):
        async def hub(path, params=None):
            return [None, [], {'id': []}, {'id': 'a/null-tags', 'tags': None},
                    {'id': 'a/adapter', 'tags': ['lora'], 'cardData': [], 'lastModified': 123}]
        self.module.hub_json = hub
        result = await self.module.search_hub('qwen')
        self.assertEqual([item['repo_id'] for item in result['results']], ['a/adapter'])
        self.assertEqual(result['results'][0]['compatibility'], 'unverified')
        self.assertEqual(result['results'][0]['updated_at'], '')

    async def test_search_requests_cards_filters_mature_before_paging_and_enriches_pinned_descriptions(self):
        rows=[{'id':f'artist/mature-{index}','sha':'a'*40,'tags':['lora','not-for-all-audiences'],
               'lastModified':'2026-10-01','cardData':{'base_model':'Qwen/Qwen-Image-2.1'}} for index in range(6)]
        rows += [{'id':f'artist/style-{index}','sha':'b'*40,'tags':['lora'],'lastModified':'2026-09-30',
                  'cardData':{'base_model':'Qwen/Qwen-Image-2.1'}} for index in range(25)]
        calls=[]
        async def hub(path,params=None):calls.append(params);return rows
        self.module.hub_json=hub
        self.module.model_card=AsyncMock(return_value={'description':'A soft illustrative style supplied by its publisher.',
                                                      'content_rating':'unknown','content_rating_source':None})
        result=await self.module.search_hub('qwen')
        self.assertEqual(len(result['results']),20)
        self.assertEqual(result['hidden_count'],6)
        self.assertEqual(result['next_page'],1)
        self.assertTrue(all(item['content_rating']=='unknown' and item['repo_id'].startswith('artist/style-') for item in result['results']))
        self.assertTrue(all(item['description'].startswith('A soft illustrative') and '/blob/'+'b'*40+'/README.md' in item['source_url'] for item in result['results']))
        self.assertTrue(all(params['cardData']=='true' for params in calls))
        self.assertEqual(self.module.model_card.await_count,20)
        visible=await self.module.search_hub('qwen',show_adult=True)
        self.assertEqual(visible['hidden_count'],0)
        self.assertEqual(sum(item['content_rating']=='adult' for item in visible['results']),6)

    async def test_publisher_description_metadata_avoids_unnecessary_readme_fetch(self):
        async def hub(path,params=None):
            return [{'id':'artist/style','sha':'a'*40,'tags':['lora'],
                     'cardData':{'description':'A **watercolor** adapter for vivid landscape illustrations.'}}]
        self.module.hub_json=hub
        result=await self.module.search_hub('qwen')
        self.assertEqual(result['results'][0]['description'],'A watercolor adapter for vivid landscape illustrations.')
        self.module.model_card.assert_not_awaited()

    async def test_duplicate_search_reply_cannot_remove_mature_publisher_declaration(self):
        async def hub(path,params=None):
            tags=['lora','nsfw'] if params['filter'].startswith('base_model:adapter:') else ['lora']
            return [{'id':'artist/style','tags':tags}]
        self.module.hub_json=hub
        result=await self.module.search_hub('qwen')
        self.assertEqual(result['results'],[])
        self.assertEqual(result['hidden_count'],1)
        visible=await self.module.search_hub('qwen',show_adult=True)
        self.assertEqual(visible['results'][0]['content_rating'],'adult')

    async def test_filtered_mature_labels_upgrade_curated_recommendations_without_changing_artifact(self):
        reviewed=self.module.CURATED[0]
        before=await self.module.library(self.request(),reviewed['model'])
        original=next(item for item in before['curated'] if item['repo_id']==reviewed['repo_id'])
        self.assertEqual(original['content_rating'],'unknown')
        async def hub(path,params=None):
            return [{'id':reviewed['repo_id'],'tags':['lora','not-for-all-audiences']}]
        self.module.hub_json=hub
        found=await self.module.search_hub(reviewed['model'])
        self.assertEqual(found['results'],[])
        self.assertEqual(found['content_labels'][reviewed['repo_id']]['content_rating'],'adult')
        after=await self.module.library(self.request(),reviewed['model'])
        updated=next(item for item in after['curated'] if item['repo_id']==reviewed['repo_id'])
        self.assertEqual(updated['content_rating'],'adult')
        self.assertIsNone(updated['preview_url'])
        for field in ('model','filename','revision','sha256','bytes'):
            self.assertEqual(updated[field],original[field])
        async def unknown(path,params=None):return [{'id':reviewed['repo_id'],'tags':['lora']}]
        self.module.hub_json=unknown
        repeated=await self.module.search_hub(reviewed['model'])
        self.assertEqual(repeated['content_labels'][reviewed['repo_id']]['content_rating'],'adult')

    async def test_repository_metadata_and_file_descriptions_are_retained_but_mature_browse_is_opt_in(self):
        metadata={'sha':'a'*40,'tags':['not-for-all-audiences'],
                  'cardData':{'base_model':'Tongyi-MAI/Z-Image-Turbo'},
                  'siblings':[{'rfilename':'model.safetensors','lfs':{'size':len(self.body),'sha256':self.digest}}]}
        async def hub(path,params=None):return metadata
        self.module.hub_json=hub
        self.module.model_card=AsyncMock(return_value={'description':'A publisher supplied synthetic fixture adapter.', 'content_rating':'unknown'})
        hidden=await self.module.files(self.request(),'z-image-turbo','artist/style')
        self.assertEqual(hidden['files'],[])
        self.assertEqual(hidden['content_rating'],'adult')
        visible=await self.module.files(self.request(),'z-image-turbo','artist/style',show_adult=True)
        self.assertEqual(visible['files'][0]['description'],'A publisher supplied synthetic fixture adapter.')
        self.assertEqual(visible['files'][0]['content_rating'],'adult')
        self.assertIn('Publisher tag',visible['content_rating_source'])

    async def test_gallery_preference_is_strict_persistent_and_requires_local_csrf(self):
        self.assertEqual(await self.module.preferences(self.fixture.request()),{'show_adult_content':False})
        for value in ('true',1,None):
            with self.assertRaises(ValidationError):self.module.LoraPreferences(show_adult_content=value)
        payload=self.module.LoraPreferences(show_adult_content=True)
        for request in (self.fixture.request(token=False),self.fixture.request(origin='https://foreign.example')):
            with self.assertRaises(HTTPException) as denied:await self.module.save_preferences(request,payload)
            self.assertEqual(denied.exception.status_code,403)
        self.module.write_config({'hardware_guide_dismissed':True,'model_directory':'existing-models'})
        saved=await self.module.save_preferences(self.fixture.request(),payload)
        self.assertEqual(saved,{'show_adult_content':True})
        self.assertEqual(await self.module.preferences(self.fixture.request()),saved)
        self.assertTrue(self.module.read_config()['hardware_guide_dismissed'])
        self.assertEqual(self.module.read_config()['model_directory'],'existing-models')

    async def test_hub_reader_handles_json_split_across_network_packets(self):
        class Stream:
            async def iter_chunked(self, amount):
                for chunk in (b'[{"id":', b'"author/adapter"', b'}]'):
                    yield chunk
        class Response:
            status = 200
            content = Stream()
            async def __aenter__(self): return self
            async def __aexit__(self, *args): return None
        class Session:
            def __init__(self, **kwargs): pass
            async def __aenter__(self): return self
            async def __aexit__(self, *args): return None
            def get(self, address, **kwargs):
                self.address = address
                return Response()
        with patch.object(self.module.aiohttp, 'ClientSession', Session):
            result = await self.module.hub_json('/api/models')
        self.assertEqual(result, [{'id': 'author/adapter'}])

    async def test_repo_files_pin_revision_require_lfs_sha_and_safe_safetensors(self):
        metadata = {'sha': 'b' * 40, 'cardData': {'base_model': 'Tongyi-MAI/Z-Image-Turbo'}, 'siblings': [
            {'rfilename': 'dir/good.safetensors', 'lfs': {'size': 123, 'sha256': 'c' * 64}},
            {'rfilename': '../bad.safetensors', 'lfs': {'size': 123, 'sha256': 'c' * 64}},
            {'rfilename': 'model.pt', 'lfs': {'size': 123, 'sha256': 'c' * 64}},
            {'rfilename': 'nohash.safetensors', 'size': 123}]}
        calls = []
        async def hub(path, params=None): calls.append(path); return metadata
        self.module.hub_json = hub
        result = await self.module.repository_files('z-image-turbo', 'a/b')
        self.assertEqual(len(result['files']), 1)
        self.assertEqual({key: result['files'][0][key] for key in ('filename', 'bytes', 'sha256', 'compatibility')},
                         {'filename': 'dir/good.safetensors', 'bytes': 123, 'sha256': 'c' * 64, 'compatibility': 'declared'})
        self.assertEqual(result['files'][0]['content_rating'], 'unknown')
        self.assertIn('/revision/' + 'b' * 40, calls[-1])
        metadata['cardData']['base_model'] = 'Tongyi-MAI/Z-Image'
        with self.assertRaises(ValueError): await self.module.repository_files('z-image-turbo', 'a/b')

    async def test_repo_files_reject_revision_mismatch_and_invalid_metadata(self):
        async def selected_revision_mismatch(path, params=None):
            return {'sha': 'b' * 40, 'siblings': []}
        self.module.hub_json = selected_revision_mismatch
        with self.assertRaisesRegex(ValueError, 'selected revision'):
            await self.module.repository_files('qwen', 'a/b', 'a' * 40)
        async def blob_revision_mismatch(path, params=None):
            return {'sha': ('b' if params else 'a') * 40, 'siblings': []}
        self.module.hub_json = blob_revision_mismatch
        with self.assertRaisesRegex(ValueError, 'invalid file list'):
            await self.module.repository_files('qwen', 'a/b')
        async def invalid_metadata(path, params=None): return []
        self.module.hub_json = invalid_metadata
        with self.assertRaisesRegex(ValueError, 'invalid repository metadata'):
            await self.module.repository_files('qwen', 'a/b')

    async def test_unverified_requires_assignment_and_no_download_happens(self):
        self.remote('unverified')
        async def unexpected(*args): self.fail('Unassigned adapter must not download')
        self.module.download_verified = unexpected
        await self.module.download_begin(self.request(True), self.payload())
        await self.module.manager.task
        self.assertEqual(self.module.manager.status()['phase'], 'error')
        self.assertIn('Explicitly assign', self.module.manager.status()['error'])
        self.assertFalse(self.fixture.main.generation_lock.locked())

    async def test_curated_repository_does_not_trust_new_revisions_or_unknown_files(self):
        repo = self.module.CURATED[0]['repo_id']
        metadata = {'id': repo, 'sha': 'd' * 40, 'cardData': {'base_model': 'Qwen/Qwen-Image-2.1'}, 'siblings': []}
        async def hub(path, params=None): return metadata
        self.module.hub_json = hub
        with self.assertRaises(ValueError): await self.module.repository_files('z-image-turbo', repo)
        metadata['cardData'] = {}
        metadata['siblings'] = [{'rfilename': 'new.safetensors', 'lfs': {'size': 123, 'sha256': 'c' * 64}}]
        result = await self.module.repository_files('z-image-turbo', repo)
        self.assertEqual(result['compatibility'], 'unverified')
        self.assertEqual(result['files'][0]['compatibility'], 'unverified')
        self.assertNotIn('recommended_settings', result['files'][0])

    async def install_fixture(self, match='declared'):
        self.remote(match)
        async def download(artifact, target, progress):
            self.assertTrue(artifact['url'].startswith('https://huggingface.co/author/adapter/resolve/' + 'a' * 40 + '/'))
            target.parent.mkdir(parents=True, exist_ok=True); target.write_bytes(self.body)
            progress(len(self.body), len(self.body))
        self.module.download_verified = download
        await self.module.download_begin(self.request(True), self.payload(allow_unverified=match == 'unverified'))
        await self.module.manager.task
        self.assertEqual(self.module.manager.status()['phase'], 'complete')
        return self.module.installed()[0]

    async def test_install_records_model_binding_and_does_not_enable_automatically(self):
        entry = await self.install_fixture('unverified')
        self.assertEqual(entry['compatibility'], 'unverified')
        self.assertEqual(len(entry['id']), 24)
        self.assertFalse(self.fixture.main.generation_lock.locked())
        self.assertEqual(self.module.resolve_loras('z-image-turbo', []), [])
        self.assertEqual(self.module.resolve_loras('z-image-turbo', [{'id': entry['id'], 'strength': 0.8}]), [(entry['comfy_filename'], 0.8)])
        with self.assertRaises(ValueError): self.module.resolve_loras('qwen', [{'id': entry['id'], 'strength': 1}])

    async def test_explicit_install_keeps_description_rating_and_publisher_provenance_offline(self):
        self.remote()
        remote=self.module.repository_files
        async def files(model,repo,revision=None):
            value=await remote(model,repo,revision)
            value.update(description='A publisher supplied metadata test fixture.',content_rating='adult',
                         content_rating_source='Publisher tag: not-for-all-audiences',
                         source_url='https://huggingface.co/'+repo+'/blob/'+revision+'/README.md',
                         publisher_example_url='https://huggingface.co/'+repo+'/resolve/'+revision+'/images/example.png')
            return value
        self.module.repository_files=files
        async def download(artifact,target,progress):
            target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(self.body)
        self.module.download_verified=download
        await self.module.download_begin(self.request(True),self.payload())
        await self.module.manager.task
        self.assertEqual(self.module.manager.status()['phase'],'complete')
        entry=self.module.installed()[0]
        self.assertEqual(entry['content_rating'],'adult')
        self.assertEqual(entry['description'],'A publisher supplied metadata test fixture.')
        self.assertIn('Publisher tag',entry['content_rating_source'])
        self.assertIsNone(entry['preview_url'])
        visible=await self.module.library(self.request(),show_adult=True)
        self.assertTrue(visible['installed'][0]['preview_url'].endswith('?show_adult=true'))
        self.assertEqual(self.module.resolve_loras('z-image-turbo',[{'id':entry['id'],'strength':1}]),[(entry['comfy_filename'],1.0)])

    async def test_resolver_rejects_missing_duplicate_unsafe_strength_and_modified_registry_path(self):
        entry = await self.install_fixture()
        selection = {'id': entry['id'], 'strength': 1}
        for choices in ([selection, selection], [{'id': entry['id'], 'strength': float('nan')}],
                        [{'id': entry['id'], 'strength': 3}], [{'id': '../path', 'strength': 1}]):
            with self.assertRaises(ValueError): self.module.resolve_loras('z-image-turbo', choices)
        entry['comfy_filename'] = '../outside.safetensors'
        self.module.write_registry({entry['id']: entry})
        self.assertEqual(self.module.installed(), [])

    async def test_registry_ignores_malformed_entries_and_model_identity_reassignment(self):
        entry = await self.install_fixture()
        registry = {entry['id']: entry, 'missing-fields': {'bytes': len(self.body)}, 'not-an-entry': []}
        for index, change in enumerate(({'model': []}, {'model': 'qwen'}, {'id': []}, {'bytes': True},
                                       {'sha256': None}, {'filename': '../model.safetensors'},
                                       {'comfy_filename': 'local-image/qwen-' + entry['id'] + '.safetensors'})):
            registry['bad-' + str(index)] = {**entry, **change}
        self.module.write_registry(registry)
        self.assertEqual([item['id'] for item in self.module.installed()], [entry['id']])
        self.assertEqual(self.module.resolve_loras('z-image-turbo', [{'id': entry['id'], 'strength': 1}]),
                         [(entry['comfy_filename'], 1.0)])

    async def test_resolved_lora_subfolder_must_stay_inside_configured_model_root(self):
        entry = await self.install_fixture()
        original_resolve = Path.resolve
        models = original_resolve(self.root)
        outside = original_resolve(self.fixture.directory / 'outside-models')
        def resolve(path, *args, **kwargs):
            return outside if path == models / 'loras' else original_resolve(path, *args, **kwargs)
        with patch.object(Path, 'resolve', resolve):
            with self.assertRaisesRegex(ValueError, 'outside the configured model directory'):
                self.module.installed_path(entry)
            self.assertEqual(self.module.installed(), [])
            with self.assertRaisesRegex(ValueError, 'missing'):
                self.module.resolve_loras('z-image-turbo', [{'id': entry['id'], 'strength': 1}])

    async def test_busy_lock_rejects_without_starting_job(self):
        await self.fixture.main.generation_lock.acquire()
        try:
            with self.assertRaises(HTTPException) as error:
                await self.module.download_begin(self.request(True), self.payload())
            self.assertEqual(error.exception.status_code, 409)
            self.assertIsNone(self.module.manager.task)
        finally: self.fixture.main.generation_lock.release()

    def test_header_rejects_full_model_and_pickle_content(self):
        path = self.fixture.directory / 'fake.safetensors'
        for data in (b'pickle', struct.pack('<Q', 2**40)):
            path.write_bytes(data)
            with self.assertRaises(ValueError): self.module.validate_lora_header(path)
        header = json.dumps({'full.weight': {}}).encode()
        path.write_bytes(struct.pack('<Q', len(header)) + header)
        with self.assertRaises(ValueError): self.module.validate_lora_header(path)
        path.write_bytes(self.body); self.module.validate_lora_header(path)

    async def test_offline_inventory_does_not_make_network_requests(self):
        await self.install_fixture()
        async def fail(*args): raise AssertionError('Inventory must be offline')
        self.module.hub_json = fail
        result = await self.module.library(self.request(), 'z-image-turbo')
        self.assertEqual(len(result['installed']), 1)
        self.assertTrue(result['curated'])

    async def test_same_repository_styles_have_exact_file_metadata_and_pinned_route(self):
        styles = [item for item in self.module.CURATED if item['repo_id'] == 'rehan-fal/klein-style-fleet']
        self.assertEqual(len(styles), 2)
        metadata = {'sha': styles[0]['revision'], 'cardData': {'base_model': 'black-forest-labs/FLUX.2-klein-base-4B'},
                    'siblings': [{'rfilename': item['filename'], 'lfs': {'size': item['bytes'], 'sha256': item['sha256']}} for item in styles]}
        metadata['siblings'].append({'rfilename': 'other.safetensors', 'lfs': {'size': 100, 'sha256': 'a' * 64}})
        calls = []
        async def hub(path, params=None): calls.append(path); return metadata
        self.module.hub_json = hub
        result = await self.module.files(self.request(), 'flux2-klein-4b', styles[0]['repo_id'], styles[0]['revision'])
        returned = {item['filename']: item for item in result['files']}
        for style in styles:
            item = returned[style['filename']]
            self.assertEqual(item['compatibility'], 'curated')
            self.assertEqual(item['title'], style['title'])
            self.assertEqual(item['trigger_phrase'], style['trigger_phrase'])
            self.assertEqual(item['recommended_strength'], style['recommended_strength'])
        self.assertEqual(returned['other.safetensors']['description'], '')
        self.assertEqual(returned['other.safetensors']['compatibility'], 'declared')
        self.assertTrue(all(styles[0]['revision'] in path for path in calls))

    async def test_curated_install_refreshes_usage_and_requires_reference_before_resolution(self):
        saved = await self.install_fixture()
        reviewed = {**saved, 'title': 'Reference edit', 'description': 'Add an input image.', 'usage': 'reference-edit',
                    'recommended_strength': .7, 'trigger_phrase': 'Adjust exposure'}
        self.module.CURATED = (reviewed,)
        item = self.module.installed()[0]
        self.assertEqual(item['recommended_strength'], .7)
        self.assertEqual(item['usage'], 'reference-edit')
        selected = [{'id': item['id'], 'strength': .7}]
        with self.assertRaisesRegex(ValueError, 'Add a reference image'):
            self.module.resolve_loras('z-image-turbo', selected)
        self.assertEqual(self.module.resolve_loras('z-image-turbo', selected, reference_count=1), [(saved['comfy_filename'], .7)])
        # Same-size artifact metadata must not inherit reviewed instructions.
        reviewed['sha256'] = 'f' * 64
        self.assertNotIn('usage', self.module.installed()[0])

    async def test_known_partial_loader_adapter_is_not_recommended_or_resolved(self):
        self.assertFalse(any(item['repo_id'].casefold() == 'limbicnation/pixel-art-lora' for item in self.module.CURATED))
        saved = await self.install_fixture()
        self.module.SPECIAL_WORKFLOWS = {**self.module.SPECIAL_WORKFLOWS, saved['repo_id'].casefold(): 'Six tensors cannot load.'}
        with self.assertRaisesRegex(ValueError, 'Six tensors cannot load'):
            self.module.resolve_loras('z-image-turbo', [{'id': saved['id'], 'strength': 1}])

    def test_catalog_current_families_have_distinct_pins_usage_and_license_notes(self):
        identities = set()
        for model in ('qwen', 'z-image-turbo', 'flux2-klein-4b', 'flux2-klein-9b'):
            items = [item for item in self.module.CURATED if item['model'] == model]
            self.assertIn(len(items), (2, 3))
            for item in items:
                self.module.check_revision(item['revision']); self.module.check_filename(item['filename'])
                key = (item['model'], item['repo_id'], item['revision'], item['filename'])
                self.assertNotIn(key, identities); identities.add(key)
                self.assertEqual(len(item['sha256']), 64)
                self.assertIn(item['usage'], ('text-to-image', 'reference-edit', 'both'))
                self.assertTrue(item['description']); self.assertTrue(item['style'])
                if model in ('qwen', 'flux2-klein-9b'):
                    self.assertTrue(item['license_note'])
        qwen = [item for item in self.module.CURATED if item['model'] == 'qwen']
        self.assertEqual(sum(item['usage'] == 'reference-edit' for item in qwen), 2)


if __name__ == '__main__': unittest.main()
