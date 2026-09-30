"""Per-profile stock API keys, encrypted for the current Windows account."""
import ctypes
from ctypes import wintypes
import os
from app_paths import data_root

ENV_KEYS = {'pexels': 'PEXELS_API_KEY', 'unsplash': 'UNSPLASH_ACCESS_KEY'}


class Blob(ctypes.Structure):
    _fields_ = [('size', wintypes.DWORD), ('data', ctypes.POINTER(ctypes.c_ubyte))]


def _crypt(data, protect):
    if os.name != 'nt':
        raise ValueError('Set the stock API key in the server environment on this platform.')
    buffer = ctypes.create_string_buffer(data)
    source = Blob(len(data), ctypes.cast(buffer, ctypes.POINTER(ctypes.c_ubyte)))
    result = Blob()
    function = ctypes.windll.crypt32.CryptProtectData if protect else ctypes.windll.crypt32.CryptUnprotectData
    if not function(ctypes.byref(source), None, None, None, None, 1, ctypes.byref(result)):
        raise ValueError('Windows could not access the saved stock connection. Reconnect this source.')
    try:
        return ctypes.string_at(result.data, result.size)
    finally:
        ctypes.windll.kernel32.LocalFree(result.data)


def credential_path(provider):
    if provider not in ENV_KEYS: raise ValueError('Choose Pexels or Unsplash.')
    return data_root() / 'credentials' / (provider + '.dpapi')


def get_key(provider):
    if provider not in ENV_KEYS: return ''
    environment = os.environ.get(ENV_KEYS[provider], '').strip()
    if environment: return environment
    try:
        return _crypt(credential_path(provider).read_bytes(), False).decode('utf-8')
    except (OSError, ValueError, UnicodeError):
        return ''


def save_key(provider, key):
    path = credential_path(provider)
    if not key:
        path.unlink(missing_ok=True)
        return
    if len(key) > 512 or any(ord(c) < 33 or ord(c) > 126 for c in key):
        raise ValueError('Enter a valid API key.')
    encrypted = _crypt(key.encode('utf-8'), True)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix('.tmp')
    temporary.write_bytes(encrypted)
    os.replace(temporary, path)
