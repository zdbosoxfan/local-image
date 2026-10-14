#!/usr/bin/env python3
"""Download and audit every CC0 raw.pixls.us repository sample, pinned by the repository's SHA-256.
One bounded audit process at a time; four streaming download workers. All photos stay in target/.
Run after building camdata_audit with --features testgen. --download-only skips the decoder audit.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib
import html
import json
from pathlib import Path
import re
import shutil
import subprocess
import time
import urllib.parse
import urllib.request

root = Path(__file__).resolve().parents[1]
work = root / 'target/camdata-corpus'
raw = work / 'raw'
raw.mkdir(parents=True, exist_ok=True)
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--download-only', action='store_true')
p.add_argument('--audit-only', action='store_true')
a = p.parse_args()
snapshot = root / 'target/camdata-reference/rawpixls-repository.json'
if not snapshot.exists():
    snapshot.parent.mkdir(parents=True, exist_ok=True)
    snapshot.write_bytes(urllib.request.urlopen('https://raw.pixls.us/json/getrepository.php', timeout=60).read())
entries = []
for row in json.loads(snapshot.read_text())['data']:
    if 'publicdomain/zero/1.0/' not in row[5]:
        continue
    url = html.unescape(re.search(r"href='([^']+)'", row[7])[1])
    file_id = re.search(r'getfile.php/(\d+)/', url)[1]
    sha256 = re.search(r'title=[\'"]sha256 Checksum[\'"]>([0-9a-f]{64})', row[7])[1]
    ext = url.rsplit('.', 1)[-1].lower()
    entries.append({'id': file_id, 'make': row[0], 'model': row[1], 'mode': row[2], 'url': url,
                    'sha256': sha256, 'file': f'{file_id}.{ext}', 'license': 'CC0-1.0'})
(work / 'manifest.json').write_text(json.dumps(entries, indent=2)+'\n')


def digest(path):
    with path.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def fetch(e):
    dest = raw / e['file']
    if dest.exists() and digest(dest) == e['sha256']:
        return e, None
    old = root / 'target/rawdecode' / e['file']
    if not old.exists():
        old = root / 'target/rawdecode' / e['file'].upper()
    if old.exists() and digest(old) == e['sha256']:
        shutil.copyfile(old, dest)
        return e, None
    error = None
    for attempt in range(3):
        try:
            request = urllib.request.Request(urllib.parse.quote(e['url'], safe=':/'), headers={'User-Agent': 'LocalImage-camera-calibration-audit'})
            sha = hashlib.sha256()
            part = dest.with_suffix(dest.suffix+'.part')
            with urllib.request.urlopen(request, timeout=120) as response, part.open('wb') as out:
                while chunk := response.read(1024*1024):
                    sha.update(chunk)
                    out.write(chunk)
            if sha.hexdigest() != e['sha256']:
                raise RuntimeError('source hash mismatch')
            part.replace(dest)
            return e, None
        except Exception as exc:
            error = str(exc)
            time.sleep(1+attempt)
    return e, error


failures = []
if not a.audit_only:
    with ThreadPoolExecutor(max_workers=4) as pool:
        for i, future in enumerate(as_completed(pool.submit(fetch, e) for e in entries), 1):
            e, error = future.result()
            if error:
                failures.append({'entry': e, 'error': error})
            if i % 20 == 0 or error:
                print(f'download {i}/{len(entries)}: {len(failures)} errors', flush=True)
    (work / 'download-errors.json').write_text(json.dumps(failures, indent=2)+'\n')
if a.download_only:
    raise SystemExit(bool(failures))
audit = work / 'camdata_audit'
if not audit.exists():
    raise RuntimeError('Build camdata_audit --features testgen, then copy its executable into target/camdata-corpus/')
probe = subprocess.run([str(audit)], capture_output=True, text=True, check=True)
if 'Run with --features testgen' in probe.stderr:
    raise RuntimeError('The audit executable was built without testgen; rebuild and copy the correct executable')
prior = {}
result = work / 'levels.jsonl'
if result.exists():
    for line in result.read_text().splitlines():
        r = json.loads(line)
        prior[r['entry']['id']] = r
with result.open('a') as out:
    for i, e in enumerate(entries, 1):
        if e['id'] in prior or not (raw/e['file']).exists():
            continue
        try:
            r = subprocess.run([str(audit), '--levels-only', str(raw/e['file'])], capture_output=True, text=True, timeout=180)
            record = json.loads(r.stdout.splitlines()[-1]) if r.returncode == 0 else {'status': 'audit_error', 'error': r.stderr[-2000:]}
        except Exception as exc:
            record = {'status': 'audit_error', 'error': str(exc)}
        record['entry'] = e
        out.write(json.dumps(record)+'\n')
        out.flush()
        if i % 20 == 0 or record['status'] != 'ok':
            print(f'audit {i}/{len(entries)}: {record["status"]} {e["make"]} {e["model"]}', flush=True)
# Render a reproducible heavily clipped set from the level census.
records = [json.loads(line) for line in result.read_text().splitlines()]
candidates = sorted((r for r in records if r.get('fully_clipped_bins', 0) >= 100 and r['status'] == 'ok' and not r['entry']['file'].endswith('.dng')),
                    key=lambda r: (-r['fully_clipped_bins'], int(r['entry']['id'])))
selected=[]
with (work/'highlights.jsonl').open('w') as out:
    for r in candidates:
        process = subprocess.run([str(audit), str(raw/r['entry']['file'])], capture_output=True, text=True, timeout=180)
        if process.returncode:
            raise RuntimeError(process.stderr)
        record = json.loads(process.stdout.splitlines()[-1])
        if record.get('fully_clipped_bins',0)<100 or 'after_full' not in record:
            continue
        selected.append(r)
        record['entry'] = r['entry']
        out.write(json.dumps(record)+'\n')
        out.flush()
        print(f'highlights: {r["entry"]["id"]}', flush=True)
        if len(selected)==20:break
print(f'complete: {len(records)} level records, {len(selected)} clipped renders, {len(failures)} download errors', flush=True)
