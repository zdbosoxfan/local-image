"""Isolated AI setup safety tests. No GPU jobs, real downloads, or user files."""
import asyncio
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import AsyncMock, Mock, patch

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE / 'backend'))
import managed_ai as ai
from app_paths import read_config, write_config


def create_runtime(root, portable=True):
    code = root / 'ComfyUI' if portable else root
    code.mkdir(parents=True)
    (code / 'main.py').write_text('# synthetic fixture')
    (code / 'folder_paths.py').write_text('# synthetic fixture')
    (code / 'comfy').mkdir()
    python = root / 'python_embeded' / 'python.exe' if portable else code / '.venv' / 'Scripts' / 'python.exe'
    python.parent.mkdir(parents=True)
    python.write_bytes(b'synthetic interpreter, never executed')
    return code, python


class Content:
    def __init__(self, body):
        self.body = body

    async def iter_chunked(self, size):
        yield self.body[:3]
        yield self.body[3:]


class Response:
    def __init__(self, body=b'', status=200, headers=None):
        self.content = Content(body)
        self.status = status
        self.headers = headers or {}

    async def __aenter__(self):
        return self

    async def __aexit__(self, *args):
        return False


class Client:
    def __init__(self, responses):
        self.responses = iter(responses)
        self.calls = []

    async def __aenter__(self):
        return self

    async def __aexit__(self, *args):
        return False

    def get(self, address, **kwargs):
        self.calls.append((address, kwargs))
        return next(self.responses)


