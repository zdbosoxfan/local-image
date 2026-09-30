"""Capability-driven local image generation with guarded session references."""
import asyncio
from contextlib import AsyncExitStack
from pathlib import Path
import secrets
import tempfile
from typing import Literal
import uuid

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel, ConfigDict, Field, model_validator

import local_remove as editor
import qwen_image
import z_image
import flux2_image
import hidream_image
import ernie_image
from generation_metadata import validate_generation_metadata, validate_lora_metadata, VARIANTS
from generation_model_details import enrich_model
from app_paths import model_directory
from stock_attribution import collect_attributions, unique_attributions
from comfy_inventory import read_inventory
from operation_progress import operation, snapshot as progress_snapshot

router = APIRouter(prefix='/api/local-remove/generation')
MODEL_DEFAULTS = {
    'qwen': {'variant': 'int8', 'steps': 25, 'guidance': 1.0},
    'z-image-turbo': {'variant': 'bf16', 'steps': 8, 'guidance': 1.0},
    'flux2-dev': {'variant': 'fp8', 'steps': 20, 'guidance': 4.0},
    'flux2-klein-4b': {'variant': 'bf16', 'steps': 4, 'guidance': 1.0},
    'flux2-klein-9b': {'variant': 'fp8', 'steps': 4, 'guidance': 1.0},
    'hidream-o1': {'variant': 'fp8', 'steps': 50, 'guidance': 5.0},
    'ernie-image': {'variant': 'bf16', 'steps': 50, 'guidance': 4.0},
}


class LoraSelection(BaseModel):
    model_config = ConfigDict(extra='forbid', strict=True)
    id: str = Field(min_length=24, max_length=24, pattern=r'^[a-f0-9]{24}$')
    strength: float = Field(default=1.0, ge=-2, le=2, allow_inf_nan=False)


class GenerationRequest(BaseModel):
    model_config = ConfigDict(extra='forbid', strict=True)
    model: Literal['qwen', 'z-image-turbo', 'flux2-dev', 'flux2-klein-4b', 'flux2-klein-9b', 'hidream-o1', 'ernie-image'] = 'qwen'
    variant: Literal['int8', 'bf16', 'fp8'] | None = None
    prompt: str = Field(min_length=1, max_length=4000)
    negative_prompt: str = Field(default='', max_length=2000)
    width: int = Field(default=1024, ge=256, le=4096, multiple_of=32)
    height: int = Field(default=1024, ge=256, le=4096, multiple_of=32)
    seed: int | None = Field(default=None, ge=0, le=2**53-1)
    transparent: bool = False
    reference_session_ids: list[str] = Field(default_factory=list, max_length=10)
    steps: int | None = Field(default=None, ge=1, le=100)
    guidance: float | None = Field(default=None, ge=1, le=10, allow_inf_nan=False)
    denoise: float | None = Field(default=None, ge=0.05, le=1, allow_inf_nan=False)
    loras: list[LoraSelection] = Field(default_factory=list, max_length=3)

    @model_validator(mode='before')
    @classmethod
    def model_canvas_defaults(cls, values):
        if isinstance(values, dict) and values.get('model') == 'hidream-o1':
            return {'width': 2048, 'height': 2048, **values}
        return values

    @model_validator(mode='after')
    def valid_model_inputs(self):
        if not self.prompt.strip():
            raise ValueError('Describe the image to generate.')
        if self.width * self.height > 4194304:
            raise ValueError('Choose an output size up to 4 megapixels.')
        if self.variant is not None and self.variant not in VARIANTS[self.model]:
            raise ValueError('Choose a precision preset supported by this model.')
        validate_lora_metadata([item.model_dump() for item in self.loras])
        if self.model == 'ernie-image' and (self.reference_session_ids or self.transparent or self.loras):
            raise ValueError('ERNIE-Image Base supports opaque text-to-image generation without references or adapters in this preset.')
        if len(set(self.reference_session_ids)) != len(self.reference_session_ids):
            raise ValueError('Add each reference image only once.')
        for reference in self.reference_session_ids:
            try:
                if str(uuid.UUID(reference)) != reference:
                    raise ValueError()
            except (ValueError, TypeError):
                raise ValueError('Choose reference images already imported into Local Image.')
        if self.model == 'z-image-turbo':
            if self.variant not in (None, 'bf16'):
                raise ValueError('Z-Image Turbo uses the BF16 preset.')
            if self.transparent:
                raise ValueError('Choose Qwen Image 2.1 for transparent generation.')
            if self.negative_prompt.strip():
                raise ValueError('Z-Image Turbo does not use a negative prompt at guidance 1.')
            if len(self.reference_session_ids) > 1:
                raise ValueError('Z-Image Turbo accepts one starting image for a variation.')
            if self.guidance not in (None, 1) or self.steps is not None and self.steps > 50:
                raise ValueError('Z-Image Turbo uses guidance 1 and between 1 and 50 steps.')
            if not self.reference_session_ids and self.denoise not in (None, 1):
                raise ValueError('Variation strength requires a starting image.')
        else:
            if self.denoise is not None:
                raise ValueError('This model uses image references and instructions, rather than variation strength.')
            if self.model == 'hidream-o1' and self.transparent:
                raise ValueError('HiDream O1 produces opaque output. Choose Qwen for transparent generation.')
            if self.model.startswith('flux2-'):
                if self.transparent or self.negative_prompt.strip():
                    raise ValueError('FLUX.2 generates opaque images and does not use a negative prompt.')
                if len(self.reference_session_ids) > 4:
                    raise ValueError('Use at most four FLUX.2 reference images.')
                if self.model.startswith('flux2-klein-') and (self.guidance not in (None, 1) or self.steps is not None and self.steps > 50):
                    raise ValueError('Distilled FLUX.2 Klein uses guidance 1 and between 1 and 50 steps.')
        return self


