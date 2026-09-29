"""Install the pinned base SeedVR2 7B FP16 model and VAE for native ComfyUI."""
import argparse
import asyncio
import json
from pathlib import Path
import shutil
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
from seedvr2_download_catalog import SEEDVR2_FILES
from managed_ai import download_verified


async def download(root):
    root = root.resolve()
    root.mkdir(parents=True, exist_ok=True)
    files = SEEDVR2_FILES['fp16']
    targets = [root / item['folder'] / item['name'] for item in files]
    for target in targets:
        if not target.parent.resolve().is_relative_to(root):
            raise ValueError('Model folder resolves outside the configured model directory.')
    missing_bytes = sum(item['bytes'] for item, target in zip(files, targets) if not target.exists())
    if shutil.disk_usage(root).free < missing_bytes + 1024 ** 3:
        raise RuntimeError('Not enough free disk space for SeedVR2 7B FP16.')
    for item, target in zip(files, targets):
        print(('Verifying ' if target.exists() else 'Downloading ') + item['name'], flush=True)
        last = [time.monotonic()]
        def progress(done, total):
            if time.monotonic() - last[0] >= 10 or done == total:
                print(f'{done / total:.0%}', flush=True)
                last[0] = time.monotonic()
        await download_verified(item, target, progress)
        print(json.dumps({'model': 'seedvr2', 'variant': 'fp16', 'verified': str(target),
                          'sha256': item['sha256'], 'bytes': item['bytes']}), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--models-dir', type=Path, required=True)
    asyncio.run(download(parser.parse_args().models_dir))
