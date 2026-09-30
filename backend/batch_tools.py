"""Reusable product treatments and a durable, sequential, review-before-export queue.

The queue owns immutable snapshots, not the editor's recovery documents. A saved
treatment owns its backdrop but never another product's foreground or alpha mask.
"""
import asyncio
from copy import deepcopy
import json
import math
import os
from pathlib import Path
import re
import shutil
import stat
import time
from typing import Literal
import uuid
import zipfile

from fastapi import APIRouter, HTTPException, Request
from fastapi.responses import FileResponse
from PIL import Image
from pydantic import BaseModel, ConfigDict, Field, model_validator

import local_remove as editor
from cutout_composite import initial_cutout, validate_cutout, transform_matrix
from stock_attribution import collect_attributions

router = APIRouter(prefix='/api/local-remove/batch')
MAX_ITEMS = 100
MAX_STORAGE = 12 * 1024**3
tasks = {}
queue_lock = asyncio.Lock()


def linked(path):
    try:
        return path.is_symlink() or bool(getattr(path.lstat(), 'st_file_attributes', 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT)
    except FileNotFoundError:
        return False


def own_root(kind):
    path = editor.ROOT / 'batch-tools' / kind
    for parent in (path.parent, path):
        if linked(parent):
            raise HTTPException(400, 'Batch storage must be a regular local folder.')
    path.mkdir(parents=True, exist_ok=True)
    return path


def own_directory(kind, identifier, required=True):
    editor.validate_id(identifier, 'Batch item')
    root = own_root(kind)
    path = root / identifier
    if linked(path) or path.resolve().parent != root.resolve():
        raise HTTPException(400, 'The batch item is not a regular local folder.')
    if required and not path.is_dir():
        raise HTTPException(404, 'Batch item not found.')
    return path


def atomic_json(path, value):
    tmp = path.with_name('.' + uuid.uuid4().hex + '.tmp')
    try:
        tmp.write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding='utf-8')
        os.replace(tmp, path)
    finally:
        tmp.unlink(missing_ok=True)


def load_json(path):
    if linked(path) or not path.is_file() or path.stat().st_size > 1024 * 1024:
        raise HTTPException(404, 'Batch metadata is missing or invalid.')
    try:
        value = json.loads(path.read_text(encoding='utf-8'))
        if not isinstance(value, dict):
            raise ValueError()
        return value
    except (OSError, ValueError):
        raise HTTPException(404, 'Batch metadata is missing or invalid.') from None


def bytes_used(path):
    total = 0
    for directory, dirs, files in os.walk(path, followlinks=False):
        base = Path(directory)
        if linked(base) or any(linked(base / name) for name in dirs + files):
            raise HTTPException(400, 'Batch storage contains a linked file or folder.')
        total += sum((base / name).stat().st_size for name in files)
    return total


def treatment(identifier):
    directory = own_directory('treatments', identifier)
    value = load_json(directory / 'treatment.json')
    if value.get('id') != identifier or value.get('version') != 1 or value.get('format') not in ('original', 'png', 'jpg', 'tif', 'webp'):
        raise HTTPException(400, 'The saved treatment is invalid.')
    return value


def job(identifier):
    value = load_json(own_directory('jobs', identifier) / 'job.json')
    if value.get('id') != identifier or not isinstance(value.get('items'), list) or len(value['items']) > MAX_ITEMS:
        raise HTTPException(400, 'The batch queue is invalid.')
    # A persisted running flag alone is not a live process. On restart the same
    # queue remains available for explicit resume; completed files are retained.
    if value.get('running') and identifier not in tasks:
        value.update(running=False, phase='paused', message='Interrupted. Review the queue, then resume.')
        for item in value['items']:
            if item['status'] == 'preparing':
                item['status'] = 'pending'
            elif item['status'] == 'exporting':
                item['status'] = 'ready'
        save_job(value)
    return value


def save_job(value):
    value['modified'] = time.time()
    atomic_json(own_directory('jobs', value['id']) / 'job.json', value)


def public_job(value):
    result = {key: deepcopy(value.get(key)) for key in ('id', 'name', 'created', 'modified', 'phase', 'running', 'message', 'format', 'mode', 'prepare_cutouts', 'treatment_name')}
    result['treatment_id'] = value.get('treatment', {}).get('id') if value.get('treatment') else None
    result['qwen_variant'] = value.get('qwen_variant', 'int8')
    result['items'] = [{key: item.get(key) for key in ('id', 'name', 'session_id', 'revision', 'status', 'error', 'output_name', 'credits_name', 'export_bit_depth')}
                       | {'preview': f'/api/local-remove/batch/jobs/{value["id"]}/items/{item["id"]}/preview' if item.get('prepared') else None}
                       for item in value['items']]
    result['bytes'] = bytes_used(own_directory('jobs', value['id']))
    result['download'] = f'/api/local-remove/batch/jobs/{value["id"]}/download' if value.get('archive_ready') else None
    return result


