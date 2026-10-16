//! Snapping manager and captured pointer bridge for package 00's canvas router.
//! State owns no document edits: every Action carries final snapped geometry for the
//! tool's detached preview or its single engine commit. Releasing re-queries the raw
//! pointer with CURRENT modifiers, never the already-snapped preview position.

use egui::{Color32, Pos2, Response, Stroke, Ui};
use photocraft_doc::{DocId, Document};
use photocraft_engine::{
    Result,
    vector_packages::snapping::query::{self, Angle, Cache, Exclusions, Geometry, Guide, Settings, SnapResult, View},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Manager {
    pub settings: Settings,
}
/// The caller supplies the existing English-source catalogue lookup. Stable fragment
/// IDs are resolved before lookup, matching pc-ui-egui's established i18n convention.
pub fn manager(ui: &mut Ui, manager: &mut Manager, text: impl Fn(&str) -> String) {
    let text = |key: &str| text(source(key));
    let response = ui.button(text("vector.snap.manager"));
    egui::Popup::menu(&response).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.checkbox(&mut manager.settings.enabled, text("vector.snap.enabled"));
        ui.separator();
        ui.add_enabled_ui(manager.settings.enabled, |ui| {
            for (key, value) in [
                ("nodes", &mut manager.settings.nodes),
                ("bounds", &mut manager.settings.bounds),
                ("guides", &mut manager.settings.guides),
                ("grid", &mut manager.settings.grid),
                ("pixel", &mut manager.settings.pixel),
                ("artboard", &mut manager.settings.artboard),
                ("document", &mut manager.settings.document),
                ("alignment", &mut manager.settings.alignment),
                ("spacing", &mut manager.settings.spacing),
                ("angle", &mut manager.settings.angle),
            ] {
                ui.checkbox(value, text(&format!("vector.snap.{key}")));
            }
            ui.add_enabled(query::installed_geometry().is_some(), egui::Checkbox::new(&mut manager.settings.curves, text("vector.snap.curves")))
                .on_disabled_hover_text(text("vector.snap.requiresGeometry"));
            ui.horizontal(|ui| {
                let label = ui.label(text("vector.snap.tolerance"));
                ui.add(egui::DragValue::new(&mut manager.settings.tolerance_px).range(0.0..=64.0).speed(0.25)).labelled_by(label.id);
            });
            ui.add_enabled_ui(manager.settings.grid, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.label(text("vector.snap.gridStep"));
                    ui.add(egui::DragValue::new(&mut manager.settings.grid_step).range(0.001..=1e7).speed(0.5)).labelled_by(label.id);
                });
            });
        });
    });
}

