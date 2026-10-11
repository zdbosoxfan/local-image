"""Reproducible package-00 registration probe. Never writes real shared files.
Run from repository root. No registry dependencies beyond the approved locked graph.
"""
from pathlib import Path
import shutil
import re
import subprocess
root = Path.cwd()
shadow = root / 'target/pdf-export-workspace'
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
    if crate.name == 'pc-pdf': continue
    for entry in crate.iterdir():
        to = dest / entry.name
        if entry.name == 'Cargo.toml':
            to.write_text(entry.read_text())
        elif not to.exists(): to.symlink_to(entry)
    if crate.name in ('pc-engine', 'pc-ui-egui'):
        p = dest / 'Cargo.toml'
        p.write_text(p.read_text().replace('[dependencies]', '[dependencies]\nphotocraft-pdf.workspace = true', 1))
p = shadow / 'crates/pc-pdf'
p.mkdir(parents=True, exist_ok=True)
for name in ('tests', 'fixtures'):
    if not (p/name).exists(): (p/name).symlink_to(root/'crates/pc-pdf'/name)
(p/'Cargo.toml').write_text((root/'crates/pc-pdf/fixtures/export/Cargo.toml.fragment').read_text())
(p/'src').mkdir(exist_ok=True)
(p/'src/lib.rs').write_text('#![forbid(unsafe_code)]\n#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]\n#[path = "'+str(root/'crates/pc-pdf/src/export.rs')+'"]\npub mod export;\n')
# UI modules are not yet registered in the real app. Copy lib only, link the real source.
u = shadow/'crates/pc-ui-egui/src'
if u.is_symlink():
    u.unlink(); u.mkdir()
    for entry in (root/'crates/pc-ui-egui/src').iterdir():
        if entry.name!='lib.rs': (u/entry.name).symlink_to(entry)
(u/'lib.rs').write_text((root/'crates/pc-ui-egui/src/lib.rs').read_text()+'\npub mod vector_packages {\n    pub mod pdf_export;\n}\n')
u_manifest=shadow/'crates/pc-ui-egui/Cargo.toml'
u_manifest.write_text(u_manifest.read_text()+'\n[features]\npdf-import-integrated=[]\n'+'\n[[test]]\nname="pdf_export_gpu"\npath="pdf_export_gpu.rs"\n')
(shadow/'crates/pc-ui-egui/pdf_export_gpu.rs').write_text('static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());\nfn gpu_lock() -> std::sync::MutexGuard<\'static, ()> {LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)}\n#[path="'+str(root/'crates/pc-pdf/tests/export/app_gpu.rs')+'"]\nmod app;\n')
(shadow/'Cargo.toml').write_text((root/'Cargo.toml').read_text().replace('[workspace.dependencies]', '[workspace.dependencies]\nphotocraft-pdf = { path = "crates/pc-pdf" }', 1))
shutil.copyfile(root/'Cargo.lock',shadow/'Cargo.lock')
# Package 15 source is merged but its model activation belongs to 00 and is pending.
# Apply its exact integration fragment exclusively to copies in this probe.
symbol_note=(root/'docs/wip/vector-tasks/integration/15.md').read_text()
activation=symbol_note.split('```diff\n',1)[1].split('```',1)[0]
paths=re.findall(r'^\+\+\+ b/(.+)$',activation,re.M)
for relative in paths:
    source=root/relative
    target=shadow/relative
    # Freeze each symlinked source directory before patching, never follow a symlink
    # into the real worktree with a mutating patch command.
    parents=list(target.parents)
    for parent in reversed(parents):
        if parent==shadow: continue
        if parent.is_symlink():
            src=parent.resolve()
            parent.unlink()
            shutil.copytree(src,parent,symlinks=False)
    if target.is_symlink(): target.unlink()
    target.write_bytes(source.read_bytes())
patch=shadow/'symbol-activation.patch'
patch.write_text(activation)
subprocess.run(['patch','-p1','--batch','--forward','-i',str(patch)],cwd=shadow,check=True)
# Keep package-owned WIP live in the copied source directories.
for relative in ['crates/pc-engine/src/vector_packages/pdf_export.rs','crates/pc-engine/src/vector_packages/pdf_export','crates/pc-ui-egui/src/vector_packages/pdf_export.rs']:
    target=shadow/relative
    for parent in reversed(list(target.parents)):
        if parent==shadow: continue
        if parent.is_symlink():
            src=parent.resolve();parent.unlink();shutil.copytree(src,parent,symlinks=False)
    if target.is_symlink(): target.unlink()
    elif target.is_dir(): shutil.rmtree(target)
    elif target.exists(): target.unlink()
    target.symlink_to(root/relative)
