"""Real local pixels and portable documents for shared, selectable image layers."""
import base64
import copy
import io
from pathlib import Path
import sys
import unittest
from unittest.mock import patch, AsyncMock

import numpy as np
import tifffile
from PIL import Image
from fastapi import HTTPException, UploadFile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
sys.path.insert(0, str(Path(__file__).resolve().parent / 'helpers'))
import backend_folder_save_test as fixtures
import layer_stack
from local_remove_project import write_project, extract_project


def encoded(image):
    stream = io.BytesIO(); image.save(stream, format='PNG')
    return base64.b64encode(stream.getvalue()).decode('ascii')


class LayerStackTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.fixture = fixtures.FolderSaveTests(); self.fixture.setUp()
        self.app = self.fixture.app; self.request = self.fixture.request

    def tearDown(self): self.fixture.tearDown()

    async def session(self, native=False):
        if native:
            path = self.fixture.images / 'native.tif'
            raw = np.full((32, 40, 3), (12001, 32002, 64003), dtype=np.uint16)
            tifffile.imwrite(path, raw, photometric='rgb', metadata=None)
        else: path = self.fixture.make_image(color=(170, 50, 20))
        data = self.fixture.bind(path)
        return await self.app.enable_stack(data['id'], self.request(), self.app.MergeRequest(revision=data['revision']))

    async def update(self, data, lid, **settings):
        return await self.app.update_stack_layer(data['id'], lid, self.request(), self.app.StackUpdate(revision=data['revision'], **settings))

    async def test_preview_reuses_revision_and_original_cache(self):
        data = await self.session()
        with patch.object(self.app, 'render', wraps=self.app.render) as render:
            first = await self.app.preview(data['id'], self.request(), full=True)
            second = await self.app.preview(data['id'], self.request(), full=True)
            self.assertEqual(first.path, second.path)
            self.assertEqual(render.call_count, 1)
            changed = await self.update(data, 'original', visible=False)
            third = await self.app.preview(changed['id'], self.request(), full=True)
            self.assertNotEqual(first.path, third.path)
            self.assertEqual(render.call_count, 2)

    async def test_chunked_compositing_preserves_eight_and_sixteen_bit_pixels(self):
        rng = np.random.default_rng(7)
        for dtype in (np.uint8, np.uint16):
            maximum = np.iinfo(dtype).max
            back = rng.integers(0, maximum+1, (137, 19, 4), dtype=dtype)
            front = rng.integers(0, maximum+1, back.shape, dtype=dtype)
            front[:64, :, 3] = 0
            back[:10, :, 3] = 0
            front[64:128, :, 3] = maximum
            fa = front[..., 3:4].astype(np.float64)/maximum
            ba = back[..., 3:4].astype(np.float64)/maximum
            alpha = fa+ba*(1-fa)
            numerator = front[..., :3]*fa+back[..., :3]*ba*(1-fa)
            rgb = np.divide(numerator, alpha, out=np.zeros_like(numerator), where=alpha>0)
            expected = np.concatenate((np.rint(rgb), np.rint(alpha*maximum)), axis=2).clip(0,maximum).astype(dtype)
            np.testing.assert_array_equal(self.app.stack_model.over(back, front), expected)

    async def test_stack_scaffold_preserves_revision_and_saved_status_until_actual_edit(self):
        data=await self.session()
        self.assertEqual(data['revision'],0)
        self.assertFalse(data['dirty'])
        self.assertFalse(data['edited'])
        self.assertFalse(data['stack_can_undo'])
        self.assertEqual(len(data['layer_stack']),1)
        stored=self.app.read_session(data['id'])
        stored.update(saved_revision=0,project_saved_revision=0)
        self.app.write_session(self.app.folder(data['id']),stored)
        data=await self.app.enable_stack(data['id'],self.request(),self.app.MergeRequest(revision=0))
        self.assertTrue(data['saved']); self.assertTrue(data['project_saved'])
        data=await self.update(data,'original',visible=False)
        self.assertEqual(data['revision'],1)
        self.assertTrue(data['dirty']); self.assertTrue(data['project_dirty'])
        self.assertTrue(data['stack_can_undo'])

    async def cutout(self, data, box=(10, 8, 28, 25)):
        mask = Image.new('L', (40, 32)); mask.paste(255, box)
        return await self.app.refine_cutout(data['id'], self.request(), self.app.CutoutRefine(revision=data['revision'], mask=encoded(mask), operation='replace'))

    async def test_two_cpu_heals_accumulate_in_one_editable_layer_and_undo(self):
        data = await self.session()
        data = await self.app.create_stack_layer(data['id'], self.request(), self.app.StackCreate(revision=data['revision'], name='Dust cleanup'))
        lid = data['selected_layer_id']
        for box in ((5, 6, 9, 10), (20, 18, 24, 22)):
            mask = Image.new('L', (40, 32)); mask.paste(255, box)
            data = await self.app.remove(data['id'], self.request(), self.app.RemoveRequest(revision=data['revision'], model='heal', heal_method='telea', mask=encoded(mask), target_layer_id=lid))
        self.assertEqual(len(data['layer_stack']), 2)
        self.assertEqual(len(data['layer_stack'][1]['patch_ids']), 2)
        self.assertEqual(len(data['layers']), 2)
        self.assertEqual(data['layer_stack'][1]['name'], 'Dust cleanup')
        data = await self.app.undo_stack(data['id'], self.request(), self.app.MergeRequest(revision=data['revision']))
        self.assertEqual(len(data['layer_stack'][1]['patch_ids']), 1)
        data = await self.app.redo_stack(data['id'], self.request(), self.app.MergeRequest(revision=data['revision']))
        self.assertEqual(len(data['layer_stack'][1]['patch_ids']), 2)
        for change in ({'visible':False}, {'visible':True,'locked':True}):
            data = await self.update(data, lid, **change)
            with self.assertRaises(HTTPException) as rejected:
                await self.app.remove(data['id'], self.request(), self.app.RemoveRequest(revision=data['revision'],model='heal',mask=encoded(mask),target_layer_id=lid))
            self.assertEqual(rejected.exception.status_code, 409)

    async def test_cutout_original_visibility_background_layer_transform_and_export(self):
        data = await self.cutout(await self.session()); lid = data['selected_layer_id']
        self.assertTrue(data['layer_stack'][0]['visible'])
        root = self.app.folder(data['id'])
        self.assertEqual(self.app.render(self.app.read_session(data['id'])).getpixel((0,0)), (170,50,20,255))
        data = await self.update(data, 'original', visible=False)
        self.assertEqual(self.app.render(self.app.read_session(data['id'])).getpixel((0,0))[3], 0)
        data = self.app.set_background_image(root, self.app.read_session(data['id']), Image.new('RGB',(40,32),'blue'), 'Blue backdrop.png', layer_id=lid)
        bgid = data['selected_layer_id']
        self.assertEqual([n['kind'] for n in data['layer_stack']], ['original','image','cutout'])
        data = await self.update(data, lid, transform={'offset_x':4,'offset_y':2})
        image = self.app.render(self.app.read_session(data['id']))
        self.assertEqual(image.getpixel((10,8)), (0,0,255,255))
        self.assertEqual(image.getpixel((16,12)), (170,50,20,255))
        data = await self.update(data, bgid, discarded=True)
        output = root/'real-output.png'; self.app.flatten(self.app.read_session(data['id']), output)
        with Image.open(output) as exported:
            self.assertEqual(exported.getpixel((0,0))[3], 0)
            self.assertEqual(exported.getpixel((16,12)), (170,50,20,255))
        data = await self.update(data, bgid, discarded=False, index=2)
        self.assertEqual(self.app.render(self.app.read_session(data['id'])).getpixel((16,12)), (0,0,255,255))

    async def test_selected_cutout_refinement_does_not_change_other_cutout(self):
        data = await self.cutout(await self.session()); first = data['selected_layer_id']
        data = await self.cutout(data, (2,2,6,6)); second = data['selected_layer_id']
        before_second = copy.deepcopy(data['layer_stack'][-1])
        erase = Image.new('L',(40,32)); erase.paste(255,(10,8,15,15))
        data = await self.app.refine_cutout(data['id'], self.request(), self.app.CutoutRefine(revision=data['revision'],layer_id=first,mask=encoded(erase),operation='erase'))
        self.assertEqual(next(n for n in data['layer_stack'] if n['id']==second), before_second)
        with Image.open(self.app.folder(data['id'])/next(n for n in data['layer_stack'] if n['id']==first)['cutout']['alpha']) as alpha:
            self.assertEqual(alpha.getpixel((11,9)),0)
            self.assertEqual(alpha.getpixel((20,20)),255)

    async def test_native_precision_project_roundtrip_and_alpha(self):
        data = await self.cutout(await self.session(native=True)); lid=data['selected_layer_id']
        data = await self.update(data,'original',visible=False)
        data = await self.update(data,lid,transform={'offset_x':2})
        root=self.app.folder(data['id']); stored=self.app.read_session(data['id'])
        target=root/'layered-16bit.tif'; self.app.flatten(stored,target)
        pixels=tifffile.imread(target)
        self.assertEqual(pixels.dtype,np.uint16)
        self.assertEqual(pixels[12,16].tolist(),[12001,32002,64003,65535])
        self.assertEqual(pixels[0,0,3],0)
        project=root/'layers.lremove'; manifest=write_project(root,stored,project)
        self.assertEqual(manifest['version'],3)
        reopened=self.app.import_project(project)['session']; reopened_data=self.app.read_session(reopened['id'])
        second=self.app.folder(reopened['id'])/'roundtrip.tif'; self.app.flatten(reopened_data,second)
        np.testing.assert_array_equal(tifffile.imread(second),pixels)
        self.assertEqual(reopened['layer_stack'],data['layer_stack'])

    async def test_legacy_cutout_migration_preserves_appearance(self):
        source=self.fixture.make_image(); data=self.fixture.bind(source)
        data=await self.cutout(data)
        before=np.asarray(self.app.render(self.app.read_session(data['id'])))
        data=await self.app.enable_stack(data['id'],self.request(),self.app.MergeRequest(revision=data['revision']))
        np.testing.assert_array_equal(np.asarray(self.app.render(self.app.read_session(data['id']))),before)
        self.assertFalse(data['layer_stack'][0]['visible'])

    async def test_legacy_rotated_cutout_shadow_migrates_with_only_final_rounding(self):
        source=self.fixture.make_image(); data=self.fixture.bind(source); data=await self.cutout(data)
        data=await self.app.update_cutout(data['id'],self.request(),self.app.CutoutUpdate(revision=data['revision'],transform={'rotation':30,'scale':0.7,'offset_x':1},shadow={'enabled':True,'offset_x':6,'offset_y':2,'blur':0,'opacity':1}))
        before=np.asarray(self.app.render(self.app.read_session(data['id'])))
        data=await self.app.enable_stack(data['id'],self.request(),self.app.MergeRequest(revision=data['revision']))
        after=np.asarray(self.app.render(self.app.read_session(data['id'])))
        self.assertLessEqual(np.max(np.abs(after.astype(int)-before.astype(int))),1)
        self.assertEqual([n['name'] for n in data['layer_stack']],['Original','Cutout shadow','Cutout'])

    async def test_revision_csrf_missing_and_unsafe_metadata_rejected(self):
        data=await self.session()
        for request in (self.request(csrf=False),self.request(origin='https://outside.example')):
            with self.assertRaises(HTTPException) as rejected:
                await self.app.create_stack_layer(data['id'],request,self.app.StackCreate(revision=data['revision']))
            self.assertEqual(rejected.exception.status_code,403)
        for revision,lid,expected in ((data['revision']+1,'original',409),(data['revision'],'unknown',404)):
            with self.assertRaises(HTTPException) as rejected:
                await self.app.update_stack_layer(data['id'],lid,self.request(),self.app.StackUpdate(revision=revision,visible=False))
            self.assertEqual(rejected.exception.status_code,expected)
        with self.assertRaises(HTTPException): await self.update(data,'original',transform={'offset_x':3})
        with self.assertRaises(HTTPException): await self.update(data,'original',locked=False,transform={'unknown':4})
        with self.assertRaises(ValueError):
            layer_stack.validate_stack([layer_stack.node('original','Original'),layer_stack.node('image','Unsafe',source='../x.png')])
        with self.assertRaises(ValueError):
            layer_stack.validate_stack([layer_stack.node('original','Original'),layer_stack.node('retouch','Missing',patch_ids=['0'*32])],[])

    async def test_repair_added_to_transformed_target_stays_at_brushed_canvas_position(self):
        data=await self.session()
        data=await self.app.create_stack_layer(data['id'],self.request(),self.app.StackCreate(revision=data['revision']))
        lid=data['selected_layer_id']; data=await self.update(data,lid,transform={'offset_x':5,'offset_y':3})
        mask=Image.new('L',(40,32)); mask.paste(255,(12,12,16,16))
        repair={'x':12,'y':12,'color':encoded(Image.new('RGB',(4,4),'green')),'mask':encoded(Image.new('L',(4,4),255))}
        with patch.object(self.app,'heal_image',return_value=repair):
            data=await self.app.remove(data['id'],self.request(),self.app.RemoveRequest(revision=data['revision'],model='heal',target_layer_id=lid,mask=encoded(mask)))
        pixels=self.app.render(self.app.read_session(data['id']))
        self.assertEqual(pixels.getpixel((13,13)),(0,128,0,255))
        self.assertEqual(pixels.getpixel((18,16)),(170,50,20,255))
        self.assertEqual((data['layers'][0]['x'],data['layers'][0]['y']),(7,9))

    async def test_stack_merge_and_undo_preserve_transparency(self):
        data=await self.cutout(await self.session()); data=await self.update(data,'original',visible=False)
        before=np.asarray(self.app.render(self.app.read_session(data['id'])))
        data=await self.app.merge_visible(data['id'],self.request(),self.app.MergeRequest(revision=data['revision']))
        np.testing.assert_array_equal(np.asarray(self.app.render(self.app.read_session(data['id']))),before)
        self.assertEqual(data['layer_stack'][-1]['kind'],'image')
        data=await self.app.undo_stack(data['id'],self.request(),self.app.MergeRequest(revision=data['revision']))
        np.testing.assert_array_equal(np.asarray(self.app.render(self.app.read_session(data['id']))),before)

    async def test_add_mask_to_moved_selected_source_and_refine_uses_canvas_coordinates(self):
        data=await self.session()
        data=await self.update(data,'original',locked=False,transform={'offset_x':6})
        data=await self.app.refine_cutout(data['id'],self.request(),self.app.CutoutRefine(revision=data['revision'],layer_id='original',operation='replace',mask=encoded(Image.new('L',(40,32),255))))
        lid=data['selected_layer_id']; self.assertEqual(data['layer_stack'][-1]['transform'],layer_stack.DEFAULT_TRANSFORM)
        data=await self.update(data,'original',visible=False)
        data=await self.update(data,lid,transform={'offset_x':2})
        erase=Image.new('L',(40,32)); erase.paste(255,(10,10,15,15))
        data=await self.app.refine_cutout(data['id'],self.request(),self.app.CutoutRefine(revision=data['revision'],layer_id=lid,operation='erase',mask=encoded(erase)))
        pixels=self.app.render(self.app.read_session(data['id']))
        self.assertEqual(pixels.getpixel((5,5))[3],0)
        self.assertEqual(pixels.getpixel((9,9)),(170,50,20,255))
        self.assertEqual(pixels.getpixel((12,12))[3],0)

    async def test_display_asset_shadow_and_transform_match_stack_pixels(self):
        data=await self.cutout(await self.session()); lid=data['selected_layer_id']
        data=await self.update(data,'original',visible=False)
        data=await self.app.update_cutout(data['id'],self.request(),self.app.CutoutUpdate(revision=data['revision'],layer_id=lid,shadow={'enabled':True,'blur':1,'offset_x':3,'offset_y':2},transform={'offset_x':4,'offset_y':1}))
        response=await self.app.stack_layer_display(data['id'],lid,self.request())
        with Image.open(response.path) as display:
            shifted=Image.new('RGBA',display.size); shifted.paste(display,(4,1))
        np.testing.assert_array_equal(np.asarray(shifted),np.asarray(self.app.render(self.app.read_session(data['id']))))
        original=await self.app.stack_layer_display(data['id'],'original',self.request())
        with Image.open(original.path) as image:
            self.assertEqual(image.getpixel((0,0)),(170,50,20,255))

    async def test_display_asset_reused_for_canvas_only_changes_and_undo(self):
        data = await self.cutout(await self.session()); lid = data['selected_layer_id']
        key = next(node['display_key'] for node in data['layer_stack'] if node['id'] == lid)
        with patch.object(self.app.stack_model, 'native_layer', wraps=self.app.stack_model.native_layer) as render:
            first = await self.app.stack_layer_display(data['id'], lid, self.request(), r=key)
            self.assertIn('immutable', first.headers['cache-control'])
            data = await self.update(data, 'original', visible=False)
            data = await self.update(data, lid, name='Moved subject', opacity=.4, transform={'offset_x':5, 'rotation':12})
            self.assertEqual(next(node['display_key'] for node in data['layer_stack'] if node['id'] == lid), key)
            second = await self.app.stack_layer_display(data['id'], lid, self.request(), r=key)
            data = await self.app.undo_stack(data['id'], self.request(), self.app.MergeRequest(revision=data['revision']))
            third = await self.app.stack_layer_display(data['id'], lid, self.request(), r=key)
            self.assertEqual(first.path, second.path)
            self.assertEqual(first.path, third.path)
            self.assertEqual(render.call_count, 1)
            legacy = await self.app.stack_layer_display(data['id'], lid, self.request(), r=str(data['revision']))
            self.assertEqual(legacy.headers['cache-control'], 'no-store')

    async def test_display_asset_invalidated_only_by_its_pixel_edits(self):
        data = await self.cutout(await self.session()); lid = data['selected_layer_id']
        paths = [str((await self.app.stack_layer_display(data['id'], lid, self.request())).path)]
        original_key = data['layer_stack'][0]['display_key']
        for settings in ({'feather':2}, {'shadow':{'enabled':True, 'blur':1, 'offset_x':3}}):
            data = await self.app.update_cutout(data['id'], self.request(), self.app.CutoutUpdate(revision=data['revision'], layer_id=lid, **settings))
            paths.append(str((await self.app.stack_layer_display(data['id'], lid, self.request())).path))
        erase = Image.new('L', (40,32)); erase.paste(255, (10,8,16,16))
        data = await self.app.refine_cutout(data['id'], self.request(), self.app.CutoutRefine(revision=data['revision'], layer_id=lid, mask=encoded(erase), operation='erase'))
        key = next(node['display_key'] for node in data['layer_stack'] if node['id'] == lid)
        changed = await self.app.stack_layer_display(data['id'], lid, self.request(), r=key)
        paths.append(str(changed.path))
        self.assertEqual(len(set(paths)), 4)
        self.assertEqual(data['layer_stack'][0]['display_key'], original_key)
        stale = await self.app.stack_layer_display(data['id'], lid, self.request(), r='0'*32)
        self.assertEqual(stale.headers['cache-control'], 'no-store')

    async def test_display_sprite_excludes_opacity_without_changing_native_export(self):
        for native in (False, True):
            with self.subTest(native=native):
                data = await self.cutout(await self.session(native=native)); lid = data['selected_layer_id']
                data = await self.update(data, 'original', visible=False)
                data = await self.update(data, lid, opacity=.25, transform={'offset_x':2})
                response = await self.app.stack_layer_display(data['id'], lid, self.request())
                with Image.open(response.path) as sprite:
                    self.assertEqual(sprite.getpixel((14,12))[3], 255)
                    self.assertEqual(sprite.getpixel((0,0))[3], 0)
                stored = self.app.read_session(data['id']); root = self.app.folder(data['id'])
                raw, icc, _ = self.app.decode_original(root / stored['original'])
                pixels = self.app.stack_model.render_native(stored, root, raw, icc, self.app.decode_original)
                self.assertEqual(pixels[12,16,3], round(np.iinfo(raw.dtype).max*.25))
                self.assertEqual(pixels.dtype, raw.dtype)

    async def test_display_identity_tracks_retouch_pixel_dependencies(self):
        data = await self.session(); data = self.fixture.layer(data['id'])
        stored = self.app.read_session(data['id']); patch_data = stored['layers'][0]
        node = layer_stack.node('retouch', 'Repair', patch_ids=[patch_data['id']])
        key = layer_stack.display_key(stored, node)
        for field, value in (('name', 'Renamed'), ('visible', False), ('opacity', .2), ('transform', {'offset_x':5, 'offset_y':0, 'scale':1, 'rotation':0})):
            modified = copy.deepcopy(node); modified[field] = value
            self.assertEqual(layer_stack.display_key(stored, modified), key)
        for field, value in (('x', 11), ('color', 'new-color.png'), ('mask', 'new-mask.png'), ('snapshot', 'new-snapshot.tif'), ('visible', False), ('discarded', True)):
            modified = copy.deepcopy(stored); modified['layers'][0][field] = value
            self.assertNotEqual(layer_stack.display_key(modified, node), key)
        unrelated = copy.deepcopy(stored); unrelated['layers'].append({**patch_data, 'id':'0'*32, 'x':20})
        self.assertEqual(layer_stack.display_key(unrelated, node), key)

    async def test_project_rejects_hash_valid_but_truncated_layer_png(self):
        data=await self.session(); root=self.app.folder(data['id'])
        pixels=np.random.default_rng(17).integers(0,256,(32,40,3),dtype=np.uint8)
        data=self.app.set_background_image(root,self.app.read_session(data['id']),Image.fromarray(pixels),'Noise.png')
        source=root/data['layer_stack'][-1]['source']; contents=source.read_bytes(); source.write_bytes(contents[:len(contents)//2])
        project=root/'broken.lremove'; write_project(root,self.app.read_session(data['id']),project)
        with self.assertRaises((OSError,ValueError)):
            self.app.import_project(project)

    async def test_all_background_sources_work_on_original_only_stack(self):
        background=Image.new('RGBA',(40,32),(10,100,200,255))
        contents=base64.b64decode(encoded(background))
        for method in ('generate','upload','folder','generated','stock'):
            with self.subTest(method=method):
                data=await self.session()
                if method=='generate':
                    with patch('qwen_image.run_qwen_image',new=AsyncMock(return_value=background)):
                        result=await self.app.generate_background(data['id'],self.request(),self.app.CutoutRequest(revision=data['revision'],prompt='empty blue wall'))
                elif method=='upload':
                    result=await self.app.upload_background(data['id'],self.request(),revision=data['revision'],file=UploadFile(filename='Background.png',file=io.BytesIO(contents)),layer_id=None)
                elif method=='folder':
                    self.fixture.make_image('library-background.png',(10,100,200))
                    library=await self.app.register_background_folder(self.request(native=True),self.app.OpenLocal(path=str(self.fixture.images)))
                    entry=next(item for item in library['entries'] if item['name']=='library-background.png')
                    result=await self.app.apply_library_background(data['id'],self.request(),self.app.LibraryBackground(revision=data['revision'],library_id=library['id'],entry_id=entry['id']))
                elif method=='generated':
                    source=self.fixture.bind(self.fixture.make_image('generation.png',(10,100,200)))
                    result=await self.app.use_generated_background(data['id'],self.request(),self.app.GeneratedBackground(revision=data['revision'],generated_session_id=source['id']))
                else:
                    credit={'provider':'openverse','asset_id':'photo-1','title':'Background','creator':'Photographer','creator_url':'https://example.com/photographer','source_url':'https://example.com/photo','license':'CC0','license_url':'https://creativecommons.org/publicdomain/zero/1.0/','attribution':'Public domain'}
                    result=(await self.app.import_stock_image(contents,credit,target='background',session_id=data['id'],revision=data['revision']))['session']
                self.assertEqual([item['kind'] for item in result['layer_stack']],['original','image'])
                self.assertNotIn('cutout',result)
                self.assertEqual(self.app.render(self.app.read_session(data['id'])).getpixel((0,0)),(10,100,200,255))

if __name__=='__main__': unittest.main()
