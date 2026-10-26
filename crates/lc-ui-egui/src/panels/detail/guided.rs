use super::{CanvasMap, DevelopSettings, Frame, Gesture, LightcraftApp, PhotoId, Point, Pos2, Rect, Stroke, Tokens, json, register};

pub(super) fn reconcile(app: &mut LightcraftApp) {
    if app.ui.selected_guide.is_some_and(|(photo, index)| {
        app.ui.right != super::RightPanel::Crop
            || app.ui.tool != "guidedUpright"
            || app.session.active().is_none_or(|id| id.0 != photo)
            || app.session.active().and_then(|id| app.session.develop_of(id)).is_none_or(|d| index >= d.geometry.guides.len())
    }) {
        app.ui.selected_guide = None;
    }
}

pub(crate) fn focused(app: &LightcraftApp, ctx: &egui::Context) -> bool {
    if app.ui.right != super::RightPanel::Crop || app.ui.tool != "guidedUpright" {
        return false;
    }
    let Some(photo) = app.session.active() else { return false };
    let Some(develop) = app.session.develop_of(photo) else { return false };
    let focused = ctx.memory(|memory| memory.focused());
    (0..develop.geometry.guides.len()).any(|index| focused == Some(egui::Id::new(("uprightGuide", photo.0, index))))
}

