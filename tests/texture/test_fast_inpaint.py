"""Native texture repair acceptance checks; fixtures are task-owned photo copies."""
import base64
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import time
import unittest
from unittest.mock import patch

import cv2
import numpy as np
from PIL import Image

HERE = Path(__file__).resolve().parents[2] / 'backend'
sys.path.insert(0, str(HERE))
spec = importlib.util.spec_from_file_location('candidate_fast_inpaint', HERE / 'fast_inpaint.py')
heal = importlib.util.module_from_spec(spec)
spec.loader.exec_module(heal)


def decode(result, key):
    return Image.open(io.BytesIO(base64.b64decode(result[key])))


def composite(source, result):
    output = source.copy()
    output.paste(decode(result, 'color'), (result['x'], result['y']), decode(result, 'mask'))
    return output


class TextureSafetyTests(unittest.TestCase):
    def setUp(self):
        self.source = Image.fromarray(np.random.default_rng(31).integers(0, 256, (600, 800, 3), dtype=np.uint8))
        self.mask = Image.new('L', self.source.size)
        self.mask.paste(128, (370, 270, 400, 300))

    def test_native_helper_receives_exact_context_and_excludes_entire_soft_selection(self):
        observed = {}

        def run_native(command, **kwargs):
            def arg(name):
                return command[command.index(name) + 1]
            source = Image.open(command[-1]).convert('RGB')
            keep = Image.open(arg('--inpaint')).convert('L')
            donor = Image.open(arg('--sample-masks')).convert('L')
            observed.update(size=source.size, black=int((np.array(keep) == 0).sum()),
                            donor_equal=np.array_equal(keep, donor),
                            declared=arg('--out-size'), shell=kwargs['shell'], timeout=kwargs['timeout'])
            # Simulate a helper that changes every pixel; the wrapper must isolate
            # the exact original selection and keep color unweighted.
            Image.new('RGB', source.size, 'white').save(arg('--out'))
            return subprocess.CompletedProcess(command, 0)

        with patch.object(heal.subprocess, 'run', side_effect=run_native):
            result = heal.heal_image(self.source, self.mask)
        self.assertEqual(observed['size'], (512, 512))
        self.assertEqual(observed['declared'], '512x512')
        self.assertEqual(observed['black'], 30 * 30)
        self.assertTrue(observed['donor_equal'])
        self.assertFalse(observed['shell'])
        self.assertLessEqual(observed['timeout'], 120)
        color = np.asarray(decode(result, 'color'))
        mask = np.asarray(decode(result, 'mask'))
        original = np.asarray(self.source.crop((result['x'], result['y'], result['x'] + result['width'], result['y'] + result['height'])))
        self.assertTrue(np.all(color[mask > 0] == 255), 'Colors must not be preblended with the 128 alpha.')
        self.assertTrue(np.all(mask[mask > 0] == 128), 'Preserve the original soft mask.')
        self.assertTrue(np.array_equal(color[mask == 0], original[mask == 0]))
        output = np.asarray(composite(self.source, result))
        self.assertTrue(np.array_equal(output[np.asarray(self.mask) == 0], np.asarray(self.source)[np.asarray(self.mask) == 0]))

    def test_edge_selection_keeps_native_coordinates(self):
        mask = Image.new('L', self.source.size)
        mask.paste(255, (0, 0, 12, 17))
        with patch.object(heal, '_texture_repair', side_effect=lambda color, support: color.copy()):
            result = heal.heal_image(self.source, mask)
        self.assertEqual((result['x'], result['y']), (0, 0))
        self.assertTrue(np.array_equal(composite(self.source, result), self.source))

    def test_empty_all_selected_unknown_and_mismatched_requests_fail(self):
        for mask, method in [(Image.new('L', self.source.size), 'texture'),
                             (Image.new('L', self.source.size, 255), 'texture'),
                             (Image.new('L', (4, 4), 255), 'texture'),
                             (self.mask, 'unknown')]:
            with self.subTest(method=method, size=mask.size), self.assertRaises(ValueError):
                heal.heal_image(self.source, mask, method)

    def test_oversized_repair_is_rejected_without_native_process_or_rescaling(self):
        source = Image.new('RGB', (2400, 2400), 'gray')
        mask = Image.new('L', source.size)
        mask.paste(255, (300, 300, 2200, 2200))
        with patch.object(heal.subprocess, 'run') as run, self.assertRaisesRegex(ValueError, 'too large'):
            heal.heal_image(source, mask)
        run.assert_not_called()

    def test_missing_helper_and_timeout_fail_without_telea_fallback(self):
        with patch.object(heal, 'TEXTURE_EXE', HERE / 'missing.exe'), self.assertRaisesRegex(ValueError, 'missing'):
            heal.heal_image(self.source, self.mask)
        with patch.object(heal.subprocess, 'run', side_effect=subprocess.TimeoutExpired('helper', 120)), self.assertRaisesRegex(ValueError, 'too long'):
            heal.heal_image(self.source, self.mask)

    def test_native_failure_and_resized_output_are_rejected(self):
        with patch.object(heal.subprocess, 'run', return_value=subprocess.CompletedProcess([], 1)), self.assertRaisesRegex(ValueError, 'failed'):
            heal.heal_image(self.source, self.mask)

        def wrong_size(command, **kwargs):
            Image.new('RGB', (2, 2)).save(command[command.index('--out') + 1])
            return subprocess.CompletedProcess(command, 0)
        with patch.object(heal.subprocess, 'run', side_effect=wrong_size), self.assertRaisesRegex(ValueError, 'unexpected image size'):
            heal.heal_image(self.source, self.mask)

    def test_telea_remains_explicit_and_does_not_need_native_binary(self):
        with patch.object(heal, 'TEXTURE_EXE', HERE / 'missing.exe'), patch.object(heal.subprocess, 'run') as run:
            result = heal.heal_image(self.source, self.mask, 'telea')
        run.assert_not_called()
        outside = np.asarray(self.mask) == 0
        self.assertTrue(np.array_equal(np.asarray(composite(self.source, result))[outside], np.asarray(self.source)[outside]))


