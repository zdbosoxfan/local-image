"""Native Z-Image Turbo graph and honest input-capability checks; no GPU."""
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import AsyncMock, patch

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
import z_image as z


def inventory():
    info = {node: {'input': {'required': {}}} for node in (*z.REQUIRED_NODES, 'VAEEncode', 'LoadImage')}
    for role, (node, field) in z.LOADERS.items():
        info[node]['input']['required'][field] = [[f'zimage/{z.MODEL_FILES[role]}']]
    info['CLIPLoader']['input']['required']['type'] = [['lumina2']]
    info['KSampler']['input']['required']['sampler_name'] = [['res_multistep', 'euler']]
    info['EmptySD3LatentImage']['input']['required'].update({axis: ['INT', {'min': 16, 'max': 16384, 'step': 16}] for axis in ('width', 'height')})
    return info


def png(image):
    output = io.BytesIO(); image.save(output, format='PNG'); return output.getvalue()


class ZImageGraphTests(unittest.TestCase):
    def test_inventory_resolves_native_files_and_rejects_missing_sampler(self):
        option = z.z_image_model_option(inventory())
        self.assertTrue(option['available']); self.assertEqual(option['files']['vae'], 'zimage/ae.safetensors')
        info = inventory(); info['KSampler']['input']['required']['sampler_name'] = [['euler']]
        self.assertFalse(z.z_image_model_option(info)['available'])
        self.assertIn('res_multistep', z.z_image_model_option(info)['reason'])

    def test_text_graph_matches_native_turbo_sampling_and_has_no_fake_reference(self):
        graph = z.build_z_image_workflow(z.MODEL_FILES, prompt='A blue vase', size=(768, 512), seed=0)
        self.assertEqual(graph['2']['inputs']['type'], 'lumina2')
        self.assertEqual(graph['6']['inputs']['shift'], 3)
        sampler = graph['7']['inputs']
        self.assertEqual((sampler['steps'], sampler['cfg'], sampler['sampler_name'], sampler['scheduler'], sampler['denoise']),
                         (8, 1.0, 'res_multistep', 'simple', 1.0))
        self.assertEqual(sampler['seed'], 0)
        self.assertEqual(graph['5']['class_type'], 'ConditioningZeroOut')
        self.assertEqual(graph['10']['class_type'], 'EmptySD3LatentImage')
        self.assertNotIn('11', graph)

    def test_variation_initializes_vae_latent_and_uses_denoise(self):
        graph = z.build_z_image_workflow(z.MODEL_FILES, prompt='A clay vase', input_path='photo.png', denoise=0.45)
        self.assertEqual(graph['10'], {'class_type': 'VAEEncode', 'inputs': {'pixels': ['11', 0], 'vae': ['3', 0]}})
        self.assertEqual(graph['7']['inputs']['latent_image'], ['10', 0])
        self.assertEqual(graph['7']['inputs']['denoise'], 0.45)
        self.assertEqual(graph['11']['inputs']['image'], 'photo.png')
        self.assertNotIn('reference_latents', str(graph))


class ZImageExecutionTests(unittest.IsolatedAsyncioTestCase):
    async def test_variation_crops_source_and_flattens_alpha_over_white_without_mutation(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / 'source.png'; Image.new('RGBA', (512, 256), (10, 20, 30, 0)).save(source)
            original = source.read_bytes(); captured = {}
            async def execute(graph):
                with Image.open(graph['11']['inputs']['image']) as reference:
                    captured.update(mode=reference.mode, size=reference.size, pixel=reference.getpixel((0, 0)))
                return png(Image.new('RGBA', (256, 256), (80, 90, 100, 1)))
            with patch.object(z, '_object_info', AsyncMock(return_value=inventory())), patch.object(z, '_execute_workflow', execute):
                output = await z.run_z_image('A bowl', input_path=source, size=(256, 256), seed=0, denoise=.4)
            self.assertEqual(captured, {'mode': 'RGB', 'size': (256, 256), 'pixel': (255, 255, 255)})
            self.assertEqual(output.mode, 'RGB'); self.assertEqual(output.getpixel((0, 0)), (80, 90, 100))
            self.assertEqual(source.read_bytes(), original)

    async def test_missing_vae_encoder_rejects_variation_before_gpu(self):
        info = inventory(); del info['VAEEncode']
        with patch.object(z, '_object_info', AsyncMock(return_value=info)), patch.object(z, '_execute_workflow', AsyncMock()) as execute:
            with self.assertRaisesRegex(z.ZImageError, 'VAEEncode'):
                await z.run_z_image('A bowl', input_path='unused.png')
        execute.assert_not_called()


if __name__ == '__main__':
    unittest.main()
