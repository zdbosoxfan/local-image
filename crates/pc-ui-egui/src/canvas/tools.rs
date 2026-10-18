//! Tool input: Alt-eyedropper sampling, per-tool down/move/up events, selection drags, gesture
//! commits, and the polygon lasso and crop commits.
use super::*;

/// Tools on which holding ⌥ (Alt) switches to the Eyedropper: a click or drag sets the
/// foreground colour, as with the Eyedropper itself (#417).
pub(super) fn alt_samples(tool: Tool, mods: egui::Modifiers) -> bool {
    // Control+Alt is the brush-resize drag (`brush_resize`), not sampling.
    mods.alt && !mods.ctrl && matches!(tool, Tool::Brush | Tool::Pencil | Tool::Gradient | Tool::PaintBucket)
}

/// Decided when the press starts, so ⌥ pressed or released mid-stroke never switches between
/// painting and sampling.
pub(super) fn alt_eyedropper(app: &mut PhotocraftApp, ev: ToolEvent, mods: egui::Modifiers) -> bool {
    if matches!(ev, ToolEvent::Down { .. }) {
        app.alt_sampling = alt_samples(app.ui.tool, mods);
    }
    if !app.alt_sampling {
        return false;
    }
    match ev {
        // Without ⌥: the sample sets the foreground colour, whatever the Eyedropper would do.
        ToolEvent::Down { x, y, .. } | ToolEvent::Move { x, y, .. } => sample_eyedropper(app, x, y, egui::Modifiers::NONE),
        ToolEvent::Up { .. } => app.alt_sampling = false,
    }
    true
}

pub(super) fn sample_eyedropper(app: &mut PhotocraftApp, x: f64, y: f64, mods: egui::Modifiers) {
    if let Some([r, g, b]) = composite_color(app, x, y) {
        let key = if mods.alt { "background" } else { "foreground" };
        let _ = app.run("tools.setColors", json!({ key: [r, g, b, 1.0] }));
    }
}

/// The active document's composite colour at document point (x, y): what the Eyedropper picks.
/// `None` off the image or over transparency.
pub(crate) fn composite_color(app: &mut PhotocraftApp, x: f64, y: f64) -> Option<[f32; 3]> {
    if !(x.is_finite() && y.is_finite()) {
        return None;
    }
    let v = app.run("document.pixel", json!({"x": x.floor(), "y": y.floor()})).ok()?;
    match serde_json::from_value::<Vec<f32>>(v).ok()?[..] {
        [r, g, b, a] if a > 0.0 => Some([r, g, b]),
        _ => None,
    }
}

/// The body of a `Move` for every tool: tracked position, ⇧ constraint, the moving layer, and so
/// on. It does not feed the live stroke, so the canvas can push a whole frame's recovered samples
/// and update the live stroke once (see `canvas_view`).
pub(super) fn tool_move(app: &mut PhotocraftApp, x: f64, y: f64, pressure: f32, mods: egui::Modifiers) {
    let tool = app.ui.tool;
    if tool.is_type() && app.drag.is_none() {
        crate::type_tool::pointer_move(app, x, y);
    }
    if tool == Tool::Pen {
        crate::vector_ui::pen_move(app, x, y);
    }
    let zoom = app.current_zoom();
    if let Some(d) = app.drag.as_mut().filter(|d| d.reposition) {
        d.track(mods);
        d.shift_to([x, y]);
    } else if let Some(d) = &mut app.drag {
        d.track(mods);
        // ⇧: straight 0/45/90° strokes, 45° gradient angles (stroke_constraint.rs).
        let last = d.points.last().map_or(d.start, |p| [p[0], p[1]]);
        let [x, y] = crate::stroke_constraint::constrain(d.tool, &mut d.constrain, d.start, last, [x, y], mods.shift, zoom);
        if d.points.last().is_none_or(|p| (p[0] - x).abs() + (p[1] - y).abs() > 0.25) {
            d.points.push([x, y, pressure as f64]);
            d.stamp();
            app.stylus.record_point();
        }
    }
}

/// Tool state machine. Shared by mouse input and automation.
pub fn tool_event(app: &mut PhotocraftApp, ev: ToolEvent, mods: egui::Modifiers) {
    tool_event_with_pose(app, ev, mods, None, None, None);
}

