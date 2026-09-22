"""Installed project/cache/close acceptance, using only generated test photos."""
import base64
import hashlib
import io
import json
from pathlib import Path
import re
import time
import urllib.error
import urllib.request
import zipfile

import numpy as np
from PIL import Image, ImageCms
import tifffile

HERE = Path(__file__).resolve().parent
CONNECTOR = Path(r'C:\Users\Owner\Documents\RapidRAW-AI-Connector')
BASE = 'http://127.0.0.1:5000'
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
token = re.search(r"TOKEN='([^']+)'", opener.open(BASE+'/remove').read().decode()).group(1)
key = (CONNECTOR/'local-remove-data/launcher.key').read_text().strip()
owned = []

def request(path, data=None, native=False, method=None):
    headers = {'x-local-remove-token': token, 'Origin': BASE, 'Content-Type': 'application/json'}
    if native:
        headers['x-local-launcher'] = key
    req = urllib.request.Request(BASE+path, None if data is None else json.dumps(data).encode(), headers, method=method)
    with opener.open(req, timeout=90) as response:
        return response.read(), {key.lower():value for key,value in response.headers.items()}

def api(path, data=None, native=False, method=None):
    return json.loads(request(path, data, native, method)[0])

def expect_status(status, path, data=None, native=False):
    try:
        api(path, data, native)
    except urllib.error.HTTPError as error:
        assert error.code == status, (status, error.code, error.read())
    else:
        raise AssertionError('Expected HTTP '+str(status))

def track(data):
    owned.append(data['id'])
    return data

def prefix(data):
    return '/api/local-remove/session/'+data['id']

def export_tiff(data):
    saved = api(prefix(data)+'/save', {'revision':data['revision'], 'return_to_source':False, 'format':'tif'})
    raw = request(saved['download'])[0]
    with tifffile.TiffFile(io.BytesIO(raw)) as image:
        return image.asarray(), image.pages[0].tags[34675].value

testdir = HERE/'fixtures'/('live-'+str(time.time_ns()))
testdir.mkdir(parents=True)
x, y = np.meshgrid(np.arange(768, dtype=np.uint32), np.arange(512, dtype=np.uint32))
original = np.stack([(x*73+y*17)%65536, (x*43+y*101)%65536, (x*61+y*29)%65536], axis=2).astype(np.uint16)
icc = ImageCms.ImageCmsProfile(ImageCms.createProfile('sRGB')).tobytes()
source = testdir/'Editable 16-bit test.tif'
tifffile.imwrite(source, original, photometric='rgb', extratags=[(34675, 'B', len(icc), icc, False)])
source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
collection = api('/api/local-remove/open-files', {'paths':[str(source)]}, True)['collection']
entry = collection['entries'][0]['id']
data = track(api(f"/api/local-remove/collection/{collection['id']}/entry/{entry}/open", {})['session'])
mask = Image.new('L', (768,512), 0)
mask.paste(255, (300,200,307,209))
buffer = io.BytesIO(); mask.save(buffer, format='PNG')
data = api(prefix(data)+'/remove', {'revision':data['revision'], 'model':'heal', 'heal_method':'telea', 'mask':base64.b64encode(buffer.getvalue()).decode()})
data = api(prefix(data)+'/merge', {'revision':data['revision']})
first = data['layers'][0]['id']
data = api(prefix(data)+'/layer/'+first, {'revision':data['revision'], 'visible':False, 'discarded':True}, method='PATCH')
before, before_icc = export_tiff(data)
assert before.dtype == np.uint16 and before_icc == icc
assert np.array_equal(before[np.array(mask)==0], original[np.array(mask)==0])
project = testdir/'Saved edit.lremove'
saved = api('/api/local-remove/save-project', {'session_id':data['id'], 'revision':data['revision'], 'path':str(project)}, True)
data = saved['session']; assert saved['saved'] and data['project_saved']
project_hash = hashlib.sha256(project.read_bytes()).hexdigest()
with zipfile.ZipFile(project) as archive:
    manifest = json.loads(archive.read('manifest.json'))
    assert 'source_path' not in manifest and 'project_path' not in manifest
    assert all(':' not in name and '/' not in name and '\\' not in name for name in archive.namelist())
    assert any(name.endswith('-snapshot.tif') for name in archive.namelist())

