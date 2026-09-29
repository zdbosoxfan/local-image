"""Opt-in per-user EXE install/uninstall acceptance in an isolated QA folder.

Never run if a real Local Remove/Image registration exists. Existing legacy
shortcuts are inspected and must remain byte-for-byte unchanged.
The app is not launched; models and ComfyUI are not downloaded. Only the newly
installed QA application's own uninstaller removes files/registrations.
"""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import uuid
import winreg

UNINSTALL_KEY = r'Software\Microsoft\Windows\CurrentVersion\Uninstall\LocalRemove.Windows_is1'
CLASS_KEY = r'Software\Classes\LocalRemove.Project'
OPENWITH_KEY = r'Software\Classes\.lremove\OpenWithProgids'


def windows_username():
    buffer = ctypes.create_unicode_buffer(257)
    size = ctypes.c_uint(len(buffer))
    if not ctypes.windll.advapi32.GetUserNameW(buffer, ctypes.byref(size)):
        raise RuntimeError('Could not identify the Windows account for installer acceptance.')
    return buffer.value


def values(hive, key, view):
    try:
        with winreg.OpenKey(hive, key, 0, winreg.KEY_READ | view) as opened:
            count = winreg.QueryInfoKey(opened)[1]
            return {winreg.EnumValue(opened, index)[0]: winreg.EnumValue(opened, index)[1]
                    for index in range(count)}
    except FileNotFoundError:
        return None


def registrations():
    result = []
    for name, hive in (('HKCU', winreg.HKEY_CURRENT_USER), ('HKLM', winreg.HKEY_LOCAL_MACHINE)):
        for bits, view in ((64, winreg.KEY_WOW64_64KEY), (32, winreg.KEY_WOW64_32KEY)):
            for kind, key in (('uninstall', UNINSTALL_KEY), ('project_class', CLASS_KEY),
                              ('open_with', OPENWITH_KEY)):
                entries = values(hive, key, view)
                if entries is not None:
                    result.append({'hive': name, 'bits': bits, 'kind': kind,
                                   'key': key, 'values': entries})
    return result


def shell_folder(csidl):
    buffer = ctypes.create_unicode_buffer(32768)
    status = ctypes.windll.shell32.SHGetFolderPathW(None, csidl, None, 0, buffer)
    if status != 0:
        raise RuntimeError('Could not resolve a Windows shortcut folder.')
    return Path(buffer.value)


def legacy_shortcuts():
    paths = []
    for csidl in (0x10, 0x19):  # User and common Desktop.
        paths.append(shell_folder(csidl) / 'Local Remove.lnk')
    for csidl in (0x02, 0x17):  # User and common Programs.
        folder = shell_folder(csidl) / 'Local Remove'
        paths.extend((folder / 'Local Remove.lnk', folder / 'AI connection settings.lnk'))
    result = []
    for path in paths:
        if not path.exists():
            continue
        literal = str(path).replace("'", "''")
        command = ("[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); "
                   "$link = (New-Object -ComObject WScript.Shell).CreateShortcut('" + literal + "'); "
                   "$link.TargetPath")
        startup = subprocess.STARTUPINFO()
        startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
        startup.wShowWindow = subprocess.SW_HIDE
        inspected = subprocess.run(['powershell.exe', '-NoProfile', '-NonInteractive', '-Command', command],
                                   startupinfo=startup, creationflags=subprocess.CREATE_NO_WINDOW,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=15)
        if inspected.returncode:
            raise RuntimeError('Could not inspect a legacy shortcut target; no installation allowed.')
        result.append({'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
                       'target': inspected.stdout.decode('utf-8-sig').strip()})
    return result


def preflight():
    registered = registrations()
    # Empty OpenWithProgids keys can remain after an earlier uninstall. Never
    # alter any existing association value or registered project class.
    conflicts = [item for item in registered if item['kind'] != 'open_with' or item['values']]
    shortcuts = legacy_shortcuts()
    return {'registrations': registered, 'conflicts': conflicts, 'legacy_shortcuts': shortcuts,
            'safe': not conflicts}


def hidden_run(arguments, timeout=150, environment=None):
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = subprocess.SW_HIDE
    started = time.monotonic()
    process = subprocess.run([str(value) for value in arguments], startupinfo=startup,
                             creationflags=subprocess.CREATE_NO_WINDOW,
                             stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE, timeout=timeout, env=environment)
    return {'arguments': [str(value) for value in arguments], 'exit_code': process.returncode,
            'elapsed_seconds': round(time.monotonic() - started, 3)}


