"""Check the real editor document without starting a server or opening user data."""
from html.parser import HTMLParser
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
from local_remove_frontend import render_editor


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


if __name__ == '__main__':
    unittest.main()
