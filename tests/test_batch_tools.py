"""Real CPU treatment/export checks; GPU execution alone uses a controlled fake."""
import asyncio
import importlib.util
import json
from pathlib import Path
import sys
import types
import threading
import unittest
from unittest.mock import AsyncMock, patch
import uuid
import zipfile

import numpy as np
from fastapi import HTTPException
from PIL import Image
from pydantic import ValidationError
import tifffile

BACKEND = Path(__file__).resolve().parents[1] / 'backend'
sys.path[:0] = [str(BACKEND), str(Path(__file__).resolve().parent / 'helpers')]
import backend_folder_save_test as fixtures
fixtures.V3 = BACKEND
from cutout_composite import initial_cutout, transform_matrix
from stock_attribution import collect_attributions


class BatchTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.fixture = fixtures.FolderSaveTests(); self.fixture.setUp(); self.editor = self.fixture.app
        self.module_patch = patch.dict(sys.modules, {'local_remove': self.editor}); self.module_patch.start()
        spec = importlib.util.spec_from_file_location('batch_under_test', BACKEND / 'batch_tools.py')
        self.batch = importlib.util.module_from_spec(spec); sys.modules[spec.name] = self.batch; spec.loader.exec_module(self.batch)

    async def asyncTearDown(self):
        for task in list(self.batch.tasks.values()):
            task.cancel()
            try:
                await task
            except asyncio.CancelledError:
                pass
        self.module_patch.stop(); self.fixture.tearDown()

    def request(self, **kwargs):
        return self.fixture.request(**kwargs)

    def image(self, name='product.png', size=(80, 60), bounds=(25, 15, 55, 45), cutout=True):
        data = self.fixture.bind(self.fixture.make_image(name, (160, 80, 20), size))
        if cutout:
            root = self.editor.folder(data['id']); alpha = Image.new('L', size); alpha.paste(255, bounds)
            data['cutout'] = initial_cutout(self.editor.save_cutout_alpha(root, alpha)); data['revision'] = 1
            self.editor.write_session(root, data)
        return self.editor.read_session(data['id'])

    async def treatment(self, data, format='png', background=None):
        state = data['cutout']; state['transform'].update(scale=.8, rotation=12, offset_x=8, offset_y=-3)
        state['shadow'].update(enabled=True, opacity=.4, blur=2, offset_x=3, offset_y=4)
        if background:
            asset='cutout-'+uuid.uuid4().hex+'-background.png'; Image.new('RGB',(31,23),(10,40,90)).save(self.editor.folder(data['id'])/asset)
            state['background']={'mode':'image','color':'#ffffff','asset':asset,'name':'Studio', 'attribution':background}
        self.editor.write_session(self.editor.folder(data['id']), data)
        return await self.batch.save_treatment(self.request(), self.batch.SaveTreatment(name='Warm studio 水彩',session_id=data['id'],revision=data['revision'],format=format))

    async def create(self, data, **kwargs):
        return await self.batch.create_job(self.request(), self.batch.CreateJob(sessions=[self.batch.SessionSelection(session_id=item['id'],revision=item['revision']) for item in data], **kwargs))

    async def finish(self, identifier):
        task=self.batch.tasks.get(identifier)
        if task:
            await task
            await asyncio.sleep(0)
        return self.batch.job(identifier)

    def test_portable_unicode_filenames_and_windows_device_names(self):
        self.assertLessEqual(len(self.batch.export_stem('{name}', '水' * 160 + '.png', 1).encode('utf-8')), 240)
        self.assertEqual(self.batch.export_stem('{name}', 'CON.jpg', 1), '_CON')
        self.assertEqual(self.batch.export_stem('{name}', 'a' * 130 + '.jpg', 1), 'a' * 130)

    async def test_treatment_normalizes_different_masks_and_keeps_each_subject(self):
        first=self.image(); second=self.image('other.png',(160,120),(65,35,105,95))
        preset=await self.treatment(first); hashes={item['id']:self.editor.file_hash(self.editor.folder(item['id'])/item['original']) for item in (first,second)}
        queue=await self.create([first,second],treatment_id=preset['id']); value=await self.finish(queue['id'])
        self.assertEqual([item['status'] for item in value['items']],['ready','ready'],value['items'])
        definition=self.batch.treatment(preset['id']); directory=self.batch.own_directory('jobs',queue['id'])
        for item, source in zip(value['items'],(first,second)):
            data=json.loads((directory/item['id']/'snapshot.json').read_text()); state=data['cutout']
            self.assertEqual(state['alpha'],source['cutout']['alpha']); self.assertNotIn('source_path',data)
            with Image.open(directory/item['id']/state['alpha']) as alpha: bounds=alpha.getbbox()
            matrix=transform_matrix((data['width'],data['height']),state['transform']);cx,cy=(bounds[0]+bounds[2]-1)/2,(bounds[1]+bounds[3]-1)/2
            self.assertAlmostEqual((matrix[0,0]*cx+matrix[0,1]*cy+matrix[0,2])/data['width'],definition['placement']['center_x'])
            self.assertAlmostEqual((matrix[1,0]*cx+matrix[1,1]*cy+matrix[1,2])/data['height'],definition['placement']['center_y'])
            self.assertEqual(self.editor.read_session(source['id'])['cutout'],source['cutout'])
            self.assertEqual(hashes[source['id']],self.editor.file_hash(self.editor.folder(source['id'])/source['original']))
        self.assertEqual(len(list(self.editor.SESSIONS.glob('*/session.json'))),2,'No batch copies in editor recovery')

    async def test_reviewed_public_settings_and_export_contract_are_immutable(self):
        data=self.image();preset=await self.treatment(data,format='png');queue=await self.create([data],treatment_id=preset['id'],qwen_variant='bf16');value=await self.finish(queue['id']);public=self.batch.public_job(value)
        self.assertEqual(public['treatment_id'],preset['id']);self.assertEqual(public['treatment_name'],preset['name']);self.assertEqual(public['format'],'png');self.assertEqual(public['qwen_variant'],'bf16')
        await self.batch.delete_treatment(preset['id'],self.request());self.assertEqual(self.batch.public_job(self.batch.job(queue['id']))['treatment_id'],preset['id'],'Deleted presets retain their immutable queue settings')
        with self.assertRaises(ValidationError):self.batch.ExportSelection(format='jpg')
        await self.batch.export_zip(queue['id'],self.request(),self.batch.ExportSelection());value=await self.finish(queue['id']);self.assertTrue(value['items'][0]['output_name'].endswith('.png'))

    async def test_background_and_stock_credits_survive_preset_deletion_and_zip(self):
        credit={'provider':'openverse','asset_id':'studio','title':'Studio','creator':'Photographer','creator_url':'https://www.flickr.com/photos/person/','source_url':'https://www.flickr.com/photos/person/1/','license':'CC BY 4.0','license_url':'https://creativecommons.org/licenses/by/4.0/','attribution':'Studio by Photographer, CC BY 4.0.'}
        first=self.image();second=self.image('other.png');saved=await self.treatment(first,background=credit)
        queue=await self.create([second],treatment_id=saved['id']);await self.finish(queue['id'])
        await self.batch.delete_treatment(saved['id'],self.request())
        await self.batch.export_zip(queue['id'],self.request(),self.batch.ExportSelection());value=await self.finish(queue['id'])
        item=value['items'][0]; self.assertEqual(item['status'],'exported'); self.assertEqual(item['credits'],[credit])
        directory=self.batch.own_directory('jobs',queue['id'])
        with zipfile.ZipFile(directory/'exports.zip') as archive:
            self.assertIn(item['output_name'],archive.namelist());self.assertIn(item['credits_name'],archive.namelist());self.assertIn('export-report.json',archive.namelist())
            self.assertIn('CC BY 4.0',archive.read(item['credits_name']).decode())
        self.assertEqual(collect_attributions(json.loads((directory/item['id']/'snapshot.json').read_text())),[credit])

    async def test_no_treatment_exports_own_layers_and_precision(self):
        path=self.fixture.images/'precision.tif';raw=np.full((20,30,3),(12001,32002,64003),dtype=np.uint16);tifffile.imwrite(path,raw,photometric='rgb')
        data=self.fixture.bind(path);queue=await self.create([data]);await self.finish(queue['id']);await self.batch.export_zip(queue['id'],self.request(),self.batch.ExportSelection());value=await self.finish(queue['id'])
        item=value['items'][0];self.assertEqual(item['export_bit_depth'],16);np.testing.assert_array_equal(tifffile.imread(self.batch.own_directory('jobs',queue['id'])/'exports'/item['output_name']),raw)

    async def test_explicit_png_allows_eight_bit_and_keeps_alpha(self):
        data=self.image(); queue=await self.create([data],format='png');await self.finish(queue['id']);await self.batch.export_zip(queue['id'],self.request(),self.batch.ExportSelection());value=await self.finish(queue['id']);item=value['items'][0]
        with Image.open(self.batch.own_directory('jobs',queue['id'])/'exports'/item['output_name']) as image:self.assertEqual(image.mode,'RGBA');self.assertEqual(image.getchannel('A').getextrema(),(0,255))
        self.assertEqual(item['export_bit_depth'],8)

    async def test_custom_numbered_names_work_for_folder_and_zip_without_replacing_sources(self):
        for mode in ('folder', 'zip'):
            first = self.image(mode + '-product.png'); second = self.image(mode + '-other.png')
            queue = await self.create([first, second], format='png'); await self.finish(queue['id'])
            originals = {item['id']: self.editor.file_hash(self.editor.folder(item['id']) / item['original']) for item in (first, second)}
            destination = self.fixture.images / ('exports-' + mode); destination.mkdir()
            (destination / ('Catalog-001-' + mode + '-product.png')).write_bytes(b'keep me')
            template = 'Catalog-{index}-{name}'
            if mode == 'folder':
                await self.batch.export_folder(queue['id'], self.request(native=True, csrf=False), self.batch.ExportFolder(path=str(destination), naming_template=template))
            else:
                await self.batch.export_zip(queue['id'], self.request(), self.batch.ExportSelection(naming_template=template))
            value = await self.finish(queue['id'])
            self.assertTrue(all(item['status'] == 'exported' for item in value['items']))
            self.assertTrue(value['items'][0]['output_name'].startswith('Catalog-001-'))
            self.assertEqual(value['items'][1]['output_name'], 'Catalog-002-' + mode + '-other.png')
            self.assertEqual((destination / ('Catalog-001-' + mode + '-product.png')).read_bytes(), b'keep me')
            self.batch.live_jobs.clear()
            self.assertEqual(self.batch.job(queue['id'])['naming_template'], template)
            for item in (first, second):
                self.assertEqual(originals[item['id']], self.editor.file_hash(self.editor.folder(item['id']) / item['original']))

    async def test_invalid_filename_patterns_do_not_start_export(self):
        data = self.image(); queue = await self.create([data], format='png'); await self.finish(queue['id'])
        for template in ('../{name}', '{unknown}', 'a\\b', '.', '   ', 'x\x00'):
            with self.subTest(template=template), self.assertRaises(HTTPException):
                await self.batch.export_zip(queue['id'], self.request(), self.batch.ExportSelection(naming_template=template))
        self.assertFalse(self.batch.job(queue['id'])['running'])
        self.assertEqual(self.batch.job(queue['id'])['items'][0]['status'], 'ready')

    async def test_full_preview_and_original_comparison_keep_native_dimensions(self):
        data=self.image(size=(960,720),bounds=(200,100,700,600));queue=await self.create([data]);value=await self.finish(queue['id']);item=value['items'][0]
        response=await self.batch.preview_item(queue['id'],item['id'],self.request(),full=True)
        with Image.open(response.path) as image:self.assertEqual(image.size,(960,720));self.assertEqual(image.getpixel((0,0))[3],0)
        original=await self.batch.preview_item(queue['id'],item['id'],self.request(),full=True,original=True)
        with Image.open(original.path) as image:self.assertEqual(image.size,(960,720));self.assertEqual(image.mode,'RGB')

    async def test_resume_recognizes_a_published_file_after_interrupted_manifest_commit(self):
        data=self.image();queue=await self.create([data],format='png');await self.finish(queue['id']);await self.batch.export_zip(queue['id'],self.request(),self.batch.ExportSelection());value=await self.finish(queue['id']);original_name=value['items'][0]['output_name'];value['items'][0]['status']='ready';value['items'][0].pop('output_name');value.update(phase='paused',running=False);self.batch.save_job(value)
        await self.batch.resume_job(queue['id'],self.request());value=await self.finish(queue['id']);self.assertEqual(value['items'][0]['output_name'],original_name);self.assertEqual(len(list((self.batch.own_directory('jobs',queue['id'])/'exports').glob('*.png'))),1)

    async def test_transparent_jpeg_fails_individually_other_images_export(self):
        first=self.image();second=self.image('opaque.png',cutout=False);queue=await self.create([first,second],format='jpg');await self.finish(queue['id']);await self.batch.export_zip(queue['id'],self.request(),self.batch.ExportSelection());value=await self.finish(queue['id'])
        self.assertEqual([item['status'] for item in value['items']],['failed','exported']);self.assertIn('transparency',value['items'][0]['error']);self.assertTrue(value['archive_ready'])

    async def test_missing_cutout_requires_opt_in_and_never_borrows_product_mask(self):
        first=self.image();preset=await self.treatment(first);second=self.image('uncut.png',cutout=False);queue=await self.create([second],treatment_id=preset['id']);value=await self.finish(queue['id']);self.assertEqual(value['items'][0]['status'],'needs-cutout');self.assertNotIn('cutout',self.editor.read_session(second['id']))

    async def test_optional_qwen_serializes_with_generation_and_uses_independent_alpha(self):
        first=self.image();preset=await self.treatment(first);second=self.image('uncut.png',cutout=False);lock=self.fixture.fixture.main.generation_lock;await lock.acquire();rgba=Image.new('RGBA',(80,60),(0,0,0,0));rgba.paste((120,40,30,255),(20,10,60,50))
        with patch('qwen_image.run_qwen_image',AsyncMock(return_value=rgba)) as model:
            queue=await self.create([second],treatment_id=preset['id'],prepare_cutouts=True);await asyncio.sleep(.05);self.assertEqual(model.await_count,0);lock.release();value=await self.finish(queue['id']);self.assertEqual(value['items'][0]['status'],'ready');self.assertEqual(model.await_count,1)
        snapshot=json.loads((self.batch.own_directory('jobs',queue['id'])/value['items'][0]['id']/'snapshot.json').read_text());self.assertNotEqual(snapshot['cutout']['alpha'],first['cutout']['alpha']);self.assertNotIn('cutout',self.editor.read_session(second['id']))

    async def test_revision_conflict_before_prepare_does_not_touch_image(self):
        data=self.image();before=data['revision'];data['revision']+=1;self.editor.write_session(self.editor.folder(data['id']),data)
        queue=await self.batch.create_job(self.request(),self.batch.CreateJob(sessions=[self.batch.SessionSelection(session_id=data['id'],revision=before)]));value=await self.finish(queue['id']);self.assertEqual(value['items'][0]['status'],'conflict');self.assertEqual(self.editor.read_session(data['id'])['revision'],before+1)

    async def test_revision_conflict_after_preview_cannot_export_stale_snapshot(self):
        data=self.image();queue=await self.create([data]);await self.finish(queue['id']);data['revision']+=1;self.editor.write_session(self.editor.folder(data['id']),data);await self.batch.export_zip(queue['id'],self.request(),self.batch.ExportSelection());value=await self.finish(queue['id']);self.assertEqual(value['items'][0]['status'],'conflict');self.assertFalse(value.get('archive_ready'))

    async def test_reviewed_snapshot_survives_explicit_editor_document_close(self):
        data=self.image();queue=await self.create([data]);await self.finish(queue['id']);await self.editor.close_session(data['id'],self.request(),self.editor.CloseRequest(revision=data['revision'],discard=True))
        await self.batch.export_zip(queue['id'],self.request(),self.batch.ExportSelection());value=await self.finish(queue['id']);self.assertEqual(value['items'][0]['status'],'exported')

    async def test_cancel_resumes_remaining_images_without_duplicate_export(self):
        first=self.image();second=self.image('other.png');queue=await self.create([first,second]);await self.finish(queue['id']);entered=asyncio.Event();release=asyncio.Event();original=self.batch.export_item
        async def delay(value,item):entered.set();await release.wait();await original(value,item)
        with patch.object(self.batch,'export_item',delay):
            await self.batch.export_zip(queue['id'],self.request(),self.batch.ExportSelection());await entered.wait();await self.batch.cancel_job(queue['id'],self.request());release.set();value=await self.finish(queue['id'])
        self.assertEqual(value['phase'],'paused');self.assertEqual([item['status'] for item in value['items']],['ready','ready']);await self.batch.resume_job(queue['id'],self.request());value=await self.finish(queue['id']);self.assertEqual([item['status'] for item in value['items']],['exported','exported']);self.assertEqual(len(list((self.batch.own_directory('jobs',queue['id'])/'exports').glob('*.png'))),2)

    async def test_native_folder_export_unique_names_and_no_browser_path_authority(self):
        data=self.image();queue=await self.create([data],format='png');await self.finish(queue['id']);out=self.fixture.images/'exports';out.mkdir();existing=out/'product-local-image.png';existing.write_bytes(b'keep')
        with self.assertRaises(HTTPException) as rejected:await self.batch.export_folder(queue['id'],self.request(),self.batch.ExportFolder(path=str(out)))
        self.assertEqual(rejected.exception.status_code,403);await self.batch.export_folder(queue['id'],self.request(native=True,csrf=False),self.batch.ExportFolder(path=str(out)));value=await self.finish(queue['id']);self.assertEqual(existing.read_bytes(),b'keep');self.assertEqual(value['items'][0]['output_name'],'product-local-image-2.png')
        await self.batch.delete_job(queue['id'],self.request());self.assertTrue((out/'product-local-image-2.png').is_file());self.assertTrue(self.editor.folder(data['id']).is_dir())

    async def test_only_selected_review_rows_are_exported(self):
        queue=await self.create([self.image(),self.image('other.png')]);value=await self.finish(queue['id']);selected=[value['items'][1]['id']];await self.batch.export_zip(queue['id'],self.request(),self.batch.ExportSelection(item_ids=selected));value=await self.finish(queue['id']);self.assertEqual([item['status'] for item in value['items']],['ready','exported'])

    async def test_queue_recovery_reads_interrupted_state_and_clear_is_separate(self):
        queue=await self.create([self.image()]);value=await self.finish(queue['id']);value.update(running=True,phase='preparing');value['items'][0]['status']='preparing';self.batch.save_job(value);recovered=self.batch.job(queue['id']);self.assertFalse(recovered['running']);self.assertEqual(recovered['phase'],'paused');self.assertEqual(recovered['items'][0]['status'],'pending');await self.batch.resume_job(queue['id'],self.request());await self.finish(queue['id']);await self.batch.delete_job(queue['id'],self.request());self.assertFalse(self.batch.own_directory('jobs',queue['id'],False).exists())

    async def test_requests_are_bounded_strict_and_local(self):
        data=self.image()
        with self.assertRaises(ValidationError):self.batch.CreateJob(sessions=[{'session_id':data['id'],'revision':1}]*101)
        with self.assertRaises(ValidationError):self.batch.CreateJob(sessions=[{'session_id':data['id'],'revision':1}]*2)
        with self.assertRaises(ValidationError):self.batch.CreateJob(sessions=[{'session_id':data['id'],'revision':1}],output_directory='C:/unsafe')
        with self.assertRaises(HTTPException) as rejection:await self.batch.create_job(self.request(csrf=False),self.batch.CreateJob(sessions=[{'session_id':data['id'],'revision':1}]))
        self.assertEqual(rejection.exception.status_code,403)

    async def test_collection_selection_is_authoritative_and_broken_entry_is_individual(self):
        good=self.fixture.make_image('good.png');bad=self.fixture.images/'bad.png';bad.write_bytes(b'invalid');collection=self.editor.register_collection([good,bad],'Products');queue=await self.batch.create_job(self.request(),self.batch.CreateJob(collection_id=collection['id'],entry_ids=[item['id'] for item in collection['entries']]));value=await self.finish(queue['id']);self.assertEqual([item['status'] for item in value['items']],['ready','failed']);self.assertEqual(self.editor.read_collection(collection['id'])['index'],0)
        with self.assertRaises(HTTPException):await self.batch.create_job(self.request(),self.batch.CreateJob(collection_id=collection['id'],entry_ids=[str(uuid.uuid4())]))

    async def test_collection_queue_is_durable_before_import_and_publishes_bindings_once(self):
        paths = [self.fixture.make_image(f'lazy-{index}.png') for index in range(12)]
        collection = self.editor.register_collection(paths, 'Unopened photos')
        with (patch.object(self.editor, 'reuse_or_create_session', wraps=self.editor.reuse_or_create_session) as imports,
              patch.object(self.editor, 'read_collection', wraps=self.editor.read_collection) as reads,
              patch.object(self.editor, 'write_collection', wraps=self.editor.write_collection) as writes):
            queue = await self.batch.create_job(self.request(), self.batch.CreateJob(
                collection_id=collection['id'], entry_ids=[entry['id'] for entry in collection['entries']]))
            self.assertEqual(imports.call_count, 0)
            self.assertTrue(all(item['session_id'] is None and item['revision'] is None for item in queue['items']))
            self.assertTrue(all('collection_source' not in item and 'path' not in item for item in queue['items']))
            stored = self.batch.batch_store.load(self.batch.own_directory('jobs', queue['id']), self.batch.linked)
            self.assertTrue(all(item['status'] == 'pending' and item['session_id'] is None for item in stored['items']))
            value = await self.finish(queue['id'])
            self.assertEqual(imports.call_count, len(paths))
            self.assertTrue(all(item['status'] == 'ready' for item in value['items']), value['items'])
            self.assertLessEqual(reads.call_count, 3)
            self.assertEqual(writes.call_count, 1)
        saved = self.editor.read_collection(collection['id'])
        self.assertEqual([entry['session_id'] for entry in saved['entries']], [item['session_id'] for item in value['items']])
        self.assertEqual(saved['index'], 0)

    async def test_cancel_during_import_leaves_later_collection_images_unopened_and_resume_keeps_first(self):
        paths = [self.fixture.make_image(f'cancel-import-{index}.png') for index in range(4)]
        collection = self.editor.register_collection(paths, 'Cancel imports')
        entered, release = asyncio.Event(), threading.Event()
        loop, original = asyncio.get_running_loop(), self.editor.reuse_or_create_session
        imported = []
        def pause_first(path):
            imported.append(path)
            if len(imported) == 1:
                loop.call_soon_threadsafe(entered.set)
                if not release.wait(3):
                    raise RuntimeError('Cancellation could not respond while importing.')
            return original(path)
        with patch.object(self.editor, 'reuse_or_create_session', pause_first):
            try:
                queue = await self.batch.create_job(self.request(), self.batch.CreateJob(
                    collection_id=collection['id'], entry_ids=[entry['id'] for entry in collection['entries']]))
                await asyncio.wait_for(entered.wait(), 2)
                status = await self.batch.get_job(queue['id'], self.request())
                self.assertTrue(status['running'])
                self.assertEqual(status['items'][0]['status'], 'preparing')
                await self.batch.cancel_job(queue['id'], self.request())
            finally:
                release.set()
            value = await self.finish(queue['id'])
            self.assertEqual(value['phase'], 'paused')
            self.assertEqual([item['status'] for item in value['items']], ['ready', 'pending', 'pending', 'pending'])
            self.assertEqual(imported, paths[:1])
            sid = value['items'][0]['session_id']
            await self.batch.resume_job(queue['id'], self.request())
            value = await self.finish(queue['id'])
            self.assertEqual(imported, paths)
            self.assertEqual(value['items'][0]['session_id'], sid)
            self.assertTrue(all(item['status'] == 'ready' for item in value['items']))

    async def test_interrupted_snapshot_resumes_persisted_collection_session_without_import(self):
        collection = self.editor.register_collection([self.fixture.make_image('interrupted-import.png')], 'Interrupted')
        async def interrupt_after_binding(value, item):
            await self.batch.bind_collection_item(value, item)
            raise asyncio.CancelledError()
        with patch.object(self.batch, 'prepare_item', interrupt_after_binding):
            queue = await self.batch.create_job(self.request(), self.batch.CreateJob(
                collection_id=collection['id'], entry_ids=[collection['entries'][0]['id']]))
            with self.assertRaises(asyncio.CancelledError):
                await self.finish(queue['id'])
            await asyncio.sleep(0)
        recovered = self.batch.job(queue['id'])
        self.assertEqual(recovered['phase'], 'paused')
        self.assertEqual(recovered['items'][0]['status'], 'pending')
        sid = recovered['items'][0]['session_id']
        self.assertTrue(sid)
        with patch.object(self.editor, 'reuse_or_create_session', side_effect=AssertionError('Already imported')):
            await self.batch.resume_job(queue['id'], self.request())
            value = await self.finish(queue['id'])
        self.assertEqual(value['items'][0]['status'], 'ready', value['items'])
        self.assertEqual(value['items'][0]['session_id'], sid)
        self.assertEqual(self.editor.read_collection(collection['id'])['entries'][0]['session_id'], sid)

    async def test_lazy_import_rechecks_collection_path_and_uses_later_opened_edits(self):
        paths = [self.fixture.make_image(f'authority-{index}.png') for index in range(2)]
        collection = self.editor.register_collection(paths, 'Authority')
        with patch.object(self.batch, 'launch'):
            queue = await self.batch.create_job(self.request(), self.batch.CreateJob(
                collection_id=collection['id'], entry_ids=[entry['id'] for entry in collection['entries']]))
        first = await self.editor.open_collection_entry(collection['id'], collection['entries'][0]['id'], self.request())
        data = self.editor.read_session(first['session']['id'])
        data['revision'] += 1
        self.editor.write_session(self.editor.folder(data['id']), data)
        changed = self.editor.read_collection(collection['id'])
        changed['entries'][1]['path'] = str(self.fixture.make_image('replacement.png'))
        self.editor.write_collection(changed)
        self.batch.launch(queue['id'], 'prepare')
        value = await self.finish(queue['id'])
        self.assertEqual(value['items'][0]['status'], 'ready', value['items'])
        self.assertEqual(value['items'][0]['session_id'], data['id'])
        self.assertEqual(value['items'][0]['revision'], data['revision'])
        self.assertEqual(value['items'][1]['status'], 'conflict')
        self.assertIn('changed after', value['items'][1]['error'])

    async def test_collection_selected_open_session_preserves_revision_and_newer_navigation_binding(self):
        path = self.fixture.make_image('opened-revision.png')
        collection = self.editor.register_collection([path], 'Existing edits')
        opened = await self.editor.open_collection_entry(collection['id'], collection['entries'][0]['id'], self.request())
        data = self.editor.read_session(opened['session']['id'])
        with patch.object(self.batch, 'launch'):
            queue = await self.batch.create_job(self.request(), self.batch.CreateJob(
                collection_id=collection['id'], entry_ids=[collection['entries'][0]['id']]))
        data['revision'] += 1
        self.editor.write_session(self.editor.folder(data['id']), data)
        replacement = self.editor.read_session(self.editor.create_session(path, path.name, path)['id'])
        changed = self.editor.read_collection(collection['id'])
        changed['entries'][0]['session_id'] = replacement['id']
        self.editor.write_collection(changed)
        self.batch.launch(queue['id'], 'prepare')
        value = await self.finish(queue['id'])
        self.assertEqual(value['items'][0]['status'], 'conflict')
        self.assertEqual(value['items'][0]['session_id'], data['id'])
        self.assertEqual(self.editor.read_collection(collection['id'])['entries'][0]['session_id'], replacement['id'])

    async def test_active_batch_cannot_be_started_or_cleared_twice(self):
        original=self.batch.prepare_item;entered=asyncio.Event();release=asyncio.Event()
        async def delay(value,item):entered.set();await release.wait();await original(value,item)
        with patch.object(self.batch,'prepare_item',delay):
            queue=await self.create([self.image()]);await entered.wait()
            with self.assertRaises(HTTPException) as rejection:await self.create([self.image('second.png')])
            self.assertEqual(rejection.exception.status_code,409)
            with self.assertRaises(HTTPException):await self.batch.delete_job(queue['id'],self.request())
            release.set();await self.finish(queue['id'])

    async def test_per_job_storage_limit_reports_failure_without_export(self):
        with patch.object(self.batch,'MAX_STORAGE',1):queue=await self.create([self.image()]);value=await self.finish(queue['id'])
        self.assertEqual(value['items'][0]['status'],'failed');self.assertIn('storage limit',value['items'][0]['error'])

    async def test_more_than_100_real_cutouts_prepare_and_export_without_truncation(self):
        images = [self.image(f'product-{index}.png', (12, 10), (2, 2, 10, 8)) for index in range(137)]
        queue = await self.create(images, format='png', prepare_cutouts=True)
        value = await self.finish(queue['id'])
        self.assertEqual(len(value['items']), 137)
        self.assertTrue(all(item['status'] == 'ready' for item in value['items']))
        identifiers = [item['id'] for item in value['items']]
        await self.batch.export_zip(queue['id'], self.request(), self.batch.ExportSelection(item_ids=identifiers))
        value = await self.finish(queue['id'])
        self.assertTrue(all(item['status'] == 'exported' for item in value['items']), value['message'])
        directory = self.batch.own_directory('jobs', queue['id'])
        with zipfile.ZipFile(directory / 'exports.zip') as archive:
            self.assertEqual(len([name for name in archive.namelist() if name.endswith('.png')]), 137)
            report = json.loads(archive.read('export-report.json'))
            self.assertEqual(len(report['items']), 137)
        self.assertTrue(self.editor.folder(images[-1]['id']).is_dir())

    async def test_native_folder_export_accepts_more_than_100_reviewed_images(self):
        images = [self.image(f'native-{index}.png', (12, 10), (2, 2, 10, 8)) for index in range(103)]
        queue = await self.create(images, format='png', prepare_cutouts=True)
        value = await self.finish(queue['id'])
        destination = self.fixture.images / 'large-native-export'
        destination.mkdir()
        selected = [item['id'] for item in value['items']][:101]
        await self.batch.export_folder(queue['id'], self.request(native=True, csrf=False),
                                       self.batch.ExportFolder(path=str(destination), item_ids=selected))
        value = await self.finish(queue['id'])
        self.assertEqual(sum(item['status'] == 'exported' for item in value['items']), 101, value['message'])
        self.assertEqual(sum(item['status'] == 'ready' for item in value['items']), 2)
        self.assertEqual(len(list(destination.glob('*.png'))), 101)

    async def test_legacy_queue_migrates_and_interrupted_work_resumes(self):
        data = self.image()
        queue = await self.create([data], format='png')
        value = await self.finish(queue['id'])
        directory = self.batch.own_directory('jobs', queue['id'])
        (directory / self.batch.batch_store.DATABASE).unlink()
        (directory / 'job.json').write_text(json.dumps(value))
        reopened = self.batch.job(queue['id'])
        reopened.update(running=True, phase='preparing')
        reopened['items'][0]['status'] = 'preparing'
        self.batch.save_job(reopened)
        self.assertEqual(json.loads((directory / 'job.json').read_text())['version'], 2)
        recovered = self.batch.job(queue['id'])
        self.assertEqual(recovered['phase'], 'paused')
        self.assertEqual(recovered['items'][0]['status'], 'pending')
        await self.batch.resume_job(queue['id'], self.request())
        value = await self.finish(queue['id'])
        self.assertEqual(value['items'][0]['status'], 'ready')
        self.assertTrue((directory / value['items'][0]['id'] / 'snapshot.json').is_file())

    async def test_large_worker_does_not_reload_or_walk_the_whole_queue_per_image(self):
        entries = [{'id': str(uuid.uuid4()), 'name': f'Product {index}', 'status': 'pending'} for index in range(301)]
        async def prepare(value, item):
            item.update(status='ready', prepared=True)
        with (patch.object(self.batch, 'resolve_entries', AsyncMock(return_value=(entries, 'Large queue'))),
              patch.object(self.batch, 'prepare_item', prepare),
              patch.object(self.batch, 'bytes_used', wraps=self.batch.bytes_used) as scans,
              patch.object(self.batch.batch_store, 'load', wraps=self.batch.batch_store.load) as reads):
            queue = await self.batch.create_job(self.request(), self.batch.CreateJob(sessions=[{'session_id':str(uuid.uuid4()),'revision':0}]))
            value = await self.finish(queue['id'])
            self.assertEqual(len(value['items']), 301)
            self.assertTrue(all(item['status'] == 'ready' for item in value['items']))
            self.assertLessEqual(scans.call_count, 2)
            self.assertLessEqual(reads.call_count, 2)

    async def test_large_queue_cancel_and_resume_keeps_completed_work(self):
        entries = [{'id': str(uuid.uuid4()), 'name': f'Product {index}', 'status': 'pending'} for index in range(301)]
        entered, release = asyncio.Event(), asyncio.Event()
        calls = []
        async def prepare(value, item):
            calls.append(item['id'])
            if len(calls) == 1:
                entered.set()
                await release.wait()
            item.update(status='ready', prepared=True)
        with (patch.object(self.batch, 'resolve_entries', AsyncMock(return_value=(entries, 'Large queue'))),
              patch.object(self.batch, 'prepare_item', prepare)):
            queue = await self.batch.create_job(self.request(), self.batch.CreateJob(sessions=[{'session_id':str(uuid.uuid4()),'revision':0}]))
            await entered.wait()
            await self.batch.cancel_job(queue['id'], self.request())
            release.set()
            value = await self.finish(queue['id'])
            self.assertEqual(value['phase'], 'paused')
            self.assertEqual(sum(item['status'] == 'ready' for item in value['items']), 1)
            await self.batch.resume_job(queue['id'], self.request())
            value = await self.finish(queue['id'])
            self.assertEqual(len(calls), 301)
            self.assertTrue(all(item['status'] == 'ready' for item in value['items']))

    async def test_cache_clear_can_remove_owned_interrupted_database_staging_files(self):
        queue = await self.create([self.image()])
        await self.finish(queue['id'])
        directory = self.batch.own_directory('jobs', queue['id'])
        staging = directory / ('.queue-' + uuid.uuid4().hex + '.sqlite3')
        staging.write_bytes(b'incomplete owned queue metadata')
        staging.with_name(staging.name + '-journal').write_bytes(b'interrupted journal')
        await self.batch.delete_job(queue['id'], self.request())
        self.assertFalse(directory.exists())

    async def test_zip_packaging_allows_status_and_cancel_requests_to_respond(self):
        queue = await self.create([self.image()], format='png')
        await self.finish(queue['id'])
        entered, release = asyncio.Event(), threading.Event()
        loop = asyncio.get_running_loop()
        write = zipfile.ZipFile.write
        def paused_write(archive, *args, **kwargs):
            loop.call_soon_threadsafe(entered.set)
            if not release.wait(3):
                raise RuntimeError('The event loop could not respond during ZIP packaging.')
            return write(archive, *args, **kwargs)
        with patch.object(zipfile.ZipFile, 'write', paused_write):
            try:
                await self.batch.export_zip(queue['id'], self.request(), self.batch.ExportSelection())
                await asyncio.wait_for(entered.wait(), 2)
                status = await self.batch.get_job(queue['id'], self.request())
                self.assertTrue(status['running'])
                self.assertEqual(status['message'], 'Creating ZIP from completed exports.')
                await self.batch.cancel_job(queue['id'], self.request())
            finally:
                release.set()
            value = await self.finish(queue['id'])
        self.assertEqual(value['phase'], 'paused')
        self.assertTrue(value['archive_ready'])

    async def test_unsafe_asset_copy_and_cache_contents_are_rejected(self):
        target=self.fixture.fixture.directory/'safe';target.mkdir()
        for name in ('../source.png','C:\\outside.png','/outside.png'):
            with self.assertRaises(ValueError):self.batch.safe_copy(self.fixture.images,name,target)
        preset=await self.treatment(self.image());folder=self.batch.own_directory('treatments',preset['id']);(folder/'customer-project.lremove').write_text('never delete')
        with self.assertRaises(HTTPException):await self.batch.delete_treatment(preset['id'],self.request())
        self.assertTrue((folder/'customer-project.lremove').is_file());self.assertTrue((folder/'treatment.json').is_file())

    async def test_queue_cache_clear_refuses_any_unexpected_saved_project(self):
        data=self.image();queue=await self.create([data]);await self.finish(queue['id']);directory=self.batch.own_directory('jobs',queue['id']);project=directory/'saved-product.lremove';project.write_bytes(b'customer project')
        with self.assertRaises(HTTPException):await self.batch.delete_job(queue['id'],self.request())
        self.assertEqual(project.read_bytes(),b'customer project');self.assertTrue((directory/'job.json').is_file())


if __name__=='__main__':unittest.main()