# Exact package-21 modal/menu activation, on private copies after the dependency patch.
def edit(relative, old, new):
    file=shadow/relative
    if file.is_symlink():
        content=file.read_text();file.unlink();file.write_text(content)
    content=file.read_text()
    if old not in content: raise RuntimeError('integration anchor missing: '+relative)
    file.write_text(content.replace(old,new,1))
edit('crates/pc-ui-egui/src/state.rs','    pub next_id: u64,','    #[serde(skip)]\n    pub vector_pdf_export: crate::vector_packages::pdf_export::State,\n    pub next_id: u64,')
edit('crates/pc-ui-egui/src/state.rs','            next_id:','            vector_pdf_export: Default::default(),\n            next_id:')
edit('crates/pc-ui-egui/src/lib.rs','        dialogs::show(self, &ctx);','        dialogs::show(self, &ctx);\n        crate::vector_packages::pdf_export::frame(self, &ctx);')
edit('crates/pc-ui-egui/src/menus.rs','    ("file.export.exportAs",','    ("file.export.pdf", "PDF…", &["File", "Export"], None),\n    ("file.export.exportAs",')
edit('crates/pc-ui-egui/src/menus.rs','        "file.export.exportAs" =>','        "file.export.pdf" => {crate::vector_packages::pdf_export::open_dialog(app)?; Ok(json!({"dialog":"pdf"}))},\n        "file.export.exportAs" =>')
edit('crates/pc-ui-egui/src/menus.rs','        "file.save" | "file.saveAs" | "file.export.exportAs"','        "file.export.pdf" => app.session.active().is_some() && app.services.pick_save.is_some() && app.services.write.is_some(),\n        "file.save" | "file.saveAs" | "file.export.exportAs"')
edit('crates/pc-ui-egui/src/menu_catalog.rs','    (&["File", "Export"], "Export As…",','    (&["File", "Export"], "PDF…", None, "file.export.pdf"),\n    (&["File", "Export"], "Export As…",')
# lib.rs is reset while applying 15; register 21 after that reset.
with (shadow/'crates/pc-ui-egui/src/lib.rs').open('a') as out:
    out.write('\npub mod vector_packages { pub mod pdf_export; }\n')
# 09's current scaled projection predates the symbol variant. Refuse its missing
# provider explicitly in this temporary activation; never use a stale symbol cache.
scaled=shadow/'crates/pc-compose/src/scaled_view.rs'
if scaled.is_symlink():
    content=(root/'crates/pc-compose/src/scaled_view.rs').read_text();scaled.unlink();scaled.write_text(content)
content=(root/'crates/pc-compose/src/scaled_view.rs').read_text()
content=content.replace('LayerContent::Adjustment(a) => LayerContent::Adjustment(a.clone()),','LayerContent::Adjustment(a) => LayerContent::Adjustment(a.clone()),\n                    LayerContent::Symbol(_) => return Err(ViewError("symbol projection provider required".into())),')
scaled.write_text(content)
# Keep formatting reproducible without rustfmt following module bridges into
# unowned sources. These manifests are formatter inputs, never build stand-ins.
format_root = root / 'target/pdf-export-format'
format_root.mkdir(exist_ok=True)
(format_root/'Cargo.toml').write_text('[workspace]\nresolver="3"\nmembers=["pc-pdf","pc-engine","pc-ui-egui"]\n')
for name, source in [
    ('pc-pdf', 'crates/pc-pdf/src/export.rs'),
    ('pc-engine', 'crates/pc-engine/src/vector_packages/pdf_export.rs'),
    ('pc-ui-egui', 'crates/pc-ui-egui/src/vector_packages/pdf_export.rs'),
]:
    crate = format_root/name
    crate.mkdir(exist_ok=True)
    (crate/'Cargo.toml').write_text('[package]\nname="photocraft-'+name[3:]+'"\nversion="0.0.0"\nedition="2024"\n[lib]\npath="'+str(root/source)+'"\n')
print(shadow)
