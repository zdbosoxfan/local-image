"""Portable Local Remove projects; archives contain assets, never disk paths."""
import hashlib
import json
from pathlib import Path
import re
import stat
import zipfile

from PIL import Image

FORMAT = 'local-remove-project'
VERSION = 1
MAX_LAYERS = 1000
MAX_PIXELS = 150_000_000
MAX_ASSET = 2 * 1024**3
MAX_TOTAL = 12 * 1024**3
MAX_MANIFEST = 1024**2
LAYER_KEYS = {'id', 'name', 'x', 'y', 'width', 'height', 'visible', 'discarded',
              'model', 'model_label', 'kind', 'heal_method', 'color', 'mask', 'snapshot'}
SUFFIXES = {'.png', '.jpg', '.jpeg', '.tif', '.tiff', '.webp'}


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def integer(value, minimum, maximum):
    return type(value) is int and minimum <= value <= maximum


def safe_name(value):
    return isinstance(value, str) and 0 < len(value) <= 255 and not any(c in value for c in '\\/:\x00') and value not in {'.', '..'}


def write_project(root, data, target):
    """ZIP_STORED avoids recompressing the already compressed photographic assets."""
    original = 'original' + Path(data['original']).suffix.lower()
    if original != data['original'] or Path(original).suffix not in SUFFIXES:
        raise ValueError('The original asset is invalid.')
    layers = []
    names = [original, 'base.png']
    for source in data['layers']:
        layer = {key: value for key, value in source.items() if key in LAYER_KEYS}
        lid = layer['id']
        if not re.fullmatch('[0-9a-f]{32}', lid):
            raise ValueError('A layer identifier is invalid.')
        for role in ('color', 'mask', 'snapshot'):
            if role in layer:
                name = lid + '-' + role + ('.tif' if role == 'snapshot' else '.png')
                if layer[role] != name:
                    raise ValueError('A layer asset is invalid.')
                names.append(name)
        with Image.open(root / layer['color']) as image:
            layer['width'], layer['height'] = image.size
        layers.append(layer)
    if len(layers) > MAX_LAYERS:
        raise ValueError('A project can contain up to 1,000 layers.')
    assets = {}
    total = 0
    for name in names:
        path = root / name
        size = path.stat().st_size
        if path.is_symlink() or not 0 < size <= MAX_ASSET:
            raise ValueError('A project asset is missing or too large.')
        total += size
        assets[name] = {'size': size, 'sha256': digest(path)}
    if total > MAX_TOTAL:
        raise ValueError('This project exceeds the 12 GB project limit.')
    manifest = {'format': FORMAT, 'version': VERSION, 'name': data['name'],
                'width': data['width'], 'height': data['height'], 'bit_depth': data['bit_depth'],
                'revision': data['revision'], 'original': original, 'layers': layers, 'assets': assets}
    metadata = json.dumps(manifest, ensure_ascii=False, separators=(',', ':')).encode('utf-8')
    if len(metadata) > MAX_MANIFEST:
        raise ValueError('Project metadata is too large.')
    with zipfile.ZipFile(target, 'w', compression=zipfile.ZIP_STORED, allowZip64=True) as archive:
        archive.writestr('manifest.json', metadata)
        for name in names:
            archive.write(root / name, name)
    return manifest


