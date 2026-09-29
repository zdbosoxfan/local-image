"""Offline model comparison from pinned download and hardware catalogs."""
from pathlib import Path

from app_paths import model_directory
from hardware_guide import PROFILES, NOTE
from qwen_download_catalog import QWEN_FILES, LICENSE_URL as QWEN_LICENSE_URL, LICENSE_NOTE as QWEN_LICENSE_NOTE
from hidream_download_catalog import HIDREAM_FILES, LICENSE_URL as HIDREAM_LICENSE_URL, LICENSE_NOTE as HIDREAM_LICENSE_NOTE
from ernie_download_catalog import ERNIE_FILES, LICENSE_URL as ERNIE_LICENSE_URL, LICENSE_NOTE as ERNIE_LICENSE_NOTE
from generation_download_catalog import (Z_IMAGE_FILES, FLUX_DEV_FILES, FLUX_KLEIN_FILES, FLUX_KLEIN_9B_FILES,
    LICENSE_URL as Z_LICENSE_URL, LICENSE_NOTE as Z_LICENSE_NOTE,
    FLUX_DEV_LICENSE_URL, FLUX_DEV_LICENSE_NOTE, KLEIN_LICENSE_URL, KLEIN_LICENSE_NOTE,
    KLEIN_9B_LICENSE_URL, KLEIN_9B_LICENSE_NOTE)

CATALOGS = {'qwen': QWEN_FILES, 'z-image-turbo': Z_IMAGE_FILES, 'flux2-dev': FLUX_DEV_FILES, 'flux2-klein-4b': FLUX_KLEIN_FILES, 'flux2-klein-9b': FLUX_KLEIN_9B_FILES, 'hidream-o1': HIDREAM_FILES, 'ernie-image': ERNIE_FILES}
DETAILS = {
    'ernie-image': {
        'description': 'Baidu’s Base model for text-heavy posters, graphic layouts and dense lettering.',
        'strengths': ['Poster typography and structured text layouts', 'Detailed prompts with English and Chinese text', 'Apache 2.0 model license'],
        'limitations': ['Proofread every letter; no model guarantees perfect text', 'Text-to-image only in this preset; opaque output', '50-step quality preset is slower than distilled models'],
        'license': {'label': 'Apache 2.0', 'url': ERNIE_LICENSE_URL, 'note': ERNIE_LICENSE_NOTE}},
    'hidream-o1': {
        'description': 'A unified pixel-space model for detailed layouts, text rendering and instruction-based image edits.',
        'strengths': ['Text rendering and detailed layouts', '4-megapixel generation and multiple references', 'MIT model license; self-contained checkpoint'],
        'limitations': ['50-step quality preset is slower than distilled models', 'Quality can regress below its trained 4-megapixel resolution', 'Opaque output; complex edits still require review'],
        'license': {'label': 'MIT', 'url': HIDREAM_LICENSE_URL, 'note': HIDREAM_LICENSE_NOTE}},
    'qwen': {
        'description': 'One model for new images, instruction-guided edits, transparent assets and background removal.',
        'strengths': ['Text-to-image and multiple-reference edits', 'Real transparent RGBA output', 'Compact and full-precision choices'],
        'limitations': ['Fine hair or transparent edges can need manual refinement', 'Negative prompts have no effect at guidance 1', 'Research license restricts commercial model use'],
        'license': {'label': 'Qwen Research License', 'url': QWEN_LICENSE_URL, 'note': QWEN_LICENSE_NOTE}},
    'z-image-turbo': {
        'description': 'Fast text-to-image generation and variations from a single starting image.',
        'strengths': ['Quick drafts and style exploration', 'Adjustable strength for starting-image variations', 'Apache 2.0 model license'],
        'limitations': ['Opaque output', 'Starting-image variation does not use semantic reference instructions', 'One starting image; fixed guidance 1'],
        'license': {'label': 'Apache 2.0', 'url': Z_LICENSE_URL, 'note': Z_LICENSE_NOTE}},
    'flux2-klein-4b': {
        'description': 'Distilled four-step generation and reference-guided editing for quick iterations.',
        'strengths': ['Fast drafts with four-step defaults', 'Multiple semantic image references', 'Apache 2.0 model license'],
        'limitations': ['Opaque output', 'Reference edits can change composition or fine details', 'Fixed guidance 1; no variation-strength control'],
        'license': {'label': 'Apache 2.0', 'url': KLEIN_LICENSE_URL, 'note': KLEIN_LICENSE_NOTE}},
    'flux2-klein-9b': {
        'description': 'A larger distilled Klein model for richer detail and reference-guided refinement in four steps.',
        'strengths': ['More model capacity than Klein 4B', 'Four-step generation and multiple semantic references', 'A smaller memory footprint than FLUX.2 Dev'],
        'limitations': ['Official download requires Hugging Face login and publisher license acceptance', 'Fixed guidance 1; opaque output; reference edits can change details', 'Noncommercial model license; system RAM offloading may be needed'],
        'license': {'label': 'FLUX Non-Commercial License', 'url': KLEIN_9B_LICENSE_URL, 'note': KLEIN_9B_LICENSE_NOTE}},
    'flux2-dev': {
        'description': 'A larger FLUX.2 model for detailed generation and reference-guided refinement.',
        'strengths': ['Detailed image generation', 'Multiple semantic image references', 'Adjustable distilled guidance'],
        'limitations': ['Large model download and higher memory needs', 'Opaque output; no variation-strength control', 'Noncommercial model license; reference edits can change composition'],
        'license': {'label': 'FLUX Non-Commercial License', 'url': FLUX_DEV_LICENSE_URL, 'note': FLUX_DEV_LICENSE_NOTE}},
}


