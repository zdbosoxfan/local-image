"""FLUX.2 native graph, reference conditioning and precision separation."""
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import AsyncMock, patch

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
import flux2_image as flux


def inventory():
    names = (*flux.REQUIRED_NODES, 'FluxGuidance', 'BasicGuider', 'CFGGuider',
             'ConditioningZeroOut', 'LoadImage', 'VAEEncode', 'ReferenceLatent')
    info = {name: {'input': {'required': {}}} for name in names}
    for role, (node, field) in flux.LOADERS.items():
        info[node]['input']['required'][field] = [[f'flux/{preset["files"][role]}' for preset in flux.PRESETS.values()]]
    info['CLIPLoader']['input']['required']['type'] = [['flux2']]
    info['KSamplerSelect']['input']['required']['sampler_name'] = [['euler']]
    info['EmptyFlux2LatentImage']['input']['required'].update({axis: ['INT', {'min': 16, 'max': 16384, 'step': 16}] for axis in ('width', 'height')})
    info['Flux2Scheduler']['input']['required'].update({axis: ['INT', {'min': 16, 'max': 16384, 'step': 1}] for axis in ('width', 'height')})
    return info


def png(image):
    output = io.BytesIO(); image.save(output, format='PNG'); return output.getvalue()


class Flux2GraphTests(unittest.TestCase):
    def test_inventory_uses_distilled_klein_not_existing_base_repair_model(self):
        option = flux.flux2_model_option(inventory(), 'flux2-klein-4b')
        self.assertTrue(option['available']); self.assertEqual(option['id'], 'bf16')
        self.assertEqual(option['files']['unet'], 'flux/flux-2-klein-4b.safetensors')
        info = inventory()
        info['UNETLoader']['input']['required']['unet_name'] = [['flux-2-klein-base-4b.safetensors']]
        self.assertFalse(flux.flux2_model_option(info, 'flux2-klein-4b')['available'])

    def test_live_v3_combo_schema_finds_euler_and_model_files(self):
        info = inventory()
        info['KSamplerSelect']['input']['required']['sampler_name'] = ['COMBO', {'multiselect': False, 'options': ['euler', 'heun']}]
        for node, fields in info.items():
            for name, values in list(fields.get('input', {}).get('required', {}).items()):
                if isinstance(values[0], list):
                    fields['input']['required'][name] = ['COMBO', {'options': values[0]}]
        for model in flux.PRESETS:
            self.assertTrue(flux.flux2_model_option(info, model)['available'])

    def test_klein_9b_requires_its_own_diffusion_and_larger_encoder(self):
        option = flux.flux2_model_option(inventory(), 'flux2-klein-9b')
        self.assertTrue(option['available'])
        graph = flux.build_flux2_workflow(option['files'], model='flux2-klein-9b', prompt='A detailed room', references=['ref.png'])
        self.assertEqual(graph['1']['inputs']['unet_name'], 'flux/flux-2-klein-9b-fp8.safetensors')
        self.assertEqual(graph['2']['inputs']['clip_name'], 'flux/qwen_3_8b_fp8mixed.safetensors')
        self.assertEqual(graph['8']['inputs']['steps'], 4)
        self.assertEqual(graph['6']['inputs']['cfg'], 1)
        self.assertEqual(graph['6']['inputs']['positive'], ['22', 0])
        self.assertEqual(graph['6']['inputs']['negative'], ['23', 0])
        info = inventory()
        info['CLIPLoader']['input']['required']['clip_name'] = [['qwen_3_4b.safetensors']]
        self.assertFalse(flux.flux2_model_option(info, 'flux2-klein-9b')['available'])

    def test_dev_uses_native_guidance_scheduler_and_mistral_encoder_without_turbo_lora(self):
        graph = flux.build_flux2_workflow(flux.PRESETS['flux2-dev']['files'], model='flux2-dev', prompt='A studio', size=(768, 512), seed=0)
        self.assertEqual(graph['2']['inputs']['type'], 'flux2')
        self.assertEqual(graph['2']['inputs']['clip_name'], 'mistral_3_small_flux2_fp8.safetensors')
        self.assertEqual(graph['5']['inputs']['guidance'], 4)
        self.assertEqual(graph['6']['class_type'], 'BasicGuider')
        self.assertEqual(graph['8']['inputs'], {'steps': 20, 'width': 768, 'height': 512})
        self.assertEqual(graph['9']['inputs']['noise_seed'], 0)
        self.assertEqual(graph['10']['inputs']['sampler_name'], 'euler')
        self.assertEqual(graph['11']['inputs']['latent_image'], ['7', 0])
        self.assertEqual(graph['7']['class_type'], 'EmptyFlux2LatentImage')
        self.assertNotIn('LoraLoaderModelOnly', [node['class_type'] for node in graph.values()])

    def test_distilled_klein_four_steps_and_shared_reference_latents_on_both_conditions(self):
        graph = flux.build_flux2_workflow(flux.PRESETS['flux2-klein-4b']['files'], model='flux2-klein-4b', prompt='Two objects', references=['a.png', 'b.png'])
        self.assertEqual(graph['8']['inputs']['steps'], 4)
        self.assertEqual(graph['6']['inputs']['cfg'], 1)
        self.assertEqual(graph['5']['class_type'], 'ConditioningZeroOut')
        self.assertEqual(graph['22']['inputs'], {'conditioning': ['4', 0], 'latent': ['21', 0]})
        self.assertEqual(graph['23']['inputs'], {'conditioning': ['5', 0], 'latent': ['21', 0]})
        self.assertEqual(graph['26']['inputs'], {'conditioning': ['22', 0], 'latent': ['25', 0]})
        self.assertEqual(graph['27']['inputs'], {'conditioning': ['23', 0], 'latent': ['25', 0]})
        self.assertEqual(graph['6']['inputs']['positive'], ['26', 0])
        self.assertEqual(graph['6']['inputs']['negative'], ['27', 0])
        self.assertEqual(graph['11']['inputs']['latent_image'], ['7', 0])

    def test_dev_chains_multiple_references_after_guidance_without_negative_conditioning(self):
        graph = flux.build_flux2_workflow(flux.PRESETS['flux2-dev']['files'], model='flux2-dev', prompt='Combine them', references=['a.png', 'b.png'])
        self.assertEqual(graph['22']['inputs']['conditioning'], ['5', 0])
        self.assertEqual(graph['26']['inputs']['conditioning'], ['22', 0])
        self.assertEqual(graph['6']['inputs']['conditioning'], ['26', 0])
        self.assertNotIn('23', graph); self.assertNotIn('27', graph)


