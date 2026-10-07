"""Publisher declarations and bounded plain-text model-card summaries.

Audience tags are publisher statements, not a review of model outputs. Missing
declarations remain unknown; names, filenames and prose are never classifiers.
"""
import asyncio
from collections import OrderedDict
from html.parser import HTMLParser
import re
from urllib.parse import quote

import aiohttp
import yaml

MAX_CARD_BYTES = 128 * 1024
MAX_DESCRIPTION = 600
_cards = OrderedDict()
_network = asyncio.Semaphore(4)
ADULT_TAGS = {'not-for-all-audiences', 'nsfw', 'adult', 'adult-content', 'nudity',
              'nude', 'naked', 'explicit', 'sexual-content', 'porn', 'pornography',
              'pornographic', 'hentai', 'erotic', 'erotica', '18+', 'r18'}
GENERAL_TAGS = {'sfw', 'safe-for-work', 'general-audience', 'all-ages'}
ADULT_FLAGS = {'nsfw', 'adult_content', 'nudity', 'not_for_all_audiences'}
TRUE_VALUES = {'true', 'yes', '1'}
FALSE_VALUES = {'false', 'no', '0'}


def normalized(value):
    return value.strip().casefold().replace('_', '-').replace(' ', '-') if isinstance(value, str) else ''


def content_rating(metadata):
    card = metadata.get('cardData') if isinstance(metadata.get('cardData'), dict) else {}
    tags = []
    for owner in (metadata, card):
        values = owner.get('tags', [])
        if isinstance(values, list):
            tags.extend(value for value in values[:1000] if isinstance(value, str) and len(value) <= 100)
    general = None
    for value in tags:
        tag = normalized(value)
        key, separator, flag = tag.partition(':')
        if tag in ADULT_TAGS or separator and key in {name.replace('_', '-') for name in ADULT_FLAGS} and flag in TRUE_VALUES:
            return {'content_rating': 'adult', 'content_rating_source': 'Publisher tag: ' + value}
        if tag in GENERAL_TAGS or separator and key == 'nsfw' and flag in FALSE_VALUES:
            general = 'Publisher tag: ' + value
    for key, value in card.items():
        key = normalized(key).replace('-', '_')
        flag = str(value).casefold() if type(value) is bool else ''
        if key in ADULT_FLAGS and flag in TRUE_VALUES:
            return {'content_rating': 'adult', 'content_rating_source': 'Publisher model card: ' + key + '=true'}
        if key == 'nsfw' and flag in FALSE_VALUES:
            general = 'Publisher model card: nsfw=false'
        if key in ('content_rating', 'rating', 'audience'):
            rating = normalized(value)
            if rating in ADULT_TAGS:
                return {'content_rating': 'adult', 'content_rating_source': 'Publisher model card: ' + key + '=' + rating}
            if rating in GENERAL_TAGS | {'general'}:
                general = 'Publisher model card: ' + key + '=' + rating
    return {'content_rating': 'general' if general else 'unknown', 'content_rating_source': general}


