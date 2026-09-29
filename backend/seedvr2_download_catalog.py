"""Pinned base SeedVR2 7B FP16 preset from ComfyUI's official model guide.

The base restoration model is chosen instead of the sharper variant. Both
artifacts are public safetensors; no external encoder or custom nodes are needed.
"""
REVISION = 'df48879708206a403d2a61acd55578c2e80fd233'
LICENSE_URL = 'https://github.com/ByteDance-Seed/SeedVR/blob/main/LICENSE'
LICENSE_NOTE = 'SeedVR2: Apache 2.0. This preset uses the base 7B FP16 restoration model, not the Sharp variant. Restored details can differ from the original image.'
SEEDVR2_FILES = {'fp16': (
    {'folder': 'diffusion_models', 'name': 'seedvr2_7b_fp16.safetensors',
     'bytes': 16480583960, 'sha256': '2742ca6fee63bc5cc1773f426dd4b07b78cad27f51c9ea5cd42b035e6b592252',
     'url': f'https://huggingface.co/Comfy-Org/SeedVR2/resolve/{REVISION}/diffusion_models/seedvr2_7b_fp16.safetensors'},
    {'folder': 'vae', 'name': 'seedvr2_ema_vae_fp16.safetensors',
     'bytes': 501324814, 'sha256': '20678548f420d98d26f11442d3528f8b8c94e57ee046ef93dbb7633da8612ca1',
     'url': f'https://huggingface.co/Comfy-Org/SeedVR2/resolve/{REVISION}/vae/seedvr2_ema_vae_fp16.safetensors'},
)}
