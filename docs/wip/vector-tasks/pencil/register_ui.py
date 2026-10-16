#!/usr/bin/env python3
"""Reproducible package-00 registration rehearsal, exclusively under target/.
This does not edit shared repository source. The app still uses Pen as an activation
sentinel until package 01 provides Tool::VectorPencil. Never call this final persona acceptance.
"""
from pathlib import Path
import shutil
import re
root = Path(__file__).resolve().parents[4]
out = root / 'target/pencil-registration'
out.mkdir(exist_ok=True)
manifest = (root / 'Cargo.toml').read_text()
manifest = re.sub(r'members = .*', 'members = ["crates/pc-ui-egui"]', manifest, count=1)
manifest = re.sub(r'default-members = .*', 'default-members = ["crates/pc-ui-egui"]', manifest, count=1)
manifest = re.sub(r'path = "(crates/[^\"]+)"', lambda m: 'path = "' + str(root / m[1]) + '"', manifest)
(out / 'Cargo.toml').write_text(manifest)
shutil.copy2(root / 'Cargo.lock', out / 'Cargo.lock')
for folder in ('assets','docs','licenses','contributors'):
    link = out / folder
    if not link.exists(): link.symlink_to(root / folder, target_is_directory=True)
for folder in ('lc-ui-egui', 'pc-plugins'):
    sibling = out / 'crates' / folder
    sibling.parent.mkdir(exist_ok=True)
    if not sibling.exists(): sibling.symlink_to(root / 'crates' / folder, target_is_directory=True)
ui = out / 'crates/pc-ui-egui'
shutil.copytree(root / 'crates/pc-ui-egui', ui, dirs_exist_ok=True)
# Rebuild static credits after the read-only contributor links become available.
(ui / 'build.rs').touch()
p = ui / 'src/lib.rs'
s = p.read_text().replace('pub struct PhotocraftApp {', 'pub mod vector_packages { pub mod pencil; }\npub struct PhotocraftApp {\n    pub vector_pencil: vector_packages::pencil::State,\n    pub vector_pencil_enabled: bool,',1)
s = s.replace('            move_preview: None,', '            vector_pencil: Default::default(),\n            vector_pencil_enabled: false,\n            move_preview: None,',1)
s = s.replace('        let t0 = gpu_canvas::now_ms();\n        // View', '''        if self.vector_pencil_enabled {
            let selected = self.ui.tool == state::Tool::Pen;
            self.vector_pencil.frame(&self.session, &ctx, selected);
            if !selected { self.vector_pencil_enabled = false; }
        }
        let t0 = gpu_canvas::now_ms();
        // View''',1)
p.write_text(s)
p = ui / 'src/canvas.rs'
s = p.read_text().replace('fn display_doc(app: &mut PhotocraftApp, idx: usize) -> (std::sync::Arc<Document>, u64) {','''fn display_doc(app: &mut PhotocraftApp, idx: usize) -> (std::sync::Arc<Document>, u64) {
    if app.vector_pencil_enabled && app.session.active_index() == Some(idx)
        && let Some(shown) = app.vector_pencil.display_doc(&app.session) { return shown; }''',1)
s = s.replace('pub fn tool_event(app: &mut PhotocraftApp, ev: ToolEvent, mods: egui::Modifiers) {','''pub fn tool_event(app: &mut PhotocraftApp, ev: ToolEvent, mods: egui::Modifiers) {
    if app.vector_pencil_enabled {
        let zoom = app.current_zoom();
        if let Err(e) = app.vector_pencil.pointer(&mut app.session, ev, mods, zoom) {
            app.ui.status = e.to_string(); app.ui.status_error = true;
        }
        return;
    }''',1)
# Existing canvas emits Move only when app.drag exists. The package owns its own capture.
s = s.replace('    let tool = app.ui.tool;\n    if tool.is_type()', '''    if app.vector_pencil_enabled {
        let zoom = app.current_zoom();
        if let Err(e) = app.vector_pencil.pointer(&mut app.session, ToolEvent::Move { x,y,pressure }, mods, zoom) {
            app.ui.status = e.to_string(); app.ui.status_error = true;
        }
        return;
    }
    let tool = app.ui.tool;
    if tool.is_type()''',1)
s = s.replace('        if move_input {', '''        if app.vector_pencil_enabled {
            let pressure = app.stylus.pressure();
            if let Err(e) = crate::vector_packages::pencil::canvas_input_with_pressure(&mut app.vector_pencil, &mut app.session, &response, &xf, pressure) {
                app.ui.status=e.to_string(); app.ui.status_error=true;
            }
            buttons=crate::paint_mouse::Buttons::default();
        }
        if move_input {''',1)
s = s.replace('let gesture_active_before = app.drag.is_some();', 'let gesture_active_before = app.drag.is_some() || (app.vector_pencil_enabled && app.vector_pencil.active());',1)
s = s.replace('if freehand_tool(tool) {', 'if freehand_tool(tool) || app.vector_pencil_enabled {',1)
s = s.replace('        crate::vector_ui::draw_overlay(app, &painter, &xf, &doc);', '        if !app.vector_pencil_enabled { crate::vector_ui::draw_overlay(app, &painter, &xf, &doc); }',1)
p.write_text(s)
print(out)