class PlainText(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.parts, self.hidden = [], 0

    def handle_starttag(self, tag, attrs):
        if tag in ('script', 'style', 'iframe', 'svg', 'template', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6'):
            self.hidden += 1
        elif not self.hidden and tag in ('p', 'div', 'br', 'li', 'h1', 'h2', 'h3'):
            self.parts.append('\n')

    def handle_endtag(self, tag):
        if tag in ('script', 'style', 'iframe', 'svg', 'template', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6') and self.hidden:
            self.hidden -= 1
        elif not self.hidden and tag in ('p', 'div', 'li'):
            self.parts.append('\n')

    def handle_data(self, value):
        if not self.hidden:
            self.parts.append(value)


def plain_description(value):
    if not isinstance(value, str):
        return ''
    value = value[:MAX_CARD_BYTES]
    value = re.sub(r'(?ms)^\s*(```|~~~).*?^\s*\1[^\n]*$', '', value)
    value = re.sub(r'!\[[^\]\n]*\]\([^\n]*?\)', '', value)
    value = re.sub(r'\[([^\]\n]+)\]\([^\n]*?\)', r'\1', value)
    value = re.sub(r'https?://\S+', '', value)
    parser = PlainText()
    parser.feed(value)
    text = ''.join(parser.parts)
    paragraphs = []
    current = []
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith(('#', '|', '[', '---', '***')):
            if current:
                paragraphs.append(' '.join(current)); current = []
            continue
        line = re.sub(r'^[>*+-]\s*', '', line)
        line = re.sub(r'[*_`~]', '', line)
        if line:
            current.append(line)
    if current:
        paragraphs.append(' '.join(current))
    for paragraph in paragraphs:
        paragraph = ' '.join(paragraph.split())
        if len(paragraph) < 25 or any(placeholder in paragraph.casefold() for placeholder in
                ('[more information needed]', 'this model card aims to', 'this model card has been generated', 'model card template')):
            continue
        if len(paragraph) <= MAX_DESCRIPTION:
            return paragraph
        return paragraph[:MAX_DESCRIPTION - 1].rsplit(' ', 1)[0] + '…'
    return ''


def card_parts(text):
    if not isinstance(text, str):
        return {}, ''
    text = text.lstrip('\ufeff')
    front = re.match(r'\A---\s*\n(.*?)\n---\s*(?:\n|$)', text, re.S)
    card = {}
    if front:
        try:
            parsed = yaml.safe_load(front.group(1)[:16 * 1024])
            if isinstance(parsed, dict):
                card = parsed
        except (yaml.YAMLError, RecursionError, ValueError):
            pass
        text = text[front.end():]
    return card, text


def presentation(metadata, readme=None):
    result = content_rating(metadata)
    card = metadata.get('cardData') if isinstance(metadata.get('cardData'), dict) else {}
    description = next((text for owner in (card, metadata) for key in ('description', 'model_description', 'summary')
                        if (text := plain_description(owner.get(key)))), '')
    if readme is not None:
        extra, body = card_parts(readme)
        declared = content_rating({'cardData': extra})
        if declared['content_rating'] == 'adult' or result['content_rating'] == 'unknown':
            result.update(declared)
        if not description:
            description = next((text for key in ('description', 'model_description', 'summary')
                                if (text := plain_description(extra.get(key)))), '') or plain_description(body)
    result['description'] = description
    return result


async def model_card(repo, revision):
    """Only pinned anonymous README text; no images, code or redirects."""
    if not isinstance(revision, str) or not re.fullmatch('[a-f0-9]{40}', revision):
        return {}
    key = (repo, revision)
    if key in _cards:
        _cards.move_to_end(key)
        return dict(_cards[key])
    url = 'https://huggingface.co/' + quote(repo, safe='/') + '/raw/' + revision + '/README.md'
    try:
        async with _network, aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=4, connect=2)) as client:
            async with client.get(url, allow_redirects=False) as response:
                if response.status == 404:
                    result = {}
                elif response.status != 200:
                    return {}
                else:
                    chunks, size = [], 0
                    async for block in response.content.iter_chunked(16 * 1024):
                        size += len(block)
                        if size > MAX_CARD_BYTES:
                            return {}
                        chunks.append(block)
                    result = presentation({}, b''.join(chunks).decode('utf-8', errors='replace'))
    except (aiohttp.ClientError, asyncio.TimeoutError, ValueError):
        return {}
    _cards[key] = result
    _cards.move_to_end(key)
    while len(_cards) > 256:
        _cards.popitem(last=False)
    return dict(result)


def merge_presentation(item, details):
    result = dict(item)
    if details.get('description') and not result.get('description'):
        result['description'] = details['description']
    if details.get('content_rating') in ('adult', 'general', 'unknown') and (
            details['content_rating'] == 'adult' or result.get('content_rating', 'unknown') == 'unknown'):
        result.update({key: details.get(key) for key in ('content_rating', 'content_rating_source')})
    if result.get('content_rating') not in ('adult', 'general', 'unknown'):
        result.update(content_rating='unknown', content_rating_source=None)
    return result