class ManagedAITests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='local-remove-ai-')
        self.root = Path(self.temporary.name)
        self.env = patch.dict(os.environ, {'LOCAL_REMOVE_DATA_DIR': str(self.root / 'profile'),
            'LOCAL_REMOVE_COMFY_CANDIDATES': '', 'APPDATA': str(self.root / 'roaming'),
            'LOCALAPPDATA': str(self.root / 'local')})
        self.env.start()
        self.home = patch.object(ai.Path, 'home', return_value=self.root / 'home')
        self.home.start()
        self.manager = ai.SetupManager()

    def tearDown(self):
        self.home.stop()
        self.env.stop()
        self.temporary.cleanup()

    def artifact(self, body=b'publisher bytes'):
        return {'name': 'test.safetensors', 'url': 'https://huggingface.co/author/model/resolve/revision/file',
                'bytes': len(body), 'sha256': hashlib.sha256(body).hexdigest()}

    def member(self, name, **options):
        return SimpleNamespace(filename=name, is_symlink=False, is_file=True, is_directory=False,
                               uncompressed=1, **options)

    def object_info(self):
        info = {name: {'input': {'required': {}}} for name in ai.FLUX_NODES}
        for node, key, artifact in (('UNETLoader', 'unet_name', ai.FLUX_FILES[0]),
                                   ('CLIPLoader', 'clip_name', ai.FLUX_FILES[1]),
                                   ('VAELoader', 'vae_name', ai.FLUX_FILES[2]),
                                   ('LoraLoaderModelOnly', 'lora_name', ai.FLUX_FILES[3])):
            info[node]['input']['required'][key] = [[artifact['name']]]
        info['CLIPLoader']['input']['required']['type'] = [['flux2', 'stable_diffusion']]
        return info

    def test_readiness_requires_actual_workflow_nodes_encoder_type_and_visible_files(self):
        info = self.object_info()
        self.assertTrue(ai.workflow_readiness(info)['ready'])
        del info['ReferenceLatent']
        self.assertIn('missing FLUX Klein support', ai.workflow_readiness(info)['reason'])
        info = self.object_info()
        info['CLIPLoader']['input']['required']['type'] = [['stable_diffusion']]
        self.assertFalse(ai.workflow_readiness(info)['ready'])
        info = self.object_info()
        info['UNETLoader']['input']['required']['unet_name'] = [['other-model.safetensors']]
        result = ai.workflow_readiness(info)
        self.assertFalse(result['ready'])
        self.assertIn('flux-2-klein-base-4b.safetensors', result['reason'])
        self.assertIn('restart ComfyUI with the selected model folder', result['reason'])

    async def test_running_service_is_not_ready_until_gpu_and_loader_files_are_usable(self):
        queue = {'queue_running': [], 'queue_pending': []}
        stats = {'devices': [{'type': 'cuda', 'name': 'Test GPU'}]}
        with patch.object(ai, 'comfy_request', AsyncMock(side_effect=[stats, queue, self.object_info()])):
            result = await ai.service_state()
        self.assertTrue(result['running']); self.assertTrue(result['ready'])
        with patch.object(ai, 'comfy_request', AsyncMock(side_effect=[stats, queue, {}])):
            result = await ai.service_state()
        self.assertTrue(result['running']); self.assertFalse(result['ready'])
        self.assertIn('missing FLUX', result['reason'])

    async def test_status_uses_installation_published_during_service_probe(self):
        code, python = create_runtime(self.root / 'new-portable')
        self.assertIsNone(ai.selected_installation())
        self.manager.job = {'status': 'running', 'action': 'install'}
        probe_started = asyncio.Event()
        probe_finished = asyncio.Event()
        async def pending_service_probe():
            probe_started.set()
            await probe_finished.wait()
            return {'running': False, 'busy': False}
        with patch.object(ai, 'service_state', side_effect=pending_service_probe):
            pending_status = asyncio.create_task(self.manager.status())
            await probe_started.wait()
            ai.configure(comfy_directory=str(code))
            self.manager.update(status='complete')
            probe_finished.set()
            result = await pending_status
        self.assertEqual(result['job']['status'], 'complete')
        self.assertEqual(result['installation']['path'], str(code))
        self.assertEqual(result['installation']['python'], str(python))
        self.assertTrue(result['service']['can_start'])

    async def test_start_does_not_restart_running_instance_with_missing_model_paths(self):
        with (patch.object(ai, 'service_state', AsyncMock(return_value={'running': True, 'ready': False,
                  'reason': 'Restart ComfyUI with the selected model folder.'})),
              patch.object(ai.subprocess, 'Popen', side_effect=AssertionError('must not start another process'))):
            with self.assertRaisesRegex(ai.SetupError, 'selected model folder'):
                await self.manager.start()

    def test_catalog_keeps_flux_encoder_and_comfy_converted_adapter(self):
        self.assertEqual(len(ai.FLUX_FILES), 4)
        self.assertEqual(sum(item['bytes'] for item in ai.FLUX_FILES), 16208337988)
        self.assertIn('qwen_3_4b.safetensors', [item['name'] for item in ai.FLUX_FILES])
        self.assertTrue(ai.FLUX_FILES[-1]['url'].endswith('_comfy_converted.safetensors'))
        for item in (*ai.FLUX_FILES, ai.COMFY_RELEASE):
            self.assertRegex(item['sha256'], r'^[0-9a-f]{64}$')
            ai.checked_download_url(item['url'])
        ai.checked_download_url('https://us.aws.cdn.hf.co/verified-redirect')
        for bad in ('https://us.aws.cdn.hf.co.evil.example/model', 'https://evilxethub.hf.co/model',
                    'http://us.aws.cdn.hf.co/model', 'https://huggingface.co@evil.example/model'):
            with self.assertRaises(ai.SetupError):
                ai.checked_download_url(bad)

    def test_detect_portable_source_and_separate_desktop_environment(self):
        portable, python = create_runtime(self.root / 'home' / 'ComfyUI_windows_portable')
        source, source_python = create_runtime(self.root / 'home' / 'ComfyUI-Installs' / 'Photo' / 'ComfyUI', False)
        code = self.root / 'local' / 'Programs' / 'Comfy Desktop' / 'resources' / 'ComfyUI'
        create_runtime(code, False)
        base = self.root / 'desktop-data'
        interpreter = base / '.venv' / 'Scripts' / 'python.exe'
        interpreter.parent.mkdir(parents=True)
        interpreter.write_bytes(b'fixture')
        config = self.root / 'roaming' / 'ComfyUI' / 'extra_models_config.yaml'
        config.parent.mkdir(parents=True)
        config.write_text(json.dumps({'desktop': {'base_path': str(base)}}))
        found = ai.detect_installations()
        self.assertEqual(len(found), 3)
        desktop = next(item for item in found if item['kind'] == 'desktop')
        self.assertEqual(desktop['python'], str(interpreter))
        self.assertEqual(desktop['base_directory'], str(base))
        self.assertIn(str(source), [item['path'] for item in found])
        self.assertIn(str(portable), [item['path'] for item in found])

    def test_configure_uses_detected_id_preserves_fields_and_rejects_arbitrary_target(self):
        code, python = create_runtime(self.root / 'home' / 'ComfyUI_windows_portable')
        write_config({'comfy_port': 8189, 'desktop_preference': 'preserved'})
        item = ai.detect_installations()[0]
        ai.configure(installation_id=item['id'])
        selected = ai.selected_installation()
        self.assertEqual(selected['path'], str(code))
        self.assertEqual(selected['python'], str(python))
        stored = json.loads((self.root / 'profile' / 'config.json').read_text())
        self.assertEqual(stored['desktop_preference'], 'preserved')
        self.assertEqual(stored['comfy_port'], 8189)
        for payload in ({'installation_id': 'forged'}, {'comfy_directory': str(self.root)},
                        {'model_directory': '\\\\server\\share'}, {'comfy_port': 51247}):
            with self.assertRaises(ai.SetupError):
                ai.configure(**payload)

    def test_archive_rejects_traversal_links_duplicates_and_oversize(self):
        for name in ('../outside', '/outside', 'C:/outside', 'folder/../../outside', 'file:stream', 'bad. '):
            with self.assertRaises(ai.SetupError, msg=name):
                ai.validate_archive_members([self.member(name)], self.root)
        link = self.member('link')
        link.is_symlink = True
        with self.assertRaises(ai.SetupError):
            ai.validate_archive_members([link], self.root)
        with self.assertRaises(ai.SetupError):
            ai.validate_archive_members([self.member('A'), self.member('a')], self.root)
        huge = self.member('huge'); huge.uncompressed = ai.MAX_EXTRACT_BYTES + 1
        with self.assertRaises(ai.SetupError):
            ai.validate_archive_members([huge], self.root)

    def test_extract_real_archive_validates_layout(self):
        import py7zr
        portable = self.root / 'archive-source' / 'ComfyUI_windows_portable'
        create_runtime(portable)
        archive = self.root / 'portable.7z'
        with py7zr.SevenZipFile(archive, 'w') as package:
            package.writeall(portable, 'ComfyUI_windows_portable')
        target = self.root / 'extract'; target.mkdir()
        extracted = ai.extract_portable(archive, target)
        self.assertEqual(extracted, target / 'ComfyUI_windows_portable')
        self.assertTrue(ai.installation(extracted)['startable'])

    def test_extract_bcj2_archive_with_bundled_decoder(self):
        import py7zr
        archive = HERE / 'tests' / 'fixtures' / 'comfy-portable-bcj2.7z'
        with py7zr.SevenZipFile(archive, 'r') as package:
            self.assertTrue(any('BCJ2' in method for method in package.archiveinfo().method_names))
        target = self.root / 'bcj2-extract'; target.mkdir()
        extracted = ai.extract_portable(archive, target)
        self.assertTrue(ai.installation(extracted)['startable'])
        self.assertEqual((extracted / 'ComfyUI' / 'main.py').read_bytes(),
                         b'# Synthetic ComfyUI fixture. Not executable.\n')
        self.assertEqual((extracted / 'python_embeded' / 'python.exe').read_bytes(),
                         b'Synthetic test data, not an executable.\n'
                         + b'\x90\xe8\x01\x00\x00\x00\xe9\x02\x00\x00\x00' * 4096)

    def test_extractor_requires_bundled_helper_and_empty_staging(self):
        archive = HERE / 'tests' / 'fixtures' / 'comfy-portable-bcj2.7z'
        target = self.root / 'extract'; target.mkdir()
        with (patch.object(ai, 'SEVENZIP_EXE', self.root / 'missing.exe'),
              patch.object(ai.subprocess, 'run', side_effect=AssertionError('no system fallback'))):
            with self.assertRaisesRegex(ai.SetupError, 'bundled 7-Zip extractor is missing'):
                ai.extract_portable(archive, target)
        (target / 'preserve.txt').write_bytes(b'user file')
        with self.assertRaisesRegex(ai.SetupError, 'empty staging folder'):
            ai.extract_portable(archive, target)
        self.assertEqual((target / 'preserve.txt').read_bytes(), b'user file')

    def test_failed_native_decoder_cannot_publish_runtime(self):
        archive = HERE / 'tests' / 'fixtures' / 'comfy-portable-bcj2.7z'
        target = self.root / 'extract'; target.mkdir()
        with patch.object(ai.subprocess, 'run', return_value=SimpleNamespace(returncode=2)) as run:
            with self.assertRaisesRegex(ai.SetupError, '7-Zip code 2'):
                ai.extract_portable(archive, target)
        args, options = run.call_args
        self.assertEqual(args[0][0], str(ai.SEVENZIP_EXE))
        self.assertNotIn('-spf', args[0])
        self.assertFalse(options['shell'])
        self.assertEqual(options['creationflags'], getattr(ai.subprocess, 'CREATE_NO_WINDOW', 0))
        self.assertEqual(options['cwd'], str(target.resolve()))

    def test_extracted_files_must_match_metadata_and_cannot_be_reparse_points(self):
        target = self.root / 'extract'; target.mkdir()
        child = target / 'file'; child.write_bytes(b'x')
        members = [self.member('file')]
        ai.validate_extracted_files(target, members)
        child.write_bytes(b'changed')
        with self.assertRaisesRegex(ai.SetupError, 'do not match'):
            ai.validate_extracted_files(target, members)
        child.unlink()
        with self.assertRaisesRegex(ai.SetupError, 'incomplete'):
            ai.validate_extracted_files(target, members)
        child.write_bytes(b'x')
        real_lstat = ai.Path.lstat
        def reparse_file(path):
            value = real_lstat(path)
            if path == child:
                return SimpleNamespace(st_mode=value.st_mode, st_size=value.st_size,
                                       st_file_attributes=ai.stat.FILE_ATTRIBUTE_REPARSE_POINT)
            return value
        with patch.object(ai.Path, 'lstat', reparse_file):
            with self.assertRaisesRegex(ai.SetupError, 'unsafe link'):
                ai.validate_extracted_files(target, members)

    async def test_download_only_publishes_verified_bytes_and_follows_trusted_redirect(self):
        body = b'publisher bytes'
        target = self.root / 'models' / 'test.safetensors'
        client = Client([Response(status=302, headers={'Location': 'https://us.aws.cdn.hf.co/valid'}), Response(body)])
        progress = []
        with patch.object(ai.aiohttp, 'ClientSession', return_value=client):
            await ai.download_verified(self.artifact(body), target, lambda *values: progress.append(values))
        self.assertEqual(target.read_bytes(), body)
        self.assertTrue(all(call[1]['allow_redirects'] is False for call in client.calls))
        self.assertEqual(progress[-1], (len(body), len(body)))
        self.assertEqual(list(target.parent.glob('*.part')), [])

    async def test_bad_checksum_and_untrusted_redirect_leave_no_output(self):
        for responses in ([Response(b'wrong contents!')], [Response(status=302, headers={'Location': 'https://attacker.example/model'})]):
            target = self.root / 'bad.safetensors'
            with patch.object(ai.aiohttp, 'ClientSession', return_value=Client(responses)):
                with self.assertRaises(ai.SetupError):
                    await ai.download_verified(self.artifact(), target)
            self.assertFalse(target.exists())
            self.assertEqual(list(self.root.glob('*.part')), [])

    async def test_existing_mismatch_is_preserved_and_compatible_vae_is_reused(self):
        target = self.root / 'existing.safetensors'
        target.write_bytes(b'custom file')
        with patch.object(ai.aiohttp, 'ClientSession', side_effect=AssertionError('network not expected')):
            with self.assertRaises(ai.SetupError):
                await ai.download_verified(self.artifact(), target)
            self.assertEqual(target.read_bytes(), b'custom file')
            artifact = self.artifact()
            artifact['compatible_existing'] = [self.artifact(b'custom file')]
            await ai.download_verified(artifact, target)
            self.assertEqual(target.read_bytes(), b'custom file')

    async def test_job_serializes_operations_releases_generation_lock_and_records_failure(self):
        gate = asyncio.Lock(); await gate.acquire()
        wait = asyncio.Event()
        async def work():
            await wait.wait()
            raise ai.SetupError('Download fixture failed')
        self.manager.begin('download-models', work, gate.release)
        self.assertTrue(self.manager.active)
        with self.assertRaises(ai.SetupError):
            self.manager.begin('start', AsyncMock())
        wait.set(); await self.manager.task
        self.assertFalse(gate.locked())
        self.assertEqual(self.manager.job['status'], 'error')
        self.assertIn('fixture failed', self.manager.job['error'])
        saved = json.loads((self.root / 'profile' / 'state' / 'ai-setup-job.json').read_text())
        self.assertEqual(saved['status'], 'error')

    async def test_interrupted_persisted_job_is_reported_and_retryable(self):
        folder = self.root / 'profile' / 'state'; folder.mkdir(parents=True)
        (folder / 'ai-setup-job.json').write_text(json.dumps({'id': 'test', 'status': 'running', 'action': 'install'}))
        with patch.object(ai, 'service_state', AsyncMock(return_value={'running': False, 'busy': False, 'device': '', 'port': 8188})):
            status = await self.manager.status()
        self.assertEqual(status['job']['status'], 'error')
        self.assertEqual(status['job']['phase'], 'interrupted')
        self.manager.require_idle()

    async def test_eject_refuses_busy_queue_and_sends_only_free_when_idle(self):
        request = AsyncMock(return_value={'queue_running': [['other job']], 'queue_pending': []})
        with patch.object(ai, 'comfy_request', request):
            with self.assertRaises(ai.SetupError):
                await self.manager.eject()
        self.assertEqual(request.await_count, 1)
        request = AsyncMock(side_effect=[{'queue_running': [], 'queue_pending': []}, {}])
        with patch.object(ai, 'comfy_request', request):
            await self.manager.eject()
        self.assertEqual(request.await_args.args, ('/free', {'unload_models': True, 'free_memory': True}))

    async def test_start_uses_selected_python_fixed_loopback_flags_and_no_shell(self):
        code, python = create_runtime(self.root / 'portable')
        ai.configure(comfy_directory=str(code), comfy_port=8189)
        states = [{'running': False, 'busy': False, 'device': '', 'port': 8189},
                  {'running': True, 'busy': False, 'device': 'Test GPU', 'port': 8189}]
        process = Mock(); process.poll.return_value = None
        with (patch.object(ai, 'service_state', AsyncMock(side_effect=states)),
              patch.object(ai, '_port_in_use', return_value=False), patch.object(ai.subprocess, 'Popen', return_value=process) as popen):
            await self.manager.start()
        command = popen.call_args.args[0]
        self.assertEqual(command[:3], [str(python), '-s', str(code / 'main.py')])
        self.assertEqual(command[command.index('--listen') + 1], '127.0.0.1')
        self.assertEqual(command[command.index('--port') + 1], '8189')
        self.assertIn('--disable-auto-launch', command)
        self.assertIn('--windows-standalone-build', command)
        self.assertFalse(popen.call_args.kwargs['shell'])
        process.terminate.assert_not_called(); process.kill.assert_not_called()

    async def test_install_rejects_existing_destination_without_changing_it(self):
        destination = self.root / ai.MANAGED_FOLDER; destination.mkdir()
        keep = destination / 'user-file'; keep.write_bytes(b'preserve')
        with patch.object(ai, 'download_verified', side_effect=AssertionError('network not expected')):
            with self.assertRaises(ai.SetupError):
                await self.manager.install(str(self.root))
        self.assertEqual(keep.read_bytes(), b'preserve')


if __name__ == '__main__':
    unittest.main(verbosity=2)
