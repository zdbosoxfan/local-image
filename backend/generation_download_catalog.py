"""Pinned Comfy-Org Z-Image Turbo artifacts, verified against HF LFS metadata.

https://huggingface.co/api/models/Comfy-Org/z_image_turbo/tree/
6fc90a3b1b653e935a0d175e260736de25b84df5/split_files/{diffusion_models,text_encoders,vae}
"""
REVISION = '6fc90a3b1b653e935a0d175e260736de25b84df5'
BASE_URL = f'https://huggingface.co/Comfy-Org/z_image_turbo/resolve/{REVISION}/split_files/'
LICENSE_URL = 'https://huggingface.co/Tongyi-MAI/Z-Image-Turbo'
LICENSE_NOTE = 'Z-Image Turbo: Apache 2.0. Review the model card and component licenses before distribution.'


def artifact(folder, name, size, digest):
    return {'folder': folder, 'name': name, 'bytes': size, 'sha256': digest,
            'url': BASE_URL + folder + '/' + name}


Z_IMAGE_FILES = {
    'bf16': (
        artifact('diffusion_models', 'z_image_turbo_bf16.safetensors', 12309866400,
                 '2407613050b809ffdff18a4ac99af83ea6b95443ecebdf80e064a79c825574a6'),
        artifact('text_encoders', 'qwen_3_4b.safetensors', 8044982048,
                 '6c671498573ac2f7a5501502ccce8d2b08ea6ca2f661c458e708f36b36edfc5a'),
        artifact('vae', 'ae.safetensors', 335304388,
                 'afc8e28272cd15db3919bacdb6918ce9c1ed22e96cb12c4d5ed0fba823529e38'),
    ),
}

# Dev uses Comfy-Org's public FP8 repack, not gated BFL credentials or a remote
# text encoder. Its model license still applies to these quantized weights.
FLUX_DEV_REVISION = 'ed33133cd56476eac818c0943b6f9419b3e4a3a1'
FLUX_DEV_BASE = f'https://huggingface.co/Comfy-Org/flux2-dev/resolve/{FLUX_DEV_REVISION}/split_files/'
FLUX_DEV_LICENSE_URL = 'https://huggingface.co/black-forest-labs/FLUX.2-dev/blob/main/LICENSE.md'
FLUX_DEV_LICENSE_NOTE = 'FLUX Non-Commercial License: model use is noncommercial; commercial model deployment requires a separate license. Output use follows the license terms.'
KLEIN_REVISION = 'e7b7dc27f91deacad38e78976d1f2b499d76a294'
KLEIN_LICENSE_URL = 'https://huggingface.co/black-forest-labs/FLUX.2-klein-4B/blob/main/LICENSE.md'
KLEIN_LICENSE_NOTE = 'FLUX.2 Klein 4B: Apache 2.0. This is the distilled generation model; the existing base model remains dedicated to removal.'


def flux_artifact(folder, name, size, digest):
    return {'folder': folder, 'name': name, 'bytes': size, 'sha256': digest,
            'url': FLUX_DEV_BASE + folder + '/' + name}


FLUX_VAE = flux_artifact('vae', 'flux2-vae.safetensors', 336213556,
                        'd64f3a68e1cc4f9f4e29b6e0da38a0204fe9a49f2d4053f0ec1fa1ca02f9c4b5')
FLUX_VAE['compatible_existing'] = ({'bytes': 336211292,
    'sha256': '868fe7b343cc8f3a19dbcfcafbc3d5f888802be3f89bd81b65b3621a066ce8f3'},)
FLUX_DEV_FILES = {'fp8': (
    flux_artifact('diffusion_models', 'flux2_dev_fp8mixed.safetensors', 35455599592,
                  '863a82e4ff950a42a6b0e80bea824828f129eb1a8fbbdbd9e8cb29859127b486'),
    flux_artifact('text_encoders', 'mistral_3_small_flux2_fp8.safetensors', 18034640095,
                  'e3467b7d912a234fb929cdf215dc08efdb011810b44bc21081c4234cc75b370e'),
    FLUX_VAE,
)}
FLUX_KLEIN_FILES = {'bf16': (
    {'folder': 'diffusion_models', 'name': 'flux-2-klein-4b.safetensors', 'bytes': 7751105712,
     'sha256': 'ec3d4e733a771f61c052fb4856c48b336c55eaf2c65487c2a1faeb9bbda7a343',
     'url': f'https://huggingface.co/black-forest-labs/FLUX.2-klein-4B/resolve/{KLEIN_REVISION}/flux-2-klein-4b.safetensors'},
    Z_IMAGE_FILES['bf16'][1], FLUX_VAE,
)}

# Distilled 9B and its larger Qwen encoder are distinct from the 4B preset.
# The official native template uses this FP8 pair. BFL requires license acceptance.
KLEIN_9B_REVISION = '902d9d510b51533e07729f19211414a3648b77d2'
KLEIN_9B_ENCODER_REVISION = '3f62d9d8ae1fec33c6e91453d5c712855b096b55'
KLEIN_9B_ACCESS_URL = 'https://huggingface.co/black-forest-labs/FLUX.2-klein-9b-fp8'
KLEIN_9B_LICENSE_URL = KLEIN_9B_ACCESS_URL + '/blob/main/LICENSE.md'
KLEIN_9B_LICENSE_NOTE = 'FLUX.2 Klein 9B uses the FLUX Non-Commercial License and requires publisher license acceptance on Hugging Face before download. Commercial model deployment requires a separate license; output use follows the license terms.'
FLUX_KLEIN_9B_FILES = {'fp8': (
    {'folder': 'diffusion_models', 'name': 'flux-2-klein-9b-fp8.safetensors', 'bytes': 9433061528,
     'sha256': '865ba09f5b4c3cbd3468a4bd3acb9fcb2f8740c54317482f0bcd4ed1d3655cee',
     'url': f'{KLEIN_9B_ACCESS_URL}/resolve/{KLEIN_9B_REVISION}/flux-2-klein-9b-fp8.safetensors',
     'access_url': KLEIN_9B_ACCESS_URL},
    {'folder': 'text_encoders', 'name': 'qwen_3_8b_fp8mixed.safetensors', 'bytes': 8664848742,
     'sha256': 'abad16806e0cbabc54e0325d6565847443fe396d5f0be38bb3cd3fe75a1201d6',
     'url': f'https://huggingface.co/Comfy-Org/flux2-klein-9B/resolve/{KLEIN_9B_ENCODER_REVISION}/split_files/text_encoders/qwen_3_8b_fp8mixed.safetensors'},
    FLUX_VAE,
)}
