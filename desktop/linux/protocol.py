"""Fixed native protocol validation; no page-selected paths or commands."""
import hashlib
import json
from pathlib import Path
import re
import uuid
from urllib.parse import urlsplit, unquote

BASE = 'http://127.0.0.1:51247'
# A transport memory budget, shared with the Windows host. Batch selections are
# bounded by encoded request size rather than an arbitrary number of images.
MAX_NATIVE_MESSAGE_BYTES = 16 * 1024 * 1024


def message_within_limit(raw):
    if not isinstance(raw, str) or len(raw) > MAX_NATIVE_MESSAGE_BYTES:
        return False
    try:
        return len(raw.encode('utf-8')) <= MAX_NATIVE_MESSAGE_BYTES
    except UnicodeEncodeError:
        return False


def decode_message(raw):
    if not message_within_limit(raw):
        return None
    try:
        message = json.loads(raw)
    except (ValueError, RecursionError):
        return None
    if (not isinstance(message, dict) or not isinstance(message.get('id'), str)
            or not 1 <= len(message['id']) <= 128 or not isinstance(message.get('action'), str)):
        return None
    return message


def trusted_page(address):
    try:
        u = urlsplit(address)
        return (u.scheme == 'http' and u.hostname == '127.0.0.1' and u.port == 51247
                and not u.username and not u.password and u.path == '/remove')
    except ValueError:
        return False


def identifier(value):
    if not isinstance(value, str) or str(uuid.UUID(value)) != value:
        raise ValueError('Choose an open document or prepared batch.')
    return value


def local_drop_paths(urls):
    """Only file-manager URLs from a native Qt drop, never page strings."""
    paths = []
    for value in urls:
        try:
            url = urlsplit(value)
            path = unquote(url.path)
            if (url.scheme != 'file' or url.netloc not in ('', 'localhost') or url.query or url.fragment
                    or not path.startswith('/') or '\x00' in path):
                return []
            if path not in paths:
                paths.append(path)
        except (ValueError, TypeError):
            return []
    return paths


def project_payload(message):
    revision = message.get('revision')
    if type(revision) is not int or not 0 <= revision <= 2147483647:
        raise ValueError('The project revision is invalid.')
    return {'session_id': identifier(message.get('session_id')), 'revision': revision}


def batch_payload(message):
    job = identifier(message.get('job_id'))
    items = message.get('item_ids')
    if not isinstance(items, list) or not items:
        raise ValueError('Select reviewed images to export.')
    items = [identifier(item) for item in items]
    if len(set(items)) != len(items):
        raise ValueError('Choose distinct reviewed images.')
    return job, items


def image_export_payload(message):
    payload = project_payload(message)
    if message.get('format') not in ('png', 'jpg', 'tif', 'webp'):
        raise ValueError('Choose a supported export format.')
    name = message.get('filename')
    if not isinstance(name, str) or not 1 <= len(name) <= 255:
        raise ValueError('Enter an export filename.')
    for key in ('width', 'height'):
        if type(message.get(key)) is not int or not 1 <= message[key] <= 32768:
            raise ValueError('Choose valid export dimensions.')
    payload.update({key: message[key] for key in ('format', 'filename', 'width', 'height')})
    return payload


def trusted_download(address):
    try:
        u = urlsplit(address)
        if u.scheme != 'http' or u.hostname != '127.0.0.1' or u.port != 51247 or u.username or u.password:
            return False
        parts = u.path.split('/')
        if len(parts) == 6 and parts[1:4] == ['api', 'local-remove', 'session']:
            return bool(identifier(parts[4])) and parts[5] in ('download', 'download-project', 'download-credits')
        if len(parts) == 7 and parts[1:5] == ['api', 'local-remove', 'batch', 'jobs']:
            return bool(identifier(parts[5])) and parts[6] == 'download'
    except (ValueError, TypeError, AttributeError):
        pass
    return False


UPDATE_PACKAGE_NAME = re.compile(r'^Local-Image-\d+(?:\.\d+)+(?:-[A-Za-z0-9.~+-]*)?-linux-x86_64\.(?:deb|tar\.gz)$')


def verified_update_package(info, updates_dir):
    """The backend's answer to the launcher-only installer query, checked
    again here: a release package inside the profile's updates folder whose
    size and SHA-256 match what the backend verified. Returns its path."""
    if not isinstance(info, dict):
        raise ValueError('The update is not ready to install.')
    path, digest, size = info.get('path'), info.get('sha256'), info.get('bytes')
    if (not isinstance(path, str) or not isinstance(digest, str) or not re.fullmatch(r'[0-9a-f]{64}', digest)
            or type(size) is not int or size <= 0):
        raise ValueError('The update is not ready to install.')
    folder = Path(updates_dir).resolve()
    target = Path(path)
    if target.is_symlink() or not target.is_file() or target.resolve().parent != folder:
        raise ValueError('The downloaded update is missing. Download it again.')
    target = target.resolve()
    if not UPDATE_PACKAGE_NAME.match(target.name) or target.stat().st_size != size:
        raise ValueError('The downloaded update does not match the release. Download it again.')
    with open(target, 'rb') as stream:
        if hashlib.file_digest(stream, 'sha256').hexdigest() != digest:
            raise ValueError('The downloaded update does not match its checksum. Download it again.')
    return target


class CloseGate:
    def __init__(self):
        self.pending = None
        self.approved = False

    def request(self):
        if self.pending is None:
            self.pending = str(uuid.uuid4())
        return self.pending

    def complete(self, identity, approved, busy=False):
        if identity != self.pending or self.pending is None or type(approved) is not bool:
            return False
        self.pending = None
        self.approved = approved and not busy
        return self.approved
