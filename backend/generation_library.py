"""Independent generated-image copies; clearing never deletes editor sessions."""
import asyncio
import hashlib
import json
import math
from pathlib import Path
import stat
import threading
import time
import uuid

from fastapi import APIRouter, HTTPException, Request, Response
from PIL import Image
from pydantic import BaseModel, ConfigDict, Field, model_validator

import local_remove as editor
from generation_metadata import validate_generation_metadata
from stock_attribution import validate_attributions
from upscale_metadata import validate_upscale_metadata

router = APIRouter(prefix='/api/local-remove/generation/library')
_lock = threading.RLock()
FILES = {'image.png', 'thumbnail.png', 'entry.json'}
MAX_IMAGE_BYTES = 80 * 1024 * 1024


def identity(value):
    if not isinstance(value, str):
        raise ValueError('Choose a generated image from the library.')
    try:
        if str(uuid.UUID(value)) != value:
            raise ValueError()
    except ValueError:
        raise ValueError('Choose a generated image from the library.') from None
    return value


def linked(path):
    try:
        return path.is_symlink() or bool(getattr(path.lstat(), 'st_file_attributes', 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT)
    except FileNotFoundError:
        return False


def root_directory():
    root = editor.ROOT / 'generation-library'
    if linked(root) or root.resolve().parent != editor.ROOT.resolve():
        raise ValueError('The generation library must be a regular folder inside Local Image data.')
    root.mkdir(parents=True, exist_ok=True)
    return root


def entry_directory(identifier, *, required=True):
    root = root_directory()
    path = root / identity(identifier)
    if linked(path) or path.resolve().parent != root.resolve():
        raise ValueError('This library entry is not a regular local folder.')
    if required and not path.is_dir():
        raise HTTPException(404, 'Generated image not found in the library.')
    return path


def owned_files(path):
    """Preflight the whole entry before reading or deleting any of its files."""
    if linked(path) or path.resolve().parent != root_directory().resolve() or not path.is_dir():
        raise ValueError('Unsafe generation library entry.')
    files = list(path.iterdir())
    if any(file.name not in FILES or linked(file) or not file.is_file() or file.resolve().parent != path.resolve() for file in files):
        raise ValueError('The library entry contains unexpected files. No files were deleted.')
    return files


def metadata(path):
    files = owned_files(path)
    if {file.name for file in files} != FILES or (path / 'entry.json').stat().st_size > 100_000:
        raise ValueError('The library entry is incomplete.')
    value = json.loads((path / 'entry.json').read_text(encoding='utf-8'))
    keys = {'id', 'name', 'created_at', 'width', 'height', 'generation', 'reference_attributions', 'sha256'}
    if not isinstance(value, dict) or set(value) not in (keys, keys | {'upscale'}) or identity(value['id']) != path.name:
        raise ValueError('The library metadata is invalid.')
    if value['generation'] is not None:
        validate_generation_metadata(value['generation'])
    provenance = validate_upscale_metadata(value['upscale']) if value.get('upscale') else value['generation']
    if provenance is None:
        raise ValueError('The library contains only generated or restored images.')
    validate_attributions(value['reference_attributions'])
    if (not isinstance(value['name'], str) or not 1 <= len(value['name']) <= 255
            or Path(value['name']).name != value['name'] or not value['name'].lower().endswith('.png')
            or type(value['created_at']) not in (int, float) or not math.isfinite(value['created_at'])
            or any(type(value[key]) is not int or value[key] != provenance[key] for key in ('width', 'height'))
            or not isinstance(value['sha256'], str) or len(value['sha256']) != 64
            or any(char not in '0123456789abcdef' for char in value['sha256'])
            or (path / 'image.png').stat().st_size > MAX_IMAGE_BYTES
            or (path / 'thumbnail.png').stat().st_size > 2 * 1024 * 1024):
        raise ValueError('The library metadata is invalid.')
    return value, sum(file.stat().st_size for file in files)


def _add(image, data):
    identifier = identity(data['id'])
    parameters = validate_generation_metadata(dict(data['generation'])) if data.get('generation') is not None else None
    upscale = validate_upscale_metadata(data['upscale']) if data.get('upscale') else None
    provenance = upscale or parameters
    if provenance is None:
        raise ValueError('Only generated or restored results can be added to the library.')
    credits = validate_attributions(data.get('reference_attributions', []))
    if image.size != (provenance['width'], provenance['height']):
        raise ValueError('Generated image dimensions do not match its saved settings.')
    target = entry_directory(identifier, required=False)
    if target.exists():
        metadata(target)
        return identifier
    root = root_directory()
    staging = root / ('.pending-' + uuid.uuid4().hex)
    staging.mkdir()
    try:
        clean = image.convert('RGBA' if 'A' in image.getbands() else 'RGB')
        clean.save(staging / 'image.png', icc_profile=editor.SRGB.tobytes())
        if (staging / 'image.png').stat().st_size > MAX_IMAGE_BYTES:
            raise ValueError('The generated image is too large for the library.')
        thumbnail = clean.copy(); thumbnail.thumbnail((480, 360), Image.Resampling.LANCZOS)
        thumbnail.save(staging / 'thumbnail.png')
        name = Path(str(data.get('name') or 'Generated image.png')).stem[:240] + '.png'
        value = {'id': identifier, 'name': name, 'created_at': float(data.get('created', time.time())),
                 'width': image.width, 'height': image.height, 'generation': parameters,
                 'reference_attributions': credits, 'sha256': hashlib.sha256((staging / 'image.png').read_bytes()).hexdigest()}
        if upscale:
            value['upscale'] = upscale
        (staging / 'entry.json').write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding='utf-8')
        staging.rename(target)
    finally:
        if staging.exists():
            # Staging is created here and only these fixed files can be written.
            for file in owned_files(staging):
                file.unlink()
            staging.rmdir()
    return identifier


