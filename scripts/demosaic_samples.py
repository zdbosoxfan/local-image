#!/usr/bin/env python3
"""Download the ten pinned CC0 demosaic reference files, only into target/.
All URLs/checksums/licences are recorded in the checked TSV; no client data.
"""
from pathlib import Path
import csv,hashlib,urllib.request,urllib.parse
from concurrent.futures import ThreadPoolExecutor
root=Path(__file__).resolve().parents[1]
out=root/'target/demosaic-corpus';out.mkdir(parents=True,exist_ok=True)
rows=list(csv.DictReader((root/'crates/lc-raw/tests/fixtures/demosaic-corpus.tsv').open(),delimiter='\t'))
def fetch(row):
    assert row['licence']=='CC0-1.0'
    path=out/row['file']
    if not path.exists():
        request=urllib.request.Request(urllib.parse.quote(row['url'],safe=':/'),headers={'User-Agent':'LocalImage-demosaic-reference'})
        with urllib.request.urlopen(request,timeout=120) as response:data=response.read()
        assert hashlib.sha256(data).hexdigest()==row['source_sha256'],row['file']
        temporary=path.with_suffix('.part');temporary.write_bytes(data);temporary.replace(path)
    assert hashlib.sha256(path.read_bytes()).hexdigest()==row['source_sha256'],path
    print(row['id'],row['model'],'CC0 verified',flush=True)
with ThreadPoolExecutor(max_workers=3) as pool:list(pool.map(fetch,rows))
