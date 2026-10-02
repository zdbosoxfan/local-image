#!/usr/bin/env python3
"""Verify a real Linux bundle's per-user install, native launch, and uninstall.

This is a developer/CI tool. Downloaded users do not need Python to install or
run Local Image. Example: python3 smoke_install.py --package dist/linux/<folder>
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time


def port_is_occupied():
    try:
        with socket.create_connection(('127.0.0.1', 51247), timeout=0.5):
            return True
    except ConnectionRefusedError:
        return False
    except OSError as error:
        raise RuntimeError(f'Could not check the native editor port: {error}') from error


def validate_package(package):
    for name in ('local-image', 'backend/LocalRemoveBackend'):
        path = package / name
        if not path.is_file() or not os.access(path, os.X_OK):
            raise RuntimeError(f'The package executable is missing: {name}')
        with path.open('rb') as stream:
            header = stream.read(20)
        if (len(header) != 20 or header[:5] != b'\x7fELF\x02'
                or header[5] != 1 or int.from_bytes(header[18:20], 'little') != 62):
            raise RuntimeError(f'{name} is not a Linux x86_64 executable.')
    for name in ('install.sh', 'uninstall.sh', 'VERSION', 'icon.png', 'local-image.desktop'):
        if not (package / name).is_file():
            raise RuntimeError(f'The package file is missing: {name}')
    count = 0
    for path in package.rglob('*'):
        if path.is_symlink():
            try:
                target = path.resolve(strict=True)
            except (FileNotFoundError, RuntimeError) as error:
                raise RuntimeError(f'The package has a broken symlink: {path.relative_to(package)}') from error
            if not target.is_relative_to(package):
                raise RuntimeError(f'The package symlink escapes its directory: {path.relative_to(package)}')
            count += 1
    return count


def run(command, environment, timeout=100):
    # A fresh process group lets a timeout stop only this smoke test's native
    # window, backend, and Qt children; the user's running app is never killed.
    started = time.monotonic()
    process = subprocess.Popen([str(value) for value in command], env=environment,
                               stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True, start_new_session=True)
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired as error:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            stdout, stderr = process.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            stdout, stderr = process.communicate()
        raise RuntimeError(f'{Path(command[0]).name} timed out.\n{stdout}\n{stderr}') from error
    result = {'returncode': process.returncode, 'stdout': stdout, 'stderr': stderr,
              'elapsed_seconds': round(time.monotonic() - started, 3)}
    if process.returncode:
        raise RuntimeError(f'{Path(command[0]).name} failed ({process.returncode}).\n{stdout}\n{stderr}')
    return result


def smoke_install(package, timeout):
    symlink_count = validate_package(package)
    if port_is_occupied():
        raise RuntimeError('Port 51247 is already in use. Close the existing Local Image app before running this developer smoke test.')
    with tempfile.TemporaryDirectory(prefix='local-image-bundle-install-') as temporary:
        folder = Path(temporary)
        home = folder / 'test user with spaces – café'
        home.mkdir()
        data = home / 'XDG data with spaces'
        environment = os.environ.copy()
        for key in ('LOCAL_IMAGE_DATA_DIR', 'LOCAL_REMOVE_DATA_DIR', 'LOCALAPPDATA'):
            environment.pop(key, None)
        platform = os.environ.get('QT_QPA_PLATFORM', 'offscreen')
        environment.update(HOME=str(home), XDG_DATA_HOME=str(data),
                           XDG_CONFIG_HOME=str(home / 'config'),
                           XDG_CACHE_HOME=str(home / 'cache'),
                           XDG_STATE_HOME=str(home / 'state'),
                           XDG_RUNTIME_DIR=str(home / 'runtime'),
                           QT_QPA_PLATFORM=platform)
        (home / 'runtime').mkdir(mode=0o700)
        # Keep any QTWEBENGINE_DISABLE_SANDBOX supplied by a root build
        # container. This tool does not weaken the sandbox for normal users.
        sentinels = {}
        for name in ('state/settings.json', 'state/generation-library/recovered-image.png',
                     'models/keep-model.bin', 'webview/preferences.json'):
            path = data / 'local-image' / name
            path.parent.mkdir(parents=True, exist_ok=True)
            body = ('preserved user data: ' + name).encode()
            path.write_bytes(body)
            sentinels[name] = hashlib.sha256(body).hexdigest()
        installation = run([package / 'install.sh'], environment)
        app_root = data / 'local-image-app'
        current = app_root / 'current'
        launcher = home / '.local' / 'bin' / 'local-image'
        desktop = data / 'applications' / 'local-image.desktop'
        if not current.is_symlink() or not current.resolve().is_relative_to(app_root / 'releases'):
            raise RuntimeError('The installer did not create an internal current-release symlink.')
        if not launcher.is_file() or not os.access(launcher, os.X_OK) or not desktop.is_file():
            raise RuntimeError('The installed launcher or applications-menu entry is missing.')
        installed_symlinks = validate_package(current.resolve())
        if installed_symlinks != symlink_count:
            raise RuntimeError('Installation did not preserve the bundled runtime symlinks.')
        native = run([launcher, '--smoke-test'], environment, timeout=timeout)
        if ('LOCAL_IMAGE_NATIVE_READY' not in native['stdout']
                or 'LOCAL_IMAGE_PAGE_LOADED True' not in native['stdout']):
            raise RuntimeError('The installed native app did not load its React UI and desktop bridge.\n' + native['stdout'])
        if port_is_occupied():
            raise RuntimeError('The native smoke test left its backend running.')
        uninstall = run([current / 'uninstall.sh'], environment)
        if app_root.exists() or launcher.exists() or desktop.exists():
            raise RuntimeError('Uninstall left application files or desktop integration behind.')
        for name, checksum in sentinels.items():
            path = data / 'local-image' / name
            if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != checksum:
                raise RuntimeError('Uninstall changed user data: ' + name)
        return {'passed': True, 'version': (package / 'VERSION').read_text().strip(),
                'package': str(package), 'runtime_symlink_count': symlink_count,
                'qt_platform': platform,
                'user_data_files_preserved': len(sentinels), 'temporary_profile_removed': True,
                'installation': installation, 'native': native, 'uninstall': uninstall}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', required=True, type=Path, help='Extracted Linux bundle directory')
    parser.add_argument('--timeout', default=100, type=float, help='Native startup timeout in seconds')
    parser.add_argument('--report', type=Path, help='Optional JSON verification report')
    arguments = parser.parse_args()
    if arguments.timeout <= 0:
        parser.error('--timeout must be positive.')
    result = smoke_install(arguments.package.resolve(strict=True), arguments.timeout)
    if arguments.report:
        arguments.report.parent.mkdir(parents=True, exist_ok=True)
        arguments.report.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2), flush=True)


if __name__ == '__main__':
    main()
