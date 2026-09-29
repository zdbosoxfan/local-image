"""Non-destructive alpha refinement and native-precision background compositing."""
import math
import re

import cv2
import numpy as np
from PIL import Image, ImageColor, ImageFilter, ImageOps
from stock_attribution import validate_attribution, validate_attributions


DEFAULT_SHADOW = {'enabled': False, 'opacity': 0.3, 'blur': 18.0, 'offset_x': 12.0,
                  'offset_y': 18.0, 'squeeze': 1.0, 'color': '#000000'}
DEFAULT_TRANSFORM = {'offset_x': 0.0, 'offset_y': 0.0, 'scale': 1.0, 'rotation': 0.0}


def initial_cutout(alpha):
    return {'enabled': True, 'alpha': alpha, 'feather': 0.0,
            'background': {'mode': 'transparent', 'color': '#ffffff'},
            'shadow': dict(DEFAULT_SHADOW), 'transform': dict(DEFAULT_TRANSFORM)}


def validate_cutout(state):
    """Validate portable metadata without ever granting arbitrary file-path access."""
    required = {'enabled', 'alpha', 'feather', 'background', 'shadow'}
    if not isinstance(state, dict) or not required <= set(state) or not set(state) <= required | {'transform'}:
        raise ValueError('Cutout settings are invalid.')
    if type(state['enabled']) is not bool or not isinstance(state['alpha'], str) or not re.fullmatch(r'cutout-[0-9a-f]{32}-alpha\.png', state['alpha']):
        raise ValueError('The cutout alpha asset is invalid.')
    number = lambda x, low, high: type(x) in (int, float) and math.isfinite(x) and low <= x <= high
    if not number(state['feather'], 0, 40):
        raise ValueError('Cutout feathering is invalid.')
    # Early v2 projects predate transforms; identity preserves their appearance.
    transform = state.setdefault('transform', dict(DEFAULT_TRANSFORM))
    if not isinstance(transform, dict) or set(transform) != set(DEFAULT_TRANSFORM):
        raise ValueError('Subject transform settings are invalid.')
    for key, bounds in {'offset_x': (-100000, 100000), 'offset_y': (-100000, 100000), 'scale': (0.05, 4), 'rotation': (-180, 180)}.items():
        if not number(transform[key], *bounds):
            raise ValueError('A subject transform setting is invalid.')
    bg = state['background']
    if not isinstance(bg, dict) or not {'mode', 'color'} <= set(bg) or not set(bg) <= {'mode', 'color', 'asset', 'name', 'attribution', 'reference_attributions'}:
        raise ValueError('Background settings are invalid.')
    if bg['mode'] not in ('transparent', 'color', 'image') or not isinstance(bg['color'], str) or not re.fullmatch(r'#[0-9a-fA-F]{6}', bg['color']):
        raise ValueError('The background mode or color is invalid.')
    if bg['mode'] == 'image' and 'asset' not in bg:
        raise ValueError('Choose a background image first.')
    if 'asset' in bg and (not isinstance(bg['asset'], str) or not re.fullmatch(r'cutout-[0-9a-f]{32}-background\.png', bg['asset'])):
        raise ValueError('The background asset is invalid.')
    if 'name' in bg and (not isinstance(bg['name'], str) or len(bg['name']) > 255):
        raise ValueError('The background name is invalid.')
    if 'attribution' in bg:
        validate_attribution(bg['attribution'])
    if 'reference_attributions' in bg:
        validate_attributions(bg['reference_attributions'])
    shadow = state['shadow']
    if not isinstance(shadow, dict) or set(shadow) != set(DEFAULT_SHADOW) or type(shadow['enabled']) is not bool:
        raise ValueError('Shadow settings are invalid.')
    for key, bounds in {'opacity': (0, 1), 'blur': (0, 100), 'offset_x': (-1000, 1000), 'offset_y': (-1000, 1000), 'squeeze': (0.1, 1)}.items():
        if not number(shadow[key], *bounds):
            raise ValueError('A shadow setting is invalid.')
    if not isinstance(shadow['color'], str) or not re.fullmatch(r'#[0-9a-fA-F]{6}', shadow['color']):
        raise ValueError('The shadow color is invalid.')
    return state


def cutout_assets(state):
    validate_cutout(state)
    return [state['alpha']] + ([state['background']['asset']] if state['background'].get('asset') else [])


def refine_alpha(alpha, selection, operation):
    current = np.asarray(alpha.convert('L'), dtype=np.float32) / 255
    selected = np.asarray(selection.convert('L').resize(alpha.size, Image.Resampling.LANCZOS), dtype=np.float32) / 255
    if operation == 'erase':
        result = current * (1 - selected)
    elif operation == 'restore':
        result = current + (1 - current) * selected
    elif operation == 'replace':
        result = selected
    else:
        raise ValueError('Choose restore, erase, or replace.')
    return Image.fromarray(np.rint(result * 255).astype(np.uint8))