class StrictModel(BaseModel):
    model_config = ConfigDict(extra='forbid')


class SaveTreatment(StrictModel):
    name: str = Field(min_length=1, max_length=80)
    session_id: str
    revision: int = Field(ge=0)
    format: editor.OutputFormat = 'original'


@router.get('/treatments')
async def list_treatments(request: Request):
    editor.guard(request)
    values = []
    for path in own_root('treatments').iterdir():
        try:
            value = treatment(path.name)
            values.append({key: value[key] for key in ('id', 'name', 'created', 'format')})
        except (HTTPException, OSError):
            continue
    return {'items': sorted(values, key=lambda v: v['name'].casefold()), 'bytes': bytes_used(own_root('treatments'))}


@router.post('/treatments')
async def save_treatment(request: Request, payload: SaveTreatment):
    editor.guard(request, True)
    name = payload.name.strip()
    if not name:
        raise HTTPException(400, 'Name this treatment.')
    if len(list(own_root('treatments').iterdir())) >= 100:
        raise HTTPException(400, 'Keep up to 100 saved treatments. Remove an unused treatment first.')
    async with editor.locks.setdefault(payload.session_id, asyncio.Lock()):
        data, source = editor.cutout_session(payload.session_id, payload.revision)
        if not data['cutout']['enabled']:
            raise HTTPException(400, 'Enable the cutout before saving a product treatment.')
        state = deepcopy(data['cutout'])
        with Image.open(source / state['alpha']) as alpha:
            bounds = alpha.convert('L').getbbox()
        if not bounds:
            raise HTTPException(400, 'This cutout has no visible subject.')
        width, height = data['width'], data['height']
        transform = state['transform']
        matrix = transform_matrix((width, height), transform)
        cx, cy = (bounds[0] + bounds[2] - 1) / 2, (bounds[1] + bounds[3] - 1) / 2
        center = (matrix[0, 0] * cx + matrix[0, 1] * cy + matrix[0, 2], matrix[1, 0] * cx + matrix[1, 1] * cy + matrix[1, 2])
        identifier = str(uuid.uuid4())
        target = own_directory('treatments', identifier, False)
        target.mkdir()
        try:
            background = state['background']
            if background.get('asset'):
                safe_copy(source, background['asset'], target)
            value = {'version': 1, 'id': identifier, 'name': name, 'created': time.time(), 'format': payload.format,
                     'background': background, 'shadow': state['shadow'], 'shadow_basis': min(width, height),
                     'feather_fraction': state['feather'] / min(width, height),
                     'placement': {'center_x': center[0] / width, 'center_y': center[1] / height,
                                   'width': (bounds[2] - bounds[0]) * transform['scale'] / width,
                                   'height': (bounds[3] - bounds[1]) * transform['scale'] / height, 'rotation': transform['rotation']}}
            atomic_json(target / 'treatment.json', value)
            return {key: value[key] for key in ('id', 'name', 'created', 'format')}
        except Exception:
            remove_owned(target)
            raise


def remove_owned(path):
    expected = own_root('treatments')
    if path.resolve().parent != expected.resolve() or linked(path):
        raise HTTPException(400, 'Unsafe treatment storage. No files removed.')
    files = list(path.iterdir())
    if any(linked(file) or not file.is_file() or file.resolve().parent != path.resolve()
           or not (file.name == 'treatment.json' or re.fullmatch(r'cutout-[0-9a-f]{32}-background\.png', file.name)) for file in files):
        raise HTTPException(400, 'Unexpected treatment contents. No files removed.')
    for file in files:
        file.unlink()
    path.rmdir()


@router.delete('/treatments/{identifier}')
async def delete_treatment(identifier: str, request: Request):
    editor.guard(request, True)
    treatment(identifier)
    remove_owned(own_directory('treatments', identifier))
    return {'deleted': True}


def safe_copy(source, name, target):
    if not isinstance(name, str) or not name or Path(name).name != name or any(char in name for char in '\\/:\x00'):
        raise ValueError('A batch asset name is invalid.')
    file = source / name
    if linked(source) or linked(file) or not file.is_file() or file.resolve().parent != source.resolve():
        raise ValueError('A batch asset is missing or linked.')
    if file.stat().st_size > 2 * 1024**3:
        raise ValueError('A batch asset is too large.')
    shutil.copy2(file, target / name)


