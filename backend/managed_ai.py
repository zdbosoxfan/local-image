"""Local ComfyUI setup, verified downloads, and non-destructive GPU controls.

This module never runs page-supplied commands. The HTTP adapter accepts setup
mutations only from the credentialed native host after its own folder pickers.
"""
import asyncio
from contextlib import contextmanager
import ctypes
import hashlib
import json
import os
from pathlib import Path, PurePosixPath, PureWindowsPath
import shutil
import socket
import stat
import subprocess
import sys
import tempfile
import time
from urllib.parse import urljoin, urlsplit
import uuid

import aiohttp
import yaml

from ai_download_catalog import COMFY_RELEASE, FLUX_FILES
from app_paths import data_root, log_dir, managed_ai_dir, model_directory, read_config, state_dir, write_config

GIB = 1024 ** 3
MAX_EXTRACT_BYTES = 40 * GIB
MANAGED_FOLDER = 'LocalRemove-ComfyUI'
SEVENZIP_EXE = Path(__file__).resolve().parent / 'tools' / '7zip' / '7za.exe'
FLUX_NODES = ('UNETLoader', 'LoraLoaderModelOnly', 'CLIPLoader', 'VAELoader', 'CLIPTextEncode',
              'LoadImage', 'VAEEncode', 'ReferenceLatent', 'CFGGuider', 'RandomNoise',
              'KSamplerSelect', 'Flux2Scheduler', 'EmptyFlux2LatentImage', 'SamplerCustomAdvanced',
              'VAEDecode', 'PreviewImage')


class SetupError(ValueError):
    pass


def local_directory(value):
    if not isinstance(value, str) or not value.strip() or '\x00' in value:
        raise SetupError('Choose a local folder.')
    path = Path(value).expanduser()
    if not path.is_absolute() or str(path).startswith(('\\\\', '//')):
        raise SetupError('Choose an absolute folder on this PC, not a network share.')
    return path.resolve()


def _regular_file(path):
    try:
        return path.is_file() and not path.is_symlink()
    except OSError:
        return False


def _code_root(path):
    return (_regular_file(path / 'main.py') and _regular_file(path / 'folder_paths.py')
            and (path / 'comfy').is_dir())


def installation(path, python=None, base=None, kind=None):
    """Recognize a portable/source layout; never infer an arbitrary executable."""
    path = Path(path).resolve()
    for code in (path, path / 'ComfyUI', path / 'ComfyUI_windows_portable' / 'ComfyUI'):
        if not _code_root(code):
            continue
        portable = code.parent / 'python_embeded' / 'python.exe'
        interpreters = [portable, code / '.venv' / 'Scripts' / 'python.exe',
                        code.parent / '.venv' / 'Scripts' / 'python.exe']
        if python:
            interpreters.insert(0, Path(python))
        interpreter = next((item.resolve() for item in interpreters if _regular_file(item)), None)
        category = kind or ('portable' if interpreter == portable.resolve() else 'source')
        user_base = Path(base).resolve() if base else code
        identity = hashlib.sha256((str(code).casefold() + '|' + str(user_base).casefold()).encode()).hexdigest()[:24]
        return {'id': identity, 'path': str(code), 'name': 'ComfyUI ' + category,
                'kind': category, 'startable': interpreter is not None,
                'python': str(interpreter) if interpreter else '', 'base_directory': str(user_base)}
    return None


def desktop_installations():
    """Read Desktop's declared user base; do not scan user documents recursively."""
    roaming = Path(os.environ.get('APPDATA', str(Path.home() / 'AppData' / 'Roaming')))
    local = Path(os.environ.get('LOCALAPPDATA', str(Path.home() / 'AppData' / 'Local')))
    bases = []
    configs = [roaming / 'ComfyUI' / 'extra_models_config.yaml']
    try:
        configs += list((roaming / 'Comfy Desktop' / 'instance-model-paths').glob('*.yaml'))[:40]
    except OSError:
        pass
    for config in configs:
        try:
            if config.stat().st_size < 1024 * 1024:
                parsed = yaml.safe_load(config.read_text(encoding='utf-8-sig'))
                for value in parsed.values() if isinstance(parsed, dict) else []:
                    if isinstance(value, dict) and isinstance(value.get('base_path'), str):
                        bases.append(local_directory(value['base_path']))
        except (OSError, ValueError, yaml.YAMLError):
            pass
    code_paths = [local / 'Programs' / name / 'resources' / 'ComfyUI'
                  for name in ('Comfy Desktop', 'ComfyUI', 'ComfyUI/Comfy Desktop')]
    found = []
    for base in bases:
        for code in code_paths:
            item = installation(code, base / '.venv' / 'Scripts' / 'python.exe', base, 'desktop')
            if item:
                found.append(item)
    return found


