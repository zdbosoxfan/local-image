"""Live Hugging Face discovery and native-authorized, model-bound LoRA installs.

Only Hub metadata and safetensors are read. No repository code is installed or
executed. Installed adapters remain available when the Hub is offline.
"""
import asyncio
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path, PurePosixPath
import re
import struct
from typing import Literal
from urllib.parse import quote
import uuid

import aiohttp
from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel, ConfigDict, Field

from app_paths import model_directory, state_dir
from local_remove import guard, launcher_guard
from managed_ai import SetupError, download_verified, writable_directory

router = APIRouter(prefix='/api/local-remove/loras')
ModelId = Literal['qwen', 'z-image-turbo', 'flux2-dev', 'flux2-klein-4b', 'flux2-klein-9b', 'hidream-o1']
BASES = {
    'qwen': ('Qwen/Qwen-Image-2.1', 'Comfy-Org/Qwen-Image-2.1'),
    'z-image-turbo': ('Tongyi-MAI/Z-Image-Turbo',),
    'flux2-dev': ('black-forest-labs/FLUX.2-dev',),
    # BFL explicitly recommends applying base-trained Klein LoRAs to distilled 4B.
    'flux2-klein-4b': ('black-forest-labs/FLUX.2-klein-4B', 'black-forest-labs/FLUX.2-klein-base-4B'),
    'flux2-klein-9b': ('black-forest-labs/FLUX.2-klein-9B', 'black-forest-labs/FLUX.2-klein-base-9B'),
    'hidream-o1': ('HiDream-ai/HiDream-O1-Image', 'Comfy-Org/HiDream-O1-Image'),
}
SEARCH_TERMS = {'qwen': 'Qwen-Image-2.1', 'z-image-turbo': 'z_image_turbo',
                'flux2-dev': 'Flux.2', 'flux2-klein-4b': 'klein', 'flux2-klein-9b': 'klein', 'hidream-o1': 'HiDream-O1'}
from lora_catalog import CURATED, PRESENTATION_FIELDS

SPECIAL_WORKFLOWS = {
    'limbicnation/pixel-art-lora': 'This build cannot load six global modulation tensors from either published pixel-art file. Choose Watercolor wash or Claymation miniature; partial adapter loading is not supported.',
    'viggle/qwen-image-2.1-viggle-turbo': 'This accelerator requires the publisher\'s custom sigma workflow; the standard ImageGen sampler does not implement it.',
    'prunaai/pruna-qwen-image-2.1': 'This accelerator requires a fixed custom sigma schedule that the standard ImageGen sampler does not implement.',
}


def now():
    return datetime.now(timezone.utc).isoformat()


def check_model(model):
    if not isinstance(model, str) or model not in BASES:
        raise ValueError('Choose a supported image model.')
    return model


def check_repo(repo):
    if not isinstance(repo, str) or len(repo) > 200 or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]*/[A-Za-z0-9][A-Za-z0-9_.-]*', repo) or '..' in repo:
        raise ValueError('Enter a Hugging Face repository as owner/name.')
    return repo


def check_filename(filename):
    if (not isinstance(filename, str) or len(filename) > 500 or '\\' in filename or ':' in filename
            or any(ord(char) < 32 for char in filename) or not filename.endswith('.safetensors')
            or any(part in ('', '.', '..') for part in filename.split('/')) or PurePosixPath(filename).is_absolute()):
        raise ValueError('Choose a safetensors file from the repository.')
    return filename


def check_revision(revision):
    if not isinstance(revision, str) or not re.fullmatch('[a-f0-9]{40}', revision):
        raise ValueError('Refresh the file list to obtain an immutable repository revision.')
    return revision


def curated_entry(model, repo, filename=None, revision=None):
    return next((item for item in CURATED if item['model'] == model and item['repo_id'].casefold() == repo.casefold()
                 and (filename is None or item['filename'] == filename)
                 and (revision is None or item['revision'] == revision)), None)


def compatibility(model, metadata):
    card = metadata.get('cardData')
    card = card if isinstance(card, dict) else {}
    tags = metadata.get('tags')
    tags = tags if isinstance(tags, list) else []
    bases = card.get('base_model', [])
    bases = [bases] if isinstance(bases, str) else list(bases) if isinstance(bases, list) else []
    bases += [tag.removeprefix('base_model:adapter:') for tag in tags
              if isinstance(tag, str) and tag.startswith('base_model:adapter:')]
    bases += [tag.removeprefix('base_model:') for tag in tags
              if isinstance(tag, str) and tag.startswith('base_model:') and tag.count(':') == 1]
    if any(str(base).casefold() in {name.casefold() for name in BASES[model]} for base in bases):
        return 'declared'
    return 'incompatible' if bases else 'unverified'


