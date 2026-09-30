"""Qwen API graph, capability, alpha and masked-composite regressions; no GPU."""
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import AsyncMock, patch

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
import qwen_image as qwen


def inventory():
    info = {key: {'input': {'required': {}}} for key in qwen.REQUIRED_NODES}
    for key, (node, field) in qwen.LOADERS.items():
        names = sorted({files[key] for files in qwen.MODEL_FILES.values()})
        info[node]['input']['required'][field] = [[f'qwen/{name}' for name in names]]
    info['CLIPLoader']['input']['required']['type'] = [['qwen_image']]
    info['EmptyLatentImage']['input']['required'].update({axis: ['INT', {'min': 16, 'max': 16384, 'step': 8}] for axis in ('width', 'height')})
    info['QwenImage21Cache'] = {}
    return info


def png(image):
    buffer = io.BytesIO()
    image.save(buffer, format='PNG')
    return buffer.getvalue()


class QwenGraphTests(unittest.TestCase):
    def test_choices_accept_legacy_and_native_v3_combo_fields(self):
        info = {'Node': {'input': {'required': {'legacy': [['a', 'b']], 'modern': ['COMBO', {'options': ['c', 'd']}],
                 'invalid': ['COMBO', {'options': 'not-a-list'}]}, 'optional': {'choice': ['COMBO', {'options': ['e']}]}}}}
        self.assertEqual(qwen._choices(info, 'Node', 'legacy'), ['a', 'b'])
        self.assertEqual(qwen._choices(info, 'Node', 'modern'), ['c', 'd'])
        self.assertEqual(qwen._choices(info, 'Node', 'choice'), ['e'])
        self.assertEqual(qwen._choices(info, 'Node', 'invalid'), [])
        self.assertEqual(qwen._choices(info, 'Missing', 'choice'), [])

    def test_precisions_resolve_from_live_inventory_with_subfolders(self):
        options = qwen.qwen_model_options(inventory())
        self.assertTrue(all(item['available'] for item in options))
        self.assertEqual(options[0]['files']['clip'], 'qwen/qwen3vl_8b_int8_convrot.safetensors')
        info = inventory()
        info['UNETLoader']['input']['required']['unet_name'][0].remove('qwen/qwen_image_2.1_bf16.safetensors')
        compact, full = qwen.qwen_model_options(info)
        self.assertTrue(compact['available'])
        self.assertFalse(full['available'])
        self.assertIn('qwen_image_2.1_bf16', full['reason'])

    def test_readiness_requires_qwen21_node_and_encoder_type(self):
        info = inventory()
        del info['TextEncodeQwenImage21']
        self.assertIn('Update ComfyUI', qwen.qwen_model_options(info)[0]['reason'])
        info = inventory()
        info['CLIPLoader']['input']['required']['type'] = [['qwen_image_edit']]
        self.assertFalse(qwen.qwen_model_options(info)[0]['available'])

    def test_edit_graph_keeps_alpha_and_uses_reference_latent_size(self):
        graph = qwen.build_qwen_workflow(qwen.MODEL_FILES['int8'], prompt='Cut out',
                                        references=['photo.png', 'mask.png'], size=(2048, 2048))
        self.assertEqual(graph['4']['inputs']['resolution'], 0)
        self.assertEqual(graph['4']['inputs']['vae'], ['3', 0])
        self.assertEqual(graph['6']['inputs']['latent_image'], ['4', 2])
        self.assertNotIn('5', graph)
        self.assertEqual(graph['4']['inputs']['images.image_2'], ['25', 0])
        self.assertEqual(graph['25']['class_type'], 'JoinImageWithAlpha')
        self.assertEqual(graph['25']['inputs']['alpha'], ['24', 1])
        self.assertEqual(graph['8']['class_type'], 'SaveImage')

    def test_background_graph_preserves_requested_canvas(self):
        graph = qwen.build_qwen_workflow(qwen.MODEL_FILES['bf16'], prompt='Empty studio',
                                        size=(3840, 2160), seed=123, use_cache=False)
        self.assertEqual(graph['5']['inputs'], {'width': 3840, 'height': 2160, 'batch_size': 1})
        self.assertEqual(graph['6']['inputs']['model'], ['1', 0])
        self.assertEqual(graph['6']['inputs']['seed'], 123)
        self.assertEqual(graph['2']['inputs']['type'], 'qwen_image')
        self.assertNotIn('vae', graph['4']['inputs'])

    def test_working_size_never_exceeds_budget_after_multiple32_alignment(self):
        for original in ((4000, 3000), (4096, 1025), (2503, 2001), (10000, 23), (23, 10000), (33, 33)):
            width, height = qwen.qwen_canvas_size(original)
            self.assertLessEqual(width * height, 4194304)
            self.assertLessEqual(max(width, height), 4096)
            self.assertEqual(width % 32, 0)
            self.assertEqual(height % 32, 0)
        self.assertEqual(qwen.qwen_canvas_size((12000, 600)), (3840, 192))

    def test_opaque_and_empty_cutouts_are_rejected(self):
        for mode, color in [('RGB', 'red'), ('RGBA', (255, 0, 0, 255)), ('RGBA', (0, 0, 0, 0))]:
            with self.assertRaises(qwen.QwenImageError):
                qwen.validate_cutout(Image.new(mode, (32, 32), color))

    def test_cutout_quantization_cleanup_preserves_every_soft_alpha_and_rgb_value(self):
        image = Image.new('RGBA', (256, 1))
        image.putdata([(23, 71, 149, alpha) for alpha in range(256)])
        result = qwen.finish_output_alpha(image, 'cutout')
        for value in range(256):
            pixel = result.getpixel((value, 0))
            self.assertEqual(pixel[:3], (23, 71, 149))
            self.assertEqual(pixel[3], 0 if value <= 1 else 255 if value >= 254 else value)

    def test_alpha_cleanup_does_not_change_edit_output(self):
        image = Image.new('RGBA', (256, 1))
        image.putdata([(23, 71, 149, alpha) for alpha in range(256)])
        original = image.tobytes()
        self.assertEqual(qwen.finish_output_alpha(image, 'edit').tobytes(), original)

    def test_opaque_output_composites_hidden_rgb_and_soft_edges_on_white(self):
        image = Image.new('RGBA', (4, 1))
        image.putdata([(255, 0, 255, 0), (20, 40, 60, 128), (23, 71, 149, 254), (23, 71, 149, 255)])
        original = image.tobytes()
        result = qwen.finish_output_alpha(image, 'opaque')
        self.assertEqual(list(result.getdata()), [(255, 255, 255, 255), (137, 147, 157, 255),
                                                (23, 71, 149, 255), (23, 71, 149, 255)])
        self.assertEqual(image.tobytes(), original)

    def test_background_rejects_missing_scene_but_allows_minor_alpha_dust(self):
        image = Image.new('RGBA', (100, 100), (23, 71, 149, 252))
        result = qwen.finish_output_alpha(image, 'background')
        self.assertEqual(result.getchannel('A').getextrema(), (255, 255))
        image.paste((255, 0, 255, 0), (0, 0, 20, 20))
        with self.assertRaisesRegex(qwen.QwenImageError, 'complete background scene'):
            qwen.finish_output_alpha(image, 'background')
        with self.assertRaisesRegex(qwen.QwenImageError, 'empty transparent image'):
            qwen.finish_output_alpha(Image.new('RGBA', (32, 32), (255, 0, 255, 0)), 'opaque')


