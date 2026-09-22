"""Local, model-free repair with texture synthesis or OpenCV Telea.

Texture synthesis copies suitable detail from unselected neighboring pixels.
Embark Studios' MIT-licensed implementation runs as a small native helper.
Telea remains available for thin dust/scratch defects in smooth backgrounds.
See THIRD_PARTY_NOTICES.md and licenses/ for upstream credits and licenses.
"""
import base64
import io
import os
from pathlib import Path
import subprocess
import tempfile

import cv2
import numpy as np
from PIL import Image

from removal_blend import clean_selection_mask


TEXTURE_EXE = Path(__file__).resolve().parent / 'tools' / 'texture-synthesis' / 'texture-synthesis.exe'
TEXTURE_TIMEOUT = 120
TEXTURE_MAX_EDGE = 2048
TEXTURE_MAX_PIXELS = 2048 * 2048
TEXTURE_MAX_REPAIR_PIXELS = 512 * 512


def heal_option():
    texture_available = TEXTURE_EXE.is_file()
    return {
        'id': 'heal', 'label': 'Quick Heal', 'available': True, 'device': 'CPU',
        'default_method': 'texture',
        'description': 'Replace small objects using nearby texture. No AI model or GPU service needed.',
        'credit': 'Embark Studios texture-synthesis',
        'credit_url': 'https://github.com/EmbarkStudios/texture-synthesis',
        'methods': [
            {'id': 'texture', 'label': 'Texture', 'available': texture_available,
             'description': 'Copy matching detail from nearby pixels; useful for grass, ground, and small objects.'},
            {'id': 'telea', 'label': 'Dust & scratches', 'available': True,
             'description': 'OpenCV Telea repair for fine marks and thin lines on smooth areas.'},
        ],
    }


def _context_box(source, bounds, margin, minimum=0):
    left, top, right, bottom = bounds
    width = min(source.width, max(minimum, right - left + margin * 2))
    height = min(source.height, max(minimum, bottom - top + margin * 2))
    # Center on the selection, moving the complete context rectangle inside the
    # photograph at its edges. No image pixels are rescaled or reflected.
    x = max(0, min(source.width - width, (left + right) // 2 - width // 2))
    y = max(0, min(source.height - height, (top + bottom) // 2 - height // 2))
    return (x, y, x + width, y + height)


def _texture_repair(color, support):
    if not TEXTURE_EXE.is_file():
        raise ValueError('The Texture repair helper is missing. Reinstall Local Remove or choose Dust & scratches.')
    height, width = support.shape
    if (width > TEXTURE_MAX_EDGE or height > TEXTURE_MAX_EDGE
            or width * height > TEXTURE_MAX_PIXELS
            or int(support.sum()) > TEXTURE_MAX_REPAIR_PIXELS):
        raise ValueError('This selection is too large for Quick Heal Texture. Repair a smaller area or use AI Remove.')
    # Both masks have black pixels in the repair area. Passing an explicit donor
    # mask prevents the object itself from being copied into the repair.
    with tempfile.TemporaryDirectory(prefix='local-remove-texture-') as temporary:
        directory = Path(temporary)
        source_path = directory / 'source.png'
        keep_path = directory / 'keep.png'
        output_path = directory / 'repaired.png'
        Image.fromarray(color).save(source_path)
        Image.fromarray(np.where(support, 0, 255).astype(np.uint8)).save(keep_path)
        command = [
            str(TEXTURE_EXE), '--out-size', f'{width}x{height}',
            '--inpaint', str(keep_path), '--sample-masks', str(keep_path),
            '--no-progress', '--threads', str(min(8, os.cpu_count() or 1)),
            '--seed', '42', '--out', str(output_path), 'generate', str(source_path),
        ]
        try:
            result = subprocess.run(command, capture_output=True, text=True,
                                    timeout=TEXTURE_TIMEOUT, shell=False,
                                    creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
        except subprocess.TimeoutExpired as error:
            raise ValueError('Texture repair took too long. Try a smaller selection or use AI Remove.') from error
        except OSError as error:
            raise ValueError('The Texture repair helper could not start. Reinstall Local Remove.') from error
        if result.returncode != 0:
            # Keep internal temporary paths and native diagnostics out of the UI.
            raise ValueError('Texture repair failed. Try a smaller selection with more surrounding detail.')
        try:
            with Image.open(output_path) as output:
                if output.size != (width, height):
                    raise ValueError('Texture repair returned an unexpected image size.')
                repaired = np.asarray(output.convert('RGB')).copy()
        except (OSError, SyntaxError) as error:
            raise ValueError('Texture repair did not return a readable image.') from error
    return repaired


def heal_image(source, selection, method='texture'):
    """Return an unweighted native-resolution layer patch and its original mask.

    The texture helper sees a crop with at least 192 pixels of context and a
    512-pixel minimum edge. Oversized repair requests fail rather than reducing
    photo resolution. Only the selection and a small border are stored as a
    layer; the existing compositor applies the original soft mask exactly once.
    """
    if method not in ('texture', 'telea'):
        raise ValueError('Choose Texture or Dust & scratches for Quick Heal.')
    if source.size != selection.size:
        raise ValueError('Selection dimensions must match the photo.')
    mask = clean_selection_mask(selection.convert('L'))
    bounds = mask.getbbox()
    if bounds is None:
        raise ValueError('Brush over a small mark or object first.')
    if method == 'texture':
        box = _context_box(source, bounds, margin=192, minimum=512)
        # Check dimensions before allocating the RGB crop and native process.
        if box[2] - box[0] > TEXTURE_MAX_EDGE or box[3] - box[1] > TEXTURE_MAX_EDGE:
            raise ValueError('This selection is too large for Quick Heal Texture. Repair a smaller area or use AI Remove.')
    else:
        box = _context_box(source, bounds, margin=16)
    color = np.asarray(source.crop(box).convert('RGB')).copy()
    cropped_mask = mask.crop(box)
    support = np.asarray(cropped_mask) > 0
    if support.all():
        raise ValueError('Leave some surrounding pixels unselected for Quick Heal to sample.')
    if method == 'texture':
        repaired = _texture_repair(color, support)
    else:
        # OpenCV handles channels independently, preserving RGB channel order.
        repaired = cv2.inpaint(color, support.astype(np.uint8) * 255, 3.0, cv2.INPAINT_TELEA)
    # This is a hard invariant even if a future upstream helper changes behavior.
    repaired[~support] = color[~support]
    patch_box = _context_box(source, bounds, margin=16)
    local_box = (patch_box[0] - box[0], patch_box[1] - box[1],
                 patch_box[2] - box[0], patch_box[3] - box[1])
    patch_color = Image.fromarray(repaired).crop(local_box)
    patch_mask = mask.crop(patch_box)

    def encode(image):
        output = io.BytesIO()
        image.save(output, format='PNG')
        return base64.b64encode(output.getvalue()).decode('ascii')

    return {'x': patch_box[0], 'y': patch_box[1],
            'width': patch_box[2] - patch_box[0], 'height': patch_box[3] - patch_box[1],
            'color': encode(patch_color), 'mask': encode(patch_mask)}