async def hub_json(path, params=None):
    # Callers construct only /api/models paths; no caller supplies an origin.
    if not path.startswith('/api/models') or '?' in path or '#' in path:
        raise ValueError('Invalid Hub metadata path.')
    timeout = aiohttp.ClientTimeout(total=25, connect=8)
    try:
        async with aiohttp.ClientSession(timeout=timeout) as session:
            async with session.get('https://huggingface.co' + path, params=params, allow_redirects=False) as response:
                if response.status in (401, 403):
                    raise ValueError('This repository requires publisher access approval. Open its Hugging Face page; this app does not bypass access gates.')
                if response.status != 200:
                    raise ValueError(f'Hugging Face returned HTTP {response.status}. Try again later.')
                # StreamReader.read(n) may return only the first received packet.
                # Accumulate the bounded body before parsing chunked Hub replies.
                chunks, received = [], 0
                async for chunk in response.content.iter_chunked(64 * 1024):
                    received += len(chunk)
                    if received > 4 * 1024 * 1024:
                        raise ValueError('The repository metadata is too large to browse here.')
                    chunks.append(chunk)
                return json.loads(b''.join(chunks))
    except (aiohttp.ClientError, asyncio.TimeoutError, json.JSONDecodeError) as error:
        raise ValueError('Hugging Face is unavailable. Installed LoRAs remain usable; refresh when connected.') from error


async def search_hub(model, query='', page=0):
    check_model(model)
    if len(query) > 120 or any(ord(char) < 32 for char in query) or not 0 <= page <= 4:
        raise ValueError('Use a search of up to 120 characters and a valid results page.')
    common = {'sort': 'lastModified', 'direction': '-1', 'limit': '100', 'full': 'true', 'config': 'true'}
    searches = []
    for base in BASES[model]:
        searches.append(hub_json('/api/models', {**common, 'filter': 'base_model:adapter:' + base, 'search': query}))
        searches.append(hub_json('/api/models', {**common, 'filter': 'base_model:' + base, 'search': query}))
    searches.append(hub_json('/api/models', {**common, 'filter': 'lora', 'search': query or SEARCH_TERMS[model]}))
    replies = await asyncio.gather(*searches, return_exceptions=True)
    found, errors = {}, []
    for reply in replies:
        if isinstance(reply, Exception):
            errors.append(str(reply)); continue
        if not isinstance(reply, list):
            continue
        for metadata in reply:
            if not isinstance(metadata, dict):
                continue
            repo = metadata.get('id', metadata.get('modelId', ''))
            try:
                check_repo(repo)
            except ValueError:
                continue
            tags = metadata.get('tags')
            tags = tags if isinstance(tags, list) else []
            is_adapter = any('lora' in str(tag).lower() or 'base_model:adapter:' in str(tag) for tag in tags)
            if not is_adapter:
                continue
            match = compatibility(model, metadata)
            if match == 'incompatible':
                continue
            warning = SPECIAL_WORKFLOWS.get(repo.casefold(), '')
            card = metadata.get('cardData')
            card = card if isinstance(card, dict) else {}
            found[repo] = {'repo_id': repo, 'title': repo.split('/')[-1], 'downloads': metadata.get('downloads', 0),
                'license': card.get('license', next((tag[8:] for tag in tags if str(tag).startswith('license:')), 'Not specified')),
                'compatibility': match, 'updated_at': metadata.get('lastModified') if isinstance(metadata.get('lastModified'), str) else '', 'warning': warning,
                'supported': not bool(warning), 'url': 'https://huggingface.co/' + repo}
    if errors and len(errors) == len(replies):
        raise ValueError(errors[0])
    results = sorted(found.values(), key=lambda item: item['updated_at'] or '', reverse=True)
    start = page * 20
    return {'results': results[start:start + 20], 'page': page,
            'next_page': page + 1 if page < 4 and len(results) > start + 20 else None,
            'checked_at': now(), 'warning': errors[0] if errors else ''}


