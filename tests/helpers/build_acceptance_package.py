"""Build a separate QA package from the repository spec and native build script.

No installer is compiled or run. No application is launched. The existing local
venv and verified WebView2 development SDK must already be present.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--name', required=True)
    options = parser.parse_args()
    if not options.name or any(char not in 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_' for char in options.name):
        parser.error('Use a simple QA build name.')
    build_root = (ROOT / 'dist/frontend-full' / options.name).resolve()
    if not build_root.is_relative_to((ROOT / 'dist/frontend-full').resolve()):
        raise RuntimeError('QA build path is outside the intended directory.')
    package = build_root / 'package'
    if package.exists():
        raise RuntimeError('Choose a fresh build name so earlier QA packages remain intact.')
    sdk = ROOT / 'qa-artifacts/integration/devcache/webview2-1.0.4191.47/package'
    if not (sdk / 'lib/net462/Microsoft.Web.WebView2.WinForms.dll').is_file():
        raise RuntimeError('The previously verified project-local WebView2 SDK is unavailable.')
    evidence = ROOT / 'qa-artifacts/native-acceptance/builds' / options.name
    evidence.mkdir(parents=True, exist_ok=False)
    environment = {key.upper(): value for key, value in os.environ.items()}
    environment.update(PYINSTALLER_CONFIG_DIR=str(evidence / 'pyinstaller-cache'),
                       LOCAL_IMAGE_DATA_DIR=str(evidence / 'build-profile'), PYTHONIOENCODING='utf-8')
    sources = []
    for directory in (ROOT / 'backend', ROOT / 'desktop', ROOT / 'frontend/src', ROOT / 'packaging'):
        for path in sorted(directory.rglob('*')):
            if path.is_file() and '__pycache__' not in path.parts and path.suffix.lower() not in {'.pyc'}:
                sources.append({'file': path.relative_to(ROOT).as_posix(), 'sha256': digest(path)})
    log_path = evidence / 'build.log'
    commands = [
        [sys.executable, '-m', 'PyInstaller', '--noconfirm', '--distpath', str(package),
         '--workpath', str(build_root / 'build'), str(ROOT / 'packaging/local-remove.spec')],
        ['powershell.exe', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
         str(ROOT / 'desktop/Build-NativeHost.ps1'), '-PackageRoot', str(sdk), '-OutputDirectory', str(package)],
    ]
    with log_path.open('w', encoding='utf-8') as log:
        for command in commands:
            log.write('Command: ' + subprocess.list2cmdline(command) + '\n')
            log.flush()
            result = subprocess.run(command, cwd=ROOT, env=environment, stdout=log, stderr=subprocess.STDOUT,
                                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0)
            log.write('Exit code: ' + str(result.returncode) + '\n')
            if result.returncode:
                raise RuntimeError('Package build failed. Inspect ' + str(log_path))
    records = []
    for source in sorted((ROOT / 'backend/frontend_dist').rglob('*')):
        if source.is_file():
            relative = source.relative_to(ROOT / 'backend/frontend_dist')
            target = package / 'backend/_internal/frontend_dist' / relative
            if digest(source) != digest(target):
                raise RuntimeError('Packaged frontend mismatch: ' + str(relative))
            records.append({'file': relative.as_posix(), 'sha256': digest(target), 'bytes': target.stat().st_size})
    template = package / 'backend/_internal/frontend/react.html'
    if digest(template) != digest(ROOT / 'backend/frontend/react.html'):
        raise RuntimeError('Packaged React mount template differs from the source.')
    for source in sources:
        if digest(ROOT / source['file']) != source['sha256']:
            raise RuntimeError('Source changed while packaging: ' + source['file'])
    host = package / 'Local Image.exe'
    report = {'package': str(package), 'host': str(host), 'hostSha256': digest(host),
              'backendSha256': digest(package / 'backend/LocalRemoveBackend.exe'), 'sources': sources,
              'frontendFiles': records, 'templateSha256': digest(template),
              'applicationLaunched': False, 'installerRun': False, 'log': str(log_path)}
    (evidence / 'result.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    print(json.dumps({key: value for key, value in report.items() if key not in {'sources', 'frontendFiles'}}, indent=2))


if __name__ == '__main__':
    main()
