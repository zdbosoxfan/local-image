"""Download official ComfyUI Qwen 2.1 weights with publisher SHA-256 verification.

Usage: python scripts/download_qwen.py --models-dir <ComfyUI/models> --variant int8
The model is covered by the Qwen Research License; see docs/QWEN-IMAGE-21.md.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import shutil
import sys
import time
import urllib.request

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
from qwen_download_catalog import QWEN_FILES

ARTIFACTS = {item['folder'] + '/' + item['name']: item
             for files in QWEN_FILES.values() for item in files}
FILES = {path: (item['bytes'], item['sha256']) for path, item in ARTIFACTS.items()}


def sha256(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def download(root, relative):
    size, expected = FILES[relative]
    target = root / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists():
        if target.stat().st_size == size and sha256(target) == expected:
            print('Verified existing ' + relative, flush=True)
            return
        raise RuntimeError('Existing file does not match; it was preserved: ' + str(target))
    partial = target.with_suffix(target.suffix + '.partial')
    offset = partial.stat().st_size if partial.exists() else 0
    if offset > size:
        raise RuntimeError('Partial file is larger than the expected artifact: ' + str(partial))
    url = ARTIFACTS[relative]['url']
    print('Downloading ' + relative, flush=True)
    if offset < size:
        request = urllib.request.Request(url, headers={'Range': 'bytes=' + str(offset) + '-'} if offset else {})
        with urllib.request.urlopen(request, timeout=180) as response:
            if offset and (response.status != 206 or not response.headers.get('Content-Range', '').startswith('bytes ' + str(offset) + '-')):
                raise RuntimeError('Server did not honor resume; partial download was preserved.')
            with partial.open('ab' if offset else 'wb') as stream:
                last = time.monotonic()
                while block := response.read(4 * 1024 * 1024):
                    stream.write(block)
                    offset += len(block)
                    if offset > size:
                        raise RuntimeError('Download exceeds publisher file size.')
                    if time.monotonic() - last > 20:
                        print(f'{target.name}: {offset / size:.0%}', flush=True)
                        last = time.monotonic()
    if partial.stat().st_size != size or sha256(partial) != expected:
        raise RuntimeError('Download failed publisher checksum: ' + relative)
    if target.exists():
        raise RuntimeError('Destination appeared during download; neither file was replaced.')
    partial.rename(target)
    print('Verified ' + relative, flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--models-dir', type=Path, required=True)
    parser.add_argument('--variant', choices=['int8', 'bf16', 'both'], default='int8')
    args = parser.parse_args()
    root = args.models_dir.resolve()
    root.mkdir(parents=True, exist_ok=True)
    paths = [p for p in FILES if p.startswith('vae/') or args.variant == 'both' or ('int8' in p if args.variant == 'int8' else 'bf16' in p)]
    required = sum(max(0, FILES[p][0] - ((root / p).stat().st_size if (root / p).exists() else (root / (p + '.partial')).stat().st_size if (root / (p + '.partial')).exists() else 0)) for p in paths)
    if shutil.disk_usage(root).free < required + 1024**3:
        raise RuntimeError('Not enough disk space for the selected models.')
    with ThreadPoolExecutor(max_workers=2) as pool:
        list(pool.map(lambda path: download(root, path), paths))
    print(json.dumps({'variant': args.variant, 'models_dir': str(root), 'verified': paths}), flush=True)


if __name__ == '__main__':
    main()
