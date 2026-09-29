"""Exercise live Hub metadata and optionally download one reviewed LoRA.

This is a development probe, not an app startup task. It never enables a LoRA.
"""
import argparse
import asyncio
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
import lora_library as library


async def main(args):
    report = {'models': {}}
    for model in library.BASES:
        try:
            result = await library.search_hub(model)
        except ValueError:
            # Surface the original transport failure in this developer probe.
            try:
                await library.hub_json('/api/models', {'search': 'Qwen-Image-2.1', 'limit': '5'})
            except ValueError as error:
                print('Transport cause:', repr(error.__cause__), flush=True)
            raise
        report['models'][model] = {'checked_at': result['checked_at'], 'count': len(result['results']),
                                    'first_results': result['results'][:3], 'warning': result['warning']}
    if args.download_curated:
        item = next(item for item in library.CURATED if item['model'] == args.download_curated)
        files = await library.repository_files(item['model'], item['repo_id'], item['revision'])
        report['selected_files'] = files
        payload = library.LoraDownloadRequest(**{key: item[key] for key in ('model', 'repo_id', 'filename', 'revision')})
        library.manager.begin(payload, lambda: None)
        await library.manager.task
        report['download'] = library.manager.status()
        if report['download']['phase'] != 'complete':
            raise RuntimeError(report['download']['error'])
        report['installed'] = library.installed(item['model'])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2), encoding='utf-8')
    print(json.dumps({'output': str(args.output), 'search_counts': {key: value['count'] for key, value in report['models'].items()},
                      'download': report.get('download')}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--download-curated', choices=['z-image-turbo', 'flux2-klein-4b', 'flux2-dev'])
    parser.add_argument('--output', type=Path, default=Path('qa-artifacts/loras/live-probe.json'))
    asyncio.run(main(parser.parse_args()))
