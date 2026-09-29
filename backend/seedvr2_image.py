"""Native, one-step SeedVR2 7B restoration with fixed source geometry and alpha.

The official ComfyUI template uses a resized source latent, not text generation.
Tiled VAE processing bounds encoder/decoder memory; diffusion itself is not tiled.
"""
import io
from pathlib import Path
import secrets
import tempfile

from PIL import Image, ImageOps

from qwen_image import _object_info, _choices, _normalized_name, _execute_workflow

MODEL_FILES = {'unet': 'seedvr2_7b_fp16.safetensors', 'vae': 'seedvr2_ema_vae_fp16.safetensors'}
REQUIRED_NODES = ('UNETLoader', 'VAELoader', 'LoadImage', 'SeedVR2Preprocess',
                  'VAEEncodeTiled', 'SeedVR2Conditioning', 'KSampler', 'VAEDecodeTiled',
                  'SeedVR2PostProcessing', 'SaveImage')


class SeedVR2ImageError(RuntimeError):
    pass


def seedvr2_model_option(info):
    missing = [node for node in REQUIRED_NODES if node not in info]
    reason = 'Update ComfyUI: missing SeedVR2 nodes: ' + ', '.join(missing) if missing else ''
    files = {}
    for role, node, field in (('unet', 'UNETLoader', 'unet_name'), ('vae', 'VAELoader', 'vae_name')):
        match = next((name for name in _choices(info, node, field)
                      if _normalized_name(name) == _normalized_name(MODEL_FILES[role])), None)
        if match:
            files[role] = match
    if len(files) != 2:
        reason = reason or 'Install SeedVR2 7B FP16 and its VAE, then refresh.'
    for node, field, expected in (('KSampler', 'sampler_name', 'euler'), ('KSampler', 'scheduler', 'simple'),
                                  ('SeedVR2PostProcessing', 'color_correction_method', 'lab')):
        if expected not in _choices(info, node, field):
            reason = reason or 'Update ComfyUI: the required SeedVR2 sampling or color correction is unavailable.'
    return {'id': 'fp16', 'label': '7B FP16', 'available': not reason, 'reason': reason, 'files': files}


def validate_size(source_size, size):
    if (len(size) != 2 or any(type(value) is not int or value < 256 or value > 4096 or value % 2 for value in size)
            or size[0] * size[1] > 16777216):
        raise ValueError('Use even dimensions between 256 and 4096 pixels, up to 16 megapixels.')
    sw, sh = source_size
    if size[0] < sw or size[1] < sh or size == source_size:
        raise ValueError('Choose an output larger than the source image.')
    if abs(size[0] - size[1] * sw / sh) > 2 and abs(size[1] - size[0] * sh / sw) > 2:
        raise ValueError('Keep the source aspect ratio when upscaling.')
    return size


def build_seedvr2_workflow(models, source_path, *, seed=0):
    # The source file is already resized to the exact requested dimensions.
    tile = {'tile_size': 512, 'overlap': 128, 'temporal_size': 4096, 'temporal_overlap': 8}
    return {
        '1': {'class_type': 'UNETLoader', 'inputs': {'unet_name': models['unet'], 'weight_dtype': 'default'}},
        '2': {'class_type': 'VAELoader', 'inputs': {'vae_name': models['vae']}},
        '3': {'class_type': 'LoadImage', 'inputs': {'image': str(source_path)}},
        '4': {'class_type': 'SeedVR2Preprocess', 'inputs': {'resized_images': ['3', 0]}},
        '5': {'class_type': 'VAEEncodeTiled', 'inputs': {'pixels': ['4', 0], 'vae': ['2', 0], **tile}},
        '6': {'class_type': 'SeedVR2Conditioning', 'inputs': {'model': ['1', 0], 'vae_conditioning': ['5', 0]}},
        '7': {'class_type': 'KSampler', 'inputs': {'model': ['1', 0], 'seed': seed, 'steps': 1, 'cfg': 1.0,
              'sampler_name': 'euler', 'scheduler': 'simple', 'positive': ['6', 0], 'negative': ['6', 1],
              'latent_image': ['5', 0], 'denoise': 1.0}},
        '8': {'class_type': 'VAEDecodeTiled', 'inputs': {'samples': ['7', 0], 'vae': ['2', 0], **tile}},
        '9': {'class_type': 'SeedVR2PostProcessing', 'inputs': {'images': ['8', 0],
              'original_resized_images': ['3', 0], 'color_correction_method': 'lab'}},
        '10': {'class_type': 'SaveImage', 'inputs': {'images': ['9', 0], 'filename_prefix': 'LocalImage_SeedVR2'}},
    }


async def run_seedvr2_image(source_path, *, size, seed=None):
    if seed is not None and (type(seed) is not int or not 0 <= seed <= 2**53 - 1):
        raise ValueError('Use a valid whole-number seed.')
    with Image.open(source_path) as image:
        source = ImageOps.exif_transpose(image).convert('RGBA' if 'A' in image.getbands() or 'transparency' in image.info else 'RGB')
    size = validate_size(source.size, tuple(size))
    selected = seedvr2_model_option(await _object_info())
    if not selected['available']:
        raise SeedVR2ImageError(selected['reason'])
    # Pillow resizes RGBA in premultiplied space, avoiding dark transparent fringes.
    resized = source.resize(size, Image.Resampling.LANCZOS)
    alpha = resized.getchannel('A') if resized.mode == 'RGBA' else None
    rgb = (Image.alpha_composite(Image.new('RGBA', size, 'white'), resized).convert('RGB')
           if alpha is not None else resized)
    try:
        with tempfile.TemporaryDirectory(prefix='local-image-seedvr2-') as temporary:
            path = Path(temporary) / 'source.png'
            rgb.save(path)
            graph = build_seedvr2_workflow(selected['files'], path, seed=secrets.randbits(48) if seed is None else seed)
            result = await _execute_workflow(graph)
        with Image.open(io.BytesIO(result)) as image:
            output = image.convert('RGB')
        if output.size != size:
            raise SeedVR2ImageError('SeedVR2 returned unexpected dimensions. No resized substitute was returned.')
        if alpha is not None:
            # Keep source RGB at translucent boundaries. A neutral matte is useful
            # to the restoration model but must not become a second matte when
            # the result is later composited over a dark background.
            edge_weight = alpha.point([round(255 * (value / 255) ** 4) for value in range(256)])
            output = Image.composite(output, resized.convert('RGB'), edge_weight)
            output.putalpha(alpha)
        return output
    except SeedVR2ImageError:
        raise
    except Exception as error:
        detail = str(error)
        if 'out of memory' in detail.lower() or 'allocation' in detail.lower():
            detail = 'The GPU ran out of memory. Reduce the final output resolution.'
        raise SeedVR2ImageError('SeedVR2 could not finish: ' + detail) from error
