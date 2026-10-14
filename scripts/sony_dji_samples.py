#!/usr/bin/env python3
"""Fetch SHA-pinned Sony/DJI reference samples under target only.

Default downloads CC0 DJI cases. --supplemental also downloads the publicly offered
DJI test samples (not CC0; reference-only, never redistribute). No images enter Git.
Use raw_decoder_comparison.py --manifest <manifest> --cache target/sony-dji for refs.
"""
import argparse
import csv
import hashlib
import io
from pathlib import Path
import urllib.parse
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]

def verified(data, digest, label):
    if hashlib.sha256(data).hexdigest() != digest:
        raise RuntimeError(f"{label}: SHA-256 mismatch")
    return data

def fetch(row, cache):
    destination = cache / row["file"]
    if destination.exists():
        verified(destination.read_bytes(), row["sha256"], destination)
        return
    request = urllib.request.Request(urllib.parse.quote(row["url"], safe=":/%"), headers={"User-Agent": "LocalImage-Sony-DJI-reference"})
    with urllib.request.urlopen(request, timeout=120) as response:
        data = response.read()
    if row.get("member"):
        verified(data, row["archive_sha256"], row["url"])
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            # Extract one named file to our destination, never an archive path.
            data = archive.read(row["member"])
    verified(data, row["sha256"], destination)
    temporary = destination.with_suffix(destination.suffix + ".part")
    temporary.write_bytes(data)
    temporary.replace(destination)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--supplemental", action="store_true")
    parser.add_argument("--cache", type=Path, default=ROOT / "target/sony-dji")
    args = parser.parse_args()
    cache = args.cache.resolve()
    target = (ROOT / "target").resolve()
    if not cache.is_relative_to(target):
        raise ValueError("reference files must stay under this worktree's target/")
    cache.mkdir(parents=True, exist_ok=True)
    fixtures = ROOT / "crates/lc-raw/tests/fixtures"
    manifests = [fixtures / "sony-dji-samples.tsv"]
    if args.supplemental:
        manifests.append(fixtures / "dji-supplemental-samples.tsv")
    rows = []
    for manifest in manifests:
        with manifest.open() as source:
            for row in csv.DictReader(source, delimiter="\t"):
                fetch(row, cache)
                print(row["id"], "verified", flush=True)
                rows.append(row)
    with (cache / "measurement.tsv").open("w") as destination:
        writer = csv.DictWriter(destination, fieldnames=["id", "file", "make", "model", "variant", "sha256", "url"], delimiter="\t", extrasaction="ignore")
        writer.writeheader()
        writer.writerows(rows)

if __name__ == "__main__":
    main()