def migrate_existing():
    marker = editor.ROOT / 'generation-library-v1.json'
    if linked(marker):
        raise ValueError('The generation library migration record is not a regular file.')
    if marker.exists():
        return
    root_directory()
    for path in list(editor.SESSIONS.iterdir()):
        try:
            if linked(path) or not path.is_dir() or path.resolve().parent != editor.SESSIONS.resolve():
                continue
            data = editor.read_session(identity(path.name))
            if not data.get('generation') and not data.get('upscale'):
                continue
            source = path / data['original']
            if (Path(data['original']).name != data['original'] or linked(source)
                    or source.resolve().parent != path.resolve() or source.stat().st_size > MAX_IMAGE_BYTES):
                continue
            with Image.open(source) as original:
                provenance = (validate_upscale_metadata(data['upscale']) if data.get('upscale')
                              else validate_generation_metadata(data['generation']))
                if original.size != (provenance['width'], provenance['height']):
                    continue
                image = original.copy()
        except (ValueError, OSError, HTTPException, KeyError, TypeError):
            continue  # Closed or malformed recovery sessions confer no file authority.
        try:
            _add(image, data)
        except (ValueError, KeyError, TypeError):
            continue
        # Write failures propagate. Never mark migration complete after disk failure.
    temporary = editor.ROOT / ('generation-library-migration-' + uuid.uuid4().hex + '.tmp')
    try:
        temporary.write_text(json.dumps({'version': 1, 'migrated_at': time.time()}), encoding='utf-8')
        temporary.replace(marker)
    finally:
        temporary.unlink(missing_ok=True)


def add_generated(image, data):
    with _lock:
        migrate_existing()
        return _add(image, data)


