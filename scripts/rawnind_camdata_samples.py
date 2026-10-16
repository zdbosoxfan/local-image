#!/usr/bin/env python3
"""Fetch the pinned Canon 7D RawNIND evaluation pair under target/ (CC-BY-SA-4.0).
Credit Benoit Brummer / Christophe De Vleeschouwer; dataset doi:10.14428/DVN/DEQCIM.
"""
import hashlib
from pathlib import Path
import urllib.request

root = Path(__file__).resolve().parents[1] / 'target/rawnind'
root.mkdir(parents=True, exist_ok=True)
samples = [
    (28448, 'Bayer_7D-1_ISO3200_sha1=61536b7de6506deb768693f6e495af67fc8d2562.cr2', 'e9d8055670d90f522581872904b9bd7dd8eb3c76476270e9508c23bde2d21e4a'),
    (28815, 'Bayer_7D-1_GT_ISO100_sha1=22aef4a5b4038e241082741117827f364ce6a5ac.cr2', '1d30003e322e9cfc62e820cb9ca08fea16f6af7885698962db55e13d82fc50b3'),
    (28658, 'Bayer_7D-1_ISO6400_sha1=733cb095be69607aa14eca55b1f9f471bd0f00a3.cr2', '095d8cad96ca799992ae3b51dd4f625cf095e9bbf80ef9f5bdec4dc451f98c77'),
]
for file_id, name, expected in samples:
    path = root / name
    if not path.exists():
        data = urllib.request.urlopen(f'https://dataverse.uclouvain.be/api/access/datafile/{file_id}', timeout=120).read()
        if hashlib.sha256(data).hexdigest() != expected:
            raise RuntimeError(f'{name}: download hash mismatch')
        path.write_bytes(data)
    if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        raise RuntimeError(f'{name}: local hash mismatch')
    print(f'{name}: verified')
