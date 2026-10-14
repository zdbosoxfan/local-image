#!/usr/bin/env python3
"""Sample reference TIFF ROIs as XYZ D50 using its embedded matrix/TRC ICC.

Restricted deliberately to uncompressed, interleaved 16-bit RGB TIFF and ICC
parametric curves (types 0 or 4). Unsupported encodings fail, never guess a
colour space. No dependencies; use RawTherapee -t -b16 with RTv4_sRGB output.
"""
import argparse
import csv
import math
from pathlib import Path
import struct
import sys


def sample(path, rois):
    data = Path(path).read_bytes()
    endian = {'II': '<', 'MM': '>'}[data[:2].decode('ascii')]
    if struct.unpack_from(endian + 'H', data, 2)[0] != 42:
        raise ValueError('classic TIFF required')
    offset = struct.unpack_from(endian + 'I', data, 4)[0]
    tags = {}
    for i in range(struct.unpack_from(endian + 'H', data, offset)[0]):
        at = offset + 2 + i * 12
        tag, kind, count, value = struct.unpack_from(endian + 'HHII', data, at)
        sizes = {1: 1, 3: 2, 4: 4, 7: 1}
        if kind not in sizes:
            continue
        start = at + 8 if count * sizes[kind] <= 4 else value
        blob = data[start:start + count * sizes[kind]]
        tags[tag] = blob if kind in (1, 7) else struct.unpack(endian + {3: 'H', 4: 'I'}[kind] * count, blob)
    if tags[258] != (16, 16, 16) or tags[259] != (1,) or tags[262] != (2,) or tags[277] != (3,) or tags.get(284, (1,)) != (1,):
        raise ValueError('uncompressed interleaved 16-bit RGB required')
    width, height = tags[256][0], tags[257][0]
    pixels = b''.join(data[off:off + count] for off, count in zip(tags[273], tags[279]))
    if len(pixels) != width * height * 6:
        raise ValueError('invalid strip size')
    icc = tags[34675]
    entries = {}
    for i in range(struct.unpack_from('>I', icc, 128)[0]):
        sig, off, count = struct.unpack_from('>4sII', icc, 132 + 12 * i)
        entries[sig] = icc[off:off + count]
    def fixed(blob, start, count):
        return [x / 65536 for x in struct.unpack_from('>' + 'i' * count, blob, start)]
    columns, curves = [], []
    for channel in b'rgb':
        blob = entries[bytes([channel]) + b'XYZ']
        if blob[:4] != b'XYZ ': raise ValueError('matrix profile required')
        columns.append(fixed(blob, 8, 3))
        blob = entries[bytes([channel]) + b'TRC']
        if blob[:4] != b'para': raise ValueError('parametric TRC required')
        kind = struct.unpack_from('>H', blob, 8)[0]
        if kind not in (0, 4): raise ValueError('TRC must be type 0 or 4')
        curves.append((kind, fixed(blob, 12, 1 if kind == 0 else 7)))
    def linear(value, curve):
        kind, params = curve
        if kind == 0: return value ** params[0]
        g, a, b, c, d, e, f = params
        return (a * value + b)**g + e if value >= d else c * value + f
    for row in csv.DictReader(Path(rois).open()):
        x, y, w, h = [int(math.floor(float(row[key]) * size + .5)) for key, size in zip(('x','y','width','height'), (width,height,width,height))]
        if min(w,h) <= 0 or min(x,y) < 0 or x+w > width or y+h > height:
            raise ValueError('ROI outside TIFF')
        total = [0.0] * 3
        for yy in range(y, y+h):
            for xx in range(x, x+w):
                rgb = struct.unpack_from(endian + 'HHH', pixels, (yy * width + xx) * 6)
                rgb = [linear(value / 65535, curve) for value, curve in zip(rgb, curves)]
                for c in range(3): total[c] += sum(columns[k][c] * rgb[k] for k in range(3))
        yield row['patch'], [v / (w*h) for v in total]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('tiff')
    p.add_argument('rois')
    a = p.parse_args()
    w = csv.writer(sys.stdout)
    w.writerow(('method','patch','X','Y','Z'))
    for name, xyz in sample(a.tiff, a.rois): w.writerow(('reference', name, *xyz))


if __name__ == '__main__': main()
