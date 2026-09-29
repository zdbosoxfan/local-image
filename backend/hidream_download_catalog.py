"""Full HiDream-O1 FP8 checkpoint linked by ComfyUI's native model guide.

Metadata verified from the publisher's immutable Hugging Face revision.
The integrated checkpoint needs no external VAE, text encoder or prompt agent.
"""
REVISION = '377ec7124bc46a15736c68cd9e4ad6d7da7a7614'
LICENSE_URL = 'https://github.com/HiDream-ai/HiDream-O1-Image/blob/main/LICENSE'
LICENSE_NOTE = 'HiDream-O1-Image: MIT license. This preset uses the Full model quantized to FP8; no optional prompt-refiner model is included.'
HIDREAM_FILES = {'fp8': (
    {'folder': 'checkpoints', 'name': 'hidream_o1_image_fp8_scaled.safetensors',
     'bytes': 8067535296, 'sha256': '05ad98bc4a94557697f31b839f6dbf6dba293a353d9e3c52eef7818b5802d206',
     'url': f'https://huggingface.co/Comfy-Org/HiDream-O1-Image/resolve/{REVISION}/checkpoints/hidream_o1_image_fp8_scaled.safetensors'},
)}
