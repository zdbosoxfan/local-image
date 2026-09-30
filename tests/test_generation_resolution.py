"""Connected workflow dimensions stay exact; no network or GPU execution."""
import io
from pathlib import Path
import sys
import unittest
from unittest.mock import AsyncMock, patch

from PIL import Image

sys.path[:0] = [str(Path(__file__).resolve().parents[1] / 'backend'), str(Path(__file__).resolve().parent)]
from generation_resolution import resolution_limits, validate_generation_size, upscale_resolution_limits
import qwen_image as qwen
import z_image as z
import flux2_image as flux
import ernie_image as ernie
import hidream_image as hidream
from test_qwen_image import inventory as qwen_inventory
from test_z_image import inventory as z_inventory
from test_flux2_image import inventory as flux_inventory
from test_ernie_image import inventory as ernie_inventory


class ResolutionTests(unittest.IsolatedAsyncioTestCase):
    def test_large_canvases_reach_latent_and_scheduler_without_downsizing(self):
        for size in ((3840, 2160), (4096, 2304), (7680, 4320)):
            builders = (
                (qwen.build_qwen_workflow(qwen.MODEL_FILES['int8'], prompt='A room', size=size), '5'),
                (z.build_z_image_workflow(z.MODEL_FILES, prompt='A room', size=size), '10'),
                (ernie.build_ernie_workflow(ernie.MODEL_FILES, prompt='A room', size=size), '6'),
                (hidream.build_hidream_workflow({'checkpoint': hidream.CHECKPOINT}, prompt='A room', size=size), '6'),
            )
            for graph, node in builders:
                self.assertEqual((graph[node]['inputs']['width'], graph[node]['inputs']['height']), size)
            for model in flux.PRESETS:
                graph = flux.build_flux2_workflow(flux.PRESETS[model]['files'], model=model, prompt='A room', size=size)
                for node in ('7', '8'):
                    self.assertEqual((graph[node]['inputs']['width'], graph[node]['inputs']['height']), size)

    def test_actual_node_schema_drives_bounds_including_scheduler_intersection(self):
        info = flux_inventory()
        info['EmptyFlux2LatentImage']['input']['required']['width'][1]['max'] = 12288
        info['Flux2Scheduler']['input']['required']['width'][1]['max'] = 8192
        limits = resolution_limits(info, 'flux2-klein-9b')
        self.assertEqual(limits['width'], {'min': 16, 'max': 8192, 'step': 16})
        self.assertEqual(limits['height']['max'], 16384)
        self.assertIsNone(limits['max_pixels'])
        self.assertEqual(validate_generation_size((8192, 8192), 'flux2-klein-9b', info), (8192, 8192))
        with self.assertRaisesRegex(ValueError, '8192'):
            validate_generation_size((8208, 1024), 'flux2-klein-9b', info)

    def test_qwen_t2i_and_reference_grid_are_distinct_real_constraints(self):
        info = qwen_inventory()
        limits = resolution_limits(info, 'qwen')
        self.assertEqual(limits['dimension_step'], 16)
        self.assertEqual(validate_generation_size((3840, 2160), 'qwen', info), (3840, 2160))
        refs = limits['reference_dimensions']
        self.assertEqual(refs['dimension_step'], 32)
        self.assertIsNone(refs['max_dimension'])
        with self.assertRaisesRegex(ValueError, 'multiples of 32'):
            validate_generation_size((3840, 2160), 'qwen', info, references=True)
        self.assertEqual(validate_generation_size((4096, 2304), 'qwen', info, references=True), (4096, 2304))

    def test_offline_catalogue_does_not_invent_an_upper_bound(self):
        for model in ('qwen', 'z-image-turbo', 'flux2-klein-4b', 'ernie-image'):
            limits = resolution_limits({}, model)
            self.assertIsNone(limits['max_dimension'])
            self.assertIsNone(limits['max_pixels'])
            self.assertFalse(limits['resolution_known'])
        limits = upscale_resolution_limits({})
        self.assertEqual(limits['dimension_step'], 2)
        self.assertIsNone(limits['max_dimension'])
        self.assertIsNone(limits['max_pixels'])

    def test_z_variation_does_not_inherit_unused_empty_latent_node_maximum(self):
        info = z_inventory()
        limits = resolution_limits(info, 'z-image-turbo')['reference_dimensions']
        self.assertEqual(limits['dimension_step'], 16)
        self.assertEqual(limits['resolution_node'], 'VAEEncode')
        self.assertIsNone(limits['max_dimension'])
        self.assertEqual(validate_generation_size((16400, 1024), 'z-image-turbo', info, references=True), (16400, 1024))

    async def test_runtime_validates_fresh_workflow_before_submitting(self):
        for module, inventory, run in (
            (qwen, qwen_inventory, lambda: qwen.run_qwen_image(prompt='A room', task='generate', size=(16400, 1024))),
            (z, z_inventory, lambda: z.run_z_image('A room', size=(16400, 1024))),
            (flux, flux_inventory, lambda: flux.run_flux2_image('A room', model='flux2-klein-9b', size=(16400, 1024))),
            (ernie, ernie_inventory, lambda: ernie.run_ernie_image('A room', size=(16400, 1024))),
        ):
            with patch.object(module, '_object_info', AsyncMock(return_value=inventory())), patch.object(module, '_execute_workflow', AsyncMock()) as execute:
                with self.assertRaisesRegex(ValueError, '16384'):
                    await run()
                execute.assert_not_awaited()

    async def test_wrong_size_output_is_not_turned_into_a_fake_high_resolution_result(self):
        data = io.BytesIO(); Image.new('RGB', (32, 32), 'white').save(data, format='PNG')
        for module, inventory, run in (
            (qwen, qwen_inventory, lambda: qwen.run_qwen_image(prompt='A room', task='generate', size=(3840, 2160))),
            (z, z_inventory, lambda: z.run_z_image('A room', size=(3840, 2160))),
            (flux, flux_inventory, lambda: flux.run_flux2_image('A room', model='flux2-klein-9b', size=(3840, 2160))),
        ):
            with patch.object(module, '_object_info', AsyncMock(return_value=inventory())), patch.object(module, '_execute_workflow', AsyncMock(return_value=data.getvalue())):
                with self.assertRaisesRegex(RuntimeError, 'not resized'):
                    await run()


if __name__ == '__main__':
    unittest.main()
