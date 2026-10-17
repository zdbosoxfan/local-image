//! Live Brush/Eraser strokes: the background stroke worker, queued input, airbrush timing and brush
//! cursors.
use super::*;

/// A Brush/Eraser stroke shown while it is drawn: the engine renders the real dabs onto a copy of
/// the document, and the canvas redraws only what each step changed.
pub(crate) struct LiveStroke {
    pub(super) stroke: photocraft_engine::brush_cmds::LiveStroke,
    pub(super) doc: photocraft_doc::DocId,
    pub(super) revision: u64,
    /// Preview key of the stroke; step `n` displays as `key + n`.
    pub(super) key: u64,
    /// Damage of each step (step 0 = the document before the stroke).
    pub(super) damage: Vec<DRect>,
    /// Includes restored preview tails, even if they lie outside the current stroke bounds.
    pub(super) damage_bounds: DRect,
    pub(super) steps: u64,
    pub(super) first_step: u64,
    /// Drag points rendered so far.
    pub(super) fed: usize,
    pub(super) worker: Option<StrokeWorker>,
    pub(super) shown_doc: Option<std::sync::Arc<Document>>,
    pub(super) pending: Vec<photocraft_engine::paint::StrokePoint>,
    pub(super) pending_since: Option<std::time::Instant>,
    pub(super) pending_damage: DRect,
    pub(super) finish: Option<(&'static str, serde_json::Value)>,
    pub(super) queued: Vec<QueuedStrokeEvent>,
}
pub(super) struct QueuedStrokeEvent {
    pub(super) when: std::time::Instant,
    pub(super) tick: bool,
    pub(super) event: ToolEvent,
    pub(super) modifiers: egui::Modifiers,
    pub(super) pose: [f32; 3],
    pub(super) tool: Tool,
    pub(super) settings: Option<serde_json::Value>,
    pub(super) secondary_erase: bool,
    pub(super) resize_armed: bool,
}
/// Freeze the settings at the actual release/queued press, rather than at a later worker result.
pub(super) fn stroke_settings(app: &PhotocraftApp) -> serde_json::Value {
    json!({"brush":app.session.tools.brush,"color":app.session.tools.foreground,
        "background":app.session.tools.background,"zoom":app.current_zoom(),"target":paint_target(app),
        "layer":app.session.active().and_then(|s| s.active_layer).map(|l|l.0)})
}
pub(super) fn apply_stroke_settings(params: &mut serde_json::Value, settings: &serde_json::Value) {
    if let Some(settings) = settings.as_object() {
        for (k, v) in settings {
            params[k] = v.clone();
        }
    }
}
pub(super) struct StrokeWork {
    pub(super) queued_at: std::time::Instant,
    pub(super) points: Vec<photocraft_engine::paint::StrokePoint>,
    pub(super) finish: bool,
    pub(super) ctx: egui::Context,
}
pub(super) struct StrokeFrame {
    pub(super) _queued_at: std::time::Instant,
    pub(super) doc: std::sync::Arc<Document>,
    pub(super) damage: DRect,
    pub(super) finished: Option<Box<photocraft_engine::brush_cmds::LiveStroke>>,
    #[cfg(debug_assertions)]
    pub(super) timing: photocraft_engine::paint::profiling::StrokeTiming,
}
/// One OS coordinator owns the stroke. Channels hold at most one work item and one snapshot;
/// UI input accumulates in one batch while it is busy. No waits/joins run on UI or rayon threads.
pub(super) struct StrokeWorker {
    pub(super) input: std::sync::mpsc::SyncSender<StrokeWork>,
    pub(super) output: std::sync::mpsc::Receiver<Result<StrokeFrame, String>>,
    pub(super) cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(super) busy: bool,
}
impl Drop for StrokeWorker {
    fn drop(&mut self) {
        self.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
impl StrokeWorker {
    pub(super) fn start(mut stroke: photocraft_engine::brush_cmds::LiveStroke, threads: usize) -> Option<Self> {
        let (input, requests) = std::sync::mpsc::sync_channel::<StrokeWork>(1);
        let (responses, output) = std::sync::mpsc::sync_channel(1);
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = cancel.clone();
        stroke.set_cancel_flag(cancel.clone());
        stroke.set_worker_budget(threads);
        std::thread::Builder::new()
            .name("pc-live-stroke".into())
            .spawn(move || {
                lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Interactive);
                while let Ok(work) = requests.recv() {
                    if stopped.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    #[cfg(debug_assertions)]
                    let started = std::time::Instant::now();
                    #[cfg(debug_assertions)]
                    let (result, mut timing) = photocraft_engine::paint::profiling::measure(|| stroke.push(&work.points));
                    #[cfg(not(debug_assertions))]
                    let result = stroke.push(&work.points);
                    if stopped.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    let damage = match result {
                        Ok(r) => r,
                        Err(e) => {
                            let _ = responses.send(Err(e.to_string()));
                            work.ctx.request_repaint();
                            break;
                        }
                    };
                    #[cfg(debug_assertions)]
                    {
                        timing.worker_ms = started.elapsed().as_secs_f64() * 1000.0;
                        timing.worker_batches = 1;
                    }
                    let doc = stroke.doc.clone();
                    if work.finish {
                        let _ = responses.send(Ok(StrokeFrame {
                            _queued_at: work.queued_at,
                            doc,
                            damage,
                            finished: Some(Box::new(stroke)),
                            #[cfg(debug_assertions)]
                            timing,
                        }));
                        work.ctx.request_repaint();
                        break;
                    }
                    if responses
                        .send(Ok(StrokeFrame {
                            _queued_at: work.queued_at,
                            doc,
                            damage,
                            finished: None,
                            #[cfg(debug_assertions)]
                            timing,
                        }))
                        .is_err()
                    {
                        break;
                    }
                    work.ctx.request_repaint();
                }
            })
            .ok()?;
        Some(Self { input, output, cancel, busy: false })
    }
}

impl LiveStroke {
    pub(super) fn display_key(&self) -> u64 {
        self.key + self.steps
    }

