#!/usr/bin/env python3
"""Original contract fixtures (GPL-3.0-or-later), frozen 2026-10-11; native reference
moved to format version 3 on 2026-10-14 (v2 reads only the current format).
When FORMAT_VERSION changes, re-freeze the native reference at the new version.
The manifest extends the repository's existing vector-basic fixture; catalog legacy
records freeze src/tests_format_version.rs::legacy_parts and versionless cases.
No application code or tests are executed to generate these bytes.
"""
import copy
import json
import zlib
import zipfile
from pathlib import Path
root = Path(__file__).resolve().parents[4]
native = root / 'crates/pc-format/tests/fixtures/contracts'
catalog = root / 'crates/lc-catalog/tests/fixtures/contracts'
manifest = json.loads((native.parent / 'vector-basic.json').read_text())
manifest['format_version'] = 3
manifest['generator'] = 'Local Image contract fixture (hand-authored v3)'
d = manifest['document']
d['name'] = 'Contract reference v3'
d['design_units'] = 'Millimeters'
p = d['paths'][0]['path']
black = d['layers'][0]['content']['fill']
white = {'Solid': {'mode': 'Rgb', 'c': [1., 1., 1., 0.], 'alpha': 1.}}
stroke = copy.deepcopy(d['layers'][0]['content']['stroke'])
stroke['width'] = 3.
d['layers'][0]['vector'] = {
    'corners': [{'subpath': 0, 'knot': 1, 'corner': {'radius': 2., 'kind': 'Chamfer'}}],
    'live_shape': {'rect': [2., 2., 16., 14.], 'angle': 0.25, 'parameters': {'kind': 'star', 'points': 5, 'inner': 0.5, 'corner': 2.}},
    'appearance': {'items': [
        {'kind': 'fill', 'paint': white, 'opacity': 0.625, 'blend': 'Multiply', 'visible': False},
        {'kind': 'stroke', 'base': stroke, 'profile': {'points': [{'t': 0., 'left': 1., 'right': 2.}, {'t': 0.75, 'left': 2., 'right': 3.}]}, 'brush': {'id': 'contract-brush'}, 'start_arrow': 'Triangle', 'end_arrow': 'CircleOpen', 'arrow_scale': [0.5, 2.], 'dash_align': True, 'scale_with_object': True, 'gradient_mode': 'Along'}]},
    'constraints': {'right': True, 'fixed_width': True}, 'clip': p}
d['layers'][1]['vector'] = {'on_path': {'path': p, 'start': 0.1, 'end': 0.9, 'flip': True, 'align': 'Center'}}
master = copy.deepcopy(d['layers'][0]); master.update(id=4, name='Master shape')
d['symbols'] = [{'id': 1, 'name': 'Contract symbol', 'layers': [master]}]
base = copy.deepcopy(d['layers'][1]); base.pop('vector'); base['vector_mask'] = None
instance = copy.deepcopy(base); instance.update(id=5, name='Instance', content={'kind': 'symbol', 'symbol': 1, 'transform': {'m': [1., 0., 0., 1., 3., 4.]}})
d['layers'].append(instance)
child = copy.deepcopy(d['layers'][0]); child.pop('vector'); child['vector_mask'] = None; child.update(id=7, name='Shape')
group = copy.deepcopy(base); group.update(id=6, name='Live blend', blend='PassThrough', content={'kind': 'group', 'children': [child], 'expanded': True, 'artboard': None}, vector={'live': {'kind': 'blend', 'spacing': {'Steps': 7}, 'orientation': 'AlignToPath', 'spine': p}})
d['layers'].append(group)
text = json.dumps(manifest, ensure_ascii=False, indent=2) + '\n'
(native / 'document-v3.json').write_text(text)
with zipfile.ZipFile(native / 'document-v3.pcraft', 'w', compression=zipfile.ZIP_STORED) as archive:
    entry = zipfile.ZipInfo('manifest.json', (2026, 10, 11, 0, 0, 0))
    archive.writestr(entry, text.encode())
def photo(i, current=False):
    return {'id': i, 'source': {'type': 'file', 'path': f'/contracts/{i}.jpg' if current else f'/p/{i}.jpg'}, 'file_name': f'{i}.jpg' if current else 'p.jpg', 'format': 'JPEG', 'width': 32 if current else 4, 'height': 24 if current else 3, 'imported': '2026-10-11' if current else '2026-01-01', 'develop': {}}
def snap(version, seq, c, name):
    (catalog / name).write_text(json.dumps({'format': 'lightcraft-catalog', 'version': version, 'seq': seq, 'catalog': c}, ensure_ascii=False, indent=2)+'\n')
def record(seq, op):
    body = json.dumps(op, ensure_ascii=False, separators=(',', ':'))
    return '{"seq":%d,"crc":%d,"op":%s}\n' % (seq, zlib.crc32(body.encode()), body)
legacy = {'photos': {'1': photo(1), '2': photo(2)}, 'albums': {}, 'next_photo': 3, 'next_album': 1, 'next_stack': 1}
snap(1, 2, legacy, 'catalog-v1.snap')
(catalog / 'catalog-v1.log').write_text(record(3, {'op': 'setRating', 'id': 1, 'rating': 4}))
(catalog / 'catalog-v0.log').write_text(record(1, {'op': 'addPhoto', 'photo': photo(1)}) + record(2, {'op': 'setRating', 'id': 1, 'rating': 4}))
p = photo(1, True)
p.update(rating=4, flag='pick', label='purple', meta={'title': 'Contract photo', 'keywords': ['birds', 'places|海岸']}, develop={'light': {'exposure': 1.25}, 'color': {'vibrance': 15.}, 'profile': {'id': 'lc.vivid'}})
c = {'photos': {'1': p}, 'albums': {'1': {'id': 1, 'name': 'Contract album', 'photos': [1], 'cover': 1}}, 'next_photo': 2, 'next_album': 2, 'next_stack': 1, 'label_names': {'purple': 'Deliver'}, 'browsed': {'/contracts': '2026-10-11'}}
snap(2, 4, c, 'catalog-v2.snap')