def detect_installations():
    config = read_config()
    candidates = [managed_ai_dir() / MANAGED_FOLDER,
                  Path.home() / 'ComfyUI', Path.home() / 'ComfyUI_windows_portable',
                  Path.home() / 'Documents' / 'ComfyUI', Path.home() / 'Documents' / 'ComfyUI_windows_portable',
                  Path.home() / 'Downloads' / 'ComfyUI_windows_portable']
    for name in ('comfy_directory', 'managed_comfy_directory'):
        if config.get(name):
            candidates.insert(0, Path(config[name]))
    for value in os.environ.get('LOCAL_REMOVE_COMFY_CANDIDATES', '').split(os.pathsep):
        if value:
            candidates.append(Path(value))
    found = desktop_installations()
    try:
        for child in list((Path.home() / 'ComfyUI-Installs').iterdir())[:40]:
            if child.is_dir():
                candidates.extend((child, child / 'ComfyUI'))
    except OSError:
        pass
    for path in candidates:
        try:
            item = installation(path)
            if item:
                found.append(item)
        except (OSError, ValueError):
            pass
    # A saved Desktop installation keeps code and its user environment separate.
    if config.get('comfy_directory'):
        item = installation(config['comfy_directory'], config.get('comfy_python'), config.get('comfy_base_directory'))
        if item:
            found.insert(0, item)
    return list({item['id']: item for item in reversed(found)}.values())


def selected_installation():
    config = read_config()
    if not config.get('comfy_directory'):
        return None
    return installation(config['comfy_directory'], config.get('comfy_python'), config.get('comfy_base_directory'))


def configure(*, comfy_directory=None, model_directory=None, installation_id=None, comfy_port=None):
    changes = {}
    if comfy_directory and installation_id:
        raise SetupError('Choose one ComfyUI installation.')
    item = None
    if installation_id:
        item = next((candidate for candidate in detect_installations() if candidate['id'] == installation_id), None)
        if not item:
            raise SetupError('That installation is no longer available. Detect installations again.')
    elif comfy_directory:
        path = local_directory(comfy_directory)
        item = installation(path)
        if not item:
            item = next((candidate for candidate in desktop_installations()
                         if path in (Path(candidate['path']), Path(candidate['base_directory']))), None)
        if not item:
            raise SetupError('Choose a ComfyUI folder containing main.py, or its portable parent folder.')
    if item:
        changes.update(comfy_directory=item['path'], comfy_python=item['python'], comfy_base_directory=item['base_directory'])
        if model_directory is None and not read_config().get('model_directory'):
            shared = Path.home() / 'ComfyUI-Shared' / 'models'
            changes['model_directory'] = str(shared if shared.is_dir() else Path(item['base_directory']) / 'models')
    if model_directory is not None:
        changes['model_directory'] = str(local_directory(model_directory))
    if comfy_port is not None:
        if type(comfy_port) is not int or not 1 <= comfy_port <= 65535 or comfy_port == 51247:
            raise SetupError('Choose a valid ComfyUI port other than the editor port.')
        changes['comfy_port'] = comfy_port
    if not changes:
        raise SetupError('Choose a ComfyUI installation or model folder first.')
    return write_config(changes)