    /// What changed since the canvas showed preview `key` (0 = the document before the stroke).
    pub(super) fn since(&self, key: u64) -> Option<DRect> {
        let seen = if key == 0 { 0 } else { key.checked_sub(self.key)? };
        if seen > self.steps {
            return None;
        }
        if seen < self.first_step {
            return Some(self.damage_bounds);
        }
        damage_since_step(self.key + self.first_step, &self.damage, key.max(self.key + self.first_step))
    }
}

/// [`LiveStroke::since`] for a stroke whose first preview key is `first` and step damage `damage`.
pub(super) fn damage_since_step(first: u64, damage: &[DRect], key: u64) -> Option<DRect> {
    let seen = if key == 0 { 0 } else { usize::try_from(key.checked_sub(first)?).ok()? };
    Some(damage.get(seen..)?.iter().fold(DRect::EMPTY, |a, r| a.union(r)))
}

/// The live stroke on document `idx`, while it is current.
pub(super) fn live_stroke(app: &PhotocraftApp, idx: usize) -> Option<&LiveStroke> {
    let st = app.session.documents().get(idx)?;
    app.live_stroke.as_ref().filter(|l| (app.drag.is_some() || l.finish.is_some()) && l.doc == st.doc.id && l.revision == st.revision)
}

/// `paint.stroke` params for a Brush/Eraser drag (shared by the live preview and the commit). The
/// stroke smoothing is the session brush's (the options bar's Smoothing %).
pub(super) fn stroke_params(app: &PhotocraftApp, tool: Tool, erase: bool, points: &[Vec<f64>]) -> serde_json::Value {
    let mut p = json!({ "points": points, "erase": erase, "zoom": app.current_zoom(), "target": paint_target(app), "freehand": true });
    if tool == Tool::Pencil {
        p["autoErase"] = json!(app.ui.tool_options.pencil_auto_erase);
    }
    p
}

/// The airbrush (Brush settings › Build-up, or the options bar's airbrush button) on a painting
/// tool: the brush keeps depositing while the pointer lingers.
pub(crate) fn airbrush_on(app: &PhotocraftApp, tool: Tool) -> bool {
    matches!(tool, Tool::Brush | Tool::Eraser) && app.session.tools.brush.build_up
}

/// The drag's points for `paint.stroke`, with their times when the airbrush is on
/// (`[x, y, pressure, tiltX, tiltY, rotation, timeMs]`).
pub(super) fn timed_points(app: &PhotocraftApp, d: &Drag, settings: Option<&serde_json::Value>) -> Vec<Vec<f64>> {
    let mut pts = app.stylus.stroke_points(&d.points);
    let build_up =
        matches!(d.tool, Tool::Brush | Tool::Eraser) && settings.and_then(|p| p["brush"]["buildUp"].as_bool()).unwrap_or_else(|| airbrush_on(app, d.tool));
    if build_up {
        for (i, p) in pts.iter_mut().enumerate() {
            p.resize(6, 0.0);
            p.push(d.times.get(i).copied().unwrap_or(0.0));
        }
    }
    pts
}

/// While an airbrush stroke is held still, repeat its last point every frame so paint builds up
/// (pointer moves alone would only deposit while moving).
pub(super) fn airbrush_tick(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if app.drag.is_none()
        && app.session.tools.brush.build_up
        && let Some(l) = &mut app.live_stroke
        && l.finish.is_some()
        && let Some(boundary) = l.queued.iter().rposition(|e| matches!(e.event, ToolEvent::Down { .. } | ToolEvent::Up { .. }))
        && matches!(l.queued[boundary].event, ToolEvent::Down { .. })
        && matches!(l.queued[boundary].tool, Tool::Brush | Tool::Eraser)
        && let Some(last) = l.queued.last()
    {
        let now = std::time::Instant::now();
        if now.duration_since(last.when).as_millis() >= 16 {
            let event = match last.event {
                ToolEvent::Down { x, y, pressure } | ToolEvent::Move { x, y, pressure } => ToolEvent::Move { x, y, pressure },
                ToolEvent::Up { .. } => return,
            };
            l.queued.push(QueuedStrokeEvent {
                when: now,
                tick: true,
                event,
                modifiers: last.modifiers,
                pose: last.pose,
                tool: last.tool,
                settings: None,
                secondary_erase: last.secondary_erase,
                resize_armed: last.resize_armed,
            });
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
        return;
    }
    let on = app.drag.as_ref().is_some_and(|d| airbrush_on(app, d.tool) && !d.points.is_empty());
    if !on {
        return;
    }
    let Some(d) = app.drag.as_mut() else { return };
    let now = d.born.elapsed().as_secs_f64() * 1000.0;
    if now - d.times.last().copied().unwrap_or(0.0) >= 16.0
        && let Some(p) = d.points.last().copied()
    {
        d.points.push(p);
        d.stamp();
        app.stylus.record_point();
        feed_live_stroke(app);
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(16));
}

/// Tools whose strokes the engine renders while they are drawn (`LiveStroke`).
pub(crate) fn strokes_live(tool: Tool) -> bool {
    matches!(tool, Tool::Brush | Tool::Pencil | Tool::Eraser)
}

/// The command a live-stroking tool commits: the Pencil's `paint.pencil`, else `paint.stroke`.
pub(crate) fn stroke_command(tool: Tool) -> &'static str {
    if tool == Tool::Pencil { "paint.pencil" } else { "paint.stroke" }
}

/// Windows' crosshair cursor inverts the pixels under it, so over mid-grey (the pasteboard, many
/// photos) it vanishes (#737). There the canvas draws a black-and-white crosshair itself, like
/// Photoshop's, and hides the system one.
pub(super) fn visible_crosshair(icon: egui::CursorIcon, painter: &egui::Painter, p: Pos2, draw: bool) -> egui::CursorIcon {
    if !draw || icon != egui::CursorIcon::Crosshair {
        return icon;
    }
    let p = pos2(p.x.round() + 0.5, p.y.round() + 0.5);
    let (gap, len) = (2.0, 8.0);
    for (w, c) in [(3.0, Color32::from_black_alpha(160)), (1.0, Color32::from_white_alpha(235))] {
        for d in [vec2(1.0, 0.0), vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, -1.0)] {
            painter.line_segment([p + d * gap, p + d * len], Stroke::new(w, c));
        }
    }
    egui::CursorIcon::None
}

/// The Pencil's cursor at `doc` (document pixels): the whole-pixel square its dab fills
/// (`paint::grid_square`), in screen points with its edges on physical pixels (`ppp` = pixels
/// per point), so it lines up with the pixel grid at any zoom.
pub(crate) fn pencil_cursor_rect(xf: &ViewXform, doc: [f64; 2], size: f32, ppp: f32) -> Rect {
    let [x0, y0, x1, y1] = photocraft_engine::paint::grid_square(doc[0], doc[1], size);
    let r = xf.camera().screen_bounds(Rect::from_min_max(pos2(x0 as f32, y0 as f32), pos2(x1 as f32, y1 as f32)));
    let ppp = if ppp.is_finite() && ppp > 0.0 { ppp } else { 1.0 };
    let snap = |v: f32| (v * ppp).round() / ppp;
    Rect::from_min_max(pos2(snap(r.min.x), snap(r.min.y)), pos2(snap(r.max.x), snap(r.max.y)))
}

/// Whether a brush-tip circle draws its centre mark.
/// Clone Stamp and Healing Brush stay an empty circle until Option is held, the source-point
/// cursor. Other brushes follow the cursor preference, the large-tip mark, or the Background Eraser.
pub(super) fn brush_tip_centre(tool: Tool, option: bool, show_crosshair: bool, radius: f32) -> bool {
    if tool == Tool::QuickSelection {
        return false;
    }
    if matches!(tool, Tool::CloneStamp | Tool::Healing) {
        return option || show_crosshair;
    }
    show_crosshair || radius > 6.0 || tool == Tool::BackgroundEraser
}

pub(super) fn begin_live_stroke(app: &PhotocraftApp, settings: Option<&serde_json::Value>) -> Option<LiveStroke> {
    static STROKES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let st = app.session.active()?;
    let d = app.drag.as_ref()?;
    let mut p = stroke_params(app, d.tool, d.erase, &timed_points(app, d, settings));
    if let Some(settings) = settings {
        apply_stroke_settings(&mut p, settings);
    }
    // Pixel work selects the worker; lower-core profiles have a smaller inline budget.
    let profile = lightcraft_sysmem::profile();
    let pixel_budget = (profile.threads.max(1) as f32 * 16_384.0).min(131_072.0);
    let size = p.get("brush").and_then(|b| b.get("size")).and_then(serde_json::Value::as_f64).map_or(app.session.tools.brush.size, |s| s as f32);
    let asynchronous = size.powi(2) > pixel_budget;
    let stroke = if asynchronous {
        photocraft_engine::brush_cmds::LiveStroke::begin_unpainted(&app.session, stroke_command(d.tool), &p)
    } else {
        photocraft_engine::brush_cmds::LiveStroke::begin_with(&app.session, stroke_command(d.tool), &p)
    }
    .ok()?;
    let worker = asynchronous.then(|| StrokeWorker::start(stroke.clone(), profile.threads)).flatten();
    // Thread creation failure retains the established synchronous fallback.
    let mut stroke = stroke;
    if asynchronous && worker.is_none() {
        stroke.push(&photocraft_engine::brush_cmds::parse_points(&p, stroke_command(d.tool)).ok()?).ok()?;
    }
    let fed = if worker.is_some() { 0 } else { d.points.len() };
    let n = STROKES.fetch_add(1, std::sync::atomic::Ordering::Relaxed) & 0xff_ffff;
    let damage_bounds = stroke.bounds();
    let damage = vec![damage_bounds];
    Some(LiveStroke {
        stroke,
        doc: st.doc.id,
        revision: st.revision,
        key: (1 << 44) | (n << 20),
        damage,
        damage_bounds,
        steps: 1,
        first_step: 0,
        fed,
        worker,
        shown_doc: None,
        pending: Vec::new(),
        pending_since: None,
        pending_damage: DRect::EMPTY,
        finish: None,
        queued: Vec::new(),
    })
}

/// Render the drag points the live stroke hasn't seen yet, with the pen pressure, tilt and
/// rotation the commit's `paint.stroke` gets for them.
pub(super) fn feed_live_stroke(app: &mut PhotocraftApp) {
    // A batched move replays several samples in one frame and calls this once at the end, so the
    // per-sample calls from `tool_event` are skipped (see `canvas_view`).
    if app.defer_live_stroke {
        return;
    }
    let airbrush = app.live_stroke.as_ref().is_some_and(|l| l.stroke.uses_build_up());
    let (Some(l), Some(d)) = (app.live_stroke.as_mut(), app.drag.as_ref()) else { return };
    let pose = &app.stylus.stroke;
    let pts: Vec<_> = (l.fed..d.points.len())
        .filter_map(|i| {
            let p = d.points.get(i)?;
            let t = pose.get(i).or(pose.last()).copied().unwrap_or_default();
            let mut sp = photocraft_engine::paint::StrokePoint::new(p[0], p[1], p[2] as f32);
            (sp.tilt_x, sp.tilt_y, sp.rotation) = (t[0], t[1], t[2]);
            if airbrush {
                sp.time = d.times.get(i).copied().unwrap_or(0.0);
            }
            Some(sp)
        })
        .collect();
    if pts.is_empty() {
        return;
    }
    l.fed = d.points.len();
    if l.worker.is_some() {
        if l.pending.is_empty() {
            l.pending_since = Some(std::time::Instant::now());
        }
        l.pending.extend(pts);
        return;
    }
    match l.stroke.push(&pts) {
        Ok(r) => {
            l.steps += 1;
            l.damage_bounds = l.damage_bounds.union(&r);
            if l.damage.len() == 128 {
                l.damage.drain(..64);
                l.first_step += 64;
            }
            l.damage.push(r);
        }
        Err(_) => app.live_stroke = None,
    }
}

/// Poll and publish completed snapshots. Revision/document changes and Escape discard them;
/// the owner document is never changed until the complete stroke is accepted as one command.
pub(super) fn poll_live_stroke(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let escape = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { key: egui::Key::Escape, pressed: true, .. })));
    let invalid = app
        .live_stroke
        .as_ref()
        .is_some_and(|l| app.session.active().is_none_or(|st| st.doc.id != l.doc || st.revision != l.revision) || (app.drag.is_none() && l.finish.is_none()));
    if escape || invalid {
        if app.live_stroke.is_some() {
            app.drag = None;
            app.live_stroke = None;
        }
        return;
    }
    // The first asynchronous batch includes the press, whose points weren't rasterized at begin.
    feed_live_stroke(app);
    let Some(l) = &mut app.live_stroke else { return };
    let Some(worker) = &mut l.worker else { return };
    let received = worker.output.try_recv();
    let mut completed = None;
    match received {
        Ok(Ok(frame)) => {
            worker.busy = false;
            #[cfg(debug_assertions)]
            {
                let mut timing = frame.timing;
                timing.preview_latency_ms = frame._queued_at.elapsed().as_secs_f64() * 1000.0;
                photocraft_engine::paint::profiling::record(timing);
            }
            l.shown_doc = Some(frame.doc);
            l.steps += 1;
            let damage = frame.damage.union(&std::mem::replace(&mut l.pending_damage, DRect::EMPTY));
            l.damage_bounds = l.damage_bounds.union(&damage);
            if l.damage.len() == 128 {
                l.damage.drain(..64);
                l.first_step += 64;
            }
            l.damage.push(damage);
            completed = frame.finished;
        }
        Ok(Err(error)) => {
            app.ui.status = error;
            app.ui.status_error = true;
            app.live_stroke = None;
            app.drag = None;
            return;
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            app.live_stroke = None;
            app.drag = None;
            return;
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {}
    }
    if let Some(mut stroke) = completed {
        let Some(mut l) = app.live_stroke.take() else { return };
        if let Some((cmd, p)) = l.finish.take() {
            if !stroke.bind_commit_params(&app.session, cmd, &p).unwrap_or(false) {
                // Size, colour, target or smoothing can change while the pen is held. The old
                // commit would rerender synchronously; keep that same result on the worker.
                match photocraft_engine::brush_cmds::LiveStroke::begin_unpainted(&app.session, cmd, &p)
                    .and_then(|next| photocraft_engine::brush_cmds::parse_points(&p, cmd).map(|points| (next, points)))
                {
                    Ok((next, points)) => {
                        if let Some(worker) = StrokeWorker::start(next.clone(), lightcraft_sysmem::profile().threads) {
                            l.stroke = next;
                            l.worker = Some(worker);
                            l.pending = points;
                            l.pending_since = Some(std::time::Instant::now());
                            l.pending_damage = l.damage_bounds;
                            l.finish = Some((cmd, p));
                            app.live_stroke = Some(l);
                            ctx.request_repaint();
                            return;
                        }
                        app.ui.status = "Could not finish the brush stroke".into();
                        app.ui.status_error = true;
                    }
                    Err(error) => {
                        app.ui.status = error.to_string();
                        app.ui.status_error = true;
                    }
                }
                return;
            }
            app.session.prepare_live_commit(*stroke);
            let committed = app.run(cmd, p).is_ok();
            app.session.discard_live_commit();
            if committed {
                shown_as_document(app, l.doc, |k| k == 0 || (l.key..=l.key + l.steps).contains(&k));
            }
        }
        let tool = app.ui.tool;
        for event in l.queued {
            if let Some(next) = &mut app.live_stroke
                && next.finish.is_some()
            {
                next.queued.push(event);
                continue;
            }
            if event.tick {
                if let (Some(d), ToolEvent::Move { x, y, pressure }) = (&mut app.drag, event.event) {
                    d.points.push([x, y, pressure as f64]);
                    d.times.push(event.when.saturating_duration_since(d.born).as_secs_f64() * 1000.0);
                    app.stylus.stroke.push(event.pose);
                    feed_live_stroke(app);
                }
                continue;
            }
            app.ui.tool = event.tool;
            app.secondary_erase = event.secondary_erase;
            app.brush_resize_armed = event.resize_armed;
            tool_event_with_pose(app, event.event, event.modifiers, Some(event.pose), event.settings, Some(event.when));
        }
        app.ui.tool = tool;
        ctx.request_repaint();
        return;
    }
    if !worker.busy && (!l.pending.is_empty() || l.finish.is_some()) {
        let work = StrokeWork {
            queued_at: l.pending_since.take().unwrap_or_else(std::time::Instant::now),
            points: std::mem::take(&mut l.pending),
            finish: l.finish.is_some(),
            ctx: ctx.clone(),
        };
        match worker.input.try_send(work) {
            Ok(()) => worker.busy = true,
            Err(std::sync::mpsc::TrySendError::Full(work)) => {
                l.pending_since = Some(work.queued_at);
                l.pending = work.points;
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                app.live_stroke = None;
                app.drag = None;
                return;
            }
        }
    }
    if worker.busy {
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

/// A following press is already a held gesture even while its predecessor finishes.
pub(super) fn queued_stroke_point(app: &PhotocraftApp) -> Option<[f64; 3]> {
    let l = app.live_stroke.as_ref().filter(|l| l.finish.is_some())?;
    let boundary = l.queued.iter().rposition(|e| matches!(e.event, ToolEvent::Down { .. } | ToolEvent::Up { .. }))?;
    if !matches!(l.queued[boundary].event, ToolEvent::Down { .. }) {
        return None;
    }
    l.queued[boundary..].iter().rev().find_map(|e| match e.event {
        ToolEvent::Down { x, y, pressure } | ToolEvent::Move { x, y, pressure } => Some([x, y, pressure as f64]),
        ToolEvent::Up { .. } => None,
    })
}

/// Tools whose gesture follows a freehand path (a polyline of the input points), so every pointer
/// sample the OS delivered improves the result. Other tools are driven by the pointer's latest
/// position (endpoints, anchors, guides), so feeding them a whole frame's moves only repeats work.
pub(crate) fn freehand_tool(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::AiRemove
            | Tool::AiCutout
            | Tool::Brush
            | Tool::Pencil
            | Tool::MixerBrush
            | Tool::Eraser
            | Tool::BackgroundEraser
            | Tool::HistoryBrush
            | Tool::SpotHealing
            | Tool::Healing
            | Tool::CloneStamp
            | Tool::Blur
            | Tool::Sharpen
            | Tool::Smudge
            | Tool::Dodge
            | Tool::Burn
            | Tool::Sponge
            | Tool::Lasso
            | Tool::MagneticLasso
            | Tool::Patch
            | Tool::ContentAwareMove
            | Tool::QuickSelection
    )
}