def read_manifest(archive):
    infos = archive.infolist()
    if len(infos) > MAX_LAYERS * 3 + 3:
        raise ValueError('The project contains too many assets.')
    names = [info.filename for info in infos]
    if len(set(names)) != len(names) or 'manifest.json' not in names:
        raise ValueError('The project contains missing or duplicate entries.')
    total = 0
    for info in infos:
        mode = info.external_attr >> 16
        if (not safe_name(info.filename) or info.is_dir() or stat.S_ISLNK(mode)
                or (stat.S_IFMT(mode) not in (0, stat.S_IFREG))
                or info.flag_bits & 1 or info.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED)
                or not 0 < info.file_size <= MAX_ASSET
                or info.file_size > max(1024**2, info.compress_size * 200)):
            raise ValueError('The project contains an unsafe or oversized asset.')
        total += info.file_size
    if total > MAX_TOTAL:
        raise ValueError('The project exceeds the 12 GB project limit.')
    info = archive.getinfo('manifest.json')
    if info.file_size > MAX_MANIFEST:
        raise ValueError('The project metadata is too large.')
    manifest = json.loads(archive.read(info))
    required = {'format', 'version', 'name', 'width', 'height', 'bit_depth', 'revision', 'original', 'layers', 'assets'}
    if not isinstance(manifest, dict) or set(manifest) != required or manifest['format'] != FORMAT or manifest['version'] != VERSION:
        raise ValueError('This is not a supported Local Remove project.')
    if not safe_name(manifest['name']) or not all(integer(manifest[key], 1, 100000) for key in ('width', 'height')):
        raise ValueError('Project image metadata is invalid.')
    if manifest['width'] * manifest['height'] > MAX_PIXELS or manifest['bit_depth'] not in (8, 16) or not integer(manifest['revision'], 0, 2**53 - 1):
        raise ValueError('Project precision or dimensions are invalid.')
    original = manifest['original']
    if not isinstance(original, str) or original != 'original' + Path(original).suffix.lower() or Path(original).suffix not in SUFFIXES:
        raise ValueError('The project original is invalid.')
    layers = manifest['layers']
    if not isinstance(layers, list) or len(layers) > MAX_LAYERS:
        raise ValueError('The project layer list is invalid.')
    allowed = {original, 'base.png'}
    seen = set()
    for layer in layers:
        required_layer = {'id', 'name', 'x', 'y', 'width', 'height', 'visible', 'discarded', 'color', 'mask'}
        if not isinstance(layer, dict) or not required_layer <= set(layer) or not set(layer) <= LAYER_KEYS:
            raise ValueError('A project layer is invalid.')
        lid = layer['id']
        if not isinstance(lid, str) or not re.fullmatch('[0-9a-f]{32}', lid) or lid in seen:
            raise ValueError('A project layer identifier is invalid.')
        seen.add(lid)
        if not isinstance(layer['name'], str) or len(layer['name']) > 255 or type(layer['visible']) is not bool or type(layer['discarded']) is not bool:
            raise ValueError('A project layer label or visibility is invalid.')
        for key in ('model', 'model_label', 'kind', 'heal_method'):
            if key in layer and (not isinstance(layer[key], str) or len(layer[key]) > 255):
                raise ValueError('A project layer setting is invalid.')
        if (not all(integer(layer[key], 0, 100000) for key in ('x', 'y'))
                or not all(integer(layer[key], 1, 100000) for key in ('width', 'height'))
                or layer['x'] + layer['width'] > manifest['width'] or layer['y'] + layer['height'] > manifest['height']):
            raise ValueError('A project layer lies outside its image.')
        for role in ('color', 'mask', 'snapshot'):
            if role in layer:
                name = lid + '-' + role + ('.tif' if role == 'snapshot' else '.png')
                if layer[role] != name:
                    raise ValueError('A project layer asset is invalid.')
                allowed.add(name)
        if layer.get('kind') == 'snapshot':
            if 'snapshot' not in layer or (layer['x'], layer['y'], layer['width'], layer['height']) != (0, 0, manifest['width'], manifest['height']):
                raise ValueError('A merged project layer is invalid.')
        elif 'snapshot' in layer or layer.get('kind') not in (None, 'patch'):
            raise ValueError('A project layer type is unsupported.')
    assets = manifest['assets']
    if not isinstance(assets, dict) or set(assets) != allowed or set(names) != allowed | {'manifest.json'}:
        raise ValueError('The project contains missing or unexpected assets.')
    for name, asset in assets.items():
        if (not isinstance(asset, dict) or set(asset) != {'size', 'sha256'}
                or not integer(asset['size'], 1, MAX_ASSET) or asset['size'] != archive.getinfo(name).file_size
                or not isinstance(asset['sha256'], str) or not re.fullmatch('[0-9a-f]{64}', asset['sha256'])):
            raise ValueError('Project asset verification data is invalid.')
    return manifest


def extract_project(source, target, decode_original):
    """Read only whitelisted leaf filenames with bounded extraction and hashes."""
    with zipfile.ZipFile(source, 'r') as archive:
        manifest = read_manifest(archive)
        for name, asset in manifest['assets'].items():
            remaining = asset['size']
            checksum = hashlib.sha256()
            with archive.open(name) as stream, (target / name).open('xb') as output:
                while remaining:
                    chunk = stream.read(min(1024**2, remaining))
                    if not chunk:
                        raise ValueError('A project asset ended unexpectedly.')
                    remaining -= len(chunk)
                    checksum.update(chunk)
                    output.write(chunk)
                if stream.read(1) or checksum.hexdigest() != asset['sha256']:
                    raise ValueError('A project asset failed its integrity check.')
    size = (manifest['width'], manifest['height'])
    # Inspect the header before allocating the original full-resolution array.
    with Image.open(target / manifest['original']) as original:
        if original.width * original.height > MAX_PIXELS:
            raise ValueError('The project original is too large.')
    raw, profile, preview = decode_original(target / manifest['original'])
    if preview.size != size or (16 if raw.dtype.itemsize == 2 else 8) != manifest['bit_depth']:
        raise ValueError('The project original does not match its metadata.')
    with Image.open(target / 'base.png') as base:
        if base.format != 'PNG' or base.mode != 'RGB' or base.size != size:
            raise ValueError('The project display base is invalid.')
        base.load()
    for layer in manifest['layers']:
        patch_size = (layer['width'], layer['height'])
        for role, modes in (('color', ('RGB', 'RGBA')), ('mask', ('L',))):
            with Image.open(target / layer[role]) as image:
                if image.format != 'PNG' or image.mode not in modes or image.size != patch_size:
                    raise ValueError('A project layer image is invalid.')
                image.load()
        if layer.get('kind') == 'snapshot':
            with Image.open(target / layer['mask']) as mask:
                if mask.getextrema() != (255, 255):
                    raise ValueError('A merged project layer must cover the full image.')
            with Image.open(target / layer['snapshot']) as snapshot_header:
                if snapshot_header.size != size:
                    raise ValueError('A merged project layer has invalid dimensions.')
            snapshot, snapshot_profile, _ = decode_original(target / layer['snapshot'])
            if snapshot.shape != raw.shape or snapshot.dtype != raw.dtype or snapshot_profile != profile:
                raise ValueError('A merged project layer has incompatible precision or profile.')
    return manifest
