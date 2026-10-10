"""Package 00 registration probe. Writes only target/psd23-workspace, never shared sources.
The shadow registers the already-merged package-21 manifest and package-23 hooks.
"""
from pathlib import Path
import shutil
root = Path.cwd()
shadow = root / 'target/psd23-workspace'
shadow.mkdir(parents=True, exist_ok=True)
for entry in root.iterdir():
    if entry.name in ('target', '.git', '.claude', 'crates', 'Cargo.toml', 'Cargo.lock'): continue
    target = shadow / entry.name
    if not target.exists(): target.symlink_to(entry)
for crate in (root/'crates').iterdir():
    if not crate.is_dir(): continue
    target = shadow/'crates'/crate.name
    target.mkdir(parents=True, exist_ok=True)
    for entry in crate.iterdir():
        dest = target/entry.name
        if entry.name == 'Cargo.toml': dest.write_text(entry.read_text())
        elif not dest.exists(): dest.symlink_to(entry)
(shadow/'Cargo.toml').write_text((root/'Cargo.toml').read_text().replace('[workspace.dependencies]', '[workspace.dependencies]\nphotocraft-pdf = { path = "crates/pc-pdf" }', 1))
shutil.copyfile(root/'Cargo.lock', shadow/'Cargo.lock')
pdf = shadow/'crates/pc-pdf'
(pdf/'Cargo.toml').write_text((root/'crates/pc-pdf/fixtures/export/Cargo.toml.fragment').read_text())
def private_dir(path):
    if path.is_symlink():
        origin = path.resolve(); path.unlink(); path.mkdir()
        for entry in origin.iterdir(): (path/entry.name).symlink_to(entry)
    else: path.mkdir(parents=True, exist_ok=True)
def write(relative, content):
    dest = shadow/relative
    for parent in reversed(list(dest.parents)):
        if parent == shadow: continue
        if shadow in parent.parents: private_dir(parent)
    if dest.is_symlink(): dest.unlink()
    dest.write_text(content)
write('crates/pc-pdf/src/lib.rs', '#![forbid(unsafe_code)]\npub mod export;\n')
p=shadow/'crates/pc-engine/Cargo.toml'
p.write_text(p.read_text().replace('[dependencies]', '[dependencies]\nphotocraft-pdf.workspace = true', 1))
write('crates/pc-engine/src/lib.rs', (root/'crates/pc-engine/src/lib.rs').read_text()+'\npub mod vector_export_warnings;\n')
# Exact importer fragment proposed in integration/23.md: advanced caches are raster fallback,
# while supported vector proxy blocks stay raw for unchanged re-export.
text=(root/'crates/pc-io/src/psd_import.rs').read_text()
text=text.replace('let content = if let Some(k) = adj_key {', '''let merged_vector_cache = rec.block(&crate::vector_map::fallback::CACHE_KEY)
            .is_some_and(|b| crate::vector_map::fallback::is_cache_block(&b.data));
        let content = if merged_vector_cache {
            LayerContent::Raster(self.record_surface(rec, &name))
        } else if let Some(k) = adj_key {''',1)