class SessionSelection(StrictModel):
    session_id: str
    revision: int = Field(ge=0)


class CreateJob(StrictModel):
    collection_id: str | None = None
    entry_ids: list[str] = Field(default_factory=list, max_length=MAX_ITEMS)
    sessions: list[SessionSelection] = Field(default_factory=list, max_length=MAX_ITEMS)
    treatment_id: str | None = None
    prepare_cutouts: bool = False
    qwen_variant: Literal['int8', 'bf16'] = 'int8'
    format: editor.OutputFormat | None = None

    @model_validator(mode='after')
    def selection(self):
        if bool(self.collection_id and self.entry_ids) == bool(self.sessions):
            raise ValueError('Choose collection entries or opened image sessions.')
        if len(set(self.entry_ids)) != len(self.entry_ids) or len({i.session_id for i in self.sessions}) != len(self.sessions):
            raise ValueError('Choose each image once.')
        return self


def require_idle():
    if active_queue():
        raise HTTPException(409, 'Finish or cancel the active batch before starting another.')


def active_queue():
    """Only live tasks count, including CPU exports without a generation lock."""
    return any(not task.done() for task in tasks.values())


async def resolve_entries(payload):
    if payload.sessions:
        entries = []
        for selected in payload.sessions:
            data = editor.read_session(selected.session_id)
            entries.append({'id': str(uuid.uuid4()), 'name': data['name'], 'session_id': selected.session_id, 'revision': selected.revision, 'status': 'pending'})
        return entries, 'Selected photos'
    async with editor.registration_lock, editor.collection_locks.setdefault(payload.collection_id, asyncio.Lock()):
        collection = editor.read_collection(payload.collection_id)
        known = {entry['id']: entry for entry in collection['entries']}
        if any(identifier not in known for identifier in payload.entry_ids):
            raise HTTPException(400, 'Select images from this collection.')
        entries = []
        for identifier in payload.entry_ids:
            entry = known[identifier]
            try:
                if entry.get('session_id'):
                    data = editor.read_session(entry['session_id'])
                else:
                    async with editor.source_gate(Path(entry['path'])):
                        data = await asyncio.to_thread(editor.reuse_or_create_session, Path(entry['path']))
                    entry['session_id'] = data['id']
                entries.append({'id': identifier, 'name': entry['name'], 'session_id': data['id'], 'revision': data['revision'], 'status': 'pending'})
            except (HTTPException, OSError, ValueError) as error:
                entries.append({'id': identifier, 'name': entry['name'], 'status': 'failed', 'error': str(getattr(error, 'detail', error))})
        editor.write_collection(collection)
        return entries, collection['name']


@router.post('/jobs')
async def create_job(request: Request, payload: CreateJob):
    editor.guard(request, True)
    async with queue_lock:
        require_idle()
        preset = treatment(payload.treatment_id) if payload.treatment_id else None
        entries, name = await resolve_entries(payload)
        identifier = str(uuid.uuid4())
        target = own_directory('jobs', identifier, False)
        target.mkdir()
        if preset:
            target.joinpath('treatment').mkdir()
            asset = preset['background'].get('asset')
            if asset:
                safe_copy(own_directory('treatments', preset['id']), asset, target / 'treatment')
        value = {'version': 1, 'id': identifier, 'name': name, 'created': time.time(), 'items': entries,
                 'treatment': deepcopy(preset), 'treatment_name': preset['name'] if preset else None,
                 'format': payload.format or (preset['format'] if preset else 'original'),
                 'prepare_cutouts': payload.prepare_cutouts, 'qwen_variant': payload.qwen_variant,
                 'phase': 'preparing', 'mode': 'prepare', 'running': True, 'cancel_requested': False,
                 'message': 'Preparing one image at a time.'}
        save_job(value)
        launch(identifier, 'prepare')
        return public_job(value)


def launch(identifier, mode):
    task = asyncio.create_task(run_queue(identifier, mode))
    tasks[identifier] = task
    task.add_done_callback(lambda completed: tasks.pop(identifier, None) if tasks.get(identifier) is completed else None)


