"""Portable restoration provenance, distinct from original generation settings."""
KEYS = {'model', 'variant', 'source_width', 'source_height', 'width', 'height', 'seed'}


def validate_upscale_size(source, output):
    sw, sh = source; width, height = output
    if (any(type(value) is not int or not 1 <= value <= 2**53 - 1 for value in (*source, *output))
            or min(width, height) < 2 or width % 2 or height % 2):
        raise ValueError('SeedVR2 requires even output dimensions with a shorter edge of at least 2 pixels; its postprocessor crops odd edges.')
    if width < sw or height < sh or (width, height) == (sw, sh):
        raise ValueError('Choose a larger output size; upscaling never reduces the source image.')
    if min(abs(height - width * sh / sw), abs(width - height * sw / sh)) > 2:
        raise ValueError('Keep the source aspect ratio when upscaling (within two pixels for even rounding).')


def validate_upscale_metadata(value):
    if (not isinstance(value, dict) or set(value) != KEYS or value.get('model') != 'seedvr2'
            or value.get('variant') != 'fp16' or type(value.get('seed')) is not int
            or not 0 <= value['seed'] <= 2**53 - 1):
        raise ValueError('Image upscaling settings are invalid.')
    validate_upscale_size((value['source_width'], value['source_height']), (value['width'], value['height']))
    return dict(value)