class QwenExecutionTests(unittest.IsolatedAsyncioTestCase):
    async def test_opaque_generation_uses_white_matte_without_rejecting_subject_alpha(self):
        generated = Image.new('RGBA', (256, 256), (255, 0, 255, 0))
        generated.paste((20, 40, 60, 255), (80, 60, 180, 220))
        with patch.object(qwen, '_object_info', AsyncMock(return_value=inventory())), \
             patch.object(qwen, '_execute_workflow', AsyncMock(return_value=png(generated))):
            result = await qwen.run_qwen_image(task='generate', prompt='A ceramic cup', size=(256, 256), seed=0)
        self.assertEqual(result.getpixel((0, 0)), (255, 255, 255, 255))
        self.assertEqual(result.getpixel((100, 100)), (20, 40, 60, 255))

    async def test_transparent_text_generation_needs_no_reference_and_preserves_real_alpha(self):
        generated = Image.new('RGBA', (256, 256), (200, 50, 20, 0)); generated.paste((200, 50, 20, 255), (80, 60, 180, 220))
        with patch.object(qwen, '_object_info', AsyncMock(return_value=inventory())), \
             patch.object(qwen, '_execute_workflow', AsyncMock(return_value=png(generated))) as execute:
            result = await qwen.run_qwen_image(task='generate', prompt='A ceramic cup', transparent=True, size=(256, 256), seed=0)
        graph = execute.await_args.args[0]
        self.assertIn('fully transparent background', graph['4']['inputs']['prompt'])
        self.assertNotIn('empty photographic background plate', graph['4']['inputs']['prompt'])
        self.assertEqual(graph['6']['inputs']['seed'], 0)
        self.assertEqual(result.size, (256, 256)); self.assertEqual(result.getchannel('A').getextrema(), (0, 255))

    async def test_reference_generation_keeps_order_alpha_and_chosen_canvas_without_source_mutation(self):
        with tempfile.TemporaryDirectory() as folder:
            first, second = Path(folder) / 'first.png', Path(folder) / 'second.png'
            Image.new('RGBA', (128, 64), (200, 30, 40, 255)).save(first)
            Image.new('RGBA', (64, 128), (20, 50, 100, 70)).save(second)
            original = first.read_bytes(); observed = []
            async def execute(graph):
                for node_id in ('22', '24'):
                    with Image.open(graph[node_id]['inputs']['image']) as reference:
                        observed.append((reference.size, reference.getchannel('A').getextrema()))
                self.assertEqual(graph['4']['inputs']['images.image_1'], ['23', 0])
                self.assertEqual(graph['4']['inputs']['images.image_2'], ['25', 0])
                self.assertEqual(graph['6']['inputs']['latent_image'], ['4', 2])
                return png(Image.new('RGBA', (256, 256), (20, 40, 60, 254)))
            with patch.object(qwen, '_object_info', AsyncMock(return_value=inventory())), patch.object(qwen, '_execute_workflow', execute):
                result = await qwen.run_qwen_image(first, 'Combine <image1> and <image2>', reference_paths=[second], task='generate', size=(256, 256))
            self.assertEqual(observed, [((256, 256), (0, 255)), ((64, 128), (70, 70))])
            self.assertEqual(first.read_bytes(), original)
            self.assertEqual(result.size, (256, 256)); self.assertEqual(result.getchannel('A').getextrema(), (255, 255))

    async def test_transparent_generation_rejects_opaque_output_instead_of_faking_alpha(self):
        with patch.object(qwen, '_object_info', AsyncMock(return_value=inventory())), \
             patch.object(qwen, '_execute_workflow', AsyncMock(return_value=png(Image.new('RGB', (256, 256), 'red')))):
            with self.assertRaises(qwen.QwenImageError):
                await qwen.run_qwen_image(task='generate', prompt='A red cup', transparent=True, size=(256, 256))

    async def test_cutout_preserves_real_alpha_and_restores_original_dimensions(self):
        generated = Image.new('RGBA', (96, 64), (255, 0, 0, 0))
        generated.paste((255, 0, 0, 255), (30, 20, 70, 55))
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'source.png'
            Image.new('RGB', (99, 71), 'blue').save(path)
            with patch.object(qwen, '_object_info', AsyncMock(return_value=inventory())), \
                 patch.object(qwen, '_execute_workflow', AsyncMock(return_value=png(generated))) as execute:
                result = await qwen.run_qwen_image(path, seed=4)
            self.assertEqual(result.size, (99, 71))
            self.assertEqual(result.mode, 'RGBA')
            self.assertEqual(result.getchannel('A').getextrema(), (0, 255))
            graph = execute.call_args.args[0]
            self.assertIn('real alpha channel', graph['4']['inputs']['prompt'])
            self.assertEqual(graph['6']['inputs']['seed'], 4)

    async def test_background_exclusions_exist_in_positive_and_negative_at_cfg_one(self):
        with patch.object(qwen, '_object_info', AsyncMock(return_value=inventory())), \
             patch.object(qwen, '_execute_workflow', AsyncMock(return_value=png(Image.new('RGBA', (64, 64), (23, 71, 149, 254))))) as execute:
            result = await qwen.run_qwen_image(task='background', prompt='A sunlit stone courtyard', size=(64, 64))
        graph = execute.call_args.args[0]
        self.assertIn('Do not include people', graph['4']['inputs']['prompt'])
        self.assertIn('sunlit stone courtyard', graph['4']['inputs']['prompt'])
        self.assertIn('people', graph['4']['inputs']['negative_prompt'])
        self.assertEqual(graph['6']['inputs']['cfg'], 1.0)
        self.assertEqual(result.getchannel('A').getextrema(), (255, 255))
        self.assertEqual(result.getpixel((0, 0))[:3], (23, 71, 149))

    async def test_cutout_endpoint_cleanup_happens_after_resizing(self):
        generated = Image.new('RGBA', (32, 32), (23, 71, 149, 0))
        generated.paste((23, 71, 149, 255), (5, 5, 25, 25))
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'source.png'
            Image.new('RGB', (49, 53), 'blue').save(path)
            with patch.object(qwen, '_object_info', AsyncMock(return_value=inventory())), \
                 patch.object(qwen, '_execute_workflow', AsyncMock(return_value=png(generated))):
                result = await qwen.run_qwen_image(path, seed=4)
        expected = generated.resize((49, 53), Image.Resampling.LANCZOS)
        expected_alpha = expected.getchannel('A').point([0 if v <= 1 else 255 if v >= 254 else v for v in range(256)])
        self.assertEqual(result.getchannel('A').tobytes(), expected_alpha.tobytes())

    async def test_removal_keeps_every_unselected_pixel_exact(self):
        source = Image.new('RGBA', (64, 64), (23, 41, 67, 211))
        selection = Image.new('L', source.size, 0)
        selection.paste(255, (20, 20, 40, 40))
        edited = Image.new('RGBA', source.size, (240, 180, 120, 255))
        with tempfile.TemporaryDirectory() as folder:
            photo, mask = Path(folder) / 'source.png', Path(folder) / 'mask.png'
            source.save(photo); selection.save(mask)
            with patch.object(qwen, 'run_qwen_image', AsyncMock(return_value=edited)) as generate:
                result = await qwen.run_qwen_removal(photo, mask)
            for y in range(64):
                for x in range(64):
                    expected = edited if selection.getpixel((x, y)) else source
                    self.assertEqual(result.getpixel((x, y)), expected.getpixel((x, y)))
            self.assertEqual(len(generate.call_args.kwargs['reference_paths']), 1)

    async def test_no_selection_skips_gpu(self):
        with tempfile.TemporaryDirectory() as folder:
            photo, mask = Path(folder) / 'source.png', Path(folder) / 'mask.png'
            Image.new('RGB', (32, 32), 'blue').save(photo)
            Image.new('L', (32, 32), 0).save(mask)
            with patch.object(qwen, 'run_qwen_image', AsyncMock()) as generate:
                result = await qwen.run_qwen_removal(photo, mask)
            generate.assert_not_called()
            self.assertEqual(result.getpixel((0, 0)), (0, 0, 255, 255))

    async def test_missing_model_never_queues_and_disconnection_has_actionable_status(self):
        with patch.object(qwen, '_object_info', AsyncMock(return_value={})), \
             patch.object(qwen, '_execute_workflow', AsyncMock()) as execute:
            with self.assertRaisesRegex(qwen.QwenImageError, 'Update ComfyUI'):
                await qwen.run_qwen_image(task='background')
            execute.assert_not_called()
        with patch.object(qwen, '_object_info', AsyncMock(side_effect=qwen.QwenImageError('Start ComfyUI'))):
            status = await qwen.get_qwen_status()
        self.assertFalse(status['connected'])
        self.assertFalse(status['ready'])
        self.assertEqual(status['variants'][0]['reason'], 'Start ComfyUI')


if __name__ == '__main__':
    unittest.main()