text=text.replace('if !matches!(l.content, LayerContent::Shape(_)) {','if !rec.block(&crate::vector_map::fallback::CACHE_KEY).is_some_and(|b| crate::vector_map::fallback::cache_has_shape_proxy(&b.data)) && !matches!(l.content, LayerContent::Shape(_)) {',1)
write('crates/pc-io/src/psd_import.rs', text)
# Make the normal PSD/PSB export perform the same preflight as the guarded engine boundary.
text=(root/'crates/pc-io/src/lib.rs').read_text()
text=text.replace('let (file, warnings) = document_to_psd_with(doc, &o);','let prepared = vector_map::fallback::prepare(doc, None)?;\n        let (file, warnings) = document_to_psd_with(&prepared, &o);',1)
write('crates/pc-io/src/lib.rs', text)
# Exact 00 compositor hook: retain pixel blending behavior after editability is lost.
# These are private copies; pc-io cannot depend on the compositor in the reverse direction.
kind_hook = r'''
/// Versioned PSD cache rendering role. No editable native feature is reconstructed.
pub fn psd_fallback_kind(layer: &Layer) -> Option<u8> {
    let (_, data) = layer.psd_blocks.iter().find(|(k, _)| k == b"liVF")?;
    const PREFIX: &[u8] = b"Local Image vector cache\0\0\0\0\x01\0";
    if data.len() != PREFIX.len() + 50 || !data.starts_with(PREFIX) || data[PREFIX.len()] > 2 { return None; }
    let kind = data[PREFIX.len()];
    let frame: [i32; 4] = std::array::from_fn(|k| {
        let i = PREFIX.len() + 34 + k * 4;
        i32::from_be_bytes([data[i], data[i+1], data[i+2], data[i+3]])
    });
    (data[PREFIX.len()+1] <= u8::from(kind == 1)
        && frame.iter().all(|v| v.unsigned_abs() <= 2_000_000)
        && frame[2] >= frame[0] && frame[3] >= frame[1]
        && (kind == 1 || frame == [0; 4])).then_some(kind)
}
/// Relative frame keeps ordinary gradient/pattern effects stable through Move.
pub fn psd_fallback_paint_bounds(layer: &Layer) -> Option<Rect> {
    if psd_fallback_kind(layer) != Some(1) { return None; }
    let (_, data) = layer.psd_blocks.iter().find(|(k, _)| k == b"liVF")?;
    let f: [i32; 4] = std::array::from_fn(|k| {
        let i = 64 + k * 4;
        i32::from_be_bytes([data[i], data[i+1], data[i+2], data[i+3]])
    });
    if f == [0; 4] { return None; }
    let b = layer.surface()?.content_bounds();
    Some(Rect::new(b.x0.checked_add(f[0])?, b.y0.checked_add(f[1])?, b.x0.checked_add(f[2])?, b.y0.checked_add(f[3])?))
}
/// Cached shape pixels retain the native shape's ordinary stroke-effect behavior.
pub fn vector_effects(layer: &Layer) -> bool {
    matches!(layer.content, LayerContent::Shape(_)) || psd_fallback_kind(layer) == Some(1)
}
'''
text=(root/'crates/pc-compose/src/lib.rs').read_text()
text=text.replace('if matches!(layer.content, LayerContent::Text(_)) { psblend::text_gamma() } else { 1.0 }',
                  'if matches!(layer.content, LayerContent::Text(_)) || psd_fallback_kind(layer) == Some(2) { psblend::text_gamma() } else { 1.0 }', 1)
text=text.replace('let LayerContent::Shape(sh) = &layer.content else { return None };\n    let (x0, y0, x1, y1)',
                  'let LayerContent::Shape(sh) = &layer.content else { return psd_fallback_paint_bounds(layer) };\n    let (x0, y0, x1, y1)', 1)
write('crates/pc-compose/src/lib.rs', text+kind_hook)
text=(root/'crates/pc-compose/src/effects.rs').read_text().replace(
    'matches!(layer.content, photocraft_doc::LayerContent::Shape(_))', 'crate::vector_effects(layer)')
write('crates/pc-compose/src/effects.rs', text)
text=(root/'crates/pc-gpu/src/lib.rs').read_text().replace(
    'matches!(f.layer.content, LayerContent::Shape(_)) && photocraft_compose::effect_outline(f.layer).is_none()',
    'photocraft_compose::vector_effects(f.layer) && photocraft_compose::effect_outline(f.layer).is_none()').replace(
    'matches!(layer.content, LayerContent::Shape(_)) && photocraft_compose::effect_outline(layer).is_none()',
    'photocraft_compose::vector_effects(layer) && photocraft_compose::effect_outline(layer).is_none()')
write('crates/pc-gpu/src/lib.rs', text)
text=(root/'crates/pc-gpu/src/plan.rs').read_text().replace(
    'matches!(layer.content, LayerContent::Shape(_)) && !outline',
    'photocraft_compose::vector_effects(layer) && !outline', 1)
write('crates/pc-gpu/src/plan.rs', text)
# Own tests remain live via symlinks. No shared aggregate is edited in the actual tree.
write('crates/pc-io/tests/vector_fallback_probe.rs', '#[path="vector_fallback/roundtrip.rs"]\nmod roundtrip;\n')
p=shadow/'crates/pc-io/Cargo.toml'
p.write_text(p.read_text()+'\n[[test]]\nname="vector_fallback_probe"\npath="tests/vector_fallback_probe.rs"\n[[test]]\nname="it"\npath="tests/it/main.rs"\n[[test]]\nname="coverage"\npath="tests/coverage/main.rs"\n')
write('crates/pc-ui-egui/tests/vector_fallback_probe.rs', '''static GPU_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn gpu_lock() -> std::sync::MutexGuard<'static, ()> { GPU_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner) }
#[path="../../pc-io/tests/vector_fallback/app_gpu.rs"]
mod app_gpu;
''')
p=shadow/'crates/pc-ui-egui/Cargo.toml'
p.write_text(p.read_text()+'\n[[test]]\nname="vector_fallback_probe"\npath="tests/vector_fallback_probe.rs"\n')
print(shadow)
