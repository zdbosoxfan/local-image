"""Packaged cross-PC storage smoke, without AI inference or model downloads.

Copies the package into a unique QA folder, simulates two fresh Windows user
profiles, and restores its optional read-only ACL before finishing. No original
package, real user profile, or external ComfyUI files are changed.
"""
from __future__ import annotations

import argparse
import base64
import csv
import ctypes
from datetime import datetime, timezone
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[1]
BASE = 'http://127.0.0.1:51247'
NO_WINDOW = getattr(subprocess, 'CREATE_NO_WINDOW', 0)


def request(path, data=None, key=None, base=BASE, timeout=10):
    headers = {'Origin': base, 'Content-Type': 'application/json'}
    if key:
        headers['x-local-launcher'] = key
    req = urllib.request.Request(base + path, headers=headers,
        data=json.dumps(data).encode('utf-8') if data is not None else None)
    with urllib.request.urlopen(req, timeout=timeout) as response:
        return json.loads(response.read())


def tree_hashes(root):
    hashes = {}
    for path in sorted(root.rglob('*')):
        if path.is_file():
            digest = hashlib.sha256()
            with path.open('rb') as stream:
                for block in iter(lambda: stream.read(1024 * 1024), b''):
                    digest.update(block)
            hashes[path.relative_to(root).as_posix()] = digest.hexdigest()
    return hashes


def queue_snapshot():
    try:
        return {'available': True, 'queue': request('/queue', base='http://127.0.0.1:8188', timeout=4)}
    except Exception as error:
        return {'available': False, 'error': type(error).__name__}


def fresh_environment(user):
    env = os.environ.copy()
    for name in ('LOCAL_IMAGE_DATA_DIR', 'LOCAL_REMOVE_DATA_DIR', 'LOCAL_IMAGE_MODELS_DIR',
                 'LOCAL_REMOVE_MODELS_DIR', 'LOCAL_IMAGE_AI_DIR', 'LOCAL_IMAGE_COMFY_CANDIDATES',
                 'LOCAL_REMOVE_COMFY_CANDIDATES'):
        env.pop(name, None)
    env.update(LOCALAPPDATA=str(user / 'AppData' / 'Local'),
               APPDATA=str(user / 'AppData' / 'Roaming'), USERPROFILE=str(user))
    return env


def launch(executable, env, output, arguments=('--no-open',), timeout=30):
    before = time.monotonic()
    log_path = output.with_suffix('.host.log')
    # The detached backend inherits the native host's standard handles. A PIPE
    # would wait for that backend's EOF, rather than the host's process exit.
    with log_path.open('wb') as log:
        process = subprocess.run([str(executable), *arguments, '--output', str(output)],
            cwd=executable.parent, env=env, creationflags=NO_WINDOW,
            stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
            timeout=timeout)
    result = json.loads(output.read_text(encoding='utf-8-sig')) if output.exists() else None
    return {'exit_code': process.returncode, 'seconds': round(time.monotonic() - before, 3),
            'result': result, 'stdout': log_path.read_text(encoding='utf-8', errors='replace')[:1000]}


def dacl_bytes(path):
    advapi = ctypes.WinDLL('advapi32', use_last_error=True)
    get = advapi.GetFileSecurityW
    get.argtypes = [ctypes.c_wchar_p, ctypes.c_uint32, ctypes.c_void_p, ctypes.c_uint32, ctypes.POINTER(ctypes.c_uint32)]
    get.restype = ctypes.c_int
    length = ctypes.c_uint32()
    get(str(path), 4, None, 0, ctypes.byref(length))
    if not length.value:
        raise ctypes.WinError(ctypes.get_last_error())
    buffer = ctypes.create_string_buffer(length.value)
    if not get(str(path), 4, buffer, length.value, ctypes.byref(length)):
        raise ctypes.WinError(ctypes.get_last_error())
    return buffer.raw


