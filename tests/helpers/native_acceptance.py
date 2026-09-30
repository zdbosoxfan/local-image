"""Prepare isolated real-WebView2 acceptance infrastructure; default is read-only.

probe: inspect dependencies/package hashes, compile the test-only metadata observer,
and run its metadata-only capability command. No application is launched.
prepare: create a unique QA profile and synthetic files, still without launching.
launch: explicit later phase; requires current packaged frontend hashes and the
expected host hash. Never starts while another service owns port 51247.

NativeQaWindow supports only capabilities, windows and read-only UIA inspect.
It cannot resize/close windows, invoke/select/expand controls, or enter text.
All actual Windows UI inputs belong to the separately authorized Computer Use
session; neither probe nor this metadata helper sends those inputs.
"""
import argparse
import ctypes
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time
import urllib.parse
import urllib.request
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[2]
QA = ROOT / 'qa-artifacts'
HELPER_SOURCE = Path(__file__).with_name('NativeQaWindow.cs')
TOOLS = QA / 'native-acceptance' / 'tools'
HOST = ROOT / 'dist/frontend-milestone1/package/Local Image.exe'
REFERENCES = [
    'https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/playwright',
    'https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/debug-visual-studio-code',
    'https://learn.microsoft.com/en-us/dotnet/framework/ui-automation/obtaining-ui-automation-elements',
]


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def within(path, directory):
    value = Path(path).resolve()
    if not value.is_relative_to(Path(directory).resolve()):
        raise ValueError('Path is outside the permitted QA directory: ' + str(value))
    return value


def powershell(code):
    result = subprocess.run(['powershell.exe', '-NoProfile', '-NonInteractive', '-Command', code],
                            cwd=ROOT, capture_output=True, text=True, encoding='utf-8', errors='replace',
                            creationflags=subprocess.CREATE_NO_WINDOW)
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or result.stdout.strip())
    return json.loads(result.stdout.lstrip('\ufeff')) if result.stdout.strip() else None


def package_files(host):
    packaged = host.parent / 'backend/_internal/frontend_dist'
    source = ROOT / 'backend/frontend_dist'
    records = []
    for file in source.rglob('*'):
        if file.is_file():
            relative = file.relative_to(source)
            target = packaged / relative
            records.append({'file': relative.as_posix(), 'sourceSha256': sha(file),
                            'packagedSha256': sha(target) if target.is_file() else None})
    return {'matchesCurrentFrontend': bool(records) and all(item['sourceSha256'] == item['packagedSha256'] for item in records), 'files': records}


