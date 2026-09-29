"""Native HiDream O1 Full graph with pixel-space references and seam smoothing.

Sampling follows Comfy-Org's image_hidream_o1 template. The Full publisher
recommendation is 50 steps; it is distinct from the CFG-1 Dev distillation.
"""
import io
import math
from pathlib import Path
import secrets
import tempfile

from PIL import Image, ImageOps

from qwen_image import _object_info, _choices, _normalized_name, _execute_workflow, qwen_canvas_size

CHECKPOINT = 'hidream_o1_image_fp8_scaled.safetensors'
REQUIRED_NODES = ('CheckpointLoaderSimple', 'CLIPTextEncode', 'ModelNoiseScale',
                  'HiDreamO1PatchSeamSmoothing', 'EmptyHiDreamO1LatentImage',
                  'BasicScheduler', 'KSamplerSelect', 'SamplerCustom', 'VAEDecode', 'SaveImage')


class HiDreamImageError(RuntimeError):
    pass


def hidream_model_option(info):
    missing = [name for name in REQUIRED_NODES if name not in info]
    reason = 'Update ComfyUI: missing HiDream O1 nodes: ' + ', '.join(missing) if missing else ''
    if not reason and 'dpmpp_2m_sde_gpu' not in _choices(info, 'KSamplerSelect', 'sampler_name'):
        reason = 'Update ComfyUI: the HiDream O1 sampler is unavailable.'
    if not reason and 'normal' not in _choices(info, 'BasicScheduler', 'scheduler'):
        reason = 'Update ComfyUI: the normal scheduler is unavailable.'
    match = next((name for name in _choices(info, 'CheckpointLoaderSimple', 'ckpt_name')
                  if _normalized_name(name) == _normalized_name(CHECKPOINT)), None)
    reason = reason or ('' if match else 'Missing HiDream O1 Full FP8 checkpoint. Install the preset and refresh.')
    return {'id': 'fp8', 'label': 'Full model · FP8', 'available': not reason, 'reason': reason,
            'files': {'checkpoint': match} if match else {}}


def build_hidream_workflow(models, *, prompt, negative_prompt='', size=(2048, 2048),
                           seed=0, steps=50, guidance=5.0, references=(), loras=()):
    width, height = qwen_canvas_size(size)
    graph = {
        '1': {'class_type': 'CheckpointLoaderSimple', 'inputs': {'ckpt_name': models['checkpoint']}},
        '2': {'class_type': 'CLIPTextEncode', 'inputs': {'clip': ['1', 1], 'text': prompt}},
        '3': {'class_type': 'CLIPTextEncode', 'inputs': {'clip': ['1', 1], 'text': negative_prompt}},
        '4': {'class_type': 'ModelNoiseScale', 'inputs': {'model': ['1', 0], 'noise_scale': 8.0}},
        '5': {'class_type': 'HiDreamO1PatchSeamSmoothing', 'inputs': {'model': ['4', 0],
              'start_percent': 0.8, 'end_percent': 1.0, 'pattern': 'single_shift',
              'passes': 'ramp_2_4', 'blend': 'median', 'strength': 1.0}},
        '6': {'class_type': 'EmptyHiDreamO1LatentImage', 'inputs': {'width': width, 'height': height, 'batch_size': 1}},
        '7': {'class_type': 'BasicScheduler', 'inputs': {'model': ['5', 0], 'scheduler': 'normal', 'steps': steps, 'denoise': 1.0}},
        '8': {'class_type': 'KSamplerSelect', 'inputs': {'sampler_name': 'dpmpp_2m_sde_gpu'}},
        '9': {'class_type': 'SamplerCustom', 'inputs': {'model': ['5', 0], 'add_noise': True,
              'noise_seed': int(seed), 'cfg': guidance, 'positive': ['2', 0], 'negative': ['3', 0],
              'sampler': ['8', 0], 'sigmas': ['7', 0], 'latent_image': ['6', 0]}},
        '10': {'class_type': 'VAEDecode', 'inputs': {'samples': ['9', 0], 'vae': ['1', 2]}},
        '11': {'class_type': 'SaveImage', 'inputs': {'images': ['10', 0], 'filename_prefix': 'LocalImage_HiDreamO1'}},
    }
    from lora_workflow import add_lora_chain
    graph['4']['inputs']['model'] = add_lora_chain(graph, ['1', 0], loras)
    if references:
        inputs = {'positive': ['2', 0], 'negative': ['3', 0]}
        for index, reference in enumerate(references):
            node = str(20 + index)
            graph[node] = {'class_type': 'LoadImage', 'inputs': {'image': str(reference)}}
            inputs['images.image_' + str(index + 1)] = [node, 0]
        graph['12'] = {'class_type': 'HiDreamO1ReferenceImages', 'inputs': inputs}
        graph['9']['inputs'].update(positive=['12', 0], negative=['12', 1])
    return graph


async def run_hidream_image(prompt, *, negative_prompt='', references=(), size=(2048, 2048),
                            seed=None, steps=50, guidance=5.0, loras=()):
    if not str(prompt).strip() or type(steps) is not int or not 1 <= steps <= 100:
        raise ValueError('Provide a prompt and between 1 and 100 HiDream O1 steps.')
    if not math.isfinite(guidance) or not 1 <= guidance <= 10 or len(references) > 10:
        raise ValueError('HiDream O1 uses guidance 1–10 and up to ten reference images.')
    info = await _object_info()
    selected = hidream_model_option(info)
    if not selected['available']:
        raise HiDreamImageError(selected['reason'])
    if references and any(node not in info for node in ('LoadImage', 'HiDreamO1ReferenceImages')):
        raise HiDreamImageError('Update ComfyUI: HiDream reference editing nodes are unavailable.')
    from lora_workflow import available_loras
    loras = available_loras(info, loras)
    size = qwen_canvas_size(size)
    try:
        with tempfile.TemporaryDirectory(prefix='local-image-hidream-') as temporary:
            normalized = []
            for index, path in enumerate(references):
                with Image.open(path) as image:
                    rgba = ImageOps.exif_transpose(image).convert('RGBA')
                rgb = Image.alpha_composite(Image.new('RGBA', rgba.size, 'white'), rgba).convert('RGB')
                target = qwen_canvas_size(rgb.size)
                if rgb.size != target:
                    rgb = rgb.resize(target, Image.Resampling.LANCZOS)
                reference = Path(temporary) / ('reference-' + str(index + 1) + '.png')
                rgb.save(reference)
                normalized.append(reference)
            graph = build_hidream_workflow(selected['files'], prompt=prompt, negative_prompt=negative_prompt,
                size=size, seed=secrets.randbits(48) if seed is None else seed, steps=steps,
                guidance=guidance, references=normalized, loras=loras)
            result = await _execute_workflow(graph)
        with Image.open(io.BytesIO(result)) as image:
            output = image.convert('RGB')
        if output.size != size:
            raise HiDreamImageError('HiDream returned unexpected dimensions. Refresh ComfyUI and retry.')
        return output
    except HiDreamImageError:
        raise
    except Exception as error:
        detail = str(error)
        if 'out of memory' in detail.lower() or 'allocation' in detail.lower():
            detail = 'The GPU ran out of memory. Reduce reference count or canvas size; smaller canvases may reduce HiDream quality.'
        raise HiDreamImageError('HiDream O1 could not finish: ' + detail) from error
