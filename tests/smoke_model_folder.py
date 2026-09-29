"""Verify real native-authorized folder configuration in an isolated test profile."""
import argparse
import json
from pathlib import Path
import tempfile
import urllib.request

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('profile', type=Path)
    parser.add_argument('--url', default='http://127.0.0.1:51247')
    args = parser.parse_args()
    profile = args.profile.resolve()
    key = (profile / 'state' / 'launcher.key').read_text().strip()
    def request(path, data=None):
        req = urllib.request.Request(args.url + path, data=json.dumps(data).encode() if data is not None else None,
            headers={'Content-Type': 'application/json', 'Origin': args.url, 'x-local-launcher': key})
        with urllib.request.urlopen(req, timeout=60) as response:
            return json.load(response)
    runtime = request('/api/local-remove/runtime')
    assert Path(runtime['data_root']).resolve() == profile, 'Use only the explicitly supplied test profile'
    original = request('/api/local-remove/setup')['model_directory']
    with tempfile.TemporaryDirectory(prefix='empty-test-models-', dir=profile) as temporary:
        try:
            changed = request('/api/local-remove/setup/configure', {'model_directory': temporary})
            assert changed['model_folder_changed']
            expected = 'restart_required' if changed['service']['running'] else 'will_apply_on_start'
            assert changed['model_folder_connection']['status'] == expected
            assert Path(changed['model_directory']).resolve() == Path(temporary).resolve()
            models = request('/api/local-remove/generation/models')
            assert Path(models['model_directory']).resolve() == Path(temporary).resolve()
            assert all(not variant['files_present'] and variant['missing_bytes'] > 0
                       for model in models['models'] for variant in model['variants'])
            download = request('/api/local-remove/generator/download')
            assert Path(download['model_directory']).resolve() == Path(temporary).resolve()
        finally:
            restored = request('/api/local-remove/setup/configure', {'model_directory': original})
            assert Path(restored['model_directory']).resolve() == Path(original).resolve()
    print(json.dumps({'profile': str(profile), 'native_folder_selection': True,
        'download_destination_updated': True, 'separate_disk_and_comfy_readiness': True,
        'reconnection_status': expected, 'original_folder_restored': True}))

if __name__ == '__main__':
    main()