def matching_file(root, artifact):
    path = root / artifact['folder'] / artifact['name']
    try:
        sizes = {item['bytes'] for item in (artifact, *artifact.get('compatible_existing', ()))}
        return (path.is_file() and not path.is_symlink() and path.resolve().is_relative_to(root.resolve())
                and path.stat().st_size in sizes)
    except OSError:
        return False


def enrich_model(model, root=None):
    """Add comparison facts without downloads, model loads, or directory creation."""
    root = Path(root) if root is not None else model_directory()
    details = DETAILS[model['id']]
    model.update(description=details['description'], strengths=list(details['strengths']),
                 limitations=list(details['limitations']), license=dict(details['license']))
    profiles = {item['id']: item for item in PROFILES}
    for variant in model['variants']:
        files = CATALOGS[model['id']][variant['id']]
        variant['total_bytes'] = sum(item['bytes'] for item in files)
        variant['missing_bytes'] = sum(item['bytes'] for item in files if not matching_file(root, item))
        variant['files_present'] = variant['missing_bytes'] == 0
        if model['id'] == 'flux2-klein-9b' and not variant['files_present']:
            variant['downloadable'] = False
            variant['download_note'] = 'Publisher access required. Accept the license on Hugging Face, download the exact FP8 files into the selected model folder, then refresh.'
        profile = profiles['qwen-' + variant['id'] if model['id'] == 'qwen' else model['id']]
        variant['hardware'] = {'vram_recommendation': profile['vram'], 'basis': profile['basis'],
                               'source_url': profile['source_url'], 'note': NOTE}
    default = next(variant for variant in model['variants'] if variant['id'] == model['defaults']['variant'])
    model['storage_bytes'] = default['total_bytes']
    model['hardware'] = dict(default['hardware'])
    model['recommended'] = {key: model['defaults'][key] for key in ('steps', 'guidance', 'width', 'height')}
    model['storage_note'] = 'Full preset size includes its image model, text encoder and VAE. Shared files can reduce the download. Disk size is separate from GPU memory.'
    model['availability_note'] = 'Files in the selected download folder and models visible to running ComfyUI are checked separately. Downloading verifies existing file contents.'
    return model
