#!/usr/bin/env python3
"""Measure a small chart sample from linear XYZ D50 patch CSVs, without dependencies.

Input: method,patch,X,Y,Z. Reference: verbatim DCamProf cc24_ref[ -new ].cie.
One exposure scalar per method matches D05 (neutral 5), then CIEDE2000 is
computed under D50 with kL=kC=kH=1. No per-patch fit or chroma optimization.
"""
import argparse
import csv
import json
import math
from pathlib import Path


def lab(xyz):
    white = (0.96422, 1.0, 0.82521)
    delta = 6 / 29
    f = [v ** (1 / 3) if v > delta**3 else v / (3 * delta**2) + 4 / 29
         for v in (x / w for x, w in zip(xyz, white))]
    return (116 * f[1] - 16, 500 * (f[0] - f[1]), 200 * (f[1] - f[2]))


def delta_e_2000(p, q):
    """Sharma/Wu/Dalal 2005 CIEDE2000 equations, including hue wrap cases."""
    l1, a1, b1 = p
    l2, a2, b2 = q
    cbar = (math.hypot(a1, b1) + math.hypot(a2, b2)) / 2
    g = (1 - math.sqrt(cbar**7 / (cbar**7 + 25**7))) / 2
    ap1, ap2 = (1 + g) * a1, (1 + g) * a2
    c1, c2 = math.hypot(ap1, b1), math.hypot(ap2, b2)
    h1, h2 = math.degrees(math.atan2(b1, ap1)) % 360, math.degrees(math.atan2(b2, ap2)) % 360
    dl, dc = l2 - l1, c2 - c1
    dh = h2 - h1
    if c1 * c2 == 0:
        dh = 0
    elif dh > 180:
        dh -= 360
    elif dh < -180:
        dh += 360
    dh = 2 * math.sqrt(c1 * c2) * math.sin(math.radians(dh / 2))
    lb, cb = (l1 + l2) / 2, (c1 + c2) / 2
    if c1 * c2 == 0:
        hb = h1 + h2
    elif abs(h1 - h2) <= 180:
        hb = (h1 + h2) / 2
    elif h1 + h2 < 360:
        hb = (h1 + h2 + 360) / 2
    else:
        hb = (h1 + h2 - 360) / 2
    t = (1 - 0.17 * math.cos(math.radians(hb - 30))
         + 0.24 * math.cos(math.radians(2 * hb))
         + 0.32 * math.cos(math.radians(3 * hb + 6))
         - 0.20 * math.cos(math.radians(4 * hb - 63)))
    sl = 1 + 0.015 * (lb - 50)**2 / math.sqrt(20 + (lb - 50)**2)
    sc, sh = 1 + 0.045 * cb, 1 + 0.015 * cb * t
    rt = (-2 * math.sqrt(cb**7 / (cb**7 + 25**7))
          * math.sin(math.radians(60 * math.exp(-((hb - 275) / 25)**2))))
    return math.sqrt((dl / sl)**2 + (dc / sc)**2 + (dh / sh)**2 + rt * dc / sc * dh / sh)


def reference(path):
    rows = Path(path).read_text().split('BEGIN_DATA\n', 1)[1].split('END_DATA', 1)[0]
    return {r[0]: tuple(float(x) / 100 for x in r[4:7]) for line in rows.splitlines() if (r := line.split())}


def measure(patches, refs, neutral):
    groups = {}
    for row in csv.DictReader(Path(patches).open()):
        xyz = tuple(float(row[c]) for c in ('X', 'Y', 'Z'))
        if not all(math.isfinite(v) for v in xyz):
            raise ValueError('nonfinite patch measurement')
        group = groups.setdefault(row['method'], {})
        if row['patch'] in group:
            raise ValueError('duplicate patch')
        group[row['patch']] = xyz
    results = {}
    for method, rows in groups.items():
        if set(rows) != set(refs):
            raise ValueError(f'{method}: patch IDs do not match reference')
        if rows[neutral][1] <= 0:
            raise ValueError('neutral must have positive luminance')
        gain = refs[neutral][1] / rows[neutral][1]
        errors = {name: delta_e_2000(lab(tuple(v * gain for v in xyz)), lab(refs[name])) for name, xyz in rows.items()}
        ordered = sorted(errors.values())
        results[method] = {'patch_count': len(errors), 'exposure_gain': gain,
                           'mean_delta_e_2000': sum(ordered) / len(ordered),
                           'max_delta_e_2000': ordered[-1], 'median_delta_e_2000': (ordered[(len(ordered)-1)//2] + ordered[len(ordered)//2])/2,
                           'skin_mean_delta_e_2000': (errors['A01'] + errors['A02']) / 2,
                           'meets_mean_below_2_5': sum(ordered) / len(ordered) < 2.5,
                           'patch_delta_e_2000': errors}
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('patches')
    parser.add_argument('reference')
    parser.add_argument('--neutral', default='D05')
    args = parser.parse_args()
    print(json.dumps(measure(args.patches, reference(args.reference), args.neutral), indent=2))


if __name__ == '__main__':
    main()
