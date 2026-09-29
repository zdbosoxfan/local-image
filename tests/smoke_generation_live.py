"""Explicit real-ComfyUI generation acceptance; writes only new test documents."""
import argparse
import io
import json
from pathlib import Path
import re
import time
import urllib.error
import urllib.request
import zipfile

import numpy as np
from PIL import Image


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--url', default='http://127.0.0.1:51249')
    parser.add_argument('--models', nargs='+', default=['qwen', 'z-image-turbo'])
    parser.add_argument('--output', type=Path, default=Path('qa-artifacts/v05/generation'))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    html = urllib.request.urlopen(args.url + '/remove').read().decode()
    token = re.search(r"const TOKEN='([^']+)'", html)[1]
    report = {'tests': [], 'complete': False}

    def request(path, body=None):
        req = urllib.request.Request(args.url + path, data=json.dumps(body).encode() if body is not None else None,
            headers={'Content-Type': 'application/json', 'x-local-remove-token': token})
        try:
            with urllib.request.urlopen(req, timeout=1800) as response:
                data = response.read()
                return json.loads(data) if 'application/json' in response.headers.get('Content-Type', '') else data
        except urllib.error.HTTPError as error:
            raise RuntimeError(f'{path}: {error.code} ' + error.read().decode()) from error

    def persist():
        (args.output / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')

    def run(label, payload):
        started = time.monotonic()
        print('RUN ' + label, flush=True)
        generated = request('/api/local-remove/generation', payload)
        session = generated['session']
        record = {'label': label, 'seconds': round(time.monotonic() - started, 2), 'request': payload,
                  'session_id': session['id'], 'generation': session['generation']}
        assert session['dirty'] and session['revision'] == 1
        assert session['generation']['seed'] == payload['seed']
        saved = request('/api/local-remove/session/' + session['id'] + '/save', {'revision': session['revision'], 'format': 'png'})
        output = request(saved['download'])
        name = label + '.png'; (args.output / name).write_bytes(output)
        with Image.open(io.BytesIO(output)) as image:
            alpha = np.array(image.convert('RGBA'))[:, :, 3]
            assert image.size == (payload['width'], payload['height'])
            if payload.get('transparent'):
                assert (alpha == 0).mean() > .01 and (alpha > 128).mean() > .01
            else:
                assert np.all(alpha == 255)
            record['output'] = {'file': name, 'size': list(image.size), 'mode': image.mode,
                'alpha_range': [int(alpha.min()), int(alpha.max())],
                'transparent_fraction': float((alpha == 0).mean())}
        request('/api/local-remove/session/' + session['id'] + '/export-project', {'revision': session['revision']})
        project = request('/api/local-remove/session/' + session['id'] + '/download-project')
        (args.output / (label + '.lremove')).write_bytes(project)
        with zipfile.ZipFile(io.BytesIO(project)) as archive:
            manifest = json.loads(archive.read('manifest.json'))
            assert manifest['generation'] == session['generation']
        report['tests'].append(record); persist()
        print('PASS ' + json.dumps({k: v for k, v in record.items() if k in ('label', 'seconds', 'session_id', 'output')}), flush=True)
        return session

    inventory = request('/api/local-remove/generation/models')
    (args.output / 'models.json').write_text(json.dumps(inventory, indent=2), encoding='utf-8')
    (args.output / 'hardware.json').write_text(json.dumps(request('/api/local-remove/hardware'), indent=2), encoding='utf-8')
    for model in args.models:
        status = next(item for item in inventory['models'] if item['id'] == model)
        assert status['available'], status['reason']
        preset = {'model': model, 'variant': status['defaults']['variant'], 'width': status['defaults']['width'], 'height': status['defaults']['height'], 'seed': 0}
        prompt = ('Editorial poster for a furniture exhibition. Large clear typography at the top reads "FORM & LIGHT". '
                  'Below it a single red velvet mid-century reading chair, oak legs, quiet beige room beside a tall window, '
                  'natural afternoon light, beautifully detailed interior photography. A small line at the bottom reads '
                  '"DESIGN IN EVERYDAY LIFE". Balanced layout, elegant typography, no people.' if model == 'hidream-o1' else
                  'A single red velvet mid-century reading chair, oak legs, in a quiet beige room beside a tall window, natural afternoon light, detailed editorial interior photography, no people, no text.')
        opaque = run(model + '-text', {**preset, 'prompt': prompt})
        if model == 'qwen':
            transparent = run('qwen-transparent', {**preset, 'seed': 42, 'transparent': True,
                'prompt': 'One small orange origami fox made from folded matte paper, standing in a three-quarter view, complete ears and tail, centered, isolated on a transparent background.'})
            run('qwen-multiple-references', {**preset, 'seed': 123,
                'prompt': 'Put the orange origami fox from <image2> onto the seat of the red chair in <image1>. Preserve the chair, room, sunlight, perspective and the fox design. The fox should be small enough to stand comfortably on the seat.',
                'reference_session_ids': [opaque['id'], transparent['id']]})
        else:
            payload = {**preset, 'seed': 123, 'reference_session_ids': [opaque['id']],
                'prompt': 'Change the chair upholstery to deep blue velvet. Keep the chair shape, oak legs, room, composition, any existing typography and natural window light.'}
            if model == 'z-image-turbo':
                payload['denoise'] = .55
            run(model + '-image', payload)
    report['complete'] = True; persist()
    print('All requested live model workflows completed. Visually inspect the PNGs.', flush=True)


if __name__ == '__main__':
    main()
