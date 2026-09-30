"""Exact output sizes and constraints from the connected ComfyUI workflow.

No application pixel budget or guessed maximum belongs here. Reference image
preprocessing is separate from the requested generation canvas.
"""
from math import lcm


NODES = {'qwen': 'EmptyLatentImage', 'z-image-turbo': 'EmptySD3LatentImage',
         'flux2-dev': 'EmptyFlux2LatentImage', 'flux2-klein-4b': 'EmptyFlux2LatentImage',
         'flux2-klein-9b': 'EmptyFlux2LatentImage', 'ernie-image': 'EmptyFlux2LatentImage',
         'hidream-o1': 'EmptyHiDreamO1LatentImage'}
# Qwen's VAE/model latent format has spatial downscale 16. Its semantic
# reference encoder explicitly rounds reference dimensions to multiples of 32.
QWEN_ALIGNMENT_SOURCE = 'https://github.com/Comfy-Org/ComfyUI/blob/master/comfy/latent_formats.py'
QWEN_REFERENCE_SOURCE = 'https://github.com/Comfy-Org/ComfyUI/blob/master/comfy_extras/nodes_qwen.py'


def _node_axis(info, node, axis):
    fields = info.get(node, {}).get('input', {})
    value = fields.get('required', {}).get(axis, fields.get('optional', {}).get(axis))
    if not isinstance(value, (list, tuple)) or len(value) < 2 or value[0] != 'INT' or not isinstance(value[1], dict):
        return {}
    return {key: number for key in ('min', 'max', 'step')
            if type(number := value[1].get(key)) is int and number > 0}


def resolution_limits(info, model, *, references=False):
    node = NODES[model]
    if model in ('qwen', 'z-image-turbo') and references:
        step = 32 if model == 'qwen' else 16
        axes = {axis: {'min': step, 'max': None, 'step': step} for axis in ('width', 'height')}
        node = 'TextEncodeQwenImage21' if model == 'qwen' else 'VAEEncode'
        note = (f"{'Qwen reference editing' if model == 'qwen' else 'Z-Image variations'} uses a {step}-pixel grid. "
                f'{node} reports no output-size maximum or area cap; the empty-latent node is not used in this workflow.')
        source = QWEN_REFERENCE_SOURCE if model == 'qwen' else 'https://github.com/Comfy-Org/ComfyUI/blob/master/comfy_extras/nodes_sd3.py'
    else:
        axes = {}
        for axis in ('width', 'height'):
            values = _node_axis(info, node, axis)
            # The selected Qwen VAE requires 16 even though the generic latent
            # node advertises 8. Other workflows use their node's own grid.
            step = lcm(values.get('step', 1), 16) if model == 'qwen' else values.get('step', 1)
            minimum = max(values.get('min', 1), 16 if model == 'qwen' else 1)
            maximum = values.get('max')
            if model.startswith('flux2-'):
                scheduler = _node_axis(info, 'Flux2Scheduler', axis)
                minimum = max(minimum, scheduler.get('min', 1))
                maximum = min(maximum, scheduler['max']) if maximum is not None and 'max' in scheduler else scheduler.get('max', maximum)
                step = lcm(step, scheduler.get('step', 1))
            axes[axis] = {'min': minimum, 'max': maximum, 'step': step}
        note = 'Size constraints come from the connected ComfyUI workflow. It reports no total-pixel cap; available memory determines what can run.'
        if model == 'qwen':
            note += ' Qwen output uses its VAE’s 16-pixel grid; reference editing uses 32.'
        source = QWEN_ALIGNMENT_SOURCE if model == 'qwen' else 'ComfyUI /object_info: ' + node
    maximums = [item['max'] for item in axes.values() if item['max'] is not None]
    result = {'min_dimension': max(item['min'] for item in axes.values()),
              'max_dimension': min(maximums) if maximums else None,
              'dimension_step': lcm(*(item['step'] for item in axes.values())),
              'max_pixels': None, **axes, 'resolution_policy': 'comfy-workflow',
              'resolution_label': 'Connected workflow constraints', 'resolution_note': note,
              'resolution_node': node, 'resolution_source': source,
              'resolution_known': bool(info.get(node))}
    if model in ('qwen', 'z-image-turbo') and not references:
        result['reference_dimensions'] = resolution_limits(info, model, references=True)
    return result


def exact_canvas_size(size):
    """Validate structure only; never round, shrink or access the network."""
    if not isinstance(size, (tuple, list)) or len(size) != 2 or any(type(value) is not int or value <= 0 for value in size):
        raise ValueError('Image dimensions must be positive whole numbers.')
    return tuple(size)


def upscale_resolution_limits(info):
    """SeedVR2 pads internally, then crops its result to even dimensions."""
    return {'min_dimension': 2, 'max_dimension': None, 'dimension_step': 2, 'max_pixels': None,
            'width': {'min': 2, 'max': None, 'step': 2}, 'height': {'min': 2, 'max': None, 'step': 2},
            'resolution_policy': 'comfy-workflow', 'resolution_label': 'Connected workflow constraints',
            'resolution_node': 'SeedVR2Preprocess / SeedVR2PostProcessing',
            'resolution_known': 'SeedVR2Preprocess' in info and 'SeedVR2PostProcessing' in info,
            'resolution_source': 'https://github.com/Comfy-Org/ComfyUI/blob/master/comfy_extras/nodes_seedvr.py',
            'resolution_note': 'SeedVR2 requires even dimensions with a shorter edge of at least 2 pixels because its postprocessor crops odd edges. These nodes report no maximum dimensions or area cap. Keep the source aspect ratio.'}


def validate_generation_size(size, model, info, *, references=False):
    size = exact_canvas_size(size)
    limits = resolution_limits(info, model, references=references)
    for axis, value in zip(('width', 'height'), size):
        rule = limits[axis]
        if value < rule['min'] or rule['max'] is not None and value > rule['max'] or value % rule['step']:
            maximum = f"–{rule['max']}" if rule['max'] is not None else ' or more'
            raise ValueError(f"This {limits['resolution_node']} workflow requires {axis} {rule['min']}{maximum} pixels in multiples of {rule['step']}; received {value}.")
    return size
