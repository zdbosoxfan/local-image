from pathlib import Path
import json
import zipfile

HERE = Path(__file__).resolve().parent
OUT = HERE.parent.parent/'outputs'
archive = OUT/'Local Remove source.zip'
with zipfile.ZipFile(archive) as old:
    files = {name:old.read(name) for name in old.namelist() if not name.endswith('/')}
for name in ('local_remove.py', 'local_remove.html', 'local_remove_project.py', 'LocalRemoveLauncher.cs',
             'LocalRemove.manifest', 'Build-NativeHost.ps1', 'NativeHost-README.md', 'Install-Project-Update.ps1'):
    files[name] = (HERE/name).read_bytes()
for path in (HERE/'native-host').glob('*'):
    if path.is_file():
        files['native-host/'+path.name] = path.read_bytes()
for name in ('local_remove.py', 'local_remove.html', 'local_remove_project.py', 'test_layer_projects.py',
             'test_ui_navigation.cjs', 'test_ui_projects_layers.cjs'):
    files['tests/local-remove-v5/'+name] = (HERE/name).read_bytes()
files['Local Remove - quick guide.md'] = (OUT/'Local Remove - quick guide.md').read_bytes()
files['README.txt'] = (
    'Local Remove standalone desktop update\n\n'
    'Source snapshot and compiled native host for the existing installation on this PC; not a portable installer.\n'
    'Current update script: Install-Project-Update.ps1. Earlier scripts are historical.\n'
    'This update adds cached immediate layer previews, portable .lremove projects, and explicit save/discard/cancel '
    'review before closing an image or the native app. Project files contain the original and completed layers, '
    'including hidden/discarded edits and16-bit merged snapshots. Pending selections/view are not saved. '
    'PNG/JPEG/TIFF/WebP exports remain flattened. Folder navigation retains working edits.\n'
    'Existing GPU removal, model-free texture repair, brush/zoom/pan tools, folder workflows, overwrite confirmation, '
    'output formats, merged layers, and custom icon remain available.\n'
    'Read Local Remove - quick guide.md, NativeHost-README.md, and THIRD_PARTY_NOTICES.md.\n'
    'Sources and regression tests included; no model weights, personal photos, sessions, projects, keys, or browser profiles. '
    'Tests expect this PC\'s installed Python/backend dependencies.\n'
).encode()
temporary = OUT/'Local Remove source.new.zip'
with zipfile.ZipFile(temporary, 'w', zipfile.ZIP_DEFLATED) as new:
    for name, data in files.items():
        new.writestr(name, data)
temporary.replace(archive)
print(json.dumps({'files':len(files), 'bytes':archive.stat().st_size}))