pub(super) fn tool_event_with_pose(
    app: &mut PhotocraftApp,
    ev: ToolEvent,
    mods: egui::Modifiers,
    pose: Option<[f32; 3]>,
    settings: Option<serde_json::Value>,
    when: Option<std::time::Instant>,
) {
    if app.live_stroke.as_ref().is_some_and(|l| l.finish.is_some()) {
        let settings = settings.or_else(|| matches!(ev, ToolEvent::Down { .. } | ToolEvent::Up { .. }).then(|| stroke_settings(app)));
        let sample = app.stylus.sample().unwrap_or_default();
        if let Some(l) = &mut app.live_stroke {
            l.queued.push(QueuedStrokeEvent {
                when: when.unwrap_or_else(std::time::Instant::now),
                tick: false,
                event: ev,
                modifiers: mods,
                pose: pose.unwrap_or([sample.tilt_x, sample.tilt_y, sample.rotation]),
                tool: app.ui.tool,
                settings,
                secondary_erase: app.secondary_erase,
                resize_armed: app.brush_resize_armed,
            });
        }
        return;
    }
    let armed = std::mem::take(&mut app.brush_resize_armed);
    let mut guard = std::mem::take(&mut app.ui.persona_pointer);
    let intercepted = guard.intercept(app, ev);
    app.ui.persona_pointer = guard;
    if intercepted || crate::vector_integration::pointer(app, ev, mods) {
        return;
    }
    let ids = app.session.active().map(|s| s.selected_layers()).unwrap_or_default();
    let symbol_event = match ev {
        ToolEvent::Down { x, y, .. } => crate::symbols::Event::Down(x, y),
        ToolEvent::Move { x, y, .. } => crate::symbols::Event::Move(x, y),
        ToolEvent::Up { x, y } => crate::symbols::Event::Up(x, y),
    };
    if crate::symbols::pointer(&mut app.session, &mut app.ui.symbols, symbol_event, &ids) {
        return;
    }
    // An Alt+right-drag armed for this press (`paint_mouse`): taken before anything else can
    // consume the event, so it never outlives the press it was armed for (#297).
    crate::transform_tool::end_if_left(app);
    // View › Snap / Snap To and smart guides (snap_ui.rs).
    let raw = ev;
    // Pick before building snap targets: the clicked layer may differ from the old selection.
    if let ToolEvent::Down { x, y, .. } = ev
        && app.ui.tool == Tool::Move
        && app.ui.transform.is_none()
        && crate::rulers::guide_at(app, x, y).is_none()
        && app.session.active().is_some_and(|s| s.doc.selection.is_none())
        && app.ui.tool_options.move_auto_select != (mods.command || mods.ctrl)
    {
        let target = app.ui.tool_options.move_target.clone();
        let mode = if mods.shift { "add" } else { "replace" };
        if app.session.prefs().toolbar.persona == photocraft_engine::toolsets::Persona::Vector {
            crate::vector_integration::pick_object(app, [x, y], mods.shift, target == "group");
        } else {
            let _ = app.run("layer.pickAt", json!({"x": x, "y": y, "target": target, "mode": mode}));
        }
    }
    // A press anywhere but on the floating piece (or with ⇧ / ⌥, to draw) drops it first.
    if let ToolEvent::Down { x, y, .. } = raw
        && app.session.active().is_some_and(|st| photocraft_engine::float_cmds::floating(st).is_some())
        && selection_drag_kind(app, app.ui.tool, [x, y], mods) != Some(true)
    {
        let _ = app.run("select.drop", json!({}));
    }
    let ev = crate::snap_ui::filter_event(app, ev, mods);
    // Free Transform picks the handle under the press where the user clicked: the snapped press
    // can land outside the corner's grab area and turn a corner drag into a move of the whole box.
    let transform_ev = if matches!(raw, ToolEvent::Down { .. }) { raw } else { ev };
    if crate::transform_tool::pointer(app, transform_ev, mods) {
        return;
    }
    if crate::distort_ui::pointer(app, ev, mods) {
        return;
    }
    // Window › Modifier Keys: sticky Shift/⌘/⌥ act as held keys.
    let mods = crate::workspace_ui::sticky_mods(app, mods);
    // ⌘⌥⌃-click with any tool selects the topmost layer with pixels there (quick_pick.rs); it
    // must run before the ⌃⌥ brush resize below, which the same keys would trigger.
    if crate::quick_pick::pointer(app, ev, mods) {
        return;
    }
    // Control+Alt-drag or Alt+right-drag with a painting tool resizes the brush instead of
    // painting (#231, #297).
    if crate::brush_resize::pointer(app, ev, mods, armed) {
        return;
    }
    // ⌥ (Alt) with a painting tool is the Eyedropper for that press.
    if alt_eyedropper(app, ev, mods) {
        return;
    }
    // Move tool: ⇧ locks the axis, ⌥ duplicates (move_mods.rs).
    let ev = crate::move_mods::filter_event(app, ev, mods);
    // Direct Selection, and the Pen's ⌘ (Direct Selection) and ⌥ (Convert Point) modes (#790).
    if crate::direct_select::pointer(app, ev, mods) {
        return;
    }
    // Ruler, Count and Note tools.
    if crate::analysis_ui::pointer(app, ev, mods) {
        return;
    }
    // Slice and Slice Select tools.
    if crate::slice_ui::pointer(app, ev, mods) {
        return;
    }
    // local-image: Perspective Crop: draw the frame, drag its corners.
    if crate::perspective_crop_ui::pointer(app, ev, mods) {
        return;
    }
    // Crop tool: draw, move and resize the frame.
    if crate::crop_ui::pointer(app, ev, mods) {
        return;
    }
    // Gradient tool, live mode: draw and edit Gradient Fill layers.
    if crate::gradient_ui::pointer(app, ev, mods) {
        return;
    }
    if crate::lasso_ui::pointer(app, ev, mods) {
        return;
    }
    // Magnetic Lasso: fastening points and the border following the pointer.
    if crate::magnetic_lasso_ui::pointer(app, ev, mods) {
        return;
    }
    let tool = app.ui.tool;
    if tool == Tool::Eyedropper {
        match ev {
            ToolEvent::Down { x, y, .. } | ToolEvent::Move { x, y, .. } => {
                sample_eyedropper(app, x, y, mods);
                return;
            }
            ToolEvent::Up { .. } => return,
        }
    }
    // Move tool over a guide drags the guide (off the canvas deletes it).
    match ev {
        ToolEvent::Down { x, y, .. } if tool == Tool::Move => {
            if let Some((vertical, i)) = crate::rulers::guide_at(app, x, y) {
                app.guide_drag = Some(crate::rulers::GuideDrag { vertical, index: Some(i), pos: if vertical { x } else { y } });
                return;
            }
        }
        ToolEvent::Move { x, y, .. } => {
            if let Some(d) = app.guide_drag.as_mut().filter(|d| d.index.is_some()) {
                d.pos = if d.vertical { x } else { y };
                return;
            }
        }
        ToolEvent::Up { x, y } => {
            if let Some(mut d) = app.guide_drag.filter(|d| d.index.is_some()) {
                app.guide_drag = None;
                d.pos = if d.vertical { x } else { y };
                crate::rulers::finish_drag(app, d);
                return;
            }
        }
        _ => {}
    }
    match ev {
        ToolEvent::Down { x, y, pressure } => {
            // Painting a type, shape, Smart Object or fill layer asks to rasterize it first
            // (⌥-click with the Clone Stamp or Healing Brush only sets the source).
            let sets_source = matches!(tool, Tool::CloneStamp | Tool::Healing) && mods.alt;
            if !sets_source && crate::rasterize_prompt::intercept(app, tool, x, y, pressure) {
                return;
            }
            // Marquee / lasso inside the selection: drag the outline, or ⌘-drag to cut the selected
            // pixels into a floating piece (`select.float`) and drag that.
            if let Some(cut) = selection_drag_kind(app, tool, [x, y], mods) {
                if cut
                    && app.session.active().is_some_and(|st| photocraft_engine::float_cmds::floating(st).is_none())
                    && let Err(e) = app.run("select.float", json!({"dx": 0, "dy": 0}))
                {
                    app.ui.status = e;
                    app.ui.status_error = true;
                    return;
                }
                let mut d = Drag::new(tool, [x, y], vec![[x, y, pressure as f64]], mods, false);
                d.sel_move = Some(cut);
                app.drag = Some(d);
                return;
            }
            match tool {
                Tool::Pen => {
                    crate::vector_ui::pen_down(app, x, y);
                    return;
                }
                Tool::CloneStamp | Tool::Healing if mods.alt => {
                    crate::retouch_ui::set_source(app, x, y);
                    return;
                }
                Tool::MagicWand => {
                    let o = app.ui.tool_options.clone();
                    let mode = selection_mode(app, mods);
                    let _ = app.run("select.magicWand", json!({"x": x.floor(), "y": y.floor(), "tolerance": o.tolerance, "sampleSize": o.wand_sample, "contiguous": o.contiguous, "antiAlias": o.anti_alias, "sampleAllLayers": o.sample_all_layers, "mode": mode}));
                    return;
                }
                Tool::MagicEraser => {
                    crate::eraser_ui::click(app, tool, x, y);
                    return;
                }
                // local-image: Red Eye, on a click.
                Tool::RedEye => {
                    let o = &app.ui.tool_options;
                    let pet = o.red_eye_pet;
                    let p = json!({"x": x.floor(), "y": y.floor(), "pupilSize": o.red_eye_pupil, "darken": o.red_eye_darken, "pet": pet, "catchlight": o.red_eye_catchlight});
                    match app.run("paint.redEye", p) {
                        Ok(r) if r["changed"] == false => {
                            app.ui.status = if pet {
                                "No pet eye glow found there: click on the glowing pupil."
                            } else {
                                "No red eye found there: click on the red pupil."
                            }
                            .into();
                            app.ui.status_error = false;
                        }
                        Err(e) => {
                            app.ui.status = e;
                            app.ui.status_error = true;
                        }
                        _ => {}
                    }
                    return;
                }
                Tool::PaintBucket => {
                    let o = app.ui.tool_options.clone();
                    let contents = if o.bucket_fill_pattern { "pattern" } else { "foreground" };
                    let _ = app.run("paint.bucket", json!({"x": x.floor(), "y": y.floor(), "tolerance": o.tolerance, "contiguous": o.contiguous, "antiAlias": o.anti_alias, "opacity": o.fill_opacity, "contents": contents, "target": paint_target(app)}));
                    return;
                }
                Tool::PolygonLasso => {
                    polygon_click(app, x, y, mods);
                    return;
                }
                Tool::Type | Tool::VerticalType if crate::type_tool::pointer_down(app, x, y, mods.shift) => return,
                _ => {}
            }
            crate::paint_mouse::sync_tool_smoothing(app);
            let erase = tool == Tool::Eraser || std::mem::take(&mut app.secondary_erase);
            // ⇧-click after a stroke: a straight line from where it ended (stroke_constraint.rs).
            let active = app.session.active().map(|st| st.doc.id);
            let from = app.last_stroke_end.filter(|(doc, _)| mods.shift && crate::stroke_constraint::connects(tool) && Some(*doc) == active).map(|(_, p)| p);
            let mut points = vec![[x, y, pressure as f64]];
            if let Some(p) = from {
                points.insert(0, p);
            }
            app.drag = Some(Drag::new(tool, from.map_or([x, y], |p| [p[0], p[1]]), points, mods, erase));
            if tool == Tool::Move {
                crate::move_ui::begin_pixels(app, mods.alt);
            }
            app.trail = None;
            if let (Some(when), Some(d)) = (when, app.drag.as_mut()) {
                d.born = when;
            }
            app.stylus.begin_stroke();
            if from.is_some() {
                app.stylus.record_point();
            }
            if let (Some(pose), Some(last)) = (pose, app.stylus.stroke.last_mut()) {
                *last = pose;
            }
            app.live_stroke = if strokes_live(tool) { begin_live_stroke(app, settings.as_ref()) } else { None };
        }
        ToolEvent::Move { x, y, pressure } => {
            let count = app.stylus.stroke.len();
            tool_move(app, x, y, pressure, mods);
            if let Some(pose) = pose {
                for last in &mut app.stylus.stroke[count..] {
                    *last = pose;
                }
            }
            if app.stylus.stroke.len() > count
                && let (Some(when), Some(d)) = (when, app.drag.as_mut())
                && let Some(t) = d.times.last_mut()
            {
                *t = when.saturating_duration_since(d.born).as_secs_f64() * 1000.0;
            }
            feed_live_stroke(app);
        }
        ToolEvent::Up { x, y } => {
            if tool.is_type()
                && let Some(e) = app.ui.text_edit.as_mut()
            {
                e.dragging = false;
                e.resize = None;
                app.ui.caret_blink_time = app.last_frame_time;
            }
            if tool == Tool::Pen {
                crate::vector_ui::pen_up(app);
            }
            let zoom = app.current_zoom();
            let Some(mut d) = app.drag.take() else { return };
            d.track(mods);
            if d.reposition {
                d.shift_to([x, y]);
            }
            let last = d.points.last().map_or(d.start, |p| [p[0], p[1]]);
            let [x, y] = crate::stroke_constraint::constrain(d.tool, &mut d.constrain, d.start, last, [x, y], mods.shift, zoom);
            // A Move-tool click (released where it was pressed) selects, it never moves: snapping
            // the release point would otherwise nudge the layer onto a nearby edge.
            if d.tool == Tool::Move && d.points.len() < 2 && matches!(raw, ToolEvent::Up { x, y } if [x, y] == d.start) {
                app.move_preview = None;
                crate::move_mods::finish(app);
                return;
            }
            if d.points.last().is_none_or(|p| p[0] != x || p[1] != y) {
                d.points.push([x, y, d.points.last().map_or(1.0, |p| p[2])]);
                app.stylus.record_point();
                if let (Some(pose), Some(last)) = (pose, app.stylus.stroke.last_mut()) {
                    *last = pose;
                }
            }
            if let Some(settings) = settings {
                finish_brush_gesture(app, d, Some(settings));
            } else {
                finish_gesture(app, d);
            }
            crate::move_mods::finish(app);
        }
    }
}

