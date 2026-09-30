"""Baidu ERNIE-Image Base: native text-to-image, raw prompt, publisher sampling.

This is the 50-step Base model, not the distilled Turbo or optional prompt enhancer.
"""
import io
import math
import secrets

from PIL import Image

from qwen_image import _object_info, _choices, _normalized_name, _execute_workflow, qwen_canvas_size
from generation_resolution import exact_canvas_size, validate_generation_size

MODEL_FILES = {'unet': 'ernie-image.safetensors', 'clip': 'ministral-3-3b.safetensors', 'vae': 'flux2-vae.safetensors'}
REQUIRED_NODES = ('UNETLoader', 'CLIPLoader', 'VAELoader', 'CLIPTextEncode',
                  'EmptyFlux2LatentImage', 'KSampler', 'VAEDecode', 'SaveImage')


class ErnieImageError(RuntimeError):
    pass


def ernie_model_option(info):
    missing = [node for node in REQUIRED_NODES if node not in info]
    reason = 'Update ComfyUI: missing ERNIE nodes: ' + ', '.join(missing) if missing else ''
    files = {}
    for role, node, field in (('unet', 'UNETLoader', 'unet_name'), ('clip', 'CLIPLoader', 'clip_name'), ('vae', 'VAELoader', 'vae_name')):
        match = next((name for name in _choices(info, node, field)
                      if _normalized_name(name) == _normalized_name(MODEL_FILES[role])), None)
        if match:
            files[role] = match
    if len(files) != 3:
        reason = reason or 'Install ERNIE-Image Base, its Ministral encoder and the shared FLUX.2 VAE, then refresh.'
    for node, field, expected in (('CLIPLoader', 'type', 'flux2'), ('KSampler', 'sampler_name', 'euler'), ('KSampler', 'scheduler', 'simple')):
        if expected not in _choices(info, node, field):
            reason = reason or 'Update ComfyUI: ERNIE native encoding or sampling is unavailable.'
    return {'id': 'bf16', 'label': 'Base BF16', 'available': not reason, 'reason': reason, 'files': files}


def build_ernie_workflow(models, *, prompt, negative_prompt='', size=(1024, 1024), seed=0, steps=50, guidance=4.0):
    width, height = exact_canvas_size(size)
    return {
        '1': {'class_type': 'UNETLoader', 'inputs': {'unet_name': models['unet'], 'weight_dtype': 'default'}},
        '2': {'class_type': 'CLIPLoader', 'inputs': {'clip_name': models['clip'], 'type': 'flux2', 'device': 'default'}},
        '3': {'class_type': 'VAELoader', 'inputs': {'vae_name': models['vae']}},
        '4': {'class_type': 'CLIPTextEncode', 'inputs': {'clip': ['2', 0], 'text': prompt}},
        '5': {'class_type': 'CLIPTextEncode', 'inputs': {'clip': ['2', 0], 'text': negative_prompt}},
        '6': {'class_type': 'EmptyFlux2LatentImage', 'inputs': {'width': width, 'height': height, 'batch_size': 1}},
        '7': {'class_type': 'KSampler', 'inputs': {'model': ['1', 0], 'seed': seed, 'steps': steps, 'cfg': guidance,
              'sampler_name': 'euler', 'scheduler': 'simple', 'positive': ['4', 0], 'negative': ['5', 0],
              'latent_image': ['6', 0], 'denoise': 1.0}},
        '8': {'class_type': 'VAEDecode', 'inputs': {'samples': ['7', 0], 'vae': ['3', 0]}},
        '9': {'class_type': 'SaveImage', 'inputs': {'images': ['8', 0], 'filename_prefix': 'LocalImage_ERNIE'}},
    }


async def run_ernie_image(prompt, *, negative_prompt='', size=(1024, 1024), seed=None, steps=50, guidance=4.0):
    if not str(prompt).strip() or type(steps) is not int or not 1 <= steps <= 100:
        raise ValueError('Provide a prompt and between 1 and 100 ERNIE steps.')
    if type(guidance) not in (int, float) or not math.isfinite(guidance) or not 1 <= guidance <= 10:
        raise ValueError('ERNIE guidance must be between 1 and 10.')
    info = await _object_info()
    selected = ernie_model_option(info)
    if not selected['available']:
        raise ErnieImageError(selected['reason'])
    size = validate_generation_size(size, 'ernie-image', info)
    graph = build_ernie_workflow(selected['files'], prompt=prompt, negative_prompt=negative_prompt,
        size=size, seed=secrets.randbits(48) if seed is None else seed, steps=steps, guidance=guidance)
    try:
        result = await _execute_workflow(graph)
        with Image.open(io.BytesIO(result)) as image:
            output = image.convert('RGB')
        if output.size != size:
            raise ErnieImageError('ERNIE returned unexpected dimensions. Refresh ComfyUI and retry.')
        return output
    except ErnieImageError:
        raise
    except Exception as error:
        detail = str(error)
        if 'out of memory' in detail.lower() or 'allocation' in detail.lower():
            detail = 'The GPU ran out of memory. Reduce the poster dimensions.'
        raise ErnieImageError('ERNIE-Image could not finish: ' + detail) from error