def build_probe(host):
    if os.name != 'nt':
        raise RuntimeError('Native acceptance requires Windows; no browser substitute will be launched.')
    host = within(host, ROOT / 'dist')
    if not host.is_file():
        raise RuntimeError('The requested QA package host is missing.')
    metadata = powershell(r"""
$result = @{}
foreach ($name in @('UIAutomationClient','UIAutomationTypes','WindowsBase')) {
  Add-Type -AssemblyName $name -ErrorAction Stop
}
$result.uiaClient = [System.Windows.Automation.AutomationElement].Assembly.Location
$result.uiaTypes = [System.Windows.Automation.AutomationPattern].Assembly.Location
$result.windowsBase = [System.Windows.Rect].Assembly.Location
$result.compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
$result.interactiveSession = [System.Environment]::UserInteractive
$result.is64Bit = [System.Environment]::Is64BitProcess
$result | ConvertTo-Json -Compress
""")
    TOOLS.mkdir(parents=True, exist_ok=True)
    helper = TOOLS / 'NativeQaWindow.exe'
    command = [metadata['compiler'], '/nologo', '/target:exe', '/out:' + str(helper),
               '/r:' + metadata['uiaClient'], '/r:' + metadata['uiaTypes'], '/r:' + metadata['windowsBase'],
               '/r:System.Web.Extensions.dll', str(HELPER_SOURCE)]
    compiled = subprocess.run(command, cwd=ROOT, capture_output=True, text=True,
                              creationflags=subprocess.CREATE_NO_WINDOW)
    if compiled.returncode:
        raise RuntimeError(compiled.stdout + compiled.stderr)
    checked = subprocess.run([str(helper), 'capabilities'], cwd=ROOT, capture_output=True, text=True,
                             creationflags=subprocess.CREATE_NO_WINDOW)
    if checked.returncode:
        raise RuntimeError(checked.stderr)
    report = {'timeUtc': datetime.now(timezone.utc).isoformat(), 'host': str(host), 'hostSha256': sha(host),
              'metadata': metadata, 'uiaProbe': json.loads(checked.stdout), 'helper': str(helper),
              'helperSourceSha256': sha(HELPER_SOURCE), 'packageFrontend': package_files(host),
              'applicationLaunched': False, 'windowsEnumerated': False, 'desktopActionsExecuted': False,
              'references': REFERENCES,
              'remainingRuntimeChecks': ['Actual QA window ownership and UIA tree', 'Owned Open/Folder/Project/Save dialogs',
                 'Cancel versus completed native save', 'Multi-document close cancellation', 'Native download completion',
                 'Trusted /remove native readiness', 'Actual WebView2 rendering/CSP', 'Window client sizes/current DPI/200% app text']}
    (TOOLS.parent / 'capabilities.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    return report


def prepare(host, output, mode):
    report = build_probe(host)
    run = within(output, QA)
    if run.exists():
        raise RuntimeError('Choose a new QA run directory; previous evidence is preserved.')
    run.mkdir(parents=True)
    fixtures, exports, profile = run / 'fixtures', run / 'exports', run / 'profile'
    fixtures.mkdir(); exports.mkdir(); profile.mkdir()
    from PIL import Image
    for index, color in enumerate(((125, 55, 35), (35, 85, 135)), 1):
        image = Image.new('RGB', (640, 480), color)
        for x in range(260, 276):
            for y in range(180, 196):
                image.putpixel((x, y), (245, 225, 195))
        image.save(fixtures / ('Synthetic ' + str(index) + '.png'))
    first = fixtures / 'Synthetic 1.png'; content = first.read_bytes()
    manifest = {'format': 'local-remove-project', 'version': 1, 'name': first.name, 'width': 640, 'height': 480,
                'bit_depth': 8, 'revision': 0, 'original': 'original.png', 'layers': [],
                'assets': {name: {'size': len(content), 'sha256': hashlib.sha256(content).hexdigest()} for name in ('original.png', 'base.png')}}
    project = fixtures / 'Synthetic legacy project.lremove'
    with zipfile.ZipFile(project, 'w', compression=zipfile.ZIP_STORED) as archive:
        archive.writestr('manifest.json', json.dumps(manifest))
        archive.writestr('original.png', content); archive.writestr('base.png', content)
    (profile / 'config.json').write_text(json.dumps({'comfy_port': 51999, 'setup_mode': 'later',
        'model_directory': str(profile / 'models'), 'managed_ai_directory': str(profile / 'ai')}, indent=2), encoding='utf-8')
    with socket.socket() as selected:
        selected.bind(('127.0.0.1', 0)); cdp_port = selected.getsockname()[1]
    record = {'schema': 1, 'runId': uuid.uuid4().hex, 'runRoot': str(run), 'host': report['host'], 'hostSha256': report['hostSha256'],
              'profile': str(profile), 'fixtureRoot': str(fixtures), 'exportRoot': str(exports), 'backendUrl': 'http://127.0.0.1:51247',
              'cdpPort': cdp_port, 'mode': mode, 'helper': report['helper'], 'preparedAtUtc': datetime.now(timezone.utc).isoformat(),
              'fixtures': [{'path': str(file), 'sha256': sha(file)} for file in fixtures.iterdir()], 'applicationLaunched': False}
    path = run / 'launch.json'; path.write_text(json.dumps(record, indent=2), encoding='utf-8')
    return {'manifest': str(path), 'prepared': True, 'launchableCurrentFrontend': report['packageFrontend']['matchesCurrentFrontend'], 'applicationLaunched': False}


def occupied(port):
    with socket.socket() as probe:
        probe.settimeout(1)
        return probe.connect_ex(('127.0.0.1', int(port))) == 0


def filetime(pid):
    from ctypes import wintypes
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.GetProcessTimes.argtypes = [wintypes.HANDLE] + [ctypes.POINTER(wintypes.FILETIME)] * 4
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    handle = kernel.OpenProcess(0x1000, False, pid)
    if not handle:
        raise ctypes.WinError(ctypes.get_last_error())
    values = [wintypes.FILETIME() for _ in range(4)]
    try:
        if not kernel.GetProcessTimes(handle, *(ctypes.byref(value) for value in values)):
            raise ctypes.WinError(ctypes.get_last_error())
        return str((values[0].dwHighDateTime << 32) | values[0].dwLowDateTime)
    finally:
        kernel.CloseHandle(handle)


def debug_listener(port, host_pid):
    # Metadata only: the debug listener must belong to the QA host's process
    # tree and bind exclusively to loopback, never a wildcard/network address.
    return powershell(r"""
$port = PORT_VALUE
$hostPidValue = HOST_PID_VALUE
$listeners = @(Get-NetTCPConnection -State Listen -LocalPort $port -ErrorAction SilentlyContinue)
$all = @(Get-CimInstance Win32_Process)
$records = @()
foreach ($listener in $listeners) {
  $candidate = [int]$listener.OwningProcess
  $owned = $false
  for ($depth=0; $depth -lt 16 -and $candidate -gt 0; $depth++) {
    if ($candidate -eq $hostPidValue) { $owned=$true; break }
    $found = $all | Where-Object { $_.ProcessId -eq $candidate } | Select-Object -First 1
    if ($null -eq $found) { break }
    $candidate = [int]$found.ParentProcessId
  }
  $records += @{ address=$listener.LocalAddress; pid=$listener.OwningProcess; owned=$owned }
}
@{listeners=$records} | ConvertTo-Json -Depth 5 -Compress
""".replace('PORT_VALUE', str(int(port))).replace('HOST_PID_VALUE', str(int(host_pid))))


def launch(path, expected_sha, initial_path=None):
    path = within(path, QA); manifest = json.loads(path.read_text(encoding='utf-8'))
    run, profile = within(manifest['runRoot'], QA), within(manifest['profile'], QA)
    host = within(manifest['host'], ROOT / 'dist')
    initial_paths = []
    if initial_path is not None:
        initial = Path(initial_path).resolve()
        allowed = [Path(manifest['fixtureRoot']).resolve(), Path(manifest['exportRoot']).resolve()]
        if not any(initial.is_relative_to(directory) for directory in allowed) or not initial.is_file():
            raise RuntimeError('Initial handoff input must be an existing file in this QA run.')
        if initial.suffix.lower() not in {'.png', '.jpg', '.jpeg', '.tif', '.tiff', '.webp', '.lremove'}:
            raise RuntimeError('Unsupported QA handoff input.')
        initial_paths.append(str(initial))
    if expected_sha != sha(host) or manifest['hostSha256'] != expected_sha:
        raise RuntimeError('Host hash differs from the explicitly approved final build. Prepare a new run.')
    if not package_files(host)['matchesCurrentFrontend']:
        raise RuntimeError('The package does not contain the current frontend assets. Rebuild before native acceptance.')
    if occupied(51247) or occupied(manifest['cdpPort']):
        raise RuntimeError('A required port is already occupied. No existing process will be stopped.')
    # Metadata-only identity check prevents a second instance of this QA binary.
    running = powershell("Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq '" + str(host).replace("'", "''") + "' } | Select-Object ProcessId | ConvertTo-Json -Compress")
    if running:
        raise RuntimeError('This QA package executable already has a running instance.')
    env = {key.upper(): value for key, value in os.environ.items()}
    env.pop('LOCAL_IMAGE_FRONTEND', None)
    env.update(LOCAL_IMAGE_DATA_DIR=str(profile), LOCAL_REMOVE_DATA_DIR=str(profile), COMFY_HOST='127.0.0.1', COMFY_PORT='51999',
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=' + str(manifest['cdpPort']) + ' --remote-debugging-address=127.0.0.1')
    if manifest['mode'] != 'default':
        env['LOCAL_IMAGE_FRONTEND'] = manifest['mode']
    log = (run / 'host-launch.log').open('w', encoding='utf-8')
    # Deliberately visible: this phase is specifically an owned-window UI test.
    process = subprocess.Popen([str(host), *initial_paths], cwd=host.parent, env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
                               creationflags=subprocess.CREATE_NEW_PROCESS_GROUP)
    log.close()
    manifest.update(pid=process.pid, processStartFileTime=filetime(process.pid), applicationLaunched=True, launchedAtUtc=datetime.now(timezone.utc).isoformat(), initialPaths=initial_paths)
    path.write_text(json.dumps(manifest, indent=2), encoding='utf-8')
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    for _ in range(60):
        if process.poll() is not None:
            raise RuntimeError('The recorded QA host exited during startup; inspect its own log.')
        try:
            with opener.open(manifest['backendUrl'] + '/api/local-remove/runtime', timeout=1) as response:
                runtime = json.load(response)
            if runtime.get('application') != 'local-remove' or Path(runtime['data_root']).resolve() != profile:
                raise RuntimeError('Backend identity/profile mismatch; no UI action is authorized.')
            listener = debug_listener(manifest['cdpPort'], process.pid)
            records = listener['listeners']
            if not records:
                time.sleep(.5)
                continue
            if any(item['address'] not in ('127.0.0.1', '::1') or not item['owned'] for item in records):
                raise RuntimeError('Debug listener is not exclusively loopback and owned by the QA host. No CDP or UI action is authorized.')
            with opener.open('http://127.0.0.1:' + str(manifest['cdpPort']) + '/json/list', timeout=1) as response:
                targets = json.load(response)
            editors = [item for item in targets if item.get('type') == 'page' and urllib.parse.urlsplit(item.get('url', '')).scheme == 'http'
                       and urllib.parse.urlsplit(item.get('url', '')).netloc == '127.0.0.1:51247' and urllib.parse.urlsplit(item.get('url', '')).path == '/remove']
            if len(editors) == 1:
                manifest.update(runtime=runtime, debugListener=listener, editorTargetId=editors[0]['id'], trustedPage=editors[0]['url'], startupVerified=True)
                path.write_text(json.dumps(manifest, indent=2), encoding='utf-8')
                return {'manifest': str(path), 'pid': process.pid, 'trustedPage': editors[0]['url'], 'profile': str(profile), 'nativeHandshakeVerified': False}
        except (OSError, urllib.error.URLError):
            pass
        time.sleep(.5)
    raise RuntimeError('QA startup did not become verifiable within 30 seconds. The manifest identifies only the process that was started; no other process was touched.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='operation', required=True)
    probe_parser = sub.add_parser('probe'); probe_parser.add_argument('--host', type=Path, default=HOST)
    prepare_parser = sub.add_parser('prepare'); prepare_parser.add_argument('--host', type=Path, default=HOST); prepare_parser.add_argument('--output', type=Path, required=True); prepare_parser.add_argument('--mode', choices=('default', 'react', 'legacy'), default='default')
    launch_parser = sub.add_parser('launch'); launch_parser.add_argument('--manifest', type=Path, required=True); launch_parser.add_argument('--expected-host-sha256', required=True)
    launch_parser.add_argument('--initial-path', type=Path, help='Existing file inside this QA run, for actual external-editor argument handoff testing.')
    options = parser.parse_args()
    result = build_probe(options.host) if options.operation == 'probe' else prepare(options.host, options.output, options.mode) if options.operation == 'prepare' else launch(options.manifest, options.expected_host_sha256, options.initial_path)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
