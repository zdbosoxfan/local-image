#!/usr/bin/env python3
"""Rehearse package-00 registration under target only; shared source stays untouched.
The Pen activation sentinel is temporary until 01 registers the eight tool IDs.
No GPU test or build is run by this script. Emit an exact shared-file patch for 00.
"""
from pathlib import Path
import shutil
import re
import difflib
root = Path(__file__).resolve().parents[4]
out = root / 'target/constructive-registration'
ui = out / 'crates/pc-ui-egui'
out.mkdir(parents=True, exist_ok=True)
manifest = (root / 'Cargo.toml').read_text()
manifest = re.sub(r'members = .*', 'members = ["crates/pc-engine", "crates/pc-ui-egui"]', manifest, count=1)
manifest = re.sub(r'default-members = .*', 'default-members = ["crates/pc-ui-egui"]', manifest, count=1)
manifest = re.sub(r'path = "(crates/[^\"]+)"', lambda m: 'path = "' + str(root / m[1]) + '"', manifest)
for package in ('pc-engine', 'pc-trace'):
    manifest = manifest.replace('path = "' + str(root / 'crates' / package) + '"',
                                'path = "' + str(out / 'crates' / package) + '"')
(out / 'Cargo.toml').write_text(manifest)
shutil.copy2(root / 'Cargo.lock', out / 'Cargo.lock')
for folder in ('assets', 'docs', 'licenses', 'contributors', 'tests'):
    link = out / folder
    if not link.exists():
        link.symlink_to(root / folder, target_is_directory=True)
for folder in (p.name for p in (root / 'crates').iterdir() if p.is_dir() and p.name not in ('pc-engine', 'pc-ui-egui', 'pc-trace')):
    link = out / 'crates' / folder
    link.parent.mkdir(exist_ok=True)
    if not link.exists():
        link.symlink_to(root / 'crates' / folder, target_is_directory=True)
shutil.copytree(root / 'crates/pc-engine', out / 'crates/pc-engine', dirs_exist_ok=True)
shutil.copytree(root / 'crates/pc-trace', out / 'crates/pc-trace', dirs_exist_ok=True)
trace_manifest = out / 'crates/pc-trace/Cargo.toml'
trace_manifest.write_text(re.sub(r'path = "../([^\"]+)"',
                                lambda m: 'path = "' + str(root / 'crates' / m[1]) + '"',
                                trace_manifest.read_text()))
# Baseline 25/27 integration omission, fixed only in the disposable build tree.
trace = out / 'crates/pc-trace/src/cutout/compose.rs'
text = trace.read_text()
needle = 'Ok(Trace { width: map.width, height: map.height, layers, nodes })'
if needle in text:
    trace.write_text(text.replace(needle, 'Ok(Trace { width: map.width, height: map.height, layers, nodes, strokes: Vec::new() })', 1))
shutil.copytree(root / 'crates/pc-ui-egui', ui, dirs_exist_ok=True)
(ui / 'build.rs').touch()
patch = []
def edit(relative, replacements):
    path = ui / relative
    before = path.read_text()
    after = before
    for needle, replacement in replacements:
        if after.count(needle) != 1:
            raise RuntimeError(f'{relative}: registration anchor absent/ambiguous: {needle!r}')
        after = after.replace(needle, replacement, 1)
    path.write_text(after)
    patch.extend(difflib.unified_diff(before.splitlines(True), after.splitlines(True),
                                     fromfile='a/crates/pc-ui-egui/' + relative,
                                     tofile='b/crates/pc-ui-egui/' + relative))
edit('src/lib.rs', [
    ('pub struct PhotocraftApp {', '#[path = "vector_packages/constructive.rs"]\npub mod constructive;\npub struct PhotocraftApp {\n    pub constructive_state: constructive::State,\n    pub constructive_enabled: bool,'),
    ('            move_preview: None,', '            constructive_state: Default::default(),\n            constructive_enabled: false,\n            move_preview: None,'),
    ('        let t0 = gpu_canvas::now_ms();\n        // View', '''        if self.constructive_enabled {
            let active = self.ui.tool == state::Tool::Pen;
            let ids = self.session.active().map_or_else(Vec::new, |st| st.selected_layers());
            self.constructive_state.set_targets(ids);
            self.constructive_state.frame(&self.session, &ctx, active);
            if !active { self.constructive_enabled = false; }
        }
        let t0 = gpu_canvas::now_ms();
        // View'''),
])
edit('src/canvas.rs', [
    ('fn display_doc(app: &mut PhotocraftApp, idx: usize) -> (std::sync::Arc<Document>, u64) {', '''fn display_doc(app: &mut PhotocraftApp, idx: usize) -> (std::sync::Arc<Document>, u64) {
    if app.constructive_enabled && app.session.active_index() == Some(idx)
        && let Some(shown) = app.constructive_state.display_document(&app.session) { return shown; }'''),
    ('        if move_input {\n            buttons', '''        if app.constructive_enabled {
            app.constructive_state.canvas_input(&mut app.session, &response, &xf);
            buttons = crate::paint_mouse::Buttons::default();
        }
        if move_input {
            buttons'''),
    ('        crate::vector_ui::draw_overlay(app, &painter, &xf, &doc);', '''        if app.constructive_enabled {
            if app.ui.view.extras {
                app.constructive_state.draw_overlay(&painter, &|p| xf.to_screen(p.x as f32, p.y as f32), xf.zoom as f64);
            }
        } else { crate::vector_ui::draw_overlay(app, &painter, &xf, &doc); }'''),
    ('pub fn tool_event(app: &mut PhotocraftApp, ev: ToolEvent, mods: egui::Modifiers) {', '''pub fn tool_event(app: &mut PhotocraftApp, ev: ToolEvent, mods: egui::Modifiers) {
    if app.constructive_enabled {
        let zoom = app.current_zoom() as f64;
        app.constructive_state.pointer(&mut app.session, ev, mods, zoom);
        return;
    }'''),
])
edit('src/panels.rs', [
    ('                // The tool\'s icon opens', '''                if app.constructive_enabled {
                    app.constructive_state.context_bar(ui, &mut app.session);
                    return;
                }
                // The tool's icon opens'''),
])
edit('tests/gpu/main.rs', [
    ('mod vector_integration;', 'mod vector_integration;\n#[path = "../../src/vector_packages/constructive/app_gpu.rs"]\nmod constructive_app_gpu;'),
])
(out / 'registration.patch').write_text(''.join(patch))
print(out)
