//! Canvas overlays: marching ants, crop shield, tool state, transform controls and drag previews.
use super::*;

/// Draw boundary segments as marching ants: white base, black dashes phased along x + y.
pub(super) fn marching_ants_segments(painter: &egui::Painter, xf: &ViewXform, segs: &[crate::outline::Segment], time: f64) {
    let dash = 4.0f32;
    let phase = ((time * 10.0) % (dash as f64 * 2.0)) as f32;
    let white = Stroke::new(1.0, Color32::WHITE);
    let black = Stroke::new(1.0, Color32::BLACK);
    let clip = painter.clip_rect();
    for (a, b) in segs {
        let pa = xf.to_screen(a[0] as f32, a[1] as f32);
        let pb = xf.to_screen(b[0] as f32, b[1] as f32);
        if !clip.intersects(Rect::from_two_pos(pa, pb).expand(1.0)) {
            continue;
        }
        let pa = pos2(pa.x.round() + 0.5, pa.y.round() + 0.5);
        let pb = pos2(pb.x.round() + 0.5, pb.y.round() + 0.5);
        painter.line_segment([pa, pb], white);
        let len = pa.distance(pb);
        if len < 0.5 {
            continue;
        }
        let dir = (pb - pa) / len;
        // Phase by screen position so dashes line up across joined segments.
        let start = (pa.x + pa.y + phase).rem_euclid(dash * 2.0);
        let mut t = -start;
        while t < len {
            let s0 = t.max(0.0);
            let s1 = (t + dash).min(len);
            if s1 > s0 {
                painter.line_segment([pa + dir * s0, pa + dir * s1], black);
            }
            t += dash * 2.0;
        }
    }
}

/// Photoshop crop overlay: dimmed outside, bright frame, rule-of-thirds grid, corner and edge handles.
pub(super) fn crop_overlay(painter: &egui::Painter, xf: &ViewXform, r: Rect, kind: &str) {
    let camera = xf.camera();
    let clip = camera.document_bounds(painter.clip_rect());
    let cut = r.intersect(clip);
    let dim = Color32::from_black_alpha(130);
    if !cut.is_positive() {
        painter.rect_filled(painter.clip_rect(), 0.0, dim);
    } else {
        for band in [
            Rect::from_min_max(clip.min, pos2(clip.right(), cut.top())),
            Rect::from_min_max(pos2(clip.left(), cut.bottom()), clip.max),
            Rect::from_min_max(pos2(clip.left(), cut.top()), pos2(cut.left(), cut.bottom())),
            Rect::from_min_max(pos2(cut.right(), cut.top()), pos2(clip.right(), cut.bottom())),
        ]
        .into_iter()
        .filter(Rect::is_positive)
        {
            painter.add(egui::Shape::convex_polygon(camera.quad(band).to_vec(), dim, Stroke::NONE));
        }
    }
    let line = |a, b, stroke| {
        painter.line_segment([camera.to_screen(a), camera.to_screen(b)], stroke);
    };
    painter.add(egui::Shape::closed_line(camera.quad(r).to_vec(), Stroke::new(1.0, Color32::WHITE)));
    let thin = Stroke::new(1.0, Color32::from_white_alpha(90));
    let cuts = |fractions: &[f32]| {
        for f in fractions {
            let (x, y) = (r.left() + r.width() * f, r.top() + r.height() * f);
            line(pos2(x, r.top()), pos2(x, r.bottom()), thin);
            line(pos2(r.left(), y), pos2(r.right(), y), thin);
        }
    };
    match kind {
        "none" => {}
        "grid" => cuts(&[0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875]),
        "golden" => cuts(&[0.382, 0.618]),
        "diagonal" => {
            let s = r.width().min(r.height());
            for (c, dx, dy) in [(r.left_top(), 1.0, 1.0), (r.right_top(), -1.0, 1.0), (r.left_bottom(), 1.0, -1.0), (r.right_bottom(), -1.0, -1.0)] {
                line(c, c + vec2(s * dx, s * dy), thin);
            }
        }
        "triangle" => {
            let (a, b) = (r.left_bottom(), r.right_top());
            line(a, b, thin);
            let d = (b - a).normalized();
            for c in [r.left_top(), r.right_bottom()] {
                line(c, a + d * (c - a).dot(d), thin);
            }
        }
        _ => cuts(&[1.0 / 3.0, 2.0 / 3.0]),
    }
    let handle = Stroke::new(3.0, Color32::WHITE);
    let len = (14.0 / xf.zoom).min(r.width() / 3.0).min(r.height() / 3.0);
    for (c, dx, dy) in [(r.left_top(), 1.0, 1.0), (r.right_top(), -1.0, 1.0), (r.left_bottom(), 1.0, -1.0), (r.right_bottom(), -1.0, -1.0)] {
        line(c, c + vec2(len * dx, 0.0), handle);
        line(c, c + vec2(0.0, len * dy), handle);
    }
    let (lx, ly) = (len.min(r.width() / 4.0) / 2.0, len.min(r.height() / 4.0) / 2.0);
    for (c, horizontal) in [(r.center_top(), true), (r.center_bottom(), true), (r.left_center(), false), (r.right_center(), false)] {
        let e = if horizontal { vec2(lx, 0.0) } else { vec2(0.0, ly) };
        line(c - e, c + e, handle);
    }
}

