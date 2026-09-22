# Build on Windows using Python 3.13 and requirements-build.txt.
from pathlib import Path
import importlib.metadata
import sys
from PyInstaller.utils.hooks import collect_all, copy_metadata

root = Path(SPECPATH).parent
backend = root / 'backend'
datas = [(str(backend / 'local_remove.html'), '.'),
         (str(backend / 'workflow.json'), '.'),
         (str(backend / 'licenses'), 'licenses'),
         (str(backend / 'LICENSE'), '.'),
         (str(backend / 'THIRD_PARTY_NOTICES.md'), '.')]
binaries = [(str(backend / 'tools' / 'texture-synthesis' / 'texture-synthesis.exe'), 'tools/texture-synthesis')]
hiddenimports = ['uvicorn.logging', 'uvicorn.protocols.http.h11_impl',
                 'uvicorn.protocols.websockets.websockets_impl', 'uvicorn.lifespan.on']
for package in ('imagecodecs',):
    collected_data, collected_binaries, collected_imports = collect_all(package)
    datas += collected_data
    binaries += collected_binaries
    hiddenimports += collected_imports
for distribution in importlib.metadata.distributions():
    datas += copy_metadata(distribution.metadata['Name'])
datas += [(str(Path(sys.base_prefix) / 'LICENSE.txt'), 'licenses/python')]
import cv2
for notice in ('LICENSE.txt', 'LICENSE-3RD-PARTY.txt'):
    path = Path(cv2.__file__).parent / notice
    if path.is_file():
        datas.append((str(path), 'licenses/opencv'))

a = Analysis([str(backend / 'run_local_remove.py')], pathex=[str(backend)],
             binaries=binaries, datas=datas, hiddenimports=hiddenimports,
             excludes=['tkinter', 'matplotlib', 'torch', 'pytest', 'IPython'])
pyz = PYZ(a.pure)
exe = EXE(pyz, a.scripts, [], exclude_binaries=True, name='LocalRemoveBackend',
          debug=False, strip=False, upx=False, console=False,
          icon=str(root / 'desktop' / 'icon' / 'local-remove.ico'))
coll = COLLECT(exe, a.binaries, a.datas, strip=False, upx=False, name='backend')
