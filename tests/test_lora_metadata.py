"""Publisher metadata/text only; no model files or example images are fetched."""
import asyncio
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
import lora_metadata as metadata


class RatingTests(unittest.TestCase):
    def test_exact_machine_declarations_and_mature_precedence(self):
        for declaration in ({'tags':['not-for-all-audiences']}, {'tags':['NSFW']}, {'tags':['nudity']},
                            {'tags':['nsfw:true']}, {'cardData':{'nsfw':True}}, {'cardData':{'nudity':True}},
                            {'cardData':{'content_rating':'adult'}}, {'tags':['sfw','nsfw']},
                            {'tags':['not-for-all-audiences'],'cardData':{'nsfw':False}}):
            with self.subTest(declaration=declaration):
                result=metadata.content_rating(declaration)
                self.assertEqual(result['content_rating'],'adult')
                self.assertTrue(result['content_rating_source'].startswith('Publisher'))
        for declaration in ({'cardData':{'nsfw':False}}, {'tags':['sfw']}, {'tags':['nsfw:false']},
                            {'cardData':{'content_rating':'general'}}):
            self.assertEqual(metadata.content_rating(declaration)['content_rating'],'general')

    def test_absent_malformed_or_unrelated_words_stay_unknown(self):
        for declaration in ({}, {'tags':None,'cardData':[]}, {'tags':['natural-exposure','uncensored-colors']},
                            {'id':'artist/adult-nsfw','siblings':[{'rfilename':'nudity.safetensors'}]},
                            {'cardData':{'nsfw':'false','nudity':0}}, {'tags':['nsfw:falsehood']},
                            {'description':'This style changes skin exposure and does not include nudity.'}):
            self.assertEqual(metadata.content_rating(declaration)['content_rating'],'unknown',declaration)

    def test_real_card_shape_extracts_first_prose_and_html_entities(self):
        card="""---
license: apache-2.0
tags: [lora]
---
# Z-Image-Turbo - Children&#39;s Drawings
## Model description
This LoRA will turn any prompt into a child&#39;s drawing.

## Download model
Download the model using the command below.
"""
        result=metadata.presentation({},card)
        self.assertEqual(result['description'],"This LoRA will turn any prompt into a child's drawing.")
        self.assertEqual(result['content_rating'],'unknown')

    def test_description_is_plain_bounded_and_ignores_html_code_images_and_boilerplate(self):
        card='''<h2>A very long adapter heading that is not a description</h2>
<script>runDangerousCode()</script>
![Example](images/example.png)
```python
execute_repository_code()
```
An **illustration** adapter with [publisher instructions](https://huggingface.co/artist/style), soft shapes and gentle colors.
'''
        result=metadata.presentation({},card)
        self.assertEqual(result['description'],'An illustration adapter with publisher instructions, soft shapes and gentle colors.')
        text=metadata.plain_description('A detailed artistic style. '*100)
        self.assertLessEqual(len(text),metadata.MAX_DESCRIPTION)
        self.assertTrue(text.endswith('…'))
        self.assertEqual(metadata.plain_description('This model card has been generated automatically by a model card template.'),'')

    def test_model_card_custom_rating_cannot_downgrade_explicit_mature_tag(self):
        result=metadata.presentation({'tags':['nsfw']},'---\nnsfw: false\n---\nA calm style for illustrated landscapes.')
        self.assertEqual(result['content_rating'],'adult')
        result=metadata.presentation({},'---\nnsfw: true\n---\nA publisher supplied adapter description.')
        self.assertEqual(result['content_rating'],'adult')


class Response:
    status=200
    def __init__(self,content):self.content=self;self.data=content
    async def __aenter__(self):return self
    async def __aexit__(self,*args):return None
    async def iter_chunked(self,size):
        for start in range(0,len(self.data),11):yield self.data[start:start+11]


class Session:
    def __init__(self,content):self.content=content;self.calls=[]
    async def __aenter__(self):return self
    async def __aexit__(self,*args):return None
    def get(self,url,**kwargs):self.calls.append((url,kwargs));return Response(self.content)


class CardFetchTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        metadata._cards.clear();metadata._network=asyncio.Semaphore(4)

    async def test_readme_is_commit_pinned_bounded_plain_text_and_cached(self):
        client=Session(b'# Model\n\nA painterly adapter for warm landscape illustrations.')
        with patch.object(metadata.aiohttp,'ClientSession',return_value=client) as factory:
            first=await metadata.model_card('artist/style','a'*40)
            second=await metadata.model_card('artist/style','a'*40)
            self.assertEqual(first,second)
            self.assertEqual(first['description'],'A painterly adapter for warm landscape illustrations.')
            self.assertEqual(client.calls,[('https://huggingface.co/artist/style/raw/'+'a'*40+'/README.md',{'allow_redirects':False})])
            self.assertEqual(factory.call_count,1)
            self.assertEqual(factory.call_args.kwargs['timeout'].total,4)
            self.assertEqual(await metadata.model_card('artist/style','main'),{})
            self.assertEqual(factory.call_count,1)

    async def test_oversized_card_does_not_get_processed_or_cached(self):
        client=Session(b'A publisher description that exceeds a small test limit.')
        with patch.object(metadata,'MAX_CARD_BYTES',20),patch.object(metadata.aiohttp,'ClientSession',return_value=client):
            self.assertEqual(await metadata.model_card('artist/style','a'*40),{})
        self.assertEqual(metadata._cards,{})

    async def test_page_card_reads_limit_concurrent_publisher_requests(self):
        release=asyncio.Event()
        active=0
        maximum=0
        class PooledResponse(Response):
            async def __aenter__(self):
                nonlocal active,maximum
                active+=1;maximum=max(maximum,active)
                await release.wait()
                return self
            async def __aexit__(self,*args):
                nonlocal active
                active-=1
        class PooledSession(Session):
            def __init__(self,**kwargs):super().__init__(b'A publisher description for this illustrative adapter.')
            def get(self,url,**kwargs):return PooledResponse(self.content)
        with patch.object(metadata.aiohttp,'ClientSession',PooledSession):
            tasks=[asyncio.create_task(metadata.model_card(f'artist/style-{index}','a'*40)) for index in range(12)]
            await asyncio.sleep(0)
            self.assertEqual(active,4)
            release.set()
            results=await asyncio.gather(*tasks)
        self.assertEqual(maximum,4)
        self.assertTrue(all(item['description'] for item in results))


if __name__=='__main__':unittest.main()
