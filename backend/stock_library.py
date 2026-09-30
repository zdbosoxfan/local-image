"""Keyless stock search with bounded, host-restricted image imports.

Search results carry opaque, expiring IDs. The browser never supplies a download
URL; provider metadata and HTML are treated as data, not executable content.
"""
import asyncio
from collections import OrderedDict
from datetime import datetime, timezone
from html import unescape
from html.parser import HTMLParser
import io
import json
import re
import time
from typing import Literal
from urllib.parse import urljoin, urlsplit
import uuid

import aiohttp
from fastapi import APIRouter, HTTPException, Request, Response
from PIL import Image, ImageCms, ImageOps
from pydantic import BaseModel, ConfigDict, Field, model_validator

from local_remove import guard
from stock_credentials import get_key, save_key

router = APIRouter(prefix='/api/local-remove/stock')
Provider = Literal['openverse', 'pexels', 'unsplash']
PROVIDERS = [
    {'id': 'openverse', 'label': 'Openverse', 'available': True,
     'description': 'Creative Commons and public-domain stock images from Flickr.'},
    {'id': 'pexels', 'label': 'Pexels', 'connect_url': 'https://www.pexels.com/api/',
     'description': 'Free stock photography from Pexels.'},
    {'id': 'unsplash', 'label': 'Unsplash', 'connect_url': 'https://unsplash.com/developers',
     'description': 'Photography from Unsplash.'},
]
PAGE_SIZE = 12
MAX_BYTES = 40 * 1024 * 1024
MAX_PIXELS = 40_000_000
RESULT_TTL = 60 * 60
MAX_RESULTS = 600
USER_AGENT = 'LocalImage/0.5 (desktop stock-image browser)'
MEDIA_HOSTS = {'live.staticflickr.com', 'api.openverse.org', 'images.pexels.com', 'images.unsplash.com', 'plus.unsplash.com'}
API_HOSTS = {'api.openverse.org', 'api.pexels.com', 'api.unsplash.com'}
_results = OrderedDict()
_search_cache = OrderedDict()
_thumbnails = OrderedDict()
_network_limit = asyncio.Semaphore(4)
_srgb = ImageCms.ImageCmsProfile(ImageCms.createProfile('sRGB'))


def now():
    return datetime.now(timezone.utc).isoformat()


