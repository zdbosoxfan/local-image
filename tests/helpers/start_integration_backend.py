"""Start the real source backend in the project's isolated integration profile.

No installer, native host, ComfyUI process or model job is started. Run with the
project .venv interpreter. The returned PID belongs only to this test service.
"""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
import re
import argparse

root = Path(__file__).resolve().parents[2]
output = root / 'qa-artifacts' / 'integration'
profile = output / 'react-profile'
port = 51276
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--mode', choices=('react', 'legacy'), default='react')
options = parser.parse_args()
with socket.socket() as probe:
    probe.settimeout(2)
    if probe.connect_ex(('127.0.0.1', port)) == 0:
        raise SystemExit(f'Port {port} is already in use; inspect its owner before starting a test profile.')
profile.mkdir(parents=True, exist_ok=True)
config = profile / 'config.json'
if not config.exists():
    config.write_text(json.dumps({'comfy_port': 51999, 'setup_mode': 'later',
        'model_directory': str(profile / 'models'), 'managed_ai_directory': str(profile / 'ai')}, indent=2), encoding='utf-8')
# Deduplicate Windows Path/PATH from the invoking runner without changing the
# user's environment. PowerShell 5 Start-Process rejects that inherited pair.
env = {key.upper(): value for key, value in os.environ.items()}
env.update(LOCAL_REMOVE_DATA_DIR=str(profile), LOCAL_IMAGE_DATA_DIR=str(profile),
           LOCAL_IMAGE_FRONTEND=options.mode, COMFY_HOST='127.0.0.1', COMFY_PORT='51999',
           PORT=str(port), PYTHONUNBUFFERED='1')
stdout = (output / 'backend-stdout.log').open('w', encoding='utf-8')
stderr = (output / 'backend-stderr.log').open('w', encoding='utf-8')
process = subprocess.Popen([sys.executable, '-m', 'uvicorn', 'main:app', '--host', '127.0.0.1', '--port', str(port)],
    cwd=root / 'backend', env=env, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr,
    creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0)
stdout.close()
stderr.close()
ready = False
for _ in range(30):
    if process.poll() is not None:
        raise SystemExit(f'Backend exited with {process.returncode}; inspect {output / "backend-stderr.log"}.')
    try:
        with urllib.request.urlopen(f'http://127.0.0.1:{port}/api/local-remove/runtime', timeout=1) as response:
            runtime = json.load(response)
        if runtime.get('application') != 'local-remove' or Path(runtime['data_root']).resolve() != profile.resolve():
            raise SystemExit('Unexpected backend identity/profile; no editing was attempted.')
        ready = True
        break
    except (OSError, urllib.error.URLError):
        time.sleep(.5)
log = (output / 'backend-stdout.log').read_text(encoding='utf-8', errors='replace')
server_pid = re.search(r'Started server process \[(\d+)\]', log)
info = {'pid': process.pid, 'server_pid': int(server_pid[1]) if server_pid else None,
        'ready': ready, 'url': f'http://127.0.0.1:{port}/remove', 'profile': str(profile),
        'python': sys.executable, 'frontend': options.mode, 'comfyTarget': '127.0.0.1:51999'}
(output / 'server.json').write_text(json.dumps(info, indent=2), encoding='utf-8')
print(json.dumps(info, indent=2))
if not ready:
    raise SystemExit('Startup was not verified in time; inspect the recorded PID and logs before retrying.')