def model_inventory(info, *, connected=True, connection_reason=''):
    qwen_variants = qwen_image.qwen_model_options(info)
    z_variants = [z_image.z_image_model_option(info)]
    dimensions = {'min': 256, 'max': 4096, 'step': 32, 'default_width': 1024, 'default_height': 1024, 'max_pixels': 4194304}
    models = []
    entries = [('qwen', 'Qwen Image 2.1', 'edits & transparent assets', qwen_variants),
               ('z-image-turbo', 'Z-Image Turbo', 'fast image generation', z_variants),
               ('flux2-klein-4b', 'FLUX.2 Klein 4B', 'fast reference editing', [flux2_image.flux2_model_option(info, 'flux2-klein-4b')]),
               ('flux2-klein-9b', 'FLUX.2 Klein 9B', 'richer detail & reference editing', [flux2_image.flux2_model_option(info, 'flux2-klein-9b')]),
               ('ernie-image', 'ERNIE-Image', 'posters & text layouts', [ernie_image.ernie_model_option(info)])]
    for model, label, benefit, variants in entries:
        is_qwen = model == 'qwen'; is_z = model == 'z-image-turbo'
        is_hidream = model == 'hidream-o1'
        is_ernie = model == 'ernie-image'
        default_side = 2048 if is_hidream else 1024
        variable_guidance = model in ('qwen', 'flux2-dev', 'hidream-o1', 'ernie-image')
        if not connected:
            for variant in variants:
                variant.update(available=False, reason=connection_reason)
        available = any(item['available'] for item in variants)
        loras_available = not is_ernie and connected and 'LoraLoaderModelOnly' in info
        models.append({'id': model, 'label': label, 'benefit': benefit, 'available': available,
            'reason': '' if available else variants[0]['reason'], 'variants': variants,
            'capabilities': {'text_to_image': True, 'image_reference': not is_z and not is_ernie, 'image_to_image': not is_ernie,
                'references': not is_ernie, 'reference_mode': None if is_ernie else 'init' if is_z else 'semantic',
                'transparent': is_qwen, 'max_references': 0 if is_ernie else 10 if is_qwen or is_hidream else 1 if is_z else 4,
                'negative_prompt': is_qwen or is_hidream or is_ernie, 'denoise': is_z, 'loras': loras_available, 'lora': loras_available},
            'dimensions': {**dimensions, 'default_width': default_side, 'default_height': default_side},
            'defaults': {**MODEL_DEFAULTS[model], 'width': default_side, 'height': default_side, 'denoise': 0.6 if is_z else None},
            'limits': {'min_dimension': 256, 'max_dimension': 4096, 'dimension_step': 32, 'max_pixels': 4194304,
                       'min_steps': 1, 'max_steps': 100 if variable_guidance else 50, 'min_guidance': 1,
                       'max_guidance': 10 if variable_guidance else 1, 'max_loras': 3},
            'notes': (['Designed for posters, dense text and graphic layouts. Review every letter before publishing.',
                       'Base model: 50 steps and guidance 4. Uses your prompt directly; no hidden prompt enhancer.',
                       'This preset supports text-to-image only. Use Qwen or Klein for reference editing.',
                       'Output is opaque. Negative prompts work above guidance 1.'] if is_ernie else
                      ['Designed for about 4 megapixels. Start at 2048 × 2048; smaller canvases can reduce quality.',
                       'Full model: 50 steps, guidance 5. Negative prompts work above guidance 1.',
                       'Use up to ten ordered references. Images are bounded to 4 MP and transparency is placed over white.',
                       'Output is opaque. Patch seam smoothing is enabled; no external prompt enhancer is required.'] if is_hidream else
                      ['Reference images are numbered in order: <image1>, <image2>, and so on.',
                       'The first reference is padded to the chosen canvas; all of its content remains visible.',
                       'Negative prompts have no effect at guidance 1. Higher guidance is experimental.',
                       'Transparent output is checked for real alpha. A failed transparent generation is reported.'] if is_qwen else
                      ['A starting image initializes the VAE latent; this is a variation, not semantic reference editing.',
                       'The starting image is center-cropped to the chosen aspect ratio and transparency is placed over white.',
                       'Lower variation strength retains more of the starting composition. Output is opaque.'] if is_z else
                      ['Reference images guide content and composition; output uses the chosen canvas dimensions.',
                       'Use up to four references. References are reduced to about one megapixel and transparency is placed over white.',
                       'Output is opaque. Negative prompts and variation strength are not used.'])})
    root = model_directory()
    return {'connected': connected, 'default_model': 'qwen', 'model_directory': str(root),
            'models': [enrich_model(model, root) for model in models]}


