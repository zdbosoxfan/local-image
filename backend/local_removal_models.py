"""Standalone Local Remove model choices; RapidRAW keeps its existing route."""
import asyncio
import io
import logging
import math
import os
from pathlib import Path
import tempfile

from PIL import Image, ImageDraw
import numpy as np

from removal_blend import clean_selection_mask, blend_patch
from qwen_degrid import degrid_qwen
from app_paths import model_directory

logger = logging.getLogger('Engine')
QWEN_MODEL = 'qwen_image_edit_2511_fp8mixed.safetensors'
QWEN_ENCODER = 'qwen_2.5_vl_7b_fp8_scaled.safetensors'
QWEN_VAE = 'qwen_image_vae.safetensors'
QWEN_LORA = 'Qwen-Image-Edit-2511-Object-Remover-v2-9200.safetensors'
QWEN_PIXEL_BUDGET = 960**2
QWEN_MAX_EDGE = 1472
QWEN_PROMPT = ('Remove the red highlighted object from the scene. '
               'Completely erase all objects and their shadows inside the red rectangle, '
               'and fill the marked area naturally from the surrounding scene. '
               'Remove the red rectangle too.')
FILES = {
    'klein': [('diffusion_models', 'flux-2-klein-base-4b.safetensors'),
              ('text_encoders', 'qwen_3_4b.safetensors'), ('vae', 'flux2-vae.safetensors'),
              ('loras', 'flux-2-klein-object-remove.safetensors')],
    'qwen': [('diffusion_models', QWEN_MODEL), ('text_encoders', QWEN_ENCODER),
             ('vae', QWEN_VAE), ('loras', QWEN_LORA)],
}


def model_options():
    choices = [
        {'id': 'klein', 'label': 'FLUX Klein',
         'description': 'Fast, removal-trained model with automatic edge blending.'},
        {'id': 'qwen', 'label': 'Qwen removal',
         'description': 'Experimental removal model. Slower; may introduce artificial texture or alter details.'},
    ]
    for choice in choices:
        missing = [name for folder, name in FILES[choice['id']] if not (model_directory() / folder / name).is_file()]
        choice['available'] = not missing
        if missing:
            choice['reason'] = choice['label'] + ' model files are not installed. Choose another model.'
    return choices


def build_qwen_workflow(path, seed):
    def node(kind, **inputs):
        return {'class_type': kind, 'inputs': inputs}
    return {
        '1': node('UNETLoader', unet_name=QWEN_MODEL, weight_dtype='default'),
        '2': node('LoraLoaderModelOnly', model=['1', 0], lora_name=QWEN_LORA, strength_model=1.0),
        '3': node('CLIPLoader', clip_name=QWEN_ENCODER, type='qwen_image', device='default'),
        '4': node('VAELoader', vae_name=QWEN_VAE),
        '5': node('TextEncodeQwenImageEditPlus', clip=['3', 0], prompt=QWEN_PROMPT, vae=['4', 0], image1=['30', 0]),
        '6': node('TextEncodeQwenImageEditPlus', clip=['3', 0], prompt='', vae=['4', 0], image1=['30', 0]),
        '7': node('FluxKontextMultiReferenceLatentMethod', conditioning=['5', 0], reference_latents_method='index_timestep_zero'),
        '8': node('FluxKontextMultiReferenceLatentMethod', conditioning=['6', 0], reference_latents_method='index_timestep_zero'),
        '9': node('ModelSamplingAuraFlow', model=['2', 0], shift=3.1),
        '10': node('CFGNorm', model=['9', 0], strength=1.0, pre_cfg=False),
        '11': node('VAEEncode', pixels=['30', 0], vae=['4', 0]),
        '12': node('KSampler', model=['10', 0], seed=seed, steps=40, cfg=4.0,
                   sampler_name='euler', scheduler='simple', positive=['7', 0], negative=['8', 0],
                   latent_image=['11', 0], denoise=1.0),
        '13': node('VAEDecode', samples=['12', 0], vae=['4', 0]),
        '30': node('LoadImage', image=str(path)),
        '41': node('PreviewImage', images=['13', 0]),
    }


