"""Opt-in integration against an explicitly verified isolated running backend.

This is an authenticated HTTP client, not a WebView2 picker/download-dialog test.
No models, providers, installed user state, or backend-internal mocks are used.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import sys
import unittest
import urllib.error
import urllib.parse
import urllib.request
import uuid
import zipfile

import numpy as np
from PIL import Image, ImageCms
import tifffile

ROOT = Path(__file__).resolve().parents[1]
CONFIG = None


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, response, code, message, headers, new_url):
        return None


def sha256(value):
    return hashlib.sha256(value).hexdigest()


class HttpPrecisionTests(unittest.TestCase):
    def setUp(self):
        if CONFIG is None:
            self.skipTest('Requires explicit --url and --expected-profile for isolated HTTP integration')
        self.base = CONFIG.url.rstrip('/')
        parsed = urllib.parse.urlsplit(self.base)
        self.assertEqual((parsed.scheme, parsed.hostname, parsed.path), ('http', '127.0.0.1', ''))
        self.assertIsNotNone(parsed.port)
        self.expected_profile = CONFIG.expected_profile.resolve()
        self.output = CONFIG.output.resolve()
        self.assertTrue(self.expected_profile.is_relative_to(ROOT / 'qa-artifacts'))
        self.assertTrue(self.output.is_relative_to(ROOT / 'qa-artifacts'))
        self.opener = urllib.request.build_opener(NoRedirect())
        self.token = None
        self.native_key = None
        # Verify identity and exact isolated profile before reading a credential
        # or writing fixtures, sessions, outputs or result files.
        self.runtime = self.request('/api/local-remove/runtime')
        self.assertEqual(self.runtime['application'], 'local-remove')
        self.assertEqual(Path(self.runtime['data_root']).resolve(), self.expected_profile)
        self.native_key = (self.expected_profile / 'state' / 'launcher.key').read_text(encoding='ascii').strip()
        page = self.request('/remove').decode('utf-8')
        match = re.search(r"const TOKEN\s*=\s*['\"]([^'\"]+)['\"]", page)
        self.assertIsNotNone(match, 'Rendered /remove must provide its request-specific browser token')
        self.token = match.group(1)
        self.output.mkdir(parents=True, exist_ok=True)
        self.run_directory = self.output / ('run-' + uuid.uuid4().hex)
        self.run_directory.mkdir()
        self.checks = []
        self.artifacts = {}
        self.completed = False

    def tearDown(self):
        if not hasattr(self, 'checks'):
            return
        report = {'completed': self.completed, 'command': [sys.executable, *sys.argv],
                  'transport': 'real loopback HTTP; native endpoint credential supplied by test client',
                  'not_validated': ['WebView2 host', 'owned Windows dialogs', 'native download completion', 'GPU inference'],
                  'url': self.base, 'runtime': self.runtime, 'fixture_directory': str(self.run_directory),
                  'checks': self.checks, 'artifacts': self.artifacts}
        (self.output / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')

    def request(self, path, payload=None, *, native=False, method=None):
        self.assertTrue(path.startswith('/'))
        self.assertFalse(path.startswith('//'))
        headers = {'Origin': self.base}
        if payload is not None:
            headers['Content-Type'] = 'application/json'
        if native:
            self.assertTrue(path in {'/api/local-remove/open-local', '/api/local-remove/save-project', '/api/local-remove/open-project'})
            headers['x-local-launcher'] = self.native_key
        elif self.token:
            headers['x-local-remove-token'] = self.token
        request = urllib.request.Request(self.base + path, headers=headers, method=method,
                                         data=json.dumps(payload).encode('utf-8') if payload is not None else None)
        with self.opener.open(request, timeout=60) as response:
            value = response.read()
            return json.loads(value) if response.headers.get_content_type() == 'application/json' else value

    def checked(self, name, **details):
        item = {'name': name, 'passed': True, **details}
        self.checks.append(item)
        print('PASS: ' + name, flush=True)

    def artifact(self, name, payload):
        target = self.run_directory / name
        target.write_bytes(payload)
        self.artifacts[name] = {'path': str(target), 'bytes': len(payload), 'sha256': sha256(payload)}
        return target

    def download(self, session, output_format, name):
        result = self.request(f"/api/local-remove/session/{session['id']}/save", {'revision': session['revision'], 'format': output_format})
        self.assertTrue(result['saved'])
        self.assertEqual(result['mode'], 'export')
        path = result['download']
        self.assertTrue(path.startswith(f"/api/local-remove/session/{session['id']}/download?"))
        return self.artifact(name, self.request(path)), result

    def verify_tiff(self, path, expected, profile):
        with tifffile.TiffFile(path) as image:
            actual = image.asarray()
            self.assertEqual(actual.dtype, np.dtype('uint16'))
            self.assertEqual(image.pages[0].tags[34675].value, profile)
        np.testing.assert_array_equal(actual, expected)

    def test_native_precision_projects_and_source_save_over_http(self):
        raw = np.random.default_rng(907).integers(1, 65535, (48, 64, 4), dtype=np.uint16)
        raw[..., 3] = 65535
        raw[:4, :, :] = 0
        raw[8:12, 7:19, 3] = 32123
        profile = ImageCms.ImageCmsProfile(ImageCms.createProfile('sRGB')).tobytes()
        source = self.run_directory / 'precision.tif'
        tifffile.imwrite(source, raw, photometric='rgb', extrasamples='unassalpha', metadata=None,
                         extratags=[(34675, 'B', len(profile), profile, False)])
        source_bytes = source.read_bytes()
        self.artifacts['source'] = {'path': str(source), 'sha256': sha256(source_bytes), 'icc_sha256': sha256(profile), 'shape': list(raw.shape), 'bit_depth': 16}
        session = self.request('/api/local-remove/open-local', {'path': str(source)}, native=True)
        self.assertEqual(session['bit_depth'], 16)
        self.assertTrue(session['can_return'])
        session = self.request(f"/api/local-remove/session/{session['id']}/stack", {'revision': session['revision']})
        self.assertEqual(len(session['layer_stack']), 1)
        self.assertFalse(session['dirty'])
        self.checked('Native-auth Open Local and revision-preserving stack enable', session_id=session['id'])

        original_tiff, result = self.download(session, 'original', 'export-original.tif')
        self.assertEqual(result['bit_depth'], 16)
        self.verify_tiff(original_tiff, raw, profile)
        self.checked('Original-format TIFF preserves every untouched 16-bit RGBA sample and ICC bytes', download_sha256=sha256(original_tiff.read_bytes()))
        png, result = self.download(session, 'png', 'export-explicit-8bit.png')
        self.assertEqual(result['bit_depth'], 8)
        with Image.open(png) as image:
            self.assertEqual(image.mode, 'RGBA')
            self.assertEqual(image.info['icc_profile'], profile)
            np.testing.assert_array_equal(np.asarray(image), np.rint(raw.astype(np.float64) / 257).astype(np.uint8))
        self.checked('Explicit PNG converts to 8-bit RGBA with alpha and ICC retained', download_sha256=sha256(png.read_bytes()))
        with self.assertRaises(urllib.error.HTTPError) as rejected:
            self.request(f"/api/local-remove/session/{session['id']}/save", {'revision': session['revision'], 'format': 'jpg'})
        self.assertEqual(rejected.exception.code, 400)
        self.assertIn('transparency', rejected.exception.read().decode('utf-8'))
        self.assertEqual(source.read_bytes(), source_bytes)
        self.checked('Transparent JPEG rejected; TIFF source remains unchanged')

        project_a = self.run_directory / 'native-save.lremove'
        saved = self.request('/api/local-remove/save-project', {'session_id': session['id'], 'revision': session['revision'], 'path': str(project_a)}, native=True)
        self.assertTrue(saved['saved']); self.assertTrue(saved['session']['project_saved'])
        initial_project_hash = sha256(project_a.read_bytes())
        session = self.request(f"/api/local-remove/session/{session['id']}/stack/layers", {'revision': session['revision'], 'kind': 'retouch', 'name': 'Precision fixture'})
        layer_id = session['selected_layer_id']
        session = self.request(f"/api/local-remove/session/{session['id']}/stack/layer/{layer_id}",
                               {'revision': session['revision'], 'opacity': .55, 'locked': True, 'transform': {'offset_x': 2, 'rotation': 12}}, method='PATCH')
        saved = self.request('/api/local-remove/save-project', {'session_id': session['id'], 'revision': session['revision']}, native=True)
        self.assertTrue(saved['saved']); self.assertNotEqual(sha256(project_a.read_bytes()), initial_project_hash)
        saved_a_hash = sha256(project_a.read_bytes())
        self.checked('Native-auth project Save resolves its existing project path and commits the current revision')
        project_b = self.run_directory / 'native-save-as.lremove'
        saved_as = self.request('/api/local-remove/save-project', {'session_id': session['id'], 'revision': session['revision'], 'path': str(project_b)}, native=True)
        self.assertTrue(saved_as['saved']); self.assertEqual(sha256(project_a.read_bytes()), saved_a_hash)
        with zipfile.ZipFile(project_b) as archive:
            manifest = json.loads(archive.read('manifest.json'))
            self.assertEqual(manifest['version'], 3)
            self.assertEqual(archive.read(manifest['original']), source_bytes)
            persisted_stack = [{key: value for key, value in layer.items() if key not in {'bounds', 'width', 'height'}} for layer in session['layer_stack']]
            self.assertEqual(manifest['layer_stack'], persisted_stack)
        reopened = self.request('/api/local-remove/open-project', {'path': str(project_b)}, native=True)['session']
        self.assertNotEqual(reopened['id'], session['id'])
        self.assertFalse(reopened['can_return'])
        self.assertEqual(reopened['layer_stack'], session['layer_stack'])
        roundtrip, _ = self.download(reopened, 'original', 'project-roundtrip.tif')
        self.verify_tiff(roundtrip, raw, profile)
        self.artifacts['native-save.lremove'] = {'path': str(project_a), 'sha256': saved_a_hash}
        self.artifacts['native-save-as.lremove'] = {'path': str(project_b), 'sha256': sha256(project_b.read_bytes())}
        self.checked('Native-auth Save As and Open Project preserve v3 layers, original asset bytes, exact TIFF samples, alpha and ICC')

        occupied = self.run_directory / 'precision-removed.tif'
        occupied.write_bytes(b'Existing task-owned filename collision fixture')
        occupied_bytes = occupied.read_bytes()
        unique = self.request(f"/api/local-remove/session/{session['id']}/save", {'revision': session['revision'], 'mode': 'unique', 'format': 'original'})
        self.assertEqual(unique['name'], 'precision-removed-2.tif')
        self.assertEqual(occupied.read_bytes(), occupied_bytes)
        self.assertEqual(source.read_bytes(), source_bytes)
        first_unique = self.run_directory / unique['name']
        self.verify_tiff(first_unique, raw, profile)
        first_unique_hash = sha256(first_unique.read_bytes())
        self.checked('Save Unique avoids an occupied name and preserves source bytes', unique_sha256=first_unique_hash)
        overwritten = self.request(f"/api/local-remove/session/{session['id']}/save", {'revision': session['revision'], 'mode': 'overwrite', 'format': 'original'})
        self.assertTrue(overwritten['returned'])
        self.assertEqual(overwritten['name'], source.name)
        self.verify_tiff(source, raw, profile)
        overwrite_hash = sha256(source.read_bytes())
        self.checked('Conflict-checked source overwrite succeeds and preserves exact samples, alpha and ICC', overwritten_sha256=overwrite_hash)

        external = raw.copy(); external[30, 30, 0] ^= np.uint16(73)
        tifffile.imwrite(source, external, photometric='rgb', extrasamples='unassalpha', metadata=None,
                         extratags=[(34675, 'B', len(profile), profile, False)])
        external_bytes = source.read_bytes()
        with self.assertRaises(urllib.error.HTTPError) as conflict:
            self.request(f"/api/local-remove/session/{session['id']}/save", {'revision': session['revision'], 'mode': 'overwrite', 'format': 'original'})
        self.assertEqual(conflict.exception.code, 409)
        self.assertEqual(source.read_bytes(), external_bytes)
        rescued = self.request(f"/api/local-remove/session/{session['id']}/save", {'revision': session['revision'], 'mode': 'unique', 'format': 'original'})
        self.assertEqual(rescued['name'], 'precision-removed-3.tif')
        rescue_path = self.run_directory / rescued['name']
        self.verify_tiff(rescue_path, raw, profile)
        self.assertEqual(source.read_bytes(), external_bytes)
        self.assertEqual(sha256(first_unique.read_bytes()), first_unique_hash)
        self.checked('External source change rejects overwrite with 409; Save Unique keeps both versions', external_source_sha256=sha256(external_bytes), rescued_sha256=sha256(rescue_path.read_bytes()))
        self.assertEqual(list(self.run_directory.glob('.local-remove-*')), [])
        self.checked('No unfinished source-save or project-save temporary files remain')
        self.completed = True


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', required=True)
    parser.add_argument('--expected-profile', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=ROOT / 'qa-artifacts/integration/http-precision')
    CONFIG = parser.parse_args()
    unittest.main(argv=[sys.argv[0]], testRunner=unittest.TextTestRunner(stream=sys.stdout, verbosity=2))