async def repository_files(model, repo, revision=None):
    check_model(model); check_repo(repo)
    if revision is not None:
        check_revision(revision)
    path = '/api/models/' + quote(repo, safe='/')
    metadata = await hub_json(path + ('/revision/' + revision if revision else ''))
    if not isinstance(metadata, dict):
        raise ValueError('Hugging Face returned invalid repository metadata. Refresh the file list.')
    resolved = check_revision(metadata.get('sha', ''))
    if revision is not None and resolved != revision:
        raise ValueError('The repository response does not match the selected revision. Refresh the file list.')
    match = compatibility(model, {**metadata, 'id': repo})
    if match == 'incompatible':
        raise ValueError('The publisher declares a different base model. Choose an adapter for the selected model.')
    # blobs=true includes LFS integrity metadata, recursively, without accepting
    # a user-controlled paging URL or loading repository code.
    files_meta = await hub_json(path + '/revision/' + resolved, {'blobs': 'true'})
    if not isinstance(files_meta, dict) or files_meta.get('sha') != resolved or not isinstance(files_meta.get('siblings'), list):
        raise ValueError('Hugging Face returned an invalid file list for the selected revision.')
    files = []
    for item in files_meta.get('siblings', []):
        if not isinstance(item, dict):
            continue
        filename = item.get('rfilename', '')
        try:
            check_filename(filename)
        except ValueError:
            continue
        lfs = item.get('lfs')
        lfs = lfs if isinstance(lfs, dict) else {}
        digest = lfs.get('sha256', lfs.get('oid', ''))
        size = lfs.get('size', item.get('size'))
        if type(size) is int and 0 < size <= 8 * 1024 ** 3 and isinstance(digest, str) and re.fullmatch('[a-f0-9]{64}', digest):
            entry = {'filename': filename, 'bytes': size, 'sha256': digest, 'compatibility': match}
            reviewed = curated_entry(model, repo, filename, resolved)
            if reviewed and reviewed['bytes'] == size and reviewed['sha256'] == digest:
                entry.update(compatibility='curated', **{key: reviewed[key] for key in PRESENTATION_FIELDS if key in reviewed})
            files.append(entry)
    curated = curated_entry(model, repo)
    files.sort(key=lambda item: (item['filename'] != (curated or {}).get('filename'), item['filename']))
    return {'repo_id': repo, 'revision': resolved, 'compatibility': match,
            'files': files, 'checked_at': now(), 'warning': SPECIAL_WORKFLOWS.get(repo.casefold(), ''),
            'supported': repo.casefold() not in SPECIAL_WORKFLOWS,
            'license': metadata['cardData'].get('license', 'Not specified') if isinstance(metadata.get('cardData'), dict) else 'Not specified'}


def registry_path():
    return state_dir() / 'lora-library.json'


def read_registry():
    try:
        content = json.loads(registry_path().read_text(encoding='utf-8'))
        return content if isinstance(content, dict) else {}
    except (OSError, ValueError):
        return {}


def write_registry(entries):
    path = registry_path(); path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name('lora-library-' + uuid.uuid4().hex + '.tmp')
    try:
        temporary.write_text(json.dumps(entries, indent=2), encoding='utf-8')
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


def installed_path(entry):
    name = entry.get('comfy_filename', '')
    if not isinstance(name, str) or not re.fullmatch(r'local-image/[a-z0-9-]+-[a-f0-9]{24}\.safetensors', name):
        raise ValueError('The saved LoRA filename is invalid.')
    models = model_directory().resolve()
    root = (models / 'loras').resolve()
    if not root.is_relative_to(models):
        raise ValueError('The LoRA subfolder points outside the configured model directory. Choose a regular model folder in Settings.')
    path = root / name
    if path.is_symlink() or not path.resolve().is_relative_to(root):
        raise ValueError('The saved LoRA is outside the configured model directory.')
    return path


