"""Actual per-user installation acceptance with an isolated installer identity.

The package is unchanged. Only compile-time AppIdentity and ProjectIdentity
separate QA registration from an existing production installation. The original
association trees and shortcut files must remain identical after uninstall.
No editor, GPU, ComfyUI or model downloads are started by this test.
"""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import socket
import sys
import time
import uuid
import winreg

from smoke_windows_installer import file_version, hidden_run, shell_folder, windows_username


UNINSTALL_PARENT = r'Software\Microsoft\Windows\CurrentVersion\Uninstall'
CLASSES = r'Software\Classes'
OPENWITH = CLASSES + r'\.lremove\OpenWithProgids'
REGISTRY_VIEWS = ((64, winreg.KEY_WOW64_64KEY), (32, winreg.KEY_WOW64_32KEY))
HIVES = (('HKCU', winreg.HKEY_CURRENT_USER), ('HKLM', winreg.HKEY_LOCAL_MACHINE))


def registry_tree(hive, key, view):
    try:
        with winreg.OpenKey(hive, key, 0, winreg.KEY_READ | view) as opened:
            child_count, value_count, _ = winreg.QueryInfoKey(opened)
            values = []
            for index in range(value_count):
                name, value, value_type = winreg.EnumValue(opened, index)
                if isinstance(value, bytes):
                    value = {'base64': base64.b64encode(value).decode('ascii')}
                values.append({'name': name, 'value': value, 'type': value_type})
            children = {}
            for index in range(child_count):
                name = winreg.EnumKey(opened, index)
                children[name] = registry_tree(hive, key + '\\' + name, view)
            return {'values': sorted(values, key=lambda item: item['name']),
                    'children': dict(sorted(children.items()))}
    except FileNotFoundError:
        return None


def registry_snapshot(app_identity, project_identity):
    keys = {
        'original_uninstall': UNINSTALL_PARENT + r'\LocalRemove.Windows_is1',
        'original_project': CLASSES + r'\LocalRemove.Project',
        'extension': CLASSES + r'\.lremove',
        'qa_uninstall': UNINSTALL_PARENT + '\\' + app_identity + '_is1',
        'qa_project': CLASSES + '\\' + project_identity,
    }
    return {f'{hive_name}/{bits}/{name}': registry_tree(hive, key, view)
            for hive_name, hive in HIVES for bits, view in REGISTRY_VIEWS
            for name, key in keys.items()}


def original_registry(snapshot, qa_project=None):
    result = {key: value for key, value in snapshot.items()
              if key.endswith(('/original_uninstall', '/original_project', '/extension'))}
    # During installation, the only permitted extension change is the unique
    # QA OpenWithProgids value; compare every other value and subkey verbatim.
    if qa_project:
        result = json.loads(json.dumps(result))
        for key, tree in result.items():
            if not key.endswith('/extension') or tree is None:
                continue
            openwith = tree['children'].get('OpenWithProgids')
            if openwith:
                openwith['values'] = [entry for entry in openwith['values']
                                      if entry['name'] != qa_project]
    return result


def shortcuts_snapshot():
    paths = []
    for csidl in (0x10, 0x19):
        for app_name in ('Local Remove', 'Local Image'):
            paths.append(shell_folder(csidl) / (app_name + '.lnk'))
    for csidl in (0x02, 0x17):
        for app_name in ('Local Remove', 'Local Image'):
            folder = shell_folder(csidl) / app_name
            if folder.exists():
                paths.extend(path for path in folder.rglob('*') if path.is_file())
    return {str(path): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(set(paths)) if path.is_file()}


def port_idle():
    with socket.socket() as connection:
        connection.settimeout(0.5)
        return connection.connect_ex(('127.0.0.1', 51247)) != 0


