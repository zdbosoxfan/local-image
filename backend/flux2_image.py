"""Native FLUX.2 Dev and distilled Klein generation; separate from repair LoRAs.

Dev: FluxGuidance + BasicGuider, 20 Euler steps. Distilled Klein: CFG 1,
four Euler steps. Both use Flux2Scheduler and semantic ReferenceLatent inputs.
"""
import io
import math
import secrets
import tempfile
from pathlib import Path

from PIL import Image, ImageOps

from qwen_image import _object_info, _choices, _normalized_name, _execute_workflow, qwen_canvas_size

PRESETS = {
    'flux2-dev': {'variant': 'fp8', 'label': 'FLUX.2 Dev', 'steps': 20, 'guidance': 4.0,
                 'files': {'unet': 'flux2_dev_fp8mixed.safetensors', 'clip': 'mistral_3_small_flux2_fp8.safetensors', 'vae': 'flux2-vae.safetensors'}},
    'flux2-klein-4b': {'variant': 'bf16', 'label': 'FLUX.2 Klein 4B', 'steps': 4, 'guidance': 1.0,
                      'files': {'unet': 'flux-2-klein-4b.safetensors', 'clip': 'qwen_3_4b.safetensors', 'vae': 'flux2-vae.safetensors'}},
    'flux2-klein-9b': {'variant': 'fp8', 'label': 'FLUX.2 Klein 9B', 'steps': 4, 'guidance': 1.0,
                      'files': {'unet': 'flux-2-klein-9b-fp8.safetensors', 'clip': 'qwen_3_8b_fp8mixed.safetensors', 'vae': 'flux2-vae.safetensors'}},
}
LOADERS = {'unet': ('UNETLoader', 'unet_name'), 'clip': ('CLIPLoader', 'clip_name'), 'vae': ('VAELoader', 'vae_name')}
REQUIRED_NODES = ('UNETLoader', 'CLIPLoader', 'VAELoader', 'CLIPTextEncode', 'EmptyFlux2LatentImage',
                  'Flux2Scheduler', 'RandomNoise', 'KSamplerSelect', 'SamplerCustomAdvanced', 'VAEDecode', 'SaveImage')


class Flux2ImageError(RuntimeError):
    pass


def flux2_model_option(info, model):
    preset = PRESETS[model]
    required = REQUIRED_NODES + (('FluxGuidance', 'BasicGuider') if model == 'flux2-dev' else ('CFGGuider', 'ConditioningZeroOut'))
    missing_nodes = [node for node in required if node not in info]
    reason = 'Update ComfyUI: missing FLUX.2 nodes: ' + ', '.join(missing_nodes) if missing_nodes else ''
    if not reason and 'flux2' not in _choices(info, 'CLIPLoader', 'type'):
        reason = 'Update ComfyUI: its CLIP loader does not support the flux2 encoder type.'
    if not reason and 'euler' not in _choices(info, 'KSamplerSelect', 'sampler_name'):
        reason = 'Update ComfyUI: the Euler sampler is unavailable.'
    files, missing = {}, []
    for role, expected in preset['files'].items():
        match = next((name for name in _choices(info, *LOADERS[role]) if _normalized_name(name) == _normalized_name(expected)), None)
        if match:
            files[role] = match
        else:
            missing.append(expected)
    reason = reason or ('Missing ' + preset['label'] + ' files: ' + ', '.join(missing) + '. Install the selected preset and refresh.' if missing else '')
    return {'id': preset['variant'], 'label': preset['variant'].upper(), 'available': not reason, 'reason': reason, 'files': files}


