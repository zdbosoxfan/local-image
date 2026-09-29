"""Native Qwen-Image 2.1 ComfyUI graphs, capability checks and alpha-safe output.

No weights are downloaded here. Loader names are taken from the running service,
so installations using extra_model_paths.yaml and model subfolders work too.
"""
import asyncio
import io
import re
import secrets
import tempfile
from pathlib import Path

import aiohttp
from PIL import Image, ImageOps

from app_paths import read_config

LICENSE_URL = 'https://github.com/QwenLM/Qwen-Image-2.1/blob/main/LICENSE'
NEGATIVE_PROMPT_NOTE = ('At guidance 1 the model ignores negative conditioning. '
                        'Empty-background instructions are also included in the main prompt.')
BACKGROUND_NEGATIVE = ('people, person, human, animal, product, foreground object, '
                       'subject, furniture in the foreground, text, logo, watermark')
BACKGROUND_INSTRUCTIONS = (
    'Create an empty photographic background plate for compositing. '
    'Leave the central foreground and supporting surface clear, with generous empty space '
    'for a subject to be placed later. Show only the environment, lighting and surfaces. '
    'Do not include people, animals, products, foreground subjects, text, logos or watermarks. '
    'Return a fully opaque image. Background description: '
)
MODEL_FILES = {
    'int8': {'unet': 'qwen_image_2.1_int8_convrot.safetensors',
             'clip': 'qwen3vl_8b_int8_convrot.safetensors',
             'vae': 'qwen_image_2.1_vae_bf16.safetensors'},
    'bf16': {'unet': 'qwen_image_2.1_bf16.safetensors',
             'clip': 'qwen3vl_8b_bf16.safetensors',
             'vae': 'qwen_image_2.1_vae_bf16.safetensors'},
}
REQUIRED_NODES = ('UNETLoader', 'CLIPLoader', 'VAELoader', 'TextEncodeQwenImage21',
                  'KSampler', 'VAEDecode', 'SaveImage', 'LoadImage',
                  'JoinImageWithAlpha', 'EmptyLatentImage')
LOADERS = {'unet': ('UNETLoader', 'unet_name'), 'clip': ('CLIPLoader', 'clip_name'),
           'vae': ('VAELoader', 'vae_name')}


class QwenImageError(RuntimeError):
    """An actionable connection, setup, generation, or alpha-validation failure."""


def _choices(info, node, field):
    inputs = info.get(node, {}).get('input', {})
    value = inputs.get('required', {}).get(field, inputs.get('optional', {}).get(field, []))
    if not isinstance(value, (list, tuple)) or not value:
        return []
    if isinstance(value[0], list):
        return value[0]
    # V3 nodes encode a combo as ['COMBO', {'options': [...]}]. Legacy
    # loaders instead put their options list directly in the first slot.
    if value[0] == 'COMBO' and len(value) > 1 and isinstance(value[1], dict):
        options = value[1].get('options', [])
        return options if isinstance(options, list) else []
    return []


def _normalized_name(name):
    return re.sub(r'[^a-z0-9]', '', str(name).replace('\\', '/').rsplit('/', 1)[-1].lower())


def qwen_canvas_size(size):
    """Fit to the native budget without rounding a boundary image over 4 MP."""
    width, height = map(int, size)
    if width <= 0 or height <= 0:
        raise ValueError('Image dimensions must be positive.')
    scale = min(1.0, (4194304 / (width * height)) ** .5, 4096 / max(width, height))
    bounds = tuple(max(32, int(value * scale) // 32 * 32) for value in (width, height))
    # Refit after alignment so flooring a short edge does not stretch a panorama.
    fit = min(bounds[0] / width, bounds[1] / height)
    return tuple(min(bound, max(32, round(value * fit / 32) * 32))
                 for value, bound in zip((width, height), bounds))


def qwen_model_options(object_info):
    """Resolve both precision presets against Comfy's actual loader inventory."""
    absent = [name for name in REQUIRED_NODES if name not in object_info]
    reason = ('Update ComfyUI: missing Qwen Image 2.1 workflow nodes: ' + ', '.join(absent)
              if absent else '')
    if not reason and 'qwen_image' not in _choices(object_info, 'CLIPLoader', 'type'):
        reason = 'Update ComfyUI: its CLIP loader does not support the qwen_image encoder type.'
    options = []
    for variant, expected in MODEL_FILES.items():
        files, missing = {}, []
        for key, filename in expected.items():
            choices = _choices(object_info, *LOADERS[key])
            match = next((name for name in choices if _normalized_name(name) == _normalized_name(filename)), None)
            if match is None:
                missing.append(filename)
            else:
                files[key] = match
        variant_reason = reason or ('Missing Qwen model files: ' + ', '.join(missing) +
                                   '. Install these in ComfyUI and refresh the connection.' if missing else '')
        options.append({'id': variant, 'label': 'Compact · INT8' if variant == 'int8' else 'Full precision · BF16',
                        'available': not variant_reason, 'reason': variant_reason, 'files': files,
                        'description': 'Same 7B image model; ' + ('smaller quantized weights.' if variant == 'int8' else 'larger full precision weights.')})
    return options


async def _object_info():
    port = read_config()['comfy_port']
    try:
        async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=15, connect=5)) as session:
            async with session.get(f'http://127.0.0.1:{port}/object_info') as response:
                response.raise_for_status()
                info = await response.json()
                if not isinstance(info, dict):
                    raise ValueError('Invalid node inventory')
                return info
    except (aiohttp.ClientError, asyncio.TimeoutError, ValueError) as exc:
        raise QwenImageError(f'Cannot reach ComfyUI on port {port}. Start ComfyUI with Qwen Image 2.1 installed, then reconnect.') from exc


