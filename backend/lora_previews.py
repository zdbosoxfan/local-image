"""Bounded local/publisher LoRA examples; previews never establish compatibility."""
import asyncio
from collections import OrderedDict
import hashlib
from pathlib import Path
import re
from urllib.parse import parse_qsl, quote, unquote, urljoin, urlsplit

import aiohttp
from lora_metadata import content_rating, merge_presentation

RESOURCE = Path(__file__).resolve().parent / 'frontend' / 'lora-examples'
HOSTS = {'huggingface.co', 'cdn-uploads.huggingface.co', 'cdn-lfs.huggingface.co', 'cdn-lfs.hf.co'}
# Anonymous Hub /resolve image requests now redirect to this exact Xet CDN.
# Signed links are accepted only from a trusted HTTP redirect, never card data.
SIGNED_REDIRECT_HOSTS = {'us.aws.cdn.hf.co'}
MAX_BYTES = 8 * 1024**2
_sources = OrderedDict()
_images = OrderedDict()
_ratings = {}
_owners = {}
_network = asyncio.Semaphore(3)


def identity(item):
    values = [item.get(key, '') for key in ('model', 'repo_id', 'filename', 'revision')]
    return hashlib.sha256('|'.join(values).encode()).hexdigest()[:24]


def _trusted_image_url(value, hosts, signed_hosts=()):
    if not isinstance(value, str) or not value or len(value) > 3000 or any(ord(c) < 32 for c in value):
        return None
    try:
        parsed = urlsplit(value)
        port = parsed.port
    except (ValueError, TypeError):
        return None
    if (parsed.scheme != 'https' or parsed.hostname not in hosts or parsed.username or parsed.password
            or port not in (None, 443) or parsed.fragment or '..' in unquote(parsed.path).split('/')):
        return None
    query_keys = [key.lower() for key, _ in parse_qsl(parsed.query)]
    if any('token' in key for key in query_keys):
        return None
    if parsed.hostname not in signed_hosts and any(
            'signature' in key or 'credential' in key for key in query_keys):
        return None
    return value


def image_url(value, repo, revision='main'):
    if not isinstance(value, str) or not value or len(value) > 3000 or any(ord(c) < 32 for c in value):
        return None
    try:
        relative = not urlsplit(value).scheme and not value.startswith('//')
    except (ValueError, TypeError):
        return None
    if value.startswith(('./', 'images/', 'examples/', 'assets/')) or relative:
        value = 'https://huggingface.co/' + quote(repo, safe='/') + '/resolve/' + quote(revision, safe='') + '/' + quote(value.removeprefix('./'), safe='/')
    return _trusted_image_url(value, HOSTS)


def redirect_image_url(value):
    """Validate a Location received from an already trusted anonymous request."""
    return _trusted_image_url(value, HOSTS | SIGNED_REDIRECT_HOSTS, SIGNED_REDIRECT_HOSTS)


def publisher_example(metadata, repo):
    card = metadata.get('cardData') if isinstance(metadata.get('cardData'), dict) else {}
    revision = metadata.get('sha', 'main')
    if not isinstance(revision, str) or not re.fullmatch('[a-f0-9]{40}', revision):
        revision = 'main'
    widgets = card.get('widget', metadata.get('widget', []))
    if isinstance(widgets, dict):
        widgets = [widgets]
    for widget in widgets if isinstance(widgets, list) else []:
        output = widget.get('output', {}) if isinstance(widget, dict) else {}
        for candidate in [output.get('url') if isinstance(output, dict) else None, widget.get('image') if isinstance(widget, dict) else None]:
            checked = image_url(candidate, repo, revision)
            if checked:
                return checked
    siblings = metadata.get('siblings', [])
    for entry in siblings if isinstance(siblings, list) else []:
        name = entry.get('rfilename', '') if isinstance(entry, dict) else ''
        if (isinstance(name, str) and name.lower().endswith(('.png', '.jpg', '.jpeg', '.webp'))
                and not any(part in name.lower() for part in ('logo', 'badge', 'workflow', 'avatar'))):
            checked = image_url(name, repo, revision)
            if checked:
                return checked
    return None


def mark_repository_mature(repo):
    for key, owners in _owners.items():
        if repo.casefold() in owners:
            _ratings[key] = 'adult'


