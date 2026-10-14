#!/usr/bin/env python3
"""Cache a bounded, SHA256-verified CC0 Sony body/variant selection under target/.

The complete public Sony inventory is also recorded for honest availability reporting.
Reference unpacking invokes the existing out-of-build LibRaw harness; no cargo or GUI.
"""
import argparse
import csv
import hashlib
import html
import json
from pathlib import Path
import re
import urllib.parse
import urllib.request
from raw_decoder_comparison import libraw_reference

ROOT = Path(__file__).resolve().parents[1]
# A small representative sample covers each requested body and each major codec/size.
DEFAULT_IDS = ['2414', '3480', '2418', '4466', '6929', '6930', '6935', '8847',
               '7828', '6237', '5418', '3989', '7362', '1971', '5811', '2647', '3221', '2346', '3255', '8845', '8846']
FIELDS = ['id', 'file', 'make', 'model', 'variant', 'sha256', 'url']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--ids', nargs='*', default=DEFAULT_IDS)
    parser.add_argument('--cache', type=Path, default=ROOT / 'target/sony-compare')
    parser.add_argument('--references', action='store_true')
    args = parser.parse_args()
    cache = args.cache.resolve()
    cache.relative_to((ROOT / 'target').resolve())
    cache.mkdir(parents=True, exist_ok=True)
    repository = cache / 'samples.json'
    if not repository.exists():
        repository.write_bytes(urllib.request.urlopen('https://raw.pixls.us/json/getrepository.php', timeout=120).read())
    with (ROOT / 'crates/lc-raw/tests/fixtures/sony-excellence-samples.tsv').open() as f:
        pinned = {row['id']: row for row in csv.DictReader(f, delimiter='\t')}
    rows = []
    for record in json.loads(repository.read_text())['data']:
        if record[0] != 'Sony' or 'publicdomain/zero/1.0' not in record[5]:
            continue
        url = html.unescape(re.search(r"href='([^']+)'", record[7]).group(1))
        sample_id = re.search(r'getfile.php/(\d+)', url).group(1)
        digest = re.search(r"sha256 Checksum'>([0-9a-f]{64})", record[7]).group(1)
        suffix = Path(urllib.parse.urlparse(url).path).suffix
        rows.append(dict(zip(FIELDS, [sample_id, sample_id + suffix, record[0], record[1],
                    (record[2] + ' ' + record[4]).strip(), digest, url])))
    for name, selected in [('inventory.tsv', rows), ('manifest.tsv', [pinned.get(r['id'], r) for r in rows if r['id'] in args.ids])]:
        with (cache / name).open('w') as f:
            writer = csv.DictWriter(f, fieldnames=FIELDS, delimiter='\t', lineterminator='\n')
            writer.writeheader()
            writer.writerows(selected)
    chosen = [pinned.get(r['id'], r) for r in rows if r['id'] in args.ids]
    if set(args.ids) - {r['id'] for r in chosen}:
        raise RuntimeError('Requested ID absent from current CC0 Sony inventory')
    for row in chosen:
        path = cache / row['file']
        if not path.exists():
            data = urllib.request.urlopen(urllib.parse.quote(row['url'], safe=':/'), timeout=120).read()
            if hashlib.sha256(data).hexdigest() != row['sha256']:
                raise RuntimeError('source hash mismatch: ' + row['id'])
            path.write_bytes(data)
        if hashlib.sha256(path.read_bytes()).hexdigest() != row['sha256']:
            raise RuntimeError('cached hash mismatch: ' + row['id'])
        print(row['id'], row['model'], row['variant'], flush=True)
    if args.references:
        libraw_reference(cache, chosen)


if __name__ == '__main__':
    main()
