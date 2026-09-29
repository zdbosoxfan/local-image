"""First-use hardware guidance, with capacity detection rather than model gating."""
import asyncio
import csv
import io
import os
import re
import subprocess

import aiohttp
from fastapi import APIRouter, Request
import psutil

from app_paths import read_config

router = APIRouter()
PROFILES = [
    {'id': 'hidream-o1', 'label': 'Image Gen · HiDream O1', 'vram': '32 GB planning allowance',
     'legacy': True,
     'detail': 'Full FP8 model for detailed 4-megapixel generation, text layouts and reference editing.',
     'basis': 'Validated on a 32 GB RTX 5090 at 2048 × 2048, including reference editing. This is tested capacity, not a published minimum. Reference count and canvas size affect use.',
     'source_url': 'https://docs.comfy.org/tutorials/image/hidream/hidream-o1'},
    {'id': 'manual', 'label': 'Editing & Quick Heal', 'vram': 'No dedicated GPU',
     'detail': 'Brushes, pen selections, transforms, compositing and CPU healing.',
     'basis': 'These functions run locally without a GPU model.', 'source_url': ''},
    {'id': 'seedvr2', 'label': 'Upscale · SeedVR2 7B', 'vram': '32 GB recommended',
     'detail': 'Optional image enhancement and 4K enlargement with the FP16 base model.',
     'basis': 'Local Image planning recommendation: tested at 3840 × 2160 on a 32 GB RTX 5090. This is tested capacity, not a measured peak or a published minimum. Larger output can need more memory.',
     'source_url': 'https://docs.comfy.org/tutorials/utility/seedvr2'},
    {'id': 'ernie-image', 'label': 'Image Gen · ERNIE-Image Base', 'vram': '24 GB recommended',
     'detail': 'Text-heavy posters and graphic layouts from a written prompt; BF16 preset.',
     'basis': 'The publisher describes operation on 24 GB consumer GPUs. Local Image tested 1024 × 1536 on a 32 GB RTX 5090. These are publisher guidance and tested capacity, not a measured peak or hard minimum.',
     'source_url': 'https://huggingface.co/baidu/ERNIE-Image'},
    {'id': 'flux', 'label': 'AI Remove · FLUX Klein', 'vram': '16 GB recommended',
     'detail': 'Object removal with the 4B model and removal adapter.',
     'basis': 'Planning allowance above the publisher’s approximately 13 GB 4B inference figure; actual removal usage varies.',
     'source_url': 'https://bfl.ai/blog/flux2-klein-towards-interactive-visual-intelligence'},
    {'id': 'z-image-turbo', 'label': 'Image Gen · Z-Image Turbo', 'vram': '16 GB recommended',
     'detail': 'Fast image generation and variations using the BF16 preset.',
     'basis': 'The publisher describes operation on 16 GB consumer GPUs; this is not a lower-memory compatibility guarantee.',
     'source_url': 'https://huggingface.co/Tongyi-MAI/Z-Image-Turbo'},
    {'id': 'qwen-int8', 'label': 'Qwen Compact · all AI tools', 'vram': '24 GB recommended',
     'detail': 'Generation, image edits, background removal and transparent assets.',
     'basis': 'Conservative Local Image planning estimate for this INT8 preset, not a published minimum. Validated on an RTX 5090 with 32 GB.',
     'source_url': 'https://docs.comfy.org/tutorials/image/qwen/qwen-image-2-1'},
    {'id': 'qwen-bf16', 'label': 'Qwen Full · all AI tools', 'vram': '32 GB recommended',
     'detail': 'The same Qwen model at full precision, with more memory use.',
     'basis': 'Local Image planning recommendation and tested GPU capacity, not a published minimum. ComfyUI may offload components to system RAM.',
     'source_url': 'https://docs.comfy.org/tutorials/image/qwen/qwen-image-2-1'},
    {'id': 'flux2-klein-4b', 'label': 'Image Gen · FLUX.2 Klein 4B', 'vram': '16 GB recommended',
     'detail': 'Fast generation and reference-image editing with the distilled model.',
     'basis': 'Planning allowance above the publisher’s approximately 13 GB 4B inference figure.',
     'source_url': 'https://bfl.ai/blog/flux2-klein-towards-interactive-visual-intelligence'},
    {'id': 'flux2-dev', 'label': 'Image Gen · FLUX.2 Dev', 'vram': '48 GB recommended',
     'legacy': True,
     'detail': 'The FP8 preset can use lower-VRAM GPUs with CPU offloading, substantial system RAM and slower generation.',
     'basis': 'The publisher suggests 40–48 GB for 8-bit experiments. This app uses FP8 weights; a 32 GB GPU needs offloading. This is not the separate 4-bit preset.',
     'source_url': 'https://github.com/black-forest-labs/flux2/blob/main/docs/flux2_dev_hf.md'},
    {'id': 'flux2-klein-9b', 'label': 'Image Gen · FLUX.2 Klein 9B', 'vram': '24 GB recommended',
     'detail': 'Larger distilled Klein generation and reference editing; ComfyUI can offload the text encoder to system RAM.',
     'basis': 'The publisher reports approximately 24 GB for the 9B model. This is guidance for inference, not simultaneous residence of every component or a guarantee for large canvases.',
     'source_url': 'https://bfl.ai/blog/flux2-klein-towards-interactive-visual-intelligence'},
]
NOTE = ('Planning recommendations at each model’s default resolution, not hard minimums. '
        'Offloading can reduce VRAM use but needs system RAM and runs more slowly. '
        'Large images and multiple references need more memory. Model download size is separate from VRAM.')


