"""Generation API authorization, reference snapshots, provenance and lifecycle."""
import asyncio
import json
from pathlib import Path
import sys
import types
import unittest
from unittest.mock import AsyncMock, patch
import uuid
import zipfile

from fastapi import HTTPException
from PIL import Image
from pydantic import ValidationError

BACKEND = Path(__file__).resolve().parents[1] / 'backend'
sys.path.insert(0, str(BACKEND)); sys.path.insert(0, str(Path(__file__).resolve().parent / 'helpers'))
import backend_folder_save_test as fixtures
fixtures.V3 = BACKEND
import qwen_image
import z_image
import flux2_image
import hidream_image
import ernie_image
from test_qwen_image import inventory as qwen_inventory
from test_z_image import inventory as z_inventory
from test_flux2_image import inventory as flux_inventory
from test_hidream_image import inventory as hidream_inventory
from test_ernie_image import inventory as ernie_inventory
from generation_metadata import validate_generation_metadata
from generation_resolution import validate_generation_size


def combined_inventory():
    info = qwen_inventory()
    def choices(value):
        if value and isinstance(value[0], list):
            return value[0]
        if len(value) > 1 and value[0] == 'COMBO' and isinstance(value[1], dict):
            return value[1].get('options', [])
        return []
    for inventory in (z_inventory(), flux_inventory(), hidream_inventory(), ernie_inventory()):
        for name, node in inventory.items():
            if name not in info:
                info[name] = node
            else:
                for field, value in node.get('input', {}).get('required', {}).items():
                    fields = info[name]['input']['required']
                    fields[field] = value if value and value[0] == 'INT' else [list(dict.fromkeys(choices(fields.get(field, [[]])) + choices(value)))]
    info['LoraLoaderModelOnly'] = {'input': {'required': {'lora_name': [[]]}}}
    return info


class GenerationTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.fixture = fixtures.FolderSaveTests(); self.fixture.setUp(); self.editor = self.fixture.app
        self.api = types.ModuleType('image_generation_under_test'); self.api.__file__ = str(BACKEND / 'image_generation.py')
        sys.modules[self.api.__name__] = self.api
        with patch.dict(sys.modules, {'local_remove': self.editor}):
            library = types.ModuleType('generation_library_under_test'); library.__file__ = str(BACKEND / 'generation_library.py')
            sys.modules[library.__name__] = library
            exec(compile(Path(library.__file__).read_text(encoding='utf-8'), library.__file__, 'exec'), library.__dict__)
            exec(compile(Path(self.api.__file__).read_text(encoding='utf-8'), self.api.__file__, 'exec'), self.api.__dict__)
            self.progress = sys.modules['operation_progress']
        self.library_patch = patch.dict(sys.modules, {'generation_library': library})
        self.library_patch.start()

    def tearDown(self):
        self.library_patch.stop(); sys.modules.pop('generation_library_under_test', None)
        self.fixture.tearDown(); sys.modules.pop(self.api.__name__, None)

    def payload(self, **changes):
        return self.api.GenerationRequest(prompt='A red ceramic cup', width=256, height=256, **changes)

    async def test_inventory_reports_capabilities_without_claiming_z_semantic_or_alpha_inputs(self):
        with patch.object(qwen_image, '_object_info', AsyncMock(return_value=combined_inventory())) as info:
            result = await self.api.generation_models(self.fixture.request())
        info.assert_awaited_once(); self.assertTrue(result['connected'])
        qwen, z, klein, larger, ernie = result['models']; self.assertTrue(all(model['available'] for model in result['models']))
        self.assertEqual(ernie['id'], 'ernie-image')
        self.assertFalse(ernie['capabilities']['image_to_image']); self.assertFalse(ernie['capabilities']['loras'])
        self.assertEqual(ernie['capabilities']['max_references'], 0)
        self.assertEqual((ernie['defaults']['steps'], ernie['defaults']['guidance']), (50, 4))
        self.assertEqual(larger['id'], 'flux2-klein-9b')
        self.assertNotIn('flux2-dev', [model['id'] for model in result['models']])
        self.assertNotIn('hidream-o1', [model['id'] for model in result['models']])
        self.assertTrue(qwen['capabilities']['transparent']); self.assertEqual(qwen['capabilities']['max_references'], 10)
        self.assertFalse(z['capabilities']['transparent']); self.assertFalse(z['capabilities']['image_reference'])
        self.assertEqual(z['capabilities']['reference_mode'], 'init'); self.assertTrue(z['capabilities']['denoise'])
        for model in (klein, larger):
            self.assertEqual(model['capabilities']['reference_mode'], 'semantic')
            self.assertEqual(model['capabilities']['max_references'], 4)
            self.assertFalse(model['capabilities']['transparent']); self.assertTrue(model['capabilities']['loras'])
            self.assertTrue(model['benefit'])
        self.assertEqual((klein['defaults']['steps'], klein['defaults']['guidance']), (4, 1))
        self.assertEqual((larger['defaults']['steps'], larger['defaults']['guidance']), (4, 1))
        self.assertEqual(larger['recommended']['steps'], 4)
        self.assertIn('Non-Commercial', larger['license']['label'])
        self.assertGreater(larger['variants'][0]['total_bytes'], klein['variants'][0]['total_bytes'])
        self.assertEqual(larger['defaults']['variant'], 'fp8')

    async def test_disconnected_inventory_still_lists_catalogue_and_recommended_settings(self):
        with patch.object(qwen_image, '_object_info', AsyncMock(side_effect=qwen_image.QwenImageError('ComfyUI is offline'))):
            result = await self.api.generation_models(self.fixture.request())
        self.assertFalse(result['connected']); self.assertEqual(len(result['models']), 5)
        self.assertTrue(result['model_directory'])
        for model in result['models']:
            self.assertFalse(model['available']); self.assertTrue(model['description'])
            self.assertTrue(model['strengths']); self.assertTrue(model['license']['url'])
            self.assertEqual(model['recommended']['steps'], model['defaults']['steps'])
            self.assertGreater(model['storage_bytes'], 0)

    async def test_catalogue_sampling_guidance_distinguishes_publisher_examples_and_workflow_limits(self):
        models = self.api.model_inventory(combined_inventory())['models']
        expected = {'qwen': 40, 'z-image-turbo': 8, 'flux2-klein-4b': 4, 'flux2-klein-9b': 4, 'ernie-image': 50}
        for model in models:
            with self.subTest(model=model['id']):
                guidance = model['sampling_guidance']
                self.assertEqual(model['defaults']['steps'], expected[model['id']])
                self.assertEqual(guidance['steps']['recommended'], model['defaults']['steps'])
                self.assertTrue(guidance['steps']['source_url'].startswith('https://'))
                self.assertEqual(model['limits']['resolution_policy'], 'comfy-workflow')
                self.assertEqual(model['limits']['max_dimension'], 16384)
                self.assertIsNone(model['limits']['max_pixels'])
                self.assertIsNone(guidance['resolution']['published_max_pixels'])
                self.assertIn('workflow', guidance['resolution']['workflow_limits']['label'])
                self.assertTrue(guidance['resolution']['recommended_sizes'])
                for size in guidance['resolution']['recommended_sizes']:
                    self.api.GenerationRequest(model=model['id'], prompt='An empty landscape', **size)
                if model['id'].startswith('flux2-klein-'):
                    self.assertEqual((model['limits']['min_steps'], model['limits']['max_steps']), (4, 4))
                    self.assertEqual((model['limits']['min_guidance'], model['limits']['max_guidance']), (1, 1))
        qwen = models[0]['sampling_guidance']['resolution']
        self.assertEqual(qwen['publisher_sizes'], qwen['recommended_sizes'])
        self.assertIn({'width': 848, 'height': 1264}, models[-1]['sampling_guidance']['resolution']['recommended_sizes'])
        # Catalog callers must not mutate shared publisher facts.
        qwen['recommended_sizes'][0]['width'] = 256
        self.assertEqual(self.api.model_inventory({})['models'][0]['sampling_guidance']['resolution']['recommended_sizes'][0]['width'], 2048)

    async def test_distilled_klein_new_requests_are_four_steps_but_legacy_projects_still_open(self):
        for model in ('flux2-klein-4b', 'flux2-klein-9b'):
            for steps in (None, 4):
                self.assertEqual(self.api.generation_parameters(self.payload(model=model, steps=steps))['steps'], 4)
            for steps in (1, 3, 5, 20, 50):
                with self.subTest(model=model, steps=steps), self.assertRaisesRegex(ValidationError, 'exactly 4 steps'):
                    self.payload(model=model, steps=steps)
            for legacy_steps in (1, 20, 50):
                legacy = self.api.generation_parameters(self.payload(model=model, seed=0))
                legacy['steps'] = legacy_steps
                self.assertEqual(validate_generation_metadata(legacy)['steps'], legacy_steps)
                source = self.fixture.bind(self.fixture.make_image(size=(256, 256)))
                data = self.editor.read_session(source['id']); data['generation'] = legacy
                path = self.fixture.images / f'{model}-{legacy_steps}.lremove'
                self.editor.write_project(self.editor.folder(data['id']), data, path)
                self.assertEqual(self.editor.import_project(path)['session']['generation'], legacy)

    async def test_qwen_generation_dispatch_uses_publisher_default_for_both_precisions(self):
        for variant in ('int8', 'bf16'):
            with patch.object(qwen_image, 'run_qwen_image', AsyncMock(return_value=Image.new('RGB', (256, 256), 'green'))) as generate:
                result = await self.api.generate_image(self.fixture.request(), self.payload(variant=variant, seed=0))
            self.assertEqual(generate.await_args.kwargs['steps'], 40)
            self.assertEqual(result['session']['generation']['steps'], 40)
        self.assertEqual(self.api.generation_parameters(self.payload(steps=25))['steps'], 25)

    async def test_generation_dimensions_use_workflow_constraints_without_an_app_area_cap(self):
        for model in ('qwen', 'z-image-turbo', 'flux2-klein-4b', 'flux2-klein-9b', 'ernie-image'):
            for width, height in ((224, 256), (256, 4128), (1024, 1040), (3840, 2160), (4096, 2304), (7680, 4320)):
                payload = self.api.GenerationRequest(model=model, prompt='A room', width=width, height=height)
                validate_generation_metadata(self.api.generation_parameters(payload))
                self.assertEqual(validate_generation_size((width, height), model, combined_inventory()), (width, height))
            for size in ((15, 16), (16385, 512), (1024, 1041)):
                with self.subTest(model=model, size=size), self.assertRaises(ValueError):
                    validate_generation_size(size, model, combined_inventory())

    async def test_model_specific_validation_rejects_paths_dimensions_and_unsupported_controls(self):
        invalid = [{'reference_session_ids': ['../private.png']}, {'transparent': True, 'model': 'z-image-turbo'},
                   {'model': 'z-image-turbo', 'variant': 'int8'}, {'model': 'z-image-turbo', 'negative_prompt': 'text'},
                   {'model': 'z-image-turbo', 'guidance': 2}, {'denoise': .5},
                   {'model': 'z-image-turbo', 'denoise': .3}, {'seed': True}, {'width': 0}, {'height': -1}, {'width': True},
                   {'model': 'qwen', 'variant': 'fp8'}, {'model': 'flux2-dev', 'variant': 'bf16'},
                   {'model': 'flux2-dev', 'transparent': True}, {'model': 'flux2-dev', 'negative_prompt': 'text'},
                   {'model': 'flux2-klein-4b', 'guidance': 4}, {'model': 'flux2-klein-4b', 'denoise': .5},
                   {'model': 'flux2-klein-9b', 'guidance': 4}, {'model': 'flux2-klein-9b', 'steps': 51},
                   {'model': 'flux2-klein-9b', 'variant': 'bf16'}, {'model': 'hidream-o1', 'variant': 'bf16'},
                   {'model': 'hidream-o1', 'transparent': True}, {'model': 'hidream-o1', 'denoise': .4},
                   {'model': 'flux2-dev', 'reference_session_ids': [str(uuid.uuid4()) for _ in range(5)]},
                   {'loras': [{'id': '../some.safetensors', 'strength': 1}]},
                   {'model': 'ernie-image', 'transparent': True}, {'model': 'ernie-image', 'denoise': .4},
                   {'model': 'ernie-image', 'reference_session_ids': [str(uuid.uuid4())]},
                   {'model': 'ernie-image', 'loras': [{'id': 'a' * 24, 'strength': 1}]}]
        for values in invalid:
            with self.subTest(values=values), self.assertRaises(ValidationError):
                self.api.GenerationRequest(prompt='A cup', **values)
        sid = str(uuid.uuid4())
        with self.assertRaises(ValidationError):
            self.payload(reference_session_ids=[sid, sid])

    async def test_omitted_dimensions_follow_model_defaults_without_changing_explicit_inputs(self):
        values = {'model': 'hidream-o1', 'prompt': 'A detailed room'}
        request = self.api.GenerationRequest(**values)
        parameters = self.api.generation_parameters(request)
        self.assertEqual((request.width, request.height), (2048, 2048))
        self.assertEqual((parameters['width'], parameters['height']), (2048, 2048))
        self.assertNotIn('width', values)
        explicit = self.api.GenerationRequest(**values, width=256, height=512)
        self.assertEqual((explicit.width, explicit.height), (256, 512))
        with self.assertRaises(ValidationError):
            self.api.GenerationRequest(**values, width=None)
        default = self.api.GenerationRequest(prompt='A room')
        self.assertEqual((default.width, default.height), (1024, 1024))

    async def test_submission_id_is_used_for_progress_and_is_not_saved_as_image_metadata(self):
        submission = str(uuid.uuid4())
        async def generate(*args):
            self.assertEqual(self.progress.current().job_id, submission)
            return Image.new('RGB', (256, 256), 'green')
        with patch.object(self.api, 'execute_generation', generate):
            result = await self.api.generate_image(self.fixture.request(), self.payload(operation_id=submission))
        self.assertEqual(self.progress.snapshot()['job_id'], submission)
        self.assertNotIn('operation_id', result['session']['generation'])
        for invalid in ('other-job', 'A' * 36, 12):
            with self.assertRaises(ValidationError):
                self.payload(operation_id=invalid)

    async def test_transparent_generation_preserves_seed_zero_alpha_and_unsaved_provenance(self):
        output = Image.new('RGBA', (256, 256), (160, 40, 30, 0)); output.paste((160, 40, 30, 255), (80, 50, 180, 210))
        with patch.object(qwen_image, 'run_qwen_image', AsyncMock(return_value=output)) as generate:
            result = await self.api.generate_image(self.fixture.request(), self.payload(seed=0, transparent=True))
        self.assertEqual(generate.await_args.kwargs['seed'], 0); self.assertEqual(generate.await_args.kwargs['task'], 'generate')
        data = result['session']; self.assertTrue(data['dirty']); self.assertEqual(data['revision'], 1)
        self.assertEqual(data['generation']['seed'], 0); self.assertFalse(data['can_return'])
        original = self.editor.folder(data['id']) / data['original']
        with Image.open(original) as image:
            self.assertEqual(image.mode, 'RGBA'); self.assertEqual(image.getchannel('A').getextrema(), (0, 255))
        archive_path = self.fixture.images / 'generated.lremove'
        self.editor.write_project(self.editor.folder(data['id']), data, archive_path)
        with zipfile.ZipFile(archive_path) as archive:
            manifest = json.loads(archive.read('manifest.json'))
            self.assertEqual(manifest['generation'], data['generation'])
            self.assertNotIn('reference_session_ids', manifest['generation'])

    async def test_reference_snapshots_include_repairs_and_original_alpha_without_mutating_inputs(self):
        source = self.fixture.images / 'reference.png'
        rgba = Image.new('RGBA', (256, 256), (40, 60, 80, 255)); rgba.putpixel((0, 0), (40, 60, 80, 0)); rgba.save(source)
        data = self.fixture.layer(self.fixture.bind(source)['id']); original = source.read_bytes(); captures = []
        async def generate(path, prompt, **kwargs):
            with Image.open(path) as reference:
                captures.append((reference.mode, reference.getpixel((0, 0)), reference.getpixel((10, 10))))
            return Image.new('RGBA', (256, 256), 'green')
        with patch.object(qwen_image, 'run_qwen_image', generate):
            result = await self.api.generate_image(self.fixture.request(), self.payload(reference_session_ids=[data['id']]))
        self.assertEqual(captures, [('RGBA', (40, 60, 80, 0), (200, 210, 220, 255))])
        self.assertEqual(result['reference_count'], 1); self.assertEqual(source.read_bytes(), original)
        self.assertFalse(list(self.editor.ROOT.glob('local-image-generation-*')))

    async def test_auth_busy_and_missing_reference_never_generate(self):
        with patch.object(qwen_image, 'run_qwen_image', AsyncMock()) as generate:
            with self.assertRaises(HTTPException) as denied:
                await self.api.generate_image(self.fixture.request(csrf=False), self.payload())
            self.assertEqual(denied.exception.status_code, 403)
            gate = self.fixture.fixture.main.generation_lock
            await gate.acquire()
            try:
                with self.assertRaises(HTTPException) as busy:
                    await self.api.generate_image(self.fixture.request(), self.payload())
                self.assertEqual(busy.exception.status_code, 409)
            finally:
                gate.release()
            with self.assertRaises(HTTPException) as missing:
                await self.api.generate_image(self.fixture.request(), self.payload(reference_session_ids=[str(uuid.uuid4())]))
            self.assertEqual(missing.exception.status_code, 404)
        generate.assert_not_called()

    async def test_failed_or_opaque_transparent_generation_releases_gate_and_creates_no_session(self):
        for answer in (qwen_image.QwenImageError('GPU unavailable'), Image.new('RGBA', (256, 256), 'red')):
            mock = AsyncMock(side_effect=answer) if isinstance(answer, Exception) else AsyncMock(return_value=answer)
            with patch.object(qwen_image, 'run_qwen_image', mock):
                with self.assertRaises(HTTPException) as failed:
                    await self.api.generate_image(self.fixture.request(), self.payload(transparent=True))
            self.assertEqual(failed.exception.status_code, 400)
            self.assertFalse(self.fixture.fixture.main.generation_lock.locked())
            self.assertEqual(list(self.editor.SESSIONS.glob('*/session.json')), [])
            self.assertFalse(list(self.editor.ROOT.glob('local-image-generation-*')))

    async def test_z_variation_routes_initial_image_and_strength_and_returns_opaque_session(self):
        reference = self.fixture.bind(self.fixture.make_image(size=(256, 256)))
        with patch.object(z_image, 'run_z_image', AsyncMock(return_value=Image.new('RGB', (256, 256), 'yellow'))) as generate:
            result = await self.api.generate_image(self.fixture.request(), self.payload(model='z-image-turbo',
                reference_session_ids=[reference['id']], denoise=.35, seed=0))
        self.assertEqual(generate.await_args.kwargs['denoise'], .35)
        self.assertEqual(generate.await_args.kwargs['steps'], 8); self.assertIsNotNone(generate.await_args.kwargs['input_path'])
        self.assertEqual(result['variant'], 'bf16'); self.assertFalse(result['transparent'])

    async def test_flux_dispatch_preserves_semantic_refs_and_model_specific_defaults(self):
        reference = self.fixture.bind(self.fixture.make_image(size=(256, 256)))
        for model, variant, steps, guidance in [('flux2-klein-4b', 'bf16', 4, 1), ('flux2-klein-9b', 'fp8', 4, 1), ('flux2-dev', 'fp8', 20, 4)]:
            with patch.object(flux2_image, 'run_flux2_image', AsyncMock(return_value=Image.new('RGB', (256, 256), 'blue'))) as generate:
                result = await self.api.generate_image(self.fixture.request(), self.payload(model=model,
                    reference_session_ids=[reference['id']], seed=0))
            kwargs = generate.await_args.kwargs
            self.assertEqual((kwargs['model'], kwargs['seed'], kwargs['steps'], kwargs['guidance']), (model, 0, steps, guidance))
            self.assertEqual(len(kwargs['references']), 1)
            self.assertEqual((result['variant'], result['reference_count']), (variant, 1))
            self.assertTrue(result['session']['dirty'])

    async def test_hidream_dispatch_retains_reference_negative_prompt_defaults_and_project_roundtrip(self):
        reference = self.fixture.bind(self.fixture.make_image(size=(256, 256)))
        with patch.object(hidream_image, 'run_hidream_image', AsyncMock(return_value=Image.new('RGB', (256, 256), 'blue'))) as generate:
            result = await self.api.generate_image(self.fixture.request(), self.payload(model='hidream-o1',
                reference_session_ids=[reference['id']], seed=0, negative_prompt='blur, misspelled text'))
        kwargs = generate.await_args.kwargs
        self.assertEqual((kwargs['seed'], kwargs['steps'], kwargs['guidance']), (0, 50, 5))
        self.assertEqual(kwargs['negative_prompt'], 'blur, misspelled text')
        self.assertEqual(len(kwargs['references']), 1)
        self.assertEqual(result['variant'], 'fp8'); self.assertFalse(result['transparent'])
        data = result['session']; self.assertTrue(data['dirty'])
        self.assertEqual(data['generation']['denoise'], None)
        self.assertEqual(data['generation']['model'], 'hidream-o1')
        path = self.fixture.images / 'hidream.lremove'
        self.editor.write_project(self.editor.folder(data['id']), data, path)
        restored = self.editor.import_project(path)['session']
        self.assertEqual(restored['generation'], data['generation'])
        self.assertNotIn('reference_session_ids', restored['generation'])

    async def test_hidream_execution_failure_creates_no_document_and_releases_gpu_lock(self):
        with patch.object(hidream_image, 'run_hidream_image', AsyncMock(side_effect=hidream_image.HiDreamImageError('GPU ran out of memory'))):
            with self.assertRaises(HTTPException) as failed:
                await self.api.generate_image(self.fixture.request(), self.payload(model='hidream-o1'))
        self.assertEqual(failed.exception.status_code, 400)
        self.assertIn('out of memory', failed.exception.detail)
        self.assertFalse(self.fixture.fixture.main.generation_lock.locked())
        self.assertEqual(list(self.editor.SESSIONS.glob('*/session.json')), [])
        self.assertFalse(list(self.editor.ROOT.glob('local-image-generation-*')))

    async def test_loras_resolve_exact_model_before_gpu_and_persist_ids_without_paths(self):
        item = {'id': 'a' * 24, 'strength': .7}
        resolver = unittest.mock.Mock(return_value=[('local-image/registered.safetensors', .7)])
        with patch.dict(sys.modules, {'lora_library': types.SimpleNamespace(resolve_loras=resolver)}):
            for model, module, name in [('qwen', qwen_image, 'run_qwen_image'), ('z-image-turbo', z_image, 'run_z_image'),
                                       ('flux2-dev', flux2_image, 'run_flux2_image'), ('hidream-o1', hidream_image, 'run_hidream_image')]:
                with patch.object(module, name, AsyncMock(return_value=Image.new('RGB', (256, 256), 'green'))) as generate:
                    result = await self.api.generate_image(self.fixture.request(), self.payload(model=model, loras=[item]))
                self.assertEqual(generate.await_args.kwargs['loras'], [('local-image/registered.safetensors', .7)])
                resolver.assert_called_with(model, [item], reference_count=0)
                self.assertEqual(result['session']['generation']['loras'], [item])
                self.assertNotIn('registered.safetensors', json.dumps(result['session']['generation']))
                archive_path = self.fixture.images / (model + '-lora.lremove')
                self.editor.write_project(self.editor.folder(result['session']['id']), result['session'], archive_path)
                with zipfile.ZipFile(archive_path) as archive:
                    self.assertEqual(json.loads(archive.read('manifest.json'))['generation']['loras'], [item])

    async def test_wrong_model_lora_creates_no_session_or_gpu_job(self):
        resolver = unittest.mock.Mock(side_effect=ValueError('This LoRA belongs to a different model.'))
        with patch.dict(sys.modules, {'lora_library': types.SimpleNamespace(resolve_loras=resolver)}), \
                patch.object(flux2_image, 'run_flux2_image', AsyncMock()) as generate:
            with self.assertRaises(HTTPException) as error:
                await self.api.generate_image(self.fixture.request(), self.payload(model='flux2-dev', loras=[{'id': 'b' * 24, 'strength': 1}]))
        self.assertEqual(error.exception.status_code, 400); self.assertIn('different model', error.exception.detail)
        generate.assert_not_called()
        self.assertFalse(self.fixture.fixture.main.generation_lock.locked())
        self.assertEqual(list(self.editor.SESSIONS.glob('*/session.json')), [])
        self.assertFalse(list(self.editor.ROOT.glob('local-image-generation-*')))

    async def test_reference_edit_lora_validation_happens_before_gpu_and_receives_reference_count(self):
        calls = []
        def resolve(model, selections, *, reference_count=0):
            calls.append(reference_count)
            if not reference_count:
                raise ValueError('Natural exposure is an image-editing adapter. Add a reference image before generating.')
            return [('local-image/reference-edit.safetensors', .7)]
        selected = [{'id': 'c' * 24, 'strength': .7}]
        with patch.dict(sys.modules, {'lora_library': types.SimpleNamespace(resolve_loras=resolve)}), \
                patch.object(qwen_image, 'run_qwen_image', AsyncMock(return_value=Image.new('RGB', (256, 256), 'green'))) as generate:
            with self.assertRaises(HTTPException) as error:
                await self.api.generate_image(self.fixture.request(), self.payload(loras=selected))
            self.assertEqual(error.exception.status_code, 400)
            self.assertIn('Add a reference image', error.exception.detail)
            generate.assert_not_called()
            self.assertFalse(self.fixture.fixture.main.generation_lock.locked())
            self.assertEqual(list(self.editor.SESSIONS.glob('*/session.json')), [])
            source = self.fixture.bind(self.fixture.make_image(size=(256, 256)))
            answer = await self.api.generate_image(self.fixture.request(), self.payload(loras=selected, reference_session_ids=[source['id']]))
        self.assertEqual(calls, [0, 1])
        self.assertEqual(answer['reference_count'], 1)
        self.assertEqual(answer['session']['generation']['loras'], selected)
        generate.assert_awaited_once()

    async def test_portable_metadata_accepts_older_no_lora_and_rejects_unsafe_settings(self):
        valid = self.api.generation_parameters(self.payload(model='flux2-dev', seed=0))
        older = dict(valid); older.pop('loras')
        self.assertEqual(validate_generation_metadata(older), older)
        for updates in ({'variant': 'bf16'}, {'transparent': True}, {'reference_count': 5},
                        {'loras': [{'id': '../unsafe', 'strength': 1}]}, {'loras': [{'id': 'a' * 24, 'strength': float('inf')}]},
                        {'reference_session_ids': ['local-authority']}, {'denoise': .5}):
            with self.subTest(updates=updates), self.assertRaises(ValueError):
                validate_generation_metadata({**valid, **updates})


if __name__ == '__main__':
    unittest.main()