def apply_preset(data, directory, preset, preset_directory):
    if not preset:
        return
    state = deepcopy(data['cutout'])
    target_layer = None
    if data.get('layer_stack'):
        target_layer = next((node for node in reversed(data['layer_stack'])
                             if node['kind'] == 'cutout' and not node['discarded']
                             and node['cutout']['alpha'] == state['alpha']), None)
        if target_layer is None:
            raise ValueError('The reviewed cutout no longer matches an image layer. Prepare this cutout in the editor before applying a treatment.')
    width, height = data['width'], data['height']
    with Image.open(directory / state['alpha']) as alpha:
        bounds = alpha.convert('L').getbbox()
    if not bounds:
        raise ValueError('This image has no visible cutout subject.')
    placement = preset['placement']
    scale = min(placement['width'] * width / (bounds[2] - bounds[0]), placement['height'] * height / (bounds[3] - bounds[1]))
    if not math.isfinite(scale) or not 0.05 <= scale <= 4:
        raise ValueError('The saved placement needs a scale beyond this image’s transform range. Adjust this subject individually.')
    transform = {'scale': scale, 'rotation': placement['rotation'], 'offset_x': 0, 'offset_y': 0}
    matrix = transform_matrix((width, height), transform)
    cx, cy = (bounds[0] + bounds[2] - 1) / 2, (bounds[1] + bounds[3] - 1) / 2
    transform['offset_x'] = float(placement['center_x'] * width - (matrix[0, 0] * cx + matrix[0, 1] * cy + matrix[0, 2]))
    transform['offset_y'] = float(placement['center_y'] * height - (matrix[1, 0] * cx + matrix[1, 1] * cy + matrix[1, 2]))
    shadow = deepcopy(preset['shadow'])
    factor = min(width, height) / preset['shadow_basis']
    for key, limit in (('blur', 100), ('offset_x', 1000), ('offset_y', 1000)):
        shadow[key] *= factor
        if abs(shadow[key]) > limit:
            raise ValueError('The saved shadow exceeds this image’s controls. Adjust this subject individually.')
    state.update(enabled=True, transform=transform, shadow=shadow, feather=min(40, preset['feather_fraction'] * min(width, height)), background=deepcopy(preset['background']))
    if state['background'].get('asset'):
        safe_copy(preset_directory, state['background']['asset'], directory)
    data['cutout'] = validate_cutout(state)
    if target_layer is not None:
        # The compatibility mirror does not drive modern stack rendering.
        # Apply only the declared subject treatment to its matching mask node;
        # do not guess which other image layers are backgrounds or regroup them.
        target_layer['transform'] = deepcopy(state['transform'])
        target_layer['cutout']['feather'] = state['feather']
        target_layer['cutout']['shadow'] = deepcopy(state['shadow'])
        background = state['background']
        if background['mode'] != 'transparent':
            pixels = (Image.new('RGBA', (width, height), background['color'])
                      if background['mode'] == 'color' else editor.cutout_background(directory, state))
            editor.add_stack_image(directory, data, pixels, background.get('name') or 'Treatment background',
                                   below=target_layer['id'], commit=False,
                                   attribution=background.get('attribution'),
                                   reference_attributions=background.get('reference_attributions'))
        editor.stack_model.validate_stack(data['layer_stack'], data['layers'])


