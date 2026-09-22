"""Run one real FLUX edit against an already-running installed Local Remove.

Usage: python tests/smoke_ai_installed.py <installed-data-folder> <workspace-output-folder>
Only a newly generated synthetic photo and its own session are opened/closed.
The caller starts the dedicated ComfyUI backend first and ejects its GPU later.
This script never starts/stops services, changes setup, or touches user photos.
"""
import argparse
import base64
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
from PIL import Image, ImageDraw

BASE = 'http://127.0.0.1:51247'


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        # Native credentials must never follow a redirect to another origin.
        return None


class LocalClient:
    def __init__(self):
        self.key = None
        self.token = None
        self.opener = urllib.request.build_opener(NoRedirect)

    def request(self, path, data=None, *, native=False, token=False, timeout=30):
        if not path.startswith('/api/local-remove/') and path != '/remove':
            raise ValueError('Unexpected Local Remove route')
        headers = {'Content-Type': 'application/json', 'Origin': BASE}
        if native:
            if not self.key:
                raise RuntimeError('Native credential is not loaded')
            headers['x-local-launcher'] = self.key
        if token:
            if not self.token:
                raise RuntimeError('Editor token is not loaded')
            headers['x-local-remove-token'] = self.token
        req = urllib.request.Request(BASE + path, headers=headers,
            data=json.dumps(data).encode() if data is not None else None)
        try:
            with self.opener.open(req, timeout=timeout) as response:
                body = response.read()
                return json.loads(body) if response.headers.get_content_type() == 'application/json' else body
        except urllib.error.HTTPError as error:
            # Report the server's public detail, never request headers or keys.
            try:
                detail = json.loads(error.read()).get('detail', 'Request rejected')
            except (ValueError, AttributeError):
                detail = 'Request rejected'
            raise RuntimeError(f'Local Remove HTTP {error.code}: {detail}') from None


def gpu_snapshot(port):
    """Optional read-only observation; numbers are bytes, not an unload claim."""
    if type(port) is not int or not 1 <= port <= 65535:
        return {'available': False}
    try:
        opener = urllib.request.build_opener(NoRedirect)
        with opener.open(f'http://127.0.0.1:{port}/system_stats', timeout=10) as response:
            data = json.loads(response.read())
        devices = []
        for device in data.get('devices', []):
            devices.append({key: device[key] for key in ('name', 'type', 'vram_total', 'vram_free',
                           'torch_vram_total', 'torch_vram_free') if key in device})
        return {'available': True, 'devices': devices}
    except (OSError, ValueError, TypeError):
        return {'available': False}


