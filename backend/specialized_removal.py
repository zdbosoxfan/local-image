"""Local, mask-limited FLUX.2 Klein object removal for RapidRAW.

Uses fal's Apache-2.0 object-removal LoRA and the author's red-outline input
format. Only the original brush selection is composited back into the photo.
"""
import asyncio
import base64
import io
import logging
import math
from pathlib import Path
import tempfile

from PIL import Image, ImageDraw
from removal_blend import clean_selection_mask, blend_patch

MODEL = 'flux-2-klein-base-4b.safetensors'
ENCODER = 'qwen_3_4b.safetensors'
VAE = 'flux2-vae.safetensors'
LORA = 'flux-2-klein-object-remove.safetensors'
PROMPT = 'Remove the highlighted object from the scene'
logger = logging.getLogger('Engine')


def build_removal_workflow(highlight_path, size, seed):
    width, height = size
    def node(kind, **inputs):
        return {'class_type': kind, 'inputs': inputs}
    return {
        '1': node('UNETLoader', unet_name=MODEL, weight_dtype='default'),
        '2': node('LoraLoaderModelOnly', model=['1', 0], lora_name=LORA, strength_model=1.1),
        '3': node('CLIPLoader', clip_name=ENCODER, type='flux2', device='default'),
        '4': node('VAELoader', vae_name=VAE),
        '5': node('CLIPTextEncode', clip=['3', 0], text=PROMPT),
        '6': node('CLIPTextEncode', clip=['3', 0], text=''),
        '30': node('LoadImage', image=str(highlight_path)),
        '8': node('VAEEncode', pixels=['30', 0], vae=['4', 0]),
        '9': node('ReferenceLatent', conditioning=['5', 0], latent=['8', 0]),
        '10': node('ReferenceLatent', conditioning=['6', 0], latent=['8', 0]),
        '11': node('CFGGuider', model=['2', 0], positive=['9', 0], negative=['10', 0], cfg=4.0),
        '12': node('RandomNoise', noise_seed=seed),
        '13': node('KSamplerSelect', sampler_name='euler'),
        '14': node('Flux2Scheduler', steps=28, width=width, height=height),
        '15': node('EmptyFlux2LatentImage', width=width, height=height, batch_size=1),
        '16': node('SamplerCustomAdvanced', noise=['12', 0], guider=['11', 0], sampler=['13', 0],
                   sigmas=['14', 0], latent_image=['15', 0]),
        '17': node('VAEDecode', samples=['16', 0], vae=['4', 0]),
        '41': node('PreviewImage', images=['17', 0]),
    }


def crop_box(image_size, bbox):
    w, h = image_size
    l, t, r, b = bbox
    # Include enough of the surroundings to infer texture and perspective.
    cw = min(w, max(768, round((r - l) * 2.5)))
    ch = min(h, max(768, round((b - t) * 2.0)))
    x = max(0, min(w - cw, (l + r - cw) // 2))
    y = max(0, min(h - ch, (t + b - ch) // 2))
    return (x, y, x + cw, y + ch)


def prepare_input(source, region_mask, box):
    crop = source.crop(box).convert('RGB')
    scale = min(1.0, math.sqrt(1536 * 1536 / (crop.width * crop.height)), 2048 / max(crop.size))
    size = tuple(max(16, round(v * scale / 16) * 16) for v in crop.size)
    highlighted = crop.resize(size, Image.Resampling.LANCZOS)
    bbox = region_mask.crop(box).getbbox()
    if bbox is None:
        raise ValueError('Cannot remove an empty selection')
    sx, sy = size[0] / crop.width, size[1] / crop.height
    l, t, r, b = bbox
    # Author examples use a thin red rectangle, leaving the object visible.
    rect = (max(0, round(l * sx) - 3), max(0, round(t * sy) - 3),
            min(size[0] - 1, round(r * sx) + 3), min(size[1] - 1, round(b * sy) + 3))
    ImageDraw.Draw(highlighted).rectangle(rect, outline=(255, 0, 0), width=3)
    return highlighted


async def run_removal(source_path: Path, mask: Image.Image, seed: int, plan: dict):
    from engine import ComfyClient, ImageProcessor, config, _diagnostics_status, analyze_mask, load_workflow
    original_mask = mask
    mask = await asyncio.to_thread(clean_selection_mask, mask)
    # Replan after cleaning so noise doesn't inflate crops or red guide bounds.
    plan = await asyncio.to_thread(analyze_mask, mask, load_workflow())
    with Image.open(source_path) as source_file:
        original = source_file.convert('RGB')
    current = original.copy()
    config.CACHE_DIR.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='remove_', dir=config.CACHE_DIR) as temporary:
        for index, region in enumerate(plan['regions']):
            region_mask = Image.new('L', original.size, 0)
            partition = tuple(region['partition'])
            region_mask.paste(mask.crop(partition), partition[:2])
            # Cleanup has removed distant noise. Keep the full remaining support
            # so deliberately faint strokes and broad soft tails are not omitted.
            box = crop_box(original.size, region['bbox'])
            highlighted = await asyncio.to_thread(prepare_input, current, region_mask, box)
            region['generation_size'] = list(highlighted.size)
            region['crop_box'] = list(box)
            _diagnostics_status(plan, 'running', active_region=index + 1, model='FLUX.2 Klein 4B Object Removal', blending='seamless')
            input_path = Path(temporary) / f'highlight_{index}.png'
            await asyncio.to_thread(highlighted.save, input_path)
            workflow = build_removal_workflow(input_path.absolute(), highlighted.size, (seed + index) % (2 ** 64))
            logger.info('Specialized removal: region %s/%s, size %s, FLUX.2 Klein base + fal LoRA',
                        index + 1, plan['region_count'], highlighted.size)
            generated_bytes = await ComfyClient().execute(workflow)
            with Image.open(io.BytesIO(generated_bytes)) as decoded:
                if decoded.size != highlighted.size:
                    raise ValueError('Removal model returned unexpected dimensions')
                generated = decoded.convert('RGB').resize((box[2] - box[0], box[3] - box[1]), Image.Resampling.LANCZOS)
            # Match the generated gradients to the surrounding photograph. This
            # does not mix the removed original object back into a tight mask.
            blended = await asyncio.to_thread(blend_patch, current.crop(box), generated, region_mask.crop(box))
            # Both clients apply mask opacity once to these unweighted colors.
            current.paste(blended, box[:2], region_mask.crop(box).point(lambda p: 255 if p else 0))
    full_bytes = io.BytesIO()
    current.save(full_bytes, format='PNG')
    mask_bytes = io.BytesIO()
    # RapidRAW retains its original mask and ignores the returned mask. Keep the
    # raw response extent, with original color at removed noise pixels, so it
    # cannot read transparent black where faint stray pixels used to be.
    original_mask.save(mask_bytes, format='PNG')
    response = ImageProcessor.crop_and_pack(full_bytes.getvalue(), mask_bytes.getvalue())
    x, y = response['x'], response['y']
    clean_bytes = io.BytesIO()
    mask.crop((x, y, x + response['width'], y + response['height'])).save(clean_bytes, format='PNG')
    response['mask'] = base64.b64encode(clean_bytes.getvalue()).decode('ascii')
    _diagnostics_status(plan, 'complete', active_region=plan['region_count'], model='FLUX.2 Klein 4B Object Removal', blending='seamless')
    return response