base, base_headers = request(prefix(data)+'/base-display')
rgba_raw, rgba_headers = request(prefix(data)+'/layer/'+first+'/display')
assert 'immutable' in base_headers['cache-control'] and 'immutable' in rgba_headers['cache-control']
rgba = Image.open(io.BytesIO(rgba_raw)); assert rgba.mode == 'RGBA'
native_mask = Image.open(CONNECTOR/'local-remove-data/sessions'/data['id']/data['layers'][0]['mask'])
assert np.array_equal(np.array(rgba.getchannel('A')), np.array(native_mask))

api(prefix(data)+'/close', {'revision':data['revision'], 'discard':True})
expect_status(404, prefix(data))
updated = api('/api/local-remove/collection/'+collection['id'])
assert updated['entries'][0]['session_id'] is None
assert hashlib.sha256(source.read_bytes()).hexdigest() == source_hash
assert hashlib.sha256(project.read_bytes()).hexdigest() == project_hash
reopened = track(api('/api/local-remove/open-project', {'path':str(project)}, True)['session'])
assert reopened['id'] != data['id'] and not reopened['can_return']
assert reopened['layers'] == data['layers']
after, after_icc = export_tiff(reopened)
assert np.array_equal(before, after) and after_icc == icc
second = reopened['layers'][1]['id']
samples = []
for index in range(6):
    start = time.perf_counter()
    raw, _ = request(prefix(reopened)+'/layer/'+second, {'revision':reopened['revision'], 'visible':index%2==1}, method='PATCH')
    samples.append({'seconds':round(time.perf_counter()-start, 4), 'bytes':len(raw)})
    reopened = json.loads(raw)
reopened = api(prefix(reopened)+'/layer/'+second, {'revision':reopened['revision'], 'visible':False}, method='PATCH')
restored, _ = export_tiff(reopened)
assert np.array_equal(restored, original)
reopened = api(prefix(reopened)+'/layer/'+second, {'revision':reopened['revision'], 'visible':True}, method='PATCH')
api('/api/local-remove/save-project', {'session_id':reopened['id'], 'revision':reopened['revision']}, True)
reopened = api(prefix(reopened))

other = track(api('/api/local-remove/open-project', {'path':str(project)}, True)['session'])
expect_status(409, '/api/local-remove/close-sessions', {'sessions':[{'id':reopened['id'], 'revision':reopened['revision']}, {'id':other['id'], 'revision':other['revision']+1}], 'discard':True})
assert api(prefix(reopened)) and api(prefix(other))
api(prefix(other)+'/close', {'revision':other['revision'], 'discard':True})
assert hashlib.sha256(source.read_bytes()).hexdigest() == source_hash

report = {'ok':True, 'source_unchanged':True, 'project_roundtrip_pixels_equal':True, 'original_restored_exactly':True,
          'native_16bit_and_icc_preserved':True, 'hidden_and_discarded_layers_restored':True, 'closed_session_deleted':True,
          'collection_reference_cleared':True, 'stale_batch_close_kept_every_image':True, 'immutable_display_assets':True,
          'rgba_alpha_matches_native_mask':True, 'toggle_roundtrips':samples, 'session':reopened['id'],
          'collection':collection['id'], 'owned_sessions':owned, 'project':str(project), 'source':str(source),
          'url':BASE+'/remove?session='+reopened['id']}
(HERE/'live-project-verification.json').write_text(json.dumps(report, indent=2))
print(json.dumps(report))