@router.get('/models')
async def generation_models(request: Request, refresh: bool = False):
    editor.guard(request)
    from main import generation_lock
    try:
        result = model_inventory(await read_inventory(qwen_image._object_info, refresh=refresh))
    except qwen_image.QwenImageError as error:
        result = model_inventory({}, connected=False, connection_reason=str(error))
    result['busy'] = generation_lock.locked()
    return result


@router.get('/progress')
async def generation_progress(request: Request):
    editor.guard(request, True)
    return progress_snapshot()


def generation_parameters(payload):
    defaults = MODEL_DEFAULTS[payload.model]
    return validate_generation_metadata({'model': payload.model, 'variant': payload.variant or defaults['variant'],
        'prompt': payload.prompt.strip(), 'negative_prompt': payload.negative_prompt.strip(),
        'width': payload.width, 'height': payload.height, 'seed': payload.seed if payload.seed is not None else secrets.randbits(48),
        'transparent': payload.transparent, 'steps': payload.steps if payload.steps is not None else defaults['steps'],
        'guidance': payload.guidance if payload.guidance is not None else defaults['guidance'],
        'denoise': (payload.denoise if payload.denoise is not None else (0.6 if payload.reference_session_ids else 1.0)) if payload.model == 'z-image-turbo' else None,
        'reference_count': len(payload.reference_session_ids), 'loras': [item.model_dump() for item in payload.loras]})


async def snapshot_references(ids, destination, attributions=None):
    paths = []
    # Snapshot before taking the GPU lock. Existing cutout routes take session
    # then GPU locks; reversing that order here would deadlock concurrent edits.
    async with AsyncExitStack() as stack:
        for sid in sorted(ids):
            await stack.enter_async_context(editor.locks.setdefault(sid, asyncio.Lock()))
        for index, sid in enumerate(ids):
            data = editor.read_session(sid); path = destination / ('reference-' + str(index + 1) + '.png')
            if attributions is not None:
                attributions.extend(collect_attributions(data))
            def snapshot(data=data, path=path):
                image = editor.render(data)
                if not data.get('cutout', {}).get('enabled') and not data.get('layer_stack'):
                    image = editor.attach_source_alpha(editor.folder(data['id']), data, image)
                image.save(path)
            await asyncio.to_thread(snapshot)
            paths.append(path)
    return paths


