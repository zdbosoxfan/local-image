"""Offline catalogue facts and selected-folder storage estimates, without GPU or I/O mutations."""
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
import generation_model_details as details


def record(model, variant):
    return {'id': model, 'variants': [{'id': variant, 'available': False}],
            'defaults': {'variant': variant, 'steps': 4 if model == 'flux2-klein-4b' else 25,
                         'guidance': 1.0, 'width': 1024, 'height': 1024}}


class GenerationCatalogueTests(unittest.TestCase):
    def test_all_presets_have_offline_strengths_license_recommended_sampling_and_full_download_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / 'not-created'
            for model, catalog in details.CATALOGS.items():
                for variant, files in catalog.items():
                    value = details.enrich_model(record(model, variant), root)
                    size = sum(file['bytes'] for file in files)
                    self.assertEqual(value['storage_bytes'], size)
                    self.assertEqual(value['variants'][0]['missing_bytes'], size)
                    self.assertFalse(value['variants'][0]['available'])
                    self.assertEqual([item['name'] for item in value['variants'][0]['files']], [item['name'] for item in files])
                    self.assertTrue(all(not item['exists'] for item in value['variants'][0]['files']))
                    self.assertTrue(value['strengths']); self.assertTrue(value['limitations'])
                    self.assertTrue(value['license']['url'].startswith('https://'))
                    self.assertEqual(value['recommended']['width'], 1024)
                    self.assertRegex(value['hardware']['vram_recommendation'], r'\d+ GB')
                    self.assertTrue(value['hardware']['basis'])
            self.assertFalse(root.exists(), 'Browsing an offline catalogue must not create folders or download files')

    def test_selected_folder_sizes_do_not_falsely_mark_comfy_model_available(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            existing = root / 'diffusion_models' / 'tiny.safetensors'; existing.parent.mkdir(); existing.write_bytes(b'1234')
            fake = {'qwen': {'int8': ({'folder': 'diffusion_models', 'name': 'tiny.safetensors', 'bytes': 4},
                                      {'folder': 'vae', 'name': 'missing.safetensors', 'bytes': 7})}}
            with patch.object(details, 'CATALOGS', fake):
                value = details.enrich_model(record('qwen', 'int8'), root)
                self.assertEqual((value['storage_bytes'], value['variants'][0]['missing_bytes']), (11, 7))
                self.assertFalse(value['variants'][0]['available'])
                (root / 'vae').mkdir(); (root / 'vae' / 'missing.safetensors').write_bytes(b'1234567')
                value = details.enrich_model(record('qwen', 'int8'), root)
                self.assertTrue(value['variants'][0]['files_present']); self.assertFalse(value['variants'][0]['available'])

    def test_qwen_precisions_have_separate_memory_and_storage_recommendations(self):
        value = record('qwen', 'int8'); value['variants'].append({'id': 'bf16', 'available': False})
        with tempfile.TemporaryDirectory() as directory:
            value = details.enrich_model(value, Path(directory))
        compact, full = value['variants']
        self.assertLess(compact['total_bytes'], full['total_bytes'])
        self.assertEqual(compact['hardware']['vram_recommendation'], '24 GB recommended')
        self.assertEqual(full['hardware']['vram_recommendation'], '32 GB recommended')

    def test_storage_accepts_known_compatible_file_size_and_rejects_wrong_size(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); path = root / 'vae' / 'compatible.safetensors'; path.parent.mkdir(); path.write_bytes(b'old')
            artifact = {'folder': 'vae', 'name': path.name, 'bytes': 4, 'compatible_existing': ({'bytes': 3},)}
            self.assertTrue(details.matching_file(root, artifact))
            path.write_bytes(b'invalid-size')
            self.assertFalse(details.matching_file(root, artifact))


if __name__ == '__main__':
    unittest.main()