pub(super) fn guided_overlay(
    app: &mut LightcraftApp,
    ui: &mut egui::Ui,
    resp: &egui::Response,
    map: &CanvasMap,
    frame: &Frame,
    d: &DevelopSettings,
    id: PhotoId,
) {
    let p = ui.painter_at(map.rect.expand(8.0));
    let tokens = Tokens::get(ui.ctx());
    let col = tokens.accent;
    let selected = app.ui.selected_guide.filter(|(photo, _)| *photo == id.0).map(|(_, i)| i);
    let draw = |a: Pos2, b: Pos2, sel: bool| {
        let (edge, core, at) = if sel { (3.5, 2.5, 5.0) } else { (3.0, 1.5, 4.0) };
        p.line_segment([a, b], Stroke::new(edge, tokens.overlay_bg));
        p.line_segment([a, b], Stroke::new(core, if sel { tokens.overlay_fg } else { col }));
        for q in [a, b] {
            p.circle_filled(q, at, if sel { tokens.overlay_fg } else { col });
            p.circle_stroke(q, at, Stroke::new(1.0, tokens.overlay_bg));
        }
    };
    let on_screen = |guide: &(Point, Point)| {
        let (sa, sb) = (map.screen(frame.corrected_to_transformed(guide.0)), map.screen(frame.corrected_to_transformed(guide.1)));
        (sa, sb)
    };
    for (i, guide) in d.geometry.guides.iter().enumerate() {
        let (sa, sb) = on_screen(guide);
        register(ui.ctx(), format!("uprightGuide:{i}"), Rect::from_two_pos(sa, sb));
        let response = ui.interact(
            Rect::from_two_pos(sa, sb).expand(7.0),
            egui::Id::new(("uprightGuide", id.0, i)),
            egui::Sense::focusable_noninteractive(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::SelectableLabel,
                ui.is_enabled(),
                selected == Some(i),
                format!("{} {}", li_i18n::library::t("Guided Upright"), i + 1),
            )
        });
        if ui.is_enabled() && response.has_focus() {
            for endpoint in [sa, sb] {
                p.circle_stroke(endpoint, 8.0, Stroke::new(1.0, tokens.overlay_fg));
            }
            if ui.input_mut(|input| {
                input.consume_key(egui::Modifiers::NONE, egui::Key::Space)
                    || input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
            }) {
                app.ui.selected_guide = if selected == Some(i) { None } else { Some((id.0, i)) };
            }
        }
        draw(sa, sb, selected == Some(i));
    }
    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    // The press origin must belong to the canvas even if the pointer leaves a menu in this frame.
    let nav = app.widgets.iter().rev().find(|(w, _)| w == "canvas:navigator").map(|(_, r)| *r);
    if ui.is_enabled()
        && ui.input(|i| i.pointer.primary_pressed())
        && let Some(q) = ui
            .input(|i| {
                // A same-frame release clears egui's press origin.
                i.pointer.press_origin().or_else(|| {
                    i.events.iter().rev().find_map(|e| match e {
                        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, .. } => Some(*pos),
                        _ => None,
                    })
                })
            })
            .or(resp.interact_pointer_pos())
        && resp.contains_pointer()
        && ui.ctx().layer_id_at(q) == Some(resp.layer_id)
        && resp.rect.contains(q)
        && !nav.is_some_and(|n| n.expand(5.0).contains(q))
    {
        let endpoint = d.geometry.guides.iter().enumerate().find_map(|(i, guide)| {
            let (a, b) = on_screen(guide);
            if a.distance(q) <= 7.0 {
                Some((i, 0))
            } else if b.distance(q) <= 7.0 {
                Some((i, 1))
            } else {
                None
            }
        });
        let line = d.geometry.guides.iter().position(|guide| {
            let (a, b) = on_screen(guide);
            let ab = b - a;
            let along = if ab.length_sq() > 0.0 { ((q - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
            (a + ab * along).distance(q) <= 6.0
        });
        match (endpoint, line) {
            (Some((i, end)), _) => app.gesture = Some(Gesture::GuideEndpoint { i, end, from: q }),
            (None, Some(i)) => app.ui.selected_guide = if selected == Some(i) { None } else { Some((id.0, i)) },
            (None, None) => {
                app.ui.selected_guide = None;
                app.gesture = Some(Gesture::Guide { a: map.norm(q) });
            }
        }
    }
    let q = resp.interact_pointer_pos().or_else(|| ui.input(|i| i.pointer.latest_pos()));
    if let (Some(Gesture::Guide { a }), Some(q)) = (&app.gesture, q) {
        draw(map.screen(*a), q, false);
    }
    if let (Some(Gesture::GuideEndpoint { i, end, .. }), Some(q)) = (&app.gesture, q)
        && let Some(guide) = d.geometry.guides.get(*i)
    {
        let fixed = if *end == 0 { guide.1 } else { guide.0 };
        draw(map.screen(frame.corrected_to_transformed(fixed)), q, true);
    }
    if ui.input(|i| i.pointer.any_released()) {
        match app.gesture.take() {
            Some(Gesture::GuideEndpoint { i, end, from }) => {
                app.ui.selected_guide = Some((id.0, i));
                if let (Some(q), Some(guide)) = (q, d.geometry.guides.get(i)) {
                    let fixed_t = if end == 0 { guide.1 } else { guide.0 };
                    let new_t = frame.transformed_to_corrected(map.norm(q));
                    let (a, b) = if end == 0 { (new_t, fixed_t) } else { (fixed_t, new_t) };
                    let (na, nb) = (frame.corrected_to_transformed(a), frame.corrected_to_transformed(b));
                    // Clicking within the grab radius selects without moving the endpoint.
                    let moved = from.distance(q) > 2.0;
                    if moved && na.dist(nb) > 0.02 {
                        let all: Vec<[f64; 4]> = d
                            .geometry
                            .guides
                            .iter()
                            .enumerate()
                            .map(|(j, g)| {
                                let (x, y) = if j == i { (a, b) } else { *g };
                                [x.x, x.y, y.x, y.y]
                            })
                            .collect();
                        if let Err(error) = app.run("geometry.guides", json!({"guides": all})) {
                            app.toast_error(ui.ctx(), error);
                        }
                    }
                }
            }
            Some(Gesture::Guide { a }) => {
                if let Some(q) = q
                    && map.norm(q).dist(a) > 0.02
                {
                    let b = map.norm(q);
                    let (ca, cb) = (frame.transformed_to_corrected(a), frame.transformed_to_corrected(b));
                    if let Err(error) = app.run("geometry.guides", json!({"guides": [[ca.x, ca.y, cb.x, cb.y]], "add": true})) {
                        app.toast_error(ui.ctx(), error);
                    }
                }
            }
            other => app.gesture = other,
        }
    }
}

