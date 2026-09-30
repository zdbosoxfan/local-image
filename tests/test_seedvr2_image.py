"""Native restoration boundaries and alpha/geometry preservation."""
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import AsyncMock, patch

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
import seedvr2_image as sv


def inventory():
    info = {name: {'input': {'required': {}}} for name in sv.REQUIRED_NODES}
    for node, field, options in [('UNETLoader', 'unet_name', ['nested/' + sv.MODEL_FILES['unet']]),
                                 ('VAELoader', 'vae_name', [sv.MODEL_FILES['vae']]),
                                 ('KSampler', 'sampler_name', ['euler']), ('KSampler', 'scheduler', ['simple']),
                                 ('SeedVR2PostProcessing', 'color_correction_method', ['lab', 'none'])]:
        info[node]['input']['required'][field] = ['COMBO', {'options': options}]
    return info


class SeedVR2Tests(unittest.IsolatedAsyncioTestCase):
    def test_inventory_excludes_sharp_and_requires_native_nodes(self):
        info = inventory()
        self.assertTrue(sv.seedvr2_model_option(info)['available'])
        info['UNETLoader']['input']['required']['unet_name'] = [['seedvr2_7b_sharp_fp16.safetensors']]
        self.assertFalse(sv.seedvr2_model_option(info)['available'])
        info = inventory(); del info['SeedVR2Conditioning']
        self.assertIn('SeedVR2Conditioning', sv.seedvr2_model_option(info)['reason'])

    def test_restore_conditions_on_source_latent_one_step_and_color_reference(self):
        graph = sv.build_seedvr2_workflow(sv.MODEL_FILES, 'source.png', seed=0)
        sample = graph['7']['inputs']
        self.assertEqual((sample['steps'], sample['cfg'], sample['denoise'], sample['seed']), (1, 1, 1, 0))
        self.assertEqual(sample['latent_image'], graph['6']['inputs']['vae_conditioning'])
        self.assertEqual(graph['9']['inputs']['color_correction_method'], 'lab')
        self.assertEqual(graph['5']['inputs']['overlap'], 128)
        self.assertFalse(any(node['class_type'] in ('CLIPLoader', 'CLIPTextEncode', 'EmptyLatentImage') for node in graph.values()))

    def test_bounds_preserve_source_geometry_without_app_size_caps(self):
        self.assertEqual(sv.validate_size((1536, 864), (3840, 2160)), (3840, 2160))
        self.assertEqual(sv.validate_size((1536, 864), (7680, 4320)), (7680, 4320))
        for size in ((2048, 2048), (768, 432), (1536, 864), (3841, 2161), (1, 1), (True, 256)):
            with self.assertRaises(ValueError):
                sv.validate_size((1536, 864), size)

    async def test_alpha_is_original_resample_translucent_edges_keep_source_rgb(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'source.png'
            source = Image.new('RGBA', (256, 256), (20, 40, 60, 64)); source.save(path)
            before = path.read_bytes()
            async def execute(graph):
                with Image.open(graph['3']['inputs']['image']) as prepared:
                    self.assertEqual(prepared.size, (512, 512))
                    self.assertEqual(prepared.mode, 'RGB')
                data = io.BytesIO(); Image.new('RGB', (512, 512), 'white').save(data, format='PNG'); return data.getvalue()
            with patch.object(sv, '_object_info', AsyncMock(return_value=inventory())), patch.object(sv, '_execute_workflow', execute):
                output = await sv.run_seedvr2_image(path, size=(512, 512), seed=0)
            self.assertEqual(output.getchannel('A').tobytes(), source.resize((512, 512), Image.Resampling.LANCZOS).getchannel('A').tobytes())
            self.assertLess(output.getpixel((0, 0))[0], 25)
            self.assertEqual(path.read_bytes(), before)

    async def test_wrong_output_dimensions_are_not_silently_resized(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'source.png'; Image.new('RGB', (256, 256)).save(path)
            buffer = io.BytesIO(); Image.new('RGB', (256, 256)).save(buffer, format='PNG')
            with patch.object(sv, '_object_info', AsyncMock(return_value=inventory())), patch.object(sv, '_execute_workflow', AsyncMock(return_value=buffer.getvalue())):
                with self.assertRaisesRegex(sv.SeedVR2ImageError, 'dimensions'):
                    await sv.run_seedvr2_image(path, size=(512, 512), seed=0)


if __name__ == '__main__':
    unittest.main()