def sha256_file(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(4 * 1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def checked_download_url(url):
    parts = urlsplit(url)
    host = (parts.hostname or '').lower()
    hosts = {'github.com', 'release-assets.githubusercontent.com', 'objects.githubusercontent.com',
             'huggingface.co', 'cdn-lfs.huggingface.co', 'cdn-lfs.hf.co', 'us.aws.cdn.hf.co'}
    if (parts.scheme != 'https' or parts.username or parts.password or parts.port not in (None, 443)
            or not (host in hosts or host.endswith('.xethub.hf.co'))):
        raise SetupError('The download redirected outside its trusted publisher.')
    return url


async def download_verified(artifact, target, progress=lambda done, total: None):
    """Publish a file only after byte count and upstream SHA-256 both match."""
    target = Path(target)
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists():
        accepted = (artifact, *artifact.get('compatible_existing', ()))
        matches = [item for item in accepted if item['bytes'] == target.stat().st_size]
        if target.is_symlink() or not matches:
            raise SetupError(f'{target.name} already exists but does not match the required file. Choose an empty model folder or move that file first.')
        digest = await asyncio.to_thread(sha256_file, target)
        if not any(digest == item['sha256'] for item in matches):
            raise SetupError(f'{target.name} already exists but its checksum does not match. It has not been overwritten.')
        progress(artifact['bytes'], artifact['bytes'])
        return
    if shutil.disk_usage(target.parent).free < artifact['bytes'] + 256 * 1024 * 1024:
        raise SetupError('There is not enough free disk space in the selected folder.')
    temporary = target.with_name('.' + target.name + '.local-remove-' + uuid.uuid4().hex + '.part')
    timeout = aiohttp.ClientTimeout(total=None, connect=30, sock_read=180)
    try:
        async with aiohttp.ClientSession(timeout=timeout, headers={'User-Agent': 'LocalRemove/0.2'}) as client:
            address = checked_download_url(artifact['url'])
            for _ in range(8):
                async with client.get(address, allow_redirects=False) as response:
                    if response.status in (301, 302, 303, 307, 308):
                        address = checked_download_url(urljoin(address, response.headers.get('Location', '')))
                        continue
                    if response.status != 200:
                        raise SetupError(f'The publisher could not provide {target.name} (HTTP {response.status}). Try again later.')
                    received = 0
                    digest = hashlib.sha256()
                    with temporary.open('xb') as stream:
                        async for block in response.content.iter_chunked(1024 * 1024):
                            received += len(block)
                            if received > artifact['bytes']:
                                raise SetupError('The downloaded file is larger than its verified release metadata.')
                            stream.write(block)
                            digest.update(block)
                            progress(received, artifact['bytes'])
                        stream.flush()
                        os.fsync(stream.fileno())
                    if received != artifact['bytes'] or digest.hexdigest() != artifact['sha256']:
                        raise SetupError('The download did not match its verified checksum. No model was installed; try again.')
                    # Windows rename does not overwrite an existing destination.
                    if target.exists():
                        raise SetupError('A file appeared at the destination during the download. It has not been overwritten.')
                    temporary.rename(target)
                    return
            raise SetupError('The publisher redirected the download too many times.')
    finally:
        temporary.unlink(missing_ok=True)


def validate_archive_members(members, destination):
    destination = Path(destination).resolve()
    seen = set()
    total = 0
    if len(members) > 250000:
        raise SetupError('The ComfyUI archive contains too many files.')
    for member in members:
        name = member.filename.replace('\\', '/')
        relative = PurePosixPath(name)
        if (relative.is_absolute() or PureWindowsPath(name).drive or '..' in relative.parts
                or not relative.parts or any(':' in part or part.endswith(('.', ' ')) for part in relative.parts)):
            raise SetupError('The ComfyUI archive contains an unsafe file path.')
        normalized = str(relative).casefold()
        if normalized in seen:
            raise SetupError('The ComfyUI archive contains duplicate file paths.')
        seen.add(normalized)
        resolved = (destination / Path(*relative.parts)).resolve()
        if not resolved.is_relative_to(destination):
            raise SetupError('The ComfyUI archive would write outside its installation folder.')
        if member.is_symlink or not (member.is_file or member.is_directory):
            raise SetupError('The ComfyUI archive contains a link or unsupported file type.')
        total += member.uncompressed
        if total > MAX_EXTRACT_BYTES:
            raise SetupError('The ComfyUI archive exceeds the supported installation size.')
    return total


def extract_portable(archive, destination):
    import py7zr
    destination = Path(destination)
    attributes = destination.lstat()
    if (not stat.S_ISDIR(attributes.st_mode) or destination.is_symlink()
            or getattr(attributes, 'st_file_attributes', 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT):
        raise SetupError('ComfyUI must be extracted into a regular staging folder.')
    destination = destination.resolve()
    archive = Path(archive).resolve()
    if any(destination.iterdir()):
        raise SetupError('ComfyUI must be extracted into an empty staging folder.')
    if not _regular_file(SEVENZIP_EXE):
        raise SetupError('The bundled 7-Zip extractor is missing. Reinstall Local Remove before installing ComfyUI.')
    with py7zr.SevenZipFile(archive, mode='r', max_extract_size=MAX_EXTRACT_BYTES) as package:
        if package.needs_password():
            raise SetupError('The official ComfyUI archive should not require a password.')
        members = package.list()
        required = validate_archive_members(members, destination)
        if shutil.disk_usage(destination).free < required + GIB:
            raise SetupError('There is not enough free disk space to extract ComfyUI.')
    # The official portable release uses BCJ2, which py7zr can inspect but cannot
    # decode. Use only our bundled upstream decoder after validating its members.
    # No shell or -spf (absolute archive paths), and no system-tool fallback.
    try:
        result = subprocess.run([str(SEVENZIP_EXE), 'x', '-y', '-bd', '-bb0', '-bso0', '-bse1',
                                 '-o' + str(destination), '--', str(archive)],
                                cwd=str(destination), shell=False, stdin=subprocess.DEVNULL,
                                stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=1800,
                                creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
    except subprocess.TimeoutExpired as exc:
        raise SetupError('ComfyUI extraction took too long. Check free disk space and try again.') from exc
    except OSError as exc:
        raise SetupError('The bundled 7-Zip extractor could not start. Reinstall Local Remove and try again.') from exc
    if result.returncode != 0:
        raise SetupError(f'ComfyUI extraction failed (7-Zip code {result.returncode}). Try installing again.')
    validate_extracted_files(destination, members)
    item = installation(Path(destination) / 'ComfyUI_windows_portable')
    if not item or not item['startable']:
        raise SetupError('The downloaded package does not contain the expected ComfyUI portable runtime.')
    return Path(item['path']).parent


def validate_extracted_files(destination, members):
    """Check the decoder output before a staged runtime can become executable."""
    destination = Path(destination).resolve()
    expected = {str(PurePosixPath(member.filename.replace('\\', '/'))).casefold(): member.uncompressed
                for member in members if member.is_file}
    found = set()
    pending = [destination]
    entries = 0
    while pending:
        directory = pending.pop()
        for path in directory.iterdir():
            entries += 1
            if entries > 250000:
                raise SetupError('The extracted ComfyUI package contains too many files.')
            attributes = path.lstat()
            if (stat.S_ISLNK(attributes.st_mode)
                    or getattr(attributes, 'st_file_attributes', 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT
                    or not path.resolve().is_relative_to(destination)):
                raise SetupError('The extracted ComfyUI package contains an unsafe link or path.')
            if stat.S_ISDIR(attributes.st_mode):
                pending.append(path)
            elif stat.S_ISREG(attributes.st_mode):
                name = path.relative_to(destination).as_posix().casefold()
                if name not in expected or attributes.st_size != expected[name] or name in found:
                    raise SetupError('The extracted ComfyUI files do not match the verified archive.')
                found.add(name)
            else:
                raise SetupError('The extracted ComfyUI package contains an unsupported file type.')
    if found != expected.keys():
        raise SetupError('The extracted ComfyUI package is incomplete. Try installing again.')


async def comfy_request(path, body=None, *, port=None):
    port = port or read_config()['comfy_port']
    timeout = aiohttp.ClientTimeout(total=4)
    async with aiohttp.ClientSession(timeout=timeout) as client:
        method = client.post if body is not None else client.get
        kwargs = {'json': body} if body is not None else {}
        async with method(f'http://127.0.0.1:{port}' + path, allow_redirects=False, **kwargs) as response:
            if response.status != 200:
                raise SetupError('ComfyUI did not accept the request. Check the selected connection.')
            return await response.json() if response.content_type == 'application/json' else {}


def workflow_readiness(info):
    """Check what the running ComfyUI can actually execute and load."""
    if not isinstance(info, dict) or any(node not in info for node in FLUX_NODES):
        return {'ready': False, 'reason': 'The running ComfyUI is missing FLUX Klein support. Update it, or use the dedicated Local Remove installation.'}
    def choices(node, name):
        try:
            values = info[node]['input']['required'][name][0]
            return values if isinstance(values, list) else []
        except (KeyError, IndexError, TypeError):
            return []
    if 'flux2' not in choices('CLIPLoader', 'type'):
        return {'ready': False, 'reason': 'The running ComfyUI does not support the FLUX text encoder. Update it, or use the dedicated installation.'}
    loaders = (('UNETLoader', 'unet_name', FLUX_FILES[0]), ('CLIPLoader', 'clip_name', FLUX_FILES[1]),
               ('VAELoader', 'vae_name', FLUX_FILES[2]), ('LoraLoaderModelOnly', 'lora_name', FLUX_FILES[3]))
    missing = [artifact['name'] for node, key, artifact in loaders if artifact['name'] not in choices(node, key)]
    if missing:
        return {'ready': False, 'reason': 'The running ComfyUI cannot see these FLUX files: ' + ', '.join(missing)
                + '. Download them if needed, then restart ComfyUI with the selected model folder. Local Remove leaves other running instances unchanged.'}
    return {'ready': True, 'reason': ''}


async def service_state():
    port = read_config()['comfy_port']
    result = {'running': False, 'ready': False, 'reason': 'Start the AI backend to use FLUX Remove.',
              'busy': False, 'device': '', 'port': port}
    try:
        stats = await comfy_request('/system_stats', port=port)
        queue = await comfy_request('/queue', port=port)
        if not isinstance(stats.get('devices'), list) or not isinstance(queue.get('queue_running'), list):
            return result
        result['running'] = True
        result['busy'] = bool(queue['queue_running'] or queue.get('queue_pending'))
        result['device'] = stats['devices'][0].get('name', '') if stats['devices'] else ''
        if not stats['devices'] or stats['devices'][0].get('type') != 'cuda':
            result['reason'] = 'The running ComfyUI does not have a CUDA GPU available. Quick Heal remains available.'
        else:
            result.update(workflow_readiness(await comfy_request('/object_info', port=port)))
    except (aiohttp.ClientError, asyncio.TimeoutError, OSError, ValueError, KeyError, TypeError):
        if result['running']:
            result.update(ready=False, reason='ComfyUI is running, but its FLUX support could not be checked. Wait a moment and try again.')
    return result


def _port_in_use(port):
    try:
        with socket.create_connection(('127.0.0.1', port), timeout=1):
            return True
    except OSError:
        return False


def launch_command(item, port, extra_config):
    # Revalidate both known files just before starting; no .bat, shell, or page flags.
    code = Path(item['path'])
    interpreter = Path(item['python'])
    if not _code_root(code) or not _regular_file(interpreter) or interpreter.name.lower() != 'python.exe':
        raise SetupError('The selected ComfyUI runtime is incomplete. Choose another installation.')
    command = [str(interpreter), '-s', str(code / 'main.py'), '--listen', '127.0.0.1',
               '--port', str(port), '--disable-auto-launch', '--extra-model-paths-config', str(extra_config)]
    if 'python_embeded' in interpreter.parts:
        command.append('--windows-standalone-build')
    base = item['base_directory']
    if Path(base) != code:
        command += ['--base-directory', base]
    return command


@contextmanager
def external_process_environment():
    """Keep an external ComfyUI runtime independent of PyInstaller's DLLs."""
    environment = os.environ.copy()
    if sys.platform != 'win32' or not getattr(sys, 'frozen', False):
        yield environment
        return
    bundle = Path(sys._MEIPASS).resolve()
    if 'PATH' in environment:
        environment['PATH'] = os.pathsep.join(entry for entry in environment['PATH'].split(os.pathsep)
            if not Path(os.path.expandvars(entry.strip('"'))).resolve().is_relative_to(bundle))
    # PyInstaller documents that SetDllDirectory is inherited by subprocesses.
    # This block is synchronous: restore the exact parent setting before any
    # coroutine can launch another helper or import a lazily loaded extension.
    kernel32 = ctypes.WinDLL('kernel32', use_last_error=True)
    get_directory = kernel32.GetDllDirectoryW
    get_directory.argtypes = [ctypes.c_uint32, ctypes.c_wchar_p]
    get_directory.restype = ctypes.c_uint32
    set_directory = kernel32.SetDllDirectoryW
    set_directory.argtypes = [ctypes.c_wchar_p]
    set_directory.restype = ctypes.c_int
    previous = ctypes.create_unicode_buffer(32768)
    ctypes.set_last_error(0)
    length = get_directory(len(previous), previous)
    if length >= len(previous) or (not length and ctypes.get_last_error()):
        raise SetupError('Could not read the application DLL search path before starting ComfyUI.')
    if not set_directory(None):
        raise SetupError('Could not isolate the ComfyUI DLL search path.')
    try:
        yield environment
    finally:
        if not set_directory(previous.value or None):
            raise SetupError('Could not restore the application DLL search path after starting ComfyUI.')


class SetupManager:
    def __init__(self):
        self.job = None
        self.task = None
        self.process = None

    @property
    def active(self):
        return self.job is not None and self.job['status'] == 'running'

    def require_idle(self):
        if self.active:
            raise SetupError('Wait for the current AI setup step to finish.')

    def update(self, **values):
        if self.job:
            self.job.update(values)

    def _save_job(self):
        folder = state_dir()
        folder.mkdir(parents=True, exist_ok=True)
        temporary = folder / ('ai-setup-' + uuid.uuid4().hex + '.tmp')
        try:
            temporary.write_text(json.dumps(self.job, indent=2), encoding='utf-8')
            os.replace(temporary, folder / 'ai-setup-job.json')
        finally:
            temporary.unlink(missing_ok=True)

    def begin(self, action, operation, release=None):
        self.require_idle()
        self.job = {'id': uuid.uuid4().hex, 'action': action, 'status': 'running', 'phase': 'preparing',
                    'message': 'Preparing ' + action.replace('-', ' ') + '…', 'progress': 0,
                    'downloaded_bytes': 0, 'total_bytes': 0, 'error': None}
        try:
            self._save_job()
        except Exception:
            self.job = None
            raise

        async def run():
            try:
                await operation()
                self.update(status='complete', progress=100, phase='complete')
            except asyncio.CancelledError:
                self.update(status='error', phase='interrupted', error='Setup was interrupted. Open AI setup and try again.',
                            message='Setup was interrupted. Existing files were kept.')
                raise
            except Exception as error:
                self.update(status='error', phase='error', error=str(error), message=str(error))
            finally:
                if release:
                    release()
                self._save_job()
        self.task = asyncio.create_task(run())

    async def status(self):
        if self.job is None:
            try:
                saved = json.loads((state_dir() / 'ai-setup-job.json').read_text(encoding='utf-8'))
                if isinstance(saved, dict) and saved.get('status') in ('running', 'complete', 'error'):
                    self.job = saved
                    if saved['status'] == 'running':
                        self.update(status='error', phase='interrupted', error='Setup was interrupted. Try the step again.',
                                    message='Setup was interrupted. Existing files were kept.')
            except (OSError, ValueError):
                pass
        service = await service_state()
        # Install jobs may publish their selected runtime while service probing
        # yields. Read config with the completed job state after that await.
        item = selected_installation()
        service.update(starting=self.active and self.job['action'] == 'start',
                       can_start=bool(item and item['startable'] and not service['running'] and not self.active),
                       can_eject=service['running'] and not service['busy'] and not self.active)
        root = model_directory()
        models = []
        for artifact in FLUX_FILES:
            path = root / artifact['folder'] / artifact['name']
            try:
                size = path.stat().st_size if _regular_file(path) else 0
            except OSError:
                size = 0
            sizes = [artifact['bytes'], *(item['bytes'] for item in artifact.get('compatible_existing', ()))]
            models.append({'name': artifact['name'], 'folder': artifact['folder'], 'label': artifact['label'],
                           'exists': size in sizes, 'bytes': size, 'expected_bytes': artifact['bytes']})
        return {'installation': item, 'installations': detect_installations(),
                'managed_directory': str(managed_ai_dir()), 'model_directory': str(root),
                'models': models, 'service': service, 'job': dict(self.job) if self.job else None,
                'download_bytes': sum(model['expected_bytes'] for model in models if not model['exists'])}

    async def install(self, directory):
        parent = local_directory(directory)
        parent.mkdir(parents=True, exist_ok=True)
        destination = parent / MANAGED_FOLDER
        if destination.exists():
            raise SetupError(f'{MANAGED_FOLDER} already exists. Choose its installation instead, or choose another parent folder.')
        if shutil.disk_usage(parent).free < 12 * GIB:
            raise SetupError('Allow at least 12 GB of free space to download and extract ComfyUI. Models need additional space.')
        # TemporaryDirectory owns only this unique staging folder. No user folder
        # is ever recursively deleted, including failed or interrupted installs.
        with tempfile.TemporaryDirectory(prefix='.local-remove-install-', dir=parent) as staging:
            staging = Path(staging)
            archive = staging / COMFY_RELEASE['name']
            self.update(phase='downloading', message='Downloading the official ComfyUI portable package…', total_bytes=COMFY_RELEASE['bytes'])
            await download_verified(COMFY_RELEASE, archive, lambda done, total: self.update(
                downloaded_bytes=done, progress=round(done / total * 60, 1)))
            extracted = staging / 'extracted'
            extracted.mkdir()
            self.update(phase='extracting', message='Extracting ComfyUI. This can take several minutes…', progress=65)
            portable = await asyncio.to_thread(extract_portable, archive, extracted)
            if destination.exists():
                raise SetupError('The destination was created by another operation. It has not been changed.')
            portable.rename(destination)
        item = installation(destination)
        write_config({'managed_comfy_directory': str(destination), 'comfy_directory': item['path'],
                      'comfy_python': item['python'], 'comfy_base_directory': item['base_directory'],
                      'model_directory': read_config().get('model_directory') or str(Path(item['path']) / 'models')})
        self.update(message='ComfyUI installed. Download the FLUX files, then start the AI backend.')

    async def download_models(self):
        root = model_directory().resolve()
        root.mkdir(parents=True, exist_ok=True)
        total = sum(artifact['bytes'] for artifact in FLUX_FILES)
        missing = sum(artifact['bytes'] for artifact in FLUX_FILES if not (root / artifact['folder'] / artifact['name']).exists())
        if shutil.disk_usage(root).free < missing + 256 * 1024 * 1024:
            raise SetupError('There is not enough disk space for the FLUX files in this model folder.')
        done_before = 0
        self.update(total_bytes=total)
        for artifact in FLUX_FILES:
            self.update(phase='downloading', message='Downloading or verifying ' + artifact['label'] + '…')
            await download_verified(artifact, root / artifact['folder'] / artifact['name'],
                lambda done, size: self.update(downloaded_bytes=done_before + done,
                                               progress=round((done_before + done) / total * 100, 1)))
            done_before += artifact['bytes']
        write_config({'model_directory': str(root)})
        self.update(message='All four FLUX files are verified and ready. Start the AI backend to use them.')

    async def start(self):
        state = await service_state()
        if state['running']:
            if not state.get('ready'):
                raise SetupError(state.get('reason') or 'The running ComfyUI is not ready for FLUX Remove.')
            self.update(message='ComfyUI is already running on the selected port.')
            return
        if await asyncio.to_thread(_port_in_use, state['port']):
            raise SetupError('The selected port is used by another application. Choose another ComfyUI port.')
        item = selected_installation()
        if not item or not item['startable']:
            raise SetupError('Choose an installed ComfyUI runtime, or install the dedicated copy first.')
        if self.process and self.process.poll() is None:
            raise SetupError('The AI backend is still starting. Check its log before trying again.')
        extra = state_dir() / 'local-remove-model-paths.yaml'
        extra.parent.mkdir(parents=True, exist_ok=True)
        extra.write_text(json.dumps({'local_remove': {'base_path': str(model_directory().resolve()),
            'diffusion_models': 'diffusion_models', 'text_encoders': 'text_encoders', 'vae': 'vae', 'loras': 'loras'}}), encoding='utf-8')
        command = launch_command(item, state['port'], extra)
        log_dir().mkdir(parents=True, exist_ok=True)
        with (log_dir() / 'comfyui.log').open('ab') as log, external_process_environment() as environment:
            self.process = subprocess.Popen(command, cwd=item['path'], stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0), shell=False, env=environment)
        self.update(phase='starting', message='Starting ComfyUI. The first launch may take a few minutes…', progress=10)
        deadline = time.monotonic() + 180
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise SetupError('ComfyUI could not start. Check comfyui.log in the Local Remove logs folder.')
            state = await service_state()
            if state['running']:
                self.update(message='AI backend started. FLUX loads into GPU memory when you apply a removal.'
                            if state.get('ready') else 'AI backend started. ' + state.get('reason', 'Finish setting up FLUX before removing objects.'))
                return
            await asyncio.sleep(1)
        raise SetupError('ComfyUI is still starting. Check comfyui.log; the process was left running.')

    async def eject(self):
        queue = await comfy_request('/queue')
        if not isinstance(queue.get('queue_running'), list) or not isinstance(queue.get('queue_pending'), list):
            raise SetupError('The selected service did not return a valid ComfyUI queue.')
        if queue['queue_running'] or queue['queue_pending']:
            raise SetupError('Wait for all queued ComfyUI work to finish before releasing GPU memory.')
        await comfy_request('/free', {'unload_models': True, 'free_memory': True})
        self.update(message='GPU unload requested. ComfyUI will release its loaded models when its worker is idle.')


manager = SetupManager()
