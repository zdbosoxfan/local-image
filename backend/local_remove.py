"""Persistent non-destructive local removal sessions and external-editor round trips."""
import asyncio
import base64
from contextlib import AsyncExitStack, nullcontext
import hashlib
import io
import json
import math
import os
from pathlib import Path
import re
import secrets
import shutil
import stat
import tempfile
import threading
import time
from typing import Literal
import uuid
import copy

import aiohttp
import numpy as np
import tifffile
from fastapi import APIRouter, File, Form, HTTPException, Request, UploadFile
from fastapi.responses import FileResponse, HTMLResponse, JSONResponse, Response
from PIL import Image, ImageCms, ImageOps
from pydantic import BaseModel, Field, model_validator

from quality_settings import _local_request
from engine import config
from local_removal_models import model_options as ai_model_options, run_local_removal
from fast_inpaint import heal_image, heal_option
from local_remove_project import write_project, extract_project, MAX_TOTAL
from local_remove_frontend import render_editor, frontend_asset
from frontend_tokens import issue_browser_token, valid_browser_token
from app_paths import APP_VERSION, cache_dir, data_root, read_config, state_dir
from cutout_composite import (initial_cutout, validate_cutout, refine_alpha,
                              compose_image, compose_native, DEFAULT_TRANSFORM,
                              DEFAULT_SHADOW, inverse_selection, underlay)
from stock_attribution import validate_attribution, validate_attributions, collect_attributions
import layer_stack as stack_model

router = APIRouter()
_session_clock_lock = threading.Lock()
_last_session_modified = 0.0
ROOT = state_dir()
SESSIONS = ROOT / 'sessions'
SESSIONS.mkdir(parents=True, exist_ok=True)
COLLECTIONS = ROOT / 'collections'
COLLECTIONS.mkdir(parents=True, exist_ok=True)
BACKGROUNDS = ROOT / 'backgrounds'
BACKGROUNDS.mkdir(parents=True, exist_ok=True)
SECRET_FILE = ROOT / 'launcher.key'
if not SECRET_FILE.exists():
    SECRET_FILE.write_text(secrets.token_urlsafe(48), encoding='ascii')
LAUNCHER_KEY = SECRET_FILE.read_text(encoding='ascii').strip()
CSRF = secrets.token_urlsafe(32)
locks = {}
source_locks = {}
collection_locks = {}
registration_lock = asyncio.Lock()
thumbnail_gate = asyncio.Semaphore(2)
settings_lock = asyncio.Lock()
geometry_cache = {}
SRGB = ImageCms.ImageCmsProfile(ImageCms.createProfile('sRGB'))
HEADERS = {'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff'}
ASSET_HEADERS = {'Cache-Control': 'private, max-age=31536000, immutable', 'X-Content-Type-Options': 'nosniff'}
SUPPORTED = {'.jpg', '.jpeg', '.png', '.tif', '.tiff', '.webp'}
RemovalModel = Literal['klein']


def guard(request, write=False):
    _local_request(request)
    if write and not valid_browser_token(request.headers.get('x-local-remove-token', ''), CSRF):
        raise HTTPException(403, 'Reload Local Image and try again.')


def launcher_guard(request):
    guard(request)
    if not secrets.compare_digest(request.headers.get('x-local-launcher', ''), LAUNCHER_KEY):
        raise HTTPException(403, 'Choose files or folders using the Local Image application.')


def path_key(path):
    return os.path.normcase(str(Path(path).resolve()))


def source_gate(path):
    return source_locks.setdefault(path_key(path), asyncio.Lock())


def natural_name(path):
    name = Path(path).name
    return ([(1, int(part)) if part.isdigit() else (0, part.casefold())
             for part in re.split(r'(\d+)', name)], name.casefold(), name)


def validate_id(value, label):
    try:
        if str(uuid.UUID(value)) != value:
            raise ValueError()
    except (ValueError, TypeError, AttributeError):
        raise HTTPException(404, label + ' not found')


def read_collection(cid):
    validate_id(cid, 'Collection')
    try:
        data=json.loads((COLLECTIONS / (cid + '.json')).read_text(encoding='utf-8'))
        if (not isinstance(data,dict) or data.get('id')!=cid or not isinstance(data.get('name'),str)
                or not isinstance(data.get('entries'),list)
                or any(not isinstance(item,dict) or not all(isinstance(item.get(key),str) for key in ('id','name','path'))
                       for item in data['entries'])):
            raise ValueError('Malformed collection')
        return data
    except (OSError, ValueError):
        raise HTTPException(404, 'Collection not found')


def write_collection(data):
    data['modified'] = time.time()
    temporary = COLLECTIONS / ('collection-' + uuid.uuid4().hex + '.tmp')
    try:
        temporary.write_text(json.dumps(data, indent=2), encoding='utf-8')
        os.replace(temporary, COLLECTIONS / (data['id'] + '.json'))
    finally:
        temporary.unlink(missing_ok=True)


def public_collection(data):
    entries = []
    for entry in data['entries']:
        item = {'id': entry['id'], 'name': entry['name'], 'session_id': entry.get('session_id'),
                'edited': False, 'saved': False, 'dirty': False, 'saved_name': None}
        if entry.get('error'):
            item['error'] = entry['error']
        if entry.get('session_id'):
            try:
                saved = public(read_session(entry['session_id']))
                item.update({key: saved[key] for key in ('edited', 'saved', 'dirty', 'saved_name')})
            except (HTTPException, OSError, ValueError, KeyError, TypeError):
                item['session_id'] = None
        entries.append(item)
    return {'id': data['id'], 'name': data['name'], 'entries': entries, 'index': data.get('index', 0)}


def register_collection(paths, name, source_folder=None):
    """Persist only the explicitly chosen files; images are decoded lazily."""
    keys = [path_key(path) for path in paths]
    identity = 'folder:' + path_key(source_folder) if source_folder else 'files:' + hashlib.sha256('\n'.join(keys).encode('utf-8')).hexdigest()
    existing = None
    for saved in COLLECTIONS.glob('*.json'):
        try:
            candidate = read_collection(saved.stem)
            if candidate.get('identity') == identity and (existing is None or candidate.get('modified', 0) > existing.get('modified', 0)):
                existing = candidate
        except (HTTPException, OSError, ValueError, AttributeError):
            continue
    data = existing or {'id': str(uuid.uuid4()), 'name': name, 'identity': identity, 'entries': [], 'index': 0, 'created': time.time()}
    previous = {path_key(item['path']): item for item in data['entries']}
    entries = []
    for path, key in zip(paths, keys):
        item = dict(previous.get(key, {'id': str(uuid.uuid4()), 'session_id': None}))
        item.update(path=str(path), name=path.name)
        entries.append(item)
    data.update(name=name, entries=entries)
    data['index'] = min(data.get('index', 0), max(0, len(entries) - 1))
    write_collection(data)
    return data


def read_settings():
    """Keep the saved choice even while its model or GPU service is unavailable."""
    try:
        saved = json.loads((ROOT / 'settings.json').read_text(encoding='utf-8'))
        model = saved.get('model')
        if model == 'klein':
            return {'model': model}
    except (OSError, ValueError, AttributeError):
        pass
    return {'model': 'klein'}


def model_options():
    return [*ai_model_options(), heal_option()]


def available_model(model, options=None):
    option = next((item for item in (model_options() if options is None else options)
                   if item['id'] == model), None)
    if option is None or not option.get('available'):
        reason = option.get('reason') if option else None
        label = option['label'] if option else model
        raise HTTPException(409, reason or f'{label} is not installed or ready. Choose another removal model.')
    return option


def settings_response():
    return {**read_settings(), 'models': model_options()}


class RemovalSettings(BaseModel):
    model: RemovalModel


@router.get('/api/local-remove/settings')
async def get_settings(request: Request):
    guard(request)
    return settings_response()


@router.patch('/api/local-remove/settings')
async def update_settings(request: Request, payload: RemovalSettings):
    guard(request, True)
    async with settings_lock:
        options = model_options()
        available_model(payload.model, options)
        temporary = ROOT / ('settings-' + uuid.uuid4().hex + '.tmp')
        try:
            temporary.write_text(json.dumps({'model': payload.model}, indent=2), encoding='utf-8')
            os.replace(temporary, ROOT / 'settings.json')
        finally:
            temporary.unlink(missing_ok=True)
        return {'model': payload.model, 'models': options}


def folder(sid):
    validate_id(sid, 'Session')
    path = SESSIONS / sid
    if not (path / 'session.json').is_file():
        raise HTTPException(404, 'Session not found')
    return path


def read_session(sid):
    try:
        data=json.loads((folder(sid) / 'session.json').read_text(encoding='utf-8'))
        if (not isinstance(data,dict) or data.get('id')!=sid or not isinstance(data.get('layers'),list)
                or not isinstance(data.get('revision'),int) or not isinstance(data.get('original'),str)):
            raise ValueError('Malformed session')
        return data
    except (OSError,ValueError):
        raise HTTPException(404,'Session data is unavailable.')


def write_session(path, data):
    # Windows clock ticks can cover several edits. Keep write order distinct so
    # reopening a source never chooses an older document by its random UUID.
    global _last_session_modified
    with _session_clock_lock:
        previous = max(_last_session_modified, data.get('modified', 0))
        data['modified'] = max(time.time(), math.nextafter(previous, math.inf))
        _last_session_modified = data['modified']
    tmp = path / ('session-' + uuid.uuid4().hex + '.tmp')
    tmp.write_text(json.dumps(data, indent=2), encoding='utf-8')
    tmp.replace(path / 'session.json')


