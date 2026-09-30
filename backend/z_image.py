"""Native Z-Image Turbo text-to-image and VAE-initialized image variations.

Graph defaults follow Comfy-Org's image_z_image_turbo.json: AuraFlow shift 3,
res_multistep/simple, eight steps and CFG 1. Image variations initialize the
sampler with VAEEncode; they are not semantic image-reference conditioning.
"""
import io
import secrets
import tempfile
from pathlib import Path

from PIL import Image, ImageOps

from qwen_image import _object_info, _choices, _normalized_name, _execute_workflow, qwen_canvas_size
from generation_resolution import exact_canvas_size, validate_generation_size

MODEL_FILES = {'unet': 'z_image_turbo_bf16.safetensors', 'clip': 'qwen_3_4b.safetensors', 'vae': 'ae.safetensors'}
LOADERS = {'unet': ('UNETLoader', 'unet_name'), 'clip': ('CLIPLoader', 'clip_name'), 'vae': ('VAELoader', 'vae_name')}
REQUIRED_NODES = ('UNETLoader', 'CLIPLoader', 'VAELoader', 'CLIPTextEncode', 'ConditioningZeroOut',
                  'EmptySD3LatentImage', 'ModelSamplingAuraFlow', 'KSampler', 'VAEDecode', 'SaveImage')
LICENSE_URL = 'https://huggingface.co/Tongyi-MAI/Z-Image-Turbo'


class ZImageError(RuntimeError):
    pass


def z_image_model_option(info):
    missing_nodes = [node for node in REQUIRED_NODES if node not in info]
    reason = 'Update ComfyUI: missing Z-Image Turbo nodes: ' + ', '.join(missing_nodes) if missing_nodes else ''
    if not reason and 'lumina2' not in _choices(info, 'CLIPLoader', 'type'):
        reason = 'Update ComfyUI: its CLIP loader does not support the lumina2 encoder type.'
    if not reason and 'res_multistep' not in _choices(info, 'KSampler', 'sampler_name'):
        reason = 'Update ComfyUI: the Z-Image Turbo res_multistep sampler is unavailable.'
    files, missing = {}, []
    for key, expected in MODEL_FILES.items():
        match = next((name for name in _choices(info, *LOADERS[key]) if _normalized_name(name) == _normalized_name(expected)), None)
        if match:
            files[key] = match
        else:
            missing.append(expected)
    reason = reason or ('Missing Z-Image Turbo files: ' + ', '.join(missing) + '. Install the model preset and refresh.' if missing else '')
    return {'id': 'bf16', 'label': 'BF16', 'available': not reason, 'reason': reason, 'files': files}


def build_z_image_workflow(models, *, prompt, size=(1024, 1024), seed=0, steps=8, input_path=None, denoise=1.0, loras=()):
    width, height = exact_canvas_size(size)
    graph = {
        '1': {'class_type': 'UNETLoader', 'inputs': {'unet_name': models['unet'], 'weight_dtype': 'default'}},
        '2': {'class_type': 'CLIPLoader', 'inputs': {'clip_name': models['clip'], 'type': 'lumina2', 'device': 'default'}},
        '3': {'class_type': 'VAELoader', 'inputs': {'vae_name': models['vae']}},
        '4': {'class_type': 'CLIPTextEncode', 'inputs': {'clip': ['2', 0], 'text': prompt}},
        '5': {'class_type': 'ConditioningZeroOut', 'inputs': {'conditioning': ['4', 0]}},
        '6': {'class_type': 'ModelSamplingAuraFlow', 'inputs': {'model': ['1', 0], 'shift': 3.0}},
        '7': {'class_type': 'KSampler', 'inputs': {'model': ['6', 0], 'seed': int(seed), 'steps': int(steps),
              'cfg': 1.0, 'sampler_name': 'res_multistep', 'scheduler': 'simple',
              'positive': ['4', 0], 'negative': ['5', 0], 'latent_image': ['10', 0], 'denoise': float(denoise) if input_path else 1.0}},
        '8': {'class_type': 'VAEDecode', 'inputs': {'samples': ['7', 0], 'vae': ['3', 0]}},
        '9': {'class_type': 'SaveImage', 'inputs': {'images': ['8', 0], 'filename_prefix': 'LocalImage_ZImageTurbo'}},
    }
    from lora_workflow import add_lora_chain
    graph['6']['inputs']['model'] = add_lora_chain(graph, ['1', 0], loras)
    if input_path:
        graph['11'] = {'class_type': 'LoadImage', 'inputs': {'image': str(input_path)}}
        graph['10'] = {'class_type': 'VAEEncode', 'inputs': {'pixels': ['11', 0], 'vae': ['3', 0]}}
    else:
        graph['10'] = {'class_type': 'EmptySD3LatentImage', 'inputs': {'width': width, 'height': height, 'batch_size': 1}}
    return graph


async def run_z_image(prompt, *, input_path=None, size=(1024, 1024), seed=None, steps=8, denoise=0.6, loras=()):
    if not str(prompt).strip():
        raise ValueError('Describe the image to generate.')
    if type(steps) is not int or not 1 <= steps <= 50:
        raise ValueError('Z-Image Turbo uses between 1 and 50 steps.')
    if not 0.05 <= float(denoise) <= 1:
        raise ValueError('Image variation strength must be between 0.05 and 1.')
    info = await _object_info()
    selected = z_image_model_option(info)
    if not selected['available']:
        raise ZImageError(selected['reason'])
    from lora_workflow import available_loras
    loras = available_loras(info, loras)
    if input_path and any(node not in info for node in ('VAEEncode', 'LoadImage')):
        raise ZImageError('Update ComfyUI: image variations require VAEEncode and LoadImage.')
    size = validate_generation_size(size, 'z-image-turbo', info, references=input_path is not None)
    try:
        with tempfile.TemporaryDirectory(prefix='local-image-z-') as temporary:
            normalized = None
            if input_path:
                with Image.open(input_path) as original:
                    rgba = ImageOps.exif_transpose(original).convert('RGBA')
                white = Image.new('RGBA', rgba.size, 'white')
                rgba = Image.alpha_composite(white, rgba).convert('RGB')
                image = ImageOps.fit(rgba, size, method=Image.Resampling.LANCZOS)
                normalized = Path(temporary) / 'initial-image.png'; image.save(normalized)
            graph = build_z_image_workflow(selected['files'], prompt=prompt, size=size,
                seed=secrets.randbits(48) if seed is None else seed, steps=steps, input_path=normalized, denoise=denoise, loras=loras)
            result = await _execute_workflow(graph)
        with Image.open(io.BytesIO(result)) as image:
            output = image.convert('RGB')
        if output.size != size:
            raise ZImageError('Z-Image Turbo returned unexpected dimensions; the result was not resized. Refresh ComfyUI and retry.')
        return output
    except ZImageError:
        raise
    except Exception as error:
        detail = str(error)
        if 'out of memory' in detail.lower() or 'allocation' in detail.lower():
            detail = 'The GPU ran out of memory. Reduce the output dimensions or close other GPU jobs.'
        raise ZImageError('Z-Image Turbo could not finish: ' + detail) from error
