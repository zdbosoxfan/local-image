"""Pinned official ComfyUI ERNIE-Image Base BF16 preset, without prompt rewriting."""
from generation_download_catalog import FLUX_VAE
REVISION = '82fe29a5cd056f8b1deebc50570f125bcd4f4bea'
LICENSE_URL = 'https://github.com/baidu/ERNIE-Image/blob/main/LICENSE'
LICENSE_NOTE = 'ERNIE-Image Base: Apache 2.0. Raw-prompt text-to-image preset; no prompt enhancer is downloaded. Text spelling and layout still need visual review.'
ERNIE_FILES = {'bf16': (
    {'folder': 'diffusion_models', 'name': 'ernie-image.safetensors',
     'bytes': 16067025480, 'sha256': '94a35abaa0899cccc34d2e37310abf74a0a714256526117bba782c7eb4eb91c7',
     'url': f'https://huggingface.co/Comfy-Org/ERNIE-Image/resolve/{REVISION}/diffusion_models/ernie-image.safetensors'},
    {'folder': 'text_encoders', 'name': 'ministral-3-3b.safetensors',
     'bytes': 7717637511, 'sha256': '49a750a128863854eac7d85e1a277a7b44bf6ec3646405b84686dfeeca3708ca',
     'url': f'https://huggingface.co/Comfy-Org/ERNIE-Image/resolve/{REVISION}/text_encoders/ministral-3-3b.safetensors'},
    {'folder': 'vae', 'name': 'flux2-vae.safetensors',
     'bytes': 336213556, 'sha256': 'd64f3a68e1cc4f9f4e29b6e0da38a0204fe9a49f2d4053f0ec1fa1ca02f9c4b5',
     'compatible_existing': FLUX_VAE.get('compatible_existing', ()),
     'url': f'https://huggingface.co/Comfy-Org/ERNIE-Image/resolve/{REVISION}/vae/flux2-vae.safetensors'},
)}