fn source(key: &str) -> &str {
    match key {
        "vector.snap.manager" => "Snapping Manager",
        "vector.snap.enabled" => "Enable snapping",
        "vector.snap.nodes" => "Snap to nodes",
        "vector.snap.curves" => "Snap to curves",
        "vector.snap.bounds" => "Snap to bounding boxes",
        "vector.snap.guides" => "Snap to guides",
        "vector.snap.grid" => "Snap to grid",
        "vector.snap.pixel" => "Snap to pixel grid",
        "vector.snap.artboard" => "Snap to artboards",
        "vector.snap.document" => "Snap to document",
        "vector.snap.alignment" => "Alignment guides",
        "vector.snap.spacing" => "Equal spacing guides",
        "vector.snap.angle" => "Angle guides",
        "vector.snap.tolerance" => "Screen tolerance (pixels)",
        "vector.snap.gridStep" => "Grid spacing",
        "vector.snap.requiresGeometry" => "Curve snapping is unavailable.",
        _ => key,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    Point,
    Rect([f64; 4]),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    pub document: DocId,
    pub revision: u64,
    pub targets: Arc<Exclusions>,
    /// Pointer in document space, with the exact correction used by geometry.
    pub pointer: [f64; 2],
    pub geometry: Gesture,
    pub snap: SnapResult,
}
impl Preview {
    /// Repeat immediately before publication, after preparing detached caches. Groups
    /// preflight every descendant; a late locked/missing target cannot partially commit.
    pub fn preflight(&self, doc: &Document, revision: u64) -> Result<()> {
        let error = |s: &str| photocraft_engine::EngineError::Other(s.into());
        if doc.id != self.document || revision != self.revision {
            return Err(error("stale snapping gesture"));
        }
        if self.targets.layers.iter().any(|id| doc.layer(*id).is_none()) || self.targets.paths.iter().any(|name| !doc.paths.iter().any(|p| &p.name == name)) {
            return Err(error("snapping gesture target is missing"));
        }
        for (path, _, layer) in doc.walk() {
            if (1..=path.len()).any(|n| doc.layer_at(&path[..n]).is_some_and(|l| self.targets.layers.contains(&l.id))) {
                let locks = doc.effective_locks(layer.id);
                if locks.all
                    || match self.geometry {
                        Gesture::Point => locks.pixels,
                        Gesture::Rect(_) => locks.position,
                    }
                {
                    return Err(error("snapping gesture target is locked"));
                }
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Begin(Preview),
    Preview(Preview),
    Commit(Preview),
    Cancel,
}

struct Capture {
    cache: Cache,
    targets: Arc<Exclusions>,
    start: [f64; 2],
    gesture: Gesture,
    origin: Pos2,
    last: Pos2,
    moved: bool,
    mapping: Mapping,
}
#[derive(Default)]
pub struct State {
    capture: Option<Capture>,
    pub preview: Option<Preview>,
}
/// Freeze the screen-to-document transform at press, as Move does, so relayout,
/// zoom/flip changes and crossing panels cannot change the gesture's origin.
#[derive(Clone, Copy)]
pub struct Mapping {
    pub origin: Pos2,
    pub document_origin: [f64; 2],
    pub zoom: f64,
    pub flip: bool,
}
impl Mapping {
    fn point(self, p: Pos2) -> [f64; 2] {
        let d = p - self.origin;
        [self.document_origin[0] + f64::from(d.x) / self.zoom * if self.flip { -1.0 } else { 1.0 }, self.document_origin[1] + f64::from(d.y) / self.zoom]
    }
}
pub struct Input<'a> {
    pub document: &'a Document,
    pub revision: u64,
    pub exclusions: Exclusions,
    pub gesture: Gesture,
    pub settings: &'a Settings,
    pub view: View,
    pub mapping: Mapping,
    pub angle: Option<Angle>,
    pub geometry: Option<Geometry>,
    /// Set by the router on tool/persona changes or modal activation.
    pub cancel: bool,
}
impl State {
    pub fn captured(&self) -> bool {
        self.capture.is_some()
    }
    pub fn cancel(&mut self) {
        self.capture = None;
        self.preview = None;
    }
    /// Call before painting the canvas. Uses egui pointer events, not drag thresholds.
    /// Captured motion/release remains active outside the original response rectangle.
    pub fn input(&mut self, response: &Response, input: Input<'_>) -> Result<Option<Action>> {
        let ctx = &response.ctx;
        let (pressed, released, down, position, mods, focused, escape) = ctx.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_released(),
                i.pointer.primary_down(),
                i.pointer.latest_pos().or(i.pointer.interact_pos()),
                i.modifiers,
                i.focused && !i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false))),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if self.capture.as_ref().is_some_and(|c| !c.cache.is_current(input.document.id, input.revision)) || input.cancel || !focused || escape {
            let had = self.captured();
            self.cancel();
            return Ok(had.then_some(Action::Cancel));
        }
        let mut began = false;
        if !self.captured() && pressed {
            let origin = ctx.input(|i| i.pointer.press_origin()).filter(|p| response.rect.contains(*p) && ctx.layer_id_at(*p) == Some(response.layer_id));
            if let Some(origin) = origin {
                input.view.tolerance(input.settings.tolerance_px)?;
                // Mapping is validated by the same bounds as the query view.
                View { zoom: input.mapping.zoom, pixels_per_point: input.view.pixels_per_point }.tolerance(input.settings.tolerance_px)?;
                let cache = Cache::build(input.document, input.revision, input.exclusions, input.geometry)?;
                let raw = input.mapping.point(origin);
                let snapped = match input.gesture {
                    Gesture::Point => cache.point(raw, input.settings, input.view, input.angle, false)?,
                    Gesture::Rect(_) => SnapResult::unchanged(raw),
                };
                let start = snapped.point;
                let targets = Arc::new(cache.exclusions().clone());
                self.preview = Some(Preview {
                    document: cache.document(),
                    revision: cache.revision(),
                    targets: targets.clone(),
                    pointer: start,
                    geometry: input.gesture,
                    snap: snapped,
                });
                self.capture = Some(Capture { cache, targets, start, gesture: input.gesture, origin, last: origin, moved: false, mapping: input.mapping });
                began = true;
            }
        }
        let Some(capture) = &mut self.capture else {
            return Ok(None);
        };
        if !down && !released {
            self.cancel();
            return Ok(Some(Action::Cancel));
        }
        let position = match position {
            Some(position) => position,
            None if released => capture.last,
            None => return Ok(None),
        };
        capture.last = position;
        capture.moved |= position != capture.origin;
        if released && !capture.moved && matches!(capture.gesture, Gesture::Rect(_)) {
            self.cancel();
            return Ok(Some(Action::Cancel));
        }
        // Rect translations are relative to the unsnapped press. Point drawing starts
        // snapped, while the raw endpoint is always re-evaluated independently.
        let raw = if matches!(capture.gesture, Gesture::Rect(_)) {
            let delta = position - capture.origin;
            [
                capture.start[0] + f64::from(delta.x) / capture.mapping.zoom * if capture.mapping.flip { -1.0 } else { 1.0 },
                capture.start[1] + f64::from(delta.y) / capture.mapping.zoom,
            ]
        } else {
            capture.mapping.point(position)
        };
        let bypass = mods.ctrl || mods.command || mods.mac_cmd;
        let update = (|| {
            let (snap, geometry, pointer) = match capture.gesture {
                Gesture::Point => {
                    let snap = capture.cache.point(raw, input.settings, input.view, input.angle, bypass)?;
                    let pointer = snap.point;
                    (snap, Gesture::Point, pointer)
                }
                Gesture::Rect(rect) => {
                    let moved = query::translated(rect, [raw[0] - capture.start[0], raw[1] - capture.start[1]]);
                    let snap =
                        if capture.moved { capture.cache.rect(moved, input.settings, input.view, bypass)? } else { SnapResult::unchanged([rect[0], rect[1]]) };
                    let geometry = Gesture::Rect(query::translated(moved, snap.delta));
                    let pointer = [raw[0] + snap.delta[0], raw[1] + snap.delta[1]];
                    (snap, geometry, pointer)
                }
            };
            Ok(Preview { document: capture.cache.document(), revision: capture.cache.revision(), targets: capture.targets.clone(), pointer, geometry, snap })
        })();
        let preview = match update {
            Ok(preview) => preview,
            Err(e) => {
                self.cancel();
                return Err(e);
            }
        };
        self.preview = Some(preview.clone());
        if released {
            self.cancel();
            preview.preflight(input.document, input.revision)?;
            Ok(Some(Action::Commit(preview)))
        } else if began {
            Ok(Some(Action::Begin(preview)))
        } else {
            Ok(Some(Action::Preview(preview)))
        }
    }
}

/// Paint at the caller's canvas transform. Labels use document unit formatting supplied
/// by the app; guide geometry always comes from the final snapped query result.
pub fn paint(painter: &egui::Painter, guides: &[Guide], screen: impl Fn([f64; 2]) -> Pos2, measure: impl Fn(f64) -> String) {
    let color = Color32::from_rgb(255, 0, 255);
    for guide in guides {
        let (a, b) = (screen(guide.a), screen(guide.b));
        if a == b {
            painter.circle_stroke(a, 4.0, Stroke::new(1.0, color));
        } else {
            painter.line_segment([a, b], Stroke::new(1.0, color));
        }
        if let Some(value) = guide.value {
            painter.text(a + (b - a) * 0.5, egui::Align2::CENTER_BOTTOM, measure(value), egui::FontId::proportional(11.0), color);
        }
    }
}

#[cfg(test)]
#[path = "snapping/tests.rs"]
mod tests;