class Flux2ExecutionTests(unittest.IsolatedAsyncioTestCase):
    async def test_reference_is_opaque_bounded_and_does_not_change_original(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'transparent.png'
            Image.new('RGBA', (2048, 1024), (5, 10, 20, 0)).save(source); original = source.read_bytes()
            capture = {}
            async def execute(graph):
                with Image.open(graph['20']['inputs']['image']) as reference:
                    capture.update(mode=reference.mode, size=reference.size, pixel=reference.getpixel((0, 0)))
                return png(Image.new('RGBA', (256, 256), (20, 30, 40, 0)))
            with patch.object(flux, '_object_info', AsyncMock(return_value=inventory())), patch.object(flux, '_execute_workflow', execute):
                image = await flux.run_flux2_image('A ceramic object', model='flux2-klein-4b', references=[source], size=(256, 256), seed=0)
            self.assertEqual(image.mode, 'RGB'); self.assertEqual(image.size, (256, 256))
            self.assertEqual(capture['pixel'], (255, 255, 255)); self.assertEqual(capture['mode'], 'RGB')
            self.assertLessEqual(capture['size'][0] * capture['size'][1], 1048576)
            self.assertEqual(source.read_bytes(), original)

    async def test_missing_reference_node_or_wrong_guidance_never_submits(self):
        info = inventory(); del info['ReferenceLatent']
        with patch.object(flux, '_object_info', AsyncMock(return_value=info)), patch.object(flux, '_execute_workflow', AsyncMock()) as execute:
            with self.assertRaisesRegex(flux.Flux2ImageError, 'ReferenceLatent'):
                await flux.run_flux2_image('Edit', model='flux2-dev', references=['unused.png'])
            with self.assertRaises(ValueError):
                await flux.run_flux2_image('Edit', model='flux2-klein-4b', guidance=4)
            with self.assertRaises(ValueError):
                await flux.run_flux2_image('Edit', model='flux2-klein-9b', guidance=4)
        execute.assert_not_called()


if __name__ == '__main__':
    unittest.main()