async def prepare_item(value, item):
    identifier, sid = value['id'], item['session_id']
    destination = own_directory('jobs', identifier) / item['id']
    editor.validate_id(item['id'], 'Batch image')
    destination.mkdir(exist_ok=True)
    if linked(destination):
        raise ValueError('The batch image folder is linked.')
    async with editor.locks.setdefault(sid, asyncio.Lock()):
        data = editor.read_session(sid)
        if data['revision'] != item['revision']:
            raise HTTPException(409, 'This image changed after the batch was selected. Create a new queue using its latest edits.')
        source = editor.folder(sid)
        data = deepcopy(data)
        assets = {data['original'], 'base.png'}
        for layer in data['layers']:
            assets.update(layer[key] for key in ('color', 'mask', 'snapshot') if key in layer)
        if data.get('layer_stack'):
            # Stack sources and each mask are independent immutable assets. A
            # detached queue must retain even hidden/restorable nodes, rather
            # than relying on the editor recovery folder while rendering.
            assets.update(editor.stack_model.assets(data['layer_stack']))
        if data.get('cutout'):
            state = validate_cutout(data['cutout'])
            assets.add(state['alpha'])
            if state['background'].get('asset'):
                assets.add(state['background']['asset'])
        estimate = sum((source / name).stat().st_size for name in assets)
        if bytes_used(own_directory('jobs', identifier)) + estimate > MAX_STORAGE:
            raise ValueError('This queue exceeds its 12 GB storage limit. Use a smaller batch.')
        await asyncio.to_thread(lambda: [safe_copy(source, name, destination) for name in assets])
    if value.get('treatment') and not data.get('cutout', {}).get('enabled'):
        if not value['prepare_cutouts']:
            item.update(status='needs-cutout', error='Prepare a cutout for this image, or enable Qwen preparation for a new queue.')
            return
        if any(node['kind'] == 'cutout' and not node['discarded'] for node in data.get('layer_stack', [])):
            raise ValueError('This image already has a disabled cutout layer. Choose and prepare the intended cutout in the editor before applying a treatment.')
        from main import generation_lock
        from qwen_image import run_qwen_image, qwen_canvas_size
        async with generation_lock:
            if job(identifier).get('cancel_requested'):
                item['status'] = 'pending'
                return
            image = await asyncio.to_thread(editor.render, data, include_cutout=False, root_override=destination)
            source = destination / 'batch-source.png'
            image.save(source)
            result = await run_qwen_image(source, 'Isolate the main product. Preserve its details and make the background fully transparent.',
                                          variant=value['qwen_variant'], size=qwen_canvas_size((data['width'], data['height'])), task='cutout')
            if 'A' not in result.getbands():
                raise ValueError('Qwen returned an opaque image. Refine this product manually.')
            alpha = result.getchannel('A').resize((data['width'], data['height']), Image.Resampling.LANCZOS)
            low, high = alpha.getextrema()
            if low >= 250 or high <= 5:
                raise ValueError('Qwen did not produce a usable cutout. Refine this product manually.')
            asset = editor.save_cutout_alpha(destination, alpha)
            data['cutout'] = initial_cutout(asset)
            if data.get('layer_stack'):
                # Qwen contributes alpha only. Preserve the applied native
                # source/repair pixels in an ordinary detached cutout layer.
                # No editor commit or revision change occurs in this queue.
                pixels = await asyncio.to_thread(editor.native_pixels, data, destination)
                source_asset = await asyncio.to_thread(editor.stack_snapshot, destination, data, pixels)
                node = editor.stack_model.node('cutout', 'Batch cutout', source=source_asset,
                                               cutout=deepcopy(data['cutout']))
                credits = collect_attributions(data)
                if credits:
                    node['reference_attributions'] = credits
                for existing in data['layer_stack']:
                    if existing['visible'] and not existing['discarded']:
                        existing['visible'] = False
                data['layer_stack'].append(node)
    apply_preset(data, destination, value.get('treatment'), own_directory('jobs', identifier) / 'treatment')
    if bytes_used(own_directory('jobs', identifier)) > MAX_STORAGE:
        raise ValueError('This queue exceeds its 12 GB storage limit. Use a smaller batch.')
    # Paths granting save/overwrite authority and histories do not belong to a
    # detached export snapshot. Provenance and credits remain in its metadata.
    for key in ('source_path', 'project_path', 'last_saved_path', 'cutout_undo', 'cutout_redo'):
        data.pop(key, None)
    atomic_json(destination / 'snapshot.json', data)
    preview = await asyncio.to_thread(editor.render, data, root_override=destination)
    preview.thumbnail((600, 400), Image.Resampling.LANCZOS)
    preview.save(destination / 'preview.png')
    item.update(status='ready', prepared=True, error=None)


def unique_export(temporary, destination, stem, suffix, before_publish=None):
    # Atomic no-replace publication; Windows rename is the fallback for volumes
    # without hard links. Never use os.replace against a customer's output.
    for number in range(1, 100000):
        name = stem + ('-' + str(number) if number > 1 else '') + suffix
        target = destination / name
        if target.exists():
            continue
        if before_publish is not None:
            before_publish(target)
        try:
            os.link(temporary, target)
        except FileExistsError:
            continue
        except OSError:
            if os.name != 'nt':
                raise
            try:
                os.rename(temporary, target)
            except FileExistsError:
                continue
            return target
        temporary.unlink()
        return target
    raise ValueError('Too many exports use this filename. Choose another folder.')