class NativePhotoAcceptance(unittest.TestCase):
    @unittest.skipUnless(heal.TEXTURE_EXE.is_file() and (HERE / 'baseline-crops' / 'case2-source.png').is_file(), 'Native helper and protected local photo fixture required')
    def test_real_bush_texture_and_native_pixel_preservation(self):
        source = Image.open(HERE / 'baseline-crops' / 'case2-source.png').convert('RGB')
        mask = Image.open(HERE / 'baseline-crops' / 'case2-mask.png').convert('L')
        telea = Image.open(HERE / 'baseline-crops' / 'case2-telea.png').convert('RGB')
        started = time.perf_counter()
        result = heal.heal_image(source, mask)
        elapsed = time.perf_counter() - started
        output = composite(source, result)
        outside = np.asarray(mask) == 0
        self.assertEqual(output.size, source.size)
        self.assertTrue(np.array_equal(np.asarray(output)[outside], np.asarray(source)[outside]))
        core = cv2.erode((np.asarray(mask) > 200).astype(np.uint8), np.ones((9, 9), np.uint8)) > 0
        def detail(image):
            gray = cv2.cvtColor(np.asarray(image), cv2.COLOR_RGB2GRAY).astype(np.float32)
            return float(np.std((gray - cv2.GaussianBlur(gray, (0, 0), 1.0))[core]))
        texture_detail, telea_detail = detail(output), detail(telea)
        self.assertGreater(texture_detail, telea_detail * 2, 'Texture repair must retain image detail instead of producing the smooth Telea smear.')
        directory = HERE / 'texture-synthesis' / 'production-helper-test'
        directory.mkdir(exist_ok=True)
        output.save(directory / 'bush-result.png')
        stats = {'seconds': elapsed, 'native_crop': list(source.size), 'output_patch': [result['width'], result['height']], 'texture_detail_std': texture_detail, 'telea_detail_std': telea_detail, 'outside_changed': 0}
        (directory / 'results.json').write_text(json.dumps(stats, indent=2))
        print(json.dumps(stats), flush=True)


if __name__ == '__main__':
    unittest.main(verbosity=2)
