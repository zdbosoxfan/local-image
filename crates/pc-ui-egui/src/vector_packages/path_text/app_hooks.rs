//! Exact full-app hooks for package 00's serial router registration. Original code.
//! Compiled in the disposable registration tree and by 00 after merging; not an
//! alternative app or event consumer. All actual edits use the package engine command.
use crate::{
    PhotocraftApp,
    canvas::{ToolEvent, ViewXform},
};
use egui::{Context, Event, Key};
use photocraft_doc::{Color, text::CharStyle};
use photocraft_geom::Point;
use std::sync::Arc;

fn report(app: &mut PhotocraftApp, result: photocraft_engine::Result<()>) {
    if let Err(e) = result {
        app.ui.vector_path_text.cancel();
        app.ui.status = e.to_string();
        app.ui.status_error = true;
    }
}
pub fn synchronize(app: &mut PhotocraftApp) {
    let enabled = app.ui.tool.is_type();
    let result = app.ui.vector_path_text.synchronize(&app.session, enabled);
    report(app, result);
}
pub fn event(app: &mut PhotocraftApp, ev: ToolEvent) -> bool {
    if !app.ui.tool.is_type() {
        return false;
    }
    synchronize(app);
    app.ui.vector_path_text.orientation = if app.ui.tool == crate::state::Tool::VerticalType {
        photocraft_doc::text::Orientation::Vertical
    } else {
        photocraft_doc::text::Orientation::Horizontal
    };
    match ev {
        ToolEvent::Down { x, y, .. } => {
            let tolerance = 6. / f64::from(app.current_zoom().max(0.01));
            if app.ui.text_edit.is_some() {
                let path_selected =
                    app.session.active().is_some_and(|s| s.active_layer.and_then(|id| s.doc.layer(id)).is_some_and(|l| l.vector.on_path.is_some()));
                let curve_hit = app
                    .session
                    .active()
                    .is_some_and(|s| photocraft_engine::vector_packages::path_text::curve_at(&s.doc, Point::new(x, y), tolerance).ok().flatten().is_some());
                if path_selected || curve_hit {
                    crate::type_tool::commit(app);
                    synchronize(app);
                }
            }
            let options = &app.ui.tool_options;
            let [r, g, b, a] = app.session.tools.foreground;
            let mut color = Color::rgb(r, g, b);
            color.alpha = a;
            let mut style = CharStyle { font_family: options.type_font.clone(), size_pt: options.type_size, color, ..Default::default() };
            photocraft_engine::type_cmds::apply_char_props(&mut style, &serde_json::json!({"fontStyle":options.type_style}));
            let mut paragraph = Default::default();
            photocraft_engine::type_cmds::apply_para_props(&mut paragraph, &serde_json::json!({"align":options.type_align}));
            app.ui.vector_path_text.paragraph = paragraph;
            match app.ui.vector_path_text.down(&app.session, Point::new(x, y), tolerance, style) {
                Ok(consumed) => consumed,
                Err(e) => {
                    report(app, Err(e));
                    true
                }
            }
        }
        ToolEvent::Move { x, y, .. } if app.ui.vector_path_text.draft.is_some() => {
            let result = app.ui.vector_path_text.motion(&app.session, Point::new(x, y));
            report(app, result);
            true
        }
        ToolEvent::Up { x, y } if app.ui.vector_path_text.draft.is_some() => {
            let result = app.ui.vector_path_text.motion(&app.session, Point::new(x, y)).and_then(|_| app.ui.vector_path_text.up(&mut app.session));
            report(app, result);
            true
        }
        _ => false,
    }
}
pub fn keys(app: &mut PhotocraftApp, ctx: &Context) -> bool {
    synchronize(app);
    if app.ui.vector_path_text.draft.is_none() {
        return false;
    }
    let events = ctx.input(|i| i.events.clone());
    let mut handled = Vec::new();
    for (index, event) in events.into_iter().enumerate() {
        let result = match event {
            Event::Text(text) | Event::Paste(text) | Event::Ime(egui::ImeEvent::Commit(text)) => app.ui.vector_path_text.type_text(&app.session, &text),
            Event::Key { key: Key::Backspace, pressed: true, .. } => app.ui.vector_path_text.backspace(&app.session),
            Event::Key { key: Key::Delete, pressed: true, .. } => app.ui.vector_path_text.delete(&app.session),
            Event::Key { key: Key::ArrowLeft, pressed: true, modifiers, .. } => {
                app.ui.vector_path_text.navigate(&app.session, false, modifiers.shift);
                Ok(())
            }
            Event::Key { key: Key::ArrowRight, pressed: true, modifiers, .. } => {
                app.ui.vector_path_text.navigate(&app.session, true, modifiers.shift);
                Ok(())
            }
            Event::Key { key: Key::A, pressed: true, modifiers, .. } if modifiers.command || modifiers.ctrl => {
                app.ui.vector_path_text.select_all(&app.session);
                Ok(())
            }
            Event::Key { key: Key::Enter, pressed: true, .. } => app.ui.vector_path_text.commit(&mut app.session),
            Event::Key { key: Key::Escape, pressed: true, .. } | Event::WindowFocused(false) | Event::PointerGone => {
                app.ui.vector_path_text.cancel();
                Ok(())
            }
            _ => continue,
        };
        handled.push(index);
        report(app, result);
    }
    ctx.input_mut(|i| {
        let mut index = 0;
        i.events.retain(|_| {
            let keep = !handled.contains(&index);
            index += 1;
            keep
        });
    });
    true
}
pub fn display_doc(app: &PhotocraftApp, index: usize) -> Option<(Arc<photocraft_doc::Document>, u64)> {
    let state = &app.ui.vector_path_text;
    let draft = state.draft.as_ref()?;
    if app.session.active_index() != Some(index)
        || photocraft_engine::vector_packages::path_text::check_fresh(&app.session, draft.document, draft.revision).is_err()
    {
        return None;
    }
    Some((Arc::new(state.preview.as_ref()?.clone()), state.generation | (1 << 62)))
}
pub fn context(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    synchronize(app);
    if app.ui.vector_path_text.overlay.is_some() || app.ui.vector_path_text.draft.is_some() {
        let result = crate::vector_packages::path_text::context(ui, &mut app.ui.vector_path_text, &mut app.session);
        report(app, result);
    }
}
pub fn overlay(app: &PhotocraftApp, painter: &egui::Painter, xf: &ViewXform) {
    let state = &app.ui.vector_path_text;
    crate::vector_packages::path_text::paint_overlay(state, painter, |p| xf.to_screen(p.x as f32, p.y as f32));
    if state.draft.is_some()
        && let Some(caret) = state.caret_segment()
    {
        let caret = caret.map(|p| {
            let p = state.transform.apply(p);
            xf.to_screen(p.x as f32, p.y as f32)
        });
        let rect = egui::Rect::from_two_pos(caret[0], caret[1]).expand(1.);
        painter.ctx().output_mut(|o| {
            o.ime = Some(egui::output::IMEOutput { purpose: egui::IMEPurpose::Normal, rect, cursor_rect: rect, should_interrupt_composition: false });
        });
    }
}