async def export_item(value, item):
    source = own_directory('jobs', value['id']) / item['id']
    if linked(source):
        raise ValueError('The batch snapshot folder is linked.')
    data = load_json(source / 'snapshot.json')
    sid = item['session_id']
    async with editor.locks.setdefault(sid, asyncio.Lock()):
        try:
            latest_revision = editor.read_session(sid)['revision']
        except HTTPException as error:
            if error.status_code != 404:
                raise
            # Closing an editor document clears its recovery session, not the
            # reviewed immutable queue snapshot. There is no newer document to
            # overwrite and folder exports still never touch the source file.
            latest_revision = item['revision']
        if latest_revision != item['revision']:
            raise HTTPException(409, 'This image changed after its preview. Create a new queue before exporting it.')
        suffix = Path(data['original']).suffix if value['format'] == 'original' else editor.FORMAT_SUFFIX[value['format']]
        destination = Path(value['output_directory']) if value['mode'] == 'folder' else own_directory('jobs', value['id']) / 'exports'
        destination.mkdir(exist_ok=True)
        if linked(destination):
            raise ValueError('Choose a regular export folder.')
        temporary = destination / ('.local-image-batch-' + uuid.uuid4().hex + suffix)
        try:
            if bytes_used(own_directory('jobs', value['id'])) + sum(file.stat().st_size for file in source.iterdir() if file.is_file()) > MAX_STORAGE:
                raise ValueError('This queue exceeds its 12 GB export storage limit. Use a smaller batch.')
            await asyncio.to_thread(editor.flatten, data, temporary, value['format'] != 'original', root_override=source)
            if job(value['id']).get('cancel_requested'):
                item['status'] = 'ready'
                return
            stem = re.sub(r'[<>:"/\\|?*\x00-\x1f]', '_', Path(item['name']).stem).strip(' .')[:160] or 'Image'
            # Journal before no-replace publication. If Windows/app closes just
            # after publishing a file, resume recognizes these exact bytes and
            # keeps one output rather than creating an unnecessary duplicate.
            journal_path = source / 'export-journal.json'
            output = None
            if journal_path.exists():
                recorded = load_json(journal_path)
                name = recorded.get('name')
                if (isinstance(name, str) and Path(name).name == name and not any(char in name for char in '\\/:\x00')
                        and recorded.get('directory') == str(destination.resolve())):
                    candidate = destination / name
                    if candidate.is_file() and not linked(candidate) and editor.file_hash(candidate) == recorded.get('sha256'):
                        output = candidate
            if output is None:
                digest = await asyncio.to_thread(editor.file_hash, temporary)
                def record(candidate):
                    atomic_json(journal_path, {'directory': str(destination.resolve()), 'name': candidate.name, 'sha256': digest})
                output = await asyncio.to_thread(unique_export, temporary, destination, stem + '-local-image', suffix, record)
            item.update(status='exported', output_name=output.name, error=None,
                        export_bit_depth=16 if data['bit_depth'] == 16 and suffix.lower() in ('.tif', '.tiff') else 8)
            credits = collect_attributions(data)
            if credits:
                credit_temp = destination / ('.local-image-batch-' + uuid.uuid4().hex + '.txt')
                try:
                    credit_temp.write_text('\n\n'.join(credit['attribution'] + '\nSource: ' + credit['source_url'] + '\nLicense: ' + credit['license_url'] for credit in credits), encoding='utf-8')
                    existing = destination / (output.stem + '-credits.txt')
                    credit_output = existing if existing.is_file() and not linked(existing) and editor.file_hash(existing) == editor.file_hash(credit_temp) else unique_export(credit_temp, destination, output.stem + '-credits', '.txt')
                    item.update(credits_name=credit_output.name, credits=credits)
                finally:
                    credit_temp.unlink(missing_ok=True)
        finally:
            temporary.unlink(missing_ok=True)


async def run_queue(identifier, mode):
    try:
        value = job(identifier)
        for index in range(len(value['items'])):
            current = job(identifier)
            if current.get('cancel_requested'):
                break
            item = current['items'][index]
            eligible = item['status'] == 'pending' if mode == 'prepare' else item['status'] == 'ready' and item['id'] in current.get('export_item_ids', [])
            if not eligible:
                continue
            item['status'] = 'preparing' if mode == 'prepare' else 'exporting'
            current['message'] = ('Preparing' if mode == 'prepare' else 'Exporting') + f' {index + 1} of {len(current["items"])}: {item["name"]}'
            save_job(current)
            try:
                if mode == 'prepare':
                    await prepare_item(current, item)
                else:
                    await export_item(current, item)
            except Exception as error:
                item.update(status='conflict' if isinstance(error, HTTPException) and error.status_code == 409 else 'failed', error=str(getattr(error, 'detail', error)))
            # A cancel request may have been persisted during an awaited image.
            latest = job(identifier)
            latest['items'][index] = item
            save_job(latest)
            await asyncio.sleep(0)
        value = job(identifier)
        cancelled = value.get('cancel_requested')
        if mode == 'export' and value['mode'] == 'zip' and any(item['status'] == 'exported' for item in value['items']):
            directory = own_directory('jobs', identifier)
            exported_size = sum((directory / 'exports' / item['output_name']).stat().st_size for item in value['items'] if item['status'] == 'exported')
            if bytes_used(directory) + exported_size > MAX_STORAGE:
                raise ValueError('The ZIP would exceed this queue’s 12 GB limit. Download smaller batches or export to a folder.')
            temporary = directory / 'exports.pending.zip'
            with zipfile.ZipFile(temporary, 'w', zipfile.ZIP_STORED, allowZip64=True) as archive:
                for item in value['items']:
                    if item['status'] == 'exported':
                        archive.write(directory / 'exports' / item['output_name'], item['output_name'])
                        if item.get('credits_name'):
                            archive.write(directory / 'exports' / item['credits_name'], item['credits_name'])
                archive.writestr('export-report.json', json.dumps({'name': value['name'], 'treatment': value.get('treatment_name'), 'items': [{key: item.get(key) for key in ('name', 'status', 'error', 'output_name', 'credits_name', 'credits', 'export_bit_depth')} for item in value['items']]}, ensure_ascii=False, indent=2))
            os.replace(temporary, directory / 'exports.zip')
            value['archive_ready'] = True
        failures = sum(item['status'] in ('failed', 'conflict', 'needs-cutout') for item in value['items'])
        value.update(running=False, phase='paused' if cancelled else 'review' if mode == 'prepare' else 'complete',
                     message='Cancelled after the current image. Completed outputs are kept; resume continues the remaining images.' if cancelled
                     else ('Review each preview before exporting.' if mode == 'prepare' else 'Export complete.') + (f' {failures} image(s) need attention.' if failures else ''))
        save_job(value)
    except Exception as error:
        value = job(identifier)
        value.update(running=False, phase='paused', message='Batch paused: ' + str(getattr(error, 'detail', error)))
        save_job(value)