class PlainText(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.parts, self.links = [], []
    def handle_data(self, data):
        self.parts.append(data)
    def handle_starttag(self, tag, attrs):
        if tag in ('br', 'p', 'div'): self.parts.append(' ')
        if tag == 'a':
            self.links.extend(value for key, value in attrs if key == 'href' and value)


def plain(value, limit=1000):
    if not isinstance(value, str): return ''
    parser = PlainText()
    parser.feed(value[:20000])
    return ' '.join(''.join(parser.parts).split())[:limit]


def public_link(value):
    if not isinstance(value, str) or len(value) > 2000 or any(ord(c) < 32 for c in value): return ''
    value = unescape(value)
    if value.startswith('//'): value = 'https:' + value
    try:
        parsed = urlsplit(value)
        if parsed.scheme in ('http', 'https') and parsed.hostname and not parsed.username and not parsed.password:
            return value
    except ValueError:
        pass
    return ''


def checked_url(value, api=False):
    try:
        parsed = urlsplit(value)
        hosts = API_HOSTS if api else MEDIA_HOSTS
        if (parsed.scheme != 'https' or parsed.hostname not in hosts or parsed.port not in (None, 443)
                or parsed.username or parsed.password or parsed.fragment or len(value) > 4000
                or any(ord(c) < 32 for c in value)):
            raise ValueError()
        if not api:
            if parsed.hostname == 'api.openverse.org' and not re.fullmatch(r'/v1/images/[a-f0-9-]{36}/thumb/', parsed.path):
                raise ValueError()
        return value
    except (TypeError, ValueError, AttributeError):
        raise ValueError('This image host is not supported by the stock library.') from None


async def bounded_get(url, *, params=None, limit=MAX_BYTES, api=False):
    checked_url(url, api)
    headers = {'User-Agent': USER_AGENT}
    # Credentials are scoped to exact API hosts. API redirects are rejected.
    host = urlsplit(url).hostname
    provider = {'api.pexels.com': 'pexels', 'api.unsplash.com': 'unsplash'}.get(host) if api else None
    if provider:
        key = get_key(provider)
        if not key: raise ValueError('Connect ' + provider.title() + ' with an API key to search this source.')
        headers['Authorization'] = ('Client-ID ' if provider == 'unsplash' else '') + key
    timeout = aiohttp.ClientTimeout(total=45, connect=10, sock_read=20)
    try:
        async with _network_limit, aiohttp.ClientSession(timeout=timeout, headers=headers) as session:
            for attempt in range(4):
                async with session.get(url, params=params, allow_redirects=False) as response:
                    if response.status in (301, 302, 303, 307, 308):
                        if api or attempt == 3:
                            raise ValueError('The provider returned an unsupported redirect. Try another image.')
                        url = checked_url(urljoin(url, response.headers.get('Location', '')))
                        params = None
                        continue
                    if response.status == 429:
                        raise ValueError('This source is temporarily rate limited. Wait a little and search again.')
                    if response.status in (401, 403) and provider:
                        raise ValueError('This stock connection was rejected. Check the API key and account access.')
                    if response.status != 200:
                        raise ValueError(f'The stock provider returned HTTP {response.status}. Try another image or search again.')
                    if response.content_length is not None and response.content_length > limit:
                        raise ValueError('This stock file is too large to import. Choose a smaller image.')
                    chunks, received = [], 0
                    async for chunk in response.content.iter_chunked(64 * 1024):
                        received += len(chunk)
                        if received > limit:
                            raise ValueError('This stock file is too large to import. Choose a smaller image.')
                        chunks.append(chunk)
                    return b''.join(chunks)
    except (aiohttp.ClientError, asyncio.TimeoutError) as error:
        raise ValueError('The stock provider could not be reached. Check your connection and try again.') from error
    raise ValueError('The stock image could not be downloaded.')


async def provider_json(url, params):
    try:
        result = json.loads(await bounded_get(url, params=params, limit=3 * 1024 * 1024, api=True))
    except (json.JSONDecodeError, UnicodeError) as error:
        raise ValueError('The stock provider returned invalid search data. Try again later.') from error
    if not isinstance(result, dict) or result.get('error'):
        raise ValueError('The stock provider could not complete that search.')
    return result


def dimensions(width, height):
    if type(width) is not int or type(height) is not int or min(width, height) < 1:
        return 0, 0
    return width, height


def remember(item, download_url, thumbnail, download_location=None):
    checked_url(download_url)
    checked_url(thumbnail)
    if not item['source_url'] or not item['license']:
        raise ValueError('This image is missing attribution or license information.')
    identity = uuid.uuid4().hex
    stamp = time.monotonic()
    while _results and (len(_results) >= MAX_RESULTS or next(iter(_results.values()))['expires'] <= stamp):
        removed, _ = _results.popitem(last=False)
        _thumbnails.pop(removed, None)
    if download_location:
        checked_url(download_location, api=True)
        location = urlsplit(download_location)
        if location.hostname != 'api.unsplash.com' or not re.fullmatch(r'/photos/[A-Za-z0-9_-]+/download', location.path):
            raise ValueError('Invalid Unsplash download tracking endpoint.')
    _results[identity] = {'item': dict(item), 'download': download_url, 'thumbnail': thumbnail,
                          'download_location': download_location, 'expires': stamp + RESULT_TTL}
    # Unsplash requires API image URLs to be hotlinked during browsing.
    preview = thumbnail if item['provider'] == 'unsplash' else '/api/local-remove/stock/thumbnail/' + identity
    return {**item, 'id': identity, 'thumbnail_url': preview}


def result_entry(identity):
    if not isinstance(identity, str) or not re.fullmatch('[a-f0-9]{32}', identity):
        raise ValueError('Choose an image from the stock search results.')
    entry = _results.get(identity)
    if not entry or entry['expires'] <= time.monotonic():
        raise ValueError('This stock result has expired. Search again to refresh it.')
    return entry


async def openverse_search(query, page):
    payload = await provider_json('https://api.openverse.org/v1/images/', {
        'q': query, 'page': str(page + 1), 'page_size': str(PAGE_SIZE),
        'source': 'flickr', 'license': 'by,by-sa,cc0,pdm', 'mature': 'false'})
    records = payload.get('results')
    if not isinstance(records, list): raise ValueError('Openverse returned invalid search results.')
    results = []
    for source in records:
        if not isinstance(source, dict): continue
        try:
            if source.get('license') not in {'by', 'by-sa', 'cc0', 'pdm'} or source.get('mature'): continue
            asset = str(uuid.UUID(source['id']))
            width, height = dimensions(source.get('width'), source.get('height'))
            if width * height > MAX_PIXELS: continue
            version = plain(source.get('license_version'), 30)
            license = {'cc0': 'CC0', 'pdm': 'Public domain'}.get(source['license'], 'CC ' + source['license'].upper())
            item = {'provider': 'openverse', 'asset_id': asset, 'title': plain(source.get('title')) or 'Stock image',
                'creator': plain(source.get('creator')) or 'See source page', 'creator_url': public_link(source.get('creator_url')),
                'license': (license + ' ' + version).strip(), 'license_url': public_link(source.get('license_url')),
                'source_url': public_link(source.get('foreign_landing_url')), 'width': width, 'height': height,
                'attribution': plain(source.get('attribution'), 4000)}
            if not item['attribution']:
                item['attribution'] = f"{item['title']} — {item['creator']}; {item['license']}. {item['source_url']}"[:4000]
            results.append(remember(item, source['url'], 'https://api.openverse.org/v1/images/' + asset + '/thumb/'))
        except (KeyError, TypeError, ValueError, AttributeError):
            continue
    count = payload.get('page_count', 0)
    return results, page + 1 if type(count) is int and page + 1 < count and page < 49 else None


async def pexels_search(query, page):
    payload = await provider_json('https://api.pexels.com/v1/search', {
        'query': query, 'page': page + 1, 'per_page': PAGE_SIZE})
    records = payload.get('photos')
    if not isinstance(records, list): raise ValueError('Pexels returned invalid search results.')
    results = []
    for source in records:
        try:
            if not isinstance(source, dict) or type(source.get('id')) is not int: continue
            width, height = dimensions(source.get('width'), source.get('height'))
            if not width or width * height > MAX_PIXELS: continue
            item = {'provider': 'pexels', 'asset_id': str(source['id']),
                    'title': plain(source.get('alt')) or 'Pexels photo',
                    'creator': plain(source.get('photographer')) or 'Pexels photographer',
                    'creator_url': public_link(source.get('photographer_url')),
                    'source_url': public_link(source.get('url')), 'license': 'Pexels License',
                    'license_url': 'https://www.pexels.com/license/', 'width': width, 'height': height}
            item['attribution'] = f"Photo by {item['creator']} on Pexels. {item['source_url']}"
            results.append(remember(item, source['src']['original'], source['src']['medium']))
        except (KeyError, TypeError, ValueError, AttributeError): continue
    return results, page + 1 if payload.get('next_page') and page < 49 else None


def unsplash_link(value):
    link = public_link(value)
    return link + ('&' if '?' in link else '?') + 'utm_source=local_image&utm_medium=referral' if link else ''


async def unsplash_search(query, page):
    payload = await provider_json('https://api.unsplash.com/search/photos', {
        'query': query, 'page': page + 1, 'per_page': PAGE_SIZE, 'content_filter': 'high'})
    records = payload.get('results')
    if not isinstance(records, list): raise ValueError('Unsplash returned invalid search results.')
    results = []
    for source in records:
        try:
            if not isinstance(source, dict) or not re.fullmatch(r'[A-Za-z0-9_-]+', source.get('id', '')): continue
            width, height = dimensions(source.get('width'), source.get('height'))
            if not width or width * height > MAX_PIXELS: continue
            user = source['user']
            item = {'provider': 'unsplash', 'asset_id': source['id'],
                    'title': plain(source.get('description') or source.get('alt_description')) or 'Unsplash photo',
                    'creator': plain(user.get('name')) or 'Unsplash photographer',
                    'creator_url': unsplash_link(user['links']['html']),
                    'source_url': unsplash_link(source['links']['html']), 'license': 'Unsplash License',
                    'license_url': 'https://unsplash.com/license', 'width': width, 'height': height}
            item['attribution'] = f"Photo by {item['creator']} on Unsplash. {item['source_url']}"
            results.append(remember(item, source['urls']['full'], source['urls']['small'], source['links']['download_location']))
        except (KeyError, TypeError, ValueError, AttributeError): continue
    count = payload.get('total_pages', 0)
    return results, page + 1 if type(count) is int and page + 1 < count and page < 49 else None


async def search_stock(provider, query, page=0):
    if provider not in {'openverse', 'pexels', 'unsplash'} or not isinstance(query, str) or not 1 <= len(query.strip()) <= 120 or any(ord(c) < 32 for c in query) or type(page) is not int or not 0 <= page <= 49:
        raise ValueError('Choose a stock provider and enter a search of 1–120 characters.')
    key = (provider, query.strip(), page)
    cached = _search_cache.get(key)
    if cached and cached[0] > time.monotonic(): return cached[1]
    results, next_page = await {'openverse': openverse_search, 'pexels': pexels_search, 'unsplash': unsplash_search}[provider](query.strip(), page)
    response = {'results': results, 'next_page': next_page, 'page': page, 'checked_at': now(),
        'warning': '' if results else 'No supported images on this page. Try another search or the next page.'}
    _search_cache[key] = (time.monotonic() + 60, response)
    while len(_search_cache) > 50: _search_cache.popitem(last=False)
    return response


def normalize_image(content, thumbnail=False):
    try:
        with Image.open(io.BytesIO(content)) as source:
            if source.format not in {'JPEG', 'PNG', 'WEBP', 'TIFF'} or source.width * source.height > MAX_PIXELS:
                raise ValueError('Choose a JPEG, PNG, WebP or TIFF image of at most 40 megapixels.')
            oriented = ImageOps.exif_transpose(source)
            image = oriented.convert('RGBA')
            if source.info.get('icc_profile'):
                try:
                    profile = ImageCms.ImageCmsProfile(io.BytesIO(source.info['icc_profile']))
                    color = oriented if oriented.mode in ('RGB', 'CMYK', 'LAB', 'L') else oriented.convert('RGB')
                    converted = ImageCms.profileToProfile(color, profile, _srgb, outputMode='RGB').convert('RGBA')
                    converted.putalpha(image.getchannel('A'))
                    image = converted
                except (OSError, ValueError, ImageCms.PyCMSError) as error:
                    raise ValueError('The stock image has an unsupported color profile. Choose another image.') from error
            if thumbnail: image.thumbnail((480, 360), Image.Resampling.LANCZOS)
            # Strip untrusted EXIF/text and embedded profiles before local storage.
            clean = Image.new('RGBA', image.size); clean.paste(image)
            output = io.BytesIO(); clean.save(output, 'PNG')
            result = output.getvalue()
            if len(result) > MAX_BYTES: raise ValueError('The decoded stock image is too large to import.')
            return result
    except (OSError, SyntaxError, Image.DecompressionBombError) as error:
        raise ValueError('The provider file is not a supported image.') from error


async def fetch_stock_image(identity):
    entry = result_entry(identity)
    if entry.get('download_location'):
        await provider_json(entry['download_location'], {})
    content = await bounded_get(entry['download'])
    data = await asyncio.to_thread(normalize_image, content)
    attribution = {key: entry['item'][key] for key in ('provider', 'asset_id', 'title', 'creator', 'creator_url',
                   'source_url', 'license', 'license_url', 'attribution')}
    return data, attribution


class StockImport(BaseModel):
    model_config = ConfigDict(extra='forbid', strict=True)
    id: str = Field(pattern='^[a-f0-9]{32}$')
    target: Literal['image', 'background'] = 'image'
    session_id: str | None = Field(default=None, max_length=100)
    revision: int | None = Field(default=None, ge=0)
    layer_id: str | None = Field(default=None, max_length=100)
    @model_validator(mode='after')
    def require_background_session(self):
        if self.target == 'background' and (not self.session_id or self.revision is None):
            raise ValueError('Choose the current image and revision for this background.')
        if self.target == 'image' and (self.session_id is not None or self.revision is not None or self.layer_id is not None):
            raise ValueError('Open stock images as a new image without a target session.')
        if self.session_id is not None and not re.fullmatch('[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}', self.session_id):
            raise ValueError('Choose an open image as the background target.')
        return self


@router.get('/providers')
async def providers(request: Request):
    guard(request)
    return {'providers': [{**item, 'available': item['id'] == 'openverse' or bool(get_key(item['id'])),
                           'needs_key': item['id'] != 'openverse'} for item in PROVIDERS],
            'default_provider': 'openverse'}


class StockConnection(BaseModel):
    model_config = ConfigDict(extra='forbid', strict=True)
    key: str = Field(max_length=512)


@router.put('/connection/{provider}')
async def connect_provider(provider: Literal['pexels', 'unsplash'], payload: StockConnection, request: Request):
    guard(request, True)
    try:
        save_key(provider, payload.key.strip())
        _search_cache.clear()
        return {'provider': provider, 'connected': bool(get_key(provider))}
    except ValueError as error: raise HTTPException(400, str(error)) from error


@router.get('/search')
async def search(request: Request, provider: Provider = 'openverse', query: str = '', page: int = 0):
    guard(request)
    try: return await search_stock(provider, query, page)
    except ValueError as error: raise HTTPException(400, str(error)) from error


@router.get('/thumbnail/{identity}')
async def thumbnail(identity: str, request: Request):
    guard(request)
    try:
        entry = result_entry(identity)
        if identity not in _thumbnails:
            content = await bounded_get(entry['thumbnail'], limit=8 * 1024 * 1024)
            _thumbnails[identity] = await asyncio.to_thread(normalize_image, content, True)
            while len(_thumbnails) > 80: _thumbnails.popitem(last=False)
        return Response(_thumbnails[identity], media_type='image/png', headers={'Cache-Control': 'private, max-age=600'})
    except ValueError as error: raise HTTPException(400, str(error)) from error


@router.post('/import')
async def import_stock(request: Request, payload: StockImport):
    guard(request, True)
    try:
        content, attribution = await fetch_stock_image(payload.id)
        import local_remove as editor
        return await editor.import_stock_image(content, attribution, target=payload.target,
                                              session_id=payload.session_id, revision=payload.revision,
                                              **({'layer_id':payload.layer_id} if payload.layer_id is not None else {}))
    except ValueError as error: raise HTTPException(400, str(error)) from error
