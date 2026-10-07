"""Real thumbnails and metadata boundaries; no model or external network calls."""
import asyncio
import io
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

_temporary = tempfile.TemporaryDirectory(prefix='local-image-lora-gallery-')
_environment = patch.dict(os.environ, {'LOCAL_IMAGE_DATA_DIR': str(Path(_temporary.name)/'profile')})
sys.path.insert(0, str(Path(__file__).resolve().parents[1]/'backend'))
from PIL import Image
import lora_previews as previews
from lora_catalog import CURATED

# The profile override is needed while the backend modules import, but must not
# leak into other test modules: unittest discover imports every module first.
_environment.stop()


def setUpModule():
    _environment.start()


def setUpModule():
    _environment.start()


def tearDownModule():
    _environment.stop(); _temporary.cleanup()


class FakeResponse:
    def __init__(self, status, *, location=None, content=b''):
        self.status=status; self.headers={'Location':location} if location else {}
        self.content=self; self.pixels=content
    async def __aenter__(self):return self
    async def __aexit__(self, *args):return None
    async def iter_chunked(self, size):
        for offset in range(0,len(self.pixels),size):yield self.pixels[offset:offset+size]


class FakeSession:
    def __init__(self, responses):self.responses=list(responses);self.calls=[]
    async def __aenter__(self):return self
    async def __aexit__(self, *args):return None
    def get(self, url, *, allow_redirects):
        self.calls.append((url,allow_redirects))
        return self.responses.pop(0)


class PreviewTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        previews._sources.clear();previews._images.clear();previews._ratings.clear();previews._owners.clear()

    def test_every_retained_curated_adapter_has_its_own_example(self):
        active=[entry for entry in CURATED if entry['model']!='flux2-dev']
        self.assertEqual(len(active),10)
        for entry in active:
            row=previews.with_example(entry)
            self.assertTrue(row['preview_available'],entry['title'])
            self.assertEqual(row['example_source'],'local-test')
            self.assertIn(previews.identity(entry),row['preview_url'])

    async def test_local_example_decodes_to_bounded_rgba_thumbnail(self):
        data=await previews.preview(previews.identity(CURATED[0]))
        with Image.open(io.BytesIO(data)) as image:
            self.assertEqual(image.format,'PNG')
            self.assertEqual(image.mode,'RGBA')
            self.assertLessEqual(image.width,480); self.assertLessEqual(image.height,360)

    def test_gallery_uses_publisher_widget_when_trusted(self):
        metadata={'sha':'a'*40,'cardData':{'widget':[{'output':{'url':'images/sample.png'}}]}}
        row=previews.with_example({'repo_id':'artist/style'},metadata)
        self.assertTrue(row['preview_available'])
        self.assertEqual(row['example_source'],'publisher')
        self.assertTrue(row['preview_requires_consent'])
        self.assertTrue(row['preview_url'].startswith('/api/local-remove/loras/preview/'))

    def test_missing_examples_are_explicit_not_synthetic_placeholders(self):
        row=previews.with_example({'repo_id':'artist/no-images'})
        self.assertFalse(row['preview_available']);self.assertIsNone(row['preview_url'])

    def test_rejects_untrusted_local_and_credentialed_examples(self):
        for url in ['http://127.0.0.1/model.png','https://example.com/test.png',
                    'https://user:pass@huggingface.co/a.png','https://huggingface.co/a?token=secret',
                    'https://huggingface.co/a?%53ignature=secret',
                    'https://huggingface.co/a?X-Amz-Credential=secret',
                    'https://us.aws.cdn.hf.co/a?Signature=secret',
                    'data:image/png;base64,anything','file:///C:/image.png','//localhost/a.png']:
            self.assertIsNone(previews.image_url(url,'a/style'),url)

    def test_redirect_validator_accepts_exact_signed_cdn_only(self):
        url='https://us.aws.cdn.hf.co/xet-bridge-us/example/image?Signature=public-cdn-signature&Policy=policy&Expires=1'
        self.assertEqual(previews.redirect_image_url(url),url)
        for invalid in ['https://us.aws.cdn.hf.co.evil.example/a?Signature=x',
                        'https://example.s3.amazonaws.com/a?Signature=x',
                        'https://user:pass@us.aws.cdn.hf.co/a?Signature=x',
                        'http://us.aws.cdn.hf.co/a?Signature=x',
                        'https://us.aws.cdn.hf.co:444/a?Signature=x',
                        'https://us.aws.cdn.hf.co/a?%74oken=secret',
                        'https://huggingface.co/a?Signature=x',
                        'https://us.aws.cdn.hf.co/%2e%2e/a?Signature=x']:
            self.assertIsNone(previews.redirect_image_url(invalid),invalid)

    async def test_trusted_resolve_redirect_returns_bounded_rgba_and_caches(self):
        row=previews.with_example({'repo_id':'artist/style'}, {'sha':'a'*40,
            'cardData':{'widget':[{'output':{'url':'images/sample.png'}}]}})
        key=row['preview_url'].rsplit('/',1)[-1]
        original=previews._sources[key]
        signed='https://us.aws.cdn.hf.co/xet-bridge-us/example/image?Signature=public-cdn-signature&Expires=1'
        buffer=io.BytesIO();Image.new('RGB',(800,600),(20,80,140)).save(buffer,'PNG')
        client=FakeSession([FakeResponse(302,location=signed),FakeResponse(200,content=buffer.getvalue())])
        with patch.object(previews.aiohttp,'ClientSession',return_value=client) as factory:
            pixels=await previews.preview(key,show_unrated=True)
            self.assertEqual(await previews.preview(key,show_unrated=True),pixels,'A decoded preview is reused without another fetch')
        self.assertEqual(client.calls,[(original,False),(signed,False)])
        self.assertEqual(factory.call_count,1)
        timeout=factory.call_args.kwargs['timeout']
        self.assertEqual(timeout.total,18);self.assertEqual(timeout.connect,6)
        with Image.open(io.BytesIO(pixels)) as image:
            self.assertEqual(image.format,'PNG');self.assertEqual(image.mode,'RGBA')
            self.assertLessEqual(image.width,480);self.assertLessEqual(image.height,360)

    async def test_untrusted_or_credentialed_redirect_is_never_requested(self):
        for location in ['https://untrusted.example/a.png',
                         'https://us.aws.cdn.hf.co.evil.example/a?Signature=x',
                         'https://user:pass@us.aws.cdn.hf.co/a?Signature=x',
                         'https://us.aws.cdn.hf.co/a?token=secret']:
            row=previews.with_example({'repo_id':'artist/style'},
                {'cardData':{'widget':[{'output':{'url':'images/sample.png'}}]}})
            key=row['preview_url'].rsplit('/',1)[-1]
            client=FakeSession([FakeResponse(302,location=location)])
            with patch.object(previews.aiohttp,'ClientSession',return_value=client):
                with self.assertRaisesRegex(ValueError,'outside trusted image hosting'):
                    await previews.preview(key,show_unrated=True)
            self.assertEqual(len(client.calls),1,'Reject the redirect before opening the target')
            self.assertNotIn(key,previews._images)

    async def test_redirect_chain_and_download_size_remain_bounded(self):
        row=previews.with_example({'repo_id':'artist/style'},
            {'cardData':{'widget':[{'output':{'url':'images/sample.png'}}]}})
        key=row['preview_url'].rsplit('/',1)[-1]
        client=FakeSession([FakeResponse(302,location='https://us.aws.cdn.hf.co/a?Signature=x')]*4)
        with patch.object(previews.aiohttp,'ClientSession',return_value=client):
            with self.assertRaisesRegex(ValueError,'too many times'):await previews.preview(key,show_unrated=True)
        self.assertEqual(len(client.calls),4)
        client=FakeSession([FakeResponse(200,content=b'x'*33)])
        with patch.object(previews.aiohttp,'ClientSession',return_value=client),patch.object(previews,'MAX_BYTES',32):
            with self.assertRaisesRegex(ValueError,'too large'):await previews.preview(key,show_unrated=True)
        self.assertNotIn(key,previews._images)

    async def test_unrated_publisher_preview_requires_choice_before_network_and_cached_bytes(self):
        row=previews.with_example({'repo_id':'artist/unrated'},
            {'cardData':{'widget':[{'output':{'url':'images/sample.png'}}]}})
        key=row['preview_url'].rsplit('/',1)[-1]
        self.assertTrue(row['preview_requires_consent'])
        with patch.object(previews.aiohttp,'ClientSession') as requests:
            with self.assertRaisesRegex(ValueError,'not rated'):
                await previews.preview(key)
            previews._images[key]=b'cached neutral fixture'
            with self.assertRaisesRegex(ValueError,'not rated'):
                await previews.preview(key)
            self.assertEqual(await previews.preview(key,show_unrated=True),b'cached neutral fixture')
        requests.assert_not_called()

    async def test_mature_preview_cannot_bypass_opt_in_with_unrated_choice_or_cache(self):
        item={'repo_id':'artist/mature'}
        metadata={'tags':['not-for-all-audiences'],'cardData':{'widget':[{'output':{'url':'images/sample.png'}}]}}
        hidden=previews.with_example(item,metadata)
        self.assertIsNone(hidden['preview_url'])
        self.assertEqual(hidden['content_rating'],'adult')
        visible=previews.with_example(item,metadata,show_adult=True)
        self.assertTrue(visible['preview_url'].endswith('?show_adult=true'))
        self.assertFalse(visible['preview_requires_consent'])
        key=visible['preview_url'].split('?')[0].rsplit('/',1)[-1]
        with patch.object(previews.aiohttp,'ClientSession') as requests:
            for flags in ({},{'show_unrated':True}):
                with self.assertRaisesRegex(ValueError,'Enable mature'):
                    await previews.preview(key,**flags)
            previews._images[key]=b'cached neutral fixture'
            with self.assertRaisesRegex(ValueError,'Enable mature'):
                await previews.preview(key,show_unrated=True)
            self.assertEqual(await previews.preview(key,show_adult=True),b'cached neutral fixture')
        requests.assert_not_called()

    async def test_later_mature_label_revokes_previous_unrated_cached_publisher_example(self):
        item={'repo_id':'artist/updated'}
        metadata={'cardData':{'widget':[{'output':{'url':'images/sample.png'}}]}}
        previous=previews.with_example(item,metadata)
        key=previous['preview_url'].rsplit('/',1)[-1]
        previews._images[key]=b'cached neutral fixture'
        hidden=previews.with_example(item,{**metadata,'tags':['not-for-all-audiences']})
        self.assertIsNone(hidden['preview_url'])
        self.assertEqual(previews._ratings[key],'adult')
        with patch.object(previews.aiohttp,'ClientSession') as requests:
            with self.assertRaisesRegex(ValueError,'Enable mature'):
                await previews.preview(key,show_unrated=True)
            self.assertEqual(await previews.preview(key,show_adult=True),b'cached neutral fixture')
        requests.assert_not_called()

    async def test_later_label_revokes_removed_widget_url_and_local_rating_never_downgrades(self):
        item={'repo_id':'artist/removed-widget'}
        old=previews.with_example(item,{'cardData':{'widget':[{'output':{'url':'images/sample.png'}}]}})
        key=old['preview_url'].rsplit('/',1)[-1]
        previews._images[key]=b'cached neutral fixture'
        previews.with_example(item,{'tags':['nsfw']})
        with self.assertRaisesRegex(ValueError,'Enable mature'):
            await previews.preview(key,show_unrated=True)
        curated=CURATED[0]
        local=previews.identity(curated)
        previews.with_example({**curated,'content_rating':'adult'})
        rediscovered=previews.with_example(curated)
        self.assertEqual(rediscovered['content_rating'],'adult')
        self.assertIsNone(rediscovered['preview_url'])
        with self.assertRaisesRegex(ValueError,'Enable mature'):
            await previews.preview(local,show_unrated=True)

    async def test_unknown_or_path_like_preview_keys_cannot_read_files(self):
        for key in ['../../config','missing','b'*24]:
            with self.assertRaises(ValueError):await previews.preview(key)


if __name__=='__main__': unittest.main()