@router.get('/jobs')
async def list_jobs(request: Request):
    editor.guard(request)
    values = []
    for path in own_root('jobs').iterdir():
        try:
            values.append(public_job(job(path.name)))
        except (HTTPException, OSError, ValueError):
            continue
    return {'items': sorted(values, key=lambda value: value['created'], reverse=True), 'bytes': bytes_used(own_root('jobs'))}


@router.get('/jobs/{identifier}')
async def get_job(identifier: str, request: Request):
    editor.guard(request)
    return public_job(job(identifier))


@router.delete('/jobs/{identifier}')
async def delete_job(identifier: str, request: Request):
    editor.guard(request, True)
    async with queue_lock:
        value = job(identifier)
        if value.get('running') or identifier in tasks:
            raise HTTPException(409, 'Cancel the batch and wait for the current image before clearing its cache.')
        directory = own_directory('jobs', identifier)
        bytes_used(directory)  # Refuse linked/reparse contents before any deletion.
        if directory.resolve().parent != own_root('jobs').resolve():
            raise HTTPException(400, 'Unsafe batch cache. No files removed.')
        # Refuse unexpected contents, especially saved editable projects. Clear
        # cache only owns its documented snapshots/previews and exported copies.
        known_entries = {item['id'] for item in value['items']}
        exported = {item[key] for item in value['items'] for key in ('output_name', 'credits_name') if item.get(key)}
        for base, dirs, files in os.walk(directory):
            relative = Path(base).relative_to(directory)
            if not relative.parts:
                valid_dirs = {'treatment', 'exports'} | known_entries
                valid_files = {'job.json', 'exports.zip', 'exports.pending.zip'}
            elif len(relative.parts) == 1 and relative.parts[0] == 'exports':
                valid_dirs, valid_files = set(), exported
            elif len(relative.parts) == 1 and relative.parts[0] == 'treatment':
                valid_dirs = set()
                valid_files = {value.get('treatment', {}).get('background', {}).get('asset')} if value.get('treatment') else set()
            elif len(relative.parts) == 1 and relative.parts[0] in known_entries:
                valid_dirs = set()
                valid_files = {name for name in files if name in ('snapshot.json', 'preview.png', 'preview-full.png', 'original-full.png', 'batch-source.png', 'export-journal.json')
                               or re.fullmatch(r'original\.(png|jpg|jpeg|tif|tiff|webp)|cutout-[0-9a-f]{32}-(alpha|background)\.png|stack-[0-9a-f]{32}-source\.(png|tif)|[0-9a-f]{32}-(color|mask)\.png|[0-9a-f]{32}-snapshot\.tif|base\.png', name)}
            else:
                valid_dirs, valid_files = set(), set()
            if any(name not in valid_dirs for name in dirs) or any(name not in valid_files for name in files):
                raise HTTPException(400, 'The queue contains unexpected files. No files were removed; move saved projects out of its cache first.')
        shutil.rmtree(directory)
        return {'deleted': True, 'preserved': 'Original files, editor documents, saved projects and folder exports are kept.'}


@router.post('/jobs/{identifier}/cancel')
async def cancel_job(identifier: str, request: Request):
    editor.guard(request, True)
    value = job(identifier)
    value['cancel_requested'] = True
    save_job(value)
    return public_job(value)


