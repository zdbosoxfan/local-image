"""Remove the phase-locked 2-pixel lattice from a native Qwen VAE decode.

Derived from ComfyUI-DeGrid by lunaaispace-eng, Apache License 2.0:
https://github.com/lunaaispace-eng/ComfyUI-DeGrid
Pinned upstream: 5699bc33f71e1be12523fdea105cc9c2abfe1cd0/degrid_core.py
License copy: licenses/ComfyUI-DeGrid-LICENSE.txt

Modified for Local Remove: NumPy/OpenCV CPU implementation; single RGB Pillow
image interface; no ComfyUI/Torch dependency, preview widgets or manual mode.
The kernel, phase detector, auto limit and clean-image threshold are retained.
Apply immediately after decoding, before any resize. This removes a particular
decoder lattice, not other generated texture or scene/detail inaccuracies.
"""
import cv2
import numpy as np
from PIL import Image

_KERNEL = np.array([1, -8, 28, -56, 70, -56, 28, -8, 1], dtype=np.float32) / 256
_IDENTITY = np.ones(1, dtype=np.float32)
_NEGLIGIBLE_AMP = 0.5 / 255


def _extract_grid(pixels):
    if min(pixels.shape[:2]) <= 8:
        return np.zeros_like(pixels)
    # Reflect-101 matches PyTorch's reflection padding, without repeating edges.
    bx = cv2.sepFilter2D(pixels, -1, _KERNEL, _IDENTITY, borderType=cv2.BORDER_REFLECT_101)
    by = cv2.sepFilter2D(pixels, -1, _IDENTITY, _KERNEL, borderType=cv2.BORDER_REFLECT_101)
    bxy = cv2.sepFilter2D(bx, -1, _IDENTITY, _KERNEL, borderType=cv2.BORDER_REFLECT_101)
    return bx + by - bxy


def _lattice_stats(correction):
    height, width = correction.shape[:2]
    even = correction[:height // 2 * 2, :width // 2 * 2]
    if min(even.shape[:2]) < 2:
        return dict(amp_255=0.0, checker_255=0.0, vstripe_255=0.0, hstripe_255=0.0)
    # Phase averaging cancels incoherent natural texture and retains a lattice.
    m00, m01, m10, m11 = [even[y::2, x::2].mean((0, 1), dtype=np.float64)
                         for y in range(2) for x in range(2)]
    phases = np.stack((m00, m01, m10, m11))
    return {
        'amp_255': float(np.max(np.ptp(phases, axis=0)) * 255),
        'checker_255': float(np.max(np.abs((m00+m11-m01-m10)/2)) * 255),
        'vstripe_255': float(np.max(np.abs((m00+m10-m01-m11)/2)) * 255),
        'hstripe_255': float(np.max(np.abs((m00+m01-m10-m11)/2)) * 255),
    }


def _filter_pixels(pixels):
    """Process one HWC float32 RGB image in [0,1]; return pixels and diagnostics."""
    correction = _extract_grid(pixels)
    stats = _lattice_stats(correction)
    if stats['amp_255'] < _NEGLIGIBLE_AMP * 255:
        stats.update(skipped=True, limit_255=0.0, clipped_pct=0.0)
        return pixels, stats

    # Match upstream CHW sampling order and bound percentile work for big images.
    flat = np.abs(correction.transpose(2, 0, 1)).reshape(-1)
    if flat.size > 1_000_000:
        flat = flat[::flat.size // 1_000_000 + 1]
    limit = float(np.clip(np.quantile(flat, 0.75) * 3, 0.004, 0.05))
    stats.update(skipped=False, limit_255=limit * 255,
                 clipped_pct=float(np.mean(np.abs(correction) > limit) * 100))
    cleaned = np.clip(pixels - np.clip(correction, -limit, limit), 0, 1)
    return cleaned, stats


def degrid_qwen(image):
    """Return (RGB Pillow image, diagnostics), preserving clean images exactly.

    Only call on a newly decoded image at its native generation dimensions.
    Unselected source pixels must still be protected by the removal compositor.
    """
    if image.mode != 'RGB':
        raise ValueError('Qwen decoder cleanup expects an RGB image.')
    pixels = np.asarray(image, dtype=np.float32) / 255
    cleaned, stats = _filter_pixels(pixels)
    if stats['skipped']:
        return image, stats
    result = Image.fromarray(np.rint(cleaned * 255).astype(np.uint8), 'RGB')
    result.info.update(image.info)
    return result, stats
