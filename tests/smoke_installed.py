"""Exercise a running packaged backend with synthetic photos only.

Usage: python tests/smoke_installed.py <isolated-data-folder>
The caller starts the packaged host with LOCAL_REMOVE_DATA_DIR set to that folder.
"""
import base64
import io
import json
from pathlib import Path
import re
import sys
import urllib.error
import urllib.request
import zipfile

import numpy as np
from PIL import Image
import tifffile

root = Path(sys.argv[1]).resolve()
base = 'http://127.0.0.1:51247'
key = (root / 'state' / 'launcher.key').read_text().strip()


def request(path, data=None, token=None, native=False):
    headers = {'Content-Type': 'application/json', 'Origin': base}
    if native:
        headers['x-local-launcher'] = key
    if token:
        headers['x-local-remove-token'] = token
    req = urllib.request.Request(base + path, headers=headers,
        data=json.dumps(data).encode() if data is not None else None)
    with urllib.request.urlopen(req, timeout=60) as response:
        body = response.read()
        return json.loads(body) if response.headers.get_content_type() == 'application/json' else body


runtime = request('/api/local-remove/runtime')
assert Path(runtime['data_root']).resolve() == root
request('/api/local-remove/heartbeat', {}, native=True)
html = request('/remove').decode()
token = re.search(r'const TOKEN\s*=\s*[\'"]([^\'"]+)', html)
if not token:
    token = re.search(r'(?:TOKEN|CSRF|csrfToken|token)\s*[:=]\s*[\'"]([^\'"]{20,})', html)
assert token, 'Could not find the rendered editor token'
token = token.group(1)
assert 'AI connection...' in html and '__TOKEN__' not in html

fixtures = root / 'synthetic test photos'
fixtures.mkdir(exist_ok=True)
pixels = np.random.default_rng(7).integers(0, 65535, (96, 128, 3), dtype=np.uint16)
photo = fixtures / '16-bit compressed photo.tif'
tifffile.imwrite(photo, pixels, photometric='rgb', compression='deflate', metadata=None)
session = request('/api/local-remove/open-local', {'path': str(photo)}, native=True)
assert session['bit_depth'] == 16
mask = Image.new('L', (128, 96))
mask.paste(255, (55, 40, 65, 50))
buffer = io.BytesIO()
mask.save(buffer, format='PNG')
for method in ('telea', 'texture'):
    session = request('/api/local-remove/session/' + session['id'] + '/remove',
        {'revision': session['revision'], 'model': 'heal', 'heal_method': method,
         'mask': base64.b64encode(buffer.getvalue()).decode()}, token=token)
    assert session['layers'][-1]['heal_method'] == method

project = fixtures / 'Editable result.lremove'
saved = request('/api/local-remove/save-project',
    {'session_id': session['id'], 'revision': session['revision'], 'path': str(project)}, native=True)
assert project.exists()
with zipfile.ZipFile(project) as archive:
    assert 'manifest.json' in archive.namelist()
assert np.array_equal(tifffile.imread(photo), pixels), 'Original photo changed'
try:
    request('/api/local-remove/reload-config', {})
    raise AssertionError('Runtime changes were accepted without the native credential')
except urllib.error.HTTPError as error:
    assert error.code == 403

print(json.dumps({'packaged_backend': runtime['version'], 'tiff_16bit': True,
    'quick_heal_methods': ['telea', 'texture'], 'editable_project': True,
    'original_unchanged': True, 'unauthorized_config_rejected': True}))