def listing():
    with _lock:
        migrate_existing()
        items, total, skipped = [], 0, 0
        for path in root_directory().iterdir():
            try:
                identity(path.name)
                if linked(path) or path.resolve().parent != root_directory().resolve() or not path.is_dir():
                    raise ValueError('Unsafe library entry.')
                # Count owned cache files even when entry metadata is damaged.
                total += sum(file.stat().st_size for file in path.iterdir()
                             if file.name in FILES and not linked(file) and file.is_file())
                value, size = metadata(path)
                provenance = value.get('upscale') or value['generation']
                items.append({**value, 'model': provenance['model'], 'variant': provenance['variant'],
                              'bytes': size, 'thumbnail': f'/api/local-remove/generation/library/{value["id"]}/thumbnail'})
            except (ValueError, OSError, KeyError, HTTPException, TypeError):
                skipped += 1
        items.sort(key=lambda item: (item['created_at'], item['id']), reverse=True)
        return {'items': items, 'count': len(items), 'bytes': total,
                'warning': 'Some incomplete or unexpected library entries could not be read.' if skipped else '',
                'storage_note': 'Library images, thumbnails and metadata only. Clearing keeps open documents, recovery sessions, saved projects and model files.'}


def open_image(identifier):
    with _lock:
        migrate_existing()
        path = entry_directory(identifier)
        value, _ = metadata(path)
        if hashlib.sha256((path / 'image.png').read_bytes()).hexdigest() != value['sha256']:
            raise ValueError('This library image has changed or is damaged. It was not opened.')
        with Image.open(path / 'image.png') as image:
            if image.format != 'PNG' or image.size != (value['width'], value['height']):
                raise ValueError('The library image dimensions do not match its metadata.')
            image.verify()
        result = editor.create_session(path / 'image.png', value['name'])
        data = editor.read_session(result['id'])
        data['revision'] = 1
        if value['generation'] is not None:
            data['generation'] = dict(value['generation'])
        if value.get('upscale') is not None:
            data['upscale'] = dict(value['upscale'])
        if value['reference_attributions']:
            data['reference_attributions'] = value['reference_attributions']
        editor.write_session(editor.folder(data['id']), data)
        return {'session': editor.public(data)}


class DeleteImages(BaseModel):
    model_config = ConfigDict(extra='forbid', strict=True)
    ids: list[str] = Field(default_factory=list, max_length=10000)
    all: bool = False

    @model_validator(mode='after')
    def valid_selection(self):
        if self.all == bool(self.ids) or len(set(self.ids)) != len(self.ids):
            raise ValueError('Choose selected images or clear the whole library.')
        for identifier in self.ids:
            identity(identifier)
        return self


def delete_images(payload):
    with _lock:
        migrate_existing()
        ids = []
        if payload.all:
            for path in root_directory().iterdir():
                try:
                    ids.append(identity(path.name))
                except ValueError:
                    continue
        else:
            ids = payload.ids
        # Validate the complete selection before deleting its first file.
        selected = [(identifier, entry_directory(identifier)) for identifier in ids]
        checked = [(identifier, path, owned_files(path)) for identifier, path in selected]
        freed, deleted = 0, []
        for identifier, path, files in checked:
            for file in files:
                size = file.stat().st_size
                file.unlink(); freed += size
            path.rmdir(); deleted.append(identifier)
        return {**listing(), 'deleted': deleted, 'freed_bytes': freed}


@router.get('')
async def list_library(request: Request):
    editor.guard(request)
    try:
        return await asyncio.to_thread(listing)
    except (ValueError, OSError) as error:
        raise HTTPException(400, str(error)) from error


@router.get('/{identifier}/thumbnail')
async def thumbnail(identifier: str, request: Request):
    editor.guard(request)
    def read():
        with _lock:
            path = entry_directory(identifier)
            metadata(path)
            return (path / 'thumbnail.png').read_bytes()
    try:
        return Response(await asyncio.to_thread(read), media_type='image/png', headers=editor.HEADERS)
    except (ValueError, OSError) as error:
        raise HTTPException(400, str(error)) from error


@router.post('/{identifier}/open')
async def open_library_image(identifier: str, request: Request):
    editor.guard(request, True)
    try:
        return await asyncio.to_thread(open_image, identifier)
    except (ValueError, OSError) as error:
        raise HTTPException(400, str(error)) from error


@router.post('/delete')
async def delete_library_images(request: Request, payload: DeleteImages):
    editor.guard(request, True)
    try:
        return await asyncio.to_thread(delete_images, payload)
    except (ValueError, OSError) as error:
        raise HTTPException(400, str(error)) from error
