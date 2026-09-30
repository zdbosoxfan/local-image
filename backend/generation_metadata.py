"""Portable, bounded generation provenance; never contains filesystem authority."""
import math
import re

KEYS = {'model', 'variant', 'prompt', 'negative_prompt', 'width', 'height', 'seed',
        'transparent', 'steps', 'guidance', 'denoise', 'reference_count'}
VARIANTS = {'qwen': ('int8', 'bf16'), 'z-image-turbo': ('bf16',),
            'flux2-dev': ('fp8',), 'flux2-klein-4b': ('bf16',), 'flux2-klein-9b': ('fp8',), 'hidream-o1': ('fp8',), 'ernie-image': ('bf16',)}


def validate_lora_metadata(value):
    if not isinstance(value, list) or len(value) > 3:
        raise ValueError('Choose at most three LoRAs.')
    seen = set()
    for item in value:
        if (not isinstance(item, dict) or set(item) != {'id', 'strength'}
                or not isinstance(item['id'], str) or not re.fullmatch(r'[a-f0-9]{24}', item['id'])
                or item['id'] in seen or type(item['strength']) not in (int, float)
                or not math.isfinite(item['strength']) or not -2 <= item['strength'] <= 2):
            raise ValueError('Image generation LoRA settings are invalid.')
        seen.add(item['id'])
    return value


def validate_generation_metadata(value):
    if not isinstance(value, dict) or set(value) not in (KEYS, KEYS | {'loras'}):
        raise ValueError('Image generation settings are invalid.')
    integer = lambda x, low, high: type(x) is int and low <= x <= high
    number = lambda x, low, high: type(x) in (int, float) and math.isfinite(x) and low <= x <= high
    if (not isinstance(value['model'], str) or not isinstance(value['variant'], str)
            or value['model'] not in VARIANTS or value['variant'] not in VARIANTS[value['model']]):
        raise ValueError('Image generation model settings are invalid.')
    if not isinstance(value['prompt'], str) or not 1 <= len(value['prompt']) <= 4000 or not isinstance(value['negative_prompt'], str) or len(value['negative_prompt']) > 2000:
        raise ValueError('Image generation prompts are invalid.')
    # Provenance records the image that actually exists; it must not apply a
    # current workflow's constraints to older documents or require a connection.
    if (not all(integer(value[key], 1, 2**53-1) for key in ('width', 'height'))
            or not integer(value['seed'], 0, 2**53-1)
            or type(value['transparent']) is not bool or not integer(value['steps'], 1, 100)
            or not number(value['guidance'], 1, 10) or not integer(value['reference_count'], 0, 10)):
        raise ValueError('Image generation dimensions or sampling settings are invalid.')
    if value['model'] == 'z-image-turbo':
        if (value['variant'] != 'bf16' or value['transparent'] or value['negative_prompt'] or value['guidance'] != 1
                or value['steps'] > 50 or value['reference_count'] > 1
                or not number(value['denoise'], 0.05, 1)
                or value['reference_count'] == 0 and value['denoise'] != 1):
            raise ValueError('Z-Image Turbo generation settings are invalid.')
    else:
        if value['denoise'] is not None:
            raise ValueError('This model uses semantic references, not variation strength.')
        if value['model'] == 'hidream-o1' and value['transparent']:
            raise ValueError('HiDream O1 produces opaque output.')
        if value['model'] == 'ernie-image' and (value['transparent'] or value['reference_count'] or value.get('loras')):
            raise ValueError('ERNIE-Image Base supports opaque text-to-image generation without references or adapters in this preset.')
        if value['model'].startswith('flux2-') and (value['transparent'] or value['negative_prompt'] or value['reference_count'] > 4):
            raise ValueError('FLUX.2 uses up to four references and produces opaque output without negative prompts.')
        if value['model'].startswith('flux2-klein-') and (value['guidance'] != 1 or value['steps'] > 50):
            raise ValueError('Distilled FLUX.2 Klein uses guidance 1 and between 1 and 50 steps.')
    if 'loras' in value:
        validate_lora_metadata(value['loras'])
    return value