/// Is document point `p` inside the active document's selection?
pub(super) fn inside_selection(app: &PhotocraftApp, p: [f64; 2]) -> bool {
    let Some(sel) = app.session.active().and_then(|st| st.doc.selection.as_ref()) else { return false };
    let mut v = [0.0f32];
    sel.read_pixel(p[0].floor() as i32, p[1].floor() as i32, &mut v);
    v[0] > 0.0
}

/// Does a press with `tool` at `p` drag the selection rather than draw a new one? `Some(true)`
/// moves the floating piece (⌘ cuts one first; a floating piece moves with a plain drag),
/// `Some(false)` just the outline (no ⇧ / ⌥ and the options bar on New Selection, so a combining
/// drag still draws).
pub fn selection_drag_kind(app: &PhotocraftApp, tool: Tool, p: [f64; 2], mods: egui::Modifiers) -> Option<bool> {
    if !matches!(tool, Tool::RectMarquee | Tool::EllipseMarquee | Tool::Lasso) {
        return None;
    }
    if let Some(f) = app.session.active().and_then(photocraft_engine::float_cmds::floating) {
        let on = inside_selection(app, [p[0] - f64::from(f.offset.0), p[1] - f64::from(f.offset.1)]);
        return (on && !mods.shift && !mods.alt).then_some(true);
    }
    if !inside_selection(app, p) {
        return None;
    }
    if mods.command && !mods.shift && !mods.alt {
        return Some(true);
    }
    (!mods.command && selection_mode(app, mods) == "replace").then_some(false)
}

