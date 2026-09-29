"""Explicit real-GPU upscale + selected-only library cleanup acceptance.

Requires an enabled SeedVR2 endpoint. Importing the input makes a fresh document;
the source file and all pre-existing library entries remain untouched.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import time
import urllib.error
import urllib.request
import uuid
import zipfile

import numpy as np
from PIL import Image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', default='http://127.0.0.1:51249')
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--width', type=int, default=3840)
    parser.add_argument('--height', type=int, default=2160)
    parser.add_argument('--seed', type=int, default=0)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    source_bytes = args.source.read_bytes(); source_hash = hashlib.sha256(source_bytes).hexdigest()
    html = urllib.request.urlopen(args.url + '/remove').read().decode()
    token = re.search(r"const TOKEN='([^']+)'", html)[1]
    report = {'complete': False, 'real_gpu': True, 'source': str(args.source.resolve()), 'source_sha256': source_hash}

    def request(path, body=None, *, binary=None, content_type='application/json'):
        data = binary if binary is not None else json.dumps(body).encode() if body is not None else None
        req = urllib.request.Request(args.url + path, data=data,
            headers={'Content-Type': content_type, 'x-local-remove-token': token})
        try:
            with urllib.request.urlopen(req, timeout=1800) as response:
                data = response.read()
                return json.loads(data) if 'application/json' in response.headers.get('Content-Type', '') else data
        except urllib.error.HTTPError as error:
            raise RuntimeError(f'{path}: HTTP {error.code} ' + error.read().decode()) from error

    def upload(path, name, content):
        boundary = 'local-image-qa-' + uuid.uuid4().hex
        body = (f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="{name}"\r\n'
                'Content-Type: application/octet-stream\r\n\r\n').encode() + content + f'\r\n--{boundary}--\r\n'.encode()
        return request(path, binary=body, content_type='multipart/form-data; boundary=' + boundary)

    def export_png(data):
        saved = request('/api/local-remove/session/' + data['id'] + '/save', {'revision': data['revision'], 'format': 'png'})
        return request(saved['download'])

    def export_project(data):
        request('/api/local-remove/session/' + data['id'] + '/export-project', {'revision': data['revision']})
        return request('/api/local-remove/session/' + data['id'] + '/download-project')

    def persist():
        (args.output / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')

    status = request('/api/local-remove/generation/upscale/models')
    assert status['enabled'], status['reason']
    assert status['model']['available'], status['model']['reason']
    before = request('/api/local-remove/generation/library')
    existing_ids = {item['id'] for item in before['items']}
    source = upload('/api/local-remove/import', 'upscale-source.png', source_bytes)
    source_png = export_png(source)
    source_project = export_project(source); (args.output / 'source.lremove').write_bytes(source_project)
    report.update(source_session_id=source['id'], library_before={'count': before['count'], 'bytes': before['bytes']})
    payload = {'session_id': source['id'], 'revision': source['revision'], 'width': args.width, 'height': args.height, 'seed': args.seed}
    report['request'] = payload; persist()
    started = time.monotonic()
    response = request('/api/local-remove/generation/upscale', payload)
    result = response['session']
    report.update(seconds=round(time.monotonic() - started, 3), result_session_id=result['id'], upscale=result['upscale'], library_warning=response['library_warning'])
    assert result['id'] != source['id'] and result['dirty'] and not response['library_warning']
    assert result['upscale']['seed'] == args.seed
    output = export_png(result); (args.output / 'upscaled.png').write_bytes(output)
    with Image.open(io.BytesIO(output)) as image:
        assert image.size == (args.width, args.height)
        output_array = np.array(image.convert('RGBA'))
    with Image.open(io.BytesIO(source_png)) as image:
        expected_alpha = np.array(image.convert('RGBA').getchannel('A').resize((args.width, args.height), Image.Resampling.LANCZOS))
    assert np.array_equal(output_array[:, :, 3], expected_alpha)
    report['output'] = {'size': [args.width, args.height], 'alpha_preserved_exactly': True, 'sha256': hashlib.sha256(output).hexdigest()}
    project = export_project(result); (args.output / 'upscaled.lremove').write_bytes(project)
    with zipfile.ZipFile(io.BytesIO(project)) as archive:
        manifest = json.loads(archive.read('manifest.json'))
        assert manifest['upscale'] == result['upscale']
        assert manifest.get('generation') == result.get('generation')
    restored = upload('/api/local-remove/import-project', 'upscaled.lremove', project)['session']
    assert restored['upscale'] == result['upscale']
    library = request('/api/local-remove/generation/library')
    item = next(item for item in library['items'] if item['id'] == result['id'])
    assert item['model'] == 'seedvr2' and item['width'] == args.width and item['upscale'] == result['upscale']
    opened = request('/api/local-remove/generation/library/' + result['id'] + '/open', {})['session']
    assert opened['id'] != result['id'] and opened['upscale'] == result['upscale']
    opened_before = request('/api/local-remove/session/' + opened['id'])
    opened_project = export_project(opened); (args.output / 'opened-library.lremove').write_bytes(opened_project)
    assert result['id'] not in existing_ids
    cleared = request('/api/local-remove/generation/library/delete', {'ids': [result['id']]})
    assert cleared['deleted'] == [result['id']]
    assert cleared['freed_bytes'] == item['bytes']
    assert cleared['bytes'] == library['bytes'] - item['bytes']
    assert existing_ids <= {entry['id'] for entry in cleared['items']}
    assert result['id'] not in {entry['id'] for entry in request('/api/local-remove/generation/library')['items']}
    opened_after = request('/api/local-remove/session/' + opened['id'])
    for key in ('id', 'revision', 'generation', 'upscale', 'layers', 'width', 'height'):
        assert opened_before.get(key) == opened_after.get(key)
    assert request('/api/local-remove/session/' + opened['id'] + '/download-project') == opened_project
    assert request('/api/local-remove/session/' + source['id'] + '/download-project') == source_project
    assert export_png(source) == source_png
    with Image.open(io.BytesIO(export_png(opened))) as image:
        assert np.array_equal(np.array(image.convert('RGBA')), output_array)
    assert hashlib.sha256(args.source.read_bytes()).hexdigest() == source_hash
    assert (args.output / 'upscaled.lremove').read_bytes() == project
    report.update(complete=True, project_roundtrip=True, library_opened_session_id=opened['id'],
        deleted_library_ids=cleared['deleted'], freed_bytes=cleared['freed_bytes'],
        library_after={'count': cleared['count'], 'bytes': cleared['bytes']},
        source_unchanged=True, opened_document_preserved=True, saved_projects_preserved=True,
        existing_library_entries_preserved=True)
    persist()
    print(json.dumps(report, indent=2), flush=True)


if __name__ == '__main__':
    main()
