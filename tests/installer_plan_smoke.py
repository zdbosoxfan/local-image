"""Validate real installer plans without installing an application.

Run on Windows with an Inno build containing the production PLANONLY handler:
    python tests/installer_plan_smoke.py path/to/Local-Image-Setup-0.7.0.exe

Each invocation uses /CURRENTUSER and /PLANONLY=1. The installer must emit its
plan and abort before installation. No elevated process or GPU job is started.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import uuid


def cases_for(output, targets):
    # Keep application destinations short regardless of where the QA reports go.
    # The actual bundled PyInstaller files must still fit below Windows MAX_PATH.
    folders = output / 'uncreated AI targets'
    program_files = os.environ.get('ProgramW6432', os.environ.get('ProgramFiles', r'C:\Program Files'))
    windows = os.environ.get('WINDIR', r'C:\Windows')
    boundary_base = str(targets) + os.sep
    if len(boundary_base) >= 155:
        raise RuntimeError('The checkout is too deep to exercise the 155-character application path boundary.')
    return [
        {'name': 'discover_defaults', 'mode': 'discover'},
        {'name': 'later_defaults', 'mode': 'later'},
        {'name': 'portable_defaults', 'mode': 'portable'},
        {'name': 'unicode_and_spaces', 'mode': 'portable',
         'models': str(folders / '模型 files – café'), 'ai': str(folders / '画像 AI runtime')},
        {'name': 'models_only', 'models': str(folders / 'Model files only')},
        {'name': 'runtime_only', 'mode': 'portable', 'ai': str(folders / 'AI runtime only')},
        {'name': 'own_app_models', 'models': str(targets / 'own_app_models'), 'error': 'app installation'},
        {'name': 'own_app_runtime_child', 'ai': str(targets / 'own_app_runtime_child' / 'Runtime'), 'error': 'app installation'},
        {'name': 'program_files_models', 'models': str(Path(program_files) / 'Local Image Model QA'), 'error': 'Program Files'},
        {'name': 'program_files_runtime', 'ai': str(Path(program_files) / 'Local Image Runtime QA'), 'error': 'Program Files'},
        {'name': 'windows_models', 'models': str(Path(windows) / 'Local Image Model QA'), 'error': 'Windows'},
        {'name': 'windows_runtime', 'ai': str(Path(windows) / 'Local Image Runtime QA'), 'error': 'Windows'},
        {'name': 'unc_models', 'models': r'\\example-server\AI Models', 'error': 'absolute local'},
        {'name': 'unc_runtime', 'ai': r'\\example-server\AI Apps', 'error': 'absolute local'},
        {'name': 'drive_relative_models', 'models': r'C:AI Models', 'error': 'absolute local'},
        {'name': 'relative_runtime', 'ai': r'AI Apps', 'error': 'absolute local'},
        {'name': 'invalid_mode', 'mode': 'cloud', 'error': 'AISETUP'},
        {'name': 'invalid_character', 'models': r'C:\AI?Models', 'error': 'invalid character'},
        {'name': 'invalid_colon', 'models': r'C:\AI:Models', 'error': 'invalid character'},
        {'name': 'normalized_own_app',
         'models': str(targets / 'normalized_own_app') + r'\..\normalized_own_app\models',
         'error': 'app installation'},
        {'name': 'nonletter_drive', 'models': r'1:\AI Models', 'error': 'absolute local'},
        {'name': 'application_path_155',
         'application': boundary_base + 'a' * (155 - len(boundary_base))},
        {'name': 'application_path_156',
         'application': boundary_base + 'b' * (156 - len(boundary_base)),
         'error': 'shorter application folder'},
    ]


def run_case(installer, output, targets, case):
    installation = Path(case.get('application', str(targets / case['name'])))
    report = output / (case['name'] + '-' + uuid.uuid4().hex + '.json')
    if installation.exists():
        raise RuntimeError(f'Refusing to use an existing installation target: {installation}')
    arguments = [str(installer), '/CURRENTUSER', '/VERYSILENT', '/SUPPRESSMSGBOXES', '/SP-',
                 '/PLANONLY=1', '/REPORT=' + str(report), '/DIR=' + str(installation)]
    if case.get('mode'):
        arguments.append('/AISETUP=' + case['mode'])
    if 'models' in case:
        arguments.append('/MODELDIR=' + case['models'])
    if 'ai' in case:
        arguments.append('/AIDIR=' + case['ai'])
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = subprocess.SW_HIDE
    started = time.monotonic()
    process = subprocess.run(arguments, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE, startupinfo=startup,
                             creationflags=subprocess.CREATE_NO_WINDOW, timeout=45)
    result = {'name': case['name'], 'arguments': arguments, 'returncode': process.returncode,
              'elapsed_seconds': round(time.monotonic() - started, 3),
              'application_path_length': len(str(installation)),
              'plan_report': str(report), 'installation_created': installation.exists(), 'passed': False}
    failures = []
    if not report.exists():
        failures.append('Installer did not write the plan report.')
    else:
        plan = json.loads(report.read_text(encoding='utf-8-sig'))
        result['plan'] = plan
        if Path(plan['installation_directory']) != installation:
            failures.append('Selected application directory was not preserved.')
        if plan['scope'] != 'current-user':
            failures.append('PLANONLY did not use the requested current-user scope.')
        error = plan.get('error', '')
        if case.get('error'):
            if case['error'].casefold() not in error.casefold():
                failures.append('Expected validation error: ' + case['error'])
        elif error:
            failures.append('Unexpected validation error: ' + error)
        else:
            expected = {'schema': 1, 'setup_mode': case.get('mode', 'discover'),
                        'model_directory': case.get('models', ''),
                        'managed_ai_directory': case.get('ai', '')}
            if plan['defaults'] != expected:
                failures.append('Default settings differ from selected folders or setup mode.')
    if result['installation_created']:
        failures.append('PLANONLY created the application destination.')
    if (output / 'uncreated AI targets').exists():
        failures.append('PLANONLY created model/runtime destinations.')
    if failures:
        result['failures'] = failures
    else:
        result['passed'] = True
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('installer', type=Path)
    parser.add_argument('--output', type=Path,
                        default=Path('qa-artifacts/v07/installer-plan-matrix'))
    args = parser.parse_args()
    if os.name != 'nt':
        parser.error('The Inno installer plan test requires Windows.')
    installer = args.installer.resolve(strict=True)
    output = args.output.resolve()
    workspace = Path(__file__).resolve().parents[1]
    if not output.is_relative_to(workspace / 'qa-artifacts'):
        parser.error('QA output must be inside this workspace qa-artifacts directory.')
    output.mkdir(parents=True, exist_ok=True)
    targets = workspace / 'qa-artifacts' / ('ip-' + uuid.uuid4().hex[:8])
    # Verify no installer registration or shortcut changes occur, in addition
    # to checking that neither application nor optional AI folders are created.
    from smoke_windows_installer_qa import registry_snapshot, shortcuts_snapshot
    from smoke_windows_installer import windows_username
    app_identity = 'LocalImage.PlanOnly.' + uuid.uuid4().hex
    project_identity = app_identity + '.Project'
    before_registry = registry_snapshot(app_identity, project_identity)
    before_shortcuts = shortcuts_snapshot()
    results = []
    for case in cases_for(output, targets):
        try:
            result = run_case(installer, output, targets, case)
        except Exception as error:
            result = {'name': case['name'], 'passed': False, 'failures': [str(error)]}
        results.append(result)
        print(json.dumps({'case': result['name'], 'passed': result['passed'],
                          'failures': result.get('failures', [])}, ensure_ascii=False), flush=True)
    registry_preserved = registry_snapshot(app_identity, project_identity) == before_registry
    shortcuts_preserved = shortcuts_snapshot() == before_shortcuts
    report = {'installer': str(installer),
              'windows_account': windows_username(),
              'sha256': hashlib.sha256(installer.read_bytes()).hexdigest(),
              'plan_only': True, 'scope': 'current-user', 'case_count': len(results),
              'passed': all(item['passed'] for item in results) and registry_preserved and shortcuts_preserved,
              'registry_preserved': registry_preserved, 'shortcuts_preserved': shortcuts_preserved,
              'passed_count': sum(item['passed'] for item in results), 'cases': results}
    (output / 'results.json').write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding='utf-8')
    print(json.dumps({'passed': report['passed'], 'passed_count': report['passed_count'],
                      'case_count': report['case_count'], 'report': str(output / 'results.json')}))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
