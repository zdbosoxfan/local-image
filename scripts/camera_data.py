#!/usr/bin/env python3
"""Verify (default) or restore (--fetch) whole, pinned camera data. Never rewrites upstream files."""
import argparse
import hashlib
import json
from pathlib import Path
import urllib.request

root = Path(__file__).resolve().parents[1] / "crates/lc-raw/data/cameras"
p = argparse.ArgumentParser(description=__doc__)
p.add_argument("--fetch", action="store_true")
a = p.parse_args()
for entry in json.loads((root / "manifest.json").read_text()):
    path = root / entry["file"]
    if a.fetch:
        url = f'https://raw.githubusercontent.com/{entry["repository"]}/{entry["revision"]}/{entry["path"]}'
        data = urllib.request.urlopen(url, timeout=60).read()
        if hashlib.sha256(data).hexdigest() != entry["sha256"]:
            raise RuntimeError(f'{path.name}: upstream hash mismatch')
        path.write_bytes(data)
    if hashlib.sha256(path.read_bytes()).hexdigest() != entry["sha256"]:
        raise RuntimeError(f'{path.name}: local hash mismatch')
    print(f'{path.name}: verified {entry["revision"]}')
