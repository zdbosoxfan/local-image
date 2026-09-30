"""Isolated layer/project lifecycle regressions; no user images or GPU jobs."""
import asyncio
import hashlib
import io
import json
import os
from pathlib import Path
import stat
import sys
import unittest
from unittest.mock import patch
import zipfile

import numpy as np
import tifffile
from fastapi import HTTPException, UploadFile
from PIL import Image

HERE=Path(__file__).resolve().parents[1]/'backend'
sys.path[:0]=[str(HERE),str(HERE.parent/'tests'/'helpers')]
import fast_inpaint  # Bind the installed v4 healer before older fixture paths.
import backend_folder_save_test as previous
previous.V3=HERE
import local_remove_project as project


class LayerProjectTests(previous.FolderSaveTests):
    async def save_project(self,session,path=None,**kwargs):
        return await self.app.save_project(self.request(native=True,csrf=False),self.app.ProjectSaveRequest(
            session_id=session['id'],revision=session['revision'],path=str(path) if path else None,**kwargs))

    async def test_display_matches_server_composite_and_toggle_never_renders(self):
        source=self.make_image(size=(128,96)); session=self.layer(self.bind(source)['id'])
        sid=session['id']; lid=session['layers'][0]['id']
        mask=Image.new('L',(4,4),128); mask.save(self.app.folder(sid)/session['layers'][0]['mask'])
        base=await self.app.base_display(sid,self.request())
        asset=await self.app.layer_display(sid,lid,self.request())
        with Image.open(base.path) as original, Image.open(asset.path) as rgba:
            composited=original.convert('RGB'); composited.paste(rgba,(10,10),rgba.getchannel('A'))
            self.assertTrue(np.array_equal(np.asarray(composited),np.asarray(self.app.render(self.app.read_session(sid)))))
            self.assertEqual(rgba.mode,'RGBA')
            self.assertEqual(rgba.size,(4,4))
        self.assertIn('immutable',asset.headers['cache-control'])
        mtime=Path(asset.path).stat().st_mtime_ns
        with patch.object(self.app,'render',side_effect=AssertionError('toggle rendered')):
            toggled=await self.app.update_layer(sid,lid,self.request(),self.app.LayerUpdate(visible=False,revision=session['revision']))
            again=await self.app.update_layer(sid,lid,self.request(),self.app.LayerUpdate(visible=False,revision=toggled['revision']))
            cached=await self.app.layer_display(sid,lid,self.request())
        self.assertEqual(again['revision'],toggled['revision'])
        self.assertEqual(Path(cached.path).stat().st_mtime_ns,mtime)
        self.assertEqual((again['layers'][0]['width'],again['layers'][0]['height']),(4,4))
        with self.assertRaises(HTTPException) as stale:
            await self.app.update_layer(sid,lid,self.request(),self.app.LayerUpdate(visible=True,revision=session['revision']))
        self.assertEqual(stale.exception.status_code,409)
        with self.assertRaises(HTTPException) as traversal:
            await self.app.layer_display(sid,'../original',self.request())
        self.assertEqual(traversal.exception.status_code,404)

    async def test_native_project_roundtrip_preserves_all_layers_and_original_without_path_authority(self):
        source=self.make_image(size=(64,48)); before=source.read_bytes()
        session=self.layer(self.bind(source)['id'])
        # FLUX returns RGBA patches even though the explicit mask controls alpha.
        patch_path=self.app.folder(session['id'])/session['layers'][0]['color']
        with Image.open(patch_path) as patch_image:
            rgba=patch_image.convert('RGBA')
        rgba.save(patch_path)
        merged=await self.app.merge_visible(session['id'],self.request(),self.app.MergeRequest(revision=session['revision']))
        discarded=await self.app.update_layer(merged['id'],merged['layers'][0]['id'],self.request(),
            self.app.LayerUpdate(discarded=True,revision=merged['revision']))
        path=self.images/'edited.lremove'
        saved=await self.save_project(discarded,path)
        self.assertTrue(saved['session']['project_saved'])
        self.assertTrue(saved['session']['has_project_path'])
        self.assertNotIn('project_path',saved['session']); self.assertNotIn('project_hash',saved['session'])
        with zipfile.ZipFile(path) as archive:
            manifest=json.loads(archive.read('manifest.json'))
            self.assertNotIn(str(source),json.dumps(manifest))
            self.assertNotIn('source_path',manifest)
            self.assertTrue(manifest['layers'][0]['discarded'])
            self.assertEqual(archive.read(manifest['original']),before)
        reopened=await self.app.open_project(self.request(native=True),self.app.OpenLocal(path=str(path)))
        loaded=reopened['session']
        self.assertNotEqual(loaded['id'],session['id'])
        self.assertFalse(loaded['can_return']); self.assertIsNone(loaded['source_name'])
        self.assertEqual(loaded['layers'],saved['session']['layers'])
        self.assertTrue(np.array_equal(np.asarray(self.app.render(self.app.read_session(loaded['id']))),
                                       np.asarray(self.app.render(self.app.read_session(session['id'])))))
        self.assertEqual(source.read_bytes(),before)
        hidden=await self.app.update_layer(loaded['id'],loaded['layers'][1]['id'],self.request(),
            self.app.LayerUpdate(visible=False,revision=loaded['revision']))
        self.assertFalse(hidden['project_saved']); self.assertTrue(hidden['project_dirty'])
        self.assertTrue(np.array_equal(np.asarray(self.app.render(self.app.read_session(hidden['id']))),np.asarray(Image.open(source))))

    async def test_16bit_project_merge_roundtrip_retains_exact_native_pixels_and_profile(self):
        source=self.images/'precision.tif'
        raw=(np.arange(64*48*3,dtype=np.uint16).reshape(48,64,3)*7)
        profile=self.app.SRGB.tobytes()
        tifffile.imwrite(source,raw,photometric='rgb',metadata=None,extratags=[(34675,'B',len(profile),profile,False)])
        session=self.layer(self.bind(source)['id'])
        merged=await self.app.merge_visible(session['id'],self.request(),self.app.MergeRequest(revision=session['revision']))
        path=self.images/'precision.lremove'; await self.save_project(merged,path)
        reopened=await self.app.open_project(self.request(native=True),self.app.OpenLocal(path=str(path)))
        data=self.app.read_session(reopened['session']['id'])
        expected=self.images/'expected.tif'; actual=self.images/'actual.tif'
        self.app.flatten(self.app.read_session(merged['id']),expected)
        self.app.flatten(data,actual)
        a,icc,_=self.app.decode_original(actual); b,_,_=self.app.decode_original(expected)
        self.assertEqual(a.dtype,np.uint16); self.assertEqual(icc,profile)
        self.assertTrue(np.array_equal(a,b)); self.assertTrue(np.array_equal(a[:10],raw[:10]))
        for layer in data['layers']: layer['visible']=False
        self.app.flatten(data,actual)
        restored,_,_=self.app.decode_original(actual)
        self.assertTrue(np.array_equal(restored,raw))

    async def test_project_save_guards_auth_revision_collisions_and_external_changes_during_render(self):
        session=self.layer(self.bind(self.make_image())['id']); path=self.images/'work.lremove'
        payload=self.app.ProjectSaveRequest(session_id=session['id'],revision=session['revision'],path=str(path))
        with self.assertRaises(HTTPException) as auth:
            await self.app.save_project(self.request(),payload)
        self.assertEqual(auth.exception.status_code,403)
        saved=await self.save_project(session,path); original=path.read_bytes()
        second=self.layer(self.bind(self.make_image('second.png'))['id'])
        with self.assertRaises(HTTPException) as collision:
            await self.save_project(second,path)
        self.assertEqual(collision.exception.status_code,409); self.assertEqual(path.read_bytes(),original)
        expected=hashlib.sha256(original).hexdigest()
        confirmed=await self.save_project(second,path,expected_hash=expected)
        self.assertTrue(confirmed['saved'])
        path.write_bytes(b'External application changed this project')
        with self.assertRaises(HTTPException) as external:
            await self.save_project(second)
        self.assertEqual(external.exception.status_code,409)
        self.assertEqual(path.read_bytes(),b'External application changed this project')
        path2=self.images/'during.lremove'; await self.save_project(session,path2)
        real=self.app.write_project
        def change_while_rendering(*args):
            result=real(*args); path2.write_bytes(b'Changed during rendering'); return result
        with patch.object(self.app,'write_project',side_effect=change_while_rendering):
            with self.assertRaises(HTTPException) as during:
                await self.save_project(session)
        self.assertEqual(during.exception.status_code,409); self.assertEqual(path2.read_bytes(),b'Changed during rendering')
        self.assertFalse(list(self.images.glob('.local-remove-project-*')))

    async def test_browser_project_download_is_unconfirmed_and_import_has_no_native_write_path(self):
        session=self.layer(self.bind(self.make_image())['id'])
        exported=await self.app.export_project(session['id'],self.request(),self.app.MergeRequest(revision=session['revision']))
        self.assertFalse(exported['session']['project_saved'])
        response=await self.app.download_project(session['id'],self.request())
        with Path(response.path).open('rb') as stream:
            imported=await self.app.upload_project(self.request(),UploadFile(stream,filename='copy.lremove'))
        self.assertFalse(imported['session']['project_saved'])
        self.assertFalse(imported['session']['has_project_path'])
        self.assertFalse(imported['session']['can_return'])
        self.assertEqual(imported['session']['layers'],session['layers'])

    async def test_close_discards_only_session_cache_resets_collection_and_keeps_saved_files(self):
        source=self.make_image(); source_bytes=source.read_bytes(); collection=await self.register()
        opened=await self.app.open_collection_entry(collection['id'],collection['entries'][0]['id'],self.request())
        edited=self.layer(opened['session']['id']); root=self.app.folder(edited['id'])
        os.chmod(root/edited['original'],stat.S_IREAD)
        path=self.images/'saved.lremove'; await self.save_project(edited,path); project_bytes=path.read_bytes()
        with self.assertRaises(HTTPException) as csrf:
            await self.app.close_session(edited['id'],self.request(csrf=False),self.app.CloseRequest(revision=edited['revision'],discard=True))
        self.assertEqual(csrf.exception.status_code,403)
        await self.app.close_session(edited['id'],self.request(),self.app.CloseRequest(revision=edited['revision'],discard=True))
        self.assertFalse(root.exists()); self.assertEqual(source.read_bytes(),source_bytes); self.assertEqual(path.read_bytes(),project_bytes)
        self.assertIsNone(self.app.read_collection(collection['id'])['entries'][0]['session_id'])
        fresh=await self.app.open_collection_entry(collection['id'],collection['entries'][0]['id'],self.request())
        self.assertEqual(fresh['session']['layers'],[]); self.assertNotEqual(fresh['session']['id'],edited['id'])
        restored=await self.app.open_project(self.request(native=True),self.app.OpenLocal(path=str(path)))
        self.assertEqual(len(restored['session']['layers']),1)

    async def test_batch_close_validates_all_before_discarding_and_accepts_unedited(self):
        first=self.bind(self.make_image()); second=self.layer(self.bind(self.make_image('second.png'))['id'])
        payload=self.app.CloseSessionsRequest(sessions=[{'id':first['id'],'revision':0},{'id':second['id'],'revision':0}],discard=True)
        with self.assertRaises(HTTPException) as stale:
            await self.app.close_sessions(self.request(),payload)
        self.assertEqual(stale.exception.status_code,409)
        self.assertTrue(self.app.folder(first['id']).is_dir()); self.assertTrue(self.app.folder(second['id']).is_dir())
        payload.sessions[1].revision=second['revision']
        await self.app.close_sessions(self.request(),payload)
        self.assertEqual(list(self.app.SESSIONS.iterdir()),[])

    async def test_close_retries_windows_preview_handle_during_staging(self):
        source=self.make_image();original=source.read_bytes();session=self.bind(source)
        session_root=self.app.folder(session['id'])
        real_rename=os.rename;attempts=[]
        def release_after_two_reads(source_path,destination):
            attempts.append((source_path,destination))
            if len(attempts)<=2:
                error=PermissionError(13,'A preview response still holds the image')
                error.winerror=5
                raise error
            return real_rename(source_path,destination)
        with patch.object(self.app.os,'rename',side_effect=release_after_two_reads),patch.object(self.app.time,'sleep') as sleep:
            await self.app.close_session(session['id'],self.request(),self.app.CloseRequest(revision=session['revision'],discard=True))
        self.assertEqual(len(attempts),3)
        self.assertEqual([call.args[0] for call in sleep.call_args_list],[0.1,0.2])
        self.assertFalse(session_root.exists())
        self.assertEqual(source.read_bytes(),original)
        self.assertFalse(list(self.app.ROOT.glob('.closing-*')))

    async def test_close_persistent_staging_lock_restores_prior_session(self):
        first=self.bind(self.make_image('first.png'));second=self.bind(self.make_image('second.png'))
        first_root=self.app.folder(first['id']);second_root=self.app.folder(second['id'])
        first_before=(first_root/'session.json').read_bytes();second_before=(second_root/'session.json').read_bytes()
        real_rename=os.rename;attempts=[]
        def second_is_locked(source,destination):
            attempts.append((Path(source),Path(destination)))
            if Path(source)==second_root:
                error=PermissionError(13,'A persistent Windows reader holds the second session')
                error.winerror=32
                raise error
            return real_rename(source,destination)
        with patch.object(self.app.os,'rename',side_effect=second_is_locked),patch.object(self.app.time,'sleep') as sleep:
            with self.assertRaises(PermissionError):
                self.app.discard_sessions([first['id'],second['id']])
        self.assertEqual(sum(source==second_root for source,_ in attempts),5)
        self.assertEqual(sleep.call_count,4)
        self.assertEqual(attempts[-1][1],first_root,'The staged first session is rolled back')
        self.assertEqual((first_root/'session.json').read_bytes(),first_before)
        self.assertEqual((second_root/'session.json').read_bytes(),second_before)
        self.assertFalse(list(self.app.ROOT.glob('.closing-*')))

    async def test_hostile_project_archives_leave_no_sessions_or_outside_files(self):
        session=self.layer(self.bind(self.make_image())['id']); clean=self.images/'clean.lremove'; await self.save_project(session,clean)
        with zipfile.ZipFile(clean) as archive: contents={item.filename:archive.read(item) for item in archive.infolist()}
        baseline=set(self.app.SESSIONS.iterdir())
        manifest=json.loads(contents['manifest.json'])
        variants=[]
        for name in ('../escaped.txt','C:/escaped.txt','nested/file.txt'):
            variant=dict(contents); variant[name]=b'outside'; variants.append((name,variant))
        bad_manifest=dict(manifest); bad_manifest['source_path']=str(self.images/'hijack.png')
        variants.append(('absolute source authority',{**contents,'manifest.json':json.dumps(bad_manifest).encode()}))
        layer_manifest=json.loads(contents['manifest.json']); layer_manifest['layers'][0]['x']=999999
        variants.append(('geometry',{**contents,'manifest.json':json.dumps(layer_manifest).encode()}))
        variants.append(('corrupt asset',{**contents,manifest['original']:b'broken asset'}))
        variants.append(('bad integrity hash',{**contents,manifest['original']:bytes(len(contents[manifest['original']]))}))
        variants.append(('huge compression',{**contents,'bomb.bin':bytes(3*1024**2)}))
        for label,variant in variants:
            path=self.images/'invalid.lremove'
            with zipfile.ZipFile(path,'w',compression=zipfile.ZIP_DEFLATED) as archive:
                for name,value in variant.items(): archive.writestr(name,value)
            with self.subTest(label=label),self.assertRaises(HTTPException):
                await self.app.open_project(self.request(native=True),self.app.OpenLocal(path=str(path)))
            self.assertEqual(set(self.app.SESSIONS.iterdir()),baseline)
        for mode in ('duplicate','symlink'):
            path=self.images/'invalid.lremove'
            with zipfile.ZipFile(path,'w') as archive:
                for name,value in contents.items():
                    if mode=='symlink' and name==manifest['original']:
                        info=zipfile.ZipInfo(name); info.create_system=3; info.external_attr=(stat.S_IFLNK|0o777)<<16
                        archive.writestr(info,value)
                    else: archive.writestr(name,value)
                if mode=='duplicate': archive.writestr('base.png',contents['base.png'])
            with self.subTest(mode=mode),self.assertRaises(HTTPException):
                await self.app.open_project(self.request(native=True),self.app.OpenLocal(path=str(path)))
            self.assertEqual(set(self.app.SESSIONS.iterdir()),baseline)
        self.assertFalse((self.app.ROOT/'escaped.txt').exists())


if __name__=='__main__':
    unittest.main(verbosity=2)