def parse_nvidia_devices(output):
    devices = []
    for row in csv.reader(io.StringIO(output)):
        if len(row) != 2:
            continue
        try:
            total = float(row[1].strip())
            if not 0 < total < 2**24:
                continue
            devices.append({'name': row[0].strip(), 'vram_gb': round(total / 1024, 1), 'source': 'NVIDIA driver'})
        except (ValueError, TypeError):
            continue
    return devices


def nvidia_devices():
    try:
        result = subprocess.run(['nvidia-smi', '--query-gpu=name,memory.total', '--format=csv,noheader,nounits'],
                                capture_output=True, text=True, timeout=4,
                                creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0)
        return parse_nvidia_devices(result.stdout) if result.returncode == 0 else []
    except (OSError, subprocess.TimeoutExpired):
        return []


async def hardware_status():
    devices, connected = [], False
    port = read_config()['comfy_port']
    try:
        async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=3)) as client:
            async with client.get(f'http://127.0.0.1:{port}/system_stats') as response:
                response.raise_for_status()
                stats = await response.json()
        if isinstance(stats, dict) and isinstance(stats.get('devices'), list):
            connected = True
            for device in stats['devices']:
                if not isinstance(device, dict) or device.get('type') in {'cpu', None}:
                    continue
                total = device.get('vram_total')
                if isinstance(total, (int, float)) and 0 < total < 2**50:
                    name = re.sub(r'^cuda:\d+\s+', '', str(device.get('name', 'GPU')))
                    name = re.sub(r'\s*:\s*cudaMalloc(?:Async)?$', '', name)
                    devices.append({'name': name, 'vram_gb': round(total / 1024**3, 1), 'source': 'ComfyUI'})
    except (aiohttp.ClientError, asyncio.TimeoutError, ValueError, TypeError):
        pass
    if not devices:
        devices = await asyncio.to_thread(nvidia_devices)
    return {'devices': devices, 'system_ram_gb': round(psutil.virtual_memory().total / 1024**3, 1),
            'comfy_connected': connected, 'profiles': [profile for profile in PROFILES if not profile.get('legacy')], 'note': NOTE}


@router.get('/api/local-remove/hardware')
async def get_hardware(request: Request):
    from local_remove import guard
    guard(request)
    return await hardware_status()
