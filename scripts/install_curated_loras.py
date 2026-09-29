"""Install the pinned current style catalog; no GPU work or remote code.

The command requires explicit profile directories and shares each model file.
Existing unrelated registry entries and installed files are preserved.
"""
import argparse
import asyncio
import hashlib
import json
import os
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE / 'backend'))

EXAMPLES = {
    "Children's drawings": 'A friendly yellow submarine sailing through a blue sea with colorful fish, playful hand-drawn illustration',
    'Photographic realism': 'Realism, a ceramic red teapot on a pale oak table beside a window, natural morning light, detailed glaze and wood grain, editorial photograph',
    'Natural exposure': 'Transform the image with balanced neutral exposure. Preserve the scene, objects and composition.',
    'Anime character consistency': 'Keep the same anime character and outfit. Change the expression to a gentle smile, preserving the pose and background.',
    'Faceted card illustration': 'sts2 card art, warrior card. A chipped steel shield with brass rivets, red rim light tracing the edges, flying stone chips, dark angular shapes behind it.',
    'Pixel art sprites': 'pixel art sprite, a friendly red panda adventurer wearing a green scarf and carrying a tiny backpack, 16-bit game asset',
    'Watercolor wash': 'wtrclr style, a red fox beside a small woodland cottage surrounded by wildflowers, morning light',
    'Claymation miniature': 'claymtn style, a smiling tiny robot tending a rooftop garden with sunflowers and a watering can',
    'Orange splatter illustration': 'dvr_osi_style, a red fox leaping through autumn leaves, bold expressive illustration',
    'Teal dark illustration': 'dvr_tldr_style, a lighthouse on a rocky island in a storm, black background, dramatic illustration',
    'Blueprint wireframe': 'dvr_wf_style, an elegant espresso machine with two cups, precise-looking technical line illustration',
}


def identity(item):
    return hashlib.sha256('|'.join(item[key] for key in ('model', 'repo_id', 'filename', 'revision')).encode()).hexdigest()[:24]


async def install(args):
    profiles = [path.expanduser().resolve() for path in args.profile]
    os.environ['LOCAL_REMOVE_DATA_DIR'] = str(profiles[0])
    import lora_library as library
    current = [item for item in library.CURATED if item['model'] in ('qwen', 'z-image-turbo', 'flux2-klein-4b', 'flux2-klein-9b')]
    # Prove all destinations share the same configured model folder before writing.
    import app_paths
    roots = []
    for profile in profiles:
        os.environ['LOCAL_REMOVE_DATA_DIR'] = str(profile)
        roots.append(app_paths.model_directory().resolve())
        registry = library.read_registry()
        for item in current:
            old = registry.get(identity(item))
            if old is not None and (not isinstance(old, dict) or any(old.get(key) != item[key] for key in ('model', 'repo_id', 'filename', 'revision', 'sha256', 'bytes'))):
                raise ValueError('An existing registry identity has different metadata; it has not been overwritten: ' + str(profile))
    if len(set(roots)) != 1:
        raise ValueError('All supplied profiles must already use the same model folder.')
    os.environ['LOCAL_REMOVE_DATA_DIR'] = str(profiles[0])
    report = {'profiles': list(map(str, profiles)), 'models_directory': str(roots[0]), 'gpu_jobs': 0, 'items': []}
    for item in current:
        print('Verifying/installing ' + item['model'] + ': ' + item['title'], flush=True)
        payload = library.LoraDownloadRequest(**{key: item[key] for key in ('model', 'repo_id', 'filename', 'revision')})
        manager = library.LoraDownloadManager()
        manager.begin(payload, lambda: None)
        await manager.task
        if manager.status()['phase'] != 'complete':
            raise RuntimeError(manager.status()['error'])
        saved = next(value for value in library.installed(item['model']) if value['id'] == identity(item))
        # Validate the actual downloaded tensor header without loading any tensors.
        library.validate_lora_header(library.installed_path(saved))
        for profile in profiles[1:]:
            os.environ['LOCAL_REMOVE_DATA_DIR'] = str(profile)
            registry = library.read_registry()
            registry[saved['id']] = {**registry.get(saved['id'], {}), **saved}
            library.write_registry(registry)
        os.environ['LOCAL_REMOVE_DATA_DIR'] = str(profiles[0])
        report['items'].append({**saved, 'sha256_verified': True, 'header_validated': True,
                                'example_prompt': EXAMPLES[item['title']],
                                'reference_required': item['usage'] == 'reference-edit',
                                'negative_prompt': 'flat, plain, simple, blurry, smooth gradients, empty background, low detail, deformed, extra limbs' if item['title'] == 'Faceted card illustration' else ''})
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2), encoding='utf-8')
        print('Verified ' + saved['id'] + ' ' + str(saved['bytes']) + ' bytes', flush=True)
    print(json.dumps({'installed': len(report['items']), 'bytes': sum(item['bytes'] for item in report['items']), 'manifest': str(args.output)}), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', action='append', required=True, type=Path)
    parser.add_argument('--output', type=Path, default=HERE / 'qa-artifacts' / 'loras' / 'curation' / 'installed-manifest.json')
    asyncio.run(install(parser.parse_args()))
