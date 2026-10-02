"""Installed application resources and per-user data locations."""
import json
import os
from pathlib import Path
import shutil
import sys
import time
import uuid

def _application_version():
    # Linux release bundles carry their own preview version; source and Windows
    # retain the backend version required by the existing Windows launcher.
    if getattr(sys, 'frozen', False) and sys.platform.startswith('linux'):
        executable_folder = Path(sys.executable).resolve().parent
        for folder in (executable_folder, executable_folder.parent):
            try:
                version = (folder / 'VERSION').read_text(encoding='utf-8').strip()
                if version and len(version) <= 80 and all(character.isalnum() or character in '.+~-' for character in version):
                    return version
            except OSError:
                pass
    return '0.7.0'


APP_VERSION = _application_version()
APP_NAME = 'Local Image'
APP_PORT = 51247
RESOURCE_DIR = Path(__file__).resolve().parent


def data_root():
    override = os.environ.get('LOCAL_IMAGE_DATA_DIR') or os.environ.get('LOCAL_REMOVE_DATA_DIR')
    if override:
        # Match the native host's lexical GetFullPath, including MSIX folder redirection.
        return Path(os.path.abspath(Path(override).expanduser()))
    local = os.environ.get('LOCALAPPDATA')
    if os.name != 'nt' and not local:
        if sys.platform == 'darwin':
            return Path.home() / 'Library' / 'Application Support' / APP_NAME
        xdg = os.environ.get('XDG_DATA_HOME', '')
        base = Path(xdg) if xdg and Path(xdg).is_absolute() else Path.home() / '.local' / 'share'
        return base / 'local-image'
    base = Path(local) if local else Path.home() / 'AppData' / 'Local'
    current, legacy = base / 'Local Image', base / 'Local Remove'
    # Match the native host, without moving recovery data or copying large files.
    def has_profile(folder):
        return (folder / 'config.json').is_file() or (folder / 'state').is_dir()
    return legacy if has_profile(legacy) and not has_profile(current) else current


def state_dir():
    return data_root() / 'state'


def cache_dir():
    return data_root() / 'cache'


def log_dir():
    return data_root() / 'logs'


def read_config():
    settings = {'comfy_port': 8188, 'model_directory': ''}
    try:
        raw = json.loads((data_root() / 'config.json').read_text(encoding='utf-8-sig'))
        if type(raw.get('comfy_port')) is int and 1 <= raw['comfy_port'] <= 65535:
            settings['comfy_port'] = raw['comfy_port']
        if isinstance(raw.get('model_directory'), str):
            settings['model_directory'] = raw['model_directory']
        for key in ('comfy_directory', 'comfy_python', 'comfy_base_directory', 'managed_comfy_directory', 'managed_ai_directory'):
            if isinstance(raw.get(key), str) and raw[key]:
                settings[key] = raw[key]
        if raw.get('setup_mode') in ('discover', 'portable', 'later'):
            settings['setup_mode'] = raw['setup_mode']
        if type(raw.get('hardware_guide_dismissed')) is bool:
            settings['hardware_guide_dismissed'] = raw['hardware_guide_dismissed']
        if type(raw.get('lora_show_adult_content')) is bool:
            settings['lora_show_adult_content'] = raw['lora_show_adult_content']
    except (OSError, ValueError, AttributeError):
        pass
    return settings


def write_config(changes):
    """Atomically update known settings without dropping desktop-owned fields."""
    destination = data_root() / 'config.json'
    destination.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    try:
        current = json.loads(destination.read_text(encoding='utf-8-sig'))
        if not isinstance(current, dict):
            current = {}
    except (OSError, ValueError):
        current = {}
    current.update(changes)
    temporary = destination.with_name('config-' + uuid.uuid4().hex + '.tmp')
    try:
        with temporary.open('x', encoding='utf-8') as stream:
            json.dump(current, stream, indent=2)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)
    return read_config()


def managed_ai_dir():
    configured = os.environ.get('LOCAL_IMAGE_AI_DIR') or read_config().get('managed_ai_directory')
    return Path(configured).expanduser() if configured else data_root() / 'ai'


def model_directory():
    configured = os.environ.get('LOCAL_IMAGE_MODELS_DIR') or os.environ.get('LOCAL_REMOVE_MODELS_DIR') or read_config()['model_directory']
    return Path(configured).expanduser() if configured else data_root() / 'models'


def workflow_file():
    target = data_root() / 'workflow.json'
    target.parent.mkdir(parents=True, exist_ok=True)
    if not target.exists():
        try:
            with target.open('xb') as destination, (RESOURCE_DIR / 'workflow.json').open('rb') as source:
                shutil.copyfileobj(source, destination)
        except FileExistsError:
            pass
    return target


def prepare_user_folders():
    data_root().mkdir(mode=0o700, parents=True, exist_ok=True)
    for directory in (state_dir(), cache_dir(), log_dir(), cache_dir() / 'thumbnails'):
        directory.mkdir(mode=0o700, parents=True, exist_ok=True)


def prune_thumbnails(max_bytes=256 * 1024 * 1024, max_age_days=7):
    """Delete only disposable thumbnails, never recovery sessions or projects."""
    folder = cache_dir() / 'thumbnails'
    folder.mkdir(parents=True, exist_ok=True)
    files = []
    for path in folder.glob('*.jpg'):
        try:
            if not path.is_symlink() and path.is_file():
                info = path.stat()
                files.append((info.st_mtime, info.st_size, path))
        except OSError:
            continue
    total = sum(size for _, size, _ in files)
    cutoff = time.time() - max_age_days * 86400
    for modified, size, path in sorted(files):
        if modified >= cutoff and total <= max_bytes:
            continue
        try:
            path.unlink()
            total -= size
        except OSError:
            pass
