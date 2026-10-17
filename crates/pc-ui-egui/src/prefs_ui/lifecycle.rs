//! Preferences lifecycle: loading and recovery, background persistence with retry and merge,
//! autosave and crash recovery, and the history log.
use super::*;

// Lifecycle

pub(super) fn load_prefs_text(services: &mut crate::Services) -> Result<Option<String>, crate::PrefsLoadError> {
    if let Some(load) = services.load_prefs_checked.as_mut() { load() } else { Ok(services.load_prefs.as_mut().and_then(|load| load())) }
}

pub(super) fn show_prefs_recovery(app: &mut PhotocraftApp) {
    let Some(recovery) = app.services.prefs_recovery.as_mut().and_then(|load| load()) else { return };
    let key = format!("ui.preferencesRecoveryDismissed:{}", recovery.backup_path);
    if app.session.prefs().dialogs.get(&key).and_then(Value::as_bool) == Some(true)
        || app.ui.notices.iter().any(|notice| notice.dismiss_pref.as_deref() == Some(key.as_str()))
    {
        return;
    }
    crate::notices::post(app, "Original preferences preserved", vec![recovery.reason, format!("Backup: {}", recovery.backup_path)], true, Some(&key));
}

/// Load saved preferences (once) and recover autosaved documents. Called when the app is
/// created.
pub fn load(app: &mut PhotocraftApp) {
    if app.prefs_rt.loaded {
        return;
    }
    app.prefs_rt.loaded = true;
    let mut repaired = false;
    let loaded = load_prefs_text(&mut app.services).and_then(|text| match text {
        Some(text) => app.session.load_prefs_json(&text).map_err(crate::PrefsLoadError::Damaged),
        None => Ok(()),
    });
    if let Err(error) = loaded {
        let unreadable = matches!(error, crate::PrefsLoadError::Unreadable(_));
        if unreadable {
            app.prefs_rt.unreadable_base = Some(app.session.prefs_value());
        } else {
            repaired = true;
        }
        let error = error.to_string();
        app.ui.status = li_i18n::fmt(tl!("Some preferences used defaults: {error}"), &[("error", &error)]);
        let notice = crate::notices::post(
            app,
            if unreadable { "Preferences couldn't be read" } else { "Preferences need repair" },
            vec![
                error,
                if unreadable {
                    "Preference changes wait until the original can be reloaded.".into()
                } else {
                    "The original must be preserved before repaired preferences can replace it.".into()
                },
            ],
            true,
            None,
        );
        if unreadable {
            app.prefs_rt.unreadable_notice = Some(notice);
        }
    }
    show_prefs_recovery(app);
    crate::dock::restore(app);
    app.sync_recent();
    app.prefs_rt.saved_rev = if repaired { 0 } else { app.session.prefs.rev() };
    app.prefs_rt.saved_value = if repaired { None } else { Some(app.session.prefs_value()) };
    if app.session.prefs().file_handling.recover_on_launch
        && let Some(recover) = app.services.recover.as_mut()
    {
        let docs = recover();
        let n = docs.len();
        for r in docs {
            app.session.add_document(r.doc, r.path);
            // Recovered documents are unsaved.
            let Some(st) = app.session.active_mut() else { continue };
            st.saved_revision = 0;
            // Their entry already holds this revision: it stays until a newer autosave replaces
            // it or the document is saved or closed (see `autosave`).
            let (id, revision) = (st.doc.id, st.revision);
            app.prefs_rt.autosaved.insert(id, (revision, std::sync::Arc::downgrade(&st.doc)));
            if let Some(adopt) = app.services.adopt_autosave.as_mut() {
                adopt(id.0, &r.key);
            }
        }
        if n > 0 {
            app.sync_views();
            app.ui.status = format!("Recovered {n} document{}", if n == 1 { "" } else { "s" });
        }
    }
}