def installed(model=None):
    entries = []
    for entry in read_registry().values():
        if not isinstance(entry, dict) or model and entry.get('model') != model:
            continue
        try:
            check_model(entry.get('model')); check_repo(entry.get('repo_id'))
            check_filename(entry.get('filename')); check_revision(entry.get('revision'))
            identity = hashlib.sha256((entry['model'] + '|' + entry['repo_id'] + '|' + entry['filename'] + '|' + entry['revision']).encode()).hexdigest()[:24]
            if (entry.get('id') != identity or entry.get('comfy_filename') != 'local-image/' + entry['model'] + '-' + identity + '.safetensors'
                    or type(entry.get('bytes')) is not int or not 0 < entry['bytes'] <= 8 * 1024 ** 3
                    or not isinstance(entry.get('sha256'), str) or not re.fullmatch('[a-f0-9]{64}', entry['sha256'])):
                continue
            path = installed_path(entry)
            if path.is_file() and path.stat().st_size == entry.get('bytes'):
                public = dict(entry)
                reviewed = curated_entry(entry['model'], entry['repo_id'], entry['filename'], entry['revision'])
                # Refresh descriptions for earlier installs without changing their
                # saved identity or trusting a different artifact in the same repo.
                if reviewed and reviewed['bytes'] == entry['bytes'] and reviewed['sha256'] == entry['sha256']:
                    public.update({key: reviewed[key] for key in PRESENTATION_FIELDS if key in reviewed})
                    public['compatibility'] = 'curated'
                if entry['repo_id'].casefold() in SPECIAL_WORKFLOWS:
                    public.update(supported=False, warning=SPECIAL_WORKFLOWS[entry['repo_id'].casefold()])
                entries.append(public)
        except (OSError, ValueError):
            continue
    return entries


def resolve_loras(model, selections, *, reference_count=0):
    check_model(model)
    if not isinstance(selections, (list, tuple)) or len(selections) > 3:
        raise ValueError('Choose up to three installed LoRAs.')
    entries = {entry['id']: entry for entry in installed()}
    result, seen = [], set()
    for selection in selections:
        if not isinstance(selection, dict) or set(selection) != {'id', 'strength'}:
            raise ValueError('Choose a LoRA from the installed library.')
        identity, strength = selection['id'], selection['strength']
        if not isinstance(identity, str) or identity in seen or identity not in entries:
            raise ValueError('This LoRA is missing or selected more than once. Open the library and select it again.')
        entry = entries[identity]
        if entry['model'] != model:
            raise ValueError('This LoRA belongs to a different model. Select an adapter for the active model.')
        if entry['repo_id'].casefold() in SPECIAL_WORKFLOWS:
            raise ValueError(SPECIAL_WORKFLOWS[entry['repo_id'].casefold()])
        if entry.get('usage') == 'reference-edit' and reference_count < 1:
            raise ValueError(f'{entry["title"]} is an image-editing adapter. Add a reference image before generating.')
        if type(strength) not in (int, float) or not math.isfinite(strength) or not -2 <= strength <= 2:
            raise ValueError('LoRA strength must be between -2 and 2.')
        seen.add(identity); result.append((entry['comfy_filename'], float(strength)))
    return result


def validate_lora_header(path):
    with path.open('rb') as stream:
        length_data = stream.read(8)
        if len(length_data) != 8:
            raise ValueError('This is not a valid safetensors adapter.')
        length = struct.unpack('<Q', length_data)[0]
        if not 2 <= length <= 8 * 1024 * 1024:
            raise ValueError('This safetensors header is invalid or too large.')
        header = json.loads(stream.read(length))
    if not isinstance(header, dict) or not any(
            re.search(r'(?:lora_[ABab]|lora_(?:up|down)|lokr_|hada_)', key) for key in header if key != '__metadata__'):
        raise ValueError('The file does not contain recognizable LoRA adapter tensors. It was not enabled.')


class LoraDownloadRequest(BaseModel):
    model_config = ConfigDict(extra='forbid', strict=True)
    model: ModelId
    repo_id: str = Field(min_length=3, max_length=200)
    filename: str = Field(min_length=1, max_length=500)
    revision: str = Field(min_length=40, max_length=40)
    allow_unverified: bool = False


