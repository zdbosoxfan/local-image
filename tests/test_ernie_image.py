"""ERNIE Base native sampling, loader identity and raw text preservation."""
import io
from pathlib import Path
import sys
import unittest
from unittest.mock import AsyncMock, patch
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
import ernie_image as er


def inventory():
    info = {name: {'input': {'required': {}}} for name in er.REQUIRED_NODES}
    info['EmptyFlux2LatentImage']['input']['required'].update({axis: ['INT', {'min': 16, 'max': 16384, 'step': 16}] for axis in ('width', 'height')})
    for node, field, options in [('UNETLoader', 'unet_name', ['nested/' + er.MODEL_FILES['unet']]),
                                 ('CLIPLoader', 'clip_name', [er.MODEL_FILES['clip']]), ('CLIPLoader', 'type', ['flux2']),
                                 ('VAELoader', 'vae_name', [er.MODEL_FILES['vae']]),
                                 ('KSampler', 'sampler_name', ['euler']), ('KSampler', 'scheduler', ['simple'])]:
        info[node]['input']['required'][field] = ['COMBO', {'options': options}]
    return info


class ErnieTests(unittest.IsolatedAsyncioTestCase):
    def test_inventory_requires_base_not_turbo_and_correct_encoder(self):
        info = inventory(); self.assertTrue(er.ernie_model_option(info)['available'])
        info['UNETLoader']['input']['required']['unet_name'] = [['ernie-image-turbo.safetensors']]
        self.assertFalse(er.ernie_model_option(info)['available'])
        info = inventory(); info['CLIPLoader']['input']['required']['clip_name'] = [['qwen_3_4b.safetensors']]
        self.assertFalse(er.ernie_model_option(info)['available'])

    def test_raw_prompt_and_negative_are_not_rewritten_with_base_sampling(self):
        graph = er.build_ernie_workflow(er.MODEL_FILES, prompt='Print "SHAPE & SOUND" exactly.', negative_prompt='extra text', seed=0)
        self.assertEqual(graph['4']['inputs']['text'], 'Print "SHAPE & SOUND" exactly.')
        self.assertEqual(graph['5']['inputs']['text'], 'extra text')
        self.assertEqual(graph['2']['inputs']['type'], 'flux2')
        sampler = graph['7']['inputs']
        self.assertEqual((sampler['steps'], sampler['cfg'], sampler['seed'], sampler['sampler_name'], sampler['scheduler']), (50, 4, 0, 'euler', 'simple'))
        self.assertEqual(sampler['model'], ['1', 0])
        self.assertFalse(any(node['class_type'] in ('FluxGuidance', 'ConditioningZeroOut', 'ModelSamplingFlux', 'ReferenceLatent') for node in graph.values()))

    async def test_run_preserves_native_dimensions_and_rejects_wrong_output(self):
        data = io.BytesIO(); Image.new('RGB', (256, 512)).save(data, format='PNG')
        with patch.object(er, '_object_info', AsyncMock(return_value=inventory())), patch.object(er, '_execute_workflow', AsyncMock(return_value=data.getvalue())):
            result = await er.run_ernie_image('A poster', size=(256, 512), seed=0)
            self.assertEqual(result.size, (256, 512)); self.assertEqual(result.mode, 'RGB')
            with self.assertRaisesRegex(er.ErnieImageError, 'dimensions'):
                await er.run_ernie_image('A poster', size=(512, 512))

    async def test_invalid_sampling_never_queues(self):
        with patch.object(er, '_execute_workflow', AsyncMock()) as submit:
            for values in ({'steps': 0}, {'steps': True}, {'guidance': float('nan')}, {'guidance': 11}):
                with self.assertRaises(ValueError):
                    await er.run_ernie_image('A poster', **values)
        submit.assert_not_called()


if __name__ == '__main__':
    unittest.main()