/// Whole-pixel offset of a selection drag in progress (`Drag::sel_move`).
pub(crate) fn selection_drag_delta(app: &PhotocraftApp) -> Option<(i32, i32)> {
    let d = app.drag.as_ref().filter(|d| d.sel_move.is_some())?;
    let end = d.points.last().map_or(d.start, |p| [p[0], p[1]]);
    Some(((end[0] - d.start[0]).round().clamp(-1e7, 1e7) as i32, (end[1] - d.start[1]).round().clamp(-1e7, 1e7) as i32))
}

/// Where the selection is drawn relative to where it is: a floating piece's offset plus a drag
/// in progress.
pub(super) fn selection_shown_offset(app: &PhotocraftApp) -> Option<(i32, i32)> {
    crate::move_ui::floating_offset(app).or_else(|| selection_drag_delta(app))
}

/// End of a selection drag: move the outline, or the floating piece. A click without moving
/// deselects, like a marquee click (a click on a floating piece leaves it floating).
pub(super) fn finish_selection_drag(app: &mut PhotocraftApp, floating: bool, start: [f64; 2], end: [f64; 2]) {
    let (dx, dy) = ((end[0] - start[0]).round(), (end[1] - start[1]).round());
    let r = match (floating, dx == 0.0 && dy == 0.0) {
        (true, true) => return,
        (false, true) if app.session.is_enabled("select.deselect") => app.run("select.deselect", json!({})),
        (false, true) => return,
        (true, false) => app.run("select.float", json!({"dx": dx, "dy": dy})),
        (false, false) => app.run("select.transformSelection", json!({"dx": dx, "dy": dy})),
    };
    if let Err(e) = r {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

pub(super) fn finish_brush_gesture(app: &mut PhotocraftApp, d: Drag, settings: Option<serde_json::Value>) {
    if !strokes_live(d.tool) {
        finish_gesture(app, d);
        return;
    }
    let captured = settings.is_some();
    let frozen = settings.unwrap_or_else(|| stroke_settings(app));
    let mut p = stroke_params(app, d.tool, d.erase, &timed_points(app, &d, Some(&frozen)));
    if crate::stroke_constraint::connects(d.tool)
        && let Some(st) = app.session.active()
    {
        app.last_stroke_end = Some((st.doc.id, d.points.last().copied().unwrap_or([d.start[0], d.start[1], 1.0])));
    }
    if let Some(l) = &mut app.live_stroke
        && l.worker.is_some()
    {
        if let Ok(points) = photocraft_engine::brush_cmds::parse_points(&p, stroke_command(d.tool)) {
            if l.pending.is_empty() {
                l.pending_since = Some(std::time::Instant::now());
            }
            l.pending.extend_from_slice(&points[l.fed.min(points.len())..]);
            l.fed = points.len();
        }
        p["seed"] = json!(l.stroke.seed);
        apply_stroke_settings(&mut p, &frozen);
        l.finish = Some((stroke_command(d.tool), p));
        return;
    }
    if captured {
        apply_stroke_settings(&mut p, &frozen);
    }
    let live = app.live_stroke.take();
    // The commit takes the live stroke's pixels when it is the same stroke, instead of
    // rendering it all again at mouse-up (`Session::prepare_live_commit`).
    let shown = live.map(|mut l| {
        p["seed"] = json!(l.stroke.seed);
        if captured {
            let _ = l.stroke.bind_commit_params(&app.session, stroke_command(d.tool), &p);
        }
        app.session.prepare_live_commit(l.stroke);
        (l.doc, l.key, l.steps)
    });
    // The canvas already shows the stroke: let the commit's damage rect refresh it rather
    // than recompositing the whole document.
    let committed = app.run(stroke_command(d.tool), p).is_ok();
    app.session.discard_live_commit();
    if committed && let Some((doc, first, steps)) = shown {
        // Raw preview key 0 = the document itself (its colour display folded in).
        shown_as_document(app, doc, |k| k == 0 || (first..=first + steps).contains(&k));
    }
}

pub(crate) fn finish_gesture(app: &mut PhotocraftApp, d: Drag) {
    let end = d.points.last().copied().unwrap_or([d.start[0], d.start[1], 1.0]);
    if let Some(floating) = d.sel_move {
        finish_selection_drag(app, floating, d.start, [end[0], end[1]]);
        return;
    }
    // Where the next ⇧-click line starts.
    if crate::stroke_constraint::connects(d.tool)
        && let Some(st) = app.session.active()
    {
        app.last_stroke_end = Some((st.doc.id, end));
    }
    if crate::eraser_ui::finish_stroke(app, d.tool, &d.points) || crate::retouch_ui::finish_stroke(app, d.tool, &d.points, d.modifiers) {
        return;
    }
    match d.tool {
        Tool::ObjectSelection => crate::retouch_ui::finish_object_selection(app, d.start, [end[0], end[1]], d.modifiers),
        t if crate::vector_ui::is_shape_tool(t) => crate::vector_ui::finish_shape(app, t, d.start, [end[0], end[1]], d.live),
        Tool::PathSelection => crate::vector_ui::path_selection_finish(app, d.start, [end[0], end[1]]),
        Tool::Type | Tool::VerticalType => crate::type_tool::pointer_up(app, d.start, [end[0], end[1]]),
        Tool::Brush | Tool::Pencil | Tool::Eraser => finish_brush_gesture(app, d, None),
        Tool::RectMarquee | Tool::EllipseMarquee => {
            let (a, b) = marquee_corners(&app.ui.tool_options, &d, [end[0], end[1]]);
            let [x0, y0, x1, y1] = marquee_px(a, b);
            if x1 - x0 < 2.0 || y1 - y0 < 2.0 {
                if app.session.is_enabled("select.deselect") {
                    let _ = app.run("select.deselect", json!({}));
                }
                return;
            }
            let mode = selection_mode(app, d.modifiers);
            let (aa, feather) = (app.ui.tool_options.anti_alias, app.ui.tool_options.feather);
            let _ = app.run("select.rect", json!({"x": x0, "y": y0, "width": x1 - x0, "height": y1 - y0, "mode": mode, "ellipse": d.tool == Tool::EllipseMarquee, "antiAlias": aa, "feather": feather}));
        }
        Tool::Patch if crate::retouch_ui::patch_drags_selection(app, d.start, d.modifiers) => crate::retouch_ui::finish_patch(app, d.start, [end[0], end[1]]),
        Tool::ContentAwareMove if crate::retouch_ui::patch_drags_selection(app, d.start, d.modifiers) => {
            crate::retouch_ui::finish_content_aware_move(app, d.start, [end[0], end[1]])
        }
        Tool::Lasso | Tool::Patch | Tool::ContentAwareMove => {
            let pts: Vec<[f64; 2]> = d.points.iter().map(|p| [p[0], p[1]]).collect();
            if pts.len() >= 3 {
                let mode = selection_mode(app, d.modifiers);
                let o = &app.ui.tool_options;
                // The Patch and Content-Aware Move tools have no Feather in their options bar.
                let feather = if d.tool == Tool::Lasso { o.feather } else { 0.0 };
                let _ = app.run("select.lasso", json!({"points": pts, "mode": mode, "antiAlias": o.anti_alias, "feather": feather}));
            } else if app.session.is_enabled("select.deselect") {
                let _ = app.run("select.deselect", json!({}));
            }
        }
        Tool::Gradient => {
            if (end[0] - d.start[0]).abs() + (end[1] - d.start[1]).abs() >= 2.0 {
                let o = app.ui.tool_options.clone();
                let fg = app.session.tools.foreground;
                let bg = app.session.tools.background;
                let _ = app.run(
                    "paint.gradient",
                    json!({"from": [d.start[0], d.start[1]], "to": [end[0], end[1]], "style": o.gradient_style, "reverse": o.gradient_reverse, "dither": o.gradient_dither, "colors": [hex(fg), hex(bg)], "opacity": o.fill_opacity, "mode": o.gradient_blend_mode.label(), "target": paint_target(app)}),
                );
            }
        }
        Tool::Move => {
            let (dx, dy) = ((end[0] - d.start[0]).round(), (end[1] - d.start[1]).round());
            crate::move_ui::finish(app, dx, dy);
        }
        _ => {}
    }
}

/// Selection mode from the options bar, overridden by modifier keys (⇧ add, ⌥ subtract, ⇧⌥ intersect).
/// The cursor badge announces the same mode (`tool_feedback`).
pub(crate) fn selection_mode(app: &PhotocraftApp, m: egui::Modifiers) -> &'static str {
    crate::tool_feedback::selection_mode(Tool::Lasso, app.ui.selection_mode, m)
}

/// A polygonal lasso click adds a vertex; clicking near the first vertex closes the polygon. The
/// first click fixes the selection mode; a new-selection polygon hides the old outline while it is
/// drawn, and replaces it in one history step when it closes.
pub(super) fn polygon_click(app: &mut PhotocraftApp, x: f64, y: f64, mods: egui::Modifiers) {
    let close = app
        .ui
        .polygon
        .first()
        .is_some_and(|p0| app.ui.polygon.len() >= 3 && ((p0[0] - x).powi(2) + (p0[1] - y).powi(2)).sqrt() < 8.0 / app.current_zoom().max(0.01) as f64);
    if close {
        commit_polygon(app);
        return;
    }
    if app.ui.polygon.is_empty() {
        app.ui.polygon_doc = app.session.active().map(|st| st.doc.id);
        app.ui.polygon_mode = selection_mode(app, mods).into();
    }
    app.ui.polygon.push([x, y]);
}

/// The polygon in progress belongs to the document it was started on (`UiState::polygon_doc`):
/// once another document becomes active it is dropped, so its outline doesn't show on (or get
/// committed to) the other document, and the selection it would have replaced is shown again.
/// Like the Lasso's ([`crate::lasso_ui::cancel_stale`]).
pub fn polygon_cancel_stale(app: &mut PhotocraftApp) {
    let active = app.session.active().map(|st| st.doc.id);
    if app.ui.polygon.is_empty() {
        app.ui.polygon_doc = None;
    } else if app.ui.polygon_doc.is_some_and(|d| Some(d) != active) {
        app.ui.polygon.clear();
        app.ui.polygon_mode.clear();
        app.ui.polygon_doc = None;
    }
}

/// A new-selection polygonal (or magnetic) lasso is being drawn, so the selection it will replace
/// is hidden.
pub fn polygon_replaces_selection(app: &PhotocraftApp) -> bool {
    (!app.ui.polygon.is_empty() && app.ui.polygon_mode == "replace") || crate::magnetic_lasso_ui::replaces_selection(app)
}

/// Close the polygonal lasso and make the selection in the mode it started in.
pub fn commit_polygon(app: &mut PhotocraftApp) {
    let pts = std::mem::take(&mut app.ui.polygon);
    let mode = std::mem::take(&mut app.ui.polygon_mode);
    if pts.len() >= 3 {
        let mode = if mode.is_empty() { selection_mode(app, egui::Modifiers::NONE).to_owned() } else { mode };
        let o = &app.ui.tool_options;
        let _ = app.run("select.lasso", json!({"points": pts, "mode": mode, "antiAlias": o.anti_alias, "feather": o.feather}));
    }
}

/// Apply the crop tool's rectangle.
pub fn commit_crop(app: &mut PhotocraftApp) {
    let Some(r) = app.ui.crop_rect.take() else { return };
    // The untouched default frame crops nothing (Photoshop's ↵ on it does nothing).
    if std::mem::take(&mut app.crop.default_frame) {
        return;
    }
    let (x, y) = (r[0].round(), r[1].round());
    let (w, h) = ((r[2] - r[0]).round().max(1.0), (r[3] - r[1]).round().max(1.0));
    let delete = app.ui.tool_options.crop_delete;
    let angle = app.ui.tool_options.crop_angle;
    if app.run("image.crop", json!({"x": x, "y": y, "width": w, "height": h, "angle": angle, "deleteCroppedPixels": delete})).is_ok()
        && let Some(i) = app.session.active_index()
    {
        app.ui.views[i].fit_pending = true;
        app.ui.tool_options.crop_angle = 0.0;
    }
}

/// "mask" when the Layers panel targets (or the canvas shows) the active layer's mask, else "pixels".
pub fn paint_target(app: &PhotocraftApp) -> serde_json::Value {
    use photocraft_engine::channel_cmds::ChannelTarget;
    let Some(st) = app.session.active() else { return json!("pixels") };
    // A targeted alpha channel (Channels panel) or Quick Mask mode wins over the layer.
    match st.channel_view.target {
        ChannelTarget::Alpha(i) if i < st.doc.channels.len() => return json!({ "channel": i }),
        ChannelTarget::Composite if st.doc.quick_mask.is_some() => return json!("quickMask"),
        _ => {}
    }
    let has_mask = st.active_layer.and_then(|id| st.doc.layer(id)).is_some_and(|l| l.mask.is_some());
    // Viewing the mask (⌥-click its thumbnail, #196) paints the mask.
    let viewing = photocraft_engine::mask_view_cmds::current(st).is_some();
    json!(if (app.ui.mask_target || viewing) && has_mask { "mask" } else { "pixels" })
}

/// `#rrggbb` for an sRGB colour (the engine's colour parameter notation).
pub(super) fn hex(c: [f32; 4]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}