async def get_qwen_status():
    try:
        variants = qwen_model_options(await _object_info())
        ready = any(item['available'] for item in variants)
        return {'connected': True, 'ready': ready, 'variants': variants,
                'reason': '' if ready else variants[0]['reason'], 'license': 'qwen-research',
                'license_url': LICENSE_URL, 'negative_prompt_note': NEGATIVE_PROMPT_NOTE}
    except QwenImageError as exc:
        variants = qwen_model_options({})
        for item in variants:
            item['reason'] = str(exc)
        return {'connected': False, 'ready': False, 'variants': variants, 'reason': str(exc),
                'license': 'qwen-research', 'license_url': LICENSE_URL,
                'negative_prompt_note': NEGATIVE_PROMPT_NOTE}


def build_qwen_workflow(models, *, prompt, negative_prompt='', references=(), size=(1024, 1024),
                        seed=0, steps=25, cfg=1.0, use_cache=True, loras=()):
    """Build an API prompt, with flattened V3 Autogrow input names.

    Editing must sample the encoder's latent output: forcing a different canvas
    changes alignment. EmptyLatentImage is used only for text-to-image.
    """
    width, height = qwen_canvas_size(size)
    graph = {
        '1': {'class_type': 'UNETLoader', 'inputs': {'unet_name': models['unet'], 'weight_dtype': 'default'}},
        '2': {'class_type': 'CLIPLoader', 'inputs': {'clip_name': models['clip'], 'type': 'qwen_image', 'device': 'default'}},
        '3': {'class_type': 'VAELoader', 'inputs': {'vae_name': models['vae']}},
        '4': {'class_type': 'TextEncodeQwenImage21', 'inputs': {'clip': ['2', 0], 'prompt': prompt,
              'negative_prompt': negative_prompt, 'resolution': 0 if references else 1024}},
        '6': {'class_type': 'KSampler', 'inputs': {'model': ['1', 0], 'seed': int(seed), 'steps': int(steps),
              'cfg': float(cfg), 'sampler_name': 'euler', 'scheduler': 'simple', 'denoise': 1.0,
              'positive': ['4', 0], 'negative': ['4', 1], 'latent_image': ['4', 2] if references else ['5', 0]}},
        '7': {'class_type': 'VAEDecode', 'inputs': {'samples': ['6', 0], 'vae': ['3', 0]}},
        '8': {'class_type': 'SaveImage', 'inputs': {'images': ['7', 0], 'filename_prefix': 'LocalRemove_Qwen21'}},
    }
    from lora_workflow import add_lora_chain
    model_link = add_lora_chain(graph, ['1', 0], loras)
    graph['6']['inputs']['model'] = model_link
    if use_cache:
        graph['9'] = {'class_type': 'QwenImage21Cache', 'inputs': {'model': model_link, 'device': 'auto', 'dtype': 'default'}}
        graph['6']['inputs']['model'] = ['9', 0]
    if references:
        graph['4']['inputs']['vae'] = ['3', 0]
        for index, path in enumerate(references, 1):
            node_id, alpha_id = str(20 + index * 2), str(21 + index * 2)
            graph[node_id] = {'class_type': 'LoadImage', 'inputs': {'image': str(path)}}
            # LoadImage MASK is inverted alpha; JoinImageWithAlpha expects that convention.
            graph[alpha_id] = {'class_type': 'JoinImageWithAlpha', 'inputs': {'image': [node_id, 0], 'alpha': [node_id, 1]}}
            graph['4']['inputs'][f'images.image_{index}'] = [alpha_id, 0]
    else:
        graph['5'] = {'class_type': 'EmptyLatentImage', 'inputs': {'width': width, 'height': height, 'batch_size': 1}}
    return graph