def file_hash(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def decode_original(path):
    icc = None
    if path.suffix.lower() in {'.tif', '.tiff'}:
        with tifffile.TiffFile(path) as tif:
            page = tif.pages[0]
            if int(page.photometric) != 2:
                raise ValueError('Export an RGB TIFF from Capture One for Local Image.')
            data = page.asarray()
            if page.planarconfig == 2:
                data = np.moveaxis(data, 0, -1)
            if data.ndim != 3 or data.shape[-1] not in (3, 4) or data.dtype not in (np.uint8, np.uint16):
                raise ValueError('Use an 8-bit or 16-bit RGB TIFF for external editing.')
            orientation = page.tags.get('Orientation')
            if orientation and orientation.value != 1:
                raise ValueError('Export a TIFF with normal orientation from Capture One.')
            tag = page.tags.get('InterColorProfile')
            icc = tag.value if tag else None
        rgb = data[..., :3]
        preview = Image.fromarray((rgb / 257).round().astype(np.uint8) if data.dtype == np.uint16 else rgb)
    else:
        with Image.open(path) as img:
            icc = img.info.get('icc_profile')
            preview = ImageOps.exif_transpose(img).convert('RGB')
            data = np.asarray(ImageOps.exif_transpose(img).convert('RGBA' if 'A' in img.getbands() or 'transparency' in img.info else 'RGB')).copy()
    if icc:
        preview = ImageCms.profileToProfile(preview, ImageCms.ImageCmsProfile(io.BytesIO(icc)), SRGB, outputMode='RGB')
    return data, icc, preview


def public(data):
    if not isinstance(data,dict) or not all(key in data for key in ('id','name','revision','layers')):
        raise ValueError('Malformed session')
    if data.get('cutout'):
        validate_cutout(data['cutout'])
    result = {k: v for k, v in data.items() if k not in {'source_path', 'source_hash', 'last_saved_hash', 'last_saved_path', 'project_path', 'project_hash', 'cutout_undo', 'cutout_redo', 'stack_undo', 'stack_redo'}}
    if data.get('layer_stack'):
        stack_model.validate_stack(data['layer_stack'], data['layers'])
        result['layer_stack'] = []
        root = SESSIONS / data['id']
        for node in data['layer_stack']:
            item = copy.deepcopy(node)
            bounds = (0, 0, data['width'], data['height'])
            if node['kind'] == 'cutout':
                key = (data['id'], 'bounds:' + node['cutout']['alpha'])
                if key not in geometry_cache:
                    with Image.open(root / node['cutout']['alpha']) as alpha:
                        geometry_cache[key] = alpha.getbbox()
                bounds = geometry_cache[key]
            elif node['kind'] == 'retouch':
                patches = [p for p in data['layers'] if p['id'] in node['patch_ids'] and p['visible'] and not p['discarded']]
                boxes = []
                for patch in patches:
                    key = (data['id'], 'bounds:' + patch['mask'])
                    if key not in geometry_cache:
                        with Image.open(root / patch['mask']) as mask:
                            geometry_cache[key] = mask.getbbox()
                    box = geometry_cache[key]
                    if box: boxes.append((box[0]+patch['x'], box[1]+patch['y'], box[2]+patch['x'], box[3]+patch['y']))
                bounds = (min(b[0] for b in boxes), min(b[1] for b in boxes), max(b[2] for b in boxes), max(b[3] for b in boxes)) if boxes else None
            item.update(bounds=bounds, width=data['width'], height=data['height'],
                        display_key=stack_model.display_key(data, node))
            result['layer_stack'].append(item)
        result['stack_can_undo'] = bool(data.get('stack_undo'))
        result['stack_can_redo'] = bool(data.get('stack_redo'))
    result['cutout_can_undo'] = bool(data.get('cutout_undo'))
    result['cutout_can_redo'] = bool(data.get('cutout_redo'))
    if data.get('cutout'):
        key = (data['id'], 'cutout:' + data['cutout']['alpha'])
        if key not in geometry_cache:
            with Image.open(SESSIONS / data['id'] / data['cutout']['alpha']) as alpha:
                geometry_cache[key] = alpha.getbbox()
        result['cutout_bounds'] = geometry_cache[key]
    result['layers'] = []
    for layer in data['layers']:
        item = dict(layer)
        key = (data['id'], layer['id'])
        if 'width' in layer and 'height' in layer:
            size = (layer['width'], layer['height'])
        else:
            size = geometry_cache.get(key)
            if size is None:
                with Image.open(SESSIONS / data['id'] / layer['color']) as patch:
                    size = patch.size
                geometry_cache[key] = size
        item.update(width=size[0], height=size[1])
        result['layers'].append(item)
    saved_revision = data.get('saved_revision')
    result.update(source_name=Path(data['source_path']).name if data.get('source_path') else None,
                  saved_revision=saved_revision, saved_name=data.get('saved_name'),
                  edited=data.get('revision', 0) > 0,
                  saved=saved_revision is not None and saved_revision == data.get('revision', 0),
                  dirty=data.get('revision', 0) != (saved_revision if saved_revision is not None else 0),
                  project_name=data.get('project_name'), project_saved_revision=data.get('project_saved_revision'),
                  project_saved=data.get('project_saved_revision') == data['revision'],
                  project_dirty=data.get('project_saved_revision') != data['revision'],
                  has_project_path=bool(data.get('project_path')))
    return result


def create_session(source, name, source_path=None):
    sid = str(uuid.uuid4())
    target = SESSIONS / sid
    target.mkdir()
    suffix = Path(name).suffix.lower()
    if suffix not in SUPPORTED:
        raise ValueError('Open a JPEG, PNG, TIFF, or WebP image. Use Edit With for RAW photos.')
    original = target / ('original' + suffix)
    shutil.copy2(source, original)
    raw, icc, preview = decode_original(original)
    preview.save(target / 'base.png', compress_level=1, icc_profile=SRGB.tobytes())
    data = {'id':sid, 'name':Path(name).name, 'width':preview.width, 'height':preview.height,
            'bit_depth':16 if raw.dtype == np.uint16 else 8, 'original':original.name,
            'source_path':str(source_path) if source_path else None,
            'source_hash':file_hash(original) if source_path else None,
            'can_return':bool(source_path), 'layers':[], 'revision':0, 'saved_revision':None,
            'created':time.time()}
    write_session(target, data)
    return public(data)


def reuse_or_create_session(path):
    if not path.is_file() or path.suffix.lower() not in SUPPORTED:
        raise HTTPException(400, 'This image is no longer available. Choose another image or open the folder again.')
    current_hash = file_hash(path)
    matching = []
    for saved in SESSIONS.glob('*/session.json'):
        try:
            data = json.loads(saved.read_text(encoding='utf-8'))
            if (data.get('id') != saved.parent.name or not isinstance(data.get('layers'), list)
                    or not isinstance(data.get('revision'), int) or not isinstance(data.get('original'), str)
                    or not (saved.parent / data['original']).is_file()):
                continue
            if (data.get('source_path') and path_key(data['source_path']) == path_key(path)
                    and current_hash in {data.get('source_hash'), data.get('last_saved_hash')}):
                matching.append(data)
        except (OSError, ValueError, AttributeError, TypeError):
            continue
    if matching:
        return max(matching, key=lambda data: (data.get('modified', data.get('created', 0)), data['id']))
    created = create_session(path, path.name, path)
    data = read_session(created['id'])
    if data['source_hash'] != current_hash or file_hash(path) != current_hash:
        raise HTTPException(409, 'The image changed while it was opening. Open it again.')
    return data


def render(data, original=False, include_cutout=True, *, root_override=None):
    root = Path(root_override) if root_override is not None else folder(data['id'])
    if not original and data.get('layer_stack'):
        raw, icc, _ = decode_original(root / data['original'])
        return stack_model.display(stack_model.render_native(data, root, raw, icc, decode_original), icc)
    with Image.open(root / 'base.png') as img:
        image = img.convert('RGB')
    if original:
        image = attach_source_alpha(root, data, image)
    if not original:
        for layer in data['layers']:
            if layer['visible'] and not layer.get('discarded'):
                with Image.open(root / layer['color']) as color, Image.open(root / layer['mask']) as mask:
                    image.paste(color, (layer['x'],layer['y']), mask.convert('L'))
        if include_cutout and data.get('cutout', {}).get('enabled'):
            state = validate_cutout(data['cutout'])
            image = attach_source_alpha(root, data, image)
            with Image.open(root / state['alpha']) as alpha:
                image = compose_image(image, alpha, cutout_background(root, state), state['shadow'], state['feather'], state['transform'])
    return image


def attach_source_alpha(root, data, image):
    with Image.open(root / data['original']) as original:
        if 'A' in original.getbands() or 'transparency' in original.info:
            image = image.convert('RGBA')
            image.putalpha(ImageOps.exif_transpose(original).convert('RGBA').getchannel('A'))
    return image


def cutout_background(root, state):
    bg = state['background']
    if bg['mode'] == 'color':
        return bg['color']
    if bg['mode'] == 'image':
        with Image.open(root / bg['asset']) as image:
            return image.convert('RGBA')
    return None


def stack_snapshot(root, data, pixels=None):
    raw, icc = native_pixels(data, root) if pixels is None else pixels
    name = 'stack-' + uuid.uuid4().hex + '-source.tif'
    tags = [(34675, 'B', len(icc), icc, False)] if icc else []
    tifffile.imwrite(root / name, raw, photometric='rgb', metadata=None, compression='deflate', extratags=tags)
    return name


def ensure_stack(root, data):
    if data.get('layer_stack'): return
    old_cutout = copy.deepcopy(data.get('cutout'))
    pixels = None
    if old_cutout:
        raw, icc, _ = decode_original(root / data['original'])
        pixels = legacy_native_pixels(data, root, raw, icc, False)
    data['layer_stack'] = [stack_model.node('original', 'Original')]
    for patch in data['layers']:
        layer = stack_model.node('retouch', patch['name'], patch_ids=[patch['id']])
        layer.update(visible=patch['visible'], discarded=patch['discarded'])
        data['layer_stack'].append(layer)
        patch.update(visible=True, discarded=False)
    if old_cutout:
        # Opting into layers must preserve a previously saved v2 appearance.
        if old_cutout['enabled']:
            for item in data['layer_stack']: item['visible'] = False
        state = copy.deepcopy(old_cutout)
        transform = state.pop('transform', dict(DEFAULT_TRANSFORM))
        state['transform'] = dict(DEFAULT_TRANSFORM)
        background = copy.deepcopy(state['background'])
        state['background'] = {'mode': 'transparent', 'color': '#ffffff'}
        if background['mode'] != 'transparent':
            image = cutout_background(root, old_cutout)
            if isinstance(image, str): image = Image.new('RGBA', (data['width'], data['height']), image)
            layer = add_stack_image(root, data, image, background.get('name', 'Background'), commit=False,
                                    attribution=background.get('attribution'), reference_attributions=background.get('reference_attributions'))
            layer['visible'] = old_cutout['enabled']
        if state['shadow']['enabled']:
            # v2 cast its shadow after transforming the subject, in canvas
            # coordinates. Preserve that exact appearance as a separate layer;
            # new cutout layers keep their editable shadow attached to the item.
            raw, icc = pixels
            with Image.open(root / state['alpha']) as mask:
                transformed = compose_native(raw, mask, None, DEFAULT_SHADOW, state['feather'], transform)
            maximum = np.iinfo(raw.dtype).max
            alpha = Image.fromarray(np.rint(transformed[..., 3].astype(np.float64)*255/maximum).astype(np.uint8))
            shadow = copy.deepcopy(state['shadow'])
            if icc:
                swatch = ImageCms.profileToProfile(Image.new('RGB',(1,1),shadow['color']), SRGB, ImageCms.ImageCmsProfile(io.BytesIO(icc)), outputMode='RGB')
                shadow['color'] = '#%02x%02x%02x' % swatch.getpixel((0,0))
            shadow_pixels = np.asarray(underlay((data['width'],data['height']), alpha, None, shadow)).astype(raw.dtype)
            if raw.dtype == np.uint16: shadow_pixels *= 257
            shade = stack_model.node('image','Cutout shadow',source=stack_snapshot(root,data,(shadow_pixels,icc)))
            shade['visible'] = old_cutout['enabled']
            data['layer_stack'].append(shade)
            state['shadow']['enabled'] = False
        layer = stack_model.node('cutout', 'Cutout', source=stack_snapshot(root, data, pixels), cutout=state)
        layer.update(visible=old_cutout['enabled'], transform=transform)
        data['layer_stack'].append(layer)
    sync_stack_cutout(data)


def sync_stack_cutout(data, lid=None):
    nodes = [n for n in data.get('layer_stack', []) if n['kind'] == 'cutout' and not n['discarded']]
    selected = next((n for n in nodes if n['id'] == lid), None) if lid else None
    selected = selected or (nodes[-1] if nodes else None)
    if selected:
        data['cutout'] = copy.deepcopy(selected['cutout'])
        data['cutout'].update(enabled=selected['visible'], transform=copy.deepcopy(selected['transform']))
    else: data.pop('cutout', None)


def stack_layer(data, lid, *, writable=False, kind=None):
    layer = next((n for n in data.get('layer_stack', []) if n['id'] == lid), None)
    if layer is None or layer['discarded']: raise HTTPException(404, 'Layer not found.')
    if kind and layer['kind'] != kind: raise HTTPException(400, 'Select a ' + kind + ' layer first.')
    if writable and (layer['locked'] or not layer['visible']):
        raise HTTPException(409, 'Select a visible, unlocked layer before editing.')
    return layer


def selected_cutout(data, lid=None, writable=True):
    nodes = [n for n in data.get('layer_stack', []) if n['kind'] == 'cutout' and not n['discarded']]
    return stack_layer(data, lid or (nodes[-1]['id'] if nodes else ''), writable=writable, kind='cutout')


def commit_stack(root, data, selected=None):
    stack_model.validate_stack(data['layer_stack'], data['layers'])
    previous = read_session(data['id'])
    if previous.get('layer_stack') == data['layer_stack'] and previous.get('layers') == data['layers']:
        return public(data)
    data['stack_undo'] = (data.get('stack_undo', []) + [copy.deepcopy(previous.get('layer_stack', data['layer_stack']))])[-20:]
    data['stack_redo'] = []
    sync_stack_cutout(data, selected)
    data['revision'] += 1; write_session(root, data)
    result = public(data)
    if selected: result['selected_layer_id'] = selected
    return result


def add_stack_image(root, data, image, name, *, below=None, commit=True, attribution=None, reference_attributions=None):
    ensure_stack(root, data)
    asset = 'stack-' + uuid.uuid4().hex + '-source.png'
    image.convert('RGBA').save(root / asset, icc_profile=SRGB.tobytes())
    layer = stack_model.node('image', Path(name).name[:255] or 'Background', source=asset)
    if attribution is not None: layer['attribution'] = validate_attribution(attribution)
    if reference_attributions: layer['reference_attributions'] = validate_attributions(reference_attributions)
    target = next((i for i, node in enumerate(data['layer_stack']) if node['id'] == below and node['kind'] == 'cutout'), None)
    if target is None:
        target = next((i for i in range(len(data['layer_stack'])-1, -1, -1) if data['layer_stack'][i]['kind'] == 'cutout' and not data['layer_stack'][i]['discarded']), len(data['layer_stack']))
    data['layer_stack'].insert(target, layer)
    return commit_stack(root, data, layer['id']) if commit else layer


def add_stack_cutout(root, data, alpha, *, source_layer_id=None):
    ensure_stack(root, data)
    if source_layer_id:
        source_layer = stack_layer(data, source_layer_id)
        raw, icc, _ = decode_original(root / data['original'])
        pixels = stack_model.native_layer(data, root, source_layer, raw, icc, decode_original), icc
    else: pixels = native_pixels(data, root)
    layer = stack_model.node('cutout', 'Cutout', source=stack_snapshot(root, data, pixels),
                             cutout=initial_cutout(save_cutout_alpha(root, alpha)))
    data['layer_stack'].append(layer)
    return commit_stack(root, data, layer['id'])


def export_exif(original, size):
    # TIFF pointer/strip tags cannot be copied into a newly encoded TIFF.
    if original.suffix.lower() in {'.tif', '.tiff'}:
        return None
    with Image.open(original) as image:
        exif = image.getexif()
        if not exif:
            return None
        width, height = size
        exif[274] = 1
        exif[256], exif[257] = width, height
        details = exif.get_ifd(34665)
        if details:
            details[40962], details[40963] = width, height
            exif[34665] = details
        return exif.tobytes()


def native_pixels(data, root, include_cutout=True):
    raw, icc, _ = decode_original(root / data['original'])
    if data.get('layer_stack'):
        return stack_model.render_native(data, root, raw, icc, decode_original), icc
    return legacy_native_pixels(data, root, raw, icc, include_cutout)


def legacy_native_pixels(data, root, raw, icc, include_cutout=True):
    for layer in data['layers']:
        if not layer['visible'] or layer.get('discarded'):
            continue
        if layer.get('kind') == 'snapshot':
            snapshot, snapshot_icc, _ = decode_original(root / layer['snapshot'])
            if snapshot.shape != raw.shape or snapshot.dtype != raw.dtype or snapshot_icc != icc:
                raise ValueError('Merged layer precision, dimensions, or color profile do not match the original.')
            raw = snapshot
            continue
        with Image.open(root / layer['color']) as image:
            patch = image.convert('RGB')
        if icc:
            patch = ImageCms.profileToProfile(patch, SRGB, ImageCms.ImageCmsProfile(io.BytesIO(icc)), outputMode='RGB')
        color = np.asarray(patch).astype(np.float32)
        if raw.dtype == np.uint16:
            color *= 257
        with Image.open(root / layer['mask']) as mask_image:
            mask = np.asarray(mask_image.convert('L')).astype(np.float32)[..., None] / 255
        x,y=layer['x'],layer['y']; h,w=color.shape[:2]
        area = raw[y:y+h, x:x+w, :3]
        area[:] = np.rint(area.astype(np.float32)*(1-mask)+color*mask).astype(raw.dtype)
    if include_cutout and data.get('cutout', {}).get('enabled'):
        state = validate_cutout(data['cutout'])
        background = cutout_background(root, state)
        shadow = dict(state['shadow'])
        # Working backgrounds and UI colors are sRGB; match the original profile
        # before compositing into its native uint8/uint16 pixel array.
        if icc:
            def to_native_color(color):
                sample = Image.new('RGB', (1, 1), color)
                return ImageCms.profileToProfile(sample, SRGB, ImageCms.ImageCmsProfile(io.BytesIO(icc)), outputMode='RGB')
            if isinstance(background, str):
                background = to_native_color(background).resize((data['width'], data['height']))
            elif background is not None:
                bg_alpha = background.getchannel('A')
                background = ImageCms.profileToProfile(background.convert('RGB'), SRGB, ImageCms.ImageCmsProfile(io.BytesIO(icc)), outputMode='RGB').convert('RGBA')
                background.putalpha(bg_alpha)
            shadow['color'] = '#%02x%02x%02x' % to_native_color(shadow['color']).getpixel((0, 0))
        with Image.open(root / state['alpha']) as alpha:
            raw = compose_native(raw, alpha, background, shadow, state['feather'], state['transform'])
    return raw, icc


def flatten(data, target, allow_8bit=False, *, root_override=None):
    root = Path(root_override) if root_override is not None else folder(data['id'])
    raw, icc = native_pixels(data, root)
    if target.suffix.lower() in {'.jpg', '.jpeg'} and raw.shape[2] == 4 and np.any(raw[..., 3] < np.iinfo(raw.dtype).max):
        raise ValueError('JPEG cannot store transparency. Choose PNG, TIFF, or WebP, or add a solid background.')
    if target.suffix.lower() in {'.tif','.tiff'}:
        tags = [(34675, 'B', len(icc), icc, False)] if icc else []
        tifffile.imwrite(target, raw, photometric='rgb', metadata=None, compression='deflate', extratags=tags)
    else:
        if raw.dtype == np.uint16:
            if not allow_8bit:
                # Only an explicit format choice can lower source precision.
                raise ValueError('Choose PNG, JPEG, or WebP explicitly for an 8-bit export, or use TIFF to preserve 16-bit precision.')
            raw = np.rint(raw.astype(np.float32) / 257).astype(np.uint8)
        image = Image.fromarray(raw)
        kwargs = {'icc_profile': icc or SRGB.tobytes()}
        exif = export_exif(root / data['original'], image.size)
        if exif:
            kwargs['exif'] = exif
        if target.suffix.lower() in {'.jpg','.jpeg'}:
            image = image.convert('RGB'); kwargs.update(quality=100, subsampling=0)
        elif target.suffix.lower() == '.webp':
            kwargs['lossless'] = True
        image.save(target, **kwargs)


@router.get('/remove')
async def page(request:Request):
    guard(request)
    nonce=secrets.token_urlsafe(24)
    try:
        html = render_editor(nonce, issue_browser_token(CSRF))
    except RuntimeError as error:
        raise HTTPException(503, str(error)) from error
    # Imported modules and compiled CSS load only from the dedicated manifest
    # route. Runtime Griffel styles still require this document's random nonce.
    assets = f" {request.url.scheme}://{request.url.netloc}/frontend-assets/"
    fonts = f'; font-src{assets}'
    return HTMLResponse(html,headers={**HEADERS,
        'X-Frame-Options':'DENY','Referrer-Policy':'no-referrer',
        'Content-Security-Policy':f"default-src 'none'; base-uri 'none'; frame-ancestors 'none'; style-src 'nonce-{nonce}'{assets}; script-src 'nonce-{nonce}'{assets}; img-src 'self' blob: data: https://images.unsplash.com https://plus.unsplash.com; connect-src 'self'; form-action 'self'{fonts}"})


@router.get('/frontend-assets/{asset:path}')
async def editor_asset(asset: str, request: Request):
    guard(request)
    try:
        path, media_type = frontend_asset(asset)
    except FileNotFoundError as error:
        raise HTTPException(404, 'Frontend asset not found') from error
    return FileResponse(path, media_type=media_type, headers=ASSET_HEADERS)


@router.get('/api/local-remove/runtime')
async def runtime(request: Request):
    guard(request)
    return {'application': 'local-remove', 'version': APP_VERSION,
            'data_root': str(data_root().absolute())}


@router.post('/api/local-remove/reload-config')
async def reload_config(request: Request):
    launcher_guard(request)
    from main import generation_lock
    if generation_lock.locked():
        raise HTTPException(409, 'Wait for the current removal to finish before changing AI settings.')
    config.COMFY_HOST = '127.0.0.1'
    config.COMFY_PORT = read_config()['comfy_port']
    return {'ok': True}


@router.post('/api/local-remove/heartbeat')
async def heartbeat(request: Request):
    launcher_guard(request)
    import runtime_lifecycle
    runtime_lifecycle.heartbeat()
    return {'ok': True}


@router.post('/api/local-remove/shutdown')
async def shutdown(request: Request):
    launcher_guard(request)
    from main import generation_lock
    from batch_tools import active_queue
    if generation_lock.locked() or active_queue():
        raise HTTPException(409, 'Wait for the current image or batch operation to finish before closing the service.')
    import runtime_lifecycle
    runtime_lifecycle.shutdown_requested = True
    return {'ok': True}


@router.get('/api/local-remove/status')
async def status(request:Request):
    guard(request)
    settings = settings_response()
    selected = next((item for item in settings['models'] if item['id'] == settings['model']), None)
    result = {'model_id': settings['model'], 'models': settings['models'],
              'model': selected['label'] if selected else settings['model'],
              'retouch_ready': heal_option()['available']}
    if selected and selected.get('reason'):
        result['reason'] = selected['reason']
    from managed_ai import service_state
    service = await service_state()
    if service.get('reason') and not result.get('reason'):
        result['reason'] = service['reason']
    return {**result, 'ready':service['ready'] and bool(selected and selected.get('available')),
            'device':service['device'] or 'Backend not running'}


@router.get('/api/local-remove/sessions')
async def sessions(request:Request):
    guard(request)
    result=[]
    for path in sorted(SESSIONS.glob('*/session.json'),key=lambda p:p.stat().st_mtime,reverse=True)[:20]:
        try: result.append(public(json.loads(path.read_text(encoding='utf-8'))))
        except (OSError, ValueError): pass
    return result


@router.post('/api/local-remove/import')
async def upload(request:Request,file:UploadFile=File(...)):
    guard(request,True)
    suffix=Path(file.filename or '').suffix.lower()
    if suffix not in SUPPORTED: raise HTTPException(400,'Choose a JPEG, PNG, TIFF, or WebP image.')
    with tempfile.TemporaryDirectory(dir=ROOT) as temporary:
        path=Path(temporary)/('input'+suffix)
        with path.open('wb') as output:
            while chunk:=await file.read(1024*1024): output.write(chunk)
        try: return await asyncio.to_thread(create_session,path,file.filename)
        except Exception as error: raise HTTPException(400,str(error)) from error


class OpenLocal(BaseModel):
    path:str


class OpenFiles(BaseModel):
    paths:list[str] = Field(min_length=1, max_length=10000)


@router.post('/api/local-remove/open-local')
async def open_local(request:Request,payload:OpenLocal):
    launcher_guard(request)
    path=Path(payload.path).resolve()
    async with source_gate(path):
        try:
            return public(await asyncio.to_thread(reuse_or_create_session,path))
        except HTTPException:
            raise
        except Exception as error:
            raise HTTPException(400,str(error)) from error


@router.post('/api/local-remove/register-folder')
async def register_folder(request:Request,payload:OpenLocal):
    launcher_guard(request)
    path=Path(payload.path).resolve()
    if not path.is_dir():
        raise HTTPException(400,'Choose an existing folder of images.')
    def scan():
        return sorted((item.resolve() for item in path.iterdir()
                       if item.is_file() and item.suffix.lower() in SUPPORTED
                       and not item.name.startswith('.local-remove-') and item.resolve().parent == path),
                      key=natural_name)
    try:
        paths=await asyncio.to_thread(scan)
        if not paths:
            raise HTTPException(400,'This folder has no JPEG, PNG, TIFF, or WebP images.')
        async with registration_lock:
            data=await asyncio.to_thread(register_collection,paths,path.name or str(path),path)
        return {'collection':public_collection(data),'session':None}
    except HTTPException:
        raise
    except OSError as error:
        raise HTTPException(400,'The image folder could not be read: '+str(error)) from error


@router.post('/api/local-remove/open-files')
async def open_files(request:Request,payload:OpenFiles):
    launcher_guard(request)
    paths=[]; seen=set()
    for name in payload.paths:
        path=Path(name).resolve()
        key=path_key(path)
        if not path.is_file() or path.suffix.lower() not in SUPPORTED:
            raise HTTPException(400,'Choose JPEG, PNG, TIFF, or WebP files. Use Open folder for a folder.')
        if key not in seen:
            paths.append(path); seen.add(key)
    paths.sort(key=natural_name)
    parents={path_key(path.parent) for path in paths}
    name=paths[0].parent.name if len(parents)==1 else 'Selected photos'
    async with registration_lock:
        data=await asyncio.to_thread(register_collection,paths,name or 'Selected photos')
    return {'collection':public_collection(data),'session':None}


@router.get('/api/local-remove/collection/{cid}')
async def collection(cid:str,request:Request):
    guard(request)
    return public_collection(read_collection(cid))


@router.get('/api/local-remove/collection/{cid}/entry/{eid}/thumbnail')
async def collection_thumbnail(cid:str,eid:str,request:Request):
    guard(request)
    data=read_collection(cid)
    entry=next((item for item in data['entries'] if item['id']==eid),None)
    if entry is None:
        raise HTTPException(404,'Image is not part of this collection.')
    async with thumbnail_gate:
        def make_thumbnail():
            saved=None
            if entry.get('session_id'):
                try:
                    candidate=read_session(entry['session_id'])
                    if candidate.get('source_path') and path_key(candidate['source_path'])==path_key(entry['path']):
                        saved=candidate
                except (HTTPException,OSError,ValueError,KeyError,TypeError):
                    pass
            if saved:
                identity=f'{saved["id"]}:{saved["revision"]}'
            else:
                stat=Path(entry['path']).stat()
                identity=f'{stat.st_mtime_ns}:{stat.st_size}'
            key=hashlib.sha256(identity.encode('utf-8')).hexdigest()[:24]
            cache=cache_dir()/'thumbnails'; cache.mkdir(parents=True, exist_ok=True)
            prefix=cid+'-'+eid+'-'
            target=cache/(prefix+key+'.jpg')
            if not target.is_file():
                if saved:
                    image=render(saved)
                else:
                    _,_,image=decode_original(Path(entry['path']))
                image.thumbnail((160,160),Image.Resampling.LANCZOS)
                # Filmstrip JPEGs are presentation copies. Composite alpha only
                # after resizing; document pixels and export rules stay intact.
                if 'A' in image.getbands():
                    matte=Image.new('RGB',image.size,'white')
                    matte.paste(image,mask=image.getchannel('A'))
                    image=matte
                elif image.mode!='RGB':
                    image=image.convert('RGB')
                temporary=cache/('thumbnail-'+uuid.uuid4().hex+'.jpg')
                try:
                    image.save(temporary,quality=85,icc_profile=SRGB.tobytes())
                    os.replace(temporary,target)
                finally:
                    temporary.unlink(missing_ok=True)
                for old in cache.glob(prefix+'*.jpg'):
                    if old!=target:
                        try: old.unlink()
                        except OSError: pass
            return target
        try:
            target=await asyncio.to_thread(make_thumbnail)
        except Exception as error:
            raise HTTPException(400,'This image preview is unavailable.') from error
    return FileResponse(target,headers=HEADERS)


@router.post('/api/local-remove/collection/{cid}/entry/{eid}/open')
async def open_collection_entry(cid:str,eid:str,request:Request):
    guard(request,True)
    async with registration_lock, collection_locks.setdefault(cid,asyncio.Lock()):
        data=read_collection(cid)
        index=next((index for index,entry in enumerate(data['entries']) if entry['id']==eid),None)
        if index is None:
            raise HTTPException(404,'Image is not part of this collection.')
        entry=data['entries'][index]
        path=Path(entry['path'])
        try:
            async with source_gate(path):
                saved=await asyncio.to_thread(reuse_or_create_session,path)
            async with locks.setdefault(saved['id'],asyncio.Lock()):
                # Do not nest source -> session locks; saving uses the reverse.
                saved=read_session(saved['id'])
                saved.update(collection_id=cid,entry_id=eid)
                write_session(folder(saved['id']),saved)
            entry['session_id']=saved['id']; entry.pop('error',None)
            data['index']=index; write_collection(data)
            return {'session':public(saved),'collection':public_collection(data),'index':index}
        except Exception as error:
            detail=error.detail if isinstance(error,HTTPException) else str(error)
            entry['error']=detail; write_collection(data)
            if isinstance(error,HTTPException):
                raise
            raise HTTPException(400,'This image could not be opened: '+str(error)) from error


@router.get('/api/local-remove/session/{sid}')
async def session(sid:str,request:Request):
    guard(request); return public(read_session(sid))


@router.get('/api/local-remove/session/{sid}/preview')
async def preview(sid:str,request:Request,original:bool=False,full:bool=False):
    guard(request)
    async with locks.setdefault(sid, asyncio.Lock()):
        data=read_session(sid)
        def make_preview():
            root=folder(sid)
            prefix='full-' if full else ''
            stem=prefix+('original-preview' if original else f'preview-{data["revision"]}')
            # Revision assets are immutable. Reopening a document or retrying a
            # request should reuse the encoded pixels instead of recompositing.
            for suffix in ('png', 'jpg'):
                cached=root/(stem+'.'+suffix)
                if cached.is_file(): return cached
            img=render(data,original)
            if not full:
                img.thumbnail((3000,3000),Image.Resampling.LANCZOS)
            suffix = 'png' if full or img.mode == 'RGBA' else 'jpg'
            path=root/(stem+'.'+suffix)
            temporary=root/('preview-'+uuid.uuid4().hex+'.'+suffix)
            try:
                img.save(temporary,quality=95,compress_level=1,icc_profile=SRGB.tobytes())
                os.replace(temporary,path)
            finally:
                temporary.unlink(missing_ok=True)
            return path
        target=await asyncio.to_thread(make_preview)
    return FileResponse(target,headers=HEADERS)


@router.get('/api/local-remove/session/{sid}/base-display')
async def base_display(sid:str,request:Request):
    guard(request)
    async with locks.setdefault(sid, asyncio.Lock()):
        data = read_session(sid); root = folder(sid)
        with Image.open(root / data['original']) as original:
            has_alpha = 'A' in original.getbands() or 'transparency' in original.info
        target = root / ('base-display.png' if has_alpha else 'base.png')
        if has_alpha and not target.is_file():
            await asyncio.to_thread(lambda: render(data, original=True).save(target, icc_profile=SRGB.tobytes()))
    return FileResponse(target,media_type='image/png',headers=ASSET_HEADERS)


@router.get('/api/local-remove/session/{sid}/layer/{lid}/display')
async def layer_display(sid:str,lid:str,request:Request):
    guard(request)
    # Resolve only a known layer; no caller-controlled filename reaches disk.
    async with locks.setdefault(sid,asyncio.Lock()):
        data=read_session(sid)
        layer=next((item for item in data['layers'] if item['id']==lid),None)
        if layer is None:
            raise HTTPException(404,'Layer not found')
        root=folder(sid); target=root/(lid+'-display.png')
        if not target.is_file():
            def make_asset():
                temporary=root/('display-'+uuid.uuid4().hex+'.png')
                try:
                    with Image.open(root/layer['color']) as color, Image.open(root/layer['mask']) as mask:
                        rgba=color.convert('RGBA')
                        rgba.putalpha(mask.convert('L'))
                        rgba.save(temporary,compress_level=1,icc_profile=SRGB.tobytes())
                    os.replace(temporary,target)
                finally:
                    temporary.unlink(missing_ok=True)
            await asyncio.to_thread(make_asset)
    return FileResponse(target,media_type='image/png',headers=ASSET_HEADERS)


class RemoveRequest(BaseModel):
    mask:str
    revision:int
    model:Literal['klein', 'heal', 'qwen']|None=None
    heal_method:Literal['texture', 'telea']=Field(default_factory=lambda: heal_option()['default_method'])
    variant:Literal['int8', 'bf16']='int8'
    prompt:str=Field(default='',max_length=4000)
    target_layer_id:str|None=None


@router.post('/api/local-remove/session/{sid}/remove')
async def remove(sid:str,request:Request,payload:RemoveRequest):
    guard(request,True)
    # Snapshot before waiting for either lock: changing settings cannot change a queued edit.
    model = payload.model if payload.model is not None else read_settings()['model']
    selected = {'label': 'Qwen Image 2.1'} if model == 'qwen' else available_model(model)
    generation_gate = nullcontext()
    if model != 'heal':
        from main import generation_lock
        generation_gate = generation_lock
    async with locks.setdefault(sid,asyncio.Lock()), generation_gate:
        data=read_session(sid)
        if data['revision']!=payload.revision: raise HTTPException(409,'The edit changed. Reload the session.')
        root=folder(sid)
        try:
            target_layer = None
            if payload.target_layer_id:
                ensure_stack(root, data)
                target_layer = stack_layer(data, payload.target_layer_id, writable=True, kind='retouch')
            with Image.open(io.BytesIO(base64.b64decode(payload.mask,validate=True))) as received:
                if received.width>8192 or received.height>8192: raise ValueError('Selection is too large')
                mask=received.convert('L').resize((data['width'],data['height']),Image.Resampling.LANCZOS)
            if not mask.getbbox(): raise ValueError('Select an object first.')
            if model == 'heal':
                result=await asyncio.to_thread(lambda: heal_image(render(data,include_cutout=False),mask,payload.heal_method))
            else:
                buf=io.BytesIO(); mask.save(buf,format='PNG')
                source=root/'generation-source.png'
                await asyncio.to_thread(lambda: render(data,include_cutout=False).save(source))
                if model == 'qwen':
                    from qwen_image import run_qwen_removal
                    mask_path = root / 'generation-mask.png'
                    mask.save(mask_path)
                    edited = await run_qwen_removal(source, mask_path, prompt=payload.prompt, variant=payload.variant, seed=secrets.randbits(48))
                    # Qwen's adapter already composites the selection. Store the
                    # edited bounding rectangle once (do not multiply soft alpha twice).
                    box = mask.getbbox()
                    color_buf = io.BytesIO(); edited.convert('RGB').crop(box).save(color_buf, format='PNG')
                    mask_buf = io.BytesIO(); Image.new('L', (box[2]-box[0], box[3]-box[1]), 255).save(mask_buf, format='PNG')
                    result = {'x': box[0], 'y': box[1], 'color': base64.b64encode(color_buf.getvalue()), 'mask': base64.b64encode(mask_buf.getvalue())}
                else:
                    result=await run_local_removal(source,buf.getvalue(),secrets.randbits(48),model)
            lid=uuid.uuid4().hex
            if target_layer and target_layer['transform'] != DEFAULT_TRANSFORM:
                with Image.open(io.BytesIO(base64.b64decode(result['color']))) as color_image, Image.open(io.BytesIO(base64.b64decode(result['mask']))) as mask_image:
                    color_image, mask_image, position = stack_model.inverse_patch(color_image, mask_image.convert('L'), (result['x'], result['y']), (data['width'], data['height']), target_layer['transform'])
                result.update(x=position[0], y=position[1])
                for role, image in (('color', color_image), ('mask', mask_image)):
                    buffer = io.BytesIO(); image.save(buffer, format='PNG'); result[role] = base64.b64encode(buffer.getvalue())
            for kind in ('color','mask'):
                (root/f'{lid}-{kind}.png').write_bytes(base64.b64decode(result[kind]))
            action='Heal' if model == 'heal' else 'Remove'
            data['layers'].append({'id':lid,'name':f'{action} {len(data["layers"])+1}',
                                  'x':result['x'],'y':result['y'],'color':f'{lid}-color.png','mask':f'{lid}-mask.png',
                                  'visible':True,'discarded':False,
                                  'model':model,'model_label':('Texture repair' if payload.heal_method=='texture' else 'Dust & scratches') if model=='heal' else selected['label'],
                                  **({'heal_method':payload.heal_method} if model=='heal' else {})})
            if data.get('layer_stack'):
                if target_layer is None:
                    target_layer = stack_model.node('retouch', f'{action} {len(data["layer_stack"])}', patch_ids=[])
                    data['layer_stack'].append(target_layer)
                target_layer['patch_ids'].append(lid)
                return commit_stack(root, data, target_layer['id'])
            data['revision']+=1; write_session(root,data)
            return public(data)
        except HTTPException: raise
        except Exception as error: raise HTTPException(400,str(error)) from error


class LayerUpdate(BaseModel):
    visible:bool|None=None
    discarded:bool|None=None
    revision:int|None=Field(default=None,ge=0)


@router.patch('/api/local-remove/session/{sid}/layer/{lid}')
async def update_layer(sid:str,lid:str,request:Request,payload:LayerUpdate):
    guard(request,True)
    async with locks.setdefault(sid,asyncio.Lock()):
        data=read_session(sid)
        if payload.revision is not None and payload.revision!=data['revision']:
            raise HTTPException(409,'The edit changed. Reload the session before changing a layer.')
        layer=next((x for x in data['layers'] if x['id']==lid),None)
        if layer is None: raise HTTPException(404,'Layer not found')
        updates=payload.model_dump(exclude_none=True,exclude={'revision'})
        if any(layer.get(key)!=value for key,value in updates.items()):
            layer.update(updates)
            data['revision']+=1; write_session(folder(sid),data)
        return public(data)


class MergeRequest(BaseModel):
    revision:int=Field(ge=0)
    layer_id:str|None=None


class StackCreate(MergeRequest):
    kind:Literal['retouch']='retouch'
    name:str=Field(default='Retouch', min_length=1, max_length=255)


class StackUpdate(MergeRequest):
    name:str|None=Field(default=None,min_length=1,max_length=255)
    visible:bool|None=None
    locked:bool|None=None
    discarded:bool|None=None
    opacity:float|None=Field(default=None,ge=0,le=1,allow_inf_nan=False)
    transform:dict|None=None
    index:int|None=Field(default=None,ge=0,le=999)


def checked_stack(sid, revision):
    data = read_session(sid); root = folder(sid)
    if data['revision'] != revision: raise HTTPException(409, 'The edit changed. Reload the document before changing layers.')
    ensure_stack(root, data)
    return data, root


@router.post('/api/local-remove/session/{sid}/stack')
async def enable_stack(sid:str, request:Request, payload:MergeRequest):
    guard(request, True)
    async with locks.setdefault(sid, asyncio.Lock()):
        data, root = checked_stack(sid, payload.revision)
        # This is a representation migration, not an image edit. Preserve the
        # document revision and save state until a real layer mutation occurs.
        stack_model.validate_stack(data['layer_stack'], data['layers'])
        write_session(root, data)
        return public(data)


@router.post('/api/local-remove/session/{sid}/stack/layers')
async def create_stack_layer(sid:str, request:Request, payload:StackCreate):
    guard(request, True)
    async with locks.setdefault(sid, asyncio.Lock()):
        data, root = checked_stack(sid, payload.revision)
        layer = stack_model.node('retouch', payload.name.strip(), patch_ids=[])
        data['layer_stack'].append(layer)
        try: return commit_stack(root, data, layer['id'])
        except ValueError as error: raise HTTPException(400, str(error)) from error


@router.patch('/api/local-remove/session/{sid}/stack/layer/{lid}')
async def update_stack_layer(sid:str, lid:str, request:Request, payload:StackUpdate):
    guard(request, True)
    async with locks.setdefault(sid, asyncio.Lock()):
        data, root = checked_stack(sid, payload.revision)
        layer = next((n for n in data['layer_stack'] if n['id'] == lid), None)
        if layer is None: raise HTTPException(404, 'Layer not found.')
        changes = payload.model_dump(exclude_none=True, exclude={'revision', 'index'})
        if layer['locked'] and any(k in changes for k in ('transform', 'opacity', 'discarded')) and payload.locked is not False:
            raise HTTPException(409, 'Unlock the layer before changing its contents.')
        if lid == 'original' and payload.discarded:
            raise HTTPException(400, 'Hide the original with its visibility control; it remains available for recovery.')
        if payload.transform is not None:
            changes['transform'] = {**layer['transform'], **payload.transform}
        layer.update(changes)
        if payload.index is not None:
            if layer['locked']: raise HTTPException(409, 'Unlock the layer before moving it in the stack.')
            data['layer_stack'].remove(layer)
            data['layer_stack'].insert(min(payload.index, len(data['layer_stack'])), layer)
        try: return commit_stack(root, data, lid)
        except ValueError as error: raise HTTPException(400, str(error)) from error


@router.get('/api/local-remove/session/{sid}/stack/layer/{lid}/display')
async def stack_layer_display(sid:str, lid:str, request:Request, r:str|None=None):
    guard(request)
    async with locks.setdefault(sid, asyncio.Lock()):
        data = read_session(sid); root = folder(sid); layer = stack_layer(data, lid)
        key = stack_model.display_key(data, layer)
        target = root / ('stack-display-' + lid + '-' + key + '.png')
        if not target.is_file():
            def make():
                temporary = root / ('display-' + uuid.uuid4().hex + '.png')
                try:
                    raw, icc, _ = decode_original(root / data['original'])
                    pixels = stack_model.native_layer(data, root, layer, raw, icc, decode_original, transformed=False)
                    stack_model.display(pixels, icc).save(temporary, compress_level=1, icc_profile=SRGB.tobytes())
                    os.replace(temporary, target)
                finally:
                    temporary.unlink(missing_ok=True)
            await asyncio.to_thread(make)
    # Only a URL carrying the current pixel identity is safe to cache forever.
    # Legacy revision URLs still work without caching a different edit's pixels.
    return FileResponse(target, media_type='image/png', headers=ASSET_HEADERS if r == key else HEADERS)


async def change_stack_history(sid, request, revision, direction):
    guard(request, True)
    async with locks.setdefault(sid, asyncio.Lock()):
        data, root = checked_stack(sid, revision)
        source = 'stack_' + direction; destination = 'stack_redo' if direction == 'undo' else 'stack_undo'
        if not data.get(source): raise HTTPException(400, 'There is no layer change to ' + direction + '.')
        previous = data[source].pop()
        data[destination] = (data.get(destination, []) + [copy.deepcopy(data['layer_stack'])])[-20:]
        data['layer_stack'] = previous; sync_stack_cutout(data)
        data['revision'] += 1; write_session(root, data)
        return public(data)


@router.post('/api/local-remove/session/{sid}/stack/undo')
async def undo_stack(sid:str, request:Request, payload:MergeRequest):
    return await change_stack_history(sid, request, payload.revision, 'undo')


@router.post('/api/local-remove/session/{sid}/stack/redo')
async def redo_stack(sid:str, request:Request, payload:MergeRequest):
    return await change_stack_history(sid, request, payload.revision, 'redo')


@router.post('/api/local-remove/session/{sid}/merge')
async def merge_visible(sid:str,request:Request,payload:MergeRequest):
    guard(request,True)
    async with locks.setdefault(sid,asyncio.Lock()):
        data=read_session(sid)
        if data['revision']!=payload.revision:
            raise HTTPException(409,'The edit changed. Reload the session before merging.')
        if data.get('layer_stack'):
            root = folder(sid)
            source = await asyncio.to_thread(stack_snapshot, root, data)
            for layer in data['layer_stack']: layer['visible'] = False
            merged = stack_model.node('image', 'Merged visible', source=source)
            data['layer_stack'].append(merged)
            return commit_stack(root, data, merged['id'])
        if data.get('cutout', {}).get('enabled'):
            raise HTTPException(400, 'Disable the cutout before merging repair layers. Cutout settings remain separately editable in the project.')
        root=folder(sid); lid=uuid.uuid4().hex
        names={kind:f'{lid}-{kind}{".tif" if kind=="snapshot" else ".png"}' for kind in ('snapshot','color','mask')}
        def make_snapshot():
            flatten(data,root/names['snapshot'])
            render(data).save(root/names['color'])
            Image.new('L',(data['width'],data['height']),255).save(root/names['mask'])
        try:
            await asyncio.to_thread(make_snapshot)
            data['layers'].append({'id':lid,'name':f'Merged {len(data["layers"])+1}','kind':'snapshot',
                                   'model':'merge','model_label':'Merged visible','visible':True,'discarded':False,
                                   'x':0,'y':0,**names})
            data['revision']+=1; write_session(root,data)
        except Exception as error:
            for name in names.values():
                (root/name).unlink(missing_ok=True)
            raise HTTPException(400,'The merged layer could not be created: '+str(error)) from error
        return public(data)


OutputFormat = Literal['original', 'png', 'jpg', 'tif', 'webp']
FORMAT_SUFFIX = {'png':'.png', 'jpg':'.jpg', 'tif':'.tif', 'webp':'.webp'}


def normalized_format(suffix):
    return {'.jpg':'jpg', '.jpeg':'jpg', '.tif':'tif', '.tiff':'tif', '.png':'png', '.webp':'webp'}[suffix.lower()]


def publish_unique(temporary, source, suffix):
    """Install a fully rendered sibling without ever replacing an existing file."""
    for number in range(1, 100000):
        tail = '' if number == 1 else '-' + str(number)
        destination = source.with_name(source.stem + '-removed' + tail + suffix)
        try:
            os.link(temporary, destination)
        except FileExistsError:
            continue
        except OSError:
            if os.name != 'nt':
                raise
            # Windows rename refuses an existing destination. This supports
            # source folders on filesystems that do not implement hard links.
            try:
                os.rename(temporary, destination)
            except FileExistsError:
                continue
            return destination
        temporary.unlink()
        return destination
    raise ValueError('Too many saved copies share this name. Rename the source image first.')


def replace_verified(temporary, destination, expected_hash):
    try:
        unchanged = file_hash(destination) == expected_hash
    except OSError:
        unchanged = False
    if not unchanged:
        raise HTTPException(409, 'The source file changed while saving. Use Save Unique to keep both versions.')
    os.replace(temporary, destination)


class SaveRequest(BaseModel):
    return_to_source:bool=False
    mode:Literal['overwrite', 'unique']|None=None
    revision:int|None=Field(default=None, ge=0)
    format:OutputFormat='original'

    @model_validator(mode='after')
    def require_revision(self):
        if (self.mode is not None or 'format' in self.model_fields_set) and self.revision is None:
            raise ValueError('Supply the current session revision when saving.')
        if self.mode == 'unique' and self.return_to_source:
            raise ValueError('Choose either overwrite or a unique copy.')
        return self


@router.post('/api/local-remove/session/{sid}/save')
async def save(sid:str,request:Request,payload:SaveRequest):
    guard(request,True)
    async with locks.setdefault(sid,asyncio.Lock()):
        data=read_session(sid); root=folder(sid)
        if payload.revision is not None and data['revision'] != payload.revision:
            raise HTTPException(409,'The edit changed. Reload the session before saving.')
        mode=payload.mode or ('overwrite' if payload.return_to_source else 'export')
        source=Path(data['source_path']) if data.get('source_path') else None
        if mode in {'overwrite','unique'} and source is None:
            raise HTTPException(400,'Open this image through the Local Image application to save beside its source. Use download export for an uploaded image.')
        if mode == 'overwrite' and payload.format != 'original' and payload.format != normalized_format(source.suffix):
            raise HTTPException(400,'Use Save Unique to change format. Overwrite keeps the source format.')
        if mode in {'overwrite','unique'}:
            suffix=source.suffix if payload.format == 'original' else FORMAT_SUFFIX[payload.format]
            destination=source
        else:
            if payload.format == 'original':
                # New UI explicitly requests the input format; old download
                # clients that omit format keep their historical PNG/TIFF default.
                suffix=Path(data['original']).suffix if 'format' in payload.model_fields_set else ('.tif' if data['bit_depth']==16 else '.png')
            else:
                suffix=FORMAT_SUFFIX[payload.format]
            destination=root/('flattened'+suffix)
        gate=source_gate(source) if source is not None and mode != 'export' else nullcontext()
        async with gate:
            expected_hash=None
            if mode == 'overwrite':
                try:
                    expected_hash=await asyncio.to_thread(file_hash,source)
                except OSError:
                    raise HTTPException(409,'The source file is missing or unreadable. Use Save Unique to keep a separate copy.')
                if expected_hash not in {data.get('source_hash'),data.get('last_saved_hash')}:
                    raise HTTPException(409,'The file changed outside this session. Use Save Unique to keep both versions.')
            temporary=destination.parent/('.local-remove-'+uuid.uuid4().hex+suffix)
            try:
                await asyncio.to_thread(flatten,data,temporary,payload.format != 'original')
                rendered_hash=await asyncio.to_thread(file_hash,temporary) if mode=='overwrite' else None
                if mode == 'unique':
                    destination=await asyncio.to_thread(publish_unique,temporary,source,suffix)
                elif mode == 'overwrite':
                    await asyncio.to_thread(replace_verified,temporary,destination,expected_hash)
                else:
                    os.replace(temporary,destination)
                if mode == 'overwrite':
                    # Record our rendered bytes, never a later external writer's
                    # content observed after the atomic replacement.
                    data['last_saved_hash']=rendered_hash
                data.update(saved_revision=data['revision'],saved_name=destination.name,last_save_mode=mode,
                            last_saved_path=str(destination),last_saved_at=time.time())
                write_session(root,data)
            except HTTPException:
                raise
            except Exception as error:
                raise HTTPException(400,str(error)) from error
            finally:
                temporary.unlink(missing_ok=True)
        result={'saved':True,'name':destination.name,'returned':mode=='overwrite','mode':mode,
                'format':normalized_format(destination.suffix),
                'bit_depth':16 if data['bit_depth']==16 and destination.suffix.lower() in {'.tif','.tiff'} else 8,
                'download':f'/api/local-remove/session/{sid}/download?ext={suffix[1:]}' if mode=='export' else None,
                'session':public(data),'collection':None,'index':None}
        if data.get('collection_id'):
            try:
                current=public_collection(read_collection(data['collection_id']))
                result['collection']=current
                result['index']=next((i for i,entry in enumerate(current['entries']) if entry['id']==data.get('entry_id')),None)
            except HTTPException:
                pass
        return result


@router.get('/api/local-remove/session/{sid}/download')
async def download(sid:str,request:Request,ext:str='png'):
    guard(request)
    if ext not in {'png','jpg','jpeg','tif','tiff','webp'}: raise HTTPException(400,'Unsupported format')
    data=read_session(sid); path=folder(sid)/('flattened.'+ext)
    if not path.is_file(): raise HTTPException(404,'Flatten the image first.')
    return FileResponse(path,filename=Path(data['name']).stem+'-removed.'+ext,headers=HEADERS)


class ProjectSaveRequest(BaseModel):
    session_id:str
    revision:int=Field(ge=0)
    path:str|None=None
    expected_hash:str|None=Field(default=None,pattern=r'^[0-9a-f]{64}$')


def publish_project(temporary,destination,expected_hash):
    if expected_hash is not None:
        try:
            current=file_hash(destination)
        except OSError:
            current=None
        if current!=expected_hash:
            raise HTTPException(409,'The project changed outside Local Image. Use Save Project As to keep both versions.')
        os.replace(temporary,destination)
    else:
        try:
            os.link(temporary,destination)
            temporary.unlink()
        except FileExistsError:
            raise HTTPException(409,'A project already exists with this name. Choose another name or confirm replacement in Save Project As.')
        except OSError:
            if os.name!='nt':
                raise
            try:
                os.rename(temporary,destination)
            except FileExistsError:
                raise HTTPException(409,'A project already exists with this name. Choose another name or confirm replacement in Save Project As.')


@router.post('/api/local-remove/save-project')
async def save_project(request:Request,payload:ProjectSaveRequest):
    launcher_guard(request)
    sid=payload.session_id
    async with locks.setdefault(sid,asyncio.Lock()):
        data=read_session(sid); root=folder(sid)
        if data['revision']!=payload.revision:
            raise HTTPException(409,'The edit changed. Save the latest session revision.')
        selected=payload.path or data.get('project_path')
        if not selected:
            raise HTTPException(400,'Choose a location with Save Project As.')
        destination=Path(selected).resolve()
        if destination.suffix.lower()!='.lremove' or not destination.parent.is_dir():
            raise HTTPException(400,'Choose an existing folder and a .lremove project filename.')
        # Only authenticated native file pickers choose a path. A loaded archive
        # never supplies a source/project path; its path is set by open_project.
        expected=payload.expected_hash
        if expected is None and data.get('project_path') and path_key(destination)==path_key(data['project_path']):
            expected=data.get('project_hash')
        async with source_gate(destination):
            if destination.exists() and expected is None:
                raise HTTPException(409,'A project already exists with this name. Confirm replacement in Save Project As or choose another name.')
            if expected is not None:
                try:
                    current=await asyncio.to_thread(file_hash,destination)
                except OSError:
                    current=None
                if current!=expected:
                    raise HTTPException(409,'The project changed outside Local Image. Use Save Project As to keep both versions.')
            temporary=destination.parent/('.local-remove-project-'+uuid.uuid4().hex+'.tmp')
            try:
                await asyncio.to_thread(write_project,root,data,temporary)
                saved_hash=await asyncio.to_thread(file_hash,temporary)
                await asyncio.to_thread(publish_project,temporary,destination,expected)
                data.update(project_path=str(destination),project_hash=saved_hash,project_name=destination.name,
                            project_saved_revision=data['revision'],project_saved_at=time.time())
                write_session(root,data)
            except HTTPException:
                raise
            except Exception as error:
                raise HTTPException(400,'The project could not be saved: '+str(error)) from error
            finally:
                temporary.unlink(missing_ok=True)
        return {'saved':True,'name':destination.name,'session':public(data),'collection':None,'index':None}


def import_project(source,project_path=None):
    sid=str(uuid.uuid4()); target=SESSIONS/sid; target.mkdir()
    try:
        manifest=extract_project(source,target,decode_original)
        data={key:manifest[key] for key in ('name','width','height','bit_depth','revision','original','layers')}
        if 'cutout' in manifest:
            data['cutout'] = manifest['cutout']
        if 'generation' in manifest:
            data['generation'] = manifest['generation']
        for key in ('source_attribution', 'reference_attributions', 'upscale', 'layer_stack'):
            if key in manifest:
                data[key] = manifest[key]
        data.update(id=sid,source_path=None,source_hash=None,can_return=False,saved_revision=None,created=time.time())
        if project_path is not None:
            data.update(project_path=str(project_path),project_name=project_path.name,project_hash=file_hash(source),
                        project_saved_revision=data['revision'],project_saved_at=time.time())
        write_session(target,data)
        return {'session':public(data),'collection':None,'index':None}
    except Exception:
        # The directory is a newly generated session, never an archive path.
        if target.resolve().parent==SESSIONS.resolve():
            shutil.rmtree(target,ignore_errors=True)
        raise


@router.post('/api/local-remove/open-project')
async def open_project(request:Request,payload:OpenLocal):
    launcher_guard(request)
    source=Path(payload.path).resolve()
    if not source.is_file() or source.suffix.lower()!='.lremove':
        raise HTTPException(400,'Choose a Local Image .lremove project.')
    async with source_gate(source):
        try:
            if source.stat().st_size>MAX_TOTAL+2*1024**2:
                raise ValueError('This project exceeds the project size limit.')
            with tempfile.TemporaryDirectory(dir=ROOT) as temporary:
                copy=Path(temporary)/'project.lremove'
                await asyncio.to_thread(shutil.copyfile,source,copy)
                if await asyncio.to_thread(file_hash,source)!=await asyncio.to_thread(file_hash,copy):
                    raise HTTPException(409,'The project changed while opening. Open it again.')
                return await asyncio.to_thread(import_project,copy,source)
        except HTTPException:
            raise
        except Exception as error:
            raise HTTPException(400,'The project could not be opened: '+str(error)) from error


@router.post('/api/local-remove/import-project')
async def upload_project(request:Request,file:UploadFile=File(...)):
    guard(request,True)
    if Path(file.filename or '').suffix.lower()!='.lremove':
        raise HTTPException(400,'Choose a Local Image .lremove project.')
    try:
        with tempfile.TemporaryDirectory(dir=ROOT) as temporary:
            path=Path(temporary)/'project.lremove'; total=0
            with path.open('wb') as output:
                while chunk:=await file.read(1024**2):
                    total+=len(chunk)
                    if total>MAX_TOTAL+2*1024**2:
                        raise ValueError('This project exceeds the project size limit.')
                    output.write(chunk)
            return await asyncio.to_thread(import_project,path)
    except Exception as error:
        raise HTTPException(400,'The project could not be imported: '+str(error)) from error


@router.post('/api/local-remove/session/{sid}/export-project')
async def export_project(sid:str,request:Request,payload:MergeRequest):
    guard(request,True)
    async with locks.setdefault(sid,asyncio.Lock()):
        data=read_session(sid); root=folder(sid)
        if data['revision']!=payload.revision:
            raise HTTPException(409,'The edit changed. Export the latest session revision.')
        temporary=root/('project-'+uuid.uuid4().hex+'.tmp')
        try:
            await asyncio.to_thread(write_project,root,data,temporary)
            os.replace(temporary,root/'export.lremove')
        except Exception as error:
            raise HTTPException(400,'The project could not be exported: '+str(error)) from error
        finally:
            temporary.unlink(missing_ok=True)
        # A browser download may be cancelled. It never authorizes discarding
        # editable work or claims a native project file has been safely saved.
        return {'download':f'/api/local-remove/session/{sid}/download-project',
                'name':Path(data['name']).stem+'.lremove','session':public(data)}


@router.get('/api/local-remove/session/{sid}/download-project')
async def download_project(sid:str,request:Request):
    guard(request)
    data=read_session(sid); path=folder(sid)/'export.lremove'
    if not path.is_file():
        raise HTTPException(404,'Export the project first.')
    return FileResponse(path,filename=Path(data['name']).stem+'.lremove',media_type='application/zip',headers=HEADERS)


@router.get('/api/local-remove/session/{sid}/download-credits')
async def download_credits(sid: str, request: Request):
    guard(request)
    from stock_attribution import collect_attributions
    from urllib.parse import quote
    data = read_session(sid)
    credits = collect_attributions(data)
    if not credits:
        raise HTTPException(404, 'This image has no stock credits to export.')
    lines = ['Local Image — image credits', 'Image: ' + data['name'], '']
    for credit in credits:
        lines.extend([credit['title'] + ' — ' + credit['creator'],
                      'License: ' + credit['license'], 'Source: ' + credit['source_url'],
                      'License details: ' + credit['license_url'], credit['attribution'], ''])
    name = quote(Path(data['name']).stem + '-credits.txt', safe='')
    return Response('\n'.join(lines), media_type='text/plain; charset=utf-8',
                    headers={**HEADERS, 'Content-Disposition': "attachment; filename*=UTF-8''" + name})


class CloseRequest(BaseModel):
    revision:int=Field(ge=0)
    discard:Literal[True]


class CloseSessionItem(BaseModel):
    id:str
    revision:int=Field(ge=0)


class CloseSessionsRequest(BaseModel):
    sessions:list[CloseSessionItem]=Field(min_length=1,max_length=1000)
    discard:Literal[True]


def discard_sessions(ids):
    """Stage all session removals before committing collection reference changes."""
    moved=[]; old_collections=[]
    def rename_when_released(source,destination):
        # Windows can deny a directory rename while a preview FileResponse or
        # thumbnail reader still holds one of its files. Retry the staging
        # move too, not only the later deletion. Persistent failures still
        # raise and restore every session already staged by this operation.
        for attempt in range(5):
            try:
                os.rename(source,destination)
                return
            except OSError as error:
                if getattr(error,'winerror',None) not in (5,32,33) or attempt==4:
                    raise
                time.sleep(0.1*(2**attempt))
    try:
        for sid in ids:
            source=folder(sid).resolve()
            if source.parent!=SESSIONS.resolve():
                raise ValueError('Invalid session location.')
            destination=(ROOT/('.closing-'+uuid.uuid4().hex)).resolve()
            if destination.parent!=ROOT.resolve():
                raise ValueError('Invalid session cleanup location.')
            rename_when_released(source,destination)
            moved.append((source,destination))
        for path in COLLECTIONS.glob('*.json'):
            try:
                data=read_collection(path.stem)
            except HTTPException:
                continue
            changed=False
            for entry in data['entries']:
                if entry.get('session_id') in ids:
                    entry['session_id']=None; entry.pop('error',None); changed=True
            if changed:
                old_collections.append((path,path.read_bytes()))
                write_collection(data)
    except Exception:
        for path,contents in reversed(old_collections):
            path.write_bytes(contents)
        for source,destination in reversed(moved):
            rename_when_released(destination,source)
        raise
    def remove_readonly(function,path,error):
        # A source photo marked read-only may give its cached original the
        # same attribute. Only our generated quarantine tree is mutable here.
        candidate=Path(path).resolve()
        if not any(candidate==target or target in candidate.parents for _,target in moved):
            raise error
        os.chmod(candidate,candidate.stat().st_mode|stat.S_IWRITE|stat.S_IREAD)
        function(path)
    for source,destination in moved:
        # Only our newly staged session directories are recursively removed.
        for attempt in range(5):
            try:
                shutil.rmtree(destination,onexc=remove_readonly)
                break
            except OSError:
                if attempt==4:
                    raise
                # An in-flight local image response may still hold a file open.
                time.sleep(0.1*(2**attempt))
    for key in list(geometry_cache):
        if key[0] in ids:
            geometry_cache.pop(key,None)


@router.post('/api/local-remove/close-sessions')
async def close_sessions(request:Request,payload:CloseSessionsRequest):
    guard(request,True)
    ids=[item.id for item in payload.sessions]
    if len(set(ids))!=len(ids):
        raise HTTPException(400,'Each image may be closed only once.')
    for sid in ids:
        validate_id(sid,'Session')
    # Folder opening uses registration -> session locks. The same order here
    # prevents a switch/save racing the all-or-nothing revision validation.
    async with registration_lock, AsyncExitStack() as stack:
        for sid in sorted(ids):
            await stack.enter_async_context(locks.setdefault(sid,asyncio.Lock()))
        for item in payload.sessions:
            data=read_session(item.id)
            if data['revision']!=item.revision:
                raise HTTPException(409,'An image changed in another window. Review its latest layers before closing.')
        try:
            await asyncio.to_thread(discard_sessions,set(ids))
        except HTTPException:
            raise
        except Exception as error:
            raise HTTPException(400,'The images could not be closed: '+str(error)) from error
    return {'closed':ids}


@router.post('/api/local-remove/session/{sid}/close')
async def close_session(sid:str,request:Request,payload:CloseRequest):
    return await close_sessions(request,CloseSessionsRequest(sessions=[CloseSessionItem(id=sid,revision=payload.revision)],discard=True))


class CutoutRequest(BaseModel):
    revision: int = Field(ge=0)
    variant: Literal['int8', 'bf16'] = 'int8'
    prompt: str = Field(default='', max_length=4000)
    seed: int = Field(default_factory=lambda: secrets.randbits(48), ge=0, le=2**53-1)
    layer_id: str | None = None


class CutoutUpdate(BaseModel):
    revision: int = Field(ge=0)
    enabled: bool | None = None
    feather: float | None = Field(default=None, ge=0, le=40, allow_inf_nan=False)
    background: dict | None = None
    shadow: dict | None = None
    transform: dict | None = None
    layer_id: str | None = None


class CutoutRefine(BaseModel):
    revision: int = Field(ge=0)
    mask: str = Field(max_length=90 * 1024 * 1024)
    operation: Literal['restore', 'erase', 'replace']
    layer_id: str | None = None


class LibraryBackground(BaseModel):
    revision: int = Field(ge=0)
    library_id: str
    entry_id: str
    layer_id: str | None = None


class GeneratedBackground(BaseModel):
    revision: int = Field(ge=0)
    generated_session_id: str
    layer_id: str | None = None


def cutout_session(sid, revision, require=True):
    data = read_session(sid)
    if data['revision'] != revision:
        raise HTTPException(409, 'The edit changed. Reload the session before editing the cutout.')
    if require and not data.get('cutout') and not data.get('layer_stack'):
        raise HTTPException(400, 'Remove the background or apply a cutout selection first.')
    if data.get('cutout'):
        validate_cutout(data['cutout'])
    return data, folder(sid)


def save_cutout_alpha(root, alpha):
    name = 'cutout-' + uuid.uuid4().hex + '-alpha.png'
    alpha.convert('L').save(root / name)
    return name


def commit_cutout(root, data):
    validate_cutout(data['cutout'])
    previous = read_session(data['id']).get('cutout')
    if previous == data['cutout']:
        return public(data)
    data['cutout_undo'] = (data.get('cutout_undo', []) + [previous])[-20:]
    data['cutout_redo'] = []
    data['revision'] += 1
    write_session(root, data)
    return public(data)


def selection_image(encoded, size):
    with Image.open(io.BytesIO(base64.b64decode(encoded, validate=True))) as image:
        if image.width * image.height > 150_000_000 or max(image.size) > 32768:
            raise ValueError('The cutout selection is too large.')
        return image.convert('L').resize(size, Image.Resampling.LANCZOS)


def set_background_image(root, data, image, name, attribution=None, reference_attributions=None, layer_id=None):
    if image.width * image.height > 150_000_000:
        raise ValueError('The background image is too large.')
    attribution = validate_attribution(attribution) if attribution is not None else None
    reference_attributions = validate_attributions(reference_attributions or [])
    if data.get('layer_stack'):
        if layer_id: stack_layer(data, layer_id)
        return add_stack_image(root, data, image, name, below=layer_id, attribution=attribution,
                               reference_attributions=reference_attributions)
    asset = 'cutout-' + uuid.uuid4().hex + '-background.png'
    image.convert('RGBA').save(root / asset, icc_profile=SRGB.tobytes())
    previous = data['cutout']['background']
    data['cutout']['background'] = {'mode': 'image', 'color': previous.get('color', '#ffffff'), 'asset': asset, 'name': Path(name).name[:255]}
    if attribution is not None:
        data['cutout']['background']['attribution'] = attribution
    if reference_attributions:
        data['cutout']['background']['reference_attributions'] = reference_attributions
    return commit_cutout(root, data)


def decode_background(source):
    with Image.open(source) as image:
        if image.width * image.height > 150_000_000:
            raise ValueError('The background image is too large.')
        oriented = ImageOps.exif_transpose(image)
        result = oriented.convert('RGBA')
        profile = image.info.get('icc_profile')
        if profile:
            alpha = result.getchannel('A')
            result = ImageCms.profileToProfile(result.convert('RGB'), ImageCms.ImageCmsProfile(io.BytesIO(profile)), SRGB, outputMode='RGB').convert('RGBA')
            result.putalpha(alpha)
        return result


def decode_stock_image(contents):
    if not isinstance(contents, bytes) or not 0 < len(contents) <= 40 * 1024 * 1024:
        raise ValueError('Choose a stock image smaller than 40 MB.')
    with Image.open(io.BytesIO(contents)) as image:
        if image.format != 'PNG' or image.width * image.height > 40_000_000 or max(image.size) > 32768:
            raise ValueError('The stock image is too large or is not a normalized PNG.')
        image.verify()
    return decode_background(io.BytesIO(contents))


async def import_stock_image(contents, attribution, *, target='image', session_id=None, revision=None, layer_id=None):
    """Import only provider-fetched bytes; callers enforce the web write guard.

    Downloading happens before this helper. Background edits compare revisions
    under the target lock, so a slow network request never replaces newer work.
    """
    attribution = validate_attribution(attribution)
    if target not in ('image', 'background'):
        raise ValueError('Choose an image document or a cutout background.')
    if target == 'background':
        validate_id(session_id, 'Session')
        if type(revision) is not int or revision < 0:
            raise ValueError('The current document revision is required.')
    elif session_id is not None or revision is not None:
        raise ValueError('Image imports do not replace an existing document.')
    image = await asyncio.to_thread(decode_stock_image, contents)
    title = re.sub(r'[<>:"/\\|?*\x00-\x1f]', '_', attribution['title']).strip(' .')[:180] or 'Stock image'
    name = 'Stock - ' + title + '.png'
    if target == 'image':
        def create():
            with tempfile.TemporaryDirectory(prefix='stock-import-', dir=ROOT) as temporary:
                source = Path(temporary) / 'stock.png'
                image.save(source, icc_profile=SRGB.tobytes())
                result = create_session(source, name)
                data = read_session(result['id'])
                data.update(source_attribution=attribution, revision=1)
                write_session(folder(result['id']), data)
                return public(data)
        result = await asyncio.to_thread(create)
    else:
        async with locks.setdefault(session_id, asyncio.Lock()):
            data, root = cutout_session(session_id, revision, require=False)
            if not data.get('cutout') and not data.get('layer_stack'):
                # Preserve transparency already present in the source image.
                original_alpha = await asyncio.to_thread(lambda: attach_source_alpha(root, data, render(data, include_cutout=False)).convert('RGBA').getchannel('A'))
                data['cutout'] = initial_cutout(await asyncio.to_thread(save_cutout_alpha, root, original_alpha))
            if not data.get('layer_stack'): data['cutout']['enabled'] = True
            result = await asyncio.to_thread(set_background_image, root, data, image, name, attribution, layer_id=layer_id)
    return {'session': result, 'target': target, 'attribution': attribution}


@router.get('/api/local-remove/qwen/status')
async def qwen_status(request: Request, refresh: bool = False):
    guard(request)
    from qwen_image import get_qwen_status
    return await get_qwen_status(refresh=True) if refresh else await get_qwen_status()


@router.post('/api/local-remove/session/{sid}/cutout/generated-background')
async def use_generated_background(sid: str, request: Request, payload: GeneratedBackground):
    guard(request, True)
    validate_id(sid, 'session')
    validate_id(payload.generated_session_id, 'session')
    if sid == payload.generated_session_id:
        raise HTTPException(400, 'Choose a different generated image for the background.')
    # Consistent lock order lets two open documents exchange backgrounds safely.
    async with AsyncExitStack() as stack:
        for session_id in sorted({sid, payload.generated_session_id}):
            await stack.enter_async_context(locks.setdefault(session_id, asyncio.Lock()))
        data, root = cutout_session(sid, payload.revision)
        generated = read_session(payload.generated_session_id)
        image = await asyncio.to_thread(render, generated)
        if not generated.get('cutout', {}).get('enabled') and not generated.get('layer_stack'):
            image = await asyncio.to_thread(attach_source_alpha, folder(generated['id']), generated, image)
        return await asyncio.to_thread(set_background_image, root, data, image, generated['name'],
                                      generated.get('source_attribution'), collect_attributions(generated), payload.layer_id)


@router.post('/api/local-remove/session/{sid}/cutout')
async def remove_background(sid: str, request: Request, payload: CutoutRequest):
    guard(request, True)
    from main import generation_lock
    from qwen_image import run_qwen_image, qwen_canvas_size
    async with locks.setdefault(sid, asyncio.Lock()), generation_lock:
        data, root = cutout_session(sid, payload.revision, require=False)
        source = root / 'cutout-source.png'
        try:
            if data.get('layer_stack') and payload.layer_id:
                selected = stack_layer(data, payload.layer_id)
                raw, icc, _ = await asyncio.to_thread(decode_original, root / data['original'])
                pixels = await asyncio.to_thread(stack_model.native_layer, data, root, selected, raw, icc, decode_original)
                await asyncio.to_thread(lambda: stack_model.display(pixels, icc).save(source))
            else:
                await asyncio.to_thread(lambda: render(data, include_cutout=False).save(source))
            result = await run_qwen_image(source, payload.prompt, variant=payload.variant,
                                          size=qwen_canvas_size((data['width'], data['height'])), seed=payload.seed, task='cutout')
            if 'A' not in result.getbands():
                raise ValueError('Qwen returned an opaque image. Use an RGBA-capable Qwen workflow or refine a manual cutout selection.')
            alpha = result.getchannel('A').resize((data['width'], data['height']), Image.Resampling.LANCZOS)
            low, high = alpha.getextrema()
            if low >= 250 or high <= 5:
                raise ValueError('Qwen did not produce a usable transparent cutout. Check that its RGBA decoder is installed, or refine a manual selection.')
            if data.get('layer_stack'):
                return await asyncio.to_thread(add_stack_cutout, root, data, alpha, source_layer_id=payload.layer_id)
            asset = await asyncio.to_thread(save_cutout_alpha, root, alpha)
            state = data.get('cutout') or initial_cutout(asset)
            state.update(alpha=asset, enabled=True)
            data['cutout'] = state
            return commit_cutout(root, data)
        except HTTPException:
            raise
        except Exception as error:
            raise HTTPException(400, 'Background removal failed: ' + str(error)) from error


@router.patch('/api/local-remove/session/{sid}/cutout')
async def update_cutout(sid: str, request: Request, payload: CutoutUpdate):
    guard(request, True)
    async with locks.setdefault(sid, asyncio.Lock()):
        data, root = cutout_session(sid, payload.revision)
        if data.get('layer_stack'):
            layer = selected_cutout(data, payload.layer_id)
            state = layer['cutout']
            try:
                if payload.enabled is not None: layer['visible'] = payload.enabled
                if payload.feather is not None: state['feather'] = payload.feather
                if payload.shadow is not None: state['shadow'].update(payload.shadow)
                if payload.transform is not None: layer['transform'].update(payload.transform)
                if payload.background is not None:
                    raise ValueError('Add a background image as a layer from Assets.')
                return commit_stack(root, data, layer['id'])
            except (ValueError, TypeError) as error: raise HTTPException(400, str(error)) from error
        state = data['cutout']
        try:
            for field in ('enabled', 'feather'):
                value = getattr(payload, field)
                if value is not None:
                    state[field] = value
            if payload.background is not None:
                if not set(payload.background) <= {'mode', 'color'}:
                    raise ValueError('Use the background picker to choose an image.')
                state['background'].update(payload.background)
            if payload.shadow is not None:
                state['shadow'].update(payload.shadow)
            if payload.transform is not None:
                state['transform'].update(payload.transform)
            return commit_cutout(root, data)
        except (ValueError, TypeError) as error:
            raise HTTPException(400, str(error)) from error


@router.post('/api/local-remove/session/{sid}/cutout/refine')
async def refine_cutout(sid: str, request: Request, payload: CutoutRefine):
    guard(request, True)
    async with locks.setdefault(sid, asyncio.Lock()):
        data, root = cutout_session(sid, payload.revision, require=False)
        try:
            selection = await asyncio.to_thread(selection_image, payload.mask, (data['width'], data['height']))
            if not selection.getbbox():
                raise ValueError('Paint or draw a selection first.')
            if data.get('layer_stack'):
                if payload.layer_id:
                    candidate = stack_layer(data, payload.layer_id)
                    if candidate['kind'] != 'cutout':
                        if payload.operation != 'replace': raise HTTPException(400, 'Add a mask to this layer before refining it.')
                        return add_stack_cutout(root, data, selection, source_layer_id=payload.layer_id)
                    layer = selected_cutout(data, payload.layer_id)
                    with Image.open(root / layer['cutout']['alpha']) as existing_alpha: alpha = existing_alpha.convert('L')
                    selection = inverse_selection(selection, layer['transform'])
                    if not selection.getbbox(): raise ValueError('The selection lies outside this layer.')
                    alpha = refine_alpha(alpha, selection, payload.operation)
                    layer['cutout']['alpha'] = save_cutout_alpha(root, alpha)
                    return commit_stack(root, data, layer['id'])
                # Manual selection can create the first cutout without a model.
                alpha = refine_alpha(Image.new('L', selection.size, 255), selection, payload.operation)
                return add_stack_cutout(root, data, alpha)
            existing = data.get('cutout', {}).get('enabled')
            if existing:
                with Image.open(root / data['cutout']['alpha']) as image:
                    alpha = image.convert('L')
                selection = await asyncio.to_thread(inverse_selection, selection, data['cutout']['transform'])
                if not selection.getbbox():
                    raise ValueError('The selection lies outside the transformed subject image. Select an area on the subject to refine.')
            else:
                alpha = Image.new('L', selection.size, 255)
            alpha = await asyncio.to_thread(refine_alpha, alpha, selection, payload.operation)
            asset = await asyncio.to_thread(save_cutout_alpha, root, alpha)
            state = data.get('cutout') or initial_cutout(asset)
            state.update(alpha=asset, enabled=True)
            if not existing:
                state['transform'] = dict(DEFAULT_TRANSFORM)
            data['cutout'] = state
            return commit_cutout(root, data)
        except HTTPException:
            raise
        except Exception as error:
            raise HTTPException(400, 'The cutout could not be refined: ' + str(error)) from error


@router.post('/api/local-remove/session/{sid}/cutout/background')
async def upload_background(sid: str, request: Request, revision: int = Form(...), file: UploadFile = File(...), layer_id: str | None = Form(default=None)):
    guard(request, True)
    async with locks.setdefault(sid, asyncio.Lock()):
        data, root = cutout_session(sid, revision)
        try:
            contents = await file.read(64 * 1024 * 1024 + 1)
            if len(contents) > 64 * 1024 * 1024:
                raise ValueError('Choose a background image smaller than 64 MB.')
            image = await asyncio.to_thread(decode_background, io.BytesIO(contents))
            return await asyncio.to_thread(set_background_image, root, data, image, file.filename or 'Background.png', layer_id=layer_id)
        except Exception as error:
            raise HTTPException(400, 'The background could not be imported: ' + str(error)) from error


@router.post('/api/local-remove/session/{sid}/cutout/generate-background')
async def generate_background(sid: str, request: Request, payload: CutoutRequest):
    guard(request, True)
    from main import generation_lock
    from qwen_image import run_qwen_image, qwen_canvas_size
    async with locks.setdefault(sid, asyncio.Lock()), generation_lock:
        data, root = cutout_session(sid, payload.revision)
        if not payload.prompt.strip():
            raise HTTPException(400, 'Describe the empty background to generate.')
        try:
            image = await run_qwen_image(None, payload.prompt, variant=payload.variant,
                                         size=qwen_canvas_size((data['width'], data['height'])), seed=payload.seed, task='background')
            return await asyncio.to_thread(set_background_image, root, data, image, 'Generated background.png', layer_id=payload.layer_id)
        except Exception as error:
            raise HTTPException(400, 'Background generation failed: ' + str(error)) from error


def read_background_library(library_id):
    validate_id(library_id, 'Background library')
    try:
        data = json.loads((BACKGROUNDS / (library_id + '.json')).read_text(encoding='utf-8'))
        if data.get('id') != library_id or not isinstance(data.get('entries'), list):
            raise ValueError('Invalid library')
        return data
    except (OSError, ValueError, AttributeError):
        raise HTTPException(404, 'Background library not found')


def public_background_library(data):
    return {'id': data['id'], 'name': data['name'], 'entries': [
        {'id': item['id'], 'name': item['name'],
         'thumbnail': f'/api/local-remove/backgrounds/{data["id"]}/{item["id"]}/thumbnail'} for item in data['entries']]}


def background_library_entry(library_id, entry_id):
    validate_id(entry_id, 'Background')
    data = read_background_library(library_id)
    entry = next((item for item in data['entries'] if item['id'] == entry_id), None)
    if entry is None:
        raise HTTPException(404, 'Background not found')
    source = Path(entry['path'])
    if not source.is_file() or source.suffix.lower() not in SUPPORTED:
        raise HTTPException(404, 'This background is unavailable. Choose its folder again.')
    return source


@router.get('/api/local-remove/backgrounds')
async def background_libraries(request: Request):
    guard(request)
    libraries = []
    for path in sorted(BACKGROUNDS.glob('*.json')):
        try:
            libraries.append(public_background_library(read_background_library(path.stem)))
        except HTTPException:
            continue
    return {'libraries': libraries}


@router.post('/api/local-remove/backgrounds/register-folder')
async def register_background_folder(request: Request, payload: OpenLocal):
    launcher_guard(request)
    source = Path(payload.path).expanduser().resolve()
    if not source.is_dir():
        raise HTTPException(400, 'Choose an existing backgrounds folder.')
    async with registration_lock:
        files = await asyncio.to_thread(lambda: sorted((p for p in source.iterdir() if p.is_file() and not p.is_symlink() and p.suffix.lower() in SUPPORTED), key=natural_name))
        if not files:
            raise HTTPException(400, 'This folder has no supported background images.')
        if len(files) > 1000:
            raise HTTPException(400, 'Choose a backgrounds folder with at most 1,000 images.')
        library_id = str(uuid.uuid5(uuid.NAMESPACE_URL, 'local-remove-background:' + path_key(source)))
        data = {'id': library_id, 'name': source.name, 'entries': [
            {'id': str(uuid.uuid5(uuid.UUID(library_id), path_key(path))), 'name': path.name, 'path': str(path)} for path in files]}
        temporary = BACKGROUNDS / (uuid.uuid4().hex + '.tmp')
        try:
            temporary.write_text(json.dumps(data), encoding='utf-8')
            os.replace(temporary, BACKGROUNDS / (library_id + '.json'))
        finally:
            temporary.unlink(missing_ok=True)
        return public_background_library(data)


@router.get('/api/local-remove/backgrounds/{library_id}/{entry_id}/thumbnail')
async def background_thumbnail(library_id: str, entry_id: str, request: Request):
    guard(request)
    source = background_library_entry(library_id, entry_id)
    async with thumbnail_gate:
        try:
            stamp = source.stat().st_mtime_ns
            target = BACKGROUNDS / (library_id + '-' + entry_id + '-' + str(stamp) + '.png')
            if not target.is_file():
                def make_thumbnail():
                    image = decode_background(source)
                    image.thumbnail((240, 160), Image.Resampling.LANCZOS)
                    image.save(target)
                await asyncio.to_thread(make_thumbnail)
            return FileResponse(target, media_type='image/png', headers=HEADERS)
        except Exception as error:
            raise HTTPException(400, 'This background preview is unavailable.') from error


@router.post('/api/local-remove/session/{sid}/cutout/library-background')
async def apply_library_background(sid: str, request: Request, payload: LibraryBackground):
    guard(request, True)
    async with locks.setdefault(sid, asyncio.Lock()):
        data, root = cutout_session(sid, payload.revision)
        source = background_library_entry(payload.library_id, payload.entry_id)
        try:
            image = await asyncio.to_thread(decode_background, source)
            return await asyncio.to_thread(set_background_image, root, data, image, source.name, layer_id=payload.layer_id)
        except Exception as error:
            raise HTTPException(400, 'This background could not be opened: ' + str(error)) from error


async def change_cutout_history(sid, request, revision, direction):
    guard(request, True)
    async with locks.setdefault(sid, asyncio.Lock()):
        data, root = cutout_session(sid, revision, require=False)
        source = 'cutout_' + direction
        destination = 'cutout_redo' if direction == 'undo' else 'cutout_undo'
        history = data.get(source, [])
        if not history:
            raise HTTPException(400, 'There is no cutout change to ' + direction + '.')
        previous = history.pop()
        if previous is not None:
            validate_cutout(previous)
        data[destination] = (data.get(destination, []) + [data.get('cutout')])[-20:]
        if previous is None:
            data.pop('cutout', None)
        else:
            data['cutout'] = previous
        data['revision'] += 1
        write_session(root, data)
        return public(data)


@router.post('/api/local-remove/session/{sid}/cutout/undo')
async def undo_cutout(sid: str, request: Request, payload: MergeRequest):
    guard(request, True)
    if read_session(sid).get('layer_stack'):
        return await change_stack_history(sid, request, payload.revision, 'undo')
    return await change_cutout_history(sid, request, payload.revision, 'undo')


@router.post('/api/local-remove/session/{sid}/cutout/redo')
async def redo_cutout(sid: str, request: Request, payload: MergeRequest):
    guard(request, True)
    if read_session(sid).get('layer_stack'):
        return await change_stack_history(sid, request, payload.revision, 'redo')
    return await change_cutout_history(sid, request, payload.revision, 'redo')


@router.get('/api/local-remove/session/{sid}/cutout/foreground')
async def cutout_foreground(sid: str, request: Request, full: bool = True, layer_id: str | None = None):
    """Original-position foreground for responsive client-side drag previews."""
    guard(request)
    data = read_session(sid)
    if data.get('layer_stack'):
        layer = selected_cutout(data, layer_id, writable=False)
        return await stack_layer_display(sid, layer['id'], request)
    if not data.get('cutout'):
        raise HTTPException(404, 'No cutout is available.')
    root = folder(sid); state = validate_cutout(data['cutout'])
    target = root / ('cutout-foreground-' + str(data['revision']) + ('-full' if full else '') + '.png')
    if not target.is_file():
        def make_foreground():
            image = attach_source_alpha(root, data, render(data, include_cutout=False))
            with Image.open(root / state['alpha']) as alpha:
                image = compose_image(image, alpha, None, DEFAULT_SHADOW, state['feather'])
            if not full:
                image.thumbnail((3000, 3000), Image.Resampling.LANCZOS)
            image.save(target, icc_profile=SRGB.tobytes())
        await asyncio.to_thread(make_foreground)
    return FileResponse(target, media_type='image/png', headers=HEADERS)


@router.get('/api/local-remove/session/{sid}/cutout/background-preview')
async def cutout_background_preview(sid: str, request: Request, full: bool = True, layer_id: str | None = None):
    guard(request)
    data = read_session(sid)
    if data.get('layer_stack'):
        layer = selected_cutout(data, layer_id, writable=False)
        root = folder(sid)
        target = root / ('stack-underlay-' + layer['id'] + '-' + str(data['revision']) + '.png')
        if not target.is_file():
            def make_underlay():
                below = copy.deepcopy(data)
                index = next(i for i, item in enumerate(below['layer_stack']) if item['id'] == layer['id'])
                for item in below['layer_stack'][index:]: item['visible'] = False
                render(below).save(target, icc_profile=SRGB.tobytes())
            await asyncio.to_thread(make_underlay)
        return FileResponse(target, media_type='image/png', headers=HEADERS)
    if not data.get('cutout'):
        raise HTTPException(404, 'No cutout is available.')
    root = folder(sid); state = validate_cutout(data['cutout'])
    target = root / ('cutout-background-preview-' + str(data['revision']) + ('-full' if full else '') + '.png')
    if not target.is_file():
        def make_background():
            size = (data['width'], data['height'])
            image = underlay(size, Image.new('L', size), cutout_background(root, state), DEFAULT_SHADOW)
            if not full:
                image.thumbnail((3000, 3000), Image.Resampling.LANCZOS)
            image.save(target, icc_profile=SRGB.tobytes())
        await asyncio.to_thread(make_background)
    return FileResponse(target, media_type='image/png', headers=HEADERS)
