#!/usr/bin/env python3
"""Paired camera-data audit metrics. Integer registration and exposure fit are fixed from noisy input.
Requires numpy; TIFF reference inputs additionally require tifffile (reference tooling only).
"""
import argparse
import json
from pathlib import Path
import numpy as np


def read(path):
    if str(path).endswith('.bin'):
        b = Path(path).read_bytes()
        w, h = np.frombuffer(b[:8], '<u4')
        return np.frombuffer(b[8:], '<f4').reshape(h, w, 3).astype(np.float64)
    import tifffile
    a = tifffile.imread(path)
    if a.ndim != 3 or a.shape[2] != 3:
        raise ValueError(f'{path}: expected RGB')
    if np.issubdtype(a.dtype, np.integer):
        a = a.astype(np.float64) / np.iinfo(a.dtype).max
    h, w = a.shape[:2]
    return a[h // 2 - 256:h // 2 + 256, w // 2 - 256:w // 2 + 256].astype(np.float64)


def mean_window(a, size=7):
    a = np.pad(a, ((1, 0), (1, 0), (0, 0))).cumsum(0).cumsum(1)
    return (a[size:, size:] - a[:-size, size:] - a[size:, :-size] + a[:-size, :-size]) / size**2


def metrics(a, b):
    mse = np.mean((a - b)**2)
    ma, mb = mean_window(a), mean_window(b)
    va, vb = mean_window(a*a) - ma*ma, mean_window(b*b) - mb*mb
    cov = mean_window(a*b) - ma*mb
    ssim = np.mean(((2*ma*mb + .01**2)*(2*cov + .03**2)) / ((ma*ma + mb*mb + .01**2)*(va + vb + .03**2)))
    return {'psnr': float(-10*np.log10(mse)), 'ssim': float(ssim)}


p = argparse.ArgumentParser(description=__doc__)
p.add_argument('truth')
p.add_argument('noisy')
p.add_argument('filtered', nargs='+')
a = p.parse_args()
truth, noisy = read(a.truth), read(a.noisy)
if truth.shape != noisy.shape:
    raise ValueError(f'shape mismatch {truth.shape} vs {noisy.shape}')
h, w = truth.shape[:2]
ref = truth[12:h-12, 12:w-12]
best = None
for dy in range(-6, 7):
    for dx in range(-6, 7):
        src = noisy[12+dy:h-12+dy, 12+dx:w-12+dx]
        gain = np.sum(ref*src, axis=(0, 1)) / np.maximum(np.sum(src*src, axis=(0, 1)), 1e-12)
        mse = np.mean((src*gain - ref)**2)
        if best is None or mse < best[0]:
            best = (mse, dy, dx, gain)
_, dy, dx, gain = best
out = {'shift_yx': [dy, dx], 'exposure_gains': gain.tolist(), 'truth': a.truth, 'region': [488, 488]}
for path in [a.noisy] + a.filtered:
    img = read(path)[12+dy:h-12+dy, 12+dx:w-12+dx]*gain
    out[path] = metrics(img, ref)
print(json.dumps(out, indent=2))