def validate_cutout(image):
    if 'A' not in image.getbands():
        raise QwenImageError('Qwen returned an opaque image instead of a cutout. Retry background removal and verify the Qwen Image 2.1 RGBA VAE is selected.')
    low, high = image.getchannel('A').getextrema()
    if low >= 250 or high <= 5:
        raise QwenImageError('Qwen did not return a usable transparent cutout. Retry with a different seed or a more specific subject instruction.')


def finish_output_alpha(image, task):
    """Preserve RGBA edges and composite opaque output without exposing hidden RGB."""
    if task == 'cutout':
        alpha = image.getchannel('A').point([0 if v <= 1 else 255 if v >= 254 else v for v in range(256)])
        image.putalpha(alpha)
    elif task in ('background', 'opaque'):
        image = image.convert('RGBA')
        alpha = image.getchannel('A')
        histogram = alpha.histogram()
        if alpha.getextrema()[1] <= 5:
            raise QwenImageError('Qwen returned an empty transparent image. Retry with a different seed or a more specific prompt.')
        if task == 'background' and sum(histogram[:250]) > image.width * image.height * .01:
            raise QwenImageError('Qwen returned a transparent subject instead of a complete background scene. Retry with a different seed or describe the empty environment more specifically.')
        # Native RGBA may contain arbitrary colors in invisible pixels. Setting
        # alpha to 255 reveals those colors; composite onto the documented white
        # matte instead. Only remove one-level opaque endpoint noise first.
        image.putalpha(alpha.point([255 if v >= 254 else v for v in range(256)]))
        image = Image.alpha_composite(Image.new('RGBA', image.size, 'white'), image)
    return image


async def _execute_workflow(workflow):
    from engine import config
    from local_comfy_client import LocalComfyClient
    config.COMFY_HOST = '127.0.0.1'
    config.COMFY_PORT = read_config()['comfy_port']
    return await LocalComfyClient().execute(workflow)


