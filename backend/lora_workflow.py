"""Model-only LoRA graph wiring after library compatibility resolution."""
import math


def available_loras(info, selections):
    """Resolve registered filenames against live ComfyUI loader choices."""
    if not selections:
        return []
    if len(selections) > 3:
        raise ValueError('Choose at most three LoRAs.')
    node = info.get('LoraLoaderModelOnly')
    if not node:
        raise ValueError('Update ComfyUI: optional LoRAs need LoraLoaderModelOnly.')
    from qwen_image import _choices
    available = _choices(info, 'LoraLoaderModelOnly', 'lora_name')
    result = []
    for name, strength in selections:
        if (not isinstance(name, str) or not name or ':' in name or name.startswith(('/', '\\'))
                or '..' in name.replace('\\', '/').split('/') or not name.lower().endswith('.safetensors')
                or type(strength) not in (int, float) or not math.isfinite(strength) or not -2 <= strength <= 2):
            raise ValueError('The selected LoRA settings are invalid.')
        resolved = next((choice for choice in available if isinstance(choice, str)
                         and choice.replace('\\', '/') == name.replace('\\', '/')), None)
        if resolved is None:
            raise ValueError('The selected LoRA is not available in ComfyUI. Refresh the library or finish its download.')
        result.append((resolved, float(strength)))
    return result


def add_lora_chain(graph, model_link, selections):
    """Return patched model link without changing text encoders or VAE."""
    for index, (name, strength) in enumerate(selections):
        node_id = str(100 + index)
        graph[node_id] = {'class_type': 'LoraLoaderModelOnly', 'inputs': {
            'model': model_link, 'lora_name': name, 'strength_model': strength}}
        model_link = [node_id, 0]
    return model_link
