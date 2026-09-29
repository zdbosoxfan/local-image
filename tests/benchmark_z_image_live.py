"""Explicit real-GPU step/resolution benchmark; no speed or quality guarantee."""
import io
import argparse
import json
from pathlib import Path
import re
import time
import urllib.request
from PIL import Image

BASE = 'http://127.0.0.1:51249'
OUT = Path('qa-artifacts/v05/z-image-steps')
PROMPT = ('A red ceramic teapot on a pale oak kitchen table beside a small green houseplant, '
          'soft morning window light, realistic glaze and wood grain, editorial product photography.')

def main():
    global BASE, OUT
    parser = argparse.ArgumentParser()
    parser.add_argument('--url', default=BASE)
    parser.add_argument('--output', type=Path, default=OUT)
    parser.add_argument('--seed', type=int, default=2027)
    args = parser.parse_args()
    BASE, OUT = args.url, args.output
    OUT.mkdir(parents=True, exist_ok=True)
    token = re.search(r"const TOKEN='([^']+)'", urllib.request.urlopen(BASE + '/remove').read().decode())[1]
    def request(path, data=None):
        req = urllib.request.Request(BASE + path, data=json.dumps(data).encode() if data is not None else None,
            headers={'Content-Type': 'application/json', 'x-local-remove-token': token})
        with urllib.request.urlopen(req, timeout=900) as response:
            body = response.read()
            return json.loads(body) if 'application/json' in response.headers.get('Content-Type', '') else body
    report = {'prompt': PROMPT, 'seed': args.seed, 'timing_scope': 'App request to completed response, excluding export; sequential jobs, same prompt. First job warms the model with a different seed to avoid a cached repeat. Use a new seed for subsequent benchmark runs. No LoRA.', 'runs': []}
    for index, (size, steps) in enumerate([(1024, 8), (1024, 1), (1024, 2), (1024, 4), (1024, 8), (512, 4), (512, 8)]):
        name = f'{index}-{size}px-{steps}steps'
        started = time.perf_counter()
        generated = request('/api/local-remove/generation', {'model': 'z-image-turbo', 'variant': 'bf16', 'prompt': PROMPT, 'seed': args.seed - 1 if index == 0 else args.seed, 'width': size, 'height': size, 'steps': steps})
        elapsed = round(time.perf_counter() - started, 3)
        session = generated['session']
        saved = request('/api/local-remove/session/' + session['id'] + '/save', {'revision': session['revision'], 'format': 'png'})
        image = request(saved['download'])
        (OUT / (name + '.png')).write_bytes(image)
        assert Image.open(io.BytesIO(image)).size == (size, size)
        report['runs'].append({'file': name + '.png', 'size': size, 'steps': steps, 'seconds': elapsed, 'warmup': index == 0, 'generation': session['generation']})
        (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
        print(f'{name}: {elapsed}s', flush=True)

if __name__ == '__main__':
    main()
