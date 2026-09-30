"""Owned synthetic Assets fixtures. No provider, model or GPU operation runs."""
import hashlib
import json
import os
from pathlib import Path
import sys
import urllib.request
import uuid

root = Path(__file__).resolve().parents[2]
profile = Path(sys.argv[1]).resolve()
base = sys.argv[2]
if not profile.is_relative_to((root / 'qa-artifacts').resolve()):
    raise RuntimeError('Assets fixture profile must remain inside this checkout QA directory.')
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
with opener.open(base + '/api/local-remove/runtime', timeout=5) as response:
    runtime = json.load(response)
if Path(runtime['data_root']).resolve() != profile:
    raise RuntimeError('Refusing to seed a different backend profile.')
os.environ.update(LOCAL_IMAGE_DATA_DIR=str(profile), LOCAL_REMOVE_DATA_DIR=str(profile), COMFY_HOST='127.0.0.1', COMFY_PORT='51999')
sys.path.insert(0, str(root / 'backend'))
from PIL import Image
import local_remove as editor
import generation_library

fixture = profile / 'qa-asset-fixtures' / uuid.uuid4().hex
fixture.mkdir(parents=True)
backgrounds = fixture / 'backgrounds'
backgrounds.mkdir()
for name, color in [('photo2.png', (60, 110, 155)), ('photo10.png', (170, 100, 60))]:
    Image.new('RGB', (256, 256), color).save(backgrounds / name)
credit = {'provider': 'openverse', 'asset_id': str(uuid.uuid4()), 'title': 'Controlled ocean',
          'creator': 'Fixture photographer', 'creator_url': 'https://example.invalid/photographer',
          'source_url': 'https://example.invalid/source', 'license': 'Controlled test fixture',
          'license_url': 'https://example.invalid/license', 'attribution': 'Controlled fixture credit; no provider image was downloaded.'}
stock_sessions = []
for name, color in [('stock-image.png', (45, 80, 150)), ('stock-reference.png', (80, 145, 90))]:
    source = fixture / name
    Image.new('RGB', (256, 256), color).save(source)
    created = editor.create_session(source, name)
    data = editor.read_session(created['id'])
    data.update(source_attribution=credit, revision=1)
    editor.write_session(editor.folder(data['id']), data)
    stock_sessions.append(editor.public(data))
source = fixture / ('Generated Assets fixture-' + fixture.name[:8] + '.png')
pixels = Image.new('RGBA', (256, 256), (0, 0, 0, 0))
pixels.paste((190, 70, 50, 255), (48, 32, 216, 236))
pixels.save(source)
created = editor.create_session(source, source.name)
data = editor.read_session(created['id'])
data.update(revision=1, generation={'model': 'qwen', 'variant': 'int8', 'prompt': 'Owned synthetic Assets fixture',
    'negative_prompt': '', 'width': 256, 'height': 256, 'seed': 0, 'transparent': True,
    'steps': 25, 'guidance': 1, 'denoise': None, 'reference_count': 0, 'loras': []}, reference_attributions=[credit])
editor.write_session(editor.folder(data['id']), data)
generation_library.add_generated(pixels, data)
result = {'profile': str(profile), 'fixtureDirectory': str(fixture), 'backgroundDirectory': str(backgrounds),
          'stockSessions': stock_sessions, 'generatedId': data['id'], 'generatedName': data['name'],
          'source': str(source), 'sourceSha256': hashlib.sha256(source.read_bytes()).hexdigest(), 'attribution': credit}
print(json.dumps(result))