/// Overlays that persist between gestures: polygonal or magnetic lasso in progress, pending crop box.
pub(super) fn draw_tool_state(app: &PhotocraftApp, painter: &egui::Painter, xf: &ViewXform, hover: Option<Pos2>) {
    crate::perspective_crop_ui::draw(app, painter, xf);
    crate::magnetic_lasso_ui::draw(app, painter, xf, hover);
    if !app.ui.polygon.is_empty() {
        let mut pts: Vec<Pos2> = app.ui.polygon.iter().map(|p| xf.to_screen(p[0] as f32, p[1] as f32)).collect();
        if let Some(h) = hover {
            pts.push(h);
        }
        crate::tool_feedback::draw_ants(painter, &pts, false);
        for p in pts.iter().take(app.ui.polygon.len()) {
            painter.rect_filled(Rect::from_center_size(*p, vec2(5.0, 5.0)), 0.0, Color32::WHITE);
            painter.rect_stroke(Rect::from_center_size(*p, vec2(5.0, 5.0)), 0.0, Stroke::new(1.0, Color32::BLACK), egui::StrokeKind::Outside);
        }
    }
    if let Some(c) = app.ui.crop_rect {
        let r = Rect::from_min_max(pos2(c[0] as f32, c[1] as f32), pos2(c[2] as f32, c[3] as f32));
        crop_overlay(painter, xf, r, &app.ui.tool_options.crop_overlay);
        // local-image: Straighten: the part of the image the tilted frame keeps.
        let angle = app.ui.tool_options.crop_angle;
        if angle.abs() > 0.01 {
            let (sn, cs) = angle.to_radians().sin_cos();
            let (cx, cy) = (((c[0] + c[2]) / 2.0) as f32, ((c[1] + c[3]) / 2.0) as f32);
            let (hw, hh) = (((c[2] - c[0]) / 2.0) as f32, ((c[3] - c[1]) / 2.0) as f32);
            let corner = |dx: f32, dy: f32| xf.to_screen(cx + cs * dx - sn * dy, cy + sn * dx + cs * dy);
            let pts = vec![corner(-hw, -hh), corner(hw, -hh), corner(hw, hh), corner(-hw, hh)];
            let accent = crate::theme::Tokens::get(painter.ctx()).accent;
            painter.add(egui::Shape::closed_line(pts.clone(), Stroke::new(1.5, accent)));
            for k in [1.0 / 3.0, 2.0 / 3.0] {
                let a = pts[0] + (pts[1] - pts[0]) * k;
                let b = pts[3] + (pts[2] - pts[3]) * k;
                painter.line_segment([a, b], Stroke::new(1.0, accent.gamma_multiply(0.6)));
                let a = pts[0] + (pts[3] - pts[0]) * k;
                let b = pts[1] + (pts[2] - pts[1]) * k;
                painter.line_segment([a, b], Stroke::new(1.0, accent.gamma_multiply(0.6)));
            }
        }
    }
}

/// Move tool › Show Transform Controls: the selected layers' shared transform bounds.
pub(super) fn transform_controls_bounds(app: &PhotocraftApp) -> Option<DRect> {
    let dragging_box = app.drag.as_ref().is_some_and(|d| d.tool == Tool::Move) && app.session.prefs().interface.show_bounding_box_when_dragging_layer;
    if app.ui.tool != Tool::Move || !(app.ui.tool_options.move_show_transform || dragging_box) || !app.ui.view.extras || app.ui.transform.is_some() {
        return None;
    }
    let st = app.session.active()?;
    let l = st.active_layer.and_then(|id| st.doc.layer(id))?;
    if crate::doc_props_ui::is_background(&st.doc, l) {
        return None;
    }
    let b = if let Some(sel) = &st.doc.selection {
        photocraft_compose::bounds::content_bounds(sel)
    } else {
        photocraft_engine::layer_multi_cmds::move_targets(&st.doc, &st.selected_layers())
            .into_iter()
            .filter_map(|id| st.doc.layer(id))
            .map(|l| photocraft_engine::transform_cmds::transform_bounds(&st.doc, l))
            .fold(DRect::EMPTY, |a, b| a.union(&b))
    };
    let (dx, dy) = crate::move_ui::drag_offset(app).unwrap_or((0, 0));
    let b = b.translate(dx, dy);
    if b.is_empty() {
        return None;
    }
    Some(b)
}