def restore_dacl(path, value):
    advapi = ctypes.WinDLL('advapi32', use_last_error=True)
    control, revision = ctypes.c_uint16(), ctypes.c_uint32()
    buffer = ctypes.create_string_buffer(value)
    get_control = advapi.GetSecurityDescriptorControl
    get_control.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint16), ctypes.POINTER(ctypes.c_uint32)]
    get_control.restype = ctypes.c_int
    if not get_control(buffer, ctypes.byref(control), ctypes.byref(revision)):
        raise ctypes.WinError(ctypes.get_last_error())
    set_security = advapi.SetFileSecurityW
    set_security.argtypes = [ctypes.c_wchar_p, ctypes.c_uint32, ctypes.c_void_p]
    set_security.restype = ctypes.c_int
    flags = 4 | (0x80000000 if control.value & 0x1000 else 0x20000000)
    if not set_security(str(path), flags, buffer):
        raise ctypes.WinError(ctypes.get_last_error())


def dacl_signature(value):
    """Compare ACEs and inheritance protection, not Windows' recalculated flags."""
    advapi = ctypes.WinDLL('advapi32', use_last_error=True)
    buffer = ctypes.create_string_buffer(value)
    present, defaulted = ctypes.c_int(), ctypes.c_int()
    pointer = ctypes.c_void_p()
    get_dacl = advapi.GetSecurityDescriptorDacl
    get_dacl.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_int),
                        ctypes.POINTER(ctypes.c_void_p), ctypes.POINTER(ctypes.c_int)]
    get_dacl.restype = ctypes.c_int
    if not get_dacl(buffer, ctypes.byref(present), ctypes.byref(pointer), ctypes.byref(defaulted)):
        raise ctypes.WinError(ctypes.get_last_error())
    acl = None
    if pointer.value:
        size = ctypes.c_uint16.from_address(pointer.value + 2).value
        acl = ctypes.string_at(pointer.value, size)
    control = int.from_bytes(value[2:4], 'little')
    # SetFileSecurity recalculates SE_DACL_AUTO_INHERITED. All ACE contents,
    # inherited ACE flags, and SE_DACL_PROTECTED must still match exactly.
    return bool(present.value), bool(control & 0x1000), acl


class ReadOnlyInstall:
    def __init__(self, directory, evidence):
        self.directory, self.evidence = directory, evidence
        self.original_saved = False
        self.applied = False
        self.sid = None
        self.log = []
        self.snapshots = {}

    def command(self, arguments, cwd=None):
        result = subprocess.run(arguments, cwd=cwd, creationflags=NO_WINDOW,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
            encoding='utf-8', errors='replace', timeout=30)
        self.log.append({'command': arguments, 'exit_code': result.returncode, 'output': result.stdout[-3000:]})
        return result.returncode == 0

    def apply(self):
        try:
            identity = subprocess.check_output(['whoami', '/user', '/fo', 'csv', '/nh'],
                creationflags=NO_WINDOW, text=True, encoding='utf-8').strip()
            self.sid = next(csv.reader(io.StringIO(identity)))[1]
            if not self.original_saved:
                self.snapshots = {str(path): dacl_bytes(path) for path in [self.directory, *self.directory.rglob('*')]}
                (self.evidence / 'install-dacl-snapshot.json').write_text(json.dumps({
                    path: base64.b64encode(value).decode('ascii') for path, value in self.snapshots.items()}, indent=2), encoding='utf-8')
                self.original_saved = True
            if self.original_saved:
                self.applied = self.command(['icacls', str(self.directory), '/deny',
                    '*' + self.sid + ':(OI)(CI)(WD,AD,WA,WEA,DE,DC)', '/t', '/c', '/q'])
            probe = self.directory / '.deployment-write-probe'
            try:
                with probe.open('xb') as stream:
                    stream.write(b'write probe')
                probe.unlink()
                blocked = False
            except PermissionError:
                blocked = True
            return {'acl_applied': self.applied, 'write_denied': blocked,
                    'scope': 'Actual current-user ACL denies writes/deletes in the copied install' if blocked else
                             'Install tree hashes verify immutability; ACL write protection was unavailable'}
        except Exception as error:
            return {'acl_applied': self.applied, 'write_denied': False, 'error': str(error),
                    'scope': 'Install tree hashes verify immutability; ACL write protection was unavailable'}

    def restore(self):
        if not self.original_saved:
            return True
        # Remove only this test's deny entries, then restore every original DACL.
        if self.sid:
            self.command(['icacls', str(self.directory), '/remove:d', '*' + self.sid, '/t', '/c', '/q'])
        try:
            for path, value in sorted(self.snapshots.items(), key=lambda item: len(Path(item[0]).parts)):
                restore_dacl(Path(path), value)
            ok = all(dacl_signature(dacl_bytes(Path(path))) == dacl_signature(value)
                     for path, value in self.snapshots.items())
            probe = self.directory / '.deployment-restored-write-probe'
            with probe.open('xb') as stream:
                stream.write(b'original write access restored')
            probe.unlink()
        except Exception as error:
            self.log.append({'restore_error': str(error)})
            ok = False
        self.applied = False
        return ok