def create_generated_session(image, parameters, directory, reference_attributions=None):
    path = directory / 'generated.png'
    image.save(path, icc_profile=editor.SRGB.tobytes())
    label = {'qwen': 'Qwen', 'z-image-turbo': 'Z-Image-Turbo', 'flux2-dev': 'FLUX2-Dev', 'flux2-klein-4b': 'FLUX2-Klein-4B', 'flux2-klein-9b': 'FLUX2-Klein-9B', 'hidream-o1': 'HiDream-O1', 'ernie-image': 'ERNIE-Image'}[parameters['model']]
    result = editor.create_session(path, f'{label}-{parameters["seed"]}.png')
    data = editor.read_session(result['id'])
    # A fresh generated image is unsaved creative work, even before retouching.
    data.update(generation=dict(parameters), revision=1)
    if reference_attributions:
        data['reference_attributions'] = unique_attributions(reference_attributions)
    editor.write_session(editor.folder(result['id']), data)
    return editor.public(data)


async def execute_generation(parameters, payload, references, loras):
    if parameters['model'] == 'qwen':
        image = await qwen_image.run_qwen_image(
            references[0] if references else None, parameters['prompt'], variant=parameters['variant'],
            reference_paths=references[1:], size=(payload.width, payload.height), seed=parameters['seed'],
            task='generate', steps=parameters['steps'], negative_prompt=parameters['negative_prompt'],
            cfg=parameters['guidance'], transparent=parameters['transparent'], loras=loras)
    elif parameters['model'] == 'z-image-turbo':
        image = await z_image.run_z_image(parameters['prompt'], input_path=references[0] if references else None,
            size=(payload.width, payload.height), seed=parameters['seed'], steps=parameters['steps'], denoise=parameters['denoise'], loras=loras)
    elif parameters['model'] == 'ernie-image':
        image = await ernie_image.run_ernie_image(parameters['prompt'], negative_prompt=parameters['negative_prompt'],
            size=(payload.width, payload.height), seed=parameters['seed'], steps=parameters['steps'], guidance=parameters['guidance'])
    elif parameters['model'] == 'hidream-o1':
        image = await hidream_image.run_hidream_image(parameters['prompt'], negative_prompt=parameters['negative_prompt'],
            references=references, size=(payload.width, payload.height), seed=parameters['seed'],
            steps=parameters['steps'], guidance=parameters['guidance'], loras=loras)
    else:
        image = await flux2_image.run_flux2_image(parameters['prompt'], model=parameters['model'], references=references,
            size=(payload.width, payload.height), seed=parameters['seed'], steps=parameters['steps'], guidance=parameters['guidance'], loras=loras)
    if parameters['transparent']:
        qwen_image.validate_cutout(image)
    elif 'A' in image.getbands():
        image = image.copy(); image.putalpha(255)
    if image.size != (payload.width, payload.height):
        raise ValueError('The model returned unexpected dimensions. Retry with another output size.')
    return image


@router.post('')
async def generate_image(request: Request, payload: GenerationRequest):
    editor.guard(request, True)
    from main import generation_lock
    if generation_lock.locked():
        raise HTTPException(409, 'Wait for the current image operation or model setup to finish.')
    parameters = generation_parameters(payload)
    try:
        loras = []
        if parameters['loras']:
            from lora_library import resolve_loras
            loras = resolve_loras(parameters['model'], parameters['loras'], reference_count=len(payload.reference_session_ids))
        with tempfile.TemporaryDirectory(prefix='local-image-generation-', dir=editor.ROOT) as temporary:
            directory = Path(temporary)
            attributions = []
            references = await snapshot_references(payload.reference_session_ids, directory, attributions)
            attributions = unique_attributions(attributions)
            if generation_lock.locked():
                raise HTTPException(409, 'Another image operation started. Generate again when it has finished.')
            with operation(parameters['model']) as progress:
                async with generation_lock:
                    image = await execute_generation(parameters, payload, references, loras)
                    progress.update('saving')
                    session = await asyncio.to_thread(create_generated_session, image, parameters, directory, attributions)
                from generation_library import add_generated
                warning = ''
                try:
                    await asyncio.to_thread(add_generated, image, session)
                except (ValueError, OSError) as error:
                    warning = 'The image opened successfully, but its library copy could not be saved: ' + str(error)
            return {'session': session, **parameters, 'library_warning': warning}
    except HTTPException:
        raise
    except (qwen_image.QwenImageError, z_image.ZImageError, flux2_image.Flux2ImageError, hidream_image.HiDreamImageError, ernie_image.ErnieImageError, ValueError) as error:
        raise HTTPException(400, str(error)) from error
    except Exception as error:
        raise HTTPException(500, 'Local Image could not complete generation: ' + str(error)) from error