def build_flux2_workflow(models, *, model, prompt, size=(1024, 1024), seed=0, steps=None, guidance=None, references=(), loras=()):
    preset = PRESETS[model]; dev = model == 'flux2-dev'
    width, height = qwen_canvas_size(size)
    graph = {
        '1': {'class_type': 'UNETLoader', 'inputs': {'unet_name': models['unet'], 'weight_dtype': 'default'}},
        '2': {'class_type': 'CLIPLoader', 'inputs': {'clip_name': models['clip'], 'type': 'flux2', 'device': 'default'}},
        '3': {'class_type': 'VAELoader', 'inputs': {'vae_name': models['vae']}},
        '4': {'class_type': 'CLIPTextEncode', 'inputs': {'clip': ['2', 0], 'text': prompt}},
        '7': {'class_type': 'EmptyFlux2LatentImage', 'inputs': {'width': width, 'height': height, 'batch_size': 1}},
        '8': {'class_type': 'Flux2Scheduler', 'inputs': {'steps': preset['steps'] if steps is None else steps, 'width': width, 'height': height}},
        '9': {'class_type': 'RandomNoise', 'inputs': {'noise_seed': int(seed)}},
        '10': {'class_type': 'KSamplerSelect', 'inputs': {'sampler_name': 'euler'}},
        '11': {'class_type': 'SamplerCustomAdvanced', 'inputs': {'noise': ['9', 0], 'guider': ['6', 0],
               'sampler': ['10', 0], 'sigmas': ['8', 0], 'latent_image': ['7', 0]}},
        '12': {'class_type': 'VAEDecode', 'inputs': {'samples': ['11', 0], 'vae': ['3', 0]}},
        '13': {'class_type': 'SaveImage', 'inputs': {'images': ['12', 0], 'filename_prefix': 'LocalImage_Flux2'}},
    }
    positive = ['4', 0]
    if dev:
        graph['5'] = {'class_type': 'FluxGuidance', 'inputs': {'conditioning': positive, 'guidance': preset['guidance'] if guidance is None else guidance}}
        positive = ['5', 0]
    else:
        graph['5'] = {'class_type': 'ConditioningZeroOut', 'inputs': {'conditioning': positive}}
    negative = ['5', 0]
    for index, reference in enumerate(references):
        load, latent, pos, neg = map(str, (20 + index * 4, 21 + index * 4, 22 + index * 4, 23 + index * 4))
        graph[load] = {'class_type': 'LoadImage', 'inputs': {'image': str(reference)}}
        graph[latent] = {'class_type': 'VAEEncode', 'inputs': {'pixels': [load, 0], 'vae': ['3', 0]}}
        graph[pos] = {'class_type': 'ReferenceLatent', 'inputs': {'conditioning': positive, 'latent': [latent, 0]}}
        positive = [pos, 0]
        if not dev:
            graph[neg] = {'class_type': 'ReferenceLatent', 'inputs': {'conditioning': negative, 'latent': [latent, 0]}}
            negative = [neg, 0]
    graph['6'] = ({'class_type': 'BasicGuider', 'inputs': {'model': ['1', 0], 'conditioning': positive}} if dev else
                  {'class_type': 'CFGGuider', 'inputs': {'model': ['1', 0], 'positive': positive, 'negative': negative, 'cfg': 1.0}})
    from lora_workflow import add_lora_chain
    graph['6']['inputs']['model'] = add_lora_chain(graph, ['1', 0], loras)
    return graph


async def run_flux2_image(prompt, *, model, references=(), size=(1024, 1024), seed=None, steps=None, guidance=None, loras=()):
    if model not in PRESETS:
        raise ValueError('Choose a supported FLUX.2 model.')
    preset = PRESETS[model]
    steps = preset['steps'] if steps is None else steps
    guidance = preset['guidance'] if guidance is None else guidance
    if not str(prompt).strip() or type(steps) is not int or not 1 <= steps <= (100 if model == 'flux2-dev' else 50):
        raise ValueError('Provide a prompt and supported FLUX.2 step count.')
    if not math.isfinite(guidance) or not 1 <= guidance <= 10 or model.startswith('flux2-klein-') and guidance != 1:
        raise ValueError('FLUX.2 Dev guidance is 1–10; distilled Klein uses guidance 1.')
    if len(references) > 4:
        raise ValueError('Use at most four FLUX.2 reference images.')
    info = await _object_info(); selected = flux2_model_option(info, model)
    if not selected['available']:
        raise Flux2ImageError(selected['reason'])
    from lora_workflow import available_loras
    loras = available_loras(info, loras)
    if references and any(node not in info for node in ('LoadImage', 'VAEEncode', 'ReferenceLatent')):
        raise Flux2ImageError('Update ComfyUI: reference editing requires LoadImage, VAEEncode, and ReferenceLatent.')
    size = qwen_canvas_size(size)
    try:
        with tempfile.TemporaryDirectory(prefix='local-image-flux2-') as temporary:
            normalized = []
            for index, path in enumerate(references):
                with Image.open(path) as image:
                    rgba = ImageOps.exif_transpose(image).convert('RGBA')
                rgb = Image.alpha_composite(Image.new('RGBA', rgba.size, 'white'), rgba).convert('RGB')
                scale = min(1.0, (1048576 / (rgb.width * rgb.height)) ** .5)
                target = qwen_canvas_size(tuple(max(32, int(value * scale)) for value in rgb.size))
                if rgb.size != target:
                    rgb = rgb.resize(target, Image.Resampling.LANCZOS)
                reference = Path(temporary) / ('reference-' + str(index + 1) + '.png'); rgb.save(reference)
                normalized.append(reference)
            graph = build_flux2_workflow(selected['files'], model=model, prompt=prompt, size=size,
                seed=secrets.randbits(48) if seed is None else seed, steps=steps, guidance=guidance, references=normalized, loras=loras)
            result = await _execute_workflow(graph)
        with Image.open(io.BytesIO(result)) as image:
            output = image.convert('RGB')
        if output.size != size:
            output = output.resize(size, Image.Resampling.LANCZOS)
        return output
    except Flux2ImageError:
        raise
    except Exception as error:
        detail = str(error)
        if 'out of memory' in detail.lower() or 'allocation' in detail.lower():
            detail = 'The GPU ran out of memory. Reduce output dimensions or reference count, or select a smaller model.'
        raise Flux2ImageError(preset['label'] + ' could not finish: ' + detail) from error