/// Attach the brush preset store once its background load finishes, and surface write
/// failures in the status bar.
pub(super) fn presets_store(app: &mut PhotocraftApp) {
    if let Some(rx) = &app.services.preset_store {
        match rx.try_recv() {
            Ok(opened) => {
                app.services.preset_store = None;
                let warnings = app.session.attach_preset_store(opened);
                if let Some(w) = warnings.first() {
                    app.ui.status = if warnings.len() == 1 { w.clone() } else { format!("{w} (+{} more brush preset warnings)", warnings.len() - 1) };
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => app.services.preset_store = None,
        }
    }
    if let Some(w) = app.session.preset_store.as_mut().map(|s| s.take_warnings()).and_then(|w| w.into_iter().next()) {
        app.ui.status = w;
    }
}

/// Resolve an absolute pixels-per-point preference without multiplying the OS DPI twice.
/// Monitor dimensions are physical pixels, independent of window size and UI zoom.
pub(super) fn display_scale(pref: prefs::UiScale, native: Option<f32>, monitor_px: Option<egui::Vec2>) -> f32 {
    let native = native.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(1.0);
    match pref {
        prefs::UiScale::Auto => {
            // A 4K display needs at least 200%; preserve larger system scales.
            let is_4k = monitor_px.is_some_and(|s| s.x.is_finite() && s.y.is_finite() && s.x.min(s.y) >= 2160.0 && s.x.max(s.y) >= 3840.0);
            if is_4k { native.max(2.0) } else { native }
        }
        fixed => fixed.name().parse::<f32>().map_or(1.0, |pct| pct / 100.0),
    }
}

pub(super) fn sync_display_scale(app: &PhotocraftApp, ctx: &egui::Context) {
    let native = ctx.native_pixels_per_point();
    // ViewportInfo uses egui points, including the current UI zoom. Converting back to
    // physical pixels avoids oscillating between 100% and 200% on successive frames.
    // logic() can receive new viewport DPI before InputState::pixels_per_point updates.
    let native_scale = native.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(1.0);
    let monitor_px = ctx.input(|i| i.viewport().monitor_size).map(|s| s * (native_scale * ctx.zoom_factor()));
    let scale = display_scale(app.session.prefs().interface.ui_scale, native, monitor_px);
    ctx.set_zoom_factor(scale / native_scale);
}

/// Interface › Show Tooltips and Tools › Show Tooltips (either one off hides them): egui never
/// shows a tooltip whose delay is infinite. Re-checked every frame because a theme change
/// rebuilds the style; that's one style read, and a write only when it differs.
pub fn sync_shared_tooltips(app: &PhotocraftApp, ctx: &egui::Context) {
    let p = app.session.prefs();
    let delay = if p.interface.show_tooltips && p.tools.show_tooltips { crate::theme::TOOLTIP_DELAY } else { f32::INFINITY };
    if ctx.global_style().interaction.tooltip_delay != delay {
        ctx.global_style_mut(|s| s.interaction.tooltip_delay = delay);
    }
}

/// Reconcile application-wide Interface preferences in every module. The caller applies its
/// own palette when the returned theme changes; hosted Library/Develop keep their visual profile.
pub fn sync_shared_interface(app: &mut PhotocraftApp, ctx: &egui::Context) -> bool {
    let before = app.ui.theme;
    sync_display_scale(app, ctx);
    li_ui_kit::color_icons::set_enabled(ctx, app.session.prefs().interface.tool_icons != "monochrome");
    // Interface theme: a preference change applies to the UI; a theme picked from the Window
    // menu is stored as the preference.
    let pref = app.session.prefs().interface.theme;
    if app.prefs_rt.theme_pref != Some(pref) {
        app.prefs_rt.theme_pref = Some(pref);
        if app.ui.theme != theme_kind(pref) {
            app.ui.theme = theme_kind(pref);
            app.checker = None;
        }
    } else if theme_pref(app.ui.theme) != pref {
        let t = theme_pref(app.ui.theme);
        app.session.prefs.edit(|p| p.interface.theme = t);
        app.prefs_rt.theme_pref = Some(t);
    }
    crate::theme::set_ui_font_size(ctx, app.session.prefs().interface.ui_font_size);
    crate::theme::sync_language(ctx, &app.session.prefs().interface.language);
    before != app.ui.theme
}

/// Per-frame upkeep: theme sync, persistence, autosave and the history log.
pub fn tick(app: &mut PhotocraftApp, ctx: &egui::Context) {
    sync_ai_preferences(app);
    if !app.prefs_rt.loaded {
        load(app);
    }
    if sync_shared_interface(app, ctx) {
        app.restyle(ctx);
    }
    presets_store(app);
    sync_shared_tooltips(app, ctx);
    app.sync_recent();
    if let Some(wait) = persist(app, ctx.input(|i| i.time)) {
        ctx.request_repaint_after(std::time::Duration::try_from_secs_f64(wait).unwrap_or_default());
    }
    let style = canvas_style(app);
    if let Some(gpu) = app.gpu.as_ref()
        && app.prefs_rt.gpu_style != Some(style)
    {
        gpu.set_style(style);
        app.prefs_rt.gpu_style = Some(style);
    }
    autosave(app);
    history_log(app);
}

/// First retry delay after a failed preferences write, in seconds; it doubles with every further
/// failure up to [`SAVE_RETRY_MAX_S`]. A new change retries sooner, but no sooner than this.
pub(super) const SAVE_RETRY_S: f64 = 2.0;
pub(super) const SAVE_RETRY_MAX_S: f64 = 30.0;
/// Failed writes in a row before a notice says preferences aren't being saved.
pub(super) const SAVE_NOTICE_AFTER: u32 = 3;

pub(super) fn sync_ai_preferences(app: &mut PhotocraftApp) {
    let revision = li_ai::settings::revision();
    if app.prefs_rt.ai_revision != revision {
        let settings = li_ai::AiSettings::load();
        if app.session.prefs().ai != settings {
            app.session.edit_prefs(|prefs| prefs.ai = settings);
        }
        app.prefs_rt.ai_revision = revision;
    }
}

/// Persist hosted edits while Library/Develop is visible, without applying editor visuals.
pub fn persist_background(app: &mut PhotocraftApp, ctx: &egui::Context) {
    sync_ai_preferences(app);
    if let Some(wait) = persist(app, ctx.input(|i| i.time)) {
        ctx.request_repaint_after(std::time::Duration::try_from_secs_f64(wait).unwrap_or_default());
    }
}

/// Retry state of a failed preferences write (times are egui input time, in seconds).
#[derive(Default)]
pub(super) struct SaveRetry {
    pub(super) failures: u32,
    /// When the next retry is due.
    pub(super) at: f64,
    /// When the last write was tried, and the revision it carried.
    pub(super) tried_at: f64,
    pub(super) tried_rev: u64,
    /// The "not saved" notice, while it's shown.
    pub(super) notice: Option<u64>,
}

/// Recover an initial I/O failure by loading the real preferences and replaying only
/// edits made since that failed load. Session values and the merge baseline move together.
pub(super) fn rebase_unreadable_prefs(app: &mut PhotocraftApp) -> Result<(), String> {
    let Some(base) = app.prefs_rt.unreadable_base.clone() else { return Ok(()) };
    let ours = app.session.prefs_value();
    match load_prefs_text(&mut app.services) {
        Err(crate::PrefsLoadError::Unreadable(error)) => return Err(error),
        Err(crate::PrefsLoadError::Damaged(_)) => app.prefs_rt.saved_value = None,
        Ok(None) => app.prefs_rt.saved_value = Some(base),
        Ok(Some(text)) => {
            let repaired = app.session.load_prefs_json(&text).is_err();
            let stored_base = app.session.prefs_value();
            let mut rebased = stored_base.clone();
            merge_changes(&base, &ours, &mut rebased);
            app.session.load_prefs_json(&serde_json::to_string(&rebased).map_err(|error| error.to_string())?)?;
            app.prefs_rt.saved_value = if repaired { None } else { Some(stored_base) };
        }
    }
    app.prefs_rt.unreadable_base = None;
    if let Some(id) = app.prefs_rt.unreadable_notice.take() {
        app.ui.notices.retain(|notice| notice.id != id);
    }
    Ok(())
}

/// Save changed preferences. A failed write stays pending and is retried with a backoff (a newer
/// change retries sooner); the status bar reports the first failure and a notice a persistent
/// one. Returns the seconds until the next retry while one is pending.
pub(super) fn persist(app: &mut PhotocraftApp, now: f64) -> Option<f64> {
    let rev = app.session.prefs.rev();
    if rev == app.prefs_rt.saved_rev && app.prefs_rt.unreadable_base.is_none() {
        return None;
    }
    let r = &app.prefs_rt.save_retry;
    if r.failures > 0 {
        let due = if rev == r.tried_rev { r.at } else { r.at.min(r.tried_at + SAVE_RETRY_S) };
        if now < due {
            return Some(due - now);
        }
    }
    let rebased = app.prefs_rt.unreadable_base.is_some();
    let written = rebase_unreadable_prefs(app).and_then(|()| {
        let ours = app.session.prefs_value();
        // An undone change leaves nothing to write; a recovered read may also need no write.
        if app.prefs_rt.saved_value.as_ref() == Some(&ours) { Ok(()) } else { write_prefs(app, &ours) }
    });
    show_prefs_recovery(app);
    match written {
        Ok(()) => {
            app.prefs_rt.saved_rev = app.session.prefs.rev();
            app.prefs_rt.saved_value = Some(app.session.prefs_value());
            let r = std::mem::take(&mut app.prefs_rt.save_retry);
            if r.failures > 0 || rebased {
                app.ui.status = if rebased { "Preferences reloaded".into() } else { "Preferences saved".into() };
                app.ui.status_error = false;
                if let Some(id) = r.notice {
                    app.ui.notices.retain(|n| n.id != id);
                }
            }
            rebased.then_some(0.0) // apply any newly loaded visual preferences on the next frame
        }
        Err(e) => {
            let r = &mut app.prefs_rt.save_retry;
            r.failures = r.failures.saturating_add(1);
            (r.tried_at, r.tried_rev) = (now, rev);
            let delay = (SAVE_RETRY_S * 2f64.powi(r.failures.min(16) as i32 - 1)).min(SAVE_RETRY_MAX_S);
            r.at = now + delay;
            let failures = r.failures;
            if failures == 1 {
                app.ui.status = format!("Couldn't save preferences: {e}. Retrying…");
                app.ui.status_error = true;
            } else if failures == SAVE_NOTICE_AFTER {
                let lines = vec![e, "Local Image keeps retrying; until a save succeeds, preference changes are lost when it closes.".into()];
                let id = crate::notices::post(app, "Preferences can't be saved", lines, true, None);
                app.prefs_rt.save_retry.notice = Some(id);
            }
            Some(delay)
        }
    }
}

/// Write `ours` over the stored preferences, keeping what another instance (a second browser
/// tab, another app window) changed there since this one last loaded or saved them.
pub(super) fn write_prefs(app: &mut PhotocraftApp, ours: &Value) -> Result<(), String> {
    if app.services.save_prefs.is_none() {
        return Ok(());
    }
    let stored = match load_prefs_text(&mut app.services) {
        Ok(text) => text.and_then(|text| serde_json::from_str::<Value>(&text).ok()),
        Err(crate::PrefsLoadError::Damaged(_)) => None, // native save must back up readable damaged bytes
        Err(crate::PrefsLoadError::Unreadable(error)) => return Err(error),
    };
    let text = match (stored, &app.prefs_rt.saved_value) {
        (Some(mut theirs @ Value::Object(_)), Some(base)) => {
            merge_changes(base, ours, &mut theirs);
            serde_json::to_string_pretty(&theirs)
        }
        _ => serde_json::to_string_pretty(ours),
    }
    .map_err(|e| e.to_string())?;
    app.services.save_prefs.as_mut().map_or(Ok(()), |save| save(&text))
}

/// Three-way merge of preference trees: whatever `ours` changed since `base` goes over `theirs`
/// (what storage holds now) and the rest of `theirs` is kept. Objects merge key by key; any
/// other value, arrays included, is replaced whole.
pub(super) fn merge_changes(base: &Value, ours: &Value, theirs: &mut Value) {
    if ours == base {
        return;
    }
    match (base, ours, theirs) {
        (Value::Object(b), Value::Object(o), Value::Object(t)) => {
            for (k, ov) in o {
                match (b.get(k), t.get_mut(k)) {
                    (Some(bv), Some(tv)) => merge_changes(bv, ov, tv),
                    // Removed there, unchanged here: it stays removed.
                    (Some(bv), None) if bv == ov => {}
                    _ => {
                        t.insert(k.clone(), ov.clone());
                    }
                }
            }
            for k in b.keys().filter(|k| !o.contains_key(*k)) {
                t.remove(k);
            }
        }
        (_, _, theirs) => *theirs = ours.clone(),
    }
}

/// Write the preferences now, through the same merge as [`persist`], for recovery choices that
/// need the result immediately. The pending revision stays unsaved when the write fails.
pub(crate) fn save_preferences(app: &mut PhotocraftApp) -> Result<(), String> {
    let rev = app.session.prefs.rev();
    let ours = app.session.prefs_value();
    write_prefs(app, &ours)?;
    app.prefs_rt.saved_rev = rev;
    app.prefs_rt.saved_value = Some(ours);
    if let Some(id) = std::mem::take(&mut app.prefs_rt.save_retry).notice {
        app.ui.notices.retain(|n| n.id != id);
    }
    Ok(())
}

/// Final shutdown may follow Save/Close without another autosave tick. Remove only
/// closed/clean documents, or every document after the complete quit prompt was accepted.
/// If shutdown happens without consent, dirty documents retain their recovery copies.
pub(crate) fn cleanup_on_exit(app: &mut PhotocraftApp) {
    let live: HashMap<DocId, bool> = app.session.documents().iter().map(|d| (d.doc.id, d.is_dirty())).collect();
    let mut ids: Vec<DocId> = app.prefs_rt.autosaved.keys().chain(app.prefs_rt.autosave_pending.keys()).chain(live.keys()).copied().collect();
    ids.sort_by_key(|id| id.0);
    ids.dedup();
    for id in ids {
        if app.allow_close || live.get(&id) != Some(&true) {
            if let Some(discard) = app.services.discard_autosave.as_mut() {
                discard(id.0);
            }
            app.prefs_rt.autosaved.remove(&id);
            app.prefs_rt.autosave_pending.remove(&id);
        }
    }
    if let Some(finish) = app.services.finish_autosave_cleanup.as_mut() {
        finish();
    }
}

/// Background autosave of documents with unsaved changes every N minutes (File Handling).
pub(super) fn autosave(app: &mut PhotocraftApp) {
    if app.allow_close {
        return;
    }
    let fh = &app.session.prefs().file_handling;
    let (on, minutes) = (fh.autosave, fh.autosave_minutes.max(1));
    if app.services.autosave.is_none() {
        return;
    }
    let now = crate::gpu_canvas::now_ms();
    // A queued autosave is not a written one. Poll every frame, including frames before the
    // next autosave interval, so failures are visible and the same revision is retried.
    if let Some(results) = app.services.autosave_results.as_mut() {
        for (raw_id, revision, result) in results() {
            let id = DocId(raw_id);
            if app.prefs_rt.autosave_pending.get(&id).is_none_or(|(pending, _)| *pending != revision) {
                continue;
            }
            let Some(written) = app.prefs_rt.autosave_pending.remove(&id) else { continue };
            match result {
                Ok(()) => {
                    app.prefs_rt.autosaved.insert(id, written);
                }
                Err(e) => autosave_failed(app, &e),
            }
        }
    }
    // Saved or closed documents drop their recovery data.
    let live: HashMap<DocId, bool> = app.session.documents().iter().map(|d| (d.doc.id, d.is_dirty())).collect();
    let mut stale: Vec<DocId> = app.prefs_rt.autosaved.keys().filter(|id| live.get(id) != Some(&true)).copied().collect();
    stale.extend(app.prefs_rt.autosave_pending.keys().filter(|id| live.get(*id) != Some(&true) && !app.prefs_rt.autosaved.contains_key(*id)).copied());
    for id in stale {
        app.prefs_rt.autosaved.remove(&id);
        app.prefs_rt.autosave_pending.remove(&id);
        if let Some(d) = app.services.discard_autosave.as_mut() {
            d(id.0);
        }
    }
    if !on {
        return;
    }
    let interval = minutes as f64 * 60_000.0;
    if app.prefs_rt.next_autosave_ms == 0.0 || app.prefs_rt.next_autosave_ms > now + interval {
        app.prefs_rt.next_autosave_ms = now + interval;
        return;
    }
    if now < app.prefs_rt.next_autosave_ms {
        return;
    }
    app.prefs_rt.next_autosave_ms = now + interval;
    let jobs: Vec<_> = app
        .session
        .documents()
        .iter()
        .filter(|d| {
            d.is_dirty()
                && !app.prefs_rt.autosave_pending.contains_key(&d.doc.id)
                && !app
                    .prefs_rt
                    .autosaved
                    .get(&d.doc.id)
                    .is_some_and(|(revision, snapshot)| *revision == d.revision && snapshot.ptr_eq(&std::sync::Arc::downgrade(&d.doc)))
        })
        .map(|d| (d.doc.clone(), d.revision, d.path.clone()))
        .collect();
    for (doc, rev, path) in jobs {
        if let Some(save) = app.services.autosave.as_mut() {
            match save(&doc, rev, path.as_deref()) {
                Ok(()) if app.services.autosave_results.is_some() => {
                    app.prefs_rt.autosave_pending.insert(doc.id, (rev, std::sync::Arc::downgrade(&doc)));
                }
                // Synchronous or mocked services report completion on return.
                Ok(()) => {
                    app.prefs_rt.autosaved.insert(doc.id, (rev, std::sync::Arc::downgrade(&doc)));
                }
                Err(e) => autosave_failed(app, &e),
            }
        }
    }
}

/// An autosave couldn't be queued or written: say so in the status bar, and keep one notice up
/// (not one per document or interval) until the user dismisses it, since crash recovery is off.
pub(super) fn autosave_failed(app: &mut PhotocraftApp, error: &str) {
    app.ui.status = format!("Autosave failed: {error}");
    app.ui.status_error = true;
    let title = tl!("Autosave failed");
    if !app.ui.notices.iter().any(|n| n.title == title) {
        crate::notices::post(
            app,
            title,
            vec![error.to_string(), tl!("Crash recovery can't keep a copy of your unsaved changes. Save your work to keep it.").to_string()],
            true,
            None,
        );
    }
}

/// Force the autosave timer to fire on the next tick (tests, `prefs` changes).
pub fn autosave_now(app: &mut PhotocraftApp) {
    app.prefs_rt.next_autosave_ms = f64::MIN_POSITIVE;
}

/// Queue changed recovery snapshots immediately after a source-preservation transaction.
/// The native store serializes/coalesces requests per document, so an older in-flight
/// write cannot land after the new snapshot. Still respects the autosave preference.
pub fn refresh_recovery(app: &mut PhotocraftApp) {
    autosave_now(app);
    autosave(app);
}

/// History Log preference: append executed commands to a text file.
pub(super) fn history_log(app: &mut PhotocraftApp) {
    let n = app.session.journal.len();
    if n <= app.prefs_rt.log_len {
        app.prefs_rt.log_len = n;
        return;
    }
    let hl = app.session.prefs().history_log.clone();
    let start = app.prefs_rt.log_len;
    app.prefs_rt.log_len = n;
    if !hl.enabled || hl.file_path.is_empty() || hl.destination == prefs::LogDestination::Metadata {
        return;
    }
    let Some(append) = app.services.append_text.as_mut() else { return };
    let mut text = String::new();
    for (id, params) in &app.session.journal[start..] {
        let label = photocraft_engine::commands::find(id).map_or(id.as_str(), |c| c.label).trim_end_matches('…');
        match hl.detail {
            prefs::LogDetail::SessionsOnly => {
                if matches!(id.as_str(), "file.new" | "file.open" | "file.close" | "file.openAs") {
                    text.push_str(&format!("{label}\n"));
                }
            }
            prefs::LogDetail::Concise => text.push_str(&format!("{label}\n")),
            prefs::LogDetail::Detailed => text.push_str(&format!("{label}  {id} {params}\n")),
        }
    }
    if !text.is_empty() {
        let _ = append(&hl.file_path, &text);
    }
}
