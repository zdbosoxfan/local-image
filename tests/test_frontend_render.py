"""Check the real editor document without starting a server or opening user data."""
from html.parser import HTMLParser
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
from local_remove_frontend import render_editor, frontend_asset, frontend_manifest, frontend_mode
from frontend_tokens import issue_browser_token, valid_browser_token


class EditorDocument(HTMLParser):
    def __init__(self, html):
        super().__init__()
        self.scripts = []
        self.styles = []
        self.external_assets = []
        self.feed(html)

    def handle_starttag(self, tag, attrs):
        attributes = dict(attrs)
        if tag == 'script':
            self.scripts.append(attributes)
            if 'src' in attributes:
                self.external_assets.append(attributes['src'])
        elif tag == 'style':
            self.styles.append(attributes)
        elif tag == 'link' and attributes.get('rel') in {'stylesheet', 'preload'}:
            self.external_assets.append(attributes.get('href'))


class EditorRenderingTests(unittest.TestCase):
    def setUp(self):
        self.mode_patch = patch.dict(os.environ, {'LOCAL_IMAGE_FRONTEND': 'legacy'})
        self.mode_patch.start()

    def tearDown(self):
        self.mode_patch.stop()

    def test_windows_bom_does_not_break_first_css_selector(self):
        with tempfile.TemporaryDirectory(prefix='local-remove-assets-') as directory:
            root = Path(directory)
            (root / 'frontend').mkdir()
            (root / 'local_remove.html').write_text('<style>__EDITOR_STYLE__</style><script>__EDITOR_SCRIPT__</script>', encoding='utf-8-sig')
            (root / 'frontend' / 'editor.css').write_text(':root{--canvas:#171819}', encoding='utf-8-sig')
            (root / 'frontend' / 'editor.js').write_text("'use strict';", encoding='utf-8-sig')
            with patch('local_remove_frontend.RESOURCE_DIR', root):
                html = render_editor('nonce', 'token')
            self.assertNotIn('\ufeff', html)
            self.assertIn('<style>:root{', html)

    def test_document_contains_local_assets_and_one_matching_nonce(self):
        html = render_editor('test-nonce', 'test-token')
        document = EditorDocument(html)
        self.assertEqual([item.get('nonce') for item in document.scripts], ['test-nonce'])
        self.assertEqual([item.get('nonce') for item in document.styles], ['test-nonce'])
        self.assertEqual(document.external_assets, [], 'The editor must run without external assets')
        self.assertIn("const TOKEN='test-token'", html)
        self.assertIn('function openSession(', html)
        self.assertIn(':root', html)
        for marker in ('__EDITOR_STYLE__', '__EDITOR_SCRIPT__', '__TOKEN__', '__NONCE__'):
            self.assertNotIn(marker, html)

    def test_resources_resolve_from_installation_and_tokens_are_not_cached(self):
        previous = Path.cwd()
        with tempfile.TemporaryDirectory(prefix='local-remove-editor-') as directory:
            try:
                os.chdir(directory)
                first = render_editor('first-page-nonce', 'first-page-token')
                second = render_editor('second-page-nonce', 'second-page-token')
            finally:
                os.chdir(previous)
        self.assertIn('first-page-token', first)
        self.assertNotIn('first-page-token', second)
        self.assertNotIn('first-page-nonce', second)
        self.assertIn('second-page-token', second)
        self.assertIn('second-page-nonce', second)

    def test_normal_startup_uses_complete_react_document_without_flag(self):
        with patch.dict(os.environ, {}, clear=True):
            html = render_editor('default-page-nonce', 'default-page-token')
        document = EditorDocument(html)
        self.assertEqual([item.get('nonce') for item in document.scripts], ['default-page-nonce', 'default-page-nonce'])
        self.assertEqual(document.scripts[-1].get('type'), 'module')
        self.assertTrue(document.scripts[-1]['src'].startswith('/frontend-assets/assets/'))
        self.assertIn('window.__LOCAL_IMAGE_BOOTSTRAP__=', html)
        self.assertNotIn('function openSession(', html)
        self.assertNotIn('id="retouch-panel"', html)


class ReactDeliveryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='local-image-manifest-')
        self.root = Path(self.temporary.name)
        frontend = self.root / 'frontend'
        frontend.mkdir()
        (frontend / 'editor.css').write_text(':root{--studio-size:310px}:root[data-ui-density=large] button{font-size:28px}button{color:red}', encoding='utf-8')
        (frontend / 'editor.js').write_text("const TOKEN='__TOKEN__';/* editor */", encoding='utf-8')
        (frontend / 'migration-bridge.js').write_text('/* migration bridge */', encoding='utf-8')
        (frontend / 'batch-tools.js').write_text('/* legacy batch controls */', encoding='utf-8')
        (frontend / 'batch-bridge.js').write_text('/* batch domain ports */', encoding='utf-8')
        (frontend / 'settings-bridge.js').write_text('/* settings domain ports */', encoding='utf-8')
        (self.root / 'local_remove.html').write_text('<html><head><style nonce="__NONCE__">__EDITOR_STYLE__</style></head><body><script nonce="__NONCE__">__EDITOR_SCRIPT__</script></body></html>', encoding='utf-8')
        (frontend / 'react.html').write_text('<html><head>__FRONTEND_HEAD__</head><body><div id="react-app-root"></div><div id="viewport"><canvas id="photo"></canvas></div><script nonce="__NONCE__">window.__LOCAL_IMAGE_BOOTSTRAP__=__BOOTSTRAP__;</script>__FRONTEND_MODULE__</body></html>', encoding='utf-8')
        self.dist = self.root / 'frontend_dist'
        (self.dist / '.vite').mkdir(parents=True)
        (self.dist / 'assets').mkdir()
        self.manifest = {
            'src/main.tsx': {'file': 'assets/main-abcdefgh.js', 'isEntry': True,
                             'css': ['assets/main-abcdefgh.css'], 'imports': ['_shared.js'],
                             'dynamicImports': ['src/lazy.tsx']},
            '_shared.js': {'file': 'assets/shared-abcdefgh.js', 'css': ['assets/shared-abcdefgh.css']},
            'src/lazy.tsx': {'file': 'assets/lazy-abcdefgh.js', 'css': ['assets/lazy-abcdefgh.css'],
                             'assets': ['assets/font-abcdefgh.woff2']},
        }
        for chunk in self.manifest.values():
            for name in [chunk['file'], *chunk.get('css', []), *chunk.get('assets', [])]:
                (self.dist / name).write_bytes(b'fixture')
        self.write_manifest()
        self.resource_patch = patch('local_remove_frontend.RESOURCE_DIR', self.root)
        self.resource_patch.start()

    def tearDown(self):
        self.resource_patch.stop()
        self.temporary.cleanup()

    def write_manifest(self):
        (self.dist / '.vite' / 'manifest.json').write_text(json.dumps(self.manifest), encoding='utf-8')

    def test_react_default_and_explicit_source_checkout_rollback(self):
        with patch.dict(os.environ, {}, clear=True):
            self.assertEqual(frontend_mode(), 'react')
            html = render_editor('nonce', 'token')
        with patch.dict(os.environ, {'LOCAL_IMAGE_FRONTEND': 'unknown'}):
            self.assertEqual(frontend_mode(), 'react')
        with patch.dict(os.environ, {'LOCAL_IMAGE_FRONTEND': 'react'}):
            self.assertEqual(frontend_mode(), 'react')
        self.assertIn('window.__LOCAL_IMAGE_BOOTSTRAP__=', html)
        self.assertNotIn('migration bridge', html)
        self.assertNotIn('/* editor */', html)
        legacy = render_editor('nonce', 'token', mode='legacy')
        self.assertNotIn('migration bridge', legacy)
        self.assertNotIn('__LOCAL_IMAGE_REACT__', legacy)
        self.assertNotIn('frontend-assets', legacy)
        with patch.dict(os.environ, {'LOCAL_IMAGE_FRONTEND': 'legacy'}):
            self.assertEqual(frontend_mode(), 'legacy')
            self.assertIn('/* editor */', render_editor('nonce', 'token'))

    def test_installed_assets_ignore_stale_legacy_environment_or_mode(self):
        (self.root / 'local_remove.html').unlink()
        for value in ('', 'react', 'legacy', 'LEGACY', 'unknown'):
            with self.subTest(value=value), patch.dict(os.environ, {'LOCAL_IMAGE_FRONTEND': value}):
                self.assertEqual(frontend_mode(), 'react')
                for mode in (None, 'legacy'):
                    html = render_editor('nonce', 'token', mode=mode)
                    self.assertIn('window.__LOCAL_IMAGE_BOOTSTRAP__=', html)
                    self.assertIn('/frontend-assets/assets/main-abcdefgh.js', html)
                    self.assertNotIn('/* editor */', html)
                    self.assertNotIn('legacy batch controls', html)

    def test_installed_broken_build_fails_closed_without_legacy_fallback(self):
        (self.root / 'local_remove.html').unlink()
        (self.dist / '.vite' / 'manifest.json').unlink()
        with patch.dict(os.environ, {'LOCAL_IMAGE_FRONTEND': 'legacy'}):
            with self.assertRaises(RuntimeError) as failed:
                render_editor('nonce', 'token')
        self.assertIn('React frontend build is unavailable', str(failed.exception))
        self.assertNotIn('LOCAL_IMAGE_FRONTEND=legacy', str(failed.exception))
        self.assertIn('reinstall', str(failed.exception))

    def test_frozen_app_ignores_legacy_even_if_obsolete_template_remains(self):
        with patch('local_remove_frontend.sys.frozen', True, create=True), patch.dict(os.environ, {'LOCAL_IMAGE_FRONTEND': 'legacy'}):
            self.assertTrue((self.root / 'local_remove.html').is_file())
            self.assertEqual(frontend_mode(), 'react')
            for mode in (None, 'legacy'):
                html = render_editor('nonce', 'token', mode=mode)
                self.assertIn('/frontend-assets/assets/main-abcdefgh.js', html)
                self.assertNotIn('/* editor */', html)

    def test_migrated_batch_has_no_legacy_controller_in_react_composition(self):
        react = render_editor('nonce', 'token', mode='react')
        legacy = render_editor('nonce', 'token', mode='legacy')
        self.assertNotIn('batch domain ports', react)
        self.assertNotIn('settings domain ports', react)
        self.assertNotIn('legacy batch controls', react)
        self.assertIn('legacy batch controls', legacy)
        self.assertNotIn('batch domain ports', legacy)

    def test_migrated_assets_exclude_legacy_presentation_adapters(self):
        frontend = self.root / 'frontend'
        for name in ('stock-studio.js', 'stock-connections.js', 'quiet-controls.js'):
            (frontend / name).write_text('/* retired asset adapter: ' + name + ' */', encoding='utf-8')
        (frontend / 'assets-bridge.js').write_text('/* explicit asset domain ports */', encoding='utf-8')
        react = render_editor('nonce', 'token', mode='react')
        legacy = render_editor('nonce', 'token', mode='legacy')
        self.assertNotIn('explicit asset domain ports', react)
        self.assertNotIn('retired asset adapter:', react)
        self.assertNotIn('explicit asset domain ports', legacy)
        for name in ('stock-studio.js', 'stock-connections.js', 'quiet-controls.js'):
            self.assertIn('retired asset adapter: ' + name, legacy)

    def test_migrated_generation_uses_ports_without_legacy_dom_adapters(self):
        frontend = self.root / 'frontend'
        for name in ('generation-studio.js', 'generation-size.js', 'generation-composer.js', 'generation-guidance.js'):
            (frontend / name).write_text('/* retired generation adapter: ' + name + ' */', encoding='utf-8')
        (frontend / 'generation-bridge.js').write_text('/* explicit generation domain ports */', encoding='utf-8')
        react = render_editor('nonce', 'token', mode='react')
        legacy = render_editor('nonce', 'token', mode='legacy')
        self.assertNotIn('explicit generation domain ports', react)
        self.assertNotIn('retired generation adapter:', react)
        self.assertNotIn('explicit generation domain ports', legacy)
        self.assertIn('retired generation adapter: generation-size.js', legacy)

    def test_nonce_bootstrap_eager_css_chunks_and_no_legacy_execution(self):
        html = render_editor('page-nonce', 'page-token', mode='react')
        parsed = EditorDocument(html)
        self.assertEqual([script.get('nonce') for script in parsed.scripts], ['page-nonce', 'page-nonce'])
        self.assertEqual(parsed.scripts[1].get('type'), 'module')
        self.assertEqual(parsed.scripts[1]['src'], '/frontend-assets/assets/main-abcdefgh.js')
        self.assertIn('/frontend-assets/assets/main-abcdefgh.css', html)
        self.assertIn('/frontend-assets/assets/shared-abcdefgh.css', html)
        self.assertIn('rel="modulepreload"', html)
        self.assertNotIn('/frontend-assets/assets/lazy-abcdefgh', html)
        self.assertNotIn('@scope', html)
        self.assertNotIn('button{color:red}', html)
        self.assertNotIn('/* editor */', html)
        self.assertNotIn('/* migration bridge */', html)
        self.assertEqual(html.count('id="viewport"'), 1)
        self.assertEqual(html.count('id="photo"'), 1)
        self.assertLess(html.index('window.__LOCAL_IMAGE_BOOTSTRAP__'), html.index('type="module"'))
        self.assertIn('"nonce":"page-nonce","token":"page-token"', html)

    def test_each_document_receives_fresh_bootstrap_without_rewriting_assets(self):
        first = render_editor('first-nonce', 'first-token', mode='react')
        second = render_editor('second-nonce', 'second-token', mode='react')
        self.assertNotIn('first-token', second)
        self.assertNotIn('first-nonce', second)
        self.assertIn('second-token', second)
        self.assertIn('first-token', first)
        self.assertEqual((self.dist / 'assets/main-abcdefgh.js').read_bytes(), b'fixture')

    def test_bootstrap_values_cannot_end_script_or_quote_attributes(self):
        html = render_editor('nonce" data-bad="yes', "quote'\n</script><script>bad()</script>", mode='react')
        parsed = EditorDocument(html)
        self.assertEqual(len(parsed.scripts), 2)
        self.assertNotIn('data-bad', parsed.scripts[0])
        self.assertNotIn('<script>bad()', html)

    def test_lazy_assets_are_available_and_private_files_are_never_served(self):
        self.assertEqual(frontend_asset('assets/lazy-abcdefgh.js')[1], 'text/javascript')
        self.assertEqual(frontend_asset('assets/font-abcdefgh.woff2')[1], 'font/woff2')
        (self.dist / 'assets/unlisted-abcdefgh.js').write_text('private', encoding='utf-8')
        for name in ('../local_remove.py', 'assets/../../local_remove.py', '.vite/manifest.json',
                     'THIRD_PARTY_NOTICES.txt', 'assets/main.js', 'assets/main-abcdefgh.js.map',
                     'assets/unlisted-abcdefgh.js', 'assets\\main-abcdefgh.js'):
            with self.subTest(name=name), self.assertRaises(FileNotFoundError):
                frontend_asset(name)

    def test_broken_manifest_fails_closed_and_legacy_still_renders(self):
        self.manifest['src/main.tsx']['imports'] = ['missing']
        self.write_manifest()
        with self.assertRaisesRegex(RuntimeError, 'React frontend build is unavailable'):
            render_editor('nonce', 'token', mode='react')
        with self.assertRaises(FileNotFoundError):
            frontend_asset('assets/main-abcdefgh.js')
        self.assertIn('/* editor */', render_editor('nonce', 'token', mode='legacy'))

    def test_manifest_cannot_publish_source_or_paths_outside_package(self):
        for name in ('../local_remove.py', 'assets/main.js', 'assets/../main-abcdefgh.js',
                     'assets/main-abcdefgh.js.map', 'https://cdn.example/main-abcdefgh.js'):
            self.manifest['src/main.tsx']['file'] = name
            self.write_manifest()
            with self.subTest(name=name), self.assertRaises(RuntimeError):
                frontend_manifest()

    def test_missing_lazy_chunk_fails_package_validation(self):
        (self.dist / 'assets/lazy-abcdefgh.js').unlink()
        with self.assertRaises(RuntimeError):
            frontend_manifest()


class BrowserTokenTests(unittest.TestCase):
    def test_page_credentials_are_unique_and_do_not_contain_process_secret(self):
        secret = 'test-process-secret'
        first, second = issue_browser_token(secret), issue_browser_token(secret)
        self.assertNotEqual(first, second)
        self.assertNotIn(secret, first)
        self.assertTrue(valid_browser_token(first, secret))
        self.assertTrue(valid_browser_token(second, secret))
        # Opening another page does not expire a credential held by a picker.
        for _ in range(25):
            issue_browser_token(secret)
        self.assertTrue(valid_browser_token(first, secret))
        self.assertTrue(valid_browser_token(secret, secret))

    def test_tampering_foreign_backend_and_invalid_tokens_are_rejected(self):
        secret = 'test-process-secret'
        token = issue_browser_token(secret)
        self.assertFalse(valid_browser_token(token, 'another-process'))
        for value in ('', 'v1.bad.bad', token + 'x', token[:-1] + ('a' if token[-1] != 'a' else 'b'),
                      'v2' + token[2:], '\u00e9', 'x' * 1000, None):
            with self.subTest(value=value):
                self.assertFalse(valid_browser_token(value, secret))


if __name__ == '__main__':
    unittest.main()
