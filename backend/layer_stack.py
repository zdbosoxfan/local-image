"""Portable, ordered image layers sharing the document's native color precision."""
import copy
import io
import math
import re
import uuid

import cv2
import numpy as np
from PIL import Image, ImageCms, ImageOps

from cutout_composite import (DEFAULT_TRANSFORM, DEFAULT_SHADOW, initial_cutout,
                              validate_cutout, compose_native, transform_matrix)
from stock_attribution import validate_attribution, validate_attributions

SRGB = ImageCms.ImageCmsProfile(ImageCms.createProfile('sRGB'))
COMMON = {'id', 'name', 'kind', 'visible', 'locked', 'discarded', 'opacity', 'transform'}
OPTIONAL = {'patch_ids', 'source', 'cutout', 'attribution', 'reference_attributions'}


def node(kind, name, **extra):
    return dict(id='original' if kind == 'original' else uuid.uuid4().hex,
                kind=kind, name=name, visible=True, locked=kind == 'original',
                discarded=False, opacity=1.0, transform=dict(DEFAULT_TRANSFORM), **extra)


def validate_stack(stack, patches=None):
    if not isinstance(stack, list) or not 1 <= len(stack) <= 1000:
        raise ValueError('A document must contain between 1 and 1,000 layers.')
    seen = set(); referenced = set()
    patch_ids = {item['id'] for item in patches} if patches is not None else None
    for item in stack:
        if not isinstance(item, dict) or not COMMON <= set(item) or not set(item) <= COMMON | OPTIONAL:
            raise ValueError('Layer metadata is invalid.')
        lid = item['id']; kind = item['kind']
        if (not isinstance(lid, str) or (lid != 'original' and not re.fullmatch('[0-9a-f]{32}', lid))
                or lid in seen or kind not in ('original', 'retouch', 'image', 'cutout')
                or (kind == 'original') != (lid == 'original')):
            raise ValueError('Layer identity is invalid.')
        seen.add(lid)
        if not isinstance(item['name'], str) or not item['name'].strip() or len(item['name']) > 255:
            raise ValueError('Enter a layer name with 1–255 characters.')
        if any(type(item[key]) is not bool for key in ('visible', 'locked', 'discarded')):
            raise ValueError('Layer visibility and lock settings are invalid.')
        if type(item['opacity']) not in (int, float) or not math.isfinite(item['opacity']) or not 0 <= item['opacity'] <= 1:
            raise ValueError('Layer opacity must be between 0 and 1.')
        checked = initial_cutout('cutout-' + '0'*32 + '-alpha.png')
        checked['transform'] = item['transform']; validate_cutout(checked)
        if kind == 'retouch':
            ids = item.get('patch_ids')
            if not isinstance(ids, list) or len(ids) > 1000 or any(not isinstance(pid, str) for pid in ids) or len(ids) != len(set(ids)):
                raise ValueError('Retouch layer patches are invalid.')
            for pid in ids:
                if not isinstance(pid, str) or not re.fullmatch('[0-9a-f]{32}', pid) or (patch_ids is not None and pid not in patch_ids):
                    raise ValueError('A retouch layer references a missing patch.')
                # Multiple nodes may deliberately share immutable patches after duplication.
                referenced.add(pid)
        elif 'patch_ids' in item:
            raise ValueError('Only retouch layers can contain repair patches.')
        if kind in ('image', 'cutout'):
            if not isinstance(item.get('source'), str) or not re.fullmatch(r'stack-[0-9a-f]{32}-source\.(png|tif)', item['source']):
                raise ValueError('The layer source asset is invalid.')
        elif 'source' in item:
            raise ValueError('This layer cannot have a source asset.')
        if kind == 'cutout':
            state = validate_cutout(item.get('cutout'))
            if state['background']['mode'] != 'transparent' or state['transform'] != DEFAULT_TRANSFORM:
                raise ValueError('Cutout layer backgrounds and transforms belong to the layer stack.')
        elif 'cutout' in item:
            raise ValueError('Only a cutout layer can contain an alpha mask.')
        if 'attribution' in item: validate_attribution(item['attribution'])
        if 'reference_attributions' in item: validate_attributions(item['reference_attributions'])
    if 'original' not in seen:
        raise ValueError('The protected original layer is missing.')
    return stack


def assets(stack):
    validate_stack(stack)
    names = set()
    for item in stack:
        if 'source' in item: names.add(item['source'])
        if item['kind'] == 'cutout': names.add(item['cutout']['alpha'])
    return names


def rgba(raw):
    if raw.shape[2] == 4: return raw.copy()
    maximum = np.iinfo(raw.dtype).max
    return np.concatenate((raw, np.full((*raw.shape[:2], 1), maximum, dtype=raw.dtype)), axis=2)


def over(back, front):
    maximum = np.iinfo(back.dtype).max
    fa = front[..., 3:4].astype(np.float64) / maximum
    ba = back[..., 3:4].astype(np.float64) / maximum
    alpha = fa + ba*(1-fa)
    numerator = front[..., :3]*fa + back[..., :3]*ba*(1-fa)
    rgb = np.divide(numerator, alpha, out=np.zeros_like(numerator), where=alpha > 0)
    return np.concatenate((np.rint(rgb), np.rint(alpha*maximum)), axis=2).clip(0, maximum).astype(back.dtype)


