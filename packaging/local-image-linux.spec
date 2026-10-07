"""Two persistent onedir runtimes: Qt desktop host and local editor server."""
from pathlib import Path
import importlib.metadata
import os
import sys
from PyInstaller.utils.hooks import collect_all, copy_metadata

root = Path(SPECPATH).parent
backend = root / 'backend'
sys.path.insert(0, str(backend))
from local_remove_frontend import frontend_manifest

frontend_manifest()
if not (backend / 'frontend_dist' / 'THIRD_PARTY_NOTICES.txt').is_file():
    raise RuntimeError('Build the React frontend and license notices before packaging.')
helper = Path(os.environ['LOCAL_IMAGE_TEXTURE_HELPER'])
if not helper.is_file():
    raise RuntimeError('The verified Linux Texture helper is missing.')

metadata = []
for distribution in importlib.metadata.distributions():
    metadata += copy_metadata(distribution.metadata['Name'])

datas = [
    (str(backend / 'frontend' / 'react.html'), 'frontend'),
    (str(backend / 'frontend' / 'app-icon.png'), 'frontend'),
    (str(backend / 'frontend' / 'lora-examples'), 'frontend/lora-examples'),
    (str(backend / 'frontend_dist'), 'frontend_dist'),
    (str(backend / 'workflow.json'), '.'),
    (str(backend / 'licenses'), 'licenses'),
    (str(backend / 'LICENSE'), '.'),
    (str(backend / 'THIRD_PARTY_NOTICES.md'), '.'),
]
binaries = [(str(helper), 'tools/texture-synthesis')]
hiddenimports = ['uvicorn.logging', 'uvicorn.protocols.http.h11_impl',
                 'uvicorn.protocols.websockets.websockets_impl', 'uvicorn.lifespan.on']
for package in ('imagecodecs', 'py7zr', 'Cryptodome'):
    package_data, package_binaries, package_imports = collect_all(package)
    datas += package_data
    binaries += package_binaries
    hiddenimports += package_imports

import cv2
for notice in ('LICENSE.txt', 'LICENSE-3RD-PARTY.txt'):
    path = Path(cv2.__file__).parent / notice
    if path.is_file():
        datas.append((str(path), 'licenses/opencv'))

server = Analysis([str(backend / 'run_local_image.py')], pathex=[str(backend)],
    binaries=binaries, datas=datas + metadata, hiddenimports=hiddenimports,
    excludes=['tkinter', 'matplotlib', 'torch', 'pytest', 'IPython', 'PySide6', 'PyQt6', 'PyQt5'])
server_pyz = PYZ(server.pure)
server_exe = EXE(server_pyz, server.scripts, [], exclude_binaries=True,
    name='LocalImageBackend', debug=False, strip=False, upx=False, console=True)
server_bundle = COLLECT(server_exe, server.binaries, server.datas,
    strip=False, upx=False, name='backend')

desktop = Analysis([str(root / 'desktop' / 'linux' / 'local_image.py')],
    pathex=[str(root / 'desktop' / 'linux'), str(backend)],
    binaries=[], datas=[(str(backend / 'frontend' / 'app-icon.png'), '.')] + metadata,
    hiddenimports=[], excludes=['tkinter', 'matplotlib', 'torch', 'pytest', 'IPython', 'PyQt6', 'PyQt5',
                                'PySide6.QtWaylandCompositor'])
desktop_pyz = PYZ(desktop.pure)
desktop_exe = EXE(desktop_pyz, desktop.scripts, [], exclude_binaries=True,
    name='local-image', debug=False, strip=False, upx=False, console=True)
desktop_bundle = COLLECT(desktop_exe, desktop.binaries, desktop.datas,
    strip=False, upx=False, name='desktop')
