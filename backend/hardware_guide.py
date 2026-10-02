"""First-use hardware guidance, with capacity detection rather than model gating."""
import asyncio
import csv
import io
import math
import os
import re
import subprocess
import time

import aiohttp
from fastapi import APIRouter, Request
from pydantic import BaseModel, ConfigDict
import psutil

from app_paths import read_config, write_config

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
    for row in csv.reader(io.StringIO(output), skipinitialspace=True):
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
        from managed_ai import external_process_environment
        with external_process_environment() as environment:
            result = subprocess.run(['nvidia-smi', '--query-gpu=name,memory.total', '--format=csv,noheader,nounits'],
                                    capture_output=True, text=True, timeout=4, env=environment,
                                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0)
        return parse_nvidia_devices(result.stdout) if result.returncode == 0 else []
    except (OSError, subprocess.TimeoutExpired):
        return []


USAGE_REFRESH_SECONDS = 2
_usage_cache = None
_usage_task = None


def _finite_number(value, minimum=0, maximum=2**50):
    if isinstance(value, bool):
        return None
    try:
        number = float(value)
    except (ValueError, TypeError, OverflowError):
        return None
    return number if math.isfinite(number) and minimum <= number <= maximum else None


def _device_name(value):
    name = re.sub(r'^cuda:\d+\s+', '', str(value or 'GPU'))
    return re.sub(r'\s*:\s*cudaMalloc(?:Async)?$', '', name).strip()


def parse_nvidia_usage(output):
    """NVIDIA's selective CSV query reports MiB, not bytes; N/A stays unknown."""
    devices = []
    for row in csv.reader(io.StringIO(output), skipinitialspace=True):
        if len(row) != 5 or not row[0].strip() or not row[1].strip():
            continue
        total = _finite_number(row[3].strip(), minimum=1, maximum=2**24)
        used = _finite_number(row[2].strip(), maximum=2**24)
        if total is not None and used is not None and used > total:
            used = None
        devices.append({
            'id': row[0].strip(), 'name': row[1].strip(), 'source': 'NVIDIA driver',
            'utilization_percent': _finite_number(row[4].strip(), maximum=100),
            'vram_used_bytes': int(used * 1024**2) if used is not None else None,
            'vram_total_bytes': int(total * 1024**2) if total is not None else None,
            'memory_scope': 'device', 'is_backend_device': False,
        })
    return devices


def nvidia_usage():
    try:
        # Never invoke a shell or launch a continuous monitoring process. Driver
        # queries run off the event loop and are killed after a short timeout.
        # Frozen applications must restore host library search paths before
        # launching nvidia-smi, just like the external ComfyUI process.
        from managed_ai import external_process_environment
        with external_process_environment() as environment:
            result = subprocess.run(
                ['nvidia-smi', '--query-gpu=uuid,name,memory.used,memory.total,utilization.gpu',
                 '--format=csv,noheader,nounits'], capture_output=True, text=True,
                timeout=1, env=environment,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0)
        return parse_nvidia_usage(result.stdout) if result.returncode == 0 else []
    except (OSError, subprocess.TimeoutExpired, ValueError):
        return []


async def _usage_system_stats(port):
    try:
        async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=1)) as client:
            async with client.get(f'http://127.0.0.1:{port}/system_stats') as response:
                response.raise_for_status()
                stats = await response.json()
        return stats if isinstance(stats, dict) and isinstance(stats.get('devices'), list) else None
    except (aiohttp.ClientError, asyncio.TimeoutError, ValueError, TypeError):
        return None


def merge_gpu_usage(driver_devices, stats):
    devices = [dict(device) for device in driver_devices]
    if stats is None:
        return devices
    # ComfyUI lists its primary device first. Matching by a unique device name
    # avoids treating a CUDA logical index as a physical NVIDIA index when
    # CUDA_VISIBLE_DEVICES remaps devices (including duplicate GPU models).
    for position, device in enumerate(stats['devices']):
        if not isinstance(device, dict) or device.get('type') in {'cpu', None}:
            continue
        name = _device_name(device.get('name'))
        matches = [item for item in devices if item['source'] == 'NVIDIA driver'
                   and item['name'].casefold() == name.casefold()]
        if len(matches) == 1:
            if position == 0:
                matches[0]['is_backend_device'] = True
            continue
        shared = device.get('type') == 'mps'
        total = None if shared else _finite_number(device.get('vram_total'), minimum=1)
        free = None if shared else _finite_number(device.get('vram_free'))
        torch_free = _finite_number(device.get('torch_vram_free'))
        scope = 'shared' if shared else 'backend'
        if free is not None and torch_free is not None and torch_free <= free:
            # Comfy includes reusable PyTorch cache in free memory. Removing
            # that cache component gives actual free device memory.
            free -= torch_free
            scope = 'device'
        used = int(total - free) if total is not None and free is not None and free <= total else None
        devices.append({
            'id': f"comfy:{device.get('type')}:{device.get('index', position)}",
            'name': name, 'source': 'ComfyUI', 'utilization_percent': None,
            'vram_used_bytes': used, 'vram_total_bytes': int(total) if total is not None else None,
            'memory_scope': scope, 'is_backend_device': position == 0,
        })
    return sorted(devices, key=lambda device: not device['is_backend_device'])


async def _collect_gpu_usage(port):
    driver_devices, stats = await asyncio.gather(asyncio.to_thread(nvidia_usage), _usage_system_stats(port))
    return {'devices': merge_gpu_usage(driver_devices, stats), 'comfy_connected': stats is not None,
            'sampled_at': int(time.time() * 1000), 'refresh_after_ms': USAGE_REFRESH_SECONDS * 1000}


async def hardware_usage():
    """A shared short-lived snapshot prevents each visible control spawning a query."""
    global _usage_cache, _usage_task
    port = read_config()['comfy_port']
    if _usage_cache is not None:
        cached_port, expires, snapshot = _usage_cache
        if cached_port == port and time.monotonic() < expires:
            return snapshot
    loop = asyncio.get_running_loop()
    if (_usage_task is None or _usage_task[0] != port or _usage_task[1].get_loop() is not loop
            or _usage_task[1].done()):
        _usage_task = (port, loop.create_task(_collect_gpu_usage(port)))
    # Cancelling a disconnected HTTP request must not cancel another request's
    # shared driver sample. Both probes have independent one-second deadlines.
    snapshot = await asyncio.shield(_usage_task[1])
    _usage_cache = (port, time.monotonic() + USAGE_REFRESH_SECONDS, snapshot)
    return snapshot


@router.get('/api/local-remove/hardware/usage')
async def get_hardware_usage(request: Request):
    from local_remove import guard
    guard(request)
    return await hardware_usage()


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
                    name = _device_name(device.get('name'))
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


class HardwarePreference(BaseModel):
    model_config = ConfigDict(extra='forbid')
    dont_show_again: bool


@router.get('/api/local-remove/hardware/preference')
async def get_hardware_preference(request: Request):
    from local_remove import guard
    guard(request)
    # None allows existing browser acknowledgements to keep their behavior until
    # the user explicitly sets this durable, per-profile preference.
    return {'dont_show_again': read_config().get('hardware_guide_dismissed')}


@router.post('/api/local-remove/hardware/preference')
async def set_hardware_preference(request: Request, payload: HardwarePreference):
    from local_remove import guard
    guard(request, True)
    settings = await asyncio.to_thread(write_config, {'hardware_guide_dismissed': payload.dont_show_again})
    return {'dont_show_again': settings['hardware_guide_dismissed']}
