"""Read Windows icon resources from the compiled EXE without running its UI."""
import ctypes
from ctypes import wintypes
import hashlib
import json
from pathlib import Path
import struct

root = Path(__file__).resolve().parents[1]
exe = root / 'native-host' / 'Local Remove.exe'
kernel = ctypes.WinDLL('kernel32', use_last_error=True)
kernel.LoadLibraryExW.argtypes = [wintypes.LPCWSTR, wintypes.HANDLE, wintypes.DWORD]
kernel.LoadLibraryExW.restype = wintypes.HMODULE
kernel.FindResourceW.argtypes = [wintypes.HMODULE, ctypes.c_void_p, ctypes.c_void_p]
kernel.FindResourceW.restype = ctypes.c_void_p
kernel.SizeofResource.argtypes = [wintypes.HMODULE, ctypes.c_void_p]
kernel.SizeofResource.restype = wintypes.DWORD
kernel.LoadResource.argtypes = [wintypes.HMODULE, ctypes.c_void_p]
kernel.LoadResource.restype = ctypes.c_void_p
kernel.LockResource.argtypes = [ctypes.c_void_p]
kernel.LockResource.restype = ctypes.c_void_p
kernel.FreeLibrary.argtypes = [wintypes.HMODULE]
CALLBACK = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HMODULE, ctypes.c_void_p, ctypes.c_void_p, wintypes.LPARAM)
kernel.EnumResourceNamesW.argtypes = [wintypes.HMODULE, ctypes.c_void_p, CALLBACK, wintypes.LPARAM]
kernel.EnumResourceNamesW.restype = wintypes.BOOL
module = kernel.LoadLibraryExW(str(exe), None, 2)  # LOAD_LIBRARY_AS_DATAFILE
if not module:
    raise ctypes.WinError(ctypes.get_last_error())
groups = []

def resource_bytes(kind, name):
    resource = kernel.FindResourceW(module, name, kind)
    assert resource, f'Missing resource: {kind}/{name}'
    return ctypes.string_at(kernel.LockResource(kernel.LoadResource(module, resource)), kernel.SizeofResource(module, resource))

@CALLBACK
def collect(module_handle, resource_type, name, parameter):
    groups.append(resource_bytes(14, name))
    return True

try:
    assert kernel.EnumResourceNamesW(module, 14, collect, 0)
    assert len(groups) == 1
    raw = groups[0]
    reserved, kind, count = struct.unpack_from('<HHH', raw)
    assert reserved == 0 and kind == 1 and count == 10
    sizes = []
    for index in range(count):
        width, height, colors, reserved, planes, bits, byte_size, resource_id = struct.unpack_from('<BBBBHHIH', raw, 6 + 14 * index)
        assert width == height and bits == 32
        assert len(resource_bytes(3, resource_id)) == byte_size
        sizes.append(width or 256)
    assert sorted(sizes) == [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
finally:
    kernel.FreeLibrary(module)

result = {
    'ok': True,
    'embedded_icon_sizes': sorted(sizes),
    'bit_depth': 32,
    'executable_sha256': hashlib.sha256(exe.read_bytes()).hexdigest(),
    'ico_sha256': hashlib.sha256((root / 'icon' / 'local-remove.ico').read_bytes()).hexdigest(),
    'source_png': str(root / 'icon' / 'local-remove-icon.png'),
    'extracted_icon_preview': str(root / 'icon' / 'embedded-icon-preview.png'),
}
(root / 'icon-verification.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
print(json.dumps(result))
