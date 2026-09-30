"""Publisher guidance, kept distinct from connected workflow constraints.

These are documented presets, not claims that a model has a hard native size cap.
The catalog and request validation own the actual limits accepted by this app.
"""
from copy import deepcopy


VERIFIED_ON = '2026-09-30'
QWEN_SOURCE = 'https://github.com/QwenLM/Qwen-Image-2.1#default-parameters'
QWEN_TEMPLATE = 'https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_qwen_image_2_1_t2i.json'
Z_SOURCE = 'https://huggingface.co/Tongyi-MAI/Z-Image-Turbo#-quick-start'
Z_TEMPLATE = 'https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_z_image_turbo.json'
ERNIE_SOURCE = 'https://huggingface.co/baidu/ERNIE-Image#recommended-parameters'
KLEIN_SOURCE = 'https://github.com/black-forest-labs/flux2#the-klein-family'


def _sizes(*pairs):
    return [{'width': width, 'height': height} for width, height in pairs]


GUIDANCE = {
    'qwen': {
        'steps': {
            'recommended': 40,
            'source_label': 'Qwen publisher default',
            'source_url': QWEN_SOURCE,
            'workflow_source_url': QWEN_TEMPLATE,
            'note': 'Qwen publishes 40 steps. The ComfyUI template starts at 25; that is a faster template preset, not the publisher default. Applies to both INT8 and BF16.',
        },
        'resolution': {
            'recommended_sizes': _sizes((2048, 2048), (2400, 1792), (1792, 2400), (2528, 1696), (1696, 2528), (2752, 1536), (1536, 2752)),
            'published_max_pixels': None,
            'source_url': 'https://github.com/QwenLM/Qwen-Image-2.1#supported-aspect-ratios',
            'note': 'The publisher recommends native 2K canvases. Listed aspect-ratio examples are recommendations, not a hard maximum; larger output is allowed when the connected workflow supports it.',
        },
    },
    'z-image-turbo': {
        'steps': {
            'recommended': 8,
            'source_label': 'ComfyUI Turbo preset',
            'source_url': Z_TEMPLATE,
            'publisher_source_url': Z_SOURCE,
            'note': 'Use 8 steps with this ComfyUI res_multistep/simple workflow. The publisher Diffusers example specifies 9 but explicitly says this produces 8 DiT forwards. ComfyUI CFG 1 disables classifier-free guidance.',
        },
        'resolution': {
            'recommended_sizes': _sizes((1024, 1024)),
            'published_max_pixels': None,
            'source_url': Z_SOURCE,
            'note': 'The publisher example uses 1024 by 1024. It does not specify a hard maximum output resolution. The connected workflow supplies dimension constraints.',
        },
    },
    'ernie-image': {
        'steps': {
            'recommended': 50,
            'source_label': 'Baidu Base recommendation',
            'source_url': ERNIE_SOURCE,
            'note': 'Baidu recommends 50 steps and guidance 4 for ERNIE-Image Base. The separate Turbo model uses 8 steps and is not this preset.',
        },
        'resolution': {
            'recommended_sizes': _sizes((1024, 1024), (848, 1264), (1264, 848), (768, 1376), (896, 1200), (1376, 768), (1200, 896)),
            'published_max_pixels': None,
            'source_url': ERNIE_SOURCE,
            'note': 'Baidu lists canvases around one megapixel, not a hard maximum. The connected workflow supplies dimension constraints.',
        },
    },
}

for _model, _variant in (('flux2-klein-4b', '4B'), ('flux2-klein-9b', '9B')):
    _source = f'https://huggingface.co/black-forest-labs/FLUX.2-klein-{_variant}#using-with-diffusers-'
    GUIDANCE[_model] = {
        'steps': {
            'recommended': 4,
            'source_label': 'BFL distilled recommendation',
            'source_url': _source,
            'implementation_source_url': KLEIN_SOURCE,
            'note': 'The distilled Klein model uses 4 steps and guidance 1; Local Image fixes this preset to those values. This is not Klein Base, which uses a different sampling preset. Weight precision does not change the recommended step count.',
        },
        'resolution': {
            'recommended_sizes': _sizes((1024, 1024)),
            'published_max_pixels': None,
            'source_url': _source,
            'note': 'The publisher example uses 1024 by 1024. It does not specify a hard maximum for these local weights. The connected workflow supplies dimension constraints.',
        },
    }


def model_guidance(model_id, limits=None):
    """Return independent JSON-safe guidance, or None for an unaudited preset."""
    if model_id not in GUIDANCE:
        return None
    result = {'verified_on': VERIFIED_ON, **deepcopy(GUIDANCE[model_id])}
    if limits is not None:
        resolution = result['resolution']
        # Preserve publisher examples and offer those accepted by the actual
        # connected workflow. Missing upper bounds are unknown, not app caps.
        resolution['publisher_sizes'] = deepcopy(resolution['recommended_sizes'])
        resolution['recommended_sizes'] = [
            size for size in resolution['recommended_sizes']
            if all(limits[axis]['min'] <= size[axis]
                   and (limits[axis]['max'] is None or size[axis] <= limits[axis]['max'])
                   and size[axis] % limits[axis]['step'] == 0 for axis in ('width', 'height'))
            and (limits['max_pixels'] is None or size['width'] * size['height'] <= limits['max_pixels'])
        ]
        resolution['workflow_limits'] = {
            'label': limits['resolution_label'], 'note': limits['resolution_note'],
            **{key: limits[key] for key in ('min_dimension', 'max_dimension', 'dimension_step', 'max_pixels')},
        }
    return result