#[cfg(test)]
pub(super) fn transform_controls_rect(app: &PhotocraftApp, xf: &ViewXform) -> Option<Rect> {
    transform_controls_bounds(app).map(|b| xf.doc_rect(b))
}

/// A visible handle starts scaling; the narrow band just outside the box starts rotation.
/// The interior stays the normal Move-tool drag target.
pub(super) fn transform_controls_hit(r: Rect, p: Pos2) -> bool {
    let handles = [r.left_top(), r.center_top(), r.right_top(), r.right_center(), r.right_bottom(), r.center_bottom(), r.left_bottom(), r.left_center()];
    let grab = crate::transform_tool::HANDLE_PX as f32;
    handles.iter().any(|h| h.distance(p) <= grab) || (r.expand(18.0).contains(p) && !r.expand(5.0).contains(p))
}

/// Enter the existing Free Transform session when a Move-tool transform control is pressed.
pub(crate) fn begin_transform_controls_at(app: &mut PhotocraftApp, ctx: &egui::Context, xf: &ViewXform, p: Pos2) -> bool {
    let Some(b) = transform_controls_bounds(app) else { return false };
    let r = Rect::from_min_max(pos2(b.x0 as f32, b.y0 as f32), pos2(b.x1 as f32, b.y1 as f32)) * xf.zoom;
    let point = xf.to_doc(p);
    if !transform_controls_hit(r, pos2(point[0] as f32, point[1] as f32) * xf.zoom) {
        return false;
    }
    match crate::transform_tool::begin(app, ctx) {
        Ok(()) => true,
        Err(e) => {
            app.ui.status = e;
            false
        }
    }
}

/// Move tool › Show Transform Controls: the selection's bounding box with its eight handles.
pub(super) fn draw_transform_controls(app: &mut PhotocraftApp, painter: &egui::Painter, xf: &ViewXform) {
    let Some(b) = transform_controls_bounds(app) else { return };
    let r = Rect::from_min_max(pos2(b.x0 as f32, b.y0 as f32), pos2(b.x1 as f32, b.y1 as f32));
    let accent = crate::theme::Tokens::get(painter.ctx()).accent;
    painter.add(egui::Shape::closed_line(xf.camera().quad(r).to_vec(), Stroke::new(1.0, accent)));
    for p in [r.left_top(), r.center_top(), r.right_top(), r.right_center(), r.right_bottom(), r.center_bottom(), r.left_bottom(), r.left_center()]
        .map(|p| xf.camera().to_screen(p))
    {
        let h = Rect::from_center_size(p, vec2(7.0, 7.0));
        painter.rect_filled(h, 0.0, Color32::WHITE);
        painter.rect_stroke(h, 0.0, Stroke::new(1.0, accent), egui::StrokeKind::Inside);
    }
}

