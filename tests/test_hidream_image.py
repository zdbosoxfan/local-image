"""HiDream Full sampling and pixel-space reference boundaries."""
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import AsyncMock, patch

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
import hidream_image as hd


def inventory():
    info = {name: {'input': {'required': {}}} for name in (*hd.REQUIRED_NODES, 'LoadImage', 'HiDreamO1ReferenceImages')}
    info['CheckpointLoaderSimple']['input']['required']['ckpt_name'] = [['models/' + hd.CHECKPOINT]]
    info['KSamplerSelect']['input']['required']['sampler_name'] = ['COMBO', {'options': ['dpmpp_2m_sde_gpu']}]
    info['BasicScheduler']['input']['required']['scheduler'] = ['COMBO', {'options': ['normal']}]
    info['EmptyHiDreamO1LatentImage']['input']['required'].update({axis: ['INT', {'min': 64, 'max': 4096, 'step': 32}] for axis in ('width', 'height')})
    return info


class HiDreamTests(unittest.IsolatedAsyncioTestCase):
    def test_inventory_requires_full_checkpoint_and_native_sampling_nodes(self):
        info = inventory()
        self.assertTrue(hd.hidream_model_option(info)['available'])
        info['CheckpointLoaderSimple']['input']['required']['ckpt_name'] = [['hidream_o1_image_dev_fp8_scaled.safetensors']]
        self.assertFalse(hd.hidream_model_option(info)['available'])
        info = inventory(); del info['ModelNoiseScale']
        self.assertIn('ModelNoiseScale', hd.hidream_model_option(info)['reason'])

    def test_full_graph_uses_pixel_space_native_noise_scale_and_complete_checkpoint(self):
        graph = hd.build_hidream_workflow({'checkpoint': hd.CHECKPOINT}, prompt='Print HELLO', negative_prompt='blur', seed=0)
        self.assertEqual(graph['6']['inputs'], {'width': 2048, 'height': 2048, 'batch_size': 1})
        self.assertEqual(graph['4']['inputs']['noise_scale'], 8)
        self.assertEqual(graph['7']['inputs']['steps'], 50)
        self.assertEqual(graph['7']['inputs']['scheduler'], 'normal')
        self.assertEqual(graph['9']['inputs']['cfg'], 5)
        self.assertEqual(graph['9']['inputs']['noise_seed'], 0)
        self.assertEqual(graph['3']['inputs']['text'], 'blur')
        self.assertEqual(graph['10']['inputs']['vae'], ['1', 2])
        self.assertFalse(any(node['class_type'] in ('VAELoader', 'CLIPLoader', 'VAEEncode') for node in graph.values()))

    def test_ordered_references_condition_both_branches_as_images_not_vae_latents(self):
        graph = hd.build_hidream_workflow({'checkpoint': hd.CHECKPOINT}, prompt='Combine', references=['first.png', 'second.png'])
        self.assertEqual(graph['12']['inputs']['images.image_1'], ['20', 0])
        self.assertEqual(graph['12']['inputs']['images.image_2'], ['21', 0])
        self.assertEqual(graph['9']['inputs']['positive'], ['12', 0])
        self.assertEqual(graph['9']['inputs']['negative'], ['12', 1])

    async def test_reference_preserves_original_flattens_alpha_and_bounds_pixels(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'reference.png'
            Image.new('RGBA', (3200, 1600), (10, 20, 30, 0)).save(source)
            original = source.read_bytes()
            async def execute(graph):
                with Image.open(graph['20']['inputs']['image']) as image:
                    self.assertEqual(image.mode, 'RGB')
                    self.assertEqual(image.getpixel((0, 0)), (255, 255, 255))
                    self.assertLessEqual(image.width * image.height, 4194304)
                output = io.BytesIO(); Image.new('RGB', (256, 256)).save(output, format='PNG')
                return output.getvalue()
            with patch.object(hd, '_object_info', AsyncMock(return_value=inventory())), patch.object(hd, '_execute_workflow', execute):
                result = await hd.run_hidream_image('An object', references=[source], size=(256, 256))
            self.assertEqual(result.size, (256, 256))
            self.assertEqual(source.read_bytes(), original)

    async def test_missing_reference_node_and_invalid_sampling_do_not_submit(self):
        info = inventory(); del info['HiDreamO1ReferenceImages']
        with patch.object(hd, '_object_info', AsyncMock(return_value=info)), patch.object(hd, '_execute_workflow', AsyncMock()) as submit:
            with self.assertRaisesRegex(hd.HiDreamImageError, 'reference'):
                await hd.run_hidream_image('Edit', references=['unused.png'])
            with self.assertRaises(ValueError):
                await hd.run_hidream_image('Edit', guidance=float('nan'))
        submit.assert_not_called()


if __name__ == '__main__':
    unittest.main()
