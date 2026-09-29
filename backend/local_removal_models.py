"""The local FLUX Klein removal model and its required components."""
from app_paths import model_directory

FILES = {
    'klein': [('diffusion_models', 'flux-2-klein-base-4b.safetensors'),
              ('text_encoders', 'qwen_3_4b.safetensors'), ('vae', 'flux2-vae.safetensors'),
              ('loras', 'flux-2-klein-object-remove.safetensors')],
}


def model_options():
    missing = [name for folder, name in FILES['klein']
               if not (model_directory() / folder / name).is_file()]
    option = {'id': 'klein', 'label': 'FLUX Klein',
              'description': 'Removal-trained model with automatic edge blending.',
              'available': not missing}
    if missing:
        option['reason'] = 'FLUX model files are missing. Open Settings to set up AI removal.'
    return [option]


async def run_local_removal(source_path, mask_bytes, seed, model):
    from engine import run_inpaint
    if model not in FILES:
        raise ValueError('Choose FLUX Klein for AI removal.')
    option = model_options()[0]
    if not option['available']:
        raise ValueError(option['reason'])
    return await run_inpaint(source_path, mask_bytes, '', '', seed)