async def run_qwen_image(input_path=None, prompt='', variant='int8', size=(1024, 1024), seed=None,
                         task='cutout', steps=25, negative_prompt='', cfg=1.0, reference_paths=(), transparent=False, loras=()):
    """Return a detached PIL image; cutouts are RGBA with actual transparency.

    Editing returns the source's original dimensions. No source is overwritten.
    Output color may change during generative extraction; callers can keep source
    RGB and use only its returned alpha when product fidelity matters.
    """
    if variant not in MODEL_FILES:
        raise ValueError('Choose the int8 or bf16 Qwen model variant.')
    if task not in ('cutout', 'background', 'edit', 'generate'):
        raise ValueError('Choose cutout, background, edit, or generate.')
    if task not in ('background', 'generate') and input_path is None:
        raise ValueError('This Qwen operation needs an input image.')
    if transparent and task != 'generate':
        raise ValueError('The transparent option belongs to image generation.')
    if not 1 <= int(steps) <= 100 or not 1 <= float(cfg) <= 10:
        raise ValueError('Use 1–100 steps and guidance between 1 and 10.')
    if len(reference_paths) + (input_path is not None) > 10:
        raise ValueError('Qwen editing supports up to ten reference images.')
    if len(size) != 2 or any(int(v) < 32 or int(v) > 4096 for v in size) or int(size[0]) * int(size[1]) > 4194304:
        raise ValueError('Choose an image size up to 4 megapixels and 4096 pixels per side.')
    info = await _object_info()
    chosen = next(item for item in qwen_model_options(info) if item['id'] == variant)
    if not chosen['available']:
        raise QwenImageError(chosen['reason'])
    from lora_workflow import available_loras
    loras = available_loras(info, loras)
    prompt = str(prompt).strip()
    if task == 'background':
        prompt = BACKGROUND_INSTRUCTIONS + (prompt or 'A simple studio cyclorama with soft natural light.')
        negative_prompt = ', '.join(filter(None, [BACKGROUND_NEGATIVE, negative_prompt]))
    elif task == 'cutout':
        prompt = ('Remove the background from <image1> and preserve the subject exactly. '
                  'Return an RGBA PNG with a transparent background and a real alpha channel. '
                  'Keep the original subject position, scale, details and colors. ' + prompt)
    elif not prompt:
        raise ValueError('Describe the image edit to perform.')
    if task == 'generate':
        prompt += ('\nCreate the subject on a fully transparent background with real RGBA alpha. '
                   'Do not draw a checkerboard or an opaque backdrop. Preserve fine subject edges.'
                   if transparent else '\nReturn a fully opaque image with a complete background.')
    references = ([Path(input_path)] if input_path is not None else []) + [Path(p) for p in reference_paths]
    original_size = None
    try:
        with tempfile.TemporaryDirectory(prefix='local-remove-qwen-') as temporary:
            normalized = []
            for index, path in enumerate(references):
                with Image.open(path) as opened:
                    image = ImageOps.exif_transpose(opened).convert('RGBA')
                if index == 0 and task != 'generate':
                    original_size = image.size
                # Cap reference area to 2K while preserving its aspect ratio.
                target = qwen_canvas_size(image.size)
                if image.size != target:
                    image = image.resize(target, Image.Resampling.LANCZOS)
                if index == 0 and task == 'generate':
                    # The native encoder derives its edit latent from reference 1.
                    # Padding keeps all reference content while honoring the chosen canvas.
                    image = ImageOps.pad(image, qwen_canvas_size(size), method=Image.Resampling.LANCZOS, color=(0, 0, 0, 0))
                destination = Path(temporary) / f'reference-{index}.png'
                image.save(destination)
                normalized.append(destination)
            workflow = build_qwen_workflow(chosen['files'], prompt=prompt, negative_prompt=negative_prompt,
                references=normalized, size=size, seed=secrets.randbits(48) if seed is None else seed,
                steps=steps, cfg=cfg, use_cache='QwenImage21Cache' in info, loras=loras)
            data = await _execute_workflow(workflow)
        with Image.open(io.BytesIO(data)) as result:
            if task == 'cutout' or task == 'generate' and transparent:
                validate_cutout(result)
            image = result.convert('RGBA')
        if original_size and image.size != original_size:
            image = image.resize(original_size, Image.Resampling.LANCZOS)
        if task == 'generate':
            if image.size != qwen_canvas_size(size):
                image = image.resize(qwen_canvas_size(size), Image.Resampling.LANCZOS)
            return finish_output_alpha(image, 'cutout' if transparent else 'opaque')
        return finish_output_alpha(image, task)
    except QwenImageError:
        raise
    except Exception as exc:
        detail = str(exc)
        if 'out of memory' in detail.lower() or 'allocation' in detail.lower():
            detail = 'The GPU ran out of memory. Choose Compact INT8, reduce image size, or close other GPU jobs.'
        raise QwenImageError('Qwen Image 2.1 could not finish: ' + detail) from exc


async def run_qwen_removal(input_path, mask_path, prompt='', variant='int8', seed=None, steps=25):
    """Instruction-guided removal, composited through the user selection.

    The separate mask reference helps locate the edit. Deterministic compositing
    guarantees that pixels where the selection is zero remain exactly unchanged.
    """
    with Image.open(input_path) as opened:
        source = ImageOps.exif_transpose(opened).convert('RGBA')
    with Image.open(mask_path) as opened:
        mask = opened.convert('L')
    if mask.size != source.size:
        mask = mask.resize(source.size, Image.Resampling.NEAREST)
    if not mask.getbbox():
        return source
    with tempfile.TemporaryDirectory(prefix='local-remove-qwen-mask-') as temporary:
        reference = Path(temporary) / 'selection.png'
        mask.convert('RGB').save(reference)
        instruction = (
            '<image1> is the photograph to edit. <image2> is a black and white selection mask: '
            'white marks the unwanted object or area to remove and black marks the area to keep. '
            'Remove the selected object completely and reconstruct a natural empty continuation '
            'of the surrounding background in its place. Match the surrounding texture, perspective, '
            'lighting and color. Do not insert any new objects, people, text or symbols. '
            'Do not draw the mask in the result. Keep the original composition and all other areas unchanged. '
            + str(prompt).strip())
        edited = await run_qwen_image(input_path, instruction, variant=variant, seed=seed,
                                      task='edit', steps=steps, reference_paths=[reference])
    if edited.size != source.size:
        edited = edited.resize(source.size, Image.Resampling.LANCZOS)
    return Image.composite(edited.convert('RGBA'), source, mask)