def with_example(item, metadata=None, *, show_adult=False):
    result = merge_presentation(item, content_rating(metadata or {}))
    rating = result.get('content_rating', 'unknown')
    key = identity(item)
    local = RESOURCE / (key + '.png')
    url = (publisher_example(metadata or {}, item.get('repo_id', ''))
           or image_url(item.get('publisher_example_url'), item.get('repo_id', ''), item.get('revision', 'main')))
    publisher_key = None
    repo = item.get('repo_id', '').casefold()
    if rating == 'adult':
        mark_repository_mature(repo)
    if local.is_file():
        _ratings[key] = 'adult' if 'adult' in (rating, _ratings.get(key)) else rating
        _owners.setdefault(key, set()).add(repo)
    if url:
        publisher_key = hashlib.sha256(url.encode()).hexdigest()[:32]
        _sources[publisher_key] = url
        _ratings[publisher_key] = 'adult' if 'adult' in (rating, _ratings.get(publisher_key)) else rating
        _owners.setdefault(publisher_key, set()).add(repo)
        _sources.move_to_end(publisher_key)
        while len(_sources) > 240:
            removed, _ = _sources.popitem(last=False)
            _images.pop(removed, None); _ratings.pop(removed, None); _owners.pop(removed, None)
    if _ratings.get(key) == 'adult' or publisher_key and _ratings.get(publisher_key) == 'adult':
        rating = 'adult'
        result.update(content_rating='adult', content_rating_source=result.get('content_rating_source') or 'Previously declared mature by the publisher.')
    if rating == 'adult' and not show_adult:
        result.update(preview_url=None, preview_available=False, preview_requires_consent=False,
                      example_source=None, example_caption='Enable mature content to view this publisher example.')
        return result
    if local.is_file():
        result.update(preview_url='/api/local-remove/loras/preview/' + key + ('?show_adult=true' if show_adult else ''),
                      preview_available=True, preview_requires_consent=False, example_source='local-test',
                      example_caption='Local Image test with this exact adapter. Results depend on the prompt and inputs.')
        return result
    if url:
        key = publisher_key
        result.update(preview_url='/api/local-remove/loras/preview/' + key + ('?show_adult=true' if show_adult else ''),
                      preview_available=True, preview_requires_consent=rating == 'unknown' and not show_adult, example_source='publisher',
                      example_caption='Publisher example; appearance does not verify base-model compatibility.')
    else:
        result.update(preview_url=None, preview_available=False, preview_requires_consent=False, example_source=None,
                      example_caption='No example image published or verified yet.')
    return result


def check_consent(key, show_adult, show_unrated):
    rating = _ratings.get(key, 'unknown')
    if rating == 'adult' and not show_adult:
        raise ValueError('Enable mature content to view this publisher example.')
    if len(key) == 32 and rating == 'unknown' and not (show_unrated or show_adult):
        raise ValueError('This publisher preview is not rated. Choose Show unrated preview to view it.')


async def preview(key, *, show_adult=False, show_unrated=False):
    if not re.fullmatch(r'[a-f0-9]{24}|[a-f0-9]{32}', key):
        raise ValueError('Choose a preview from the LoRA gallery.')
    check_consent(key, show_adult, show_unrated)
    if key in _images:
        _images.move_to_end(key)
        return _images[key]
    path = RESOURCE / (key + '.png')
    if len(key) == 24 and path.is_file():
        if path.stat().st_size > MAX_BYTES:
            raise ValueError('This example is too large.')
        content = await asyncio.to_thread(path.read_bytes)
    else:
        target = _sources.get(key)
        if not target:
            raise ValueError('Refresh the gallery to load this example.')
        async with _network, aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=18, connect=6)) as client:
            for attempt in range(4):
                async with client.get(target, allow_redirects=False) as response:
                    if response.status in (301, 302, 303, 307, 308):
                        candidate = urljoin(target, response.headers.get('Location', ''))
                        target = redirect_image_url(candidate)
                        if not target:
                            raise ValueError('The publisher example redirected outside trusted image hosting.')
                        continue
                    if response.status != 200:
                        raise ValueError('The publisher example is unavailable; the adapter remains browsable.')
                    chunks, received = [], 0
                    async for block in response.content.iter_chunked(64 * 1024):
                        received += len(block)
                        if received > MAX_BYTES:
                            raise ValueError('The publisher example is too large.')
                        chunks.append(block)
                    content = b''.join(chunks)
                    break
            else:
                raise ValueError('The publisher example redirected too many times.')
    from stock_library import normalize_image
    value = await asyncio.to_thread(normalize_image, content, True)
    check_consent(key, show_adult, show_unrated)
    _images[key] = value
    while len(_images) > 80:
        _images.popitem(last=False)
    return value