def prepare_qwen_input(source, mask, box):
    crop = source.crop(box).convert('RGB')
    # Qwen is substantially larger: keep the native official workflow's ~1MP
    # budget to leave room for its removal adapter on the 32GB GPU.
    scale = min(1.0, math.sqrt(QWEN_PIXEL_BUDGET / (crop.width * crop.height)), QWEN_MAX_EDGE / max(crop.size))
    size = tuple(max(16, round(value * scale / 16) * 16) for value in crop.size)
    # Keep a closed, visible guide even when the selection reaches a photo edge.
    # The border is conditioning context only and is removed from the result.
    margin = 32
    pixels = np.asarray(crop.resize(size, Image.Resampling.LANCZOS))
    marked = Image.fromarray(np.pad(pixels, ((margin, margin), (margin, margin), (0, 0)), mode='edge'))
    marked.info['content_box'] = (margin, margin, margin + size[0], margin + size[1])
    bbox = mask.crop(box).getbbox()
    if not bbox:
        raise ValueError('Select an object first.')
    left, top, right, bottom = bbox
    sx, sy = size[0] / crop.width, size[1] / crop.height
    # Author examples use a thin red rectangle, leaving the object visible.
    rectangle = (margin + round(left*sx)-6, margin + round(top*sy)-6,
                 margin + round(right*sx)+6, margin + round(bottom*sy)+6)
    ImageDraw.Draw(marked).rectangle(rectangle, outline=(255, 0, 0), width=6)
    return marked


def qwen_crop_box(image_size, bbox):
    # More balanced context keeps small objects legible to Qwen's vision encoder.
    width, height = image_size
    left, top, right, bottom = bbox
    crop_width = min(width, max(768, round((right-left) * 1.8)))
    crop_height = min(height, max(768, round((bottom-top) * 2.0)))
    x = max(0, min(width-crop_width, (left+right-crop_width)//2))
    y = max(0, min(height-crop_height, (top+bottom-crop_height)//2))
    return x, y, x+crop_width, y+crop_height


async def run_local_removal(source_path, mask_bytes, seed, model):
    from engine import run_inpaint, ImageProcessor, analyze_mask, load_workflow, config, _diagnostics_status
    from local_comfy_client import LocalComfyClient
    if model not in FILES:
        raise ValueError('Choose a supported removal model.')
    option = next(x for x in model_options() if x['id'] == model)
    if not option['available']:
        raise ValueError(option['reason'])
    if model == 'klein':
        return await run_inpaint(source_path, mask_bytes, '', '', seed)

    with Image.open(source_path) as source:
        current = source.convert('RGB')
    with Image.open(io.BytesIO(mask_bytes)) as received:
        mask = received.convert('L')
    if mask.size != current.size:
        raise ValueError('Selection dimensions must match the photo.')
    mask = await asyncio.to_thread(clean_selection_mask, mask)
    plan = await asyncio.to_thread(analyze_mask, mask, load_workflow())
    config.CACHE_DIR.mkdir(parents=True, exist_ok=True)
    try:
        with tempfile.TemporaryDirectory(prefix='qwen_remove_', dir=config.CACHE_DIR) as temporary:
            for index, region in enumerate(plan['regions']):
                partition = tuple(region['partition'])
                region_mask = Image.new('L', current.size)
                region_mask.paste(mask.crop(partition), partition[:2])
                box = qwen_crop_box(current.size, region['bbox'])
                marked = await asyncio.to_thread(prepare_qwen_input, current, region_mask, box)
                region['generation_size'] = list(marked.size)
                region['crop_box'] = list(box)
                _diagnostics_status(plan, 'running', active_region=index+1, model='Qwen removal', blending='seamless')
                path = Path(temporary) / f'highlight_{index}.png'
                await asyncio.to_thread(marked.save, path)
                logger.info('Qwen removal: region %s/%s, size %s, 40 steps', index+1, plan['region_count'], marked.size)
                generated_bytes = await LocalComfyClient().execute(build_qwen_workflow(path.absolute(), (seed+index) % 2**64))
                with Image.open(io.BytesIO(generated_bytes)) as decoded:
                    if decoded.size != marked.size:
                        raise ValueError('Qwen returned unexpected image dimensions.')
                    cleaned, grid_stats = await asyncio.to_thread(degrid_qwen, decoded.convert('RGB'))
                    region['decoder_grid'] = grid_stats
                    logger.info('Qwen decoder grid: %.3f/255, cleanup %s', grid_stats['amp_255'], 'skipped' if grid_stats['skipped'] else 'applied')
                    generated = cleaned.crop(marked.info['content_box']).resize((box[2]-box[0], box[3]-box[1]), Image.Resampling.LANCZOS)
                blended = await asyncio.to_thread(blend_patch, current.crop(box), generated, region_mask.crop(box))
                current.paste(blended, box[:2], region_mask.crop(box).point(lambda v: 255 if v else 0))
        image_bytes, selection_bytes = io.BytesIO(), io.BytesIO()
        current.save(image_bytes, format='PNG'); mask.save(selection_bytes, format='PNG')
        result = ImageProcessor.crop_and_pack(image_bytes.getvalue(), selection_bytes.getvalue())
        _diagnostics_status(plan, 'complete', active_region=plan['region_count'], model='Qwen removal', blending='seamless')
        return result
    except Exception as error:
        _diagnostics_status(plan, 'error', model='Qwen removal', error=str(error))
        raise