pub(super) fn draw_drag_preview(app: &mut PhotocraftApp, painter: &egui::Painter, xf: &ViewXform) {
    crate::ai_remove_ui::overlay(app, painter, xf);
    draw_tool_state(app, painter, xf, painter.ctx().input(|i| i.pointer.hover_pos()));
    let Some(d) = &app.drag else {
        app.trail = None;
        return;
    };
    // Dragging the selection: its own ants follow the pointer (`selection_shown_offset`).
    if d.sel_move.is_some() {
        return;
    }
    let last = d.points.last().map(|p| [p[0], p[1]]).unwrap_or(d.start);
    let marquee = matches!(d.tool, Tool::RectMarquee | Tool::EllipseMarquee).then(|| marquee_corners(&app.ui.tool_options, d, last));
    if let Some((a, b)) = marquee {
        let options = &app.ui.tool_options;
        let rect = if options.marquee_style == "fixedSize" { fixed_size_px(a, b, options.marquee_width, options.marquee_height) } else { marquee_px(a, b) };
        draw_marquee_readout(painter.ctx(), xf.to_screen(last[0] as f32, last[1] as f32), marquee_readout(rect));
    }
    match d.tool {
        // The canvas shows the live stroke itself (`LiveStroke`).
        Tool::Brush | Tool::Pencil | Tool::Eraser => {}
        t if t.is_brushlike() || t == Tool::QuickSelection => {
            // Retouching strokes preview as a translucent trail of the brush footprint: a mask,
            // not a brush-wide egui polyline (which zoomed in tessellates into wedges, #189).
            let col = if t == Tool::AiRemove {
                Color32::from_rgba_unmultiplied(255, 40, 60, 110)
            } else {
                Color32::from_white_alpha(if t == Tool::QuickSelection { 40 } else { 60 })
            };
            let Some(st) = app.session.active() else { return };
            let size = [st.doc.size.width, st.doc.size.height];
            let trail = app.trail.get_or_insert_with(|| crate::stroke_trail::Trail::new(size));
            trail.feed(&d.points, app.session.tools.brush.size);
            trail.draw(painter, xf, col);
        }
        t if crate::vector_ui::is_shape_tool(t) => crate::vector_ui::draw_shape_preview(app, painter, xf, t, d.start, last, d.live),
        Tool::RectMarquee | Tool::EllipseMarquee | Tool::ObjectSelection => {
            // Marching ants, visible on any pixels (#172).
            let (a, b) = marquee.unwrap_or((d.start, last));
            let r = Rect::from_two_pos(pos2(a[0] as f32, a[1] as f32), pos2(b[0] as f32, b[1] as f32));
            let pts = if d.tool == Tool::EllipseMarquee {
                crate::tool_feedback::ellipse_points(r).into_iter().map(|p| xf.camera().to_screen(p)).collect()
            } else {
                xf.camera().quad(r).to_vec()
            };
            crate::tool_feedback::draw_ants(painter, &pts, true);
        }
        // Patch / Content-Aware Move dragging the selection: its outline follows the pointer.
        Tool::Patch | Tool::ContentAwareMove if crate::retouch_ui::patch_drags_selection(app, d.start, d.modifiers) => {
            let start = d.start;
            let [dx, dy] = crate::retouch_ui::patch_offset(app, start, last);
            if let Some((_, _, segs)) = &app.outline_cache {
                let moved: Vec<crate::outline::Segment> = segs.iter().map(|(a, b)| ([a[0] + dx, a[1] + dy], [b[0] + dx, b[1] + dy])).collect();
                marching_ants_segments(painter, xf, &moved, painter.ctx().input(|i| i.time));
            }
        }
        Tool::Lasso | Tool::Patch | Tool::ContentAwareMove => {
            let mut pts: Vec<Pos2> = d.points.iter().map(|p| xf.to_screen(p[0] as f32, p[1] as f32)).collect();
            if let Some(lasso) = &d.lasso {
                pts.push(xf.to_screen(lasso.cursor[0] as f32, lasso.cursor[1] as f32));
            }
            crate::tool_feedback::draw_ants(painter, &pts, false);
        }
        Tool::Gradient => {
            let a = xf.to_screen(d.start[0] as f32, d.start[1] as f32);
            let b = xf.to_screen(last[0] as f32, last[1] as f32);
            painter.line_segment([a, b], Stroke::new(3.0, Color32::from_black_alpha(140)));
            painter.line_segment([a, b], Stroke::new(1.0, Color32::WHITE));
            painter.circle_filled(a, 3.0, Color32::WHITE);
            painter.circle_filled(b, 3.0, Color32::WHITE);
        }
        Tool::Type | Tool::VerticalType => {
            let r = Rect::from_two_pos(pos2(d.start[0] as f32, d.start[1] as f32), pos2(last[0] as f32, last[1] as f32));
            let q = xf.camera().quad(r);
            let pts = [q[0], q[1], q[2], q[3], q[0]];
            painter.add(egui::Shape::line(pts.to_vec(), Stroke::new(1.0, Color32::WHITE)));
            painter.add(egui::Shape::dashed_line(&pts, Stroke::new(1.0, Color32::BLACK), 3.0, 3.0));
        }
        // The layers themselves follow the pointer (`move_ui`); the arrow only when they can't.
        Tool::Move if !crate::move_ui::showing(app) => {
            let off = xf.camera().screen_delta(vec2((last[0] - d.start[0]) as f32, (last[1] - d.start[1]) as f32));
            painter.arrow(xf.to_screen(d.start[0] as f32, d.start[1] as f32), off, Stroke::new(2.0, crate::theme::Tokens::get(painter.ctx()).accent));
        }
        _ => {}
    }
}
