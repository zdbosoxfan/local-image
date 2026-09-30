"""Snapshot an editable document and restore it into an independent image."""
import asyncio
from pathlib import Path
import secrets
import tempfile

from fastapi import APIRouter, HTTPException, Request
from PIL import Image
from pydantic import BaseModel, ConfigDict, Field, model_validator

import local_remove as editor
import qwen_image
from generation_metadata import validate_generation_metadata
from stock_attribution import collect_attributions, validate_attribution
from upscale_metadata import validate_upscale_metadata, validate_upscale_size

router = APIRouter(prefix='/api/local-remove/generation/upscale')
# Accepted on RTX 5090: native 3840x2160 photo restoration and RGBA preservation.
# It is an optional photo enhancement; synthesized fine detail is not ground truth.
ENABLE_UPSCALE = True
LIMITS = {'min_dimension': 256, 'max_dimension': 4096, 'dimension_step': 2, 'max_pixels': 16777216}


class UpscaleRequest(BaseModel):
    model_config = ConfigDict(extra='forbid', strict=True)
    session_id: str
    revision: int = Field(ge=0)
    width: int = Field(ge=256, le=4096, multiple_of=2)
    height: int = Field(ge=256, le=4096, multiple_of=2)
    seed: int | None = Field(default=None, ge=0, le=2**53 - 1)

    @model_validator(mode='after')
    def valid_session(self):
        try:
            editor.validate_id(self.session_id, 'Session')
        except HTTPException:
            raise ValueError('Choose an image already open in Local Image.') from None
        return self


@router.get('/models')
async def upscale_models(request: Request):
    editor.guard(request)
    from seedvr2_image import seedvr2_model_option
    connected, reason = True, ''
    try:
        info = await qwen_image._object_info()
    except qwen_image.QwenImageError as error:
        info, connected, reason = {}, False, str(error)
    option = seedvr2_model_option(info)
    if not connected:
        option.update(available=False, reason=reason)
    return {'enabled': ENABLE_UPSCALE, 'connected': connected,
            'model': {'id': 'seedvr2', 'label': 'SeedVR2 7B', 'variant': 'fp16',
                      'available': option['available'], 'reason': option['reason'], 'variants': [option]},
            'limits': dict(LIMITS),
            'reason': '' if ENABLE_UPSCALE else 'Upscaling is withheld while real-image quality is being validated.'}


def snapshot(data, path):
    image = editor.render(data)
    if not data.get('cutout', {}).get('enabled') and not data.get('layer_stack'):
        image = editor.attach_source_alpha(editor.folder(data['id']), data, image)
    image.save(path, icc_profile=editor.SRGB.tobytes())
    return image


def create_upscaled_session(image, source, provenance, directory):
    path = directory / 'upscaled.png'; image.save(path, icc_profile=editor.SRGB.tobytes())
    name = Path(source['name']).stem[:210] + '-upscaled.png'
    session = editor.create_session(path, name)
    data = editor.read_session(session['id'])
    data.update(upscale=validate_upscale_metadata(provenance), revision=1)
    if source.get('generation') is not None:
        data['generation'] = validate_generation_metadata(dict(source['generation']))
    if source.get('source_attribution') is not None:
        data['source_attribution'] = validate_attribution(source['source_attribution'])
    credits = collect_attributions(source)
    if credits:
        data['reference_attributions'] = credits
    editor.write_session(editor.folder(data['id']), data)
    return editor.public(data)


@router.post('')
async def upscale_image(request: Request, payload: UpscaleRequest):
    editor.guard(request, True)
    if not ENABLE_UPSCALE:
        raise HTTPException(409, 'Upscaling is withheld while real-image quality is being validated.')
    from main import generation_lock
    from seedvr2_image import run_seedvr2_image
    if generation_lock.locked():
        raise HTTPException(409, 'Wait for the current image operation or model setup to finish.')
    try:
        with tempfile.TemporaryDirectory(prefix='local-image-upscale-', dir=editor.ROOT) as temporary:
            directory = Path(temporary)
            async with editor.locks.setdefault(payload.session_id, asyncio.Lock()):
                source = editor.read_session(payload.session_id)
                if source['revision'] != payload.revision:
                    raise HTTPException(409, 'The image changed. Refresh it before upscaling.')
                validate_upscale_size((source['width'], source['height']), (payload.width, payload.height))
                # Validate portable metadata before committing GPU time.
                if source.get('generation') is not None:
                    validate_generation_metadata(source['generation'])
                collect_attributions(source)
                source_image = await asyncio.to_thread(snapshot, source, directory / 'source.png')
            if generation_lock.locked():
                raise HTTPException(409, 'Another image operation started. Try again when it finishes.')
            provenance = {'model': 'seedvr2', 'variant': 'fp16', 'source_width': source['width'], 'source_height': source['height'],
                          'width': payload.width, 'height': payload.height, 'seed': payload.seed if payload.seed is not None else secrets.randbits(48)}
            async with generation_lock:
                result = await run_seedvr2_image(directory / 'source.png', size=(payload.width, payload.height), seed=provenance['seed'])
                if result.size != (payload.width, payload.height):
                    raise ValueError('The upscaler returned unexpected dimensions.')
                # Preserve source transparency independently of model inference.
                if 'A' in source_image.getbands():
                    result = result.convert('RGBA')
                    result.putalpha(source_image.getchannel('A').resize(result.size, Image.Resampling.LANCZOS))
                elif 'A' in result.getbands():
                    result = result.convert('RGB')
                session = await asyncio.to_thread(create_upscaled_session, result, source, provenance, directory)
            from generation_library import add_generated
            warning = ''
            try:
                await asyncio.to_thread(add_generated, result, session)
            except (ValueError, OSError) as error:
                warning = 'The image opened successfully, but its library copy could not be saved: ' + str(error)
            return {'session': session, 'upscale': provenance, 'library_warning': warning}
    except HTTPException:
        raise
    except (ValueError, RuntimeError, OSError) as error:
        raise HTTPException(400, str(error)) from error
