"""Download a pinned generation preset with the app's verified downloader.

Usage: python scripts/download_generation_models.py --models-dir <ComfyUI/models>
       --model z-image-turbo --variant bf16
Existing files are verified and preserved; this never changes ComfyUI itself.
"""
import argparse
import asyncio
import json
from pathlib import Path
import shutil
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
from generation_download_catalog import Z_IMAGE_FILES, FLUX_DEV_FILES, FLUX_KLEIN_FILES, FLUX_KLEIN_9B_FILES
from qwen_download_catalog import QWEN_FILES
from hidream_download_catalog import HIDREAM_FILES
from ernie_download_catalog import ERNIE_FILES
from managed_ai import download_verified


async def download(root, model, variant):
    root = root.resolve()
    files = {'qwen': QWEN_FILES, 'z-image-turbo': Z_IMAGE_FILES, 'ernie-image': ERNIE_FILES,
             'flux2-dev': FLUX_DEV_FILES, 'flux2-klein-4b': FLUX_KLEIN_FILES, 'flux2-klein-9b': FLUX_KLEIN_9B_FILES, 'hidream-o1': HIDREAM_FILES}[model].get(variant)
    if not files:
        raise ValueError('This model does not support the selected precision.')
    root.mkdir(parents=True, exist_ok=True)
    required = sum(item['bytes'] for item in files if not (root / item['folder'] / item['name']).exists())
    if shutil.disk_usage(root).free < required + 1024 ** 3:
        raise RuntimeError('Not enough free disk space for this preset.')
    verified = []
    for item in files:
        target = root / item['folder'] / item['name']
        if not target.parent.resolve().is_relative_to(root):
            raise ValueError('Model subfolder resolves outside the selected models directory.')
        print(('Verifying ' if target.exists() else 'Downloading ') + item['name'], flush=True)
        last = [time.monotonic()]
        def progress(done, total):
            if time.monotonic() - last[0] >= 15 or done == total:
                print(f"{item['name']}: {done / total:.0%}", flush=True)
                last[0] = time.monotonic()
        await download_verified(item, target, progress)
        verified.append(str(target))
    print(json.dumps({'model': model, 'variant': variant, 'verified': verified}), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--models-dir', type=Path, required=True)
    parser.add_argument('--model', choices=['qwen', 'z-image-turbo', 'flux2-dev', 'flux2-klein-4b', 'flux2-klein-9b', 'hidream-o1', 'ernie-image'], required=True)
    parser.add_argument('--variant', choices=['int8', 'bf16', 'fp8'], required=True)
    args = parser.parse_args()
    asyncio.run(download(args.models_dir, args.model, args.variant))