def working_image(image, dtype, icc):
    image = image.convert('RGBA'); alpha = image.getchannel('A')
    if icc:
        image = ImageCms.profileToProfile(image.convert('RGB'), SRGB, ImageCms.ImageCmsProfile(io.BytesIO(icc)), outputMode='RGB').convert('RGBA')
        image.putalpha(alpha)
    result = np.asarray(image).astype(dtype)
    if dtype == np.uint16: result *= 257
    return result


def native_layer(data, root, item, raw, icc, decode, *, transformed=True):
    size = (data['width'], data['height']); maximum = np.iinfo(raw.dtype).max
    if item['kind'] == 'original':
        image = rgba(raw)
    elif item['kind'] == 'retouch':
        image = np.zeros((*raw.shape[:2], 4), dtype=raw.dtype)
        patches = {patch['id']: patch for patch in data['layers']}
        for pid in item['patch_ids']:
            patch = patches[pid]
            if patch.get('discarded') or not patch['visible']: continue
            if patch.get('kind') == 'snapshot':
                image = rgba(decode(root / patch['snapshot'])[0]); continue
            with Image.open(root / patch['color']) as color, Image.open(root / patch['mask']) as mask:
                color = color.convert('RGBA'); color.putalpha(mask.convert('L'))
                front = working_image(color, raw.dtype, icc)
            x, y = patch['x'], patch['y']; height, width = front.shape[:2]
            image[y:y+height, x:x+width] = over(image[y:y+height, x:x+width], front)
    else:
        if item['source'].endswith('.tif'):
            source, profile, _ = decode(root / item['source'])
            if source.dtype != raw.dtype or profile != icc or source.shape[:2] != raw.shape[:2]:
                raise ValueError('The native layer source does not match its document.')
            image = rgba(source)
        else:
            with Image.open(root / item['source']) as source:
                image = working_image(ImageOps.fit(source.convert('RGBA'), size, method=Image.Resampling.LANCZOS), raw.dtype, icc)
        if item['kind'] == 'cutout':
            state = item['cutout']; shadow = dict(state['shadow'])
            if icc:
                swatch = working_image(Image.new('RGBA', (1, 1), shadow['color']), np.uint8, icc)
                shadow['color'] = '#%02x%02x%02x' % tuple(swatch[0, 0, :3])
            with Image.open(root / state['alpha']) as alpha:
                image = compose_native(image, alpha, None, shadow, state['feather'])
    if transformed and item['transform'] != DEFAULT_TRANSFORM:
        image = compose_native(image, Image.new('L', size, 255), None, DEFAULT_SHADOW, transform=item['transform'])
    if transformed and item['opacity'] != 1:
        image[..., 3] = np.rint(image[..., 3].astype(np.float64)*item['opacity']).astype(raw.dtype)
    return image


def render_native(data, root, raw, icc, decode, *, exclude=None):
    validate_stack(data['layer_stack'], data['layers'])
    result = np.zeros((*raw.shape[:2], 4), dtype=raw.dtype)
    for item in data['layer_stack']:
        if not item['visible'] or item['discarded'] or item['id'] == exclude: continue
        result = over(result, native_layer(data, root, item, raw, icc, decode))
    return result


def display(native, icc):
    image = Image.fromarray(np.rint(native.astype(np.float64)/257).astype(np.uint8) if native.dtype == np.uint16 else native)
    if icc:
        alpha = image.getchannel('A')
        image = ImageCms.profileToProfile(image.convert('RGB'), ImageCms.ImageCmsProfile(io.BytesIO(icc)), SRGB, outputMode='RGB').convert('RGBA')
        image.putalpha(alpha)
    return image


def inverse_patch(color, mask, position, size, transform):
    """Keep brush edits attached to their selected layer after it has moved."""
    if transform == DEFAULT_TRANSFORM: return color, mask, position
    front = Image.new('RGBA', size); patch = color.convert('RGBA'); patch.putalpha(mask)
    front.paste(patch, position)
    array = np.asarray(front).astype(np.float32); alpha = array[..., 3:4]/255
    premult = np.concatenate((array[..., :3]*alpha, alpha), axis=2)
    warped = cv2.warpAffine(premult, transform_matrix(size, transform), size,
                           flags=cv2.INTER_LINEAR | cv2.WARP_INVERSE_MAP, borderMode=cv2.BORDER_CONSTANT, borderValue=0)
    a = warped[..., 3:4]; rgb = np.divide(warped[..., :3], a, out=np.zeros_like(warped[..., :3]), where=a > 0)
    result_mask = Image.fromarray(np.rint(a[..., 0]*255).clip(0,255).astype(np.uint8)); bounds = result_mask.getbbox()
    if bounds is None: raise ValueError('The selection is outside the moved retouch layer.')
    return Image.fromarray(np.rint(rgb).clip(0,255).astype(np.uint8)).crop(bounds), result_mask.crop(bounds), bounds[:2]
