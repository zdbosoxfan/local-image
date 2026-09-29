"""Stock search, bounded downloads, opaque IDs and attribution handoff."""
import asyncio
import io
import json
from pathlib import Path
import sys
import types
import unittest
from unittest.mock import AsyncMock, patch

import aiohttp
from fastapi import HTTPException
from PIL import Image, PngImagePlugin
from pydantic import ValidationError
from starlette.requests import Request

HERE = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(HERE / 'backend'), str(HERE / 'tests' / 'helpers')]
import backend_settings_test as helper


class StockLibraryTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.fixture = helper.BackendSettingsTests(methodName='runTest'); self.fixture.setUp()
        self.imports = patch.dict(sys.modules, {'local_remove': self.fixture.app}); self.imports.start()
        self.module = types.ModuleType('stock_library_under_test')
        source = HERE / 'backend' / 'stock_library.py'
        exec(compile(source.read_text(encoding='utf-8'), str(source), 'exec'), self.module.__dict__)
        self.source = {'id': '937a1001-5040-4f05-8c39-76cdd1d0c791', 'title': '<b>Forest</b>',
            'creator': 'Photographer', 'creator_url': 'https://www.flickr.com/photos/123',
            'foreign_landing_url': 'https://www.flickr.com/photos/123/456',
            'url': 'https://live.staticflickr.com/8071/8425048235_1d3570274b_b.jpg',
            'license': 'by-sa', 'license_version': '2.0', 'license_url': 'https://creativecommons.org/licenses/by-sa/2.0/',
            'attribution': '<i>Forest</i> by Photographer, CC BY-SA 2.0', 'width': 1024, 'height': 768, 'mature': False}
        buffer = io.BytesIO(); Image.new('RGB', (12, 8), 'green').save(buffer, 'PNG'); self.png = buffer.getvalue()

    def tearDown(self):
        self.imports.stop(); self.fixture.tearDown()

    def request(self, trusted=True, token=True):
        headers = [(b'host', b'127.0.0.1:5000'),
                   (b'origin', b'http://127.0.0.1:5000' if trusted else b'https://remote.example')]
        if token: headers.append((b'x-local-remove-token', self.fixture.app.CSRF.encode()))
        return Request({'type': 'http', 'scheme': 'http', 'method': 'POST', 'path': '/', 'query_string': b'',
            'server': ('127.0.0.1', 5000), 'headers': headers})

    async def search_fixture(self):
        self.module.provider_json = AsyncMock(return_value={'results': [self.source], 'page_count': 2})
        return (await self.module.search_stock('openverse', 'forest'))['results'][0]

    def response(self, body=b'hello', status=200, headers=None, length=None, chunks=None):
        class Stream:
            async def iter_chunked(self, amount):
                for chunk in chunks if chunks is not None else [body]: yield chunk
        class Response:
            content = Stream()
            async def __aenter__(self): return self
            async def __aexit__(self, *args): return None
        response = Response(); response.status = status; response.headers = headers or {}; response.content_length = length
        return response

    def http(self, responses):
        calls = []
        class Session:
            def __init__(self, **kwargs): pass
            async def __aenter__(self): return self
            async def __aexit__(self, *args): return None
            def get(self, url, **kwargs):
                calls.append((url, kwargs)); return responses.pop(0)
        return Session, calls

    def test_host_policy_rejects_urls_credentials_ports_and_unapproved_redirect_targets(self):
        for value in ('http://live.staticflickr.com/a.jpg', 'https://127.0.0.1/a.jpg',
                      'https://live.staticflickr.com.evil.example/a.jpg', 'https://user:password@live.staticflickr.com/a.jpg',
                      'https://live.staticflickr.com:8443/a.jpg', 'file:///secret',
                      'https://api.openverse.org/v1/auth_tokens/', 'https://upload.wikimedia.org/wikipedia/commons/a.jpg'):
            with self.subTest(value=value), self.assertRaises(ValueError): self.module.checked_url(value)
        self.assertEqual(self.module.checked_url(self.source['url']), self.source['url'])
        self.assertEqual(self.module.public_link('javascript:alert(1)'), '')

    async def test_search_sanitizes_attribution_and_returns_only_local_preview_and_opaque_id(self):
        result = await self.search_fixture()
        self.assertEqual(result['title'], 'Forest')
        self.assertEqual(result['license'], 'CC BY-SA 2.0')
        self.assertEqual(result['attribution'], 'Forest by Photographer, CC BY-SA 2.0')
        self.assertEqual(len(result['id']), 32)
        self.assertTrue(result['thumbnail_url'].startswith('/api/local-remove/stock/thumbnail/'))
        self.assertNotIn('url', result); self.assertNotIn('download', result)
        params = self.module.provider_json.await_args.args[1]
        self.assertEqual(params['source'], 'flickr'); self.assertEqual(params['license'], 'by,by-sa,cc0,pdm')
        inventory = await self.module.providers(self.request())
        self.assertEqual(inventory['default_provider'], 'openverse')
        self.assertEqual([x['id'] for x in inventory['providers']], ['openverse'])

    async def test_search_skips_unsupported_hosts_licenses_and_malformed_results(self):
        records = [None, {}, {**self.source, 'url': 'https://localhost/secret.png'},
            {**self.source, 'license': 'by-nd'}, {**self.source, 'license': 'by-nc'},
            {**self.source, 'width': 100000, 'height': 100000}, self.source]
        self.module.provider_json = AsyncMock(return_value={'results': records, 'page_count': 2})
        result = await self.module.search_stock('openverse', 'forest')
        self.assertEqual(len(result['results']), 1); self.assertEqual(result['next_page'], 1)

    async def test_query_cache_is_bounded_short_and_page_specific(self):
        await self.search_fixture()
        await self.module.search_stock('openverse', 'forest')
        self.module.provider_json.assert_awaited_once()
        result = await self.module.search_stock('openverse', 'forest', 1)
        self.assertEqual(self.module.provider_json.await_count, 2)
        self.assertEqual(self.module.provider_json.await_args.args[1]['page'], '2')
        self.assertIsNone(result['next_page'])
        for provider, query, page in [('wikimedia', 'forest', 0), ('openverse', '', 0),
                                     ('openverse', 'x' * 121, 0), ('openverse', 'forest', 50)]:
            with self.assertRaises(ValueError): await self.module.search_stock(provider, query, page)

    async def test_forged_and_expired_identifiers_do_not_download(self):
        item = await self.search_fixture()
        self.module.bounded_get = AsyncMock()
        for identity in ('https://host/image.jpg', 'a' * 32):
            with self.assertRaises(ValueError): await self.module.fetch_stock_image(identity)
        self.module._results[item['id']]['expires'] = 0
        with self.assertRaisesRegex(ValueError, 'expired'): await self.module.fetch_stock_image(item['id'])
        self.module.bounded_get.assert_not_awaited()

    async def test_download_follows_only_approved_redirects(self):
        factory, calls = self.http([self.response(status=302, headers={'Location': 'http://127.0.0.1/secret'})])
        with patch.object(self.module.aiohttp, 'ClientSession', factory), self.assertRaises(ValueError):
            await self.module.bounded_get(self.source['url'])
        self.assertEqual(len(calls), 1); self.assertFalse(calls[0][1]['allow_redirects'])
        factory, calls = self.http([self.response(status=302, headers={'Location': self.source['url']}), self.response(b'image')])
        url = 'https://api.openverse.org/v1/images/' + self.source['id'] + '/thumb/'
        with patch.object(self.module.aiohttp, 'ClientSession', factory):
            self.assertEqual(await self.module.bounded_get(url), b'image')
        self.assertEqual(len(calls), 2)

    async def test_download_limits_declared_and_streamed_bytes(self):
        for response in (self.response(length=101), self.response(chunks=[b'a' * 60, b'b' * 60])):
            factory, _ = self.http([response])
            with patch.object(self.module.aiohttp, 'ClientSession', factory), self.assertRaisesRegex(ValueError, 'too large'):
                await self.module.bounded_get(self.source['url'], limit=100)

    async def test_provider_rate_limit_and_invalid_json_report_actionable_errors(self):
        factory, _ = self.http([self.response(status=429)])
        with patch.object(self.module.aiohttp, 'ClientSession', factory), self.assertRaisesRegex(ValueError, 'rate limited'):
            await self.module.bounded_get(self.source['url'])
        self.module.bounded_get = AsyncMock(return_value=b'not-json')
        with self.assertRaisesRegex(ValueError, 'invalid search data'):
            await self.module.provider_json('https://api.openverse.org/v1/images/', {})

    def test_normalization_rejects_non_images_and_pixel_bombs_and_strips_metadata(self):
        for content in (b'<svg></svg>', b'<html>error</html>'):
            with self.assertRaises(ValueError): self.module.normalize_image(content)
        with patch.object(self.module, 'MAX_PIXELS', 10), self.assertRaisesRegex(ValueError, '40 megapixels'):
            self.module.normalize_image(self.png)
        metadata = PngImagePlugin.PngInfo(); metadata.add_text('untrusted', '<script>bad()</script>')
        buffer = io.BytesIO(); Image.new('RGB', (12, 8), 'green').save(buffer, 'PNG', pnginfo=metadata)
        with Image.open(io.BytesIO(self.module.normalize_image(buffer.getvalue()))) as decoded:
            self.assertEqual(decoded.mode, 'RGBA'); self.assertEqual(decoded.size, (12, 8))
            self.assertNotIn('untrusted', decoded.info)

    async def test_fetch_uses_stored_download_and_preserves_exact_attribution_fields(self):
        item = await self.search_fixture()
        self.module.bounded_get = AsyncMock(return_value=self.png)
        content, attribution = await self.module.fetch_stock_image(item['id'])
        self.module.bounded_get.assert_awaited_once_with(self.source['url'])
        self.assertEqual(attribution['asset_id'], self.source['id'])
        self.assertEqual(attribution['license_url'], self.source['license_url'])
        self.assertEqual(set(attribution), {'provider','asset_id','title','creator','creator_url','source_url','license','license_url','attribution'})
        with Image.open(io.BytesIO(content)) as decoded: self.assertEqual(decoded.mode, 'RGBA')

    def test_embedded_color_is_converted_before_profile_removal_and_alpha_is_retained(self):
        buffer = io.BytesIO()
        Image.new('RGBA', (12, 8), (22, 180, 75, 123)).save(buffer, 'PNG', icc_profile=self.module._srgb.tobytes())
        with patch.object(self.module.ImageCms, 'profileToProfile', wraps=self.module.ImageCms.profileToProfile) as convert:
            normalized = self.module.normalize_image(buffer.getvalue())
        convert.assert_called_once()
        with Image.open(io.BytesIO(normalized)) as image:
            self.assertEqual(image.getpixel((0, 0)), (22, 180, 75, 123))
            self.assertNotIn('icc_profile', image.info)

    async def test_thumbnail_is_normalized_same_origin_and_cached(self):
        item = await self.search_fixture()
        self.module.bounded_get = AsyncMock(return_value=self.png)
        first = await self.module.thumbnail(item['id'], self.request())
        second = await self.module.thumbnail(item['id'], self.request())
        self.assertEqual(first.media_type, 'image/png'); self.assertEqual(first.body, second.body)
        self.module.bounded_get.assert_awaited_once()

    async def test_import_requires_origin_and_only_accepts_identifier_plus_target(self):
        item = await self.search_fixture()
        payload = self.module.StockImport(id=item['id'])
        self.module.fetch_stock_image = AsyncMock(return_value=(self.png, {'provider': 'openverse'}))
        self.fixture.app.import_stock_image = AsyncMock(return_value={'session': {'id': 'new-session'}, 'target': 'image'})
        for request in (self.request(False), self.request(token=False)):
            with self.assertRaises(HTTPException) as error:
                await self.module.import_stock(request, payload)
            self.assertEqual(error.exception.status_code, 403)
        self.module.fetch_stock_image.assert_not_awaited()
        result = await self.module.import_stock(self.request(), payload)
        self.assertEqual(result['session']['id'], 'new-session')
        self.fixture.app.import_stock_image.assert_awaited_once_with(self.png, {'provider': 'openverse'},
            target='image', session_id=None, revision=None)
        for changes in ({'url': 'https://host/image.png'}, {'target': 'background'}, {'revision': '2'},
                        {'session_id': 'a' * 36}, {'target': 'background', 'session_id': '../file', 'revision': 2}):
            with self.assertRaises(ValidationError): self.module.StockImport(id=item['id'], **changes)


if __name__ == '__main__': unittest.main()
