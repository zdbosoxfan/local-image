"""Optional registered LoRAs patch only the intended diffusion-model chain."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
import qwen_image
import z_image
import flux2_image
from lora_workflow import available_loras


class LoraWorkflowTests(unittest.TestCase):
    def test_all_model_graphs_apply_optional_loras_in_order_before_model_sampling(self):
        selections = [('local-image/style.safetensors', .7), ('local-image/detail.safetensors', -0.25)]
        graphs = [
            (qwen_image.build_qwen_workflow(qwen_image.MODEL_FILES['int8'], prompt='A bowl', loras=selections), '9'),
            (z_image.build_z_image_workflow(z_image.MODEL_FILES, prompt='A bowl', loras=selections), '6'),
            (flux2_image.build_flux2_workflow(flux2_image.PRESETS['flux2-dev']['files'], model='flux2-dev', prompt='A bowl', loras=selections), '6'),
            (flux2_image.build_flux2_workflow(flux2_image.PRESETS['flux2-klein-4b']['files'], model='flux2-klein-4b', prompt='A bowl', loras=selections), '6'),
        ]
        for graph, target in graphs:
            self.assertEqual(graph['100'], {'class_type': 'LoraLoaderModelOnly', 'inputs': {
                'model': ['1', 0], 'lora_name': selections[0][0], 'strength_model': .7}})
            self.assertEqual(graph['101']['inputs']['model'], ['100', 0])
            self.assertEqual(graph['101']['inputs']['strength_model'], -.25)
            self.assertEqual(graph[target]['inputs']['model'], ['101', 0])
            self.assertEqual(graph['4']['inputs']['clip'], ['2', 0])
            self.assertEqual(sum(node['class_type'].startswith('Lora') for node in graph.values()), 2)
        graph = qwen_image.build_qwen_workflow(qwen_image.MODEL_FILES['int8'], prompt='A bowl', loras=selections, use_cache=False)
        self.assertEqual(graph['6']['inputs']['model'], ['101', 0])

    def test_lora_readiness_requires_live_loader_and_exact_registered_filename(self):
        info = {'LoraLoaderModelOnly': {'input': {'required': {'lora_name': [['local-image/style.safetensors']]}}}}
        self.assertEqual(available_loras({}, []), [])
        self.assertEqual(available_loras(info, [('local-image/style.safetensors', 0)]), [('local-image/style.safetensors', 0.0)])
        modern = {'LoraLoaderModelOnly': {'input': {'required': {'lora_name': ['COMBO', {'options': ['local-image/style.safetensors']}]}}}}
        self.assertEqual(available_loras(modern, [('local-image/style.safetensors', 1)]), [('local-image/style.safetensors', 1.0)])
        for value in [[('../style.safetensors', 1)], [('local-image/missing.safetensors', 1)],
                      [('local-image/style.safetensors', float('nan'))], [('local-image/style.ckpt', 1)],
                      [('C:/secret.safetensors', 1)], [('local-image/style.safetensors', True)]]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                available_loras(info, value)
        with self.assertRaisesRegex(ValueError, 'LoraLoaderModelOnly'):
            available_loras({}, [('local-image/style.safetensors', 1)])


if __name__ == '__main__':
    unittest.main()