def qa_registrations_absent(snapshot):
    return all(value is None for key, value in snapshot.items()
               if key.endswith(('/qa_uninstall', '/qa_project')))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=Path('qa-artifacts/v06/qa-installer'))
    parser.add_argument('--package', type=Path, default=Path('dist/local-image-v06/package'))
    parser.add_argument('--prerequisites', type=Path, default=Path('dist/local-image-v06/prerequisites'))
    parser.add_argument('--compiler', type=Path, default=Path('dist/tools/inno/ISCC.exe'))
    args = parser.parse_args()
    if os.name != 'nt':
        parser.error('This test requires Windows.')
    workspace = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    if not output.is_relative_to(workspace / 'qa-artifacts'):
        parser.error('QA output must be inside this workspace qa-artifacts directory.')
    output.mkdir(parents=True, exist_ok=True)
    identifier = uuid.uuid4().hex
    app_identity = 'LocalImage.QA.' + identifier
    project_identity = app_identity + '.Project'
    run_root = output / ('r-' + identifier[:8])
    app = run_root / 'App'
    if len(str(app)) > 155 or run_root.exists():
        parser.error('Use a shorter QA output location or a fresh unique run directory.')
    models = run_root / '\u6a21\u578b models \u2013 caf\u00e9'
    runtime = run_root / '\u753b\u50cf portable parent'
    profile = run_root / 'isolated application profile'
    environment = os.environ.copy()
    environment.update(LOCALAPPDATA=str(run_root / 'isolated Local AppData'),
                       APPDATA=str(run_root / 'isolated Roaming AppData'),
                       LOCAL_IMAGE_DATA_DIR=str(profile), LOCAL_REMOVE_DATA_DIR=str(profile))
    before_registry = registry_snapshot(app_identity, project_identity)
    before_shortcuts = shortcuts_snapshot()
    report = {
        'status': 'pending', 'passed': False, 'windows_account': windows_username(),
        'scope': 'current-user', 'app_launched': False, 'model_downloads': False,
        'identity_difference': 'Only installer AppIdentity and ProjectIdentity differ from release.',
        'app_identity': app_identity, 'project_identity': project_identity,
        'folders': {'application': str(app), 'models': str(models), 'runtime': str(runtime),
                    'profile': str(profile)},
        'registry_before': before_registry, 'shortcuts_before': before_shortcuts,
    }
    result_file = output / 'results.json'

    def save():
        result_file.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')

    save()
    if not qa_registrations_absent(before_registry) or not port_idle():
        report.update(status='skipped', reason='QA identity exists or Local Image port is in use.')
        save()
        return 2
    owned_uninstaller = None
    markers = (models, runtime, profile)
    marker_text = 'User-owned storage survives Local Image uninstall.\n'
    try:
        compiler = args.compiler.resolve(strict=True)
        package = args.package.resolve(strict=True)
        prerequisites = args.prerequisites.resolve(strict=True)
        report['compile'] = hidden_run([
            compiler, '/Qp', '/DAppIdentity=' + app_identity,
            '/DProjectIdentity=' + project_identity, '/DPackageDir=' + str(package),
            '/DPrerequisiteDir=' + str(prerequisites), '/DInstallerDir=' + str(output),
            workspace / 'packaging' / 'LocalRemove.iss'], timeout=300)
        assert report['compile']['exit_code'] == 0, 'QA installer compilation failed.'
        installer = output / 'Local-Image-Setup-0.6.0.exe'
        report['installer'] = str(installer)
        report['sha256'] = hashlib.sha256(installer.read_bytes()).hexdigest()
        assert registry_snapshot(app_identity, project_identity) == before_registry, 'Registry changed before install.'
        assert shortcuts_snapshot() == before_shortcuts, 'Shortcuts changed before install.'
        assert port_idle(), 'Local Image started after preflight; installation cancelled.'
        report['install'] = hidden_run([
            installer, '/CURRENTUSER', '/VERYSILENT', '/SUPPRESSMSGBOXES', '/SP-', '/NOICONS',
            '/AISETUP=portable', '/DIR=' + str(app), '/MODELDIR=' + str(models),
            '/AIDIR=' + str(runtime), '/LOG=' + str(output / 'install.log')],
            environment=environment)
        assert report['install']['exit_code'] == 0, 'Actual installation failed; inspect install.log.'
        native = app / 'Local Image.exe'
        backend = app / 'backend' / 'LocalRemoveBackend.exe'
        defaults = app / 'installation-defaults.json'
        assert native.is_file() and backend.is_file(), 'Native host or backend is missing.'
        report['native_version'] = file_version(native)
        assert report['native_version'] == '0.6.0.0', 'Installed native version is incorrect.'
        report['installed_defaults'] = json.loads(defaults.read_text(encoding='utf-8-sig'))
        assert report['installed_defaults'] == {
            'schema': 1, 'setup_mode': 'portable', 'model_directory': str(models),
            'managed_ai_directory': str(runtime)}, 'Unicode storage choices changed during installation.'
        after_install = registry_snapshot(app_identity, project_identity)
        report['registry_after_install'] = after_install
        assert original_registry(after_install, project_identity) == original_registry(before_registry), 'Production registrations or original associations changed.'
        qa_uninstall = after_install['HKCU/64/qa_uninstall']
        assert qa_uninstall is not None, 'QA per-user uninstall registration is missing.'
        registered = {entry['name']: entry['value'] for entry in qa_uninstall['values']}
        assert Path(registered['InstallLocation']).resolve() == app, 'QA uninstall registration targets another folder.'
        assert registered['DisplayVersion'] == '0.6.0', 'QA uninstall version differs.'
        assert after_install['HKCU/64/qa_project'] is not None, 'QA project association is missing.'
        assert all(value is None for key, value in after_install.items()
                   if key.startswith('HKLM/') and key.endswith(('/qa_uninstall', '/qa_project'))), 'Per-user installer unexpectedly registered all-users.'
        assert shortcuts_snapshot() == before_shortcuts, '/NOICONS or legacy shortcut preservation failed.'
        report['no_icons_preserved_shortcuts'] = True
        owned_uninstaller = app / 'unins000.exe'
        assert owned_uninstaller.is_file(), 'QA uninstaller is missing.'
        assert str(owned_uninstaller).casefold() in registered['UninstallString'].casefold(), 'QA uninstaller differs from registered path.'
        assert all(not folder.exists() for folder in markers), 'Installation created optional storage/profile folders.'
        for folder in markers:
            folder.mkdir(parents=True)
            (folder / 'retained-marker.txt').write_text(marker_text, encoding='utf-8')
        assert port_idle(), 'Local Image port must remain idle before QA uninstall.'
        report['uninstall'] = hidden_run([
            owned_uninstaller, '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART',
            '/LOG=' + str(output / 'uninstall.log')], environment=environment)
        assert report['uninstall']['exit_code'] == 0, 'QA uninstall failed; inspect uninstall.log.'
        deadline = time.monotonic() + 15
        while owned_uninstaller.exists() and time.monotonic() < deadline:
            time.sleep(0.1)
        report['app_files_removed'] = not any(path.exists() for path in (native, backend, defaults, owned_uninstaller))
        assert report['app_files_removed'], 'QA application files remain after uninstall.'
        report['storage_retained'] = all((folder / 'retained-marker.txt').read_text(encoding='utf-8') == marker_text
                                        for folder in markers)
        assert report['storage_retained'], 'User model, runtime or profile storage changed during uninstall.'
        report['registry_after_uninstall'] = registry_snapshot(app_identity, project_identity)
        assert report['registry_after_uninstall'] == before_registry, 'Registry state was not restored exactly.'
        report['shortcuts_after_uninstall'] = shortcuts_snapshot()
        assert report['shortcuts_after_uninstall'] == before_shortcuts, 'Original shortcut files changed.'
        report.update(status='completed', passed=True, original_registration_preserved=True,
                      original_associations_preserved=True, original_shortcuts_preserved=True)
    except Exception as error:
        report.update(status='failed', error=str(error))
        # No manual registry/filesystem cleanup: the exact owned QA installation
        # and failed step remain reviewable in this report. Never touch production.
        report['registry_after_failure'] = registry_snapshot(app_identity, project_identity)
        report['shortcuts_after_failure'] = shortcuts_snapshot()
    save()
    print(json.dumps({'status': report['status'], 'passed': report['passed'],
                      'windows_account': report['windows_account'], 'error': report.get('error'),
                      'report': str(result_file)}, ensure_ascii=True))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
