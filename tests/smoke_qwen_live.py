"""Explicit real-GPU acceptance: generated fixtures, Qwen edits, PNGs, composite.

Run against an isolated app backend; never overwrites an input photo.
python tests/smoke_qwen_live.py --url http://127.0.0.1:51248 --variant int8
"""
import argparse
import base64
import io
import json
from pathlib import Path
import re
import time
import urllib.error
import urllib.request
import uuid

import numpy as np
from PIL import Image, ImageDraw


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--url', default='http://127.0.0.1:51248')
    parser.add_argument('--variant', choices=['int8', 'bf16'], default='int8')
    parser.add_argument('--fixture-dir', type=Path, default=Path('qa-artifacts/fixtures'))
    parser.add_argument('--output', type=Path, default=Path('qa-artifacts/qwen/live'))
    parser.add_argument('--cat-only', action='store_true')
    parser.add_argument('--resume-backpack')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    page = urllib.request.urlopen(args.url + '/remove').read().decode()
    token = re.search(r"const TOKEN='([^']+)'", page)[1]
    results = {'variant': args.variant, 'tests': [], 'sessions': []}
    if args.resume_backpack and (args.output / 'results.json').exists():
        results = json.loads((args.output / 'results.json').read_text())

    def request(path, body=None, method=None, content_type='application/json'):
        raw = json.dumps(body).encode() if isinstance(body, dict) else body
        req = urllib.request.Request(args.url + path, data=raw, method=method,
            headers={'x-local-remove-token': token, 'Content-Type': content_type})
        try:
            with urllib.request.urlopen(req, timeout=1800) as response:
                data = response.read()
                return json.loads(data) if 'application/json' in response.headers.get('Content-Type', '') else data
        except urllib.error.HTTPError as error:
            raise RuntimeError(f'{path}: {error.code} ' + error.read().decode()) from error

    def upload(path):
        boundary = 'localremove' + uuid.uuid4().hex
        body = (f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="{path.name}"\r\nContent-Type: image/png\r\n\r\n'.encode()
                + path.read_bytes() + f'\r\n--{boundary}--\r\n'.encode())
        session = request('/api/local-remove/import', body, content_type='multipart/form-data; boundary=' + boundary)
        results['sessions'].append(session['id'])
        return session

    def action(session, suffix, payload, label, method=None):
        started = time.monotonic()
        print('RUN ' + label, flush=True)
        updated = request('/api/local-remove/session/' + session['id'] + suffix,
                          {'revision': session['revision'], **payload}, method=method)
        record = {'name': label, 'seconds': round(time.monotonic() - started, 2), 'revision': updated['revision']}
        results['tests'].append(record)
        (args.output / 'results.json').write_text(json.dumps(results, indent=2))
        print('PASS ' + json.dumps(record), flush=True)
        return updated

    def export(session, name):
        saved = request('/api/local-remove/session/' + session['id'] + '/save', {'revision': session['revision'], 'format': 'png'})
        data = request(saved['download'])
        target = args.output / name
        target.write_bytes(data)
        with Image.open(target) as image:
            a = np.array(image.convert('RGBA'))[:, :, 3]
            info = {'file': name, 'mode': image.mode, 'size': image.size,
                    'alpha_range': [int(a.min()), int(a.max())], 'transparent_fraction': float((a == 0).mean()),
                    'soft_alpha_fraction': float(((a > 0) & (a < 255)).mean())}
        results['tests'][-1]['output'] = info
        (args.output / 'results.json').write_text(json.dumps(results, indent=2))
        print(json.dumps(info), flush=True)
        return target

    status = request('/api/local-remove/qwen/status')
    assert any(v['id'] == args.variant and v['available'] for v in status['variants']), status
    (args.output / 'status.json').write_text(json.dumps(status, indent=2))
    if not args.cat_only:
        backpack = request('/api/local-remove/session/' + args.resume_backpack) if args.resume_backpack else upload(args.fixture_dir / 'backpack-cup.png')
        mask = Image.new('L', (backpack['width'], backpack['height']))
        draw = ImageDraw.Draw(mask)
        sx, sy = backpack['width'] / 1536, backpack['height'] / 1024
        draw.rectangle((int(1070*sx), int(605*sy), int(1330*sx), int(835*sy)), fill=255)
        data = io.BytesIO(); mask.save(data, format='PNG')
        if not args.resume_backpack:
            backpack = action(backpack, '/remove', {'mask': base64.b64encode(data.getvalue()).decode(),
                'model': 'qwen', 'variant': args.variant, 'prompt': 'Remove the red ceramic cup and its shadow on the right. Keep the backpack unchanged.', 'seed': 42}, 'Remove red cup')
        removed = export(backpack, 'backpack-cup-removed.png')
        original = np.array(Image.open(args.fixture_dir / 'backpack-cup.png').convert('RGB'))
        edited = np.array(Image.open(removed).convert('RGB'))
        assert np.array_equal(original[np.array(mask) == 0], edited[np.array(mask) == 0]), 'Pixels outside the selection changed'
        backpack = action(backpack, '/cutout', {'variant': args.variant, 'prompt': 'Extract only the tan canvas backpack, including all straps, handle and buckles. Remove the stone ledge, garden and all other objects.', 'seed': 42}, 'Extract backpack')
        export(backpack, 'backpack-cutout.png')
        backpack = action(backpack, '/cutout/generate-background', {'variant': args.variant, 'prompt': 'A warm beige studio wall meeting a matte stone tabletop, soft window light from upper left. Empty clear tabletop for a backpack product photograph. No objects.', 'seed': 123}, 'Generate empty studio background')
        export(backpack, 'backpack-composite.png')
        backpack = action(backpack, '/cutout', {'shadow': {'enabled': True, 'opacity': .3, 'blur': 18, 'offset_x': 20, 'offset_y': 16}}, 'Apply cutout shadow', method='PATCH')
        export(backpack, 'backpack-composite-shadow.png')
        backpack = action(backpack, '/cutout', {'transform': {'offset_x': 200, 'offset_y': 100, 'scale': .78, 'rotation': -4},
            'shadow': {'squeeze': .22, 'blur': 20, 'offset_x': 35, 'offset_y': 14}}, 'Move, scale and rotate subject', method='PATCH')
        export(backpack, 'backpack-transformed.png')
        project = request('/api/local-remove/session/' + backpack['id'] + '/export-project', {'revision': backpack['revision']})
        (args.output / 'backpack-project.lremove').write_bytes(request('/api/local-remove/session/' + backpack['id'] + '/download-project'))
    cat = upload(args.fixture_dir / 'cat-fur.png')
    cat = action(cat, '/cutout', {'variant': args.variant, 'prompt': 'Extract only the long-haired cream and ginger cat, including its whiskers, fine fur, paws and tail. Remove the chair and the entire room.', 'seed': 123}, 'Extract cat with fine fur')
    export(cat, 'cat-cutout.png')
    results['complete'] = True
    (args.output / 'results.json').write_text(json.dumps(results, indent=2))
    print('All requested GPU operations completed; visually review the exported PNGs.', flush=True)


if __name__ == '__main__':
    main()