def effective_alpha(alpha, feather):
    alpha = alpha.convert('L')
    return alpha.filter(ImageFilter.GaussianBlur(feather)) if feather else alpha


def transform_matrix(size, transform):
    """Map source coordinates to canvas: clockwise rotation around image center."""
    width, height = size
    center = ((width - 1) / 2, (height - 1) / 2)
    matrix = cv2.getRotationMatrix2D(center, -transform['rotation'], transform['scale'])
    matrix[:, 2] += (transform['offset_x'], transform['offset_y'])
    return matrix


def inverse_selection(selection, transform):
    """Map a displayed canvas selection back onto the original alpha asset."""
    if transform == DEFAULT_TRANSFORM:
        return selection
    width, height = selection.size
    mask = cv2.warpAffine(np.asarray(selection.convert('L')), transform_matrix(selection.size, transform),
                          (width, height), flags=cv2.INTER_LINEAR | cv2.WARP_INVERSE_MAP,
                          borderMode=cv2.BORDER_CONSTANT, borderValue=0)
    return Image.fromarray(mask)


def underlay(size, alpha, background, shadow):
    """Place a soft editable shadow between the background and foreground."""
    if background is None:
        result = Image.new('RGBA', size, (0, 0, 0, 0))
    elif isinstance(background, str):
        result = Image.new('RGBA', size, ImageColor.getrgb(background) + (255,))
    else:
        result = ImageOps.fit(background.convert('RGBA'), size, method=Image.Resampling.LANCZOS)
    if shadow['enabled'] and shadow['opacity']:
        shadow_mask = alpha
        if shadow['squeeze'] != 1:
            bounds = alpha.getbbox()
            if bounds:
                left, top, right, bottom = bounds
                height = max(1, round((bottom - top) * shadow['squeeze']))
                reduced = alpha.crop(bounds).resize((right - left, height), Image.Resampling.LANCZOS)
                shadow_mask = Image.new('L', size)
                shadow_mask.paste(reduced, (left, bottom - height))
        shifted = Image.new('L', size)
        shifted.paste(shadow_mask, (round(shadow['offset_x']), round(shadow['offset_y'])))
        shifted = shifted.filter(ImageFilter.GaussianBlur(shadow['blur']))
        shifted = shifted.point(lambda value: round(value * shadow['opacity']))
        shade = Image.new('RGBA', size, ImageColor.getrgb(shadow['color']) + (0,))
        shade.putalpha(shifted)
        result = Image.alpha_composite(result, shade)
    return result


def compose_native(raw, alpha, background, shadow, feather=0, transform=None):
    """Composite uint8/uint16 original RGB without reducing its precision."""
    maximum = 65535 if raw.dtype == np.uint16 else 255
    alpha = effective_alpha(alpha, feather)
    foreground_alpha = np.asarray(alpha, dtype=np.float32)[..., None] / 255
    if raw.shape[2] == 4:
        foreground_alpha = foreground_alpha * (raw[..., 3:4].astype(np.float32) / maximum)
    # Transform premultiplied color with alpha together. Interpolating straight
    # RGB would leak the old background into the newly softened subject edges.
    premultiplied = raw[..., :3].astype(np.float32) * foreground_alpha
    transform = transform or DEFAULT_TRANSFORM
    if transform != DEFAULT_TRANSFORM:
        rgba = np.concatenate((premultiplied, foreground_alpha), axis=2)
        warped = cv2.warpAffine(rgba, transform_matrix(alpha.size, transform), alpha.size,
                               flags=cv2.INTER_LINEAR, borderMode=cv2.BORDER_CONSTANT, borderValue=0)
        premultiplied, foreground_alpha = warped[..., :3], warped[..., 3:4]
    shadow_alpha = Image.fromarray(np.rint(foreground_alpha[..., 0] * 255).clip(0, 255).astype(np.uint8))
    back = np.asarray(underlay(alpha.size, shadow_alpha, background, shadow), dtype=np.float32)
    back_alpha = back[..., 3:4] / 255
    result_alpha = foreground_alpha + back_alpha * (1 - foreground_alpha)
    numerator = premultiplied + back[..., :3] * (maximum / 255) * back_alpha * (1 - foreground_alpha)
    color = np.divide(numerator, result_alpha, out=np.zeros_like(numerator), where=result_alpha > 0)
    return np.concatenate((np.rint(color), np.rint(result_alpha * maximum)), axis=2).clip(0, maximum).astype(raw.dtype)


def compose_image(image, alpha, background, shadow, feather=0, transform=None):
    return Image.fromarray(compose_native(np.asarray(image.convert('RGBA')), alpha, background, shadow, feather, transform))
