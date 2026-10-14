#!/usr/bin/env python3
"""Prepare reference-only real camera noise coefficients; no benchmark is run.
The whole unmodified GPL-3.0-or-later darktable file remains under target/.
"""
from pathlib import Path
import hashlib,json,urllib.request
PIN='733bd69f32cac7ff5e41025115942772add1f088'
SHA='e22730cf69ead3534b2d2e7500ab24af6da99834250b852ca1831002ba2450f8'
root=Path(__file__).resolve().parents[1]/'target/refvec/demosaic'
root.mkdir(parents=True,exist_ok=True)
whole=root/'noiseprofiles.json'
if not whole.exists():
    whole.write_bytes(urllib.request.urlopen(f'https://raw.githubusercontent.com/darktable-org/darktable/{PIN}/data/noiseprofiles.json',timeout=30).read())
assert hashlib.sha256(whole.read_bytes()).hexdigest()==SHA,'Review the source pin before updating noise profiles'
selected=[]
for maker in json.loads(whole.read_text())['noiseprofiles']:
    for model in maker['models']:
        if model['model']=='D750':
            selected=[p for p in model['profiles'] if p['iso'] in [100,400,800,1600,3200,6400,12800,25600]]
assert len(selected)==8
with (root/'d750-noise.tsv').open('w') as out:
    for p in selected:
        assert len(p['a'])==len(p['b'])==3
        out.write('\t'.join(map(str,[p['iso']]+p['a']+p['b']))+'\n')
print('Prepared 8 D750 ISO profiles; whole upstream JSON SHA-256 verified. No benchmark run.')