def make_fixture(photo, mask_path):
    width, height = 512, 384
    y, x = np.mgrid[:height, :width]
    noise = np.random.default_rng(41).normal(0, 3.5, (height, width))
    grass = np.stack((72 + y * .08 + noise, 105 + y * .10 + noise, 53 + y * .02 + noise), axis=-1)
    sky = np.stack((126 + y * .18, 168 + y * .15, 192 + y * .13), axis=-1)
    pixels = np.where((y < 132)[..., None], sky, grass).clip(0, 255).astype(np.uint8)
    image = Image.fromarray(pixels)
    draw = ImageDraw.Draw(image)
    draw.polygon([(0, 140), (100, 107), (180, 124), (285, 99), (400, 128), (512, 104), (512, 164), (0, 164)], fill=(71, 96, 70))
    draw.polygon([(183, 384), (231, 148), (259, 148), (336, 384)], fill=(168, 156, 131))
    draw.ellipse((232, 247, 293, 269), fill=(82, 79, 62))
    draw.rounded_rectangle((238, 182, 274, 257), radius=5, fill=(177, 53, 41), outline=(116, 43, 36), width=2)
    draw.ellipse((239, 177, 273, 190), fill=(214, 115, 94), outline=(113, 50, 43))
    draw.line((246, 194, 246, 247), fill=(222, 120, 98), width=3)
    with photo.open('xb') as stream:
        image.save(stream, format='PNG')
    mask = Image.new('L', (width, height), 0)
    ImageDraw.Draw(mask).rounded_rectangle((225, 165, 303, 278), radius=9, fill=255)
    with mask_path.open('xb') as stream:
        mask.save(stream, format='PNG')
    return mask_path.read_bytes()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('installed_data_folder', type=Path)
    parser.add_argument('workspace_output_folder', type=Path)
    args = parser.parse_args()
    data_root = args.installed_data_folder.expanduser().resolve()
    output_parent = args.workspace_output_folder.expanduser().resolve()
    client = LocalClient()
    runtime = client.request('/api/local-remove/runtime')
    assert runtime.get('application') == 'local-remove', 'The selected port is not Local Remove'
    assert Path(runtime['data_root']).resolve() == data_root, 'The running backend uses a different profile'
    client.key = (data_root / 'state' / 'launcher.key').read_text(encoding='ascii').strip()
    client.request('/api/local-remove/heartbeat', {}, native=True)
    html = client.request('/remove').decode('utf-8')
    match = re.search(r"const TOKEN\s*=\s*['\"]([^'\"]+)['\"]", html)
    assert match, 'The installed editor did not render its request token'
    client.token = match.group(1)
    readiness = client.request('/api/local-remove/status')
    assert readiness.get('ready') is True, readiness.get('reason', 'FLUX is not ready')
    assert readiness.get('model_id') == 'klein', 'The selected removal model is not FLUX Klein'
    setup = client.request('/api/local-remove/setup')
    assert setup['service'].get('ready') is True, setup['service'].get('reason', 'ComfyUI is not ready')
    assert setup['service'].get('busy') is False, 'ComfyUI has queued work; run this smoke test when it is idle'
    port = setup['service']['port']

    # Unique output directories prevent overwriting previous test runs or files.
    output_parent.mkdir(parents=True, exist_ok=True)
    output = output_parent / ('flux-ai-smoke-' + time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8])
    output.mkdir()
    photo = output / 'Synthetic scene.png'
    mask_path = output / 'Selection mask.png'
    mask_bytes = make_fixture(photo, mask_path)
    original_bytes = photo.read_bytes()
    original_hash = hashlib.sha256(original_bytes).hexdigest()
    report = {'success': False, 'packaged_backend': runtime['version'], 'dimensions': [512, 384],
              'model': 'klein', 'output_directory': str(output), 'gpu_before': gpu_snapshot(port)}
    session_id = None
    error = None
    try:
        session = client.request('/api/local-remove/open-local', {'path': str(photo)}, native=True)
        session_id = session['id']
        assert session['name'] == photo.name and (session['width'], session['height']) == (512, 384)
        assert not session['layers'], 'A synthetic fixture should start without layers'
        started = time.monotonic()
        session = client.request('/api/local-remove/session/' + session_id + '/remove',
            {'revision': session['revision'], 'model': 'klein',
             'mask': base64.b64encode(mask_bytes).decode('ascii')}, token=True, timeout=300)
        report['removal_seconds'] = round(time.monotonic() - started, 2)
        assert session['id'] == session_id and len(session['layers']) == 1
        assert session['layers'][-1]['model'] == 'klein', 'FLUX did not produce the removal layer'
        assert photo.read_bytes() == original_bytes, 'The original synthetic photo changed'
        report.update(layer_model='klein', layer_count=1, original_unchanged=True,
                      original_sha256=original_hash, gpu_after=gpu_snapshot(port))

        preview = client.request('/api/local-remove/session/' + session_id + '/preview?full=true', timeout=60)
        with Image.open(io.BytesIO(preview)) as image:
            assert image.size == (512, 384), 'Preview dimensions changed'
            image.load()
        preview_path = output / 'FLUX result.jpg'
        with preview_path.open('xb') as stream:
            stream.write(preview)
        project = output / 'FLUX editable result.lremove'
        saved = client.request('/api/local-remove/save-project',
            {'session_id': session_id, 'revision': session['revision'], 'path': str(project)}, native=True, timeout=60)
        assert saved['saved'] is True and project.is_file()
        with zipfile.ZipFile(project) as archive:
            manifest = json.loads(archive.read('manifest.json'))
            assert len(manifest['layers']) == 1 and manifest['layers'][0]['model'] == 'klein'
            assert hashlib.sha256(archive.read(manifest['original'])).hexdigest() == original_hash
        assert photo.read_bytes() == original_bytes, 'Project save changed the original synthetic photo'
        report.update(success=True, preview=str(preview_path), project=str(project), editable_project=True)
    except Exception as caught:
        error = caught
        report['error'] = str(caught)
    finally:
        if session_id is not None:
            try:
                # Read the current revision, then discard only this unique fixture
                # session. Existing user sessions are never enumerated or closed.
                current = client.request('/api/local-remove/session/' + session_id)
                assert current['name'] == photo.name and current['id'] == session_id
                closed = client.request('/api/local-remove/close-sessions',
                    {'sessions': [{'id': session_id, 'revision': current['revision']}], 'discard': True},
                    token=True, timeout=300)
                assert closed['closed'] == [session_id]
                report['test_session_closed'] = True
            except Exception as cleanup_error:
                report['test_session_closed'] = False
                report['cleanup_error'] = str(cleanup_error)
                report['success'] = False
                error = error or cleanup_error
        report['original_unchanged'] = photo.read_bytes() == original_bytes
        with (output / 'smoke-result.json').open('x', encoding='utf-8') as stream:
            json.dump(report, stream, indent=2)
    print(json.dumps(report, indent=2))
    if error:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