def file_version(path):
    class FixedInfo(ctypes.Structure):
        _fields_ = [(field, ctypes.c_uint32) for field in
                    ('signature', 'structure_version', 'file_ms', 'file_ls', 'product_ms',
                     'product_ls', 'flags_mask', 'flags', 'os', 'file_type', 'subtype',
                     'date_ms', 'date_ls')]
    version = ctypes.windll.version
    size = version.GetFileVersionInfoSizeW(str(path), None)
    if not size:
        raise RuntimeError('Installed native executable has no version information.')
    buffer = ctypes.create_string_buffer(size)
    if not version.GetFileVersionInfoW(str(path), 0, size, buffer):
        raise RuntimeError('Could not read native executable version.')
    pointer = ctypes.c_void_p()
    length = ctypes.c_uint()
    if not version.VerQueryValueW(buffer, '\\', ctypes.byref(pointer), ctypes.byref(length)):
        raise RuntimeError('Could not query native executable version.')
    fixed = ctypes.cast(pointer, ctypes.POINTER(FixedInfo)).contents
    return '.'.join(str(value) for value in
                    (fixed.file_ms >> 16, fixed.file_ms & 65535,
                     fixed.file_ls >> 16, fixed.file_ls & 65535))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('installer', type=Path)
    parser.add_argument('--output', type=Path,
                        default=Path('qa-artifacts/v06/i'))
    parser.add_argument('--check-only', action='store_true')
    args = parser.parse_args()
    if os.name != 'nt':
        parser.error('This test requires Windows.')
    workspace = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    if not output.is_relative_to(workspace / 'qa-artifacts'):
        parser.error('Use an output folder inside this workspace qa-artifacts directory.')
    installer = args.installer.resolve(strict=True)
    output.mkdir(parents=True, exist_ok=True)
    report = {'installer': str(installer), 'sha256': hashlib.sha256(installer.read_bytes()).hexdigest(),
              'windows_account': windows_username(),
              'scope': 'current-user', 'app_launched': False, 'model_downloads': False,
              'preflight': preflight(), 'status': 'pending', 'passed': False}
    result_file = output / 'results.json'

    def save():
        result_file.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding='utf-8')

    save()
    if not report['preflight']['safe']:
        report.update(status='skipped', reason='Existing application registration; no installation attempted.')
        save()
        print(json.dumps({'status': report['status'], 'reason': report['reason'], 'report': str(result_file)}))
        return 2
    if args.check_only:
        report.update(status='checked', reason='Preconditions are clear; no installation requested.')
        save()
        print(json.dumps({'status': report['status'], 'safe': True, 'report': str(result_file)}))
        return 0
    # Keep nested bundled dependency/license paths below Win32 MAX_PATH.
    run_root = output / ('r-' + uuid.uuid4().hex[:8])
    if run_root.exists():
        raise RuntimeError('A unique QA installation parent already exists.')
    app = run_root / 'App'
    models = run_root / '模型 models – café'
    runtime = run_root / '画像 portable parent'
    report['folders'] = {'application': str(app), 'models': str(models), 'runtime': str(runtime)}
    expected_defaults = {'schema': 1, 'setup_mode': 'portable',
                         'model_directory': str(models), 'managed_ai_directory': str(runtime)}
    environment = os.environ.copy()
    environment['LOCALAPPDATA'] = str(run_root / 'isolated Local AppData')
    environment['LOCAL_IMAGE_DATA_DIR'] = str(run_root / 'isolated application profile')
    environment['LOCAL_REMOVE_DATA_DIR'] = environment['LOCAL_IMAGE_DATA_DIR']
    report['isolated_profile'] = environment['LOCAL_IMAGE_DATA_DIR']
    owned_uninstaller = None
    failures = []
    try:
        # Recheck immediately before the first Windows installation mutation.
        immediate_preflight = preflight()
        if not immediate_preflight['safe']:
            raise RuntimeError('Application registrations changed after preflight; install cancelled.')
        assert immediate_preflight['legacy_shortcuts'] == report['preflight']['legacy_shortcuts'], 'Legacy shortcuts changed before installation.'
        assert all(not Path(item['target']).is_relative_to(app)
                   for item in immediate_preflight['legacy_shortcuts']), 'A legacy shortcut unexpectedly points into this QA installation.'
        report['install'] = hidden_run([installer, '/CURRENTUSER', '/VERYSILENT', '/SUPPRESSMSGBOXES',
                                      '/SP-', '/NOICONS', '/AISETUP=portable', '/DIR=' + str(app),
                                      '/MODELDIR=' + str(models), '/AIDIR=' + str(runtime),
                                      '/LOG=' + str(output / 'install.log')], environment=environment)
        if report['install']['exit_code'] != 0:
            raise RuntimeError('EXE installation failed; see install.log.')
        native = app / 'Local Image.exe'
        backend = app / 'backend' / 'LocalRemoveBackend.exe'
        defaults = app / 'installation-defaults.json'
        assert native.is_file() and backend.is_file(), 'Missing installed host or bundled backend.'
        report['native_version'] = file_version(native)
        assert report['native_version'] == '0.6.0.0', 'Installed native version differs.'
        report['installed_defaults'] = json.loads(defaults.read_text(encoding='utf-8-sig'))
        assert report['installed_defaults'] == expected_defaults, 'Unicode folder choices were not preserved.'
        report['legacy_shortcuts_after_install'] = legacy_shortcuts()
        assert report['legacy_shortcuts_after_install'] == report['preflight']['legacy_shortcuts'], 'Installation changed another checkout\'s legacy shortcuts.'
        report['after_install_registrations'] = registrations()
        uninstall = [item for item in report['after_install_registrations']
                     if item['kind'] == 'uninstall' and item['hive'] == 'HKCU']
        assert uninstall, 'Current-user uninstall registration missing.'
        assert not any(item['kind'] == 'uninstall' and item['hive'] == 'HKLM'
                       for item in report['after_install_registrations']), 'Installer registered all-users unexpectedly.'
        registered = uninstall[0]['values']
        assert Path(registered['InstallLocation']).resolve() == app, 'Uninstall registration points outside QA.'
        assert registered['DisplayVersion'] == '0.6.0', 'Uninstall display version differs.'
        owned_uninstaller = app / 'unins000.exe'
        assert owned_uninstaller.is_file(), 'Installed uninstaller missing.'
        assert str(owned_uninstaller).casefold() in registered['UninstallString'].casefold(), 'Uninstaller path differs.'
        assert not models.exists() and not runtime.exists(), 'Installation created optional AI download folders.'
        for folder in (models, runtime):
            folder.mkdir(parents=True)
            (folder / 'retained-marker.txt').write_text('User storage must survive app uninstall.\n', encoding='utf-8')
        report['uninstall'] = hidden_run([owned_uninstaller, '/VERYSILENT', '/SUPPRESSMSGBOXES',
                                         '/NORESTART', '/LOG=' + str(output / 'uninstall.log')], environment=environment)
        assert report['uninstall']['exit_code'] == 0, 'Uninstall failed; see uninstall.log.'
        deadline = time.monotonic() + 10
        while owned_uninstaller.exists() and time.monotonic() < deadline:
            time.sleep(0.1)
        report['app_files_removed'] = not native.exists() and not backend.exists() and not defaults.exists()
        assert report['app_files_removed'], 'Installed application files or defaults remained.'
        report['storage_retained'] = all((folder / 'retained-marker.txt').read_text(encoding='utf-8') ==
                                         'User storage must survive app uninstall.\n' for folder in (models, runtime))
        assert report['storage_retained'], 'Uninstall changed sibling model/runtime storage.'
        report['after_uninstall_registrations'] = registrations()
        assert not any(item['kind'] in ('uninstall', 'project_class') or
                       (item['kind'] == 'open_with' and 'LocalRemove.Project' in item['values'])
                       for item in report['after_uninstall_registrations']), 'Owned application registrations remained.'
        report['legacy_shortcuts_after_uninstall'] = legacy_shortcuts()
        assert report['legacy_shortcuts_after_uninstall'] == report['preflight']['legacy_shortcuts'], 'Uninstall changed another checkout\'s legacy shortcuts.'
        report['legacy_shortcuts_preserved'] = True
        report.update(status='completed', passed=True)
    except Exception as error:
        failures.append(str(error))
        report.update(status='failed', failures=failures)
        after_failure = preflight()
        report['failure_state'] = {
            'registry': after_failure['registrations'],
            'legacy_shortcuts_preserved': after_failure['legacy_shortcuts'] == report['preflight']['legacy_shortcuts'],
            'app_files_absent': not (app / 'Local Image.exe').exists() and
                                not (app / 'backend' / 'LocalRemoveBackend.exe').exists(),
            'model_folder_created': models.exists(), 'runtime_folder_created': runtime.exists(),
        }
        # Never delete files or registrations on a failed test. The report keeps
        # the exact QA location so any required cleanup remains reviewable.
    save()
    print(json.dumps({'status': report['status'], 'passed': report['passed'],
                      'windows_account': report['windows_account'],
                      'failures': report.get('failures', []), 'report': str(result_file)}, ensure_ascii=False))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
