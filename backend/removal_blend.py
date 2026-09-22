"""Mask cleanup and seamless, selection-limited object-removal composition."""
import cv2
import numpy as np
from PIL import Image


def clean_selection_mask(mask):
    values = np.asarray(mask.convert('L')).copy()
    # Preserve antialiasing beside real strokes, discard isolated 1/255 pixels.
    # A deliberately uniformly faint brush remains a valid low-opacity selection.
    if (values >= 2).any():
        near_stroke = cv2.dilate((values >= 2).astype(np.uint8), np.ones((5, 5), np.uint8)) > 0
        values[(values == 1) & ~near_stroke] = 0
    return Image.fromarray(values)


def blend_patch(source, generated, mask, generation_size=None):
    """Match patch gradients to the surrounding photograph without ghosting.

    NORMAL_CLONE uses generated image gradients inside the selection and the
    original's boundary values. Unlike ordinary feathering or MIXED_CLONE, it
    doesn't copy the removed object's texture back into the selected area.
    Return unweighted color: clients apply the original selection alpha once.
    """
    source, generated, mask = source.convert('RGB'), generated.convert('RGB'), mask.convert('L')
    if source.size != generated.size or source.size != mask.size:
        raise ValueError('Blend source, result, and mask must have identical dimensions')
    support = np.asarray(mask) > 0
    if not support.any():
        return source.copy()
    if support.all():
        return generated.copy()
    original = np.asarray(source)
    result = np.asarray(generated)
    binary = support.astype(np.uint8) * 255
    # Very small marks lack a meaningful boundary system for Poisson blending.
    x, y, width, height = cv2.boundingRect(binary)
    if width < 4 or height < 4:
        return Image.fromarray(np.where(support[..., None], result, original))

    # The surrounding crop supplies real boundary pixels. Reflect at the photo
    # edge; never introduce a black frame or move a border-touching selection.
    pad = 32
    src = cv2.copyMakeBorder(result, pad, pad, pad, pad, cv2.BORDER_REFLECT_101)
    dst = cv2.copyMakeBorder(original, pad, pad, pad, pad, cv2.BORDER_REFLECT_101)
    selection = cv2.copyMakeBorder(binary, pad, pad, pad, pad, cv2.BORDER_REFLECT_101)
    # Avoid putting the selected object's reflected original color on an
    # artificial boundary outside the actual photograph.
    exterior = np.ones(selection.shape, dtype=bool)
    exterior[pad:-pad, pad:-pad] = False
    extrapolated = exterior & (selection > 0)
    dst[extrapolated] = src[extrapolated]
    selection[:2] = 0; selection[-2:] = 0
    selection[:, :2] = 0; selection[:, -2:] = 0
    center = (src.shape[1] // 2, src.shape[0] // 2)
    blended = cv2.seamlessClone(src, dst, selection, center, cv2.NORMAL_CLONE_WIDE)[pad:-pad, pad:-pad]
    # Enforce exact preservation even if OpenCV changes its unselected output.
    blended[~support] = original[~support]
    return Image.fromarray(blended)
