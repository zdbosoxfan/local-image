#!/usr/bin/env python3
"""Fetch only the SHA-256-pinned CC0 files in the rawler update contract into target/rawdecode."""
import argparse
import csv
import hashlib
from pathlib import Path
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument("ids", nargs="*", help="optional raw.pixls.us IDs; default: all contract files")
p.add_argument("--output", type=Path, default=ROOT / "target/rawdecode")
a = p.parse_args()
a.output.mkdir(parents=True, exist_ok=True)
with (ROOT / "crates/lc-raw/tests/fixtures/rawler-contract.tsv").open() as f:
    for row in csv.DictReader(f, delimiter="\t"):
        if a.ids and row["id"] not in a.ids:
            continue
        path = a.output / row["file"]
        if not path.exists():
            request = urllib.request.Request(urllib.parse.quote(row["url"], safe=":/"), headers={"User-Agent": "LocalImage-RAW-contract"})
            with urllib.request.urlopen(request, timeout=120) as response:
                data = response.read()
            if hashlib.sha256(data).hexdigest() != row["source_sha256"]:
                raise RuntimeError(f"{row['file']}: source SHA-256 mismatch")
            temporary = path.with_suffix(path.suffix + ".part")
            temporary.write_bytes(data)
            temporary.replace(path)
        if hashlib.sha256(path.read_bytes()).hexdigest() != row["source_sha256"]:
            raise RuntimeError(f"{path}: source SHA-256 mismatch")
        print(f"{row['id']}: {row['make']} {row['model']} {row['variant']} verified", flush=True)
