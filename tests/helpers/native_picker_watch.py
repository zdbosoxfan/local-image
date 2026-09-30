"""Bounded read-only observer for one QA-owned picker. Never sends UI input.

Run later, after an actual owned picker has been observed and its HWND recorded.
This proves only that the same owned dialog remained present for the interval;
the post-interval OK/Cancel routing must be checked separately in the real app.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
QA = ROOT / 'qa-artifacts'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--window', required=True)
    parser.add_argument('--seconds', type=int, default=610)
    parser.add_argument('--interval', type=int, default=20)
    args = parser.parse_args()
    manifest = args.manifest.resolve()
    if not manifest.is_relative_to(QA) or not 600 < args.seconds <= 900 or not 5 <= args.interval <= 60:
        raise SystemExit('Use a QA manifest, a duration from 601 to 900 seconds, and a bounded polling interval.')
    data = json.loads(manifest.read_text(encoding='utf-8'))
    run = Path(data['runRoot']).resolve()
    if not run.is_relative_to(QA) or not data.get('applicationLaunched'):
        raise SystemExit('An actual QA launch is required.')
    helper = QA / 'native-acceptance/tools/NativeQaWindow.exe'
    target = str(int(args.window))
    records = []; start = time.monotonic(); complete = False
    while True:
        process = subprocess.run([str(helper), 'windows', str(manifest)], cwd=ROOT, capture_output=True, text=True,
                                 creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0, timeout=10)
        elapsed = time.monotonic() - start
        if process.returncode:
            records.append({'elapsedSeconds': round(elapsed, 3), 'verified': False, 'error': process.stderr.strip()})
            break
        state = json.loads(process.stdout)
        selected = [window for window in state['windows'] if window['hwnd'] == target and not window['root'] and window['visible']]
        records.append({'elapsedSeconds': round(elapsed, 3), 'verified': len(selected) == 1,
                        'window': selected[0] if len(selected) == 1 else None})
        (run / 'long-picker-observations.json').write_text(json.dumps(records, indent=2), encoding='utf-8')
        if len(selected) != 1:
            break
        if elapsed >= args.seconds:
            complete = True
            break
        time.sleep(min(args.interval, args.seconds - elapsed))
    result = {'ownedDialogHeldOpen': complete, 'requestedSeconds': args.seconds, 'elapsedSeconds': round(time.monotonic() - start, 3),
              'window': target, 'desktopInputsSent': 0, 'okCancelRoutingVerified': False, 'observations': records}
    (run / 'long-picker-result.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps({key: value for key, value in result.items() if key != 'observations'}, indent=2))
    return 0 if complete else 1


if __name__ == '__main__':
    raise SystemExit(main())