def run(package, output, comfy_directory=None, coordinated=False, expected_version='0.7.0'):
    output.mkdir(parents=True, exist_ok=True)
    run_dir = output / ('run-' + uuid.uuid4().hex[:12])
    run_dir.mkdir()
    install = run_dir / 'Program Files Simulated' / 'Local Image'
    report = {'started_utc': datetime.now(timezone.utc).isoformat(), 'package': str(package),
              'run_directory': str(run_dir), 'scope': 'CPU edits and read-only ComfyUI discovery only; no inference, downloads, or external writes.',
              'complete': False, 'passed': False}
    report_path = output / 'results.json'
    report['comfy_queue_before'] = queue_snapshot()
    current_env = None
    executable = None
    owns_backend = False
    heartbeat_stop = threading.Event()
    heartbeat_thread = None
    acl = None
    try:
        try:
            occupied = request('/api/local-remove/runtime', timeout=3)
        except (urllib.error.URLError, TimeoutError, OSError):
            occupied = None
        assert occupied is None, 'Port 51247 is already owned; leave its backend unchanged and retry after it closes.'
        shutil.copytree(package, install)
        executable = install / 'Local Image.exe'
        assert executable.is_file()
        report['source_native_sha256'] = hashlib.sha256((package / 'Local Image.exe').read_bytes()).hexdigest()
        data = run_dir / 'Data Drive é 水'
        models, ai = data / 'Models', data / 'AI Runtime'
        plan = {'schema': 1, 'setup_mode': 'discover', 'model_directory': str(models), 'managed_ai_directory': str(ai)}
        (install / 'installation-defaults.json').write_text(json.dumps(plan, ensure_ascii=False), encoding='utf-8')
        before = tree_hashes(install)
        users = [run_dir / 'User One é', run_dir / 'User Two 水']
        environments = [fresh_environment(user) for user in users]
        profiles = [Path(env['LOCALAPPDATA']) / 'Local Image' for env in environments]
        for env in environments:
            Path(env['APPDATA']).mkdir(parents=True)
            if comfy_directory:
                registry = Path(env['APPDATA']) / 'Comfy Desktop' / 'installations.json'
                registry.parent.mkdir()
                registry.write_text(json.dumps([{'id': 'qa-existing', 'name': 'Existing ComfyUI (read only)',
                    'installPath': str(comfy_directory), 'sourceId': 'standalone', 'status': 'installed'}]), encoding='utf-8')
        acl = ReadOnlyInstall(install, output)
        report['install_protection'] = acl.apply()
        current_env = environments[0]
        report['first_launch'] = launch(executable, current_env, output / 'first-launch.json', timeout=180)
        assert report['first_launch']['exit_code'] == 0, report['first_launch']
        owns_backend = True
        runtime = request('/api/local-remove/runtime')
        report['first_runtime'] = runtime
        assert runtime['version'] == expected_version, runtime
        assert Path(runtime['data_root']).resolve() == profiles[0].resolve(), runtime
        config_path = profiles[0] / 'config.json'
        config = json.loads(config_path.read_text(encoding='utf-8-sig'))
        assert config['model_directory'] == str(models), config
        assert config['managed_ai_directory'] == str(ai), config
        assert config['setup_mode'] == 'discover' and config['installation_defaults_applied'] is True
        assert not (Path(current_env['LOCALAPPDATA']) / 'Local Remove').exists()
        report['seeded_config'] = config
        key = (profiles[0] / 'state' / 'launcher.key').read_text().strip()
        def heartbeat():
            while not heartbeat_stop.is_set():
                try:
                    request('/api/local-remove/heartbeat', {}, key=key, timeout=5)
                except Exception:
                    pass
                heartbeat_stop.wait(10)
        heartbeat_thread = threading.Thread(target=heartbeat, daemon=True)
        heartbeat_thread.start()
        for field in ('model_directory', 'managed_ai_directory'):
            target = install / ('Forbidden ' + field)
            try:
                request('/api/local-remove/setup/configure', {field: str(target)}, key=key)
                raise AssertionError('Packaged installation folder was accepted as mutable AI storage.')
            except urllib.error.HTTPError as error:
                detail = error.read().decode('utf-8', errors='replace')
                assert error.code == 400, detail
                assert not target.exists()
                report.setdefault('install_folder_rejections', []).append({'field': field, 'status': error.code, 'detail': detail})
        setup = request('/api/local-remove/setup/detect')
        report['read_only_setup'] = setup
        assert setup['model_directory'] == str(models)
        assert setup['managed_directory'] == str(ai)
        assert setup['install_directory'] == str(ai / 'LocalImage-ComfyUI')
        assert setup['setup_mode'] == 'discover'
        assert setup['storage']['model_folder']['free_bytes'] > 0
        assert not models.exists() and not ai.exists(), 'Reading setup created storage directories.'
        if comfy_directory:
            assert any(item['startable'] and Path(item['path']).resolve().is_relative_to(comfy_directory.resolve())
                       for item in setup['installations']), 'Existing ComfyUI was not discovered in simulated fresh profile.'
        cpu = subprocess.run([sys.executable, str(ROOT / 'tests' / 'smoke_installed.py'), str(profiles[0])],
            cwd=ROOT, env=current_env, creationflags=NO_WINDOW, stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT, text=True, encoding='utf-8', errors='replace', timeout=180)
        (output / 'cpu-backend.log').write_text(cpu.stdout, encoding='utf-8')
        report['cpu_backend'] = {'exit_code': cpu.returncode, 'output': cpu.stdout}
        assert cpu.returncode == 0, cpu.stdout
        changed = dict(config, model_directory=str(data / 'User Chosen Models'), user_edited_marker='preserve across application updates')
        config_path.write_text(json.dumps(changed, ensure_ascii=False), encoding='utf-8')
        # Re-running the updated host exercises the same bootstrap used on upgrade.
        report['existing_profile_launch'] = launch(executable, current_env, output / 'existing-profile-launch.json')
        assert report['existing_profile_launch']['exit_code'] == 0
        assert json.loads(config_path.read_text(encoding='utf-8-sig')) == changed, 'Installer defaults overwrote an existing profile.'
        report['existing_config_preserved'] = True
        report['native_self_test'] = launch(executable, current_env, output / 'native-self-test.json', ('--self-test',))
        assert report['native_self_test']['exit_code'] == 0
        report['native_webview_probe'] = launch(executable, current_env, output / 'native-webview-probe.json', ('--probe-webview',), timeout=60)
        assert report['native_webview_probe']['exit_code'] == 0, report['native_webview_probe']
        assert (profiles[0] / 'WebView2-Probe').is_dir(), 'WebView data did not stay in the user profile.'
        if coordinated:
            ready = {'profile': str(profiles[0]), 'install_directory': str(install), 'url': BASE,
                     'continue_file': str(run_dir / 'continue-after-ui'), 'heartbeat_active': True}
            (output / 'ui-ready.json').write_text(json.dumps(ready, indent=2, ensure_ascii=False), encoding='utf-8')
            print('READY_FOR_UI ' + str(output / 'ui-ready.json'), flush=True)
            deadline = time.monotonic() + 600
            while not (run_dir / 'continue-after-ui').exists():
                if time.monotonic() >= deadline:
                    raise TimeoutError('UI coordination did not finish within ten minutes.')
                time.sleep(1)
        report['second_profile_conflict'] = launch(executable, environments[1], output / 'second-profile-conflict.json')
        assert report['second_profile_conflict']['exit_code'] != 0
        assert report['second_profile_conflict']['seconds'] < 15
        assert not (profiles[1] / 'state' / 'launcher.key').exists()
        report['wrong_profile_shutdown'] = launch(executable, environments[1], output / 'wrong-profile-shutdown.json', ('--shutdown-backend',))
        assert report['wrong_profile_shutdown']['exit_code'] != 0
        assert request('/api/local-remove/runtime')['data_root'] == runtime['data_root'], 'Wrong-profile shutdown stopped another user backend.'
        report['install_tree_unchanged'] = tree_hashes(install) == before
        assert report['install_tree_unchanged'], 'Packaged application wrote into its installation directory.'
        heartbeat_stop.set()
        heartbeat_thread.join(timeout=6)
        report['matching_shutdown'] = launch(executable, current_env, output / 'matching-shutdown.json', ('--shutdown-backend',))
        assert report['matching_shutdown']['exit_code'] == 0, report['matching_shutdown']
        owns_backend = False
        current_env = environments[1]
        report['second_profile_launch'] = launch(executable, current_env, output / 'second-profile-launch.json', timeout=180)
        assert report['second_profile_launch']['exit_code'] == 0, report['second_profile_launch']
        owns_backend = True
        second_runtime = request('/api/local-remove/runtime')
        assert Path(second_runtime['data_root']).resolve() == profiles[1].resolve()
        second_config = json.loads((profiles[1] / 'config.json').read_text(encoding='utf-8-sig'))
        assert 'user_edited_marker' not in second_config
        assert second_config['model_directory'] == str(models)
        assert json.loads(config_path.read_text(encoding='utf-8-sig')) == changed
        report['two_profiles_isolated'] = True
        report['second_matching_shutdown'] = launch(executable, current_env, output / 'second-matching-shutdown.json', ('--shutdown-backend',))
        assert report['second_matching_shutdown']['exit_code'] == 0
        owns_backend = False
        assert tree_hashes(install) == before
        report['comfy_queue_after'] = queue_snapshot()
        if report['comfy_queue_before']['available'] and report['comfy_queue_after']['available']:
            report['external_comfy_queue_unchanged'] = report['comfy_queue_before']['queue'] == report['comfy_queue_after']['queue']
            assert report['external_comfy_queue_unchanged'], 'External ComfyUI queue changed during the CPU-only smoke.'
        report['passed'] = True
    except Exception as error:
        report['error'] = str(error)
        raise
    finally:
        heartbeat_stop.set()
        if heartbeat_thread:
            heartbeat_thread.join(timeout=6)
        if owns_backend and executable and current_env:
            try:
                report['cleanup_shutdown'] = launch(executable, current_env, output / 'cleanup-shutdown.json', ('--shutdown-backend',))
            except Exception as error:
                report['cleanup_error'] = str(error)
        if acl:
            report['acl_restored'] = acl.restore()
            report['acl_commands'] = acl.log
            if not report['acl_restored']:
                report['passed'] = False
        report.update(complete=True, finished_utc=datetime.now(timezone.utc).isoformat())
        report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding='utf-8')
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', type=Path, default=ROOT / 'dist/local-image-v07/package')
    parser.add_argument('--output', type=Path, default=ROOT / 'qa-artifacts/v07/deployment')
    parser.add_argument('--expected-version', default='0.7.0')
    parser.add_argument('--comfy-directory', type=Path)
    parser.add_argument('--coordinate-ui', action='store_true')
    args = parser.parse_args()
    result = run(args.package.resolve(), args.output.resolve(), args.comfy_directory, args.coordinate_ui, args.expected_version)
    print(json.dumps({key: result.get(key) for key in ('passed', 'install_tree_unchanged', 'two_profiles_isolated',
        'existing_config_preserved', 'external_comfy_queue_unchanged', 'acl_restored')}), flush=True)
    raise SystemExit(0 if result['passed'] else 1)
