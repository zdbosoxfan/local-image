"""Reproducible temporary coordinator wiring; never edits real shared files.
Run from the worktree root: python3 crates/pc-svg/fixtures/export/bootstrap.py.
All registry packages are already locked. Package 00 owns the real manifests/router.
"""
from pathlib import Path
import shutil
import os
import hashlib
root = Path.cwd()
shadow = root / 'target/svg-export-workspace'
shadow.mkdir(parents=True, exist_ok=True)
for entry in root.iterdir():
    if entry.name in ('target', '.git', '.claude', 'Cargo.toml', 'Cargo.lock', 'crates'):
        continue
    dest = shadow / entry.name
    if not dest.exists(): dest.symlink_to(entry)
for crate in (root / 'crates').iterdir():
    if not crate.is_dir(): continue
    dest = shadow / 'crates' / crate.name
    dest.mkdir(parents=True, exist_ok=True)
    if crate.name == 'pc-svg': continue
    for entry in crate.iterdir():
        to = dest / entry.name
        if entry.name == 'Cargo.toml':
            to.write_text(entry.read_text())
        elif not to.exists(): to.symlink_to(entry)
    if crate.name in ('pc-engine', 'pc-ui-egui'):
        p = dest / 'Cargo.toml'
        p.write_text(p.read_text().replace('[dependencies]', '[dependencies]\nphotocraft-svg.workspace = true', 1))
# SVG scaffold is an exact coordinator fragment, not a second implementation.
p = shadow / 'crates/pc-svg'
p.mkdir(parents=True, exist_ok=True)
for name in ('src', 'tests', 'fixtures'):
    if not (p/name).exists(): (p/name).symlink_to(root/'crates/pc-svg'/name)
(p/'Cargo.toml').write_text('''[package]
name = "photocraft-svg"
version.workspace = true
edition.workspace = true
license.workspace = true
[lints]
workspace = true
[dependencies]
photocraft-doc.workspace = true
photocraft-geom.workspace = true
photocraft-vector.workspace = true
photocraft-pathops.workspace = true
photocraft-text.workspace = true
photocraft-compose.workspace = true
image.workspace = true
kurbo.workspace = true
base64 = "=0.22.1"
serde.workspace = true
thiserror.workspace = true
serde_json.workspace = true
[dev-dependencies]
resvg = { version = "=0.45.1", default-features = false }
roxmltree = "=0.20.0"
photocraft-testkit.workspace = true
proptest.workspace = true
serde_json.workspace = true
[features]
svg-import-integrated = []
[[test]]
name = "export"
path = "tests/export/main.rs"
''')
# lib.rs belongs to 00. Keep it in the temporary package, with absolute export source.
if (p/'src').is_symlink():
    (p/'src').unlink(); (p/'src').mkdir()
(p/'src/lib.rs').write_text('#![forbid(unsafe_code)]\n#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]\n#[path = '+repr(str(root/'crates/pc-svg/src/export.rs')).replace("'",'"')+']\npub mod export;\n')
# Optional read-only package-13 implementation for actual dependency gates.
import_source = os.environ.get('SVG_EXPORT_IMPORT_SOURCE')
if import_source:
    source = Path(import_source).resolve()
    # A sibling builder may still be working. Freeze actual public source bytes for
    # this probe rather than compiling a mixture of revisions during a long build.
    snapshot = shadow/'svg-import-source/src'
    snapshot.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, snapshot/'import.rs')
    shutil.copytree(source.parent/'import', snapshot/'import', dirs_exist_ok=True)
    digest = hashlib.sha256()
    for item in [snapshot/'import.rs', *sorted((snapshot/'import').glob('*.rs'))]:
        digest.update(str(item.relative_to(snapshot)).encode()+b'\0'+item.read_bytes()+b'\0')
    (snapshot.parent/'SHA256').write_text(digest.hexdigest()+'\n')
    # Treat package 13 as a dependency in isolated package-14 linting. Its source
    # and public API are unchanged; integrated shared-crate linting remains a 00/13 gate.
    (snapshot/'lib.rs').write_text('#![forbid(unsafe_code)]\npub mod import;\n')
    (snapshot.parent/'Cargo.toml').write_text('''[package]
name = "photocraft-svg-import-probe"
version.workspace = true
edition.workspace = true
license.workspace = true
[lints]
workspace = true
[dependencies]
photocraft-doc.workspace = true
photocraft-geom.workspace = true
photocraft-vector.workspace = true
photocraft-pathops.workspace = true
photocraft-text.workspace = true
photocraft-compose.workspace = true
image.workspace = true
flate2.workspace = true
thiserror.workspace = true
usvg = { version = "=0.45.1", default-features = false }
svgtypes = "=0.15.3"
roxmltree = "=0.20.0"
simplecss = "=0.2.2"
''')
    with (p/'src/lib.rs').open('a') as out:
        out.write('pub use photocraft_svg_import_probe::import;\n')
    manifest = p/'Cargo.toml'
    manifest.write_text(manifest.read_text().replace('[dependencies]', '[dependencies]\nphotocraft-svg-import-probe = { path = "../../svg-import-source" }',1))
# Engine gate feature never compiles a substitute importer.
e = shadow/'crates/pc-engine/Cargo.toml'
e.write_text(e.read_text().replace('[features]', '[features]\nsvg-import-integrated = ["photocraft-svg/svg-import-integrated"]',1))
# Standalone temporary registration of the owned real-app GPU module.
u_manifest = shadow/'crates/pc-ui-egui/Cargo.toml'
u_manifest.write_text(u_manifest.read_text()+ '\n[features]\nsvg-import-integrated = ["photocraft-svg/svg-import-integrated"]\n[[test]]\nname = "svg_export_gpu"\npath = "svg_export_gpu.rs"\nrequired-features = ["svg-import-integrated"]\n')
(shadow/'crates/pc-ui-egui/svg_export_gpu.rs').write_text('static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());\nfn gpu_lock() -> std::sync::MutexGuard<\'static, ()> {\n    LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)\n}\n#[path = "'+str(root/'crates/pc-svg/tests/export/app_gpu.rs')+'"]\nmod app;\n')
# UI registration only; engine already has a compiled package registration slot.
u = shadow/'crates/pc-ui-egui/src'
if u.is_symlink():
    u.unlink(); u.mkdir()
    for entry in (root/'crates/pc-ui-egui/src').iterdir():
        if entry.name!='lib.rs': (u/entry.name).symlink_to(entry)
(u/'lib.rs').write_text((root/'crates/pc-ui-egui/src/lib.rs').read_text()+'\npub mod vector_packages {\n    pub mod svg_export;\n}\n')
cargo = (root/'Cargo.toml').read_text().replace('[workspace.dependencies]', '[workspace.dependencies]\nphotocraft-svg = { path = "crates/pc-svg" }',1)
(shadow/'Cargo.toml').write_text(cargo)
shutil.copyfile(root/'Cargo.lock',shadow/'Cargo.lock')
print(shadow)
