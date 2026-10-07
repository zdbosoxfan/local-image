"""Fixed native protocol validation; no page-selected paths or commands."""
import json
import uuid
from urllib.parse import urlsplit

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