@router.post('/jobs/{identifier}/resume')
async def resume_job(identifier: str, request: Request):
    editor.guard(request, True)
    async with queue_lock:
        require_idle()
        value = job(identifier)
        mode = 'prepare' if any(item['status'] == 'pending' for item in value['items']) else 'export'
        if mode == 'export' and (not any(item['status'] == 'ready' for item in value['items']) or value['mode'] == 'prepare'):
            raise HTTPException(400, 'No interrupted work remains. Review previews and choose an export destination.')
        value.update(running=True, phase='preparing' if mode == 'prepare' else 'exporting', cancel_requested=False)
        save_job(value)
        launch(identifier, mode)
        return public_job(value)


async def begin_export(identifier, *, directory=None, item_ids=None):
    async with queue_lock:
        require_idle()
        value = job(identifier)
        selected = item_ids if item_ids is not None else [item['id'] for item in value['items'] if item['status'] == 'ready']
        if len(set(selected)) != len(selected) or any(not any(item['id'] == chosen and item['status'] == 'ready' for item in value['items']) for chosen in selected):
            raise HTTPException(400, 'Choose ready images from this reviewed queue.')
        if not selected:
            raise HTTPException(400, 'Prepare and review at least one ready image first.')
        value.update(mode='folder' if directory is not None else 'zip', output_directory=str(directory) if directory else None,
                     export_item_ids=selected, running=True, phase='exporting', cancel_requested=False, message='Exporting reviewed images one at a time.')
        save_job(value)
        launch(identifier, 'export')
        return public_job(value)


class ExportSelection(StrictModel):
    item_ids: list[str] | None = Field(default=None, min_length=1, max_length=MAX_ITEMS)


@router.post('/jobs/{identifier}/export')
async def export_zip(identifier: str, request: Request, payload: ExportSelection):
    editor.guard(request, True)
    return await begin_export(identifier, item_ids=payload.item_ids)


class ExportFolder(ExportSelection):
    path: str = Field(min_length=1, max_length=32760)


@router.post('/jobs/{identifier}/export-folder')
async def export_folder(identifier: str, request: Request, payload: ExportFolder):
    editor.launcher_guard(request)
    destination = Path(payload.path)
    if not destination.is_absolute() or not destination.is_dir() or linked(destination):
        raise HTTPException(400, 'Choose an existing regular export folder.')
    if destination.resolve() == editor.ROOT.resolve() or editor.ROOT.resolve() in destination.resolve().parents:
        raise HTTPException(400, 'Choose a folder outside Local Image recovery storage.')
    return await begin_export(identifier, directory=destination.resolve(), item_ids=payload.item_ids)


@router.get('/jobs/{identifier}/items/{item_id}/preview')
async def preview_item(identifier: str, item_id: str, request: Request, full: bool = False, original: bool = False):
    editor.guard(request)
    value = job(identifier)
    editor.validate_id(item_id, 'Batch image')
    item = next((item for item in value['items'] if item['id'] == item_id), None)
    directory = own_directory('jobs', identifier) / item_id
    if linked(directory) or directory.resolve().parent != own_directory('jobs', identifier).resolve():
        raise HTTPException(400, 'The batch snapshot is not a regular local folder.')
    path = directory / ('original-full.png' if original else 'preview-full.png' if full else 'preview.png')
    if item and item.get('prepared') and (full or original) and not path.exists():
        data = load_json(directory / 'snapshot.json')
        if bytes_used(own_directory('jobs', identifier)) + data['width'] * data['height'] * 4 > MAX_STORAGE:
            raise HTTPException(400, 'Full-size preview exceeds this queue’s 12 GB cache limit. Review a smaller batch.')
        image = await asyncio.to_thread(editor.render, data, original=original, root_override=directory)
        temporary = directory / ('.preview-' + uuid.uuid4().hex + '.png')
        try:
            await asyncio.to_thread(image.save, temporary)
            os.replace(temporary, path)
        finally:
            temporary.unlink(missing_ok=True)
    if not item or not item.get('prepared') or linked(path.parent) or linked(path) or not path.is_file():
        raise HTTPException(404, 'This batch preview is not ready.')
    return FileResponse(path, headers=editor.HEADERS)


@router.get('/jobs/{identifier}/download')
async def download_job(identifier: str, request: Request):
    editor.guard(request)
    value = job(identifier)
    path = own_directory('jobs', identifier) / 'exports.zip'
    if value.get('running') or not value.get('archive_ready') or linked(path) or not path.is_file():
        raise HTTPException(404, 'The batch download is not ready.')
    return FileResponse(path, filename='Local Image batch.zip', headers=editor.HEADERS)
