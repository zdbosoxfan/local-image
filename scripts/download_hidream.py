"""Install only the pinned Full HiDream-O1 FP8 checkpoint; no prompt agent."""
import argparse
import asyncio
import json
from pathlib import Path
import shutil
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
from hidream_download_catalog import HIDREAM_FILES
from managed_ai import download_verified


async def download(root):
    root = root.resolve()
    root.mkdir(parents=True, exist_ok=True)
    item = HIDREAM_FILES['fp8'][0]
    target = root / item['folder'] / item['name']
    if not target.parent.resolve().is_relative_to(root):
        raise ValueError('Checkpoint folder resolves outside the configured model directory.')
    if not target.exists() and shutil.disk_usage(root).free < item['bytes'] + 1024 ** 3:
        raise RuntimeError('Not enough free disk space for HiDream-O1 FP8.')
    print(('Verifying ' if target.exists() else 'Downloading ') + item['name'], flush=True)
    last = [time.monotonic()]
    def progress(done, total):
        if time.monotonic() - last[0] >= 10 or done == total:
            print(f'{done / total:.0%}', flush=True)
            last[0] = time.monotonic()
    await download_verified(item, target, progress)
    print(json.dumps({'model': 'hidream-o1', 'variant': 'fp8', 'verified': str(target),
                      'sha256': item['sha256'], 'bytes': item['bytes']}), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--models-dir', type=Path, required=True)
    asyncio.run(download(parser.parse_args().models_dir))