class LoraDownloadManager:
    def __init__(self):
        self.task = None
        self.job = {'phase': 'idle', 'model': None, 'message': 'Choose an optional LoRA.', 'error': None,
                    'downloaded_bytes': 0, 'total_bytes': 0}

    @property
    def active(self):
        return self.task is not None and not self.task.done()

    def status(self):
        total = self.job['total_bytes']
        return {**self.job, 'running': self.active,
                'progress': min(1.0, self.job['downloaded_bytes'] / total) if total else 0}

    def begin(self, payload, release):
        if self.active:
            raise ValueError('A LoRA download is already running.')
        check_repo(payload.repo_id); check_filename(payload.filename); check_revision(payload.revision)
        self.job = {'phase': 'checking', 'model': payload.model, 'message': 'Checking publisher metadata…', 'error': None,
                    'downloaded_bytes': 0, 'total_bytes': 0}
        self.task = asyncio.create_task(self._run(payload, release))

    async def _run(self, payload, release):
        try:
            writable_directory(str(model_directory()), 'model folder')
            files = await repository_files(payload.model, payload.repo_id, payload.revision)
            if not files['supported']:
                raise ValueError(files['warning'])
            match = files['compatibility']
            item = next((item for item in files['files'] if item['filename'] == payload.filename), None)
            if item is None:
                raise ValueError('Choose a safetensors adapter smaller than 8 GiB with publisher SHA-256 metadata.')
            curated = curated_entry(payload.model, payload.repo_id, payload.filename, payload.revision)
            if curated and (curated['bytes'] != item['bytes'] or curated['sha256'] != item['sha256']):
                raise ValueError('The publisher metadata does not match the reviewed adapter.')
            if curated:
                match = 'curated'
            if match == 'unverified' and not payload.allow_unverified:
                raise ValueError('The base model is unverified. Explicitly assign this adapter to the selected model before downloading.')
            identity = hashlib.sha256((payload.model + '|' + payload.repo_id + '|' + payload.filename + '|' + payload.revision).encode()).hexdigest()[:24]
            entry = {'id': identity, 'model': payload.model, 'repo_id': payload.repo_id, 'filename': payload.filename,
                'revision': payload.revision, 'bytes': item['bytes'], 'sha256': item['sha256'],
                'title': curated['title'] if curated else PurePosixPath(payload.filename).stem,
                'compatibility': match, 'license': files['license'], 'installed_at': now(),
                'comfy_filename': 'local-image/' + payload.model + '-' + identity + '.safetensors'}
            if curated:
                entry.update({key: curated[key] for key in PRESENTATION_FIELDS if key in curated})
            target = installed_path(entry)
            artifact = {**item, 'name': target.name,
                'url': 'https://huggingface.co/' + quote(payload.repo_id, safe='/') + '/resolve/' + payload.revision + '/' + quote(payload.filename, safe='/')}
            self.job.update(phase='downloading', total_bytes=item['bytes'], message='Downloading ' + payload.filename)
            def progress(done, total):
                self.job['downloaded_bytes'] = done
            await download_verified(artifact, target, progress)
            await asyncio.to_thread(validate_lora_header, target)
            registry = read_registry(); registry[identity] = entry; write_registry(registry)
            self.job.update(phase='complete', message='LoRA verified and installed. Select it to apply it.', id=identity)
        except asyncio.CancelledError:
            self.job.update(phase='error', error='Download interrupted.', message='Completed model files were preserved.')
            raise
        except Exception as error:
            self.job.update(phase='error', error=str(error), message=str(error))
        finally:
            release()


manager = LoraDownloadManager()


@router.get('')
async def library(request: Request, model: ModelId | None = None):
    guard(request)
    return {'installed': installed(model), 'curated': [dict(item, compatibility='curated', supported=True)
            for item in CURATED if model is None or item['model'] == model], 'job': manager.status()}


@router.get('/search')
async def search(request: Request, model: ModelId, query: str = '', page: int = 0, refresh: bool = False):
    guard(request)
    try:
        # Always live: refresh intentionally does not rely on an application cache.
        return await search_hub(model, query, page)
    except ValueError as error:
        raise HTTPException(400, str(error)) from error


@router.get('/files')
async def files(request: Request, model: ModelId, repo_id: str, revision: str | None = None):
    guard(request)
    try:
        return await repository_files(model, repo_id, revision)
    except ValueError as error:
        raise HTTPException(400, str(error)) from error


@router.get('/download')
async def download_status(request: Request):
    guard(request)
    return manager.status()


@router.post('/download')
async def download_begin(request: Request, payload: LoraDownloadRequest):
    launcher_guard(request)
    from main import generation_lock
    from managed_ai import manager as setup_manager
    from qwen_setup import manager as model_manager
    if manager.active or setup_manager.active or model_manager.active or generation_lock.locked():
        raise HTTPException(409, 'Wait for the current image operation or model setup to finish.')
    await generation_lock.acquire()
    try:
        manager.begin(payload, generation_lock.release)
    except Exception as error:
        generation_lock.release()
        raise HTTPException(400, str(error)) from error
    return manager.status()
