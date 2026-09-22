"""Installed application resources and per-user Windows data locations."""
import json
import os
from pathlib import Path
import shutil
import time
import uuid

APP_VERSION = '0.3.1'
APP_PORT = 51247
RESOURCE_DIR = Path(__file__).resolve().parent


def data_root():
    override = os.environ.get('LOCAL_REMOVE_DATA_DIR')
    if override:
        # Match the native host's lexical GetFullPath, including MSIX folder redirection.
        return Path(os.path.abspath(Path(override).expanduser()))
    local = os.environ.get('LOCALAPPDATA')
    return (Path(local) if local else Path.home() / 'AppData' / 'Local') / 'Local Remove'


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
        for key in ('comfy_directory', 'comfy_python', 'comfy_base_directory', 'managed_comfy_directory'):
            if isinstance(raw.get(key), str) and raw[key]:
                settings[key] = raw[key]
    except (OSError, ValueError, AttributeError):
        pass
    return settings


def write_config(changes):
    """Atomically update known settings without dropping desktop-owned fields."""
    destination = data_root() / 'config.json'
    destination.parent.mkdir(parents=True, exist_ok=True)
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
    return data_root() / 'ai'


def model_directory():
    configured = os.environ.get('LOCAL_REMOVE_MODELS_DIR') or read_config()['model_directory']
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
    for directory in (state_dir(), cache_dir(), log_dir(), cache_dir() / 'thumbnails'):
        directory.mkdir(parents=True, exist_ok=True)


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
