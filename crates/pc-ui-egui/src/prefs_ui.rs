//! Edit › Preferences, Keyboard Shortcuts and Menus, Color Settings and the rest of the Edit
//! menu's dialogs, plus the shell side of preferences: loading and saving them through
//! [`crate::Services`], applying the interface theme, autosave and crash recovery, and the
//! history log.
//!
//! The values live in the engine ([`photocraft_engine::prefs::Preferences`], so agents read and
//! change them with `prefs.get` / `prefs.set`); this module only edits a working copy in a dialog
//! and commits it with those commands on Apply or OK.

use std::collections::{BTreeMap, HashMap};

use egui::{Color32, RichText, vec2};
use photocraft_doc::DocId;
use photocraft_engine::prefs::{self, SECTIONS, Theme};
use photocraft_engine::snap::{SnapLine, SnapTargets};
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;
use crate::state::DialogKind;
use crate::theme::{ThemeKind, Tokens};

/// Extra preference pages supplied by the app host. The editor provides the only modal shell,
/// navigation and Apply/OK/Cancel actions. A host keeps the draft as JSON and owns its validation.
/// Catalog maintenance actions can remain immediate, with an explicit notice in their page.
pub trait HostPreferencesPages {
    fn pages(&self) -> &'static [(&'static str, &'static str)];
    fn values(&self) -> Value;
    fn body(&mut self, ui: &mut egui::Ui, section: &str, draft: &mut Value);
    fn validate(&self, draft: &Value) -> Result<(), String>;
    fn apply(&mut self, draft: &Value) -> Result<(), String>;
}

/// Shell runtime state for preferences, autosave and snapping (not serialised).
#[derive(Default)]
pub struct Runtime {
    loaded: bool,
    /// Preferences revision last written to storage (or loaded from it).
    saved_rev: u64,
    ai_revision: u64,
    /// This instance's preferences as of `saved_rev`: the base of the three-way merge on save.
    saved_value: Option<Value>,
    /// Defaults/current state before an initial I/O failure; local edits are rebased
    /// onto the first readable original, never saved over it as a default snapshot.
    unreadable_base: Option<Value>,
    unreadable_notice: Option<u64>,
    save_retry: SaveRetry,
    theme_pref: Option<Theme>,
    next_autosave_ms: f64,
    // Resource preservation can replace a snapshot without advancing its edit revision.
    // Weak identity detects those changes without retaining another large document.
    autosaved: HashMap<DocId, (u64, std::sync::Weak<photocraft_doc::Document>)>,
    /// Autosaves queued on a background writer that hasn't reported yet. A queued write is not a
    /// durable snapshot: it moves to `autosaved` only once the write succeeded (upstream
    /// PhotoCraft 83d5c543).
    autosave_pending: HashMap<DocId, (u64, std::sync::Weak<photocraft_doc::Document>)>,
    log_len: usize,
    /// Snapping state of the drag in progress (see `snap_ui`).
    pub(crate) snap: Option<crate::snap_ui::ActiveSnap>,
    pub(crate) snap_lines: Vec<SnapLine>,
    pub(crate) guide_targets: Option<((DocId, u64), SnapTargets)>,
    /// Checkerboard colours the cached CPU checker texture was built with.
    pub(crate) checker_key: Option<[[u8; 3]; 2]>,
    /// Style last sent to the GPU canvas.
    pub(crate) gpu_style: Option<crate::gpu_canvas::CanvasStyle>,
}

/// The GPU canvas colours from Preferences › Transparency & Gamut.
pub fn canvas_style(app: &PhotocraftApp) -> crate::gpu_canvas::CanvasStyle {
    let t = &app.session.prefs().transparency_and_gamut;
    let [l, d] = t.colors();
    let f = |c: [u8; 3]| c.map(|v| v as f32 / 255.0);
    let g = prefs::parse_hex(&t.gamut_warning_color).unwrap_or([128; 3]);
    crate::gpu_canvas::CanvasStyle {
        checker_square: t.square().unwrap_or(0.0),
        checker_light: f(l),
        checker_dark: f(d),
        gamut_color: f(g),
        gamut_opacity: t.gamut_warning_opacity as f32 / 100.0,
    }
}

/// Pasteboard colour from Preferences › Interface (`None` = the theme's default canvas).
pub fn pasteboard_color(app: &PhotocraftApp) -> Option<Color32> {
    use prefs::CanvasColor;
    let i = &app.session.prefs().interface;
    Some(match i.canvas_color {
        CanvasColor::Default => return None,
        CanvasColor::Black => Color32::BLACK,
        CanvasColor::DarkGray => Color32::from_gray(40),
        CanvasColor::MediumGray => Color32::from_gray(83),
        CanvasColor::LightGray => Color32::from_gray(184),
        CanvasColor::Custom => color_of(&i.canvas_custom_color),
    })
}

fn theme_kind(t: Theme) -> ThemeKind {
    match t {
        Theme::Pro => ThemeKind::Pro,
        Theme::ProMedium => ThemeKind::ProMedium,
        Theme::Studio => ThemeKind::Studio,
        Theme::StudioLight => ThemeKind::StudioLight,
        Theme::Classic => ThemeKind::Classic,
    }
}

fn theme_pref(k: ThemeKind) -> Theme {
    match k {
        ThemeKind::Pro => Theme::Pro,
        ThemeKind::ProMedium => Theme::ProMedium,
        ThemeKind::Studio => Theme::Studio,
        ThemeKind::StudioLight => Theme::StudioLight,
        ThemeKind::Classic => Theme::Classic,
    }
}

// ------------------------------------------------------------------ lifecycle

fn load_prefs_text(services: &mut crate::Services) -> Result<Option<String>, crate::PrefsLoadError> {
    if let Some(load) = services.load_prefs_checked.as_mut() { load() } else { Ok(services.load_prefs.as_mut().and_then(|load| load())) }
}

fn show_prefs_recovery(app: &mut PhotocraftApp) {
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
fn presets_store(app: &mut PhotocraftApp) {
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
fn display_scale(pref: prefs::UiScale, native: Option<f32>, monitor_px: Option<egui::Vec2>) -> f32 {
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

fn sync_display_scale(app: &PhotocraftApp, ctx: &egui::Context) {
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
fn sync_tooltips(app: &PhotocraftApp, ctx: &egui::Context) {
    let p = app.session.prefs();
    let delay = if p.interface.show_tooltips && p.tools.show_tooltips { crate::theme::TOOLTIP_DELAY } else { f32::INFINITY };
    if ctx.global_style().interaction.tooltip_delay != delay {
        ctx.global_style_mut(|s| s.interaction.tooltip_delay = delay);
    }
}

/// Per-frame upkeep: theme sync, persistence, autosave and the history log.
pub fn tick(app: &mut PhotocraftApp, ctx: &egui::Context) {
    sync_ai_preferences(app);
    if !app.prefs_rt.loaded {
        load(app);
    }
    sync_display_scale(app, ctx);
    crate::color_icons::set_enabled(ctx, app.session.prefs().interface.tool_icons != "monochrome");
    // Interface theme: a preference change applies to the UI; a theme picked from the Window
    // menu is stored as the preference.
    let pref = app.session.prefs().interface.theme;
    if app.prefs_rt.theme_pref != Some(pref) {
        app.prefs_rt.theme_pref = Some(pref);
        if app.ui.theme != theme_kind(pref) {
            app.set_theme(ctx, theme_kind(pref));
        }
    } else if theme_pref(app.ui.theme) != pref {
        let t = theme_pref(app.ui.theme);
        app.session.prefs.edit(|p| p.interface.theme = t);
        app.prefs_rt.theme_pref = Some(t);
    }
    crate::theme::set_ui_font_size(ctx, app.session.prefs().interface.ui_font_size);
    presets_store(app);
    sync_tooltips(app, ctx);
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
const SAVE_RETRY_S: f64 = 2.0;
const SAVE_RETRY_MAX_S: f64 = 30.0;
/// Failed writes in a row before a notice says preferences aren't being saved.
const SAVE_NOTICE_AFTER: u32 = 3;

fn sync_ai_preferences(app: &mut PhotocraftApp) {
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
struct SaveRetry {
    failures: u32,
    /// When the next retry is due.
    at: f64,
    /// When the last write was tried, and the revision it carried.
    tried_at: f64,
    tried_rev: u64,
    /// The "not saved" notice, while it's shown.
    notice: Option<u64>,
}

/// Recover an initial I/O failure by loading the real preferences and replaying only
/// edits made since that failed load. Session values and the merge baseline move together.
fn rebase_unreadable_prefs(app: &mut PhotocraftApp) -> Result<(), String> {
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
fn persist(app: &mut PhotocraftApp, now: f64) -> Option<f64> {
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
fn write_prefs(app: &mut PhotocraftApp, ours: &Value) -> Result<(), String> {
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
fn merge_changes(base: &Value, ours: &Value, theirs: &mut Value) {
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
fn autosave(app: &mut PhotocraftApp) {
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
fn autosave_failed(app: &mut PhotocraftApp, error: &str) {
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
fn history_log(app: &mut PhotocraftApp) {
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

// ------------------------------------------------------------------ shortcuts

pub use crate::shortcuts::{default_shortcut, effective_shortcut};

/// Every shortcut-bearing command (engine registry plus shell commands): (id, label, menu path,
/// default shortcut), menu items first in Photoshop order.
pub fn shortcut_items(app: &PhotocraftApp) -> Vec<(String, String, Vec<String>, Option<String>)> {
    let mut out: Vec<(String, String, Vec<String>, Option<String>)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for it in crate::menus::menu_items(app) {
        if it.label == "---" || !seen.insert(it.id.clone()) {
            continue;
        }
        let default = default_shortcut(&it.id)
            .or(it.shortcut.clone().filter(|_| !app.session.prefs().shortcut_overrides(it.id.starts_with("library::")).contains_key(&it.id)));
        out.push((it.id, it.label, it.path, default));
    }
    // Commands without a menu item: the colour and fill keys (D, X, ⌥⌫…) sit under Tools, as
    // Photoshop lists its colour keys in the Tools shortcut set, after the rest; then the held
    // temporary tools (#249).
    let tools_key = |id: &str| id.starts_with("tools.") || photocraft_engine::fill_key_cmds::IDS.contains(&id);
    let mut tools = Vec::new();
    let mut layer = Vec::new();
    for c in photocraft_engine::command_specs() {
        if seen.insert(c.id.to_string()) && c.shortcut.is_some() {
            let item = (c.id.to_string(), c.label.to_string(), c.menu.iter().map(|s| s.to_string()).collect::<Vec<_>>(), c.shortcut.map(Into::into));
            if c.menu.is_empty() && tools_key(c.id) {
                tools.push((item.0, item.1, vec!["Tools".to_string()], item.3));
            } else if c.menu.is_empty() && c.id.starts_with("layer.") {
                // Stamp Visible / Stamp Down (#217): no menu item in Photoshop; listed at the end
                // of the Layer section.
                layer.push((item.0, item.1, vec!["Layer".to_string()], item.3));
            } else {
                out.push(item);
            }
        }
    }
    let at = out.iter().rposition(|i| i.2.first().map(String::as_str) == Some("Layer")).map_or(out.len(), |i| i + 1);
    out.splice(at..at, layer);
    out.extend(tools);
    for (id, label, def) in prefs::TOOL_SHORTCUTS {
        out.push((id.to_string(), label.to_string(), vec!["Tools".into()], def.map(str::to_string)));
    }
    for tool in photocraft_algo::liquify::LiquifyTool::ALL {
        out.push((
            crate::liquify_ui::shortcut_id(tool).to_owned(),
            tool.label().to_string(),
            vec!["Tools".into(), "Liquify".into()],
            (!crate::liquify_ui::shortcut(tool).is_empty()).then(|| crate::liquify_ui::shortcut(tool).to_string()),
        ));
    }
    for tool in crate::tool_tips::CameraRawTool::ALL {
        out.push((tool.shortcut_id().into(), tool.title().into(), vec!["Tools".into(), "Camera Raw".into()], Some(tool.default_shortcut().into())));
    }
    for (id, label, def) in prefs::TEMPORARY_TOOLS {
        if seen.insert(id.to_string()) {
            out.push((id.to_string(), label.to_string(), vec!["Tools".into(), "Temporary".into()], Some(def.to_string())));
        }
    }
    // Integrated vector menus may append commands to an earlier section. The
    // shortcut editor renders one heading per contiguous section, preserving
    // the menu's first-encounter order and each section's command order.
    let mut sections = Vec::<Option<String>>::new();
    for item in &out {
        let section = item.2.first().cloned();
        if !sections.contains(&section) {
            sections.push(section);
        }
    }
    out.sort_by_key(|item| sections.iter().position(|section| section.as_ref() == item.2.first()).unwrap_or(sections.len()));
    out
}

/// Text of a key press for the shortcut editor (`Cmd+Shift+K`), `None` for bare modifiers.
pub use li_keys::input::{is_modifier_key, shortcut_text};

// ------------------------------------------------------------------ menu routing

/// Shell front ends for Edit-menu commands invoked without parameters (dialogs, pickers).
/// `None` when `id` isn't handled here.
pub fn invoke(app: &mut PhotocraftApp, _ctx: &egui::Context, id: &str, params: &Value) -> Option<Result<Value, String>> {
    let empty = params.as_object().is_none_or(|o| o.is_empty());
    if !empty {
        return None;
    }
    // Retain the saved shortcut/automation alias, using the existing setup window.
    if id == "edit.preferences.integrations" {
        return crate::ai_ui::menu(app, _ctx, "li.localAi", params);
    }
    if let Some(section) = id.strip_prefix("edit.preferences.") {
        return Some(Ok(json!({"dialog": open_preferences(app, section)})));
    }
    let dialog = |d: Option<u64>| Some(d.map(|d| json!({"dialog": d})).ok_or_else(|| "not available".to_string()));
    match id {
        "edit.keyboardShortcuts" => Some(Ok(json!({"dialog": open_shortcuts(app, 0)}))),
        "edit.menus" => Some(Ok(json!({"dialog": open_shortcuts(app, 1)}))),
        "edit.toolbar" => Some(Ok(json!({"dialog": open_shortcuts(app, 2)}))),
        "edit.colorSettings" => {
            let d = crate::filter_dialog::open(app, "edit.colorSettings");
            let cur = serde_json::to_value(&app.session.color.settings).unwrap_or_default();
            // What Monitor Profile resolves to, so a fallback to sRGB is visible (#569): read the
            // displays again now, and the dialog refreshes this line as it draws.
            crate::monitor_status::read_now(app);
            let note = crate::monitor_status::note(app);
            if let Some(dm) = d.and_then(|d| app.ui.dialog_mut(d)) {
                for (k, v) in cur.as_object().into_iter().flatten() {
                    if dm.fields.contains_key(k) {
                        dm.fields.insert(k.clone(), v.clone());
                    }
                }
                dm.fields.insert("__note".into(), json!(note));
            }
            dialog(d)
        }
        "edit.fade" | "edit.findAndReplaceText" | "edit.defineBrushPreset" | "edit.defineCustomShape" | "edit.autoAlignLayers" | "edit.autoBlendLayers" => {
            if !app.session.is_enabled(id) {
                return Some(Err(photocraft_engine::commands::find(id)
                    .map_or("not available".into(), |c| format!("{} is not available right now", c.label.trim_end_matches('…')))));
            }
            let d = crate::filter_dialog::open(app, id);
            if let Some(dm) = d.and_then(|d| app.ui.dialog_mut(d)) {
                // Optional parameters the generic dialog can't express keep their defaults.
                for k in ["reference", "path"] {
                    dm.fields.remove(k);
                }
            }
            dialog(d)
        }
        "edit.contentAwareFill" => {
            if !app.session.is_enabled(id) {
                return Some(Err("Content-Aware Fill needs a selection on a pixel layer".into()));
            }
            let d = crate::filter_dialog::open(app, id);
            if let Some(dm) = d.and_then(|d| app.ui.dialog_mut(d)) {
                dm.fields.insert("__preview".into(), json!(true));
                for k in ["margin", "seed", "channel"] {
                    dm.fields.remove(k);
                }
            }
            dialog(d)
        }
        "edit.contentAwareScale" => {
            let st = app.session.active()?;
            let b = match &st.doc.selection {
                Some(s) => s.content_bounds(),
                None => st.active_layer.and_then(|l| st.doc.layer(l)).and_then(|l| l.surface()).map(|s| s.content_bounds()).unwrap_or_default(),
            };
            let d = crate::filter_dialog::open(app, id);
            if let Some(dm) = d.and_then(|d| app.ui.dialog_mut(d)) {
                dm.fields.insert("width".into(), json!(b.width()));
                dm.fields.insert("height".into(), json!(b.height()));
                dm.fields.remove("scaleX");
                dm.fields.remove("scaleY");
                dm.fields.insert("__preview".into(), json!(true));
            }
            dialog(d)
        }
        "edit.presets.presetManager" => {
            Some(Ok(json!({"dialog": open_kind(app, "presets", "Preset Manager", json!({"kind": "brushes", "selected": 0, "newName": ""}))})))
        }
        "edit.presets.exportImportPresets" => Some(Ok(
            json!({"dialog": open_kind(app, "presetsIO", "Export/Import Presets", json!({"action": "export", "brushes": true, "customShapes": true}))}),
        )),
        _ => None,
    }
}

fn open_kind(app: &mut PhotocraftApp, kind: &str, label: &str, fields: Value) -> u64 {
    let mut f = Map::new();
    f.insert("__prefsui".into(), json!(kind));
    f.insert("__label".into(), json!(label));
    if let Value::Object(m) = fields {
        f.extend(m);
    }
    app.ui.open_dialog(DialogKind::Command, f)
}

/// Revert the toolbar preview when the Shortcuts/Toolbar dialog is cancelled.
pub(crate) fn cancel(app: &mut PhotocraftApp, fields: &Map<String, Value>) {
    if is_preferences(fields) {
        crate::ai_ui::settings_ui::reset();
    }
    if fields.get("__prefsui").and_then(Value::as_str) == Some("shortcuts")
        && let Some(original) = fields.get("toolbarOriginal").and_then(|v| serde_json::from_value(v.clone()).ok())
    {
        app.session.edit_prefs(|p| p.toolbar = original);
    }
}

/// Is this a dialog rendered by this module?
pub fn owns(fields: &Map<String, Value>) -> bool {
    fields.contains_key("__prefsui")
}

/// Only Preferences supports applying changes without closing the dialog.
pub fn is_preferences(fields: &Map<String, Value>) -> bool {
    fields.get("__prefsui").and_then(Value::as_str) == Some("prefs")
}

fn preference_values(p: &prefs::Preferences) -> Map<String, Value> {
    let values = p.to_json();
    let mut sections: Map<String, Value> = SECTIONS.iter().filter_map(|(id, _)| Some((id.to_string(), values.get(id)?.clone()))).collect();
    sections.insert("toolbar".into(), json!({"vectorKeyboard": p.toolbar.vector_keyboard}));
    sections
}

/// Compare the editable sections, excluding settings managed outside Preferences.
pub fn preferences_changed(app: &PhotocraftApp, fields: &Map<String, Value>) -> bool {
    is_preferences(fields)
        && (fields.get("hostValues").is_some_and(|v| app.host_preferences.as_ref().is_some_and(|host| *v != host.values()))
            || crate::ai_ui::settings_ui::changed()
            || fields.get("values").and_then(Value::as_object).is_some_and(|values| *values != preference_values(app.session.prefs())))
}

/// Commit the working copy and keep the current section open. Failure leaves the draft intact.
pub fn apply(app: &mut PhotocraftApp, id: u64) -> Result<Value, String> {
    let d = app.ui.dialogs.iter().find(|d| d.id == id).ok_or_else(|| format!("no dialog {id}"))?;
    if d.kind != DialogKind::Command || !is_preferences(&d.fields) {
        return Err("Apply is only available for Preferences".into());
    }
    let fields = d.fields.clone();
    let result = confirm(app, &fields)?;
    let values = Value::Object(preference_values(app.session.prefs()));
    if let Some(d) = app.ui.dialog_mut(id) {
        // Use the validated values as the next draft, including any normalisation by prefs.set.
        d.fields.insert("values".into(), values);
        d.fields.insert("hostValues".into(), app.host_preferences.as_ref().map_or_else(|| app.host_preferences_values.clone(), |host| host.values()));
    }
    Ok(result)
}

/// Max dialog width for our dialogs.
pub fn width(fields: &Map<String, Value>) -> Option<f32> {
    match fields.get("__prefsui").and_then(Value::as_str)? {
        "prefs" => Some(780.0),
        "shortcuts" => Some(720.0),
        _ => Some(460.0),
    }
}

/// Open Edit › Preferences on `section`.
pub fn open_preferences(app: &mut PhotocraftApp, section: &str) -> u64 {
    let known = app.host_preferences_pages.iter().any(|(id, _)| *id == section)
        || SECTIONS.iter().any(|(id, _)| *id == section)
        || section == crate::attributions::SECTION
        || crate::ai_ui::settings_ui::owns(section);
    let section = if known && section != "integrations" { section } else { "general" };

    if let Some(existing) = app.ui.dialogs.iter_mut().find(|d| is_preferences(&d.fields)) {
        existing.fields.insert("section".into(), json!(section));
        return existing.id;
    }
    crate::ai_ui::settings_ui::reset();
    let working = preference_values(app.session.prefs());
    let order = field_order(app.session.prefs(), &working);
    open_kind(
        app,
        "prefs",
        "Preferences",
        json!({"section": section, "values": working, "__order": order,
        "hostValues": app.host_preferences.as_ref().map_or_else(|| app.host_preferences_values.clone(), |host| host.values())}),
    )
}

/// Each section's keys in declaration order (JSON objects sort their keys; the serialised text
/// keeps the struct's order, which groups related settings like Photoshop's dialog).
fn field_order(p: &prefs::Preferences, sections: &Map<String, Value>) -> Value {
    let text = serde_json::to_string(p).unwrap_or_default();
    let mut out = Map::new();
    for (id, v) in sections {
        let Some(start) = text.find(&format!("\"{id}\":{{")) else { continue };
        let body = &text[start..];
        let end = body.find('}').unwrap_or(body.len());
        let region = &body[..end];
        let mut keys: Vec<(usize, String)> =
            v.as_object().into_iter().flatten().map(|(k, _)| (region.find(&format!("\"{k}\":")).unwrap_or(usize::MAX), k.clone())).collect();
        keys.sort();
        out.insert(id.clone(), json!(keys.into_iter().map(|(_, k)| k).collect::<Vec<_>>()));
    }
    Value::Object(out)
}

/// Runtime host catalog: key, human label, menu path, default chord. The shared dialog and
/// the Library shortcut profile remain the only editor and persisted authority.
pub type HostShortcutItem = (String, String, Vec<String>, Option<String>);
pub fn set_host_shortcut_catalog(app: &mut PhotocraftApp, ctx: &egui::Context, items: Vec<HostShortcutItem>) {
    for dialog in &mut app.ui.dialogs {
        if dialog.fields.get("__prefsui").and_then(Value::as_str) == Some("shortcuts") {
            dialog.fields.insert("__hostShortcuts".into(), json!(items));
        }
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("host-shortcut-catalog"), items));
}
/// Match the host's documented exact chords, retaining the genuine metadata-save default.
fn reserved_library_chord(id: &str, chord: &str) -> Option<&'static str> {
    if !id.starts_with("library::") {
        return None;
    }
    let shortcut = li_keys::Chord::parse(chord)?;
    if ["Cmd+Alt+1", "Cmd+Alt+2", "Cmd+Alt+3"].iter().any(|key| li_keys::Chord::parse(key).is_some_and(|key| shortcut.overlaps(&key))) {
        Some("switching Library, Develop and Compositing")
    } else if id != "library::photo.saveMetadataToFile" && li_keys::Chord::parse("Cmd+S").is_some_and(|key| shortcut.overlaps(&key)) {
        Some("saving the selected unsaved composite in Library")
    } else {
        None
    }
}
fn reserved_message(action: &str) -> String {
    format!("This shortcut is reserved for {action}. Choose another shortcut; the existing binding is unchanged.")
}

fn same_chord_in_scope(_id: &str, a: &str, b: &str) -> bool {
    li_keys::Chord::parse(a).zip(li_keys::Chord::parse(b)).is_some_and(|(a, b)| a.overlaps(&b))
}

fn same_shortcut_scope(a: &str, b: &str) -> bool {
    a.starts_with("library::") == b.starts_with("library::")
}

/// Open Keyboard Shortcuts and Menus on a tab (0 shortcuts, 1 menus, 2 toolbar).
pub fn open_shortcuts(app: &mut PhotocraftApp, tab: u64) -> u64 {
    let p = app.session.prefs();
    let mut overrides = p.shortcut_overrides(false).clone();
    overrides.extend(p.shortcut_overrides(true).iter().map(|(id, key)| (id.clone(), key.clone())));
    let fields = json!({
        "tab": tab,
        "filter": "",
        "overrides": overrides,
        "profiles": p.shortcut_profiles,
        "hidden": p.menus.hidden,
        "colors": p.menus.colors,
        "toolbarOriginal": p.toolbar,
        "selected": "",
        "capture": false,
        "message": "",
    });
    open_kind(app, "shortcuts", tl!("Keyboard Shortcuts and Menus"), fields)
}

/// Capture before Open Document callbacks can switch, edit or close the admitted tab.
pub(crate) fn mismatch_target(session: &photocraft_engine::Session) -> Option<Value> {
    let state = session.active()?;
    Some(json!({"doc":state.doc.id.0,"name":state.doc.name,"revision":state.revision,
        "profile":photocraft_engine::color_cmds::document_profile(&state.doc).content_hash(),
        "working":session.color.working(state.doc.mode).content_hash(),
        "intent":session.color.settings.intent,"bpc":session.color.settings.bpc}))
}

/// The mismatch report and decision belong to the captured imported document.
pub(crate) fn open_mismatch_for(app: &mut PhotocraftApp, report: &Value, target: Value) -> u64 {
    let msg = if report.get("invalidProfile").and_then(Value::as_bool) == Some(true) {
        format!(
            "The embedded colour profile is unusable or does not match the document's colour model. Working space: {}.",
            report["working"].as_str().unwrap_or("?")
        )
    } else if report.get("missing").and_then(Value::as_bool) == Some(true) {
        format!("The document does not have an embedded colour profile. Working space: {}.", report["working"].as_str().unwrap_or("?"))
    } else {
        format!(
            "The document has an embedded colour profile that does not match the working space.\nEmbedded: {}\nWorking: {}",
            report["embedded"].as_str().unwrap_or("?"),
            report["working"].as_str().unwrap_or("?")
        )
    };
    let choice = |action: Option<&str>| match action {
        Some("converted") => "convert",
        Some("discarded") => "discard",
        Some("assigned") => "assignWorking",
        _ => "preserve",
    };
    let applied = choice(report.get("appliedAction").or_else(|| report.get("action")).and_then(Value::as_str));
    let action = choice(report.get("requestedAction").or_else(|| report.get("action")).and_then(Value::as_str));
    open_kind(
        app,
        "mismatch",
        tl!("Embedded Profile Mismatch"),
        json!({"message": format!("{}\n{msg}",target["name"].as_str().unwrap_or("Document")), "action": action, "applied": applied, "missing": report.get("missing").is_some(), "target":target}),
    )
}

// ------------------------------------------------------------------ bodies

/// Render one of our dialogs' bodies.
pub fn body(app: &mut PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    match f.get("__prefsui").and_then(Value::as_str).unwrap_or("") {
        "prefs" => {
            f.insert("__gpuInfo".into(), json!(app.perf.gpu_info.lines()));
            f.insert("__gpuFailure".into(), json!(app.perf.gpu_info.failure()));
            prefs_body(app, ui, f);
            if f.remove("openShortcuts").and_then(|v| v.as_bool()).unwrap_or(false) {
                open_shortcuts(app, 0);
            }
            crate::attributions::open_pending(app, ui.ctx(), f);
        }
        "shortcuts" => shortcuts_body(app, ui, f),
        "presets" => presets_body(app, ui, f),
        "presetsIO" => presets_io_body(ui, f),
        "mismatch" => mismatch_body(ui, f),
        "clipboard" => crate::clipboard_ui::body(ui, f),
        _ => {}
    }
}

fn humanize(key: &str) -> String {
    if key == "checkForUpdatesAtStartup" {
        return "Check for updates at startup".into();
    }
    if key == "tabletCompatibilityMode" {
        return "Pen tablet: use compatibility mode (XWayland)".into();
    }
    let mut s = String::new();
    for (i, ch) in key.chars().enumerate() {
        if i == 0 {
            s.extend(ch.to_uppercase());
        } else if ch.is_uppercase() {
            s.push(' ');
            s.extend(ch.to_lowercase());
        } else {
            s.push(ch);
        }
    }
    s.replace("Psd", "PSD").replace("Gpu", "GPU").replace("Ui ", "UI ").replace("Mb", "(MB)").replace("Exif", "EXIF").replace("Hud", "HUD")
}

fn choice_label(v: &str) -> String {
    match v {
        "colour" => "Colour".into(),
        "monochrome" => "Monochrome (default)".into(),
        "cm" => "Centimeters".into(),
        "editedFolder" => "A copy in the Edited folder (originals untouched)".into(),
        "original" => "The original file".into(),
        "saveAs" => "Ask each time (Save As)".into(),
        "mm" => "Millimeters".into(),
        "rich" => "Rich (name, shortcut and how to use)".into(),
        "simple" => "Simple (name and shortcut)".into(),
        "75" | "100" | "125" | "150" | "175" | "200" | "250" | "300" => format!("{v}%"),
        "8" => "8 Bits/Channel".into(),
        "16" => "16 Bits/Channel".into(),
        "postScript" => "PostScript (72 points/inch)".into(),
        "traditional" => "Traditional (72.27 points/inch)".into(),
        "vulkan" => "Vulkan".into(),
        "dx12" => "DirectX 12".into(),
        "metal" => "Metal".into(),
        "gl" => "OpenGL".into(),
        "cpu" => "CPU (no GPU acceleration)".into(),
        v => humanize(v),
    }
}

/// An editable `#rrggbb` field beside a colour swatch (#668): typing a valid value sets the
/// colour; while it's being typed a partial value is kept, and it shows the colour otherwise.
fn hex_field(ui: &mut egui::Ui, key: &str, rgb: &mut [u8; 3]) {
    let id = egui::Id::new(("pref-hex", key));
    let shown = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
    let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| shown.clone());
    let r = ui.add(egui::TextEdit::singleline(&mut text).font(crate::theme::mono(11.5)).desired_width(72.0).char_limit(7));
    if r.changed()
        && let Some(c) = prefs::parse_hex(&text)
    {
        *rgb = c;
    }
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
}

fn color_of(s: &str) -> Color32 {
    prefs::parse_hex(s).map_or(Color32::GRAY, |c| Color32::from_rgb(c[0], c[1], c[2]))
}

fn prefs_nav_row(ui: &mut egui::Ui, title: &str, selected: bool, empty: bool) -> bool {
    crate::widgets::preferences_tab(ui, title, selected, empty).clicked()
}

/// Preferences: section list on the left, the section's settings on the right.
fn prefs_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let mut section = f.get("section").and_then(Value::as_str).unwrap_or("general").to_string();
    let mut values = f.get("values").cloned().unwrap_or(Value::Null);
    // The dialog follows the language being edited, so a change shows before OK.
    let lang = li_i18n::Lang::from_pref(values.pointer("/interface/language").and_then(Value::as_str).unwrap_or("auto"));
    let available = (ui.ctx().content_rect().width() - 48.0).min(750.0);
    let nav_width = (available * 0.23).clamp(130.0, 170.0);
    let body_width = (available - nav_width - 24.0).max(280.0);
    let body_height = (ui.ctx().content_rect().height() - 210.0).clamp(160.0, 390.0);
    ui.horizontal_top(|ui| {
        // Section list.
        ui.vertical(|ui| {
            ui.set_width(nav_width);
            egui::ScrollArea::vertical().max_height(body_height).id_salt("prefs-navigation").show(ui, |ui| {
                for (id, title) in SECTIONS {
                    if id == "integrations" || (!app.host_preferences_pages.is_empty() && !["general", "interface", "performance"].contains(&id)) {
                        continue;
                    }
                    if ui.push_id(id, |ui| prefs_nav_row(ui, tl!(title), section == id, !has_visible_fields(&values, id))).inner {
                        section = id.to_string();
                    }
                }
                if !app.host_preferences_pages.is_empty() {
                    crate::widgets::hairline(ui);
                    crate::widgets::section_label(ui, "Library & Develop");
                    for (id, title) in app.host_preferences_pages {
                        let response = ui.push_id(id, |ui| crate::widgets::preferences_tab(ui, tl!(title), section == *id, false)).inner;
                        ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(("host-prefs-tab", *id)), response.rect));
                        if response.clicked() {
                            section = id.to_string();
                        }
                    }
                }
                if !app.host_preferences_pages.is_empty() {
                    crate::widgets::hairline(ui);
                    crate::widgets::section_label(ui, "Compositing");
                    for (id, title) in SECTIONS {
                        if ["general", "interface", "performance", "integrations"].contains(&id) {
                            continue;
                        }
                        if ui.push_id(id, |ui| prefs_nav_row(ui, tl!(title), section == id, !has_visible_fields(&values, id))).inner {
                            section = id.to_string();
                        }
                    }
                }
                ui.separator();
                crate::widgets::section_label(ui, "AI");
                for (id, title) in crate::ai_ui::settings_ui::PAGES {
                    if ui.push_id(id, |ui| prefs_nav_row(ui, tl!(title), section == id, false)).inner {
                        section = id.to_string();
                    }
                }
            });
            ui.add_space(6.0);
            if crate::widgets::secondary_button(ui, "Keyboard Shortcuts…", 0.0).clicked() {
                f.insert("openShortcuts".into(), json!(true));
            }
            crate::attributions::prefs_nav(ui, &mut section);
        });
        crate::widgets::vline(ui, body_height + 30.0);
        ui.vertical(|ui| {
            ui.set_width(body_width);
            if app.host_preferences_pages.iter().any(|(id, _)| *id == section) {
                if let Some(host) = app.host_preferences.as_mut() {
                    let draft = f.entry("hostValues").or_insert_with(|| host.values());
                    egui::ScrollArea::vertical().max_height(body_height).id_salt("host-prefs-scroll").show(ui, |ui| host.body(ui, &section, draft));
                }
                return;
            }
            if crate::ai_ui::settings_ui::page(app, ui, &section) {
                return;
            }
            if crate::attributions::prefs_page(ui, &section, f) {
                return;
            }
            let title = SECTIONS.iter().find(|(id, _)| *id == section).map_or("General", |(_, t)| *t);
            ui.label(RichText::new(tl!(&title)).font(crate::theme::semibold(14.0)).color(t.text));
            ui.add_space(6.0);
            egui::ScrollArea::vertical().max_height(body_height).id_salt("prefs-scroll").show(ui, |ui| {
                let order: Vec<String> =
                    f.get("__order").and_then(|o| o.get(&section)).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
                if section == "tools" {
                    let mut preset = serde_json::from_value(values["toolbar"]["vectorKeyboard"]["preset"].clone()).unwrap_or_default();
                    if crate::vector_integration::keyboard_preset(ui, &mut preset) {
                        values["toolbar"]["vectorKeyboard"]["preset"] = json!(preset);
                    }
                }
                if !has_visible_fields(&values, &section) {
                    ui.add_space(4.0);
                    ui.label(RichText::new(tl!("These settings aren't available in Local Image yet.")).color(t.text_faint));
                } else if let Some(obj) = values.get_mut(&section).and_then(Value::as_object_mut) {
                    section_fields(ui, &section, obj, &order, lang);
                    if section == "performance" {
                        gpu_status_rows(ui, f.get("__gpuInfo"), f.get("__gpuFailure").and_then(Value::as_str), obj);
                        processing_rows(ui, obj);
                    }
                    ui.add_space(8.0);
                }
                if has_visible_fields(&values, &section)
                    && crate::widgets::secondary_button(ui, tl!("Reset Section"), 110.0).clicked()
                    && let Some(def) = prefs::Preferences::default().get(&section)
                {
                    values[&section] = def;
                }
            });
        });
    });
    f.insert("section".into(), json!(section));
    f.insert("values".into(), values);
}

/// Preferences › Performance: what the app renders with now, and a reset of the GPU backend
/// (a crashed start may have moved it to a safer choice).
fn gpu_status_rows(ui: &mut egui::Ui, info: Option<&Value>, failure: Option<&str>, obj: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(10.0);
    ui.label(RichText::new(tl!("Graphics")).font(crate::theme::semibold(12.5)).color(t.text));
    // CPU mode because of a GPU failure: say so first, with the way back.
    if let Some(reason) = failure {
        ui.label(RichText::new(tl!("GPU acceleration is off after a graphics failure.")).color(t.warning));
        ui.label(RichText::new(reason).color(t.text_dim));
        let retry_chosen = rendering_mode_value(obj) != "cpu" && obj.get("gpuBackend").and_then(Value::as_str).unwrap_or("auto") == "auto";
        if !retry_chosen && crate::widgets::secondary_button(ui, tl!("Retry GPU"), 120.0).clicked() {
            obj.insert("renderingMode".into(), json!("gpu"));
            obj.insert("gpuBackend".into(), json!("auto"));
        }
        ui.add_space(4.0);
    }
    for line in info.and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        ui.label(RichText::new(line).color(t.text_dim));
    }
    ui.add_space(4.0);
    ui.label(RichText::new(tl!("Applies at next launch.")).color(t.text_faint));
    ui.collapsing(tl!("Advanced"), |ui| {
        ui.horizontal(|ui| {
            ui.label(tl!("GPU Backend"));
            let mut current = obj.get("gpuBackend").and_then(Value::as_str).unwrap_or("auto").to_string();
            let options = prefs::choices("performance.gpuBackend").unwrap_or(&[]);
            let labels: Vec<String> = options.iter().map(|o| choice_label(o)).collect();
            let pairs: Vec<(String, &str)> = options.iter().map(|o| o.to_string()).zip(labels.iter().map(String::as_str)).collect();
            crate::widgets::dropdown(ui, "graphics-backend", &mut current, &pairs, 220.0);
            obj.insert("gpuBackend".into(), json!(current));
        });
    });
}

/// local-image: new Processing labels, shown in English until the next translation pass (owner:
/// translations last); listed for the i18n owner in docs/wip/BUILD-threads-a.md.
const PROCESSING: &str = "Processing";
const PROCESSING_THREADS: &str = "Processing threads";
const BACKGROUND_PRIORITY: &str = "Background work priority";
const PROCESSING_MEMORY: &str = "Memory for processing";
const DETECTED: &str = "Detected: {logical} logical / {physical} physical cores{hybrid}, {ram} GB RAM";

/// local-image: Preferences › Performance › Processing: optional overrides over the values the app
/// derives from the hardware at start-up (`lightcraft_sysmem::profile`; threading policy rules 1, 6
/// and 7). Every row defaults to Automatic and shows the automatic value. The compute pool is built
/// at launch, so changes apply then.
fn processing_rows(ui: &mut egui::Ui, obj: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let p = lightcraft_sysmem::profile();
    let hw = &p.hardware;
    let mut c: prefs::SystemResources = obj.get("system").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    ui.add_space(10.0);
    ui.label(RichText::new(tl!(PROCESSING)).font(crate::theme::semibold(12.5)).color(t.text));
    let auto = |v: String| format!("{} ({v})", tl!("Automatic"));
    let threads: Vec<(u32, String)> =
        std::iter::once((0, auto(p.threads_auto.to_string()))).chain((1..=hw.logical.clamp(1, 1024) as u32).map(|n| (n, n.to_string()))).collect();
    let memory: Vec<(u32, String)> = std::iter::once((0, auto(format!("{} %", lightcraft_sysmem::profile::AUTO_MEMORY_PERCENT))))
        .chain((1..=9).map(|n| (n * 10, format!("{} %", n * 10))))
        .collect();
    let priority = [(prefs::BackgroundPriority::Low, tl!("Low").to_string()), (prefs::BackgroundPriority::Normal, tl!("Normal").to_string())];
    egui::Grid::new("prefs-processing").num_columns(2).spacing([14.0, 7.0]).show(ui, |ui| {
        ui.label(RichText::new(tl!(PROCESSING_THREADS)).color(t.text_dim));
        let opts: Vec<(u32, &str)> = threads.iter().map(|(n, l)| (*n, l.as_str())).collect();
        crate::widgets::dropdown(ui, "pref-processing-threads", &mut c.threads, &opts, 220.0);
        ui.end_row();
        ui.label(RichText::new(tl!(BACKGROUND_PRIORITY)).color(t.text_dim));
        let opts: Vec<(prefs::BackgroundPriority, &str)> = priority.iter().map(|(v, l)| (*v, l.as_str())).collect();
        crate::widgets::dropdown(ui, "pref-background-priority", &mut c.background_priority, &opts, 220.0);
        ui.end_row();
        ui.label(RichText::new(tl!(PROCESSING_MEMORY)).color(t.text_dim));
        let opts: Vec<(u32, &str)> = memory.iter().map(|(n, l)| (*n, l.as_str())).collect();
        crate::widgets::dropdown(ui, "pref-processing-memory", &mut c.memory_percent, &opts, 220.0);
        ui.end_row();
    });
    crate::widgets::checkbox(ui, &mut c.full_speed_on_battery, tl!("Keep full speed on battery"))
        .on_hover_text(tl!("Otherwise use fewer compute threads and pause indexing, Smart Sort and background preview builds on battery."));
    let hybrid = match (hw.perf_cores, hw.eff_cores) {
        (Some(pc), Some(ec)) => format!(" ({pc}P + {ec}E)"),
        _ => String::new(),
    };
    let detected = tl!(DETECTED)
        .replace("{logical}", &hw.logical.to_string())
        .replace("{physical}", &hw.physical.to_string())
        .replace("{hybrid}", &hybrid)
        .replace("{ram}", &format!("{:.0}", hw.ram_total as f64 / (1u64 << 30) as f64));
    ui.label(RichText::new(detected).color(t.text_dim));
    ui.label(RichText::new(tl!("Applies at next launch.")).color(t.text_faint));
    if let Ok(v) = serde_json::to_value(&c) {
        obj.insert("system".into(), v);
    }
}

/// One user-facing choice: Automatic, GPU or CPU / Compatibility.
fn rendering_mode_row(ui: &mut egui::Ui, obj: &mut Map<String, Value>) {
    let mut current = rendering_mode_value(obj);
    ui.horizontal(|ui| {
        ui.label(tl!("Rendering Mode"));
        let pairs =
            vec![("auto".to_string(), tl!("Automatic (recommended)")), ("gpu".to_string(), tl!("GPU")), ("cpu".to_string(), tl!("CPU / Compatibility"))];
        let previous = current.clone();
        crate::widgets::dropdown(ui, "rendering-mode", &mut current, &pairs, 240.0);
        if current != previous {
            obj.insert("renderingMode".into(), json!(current));
        }
    });
    ui.label(tl!("Automatic uses GPU acceleration when available and falls back to CPU rendering on errors."));
    ui.add_space(8.0);
}

fn rendering_mode_value(obj: &Map<String, Value>) -> String {
    obj.get("renderingMode").and_then(Value::as_str).unwrap_or("auto").to_string()
}

/// Does `section` have any setting the dialog shows (see [`prefs::HIDDEN_UNTIL_IMPLEMENTED`])?
fn has_visible_fields(values: &Value, section: &str) -> bool {
    values.get(section).and_then(Value::as_object).is_some_and(|o| o.keys().any(|k| !prefs::is_hidden(&format!("{section}.{k}"))))
}

/// Generic editor for a section's fields: checkboxes, dropdowns for choices, colour swatches,
/// number fields with the preference's range, text fields.
fn section_fields(ui: &mut egui::Ui, section: &str, obj: &mut Map<String, Value>, order: &[String], lang: li_i18n::Lang) {
    let t = Tokens::get(ui.ctx());
    if section == "interface" {
        let mut current = obj.get("toolIcons").and_then(Value::as_str).unwrap_or("colour").to_string();
        ui.horizontal(|ui| {
            ui.label(tl!("Tool icons"));
            let choices = [("monochrome".to_string(), tl!("Monochrome (default)")), ("colour".to_string(), tl!("Colour"))];
            crate::widgets::dropdown(ui, "pref-interface.toolIcons", &mut current, &choices, 220.0);
        });
        obj.insert("toolIcons".into(), json!(current));
        ui.add_space(8.0);
    }
    if section == "performance" {
        rendering_mode_row(ui, obj);
    }
    let mut keys: Vec<String> = order.iter().filter(|k| obj.contains_key(*k)).cloned().collect();
    keys.extend(obj.keys().filter(|k| !order.contains(k)).cloned());
    egui::Grid::new(("prefs-grid", section)).num_columns(2).spacing([14.0, 7.0]).show(ui, |ui| {
        for k in keys {
            let path = format!("{section}.{k}");
            // Settings nothing reads yet stay out of the dialog (issue #204); their stored values
            // pass through untouched.
            if (section == "interface" && k == "toolIcons")
                || prefs::is_hidden(&path)
                || (section == "performance" && matches!(k.as_str(), "gpuBackend" | "renderingMode"))
            {
                continue;
            }
            let v = obj.get(&k).cloned().unwrap_or(Value::Null);
            let human = humanize(&k);
            let label = tl!(&human).to_string();
            match &v {
                Value::Bool(b) => {
                    ui.label("");
                    let mut b = *b;
                    crate::widgets::checkbox(ui, &mut b, &label);
                    obj.insert(k, json!(b));
                }
                Value::String(s) if path == "tools.tabletCompatibilityMode" => {
                    ui.label(tl!("Pen tablet: use compatibility mode (XWayland)"))
                        .on_hover_text("Auto detects an attached pen and uses XWayland for pressure, tilt and eraser. On forces compatibility; Off keeps native Wayland. XWayland forces 8-bit display output (dithered); native Wayland can use 10-bit. XWayland can look softer at fractional display scaling. Restart to apply. LOCAL_IMAGE_NATIVE_WAYLAND=1 keeps native Wayland.");
                    let mut cur = s.clone();
                    crate::widgets::dropdown(ui, "pref-tools.tabletCompatibilityMode", &mut cur, &[("auto".into(), "Auto"), ("on".into(), "On"), ("off".into(), "Off")], 220.0);
                    obj.insert(k, json!(cur));
                }
                Value::String(s) if path == "interface.language" => {
                    ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                    let mut pairs: Vec<(String, &str)> = vec![("auto".into(), tl!("Auto"))];
                    pairs.extend(li_i18n::Lang::ALL.into_iter().map(|l| (l.code().to_string(), l.name())));
                    let mut cur = s.clone();
                    crate::widgets::dropdown(ui, &format!("pref-{path}"), &mut cur, &pairs, 220.0);
                    obj.insert(k, json!(cur));
                }
                Value::String(s) if prefs::choices(&path).is_some() => {
                    ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                    let opts = prefs::choices(&path).unwrap_or(&[]);
                    let labels: Vec<String> = opts.iter().map(|o| choice_label(o)).collect();
                    let pairs: Vec<(String, &str)> = opts.iter().map(|o| o.to_string()).zip(labels.iter().map(String::as_str)).collect();
                    let mut cur = s.clone();
                    crate::widgets::dropdown(ui, &format!("pref-{path}"), &mut cur, &pairs, 220.0);
                    obj.insert(k, json!(cur));
                }
                Value::String(s) if prefs::is_color(&path) => {
                    ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                    let c = prefs::parse_hex(s).unwrap_or([128, 128, 128]);
                    let mut rgb = c;
                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgb(&mut rgb);
                        hex_field(ui, &path, &mut rgb);
                    });
                    obj.insert(k, json!(format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])));
                    let _ = color_of(s);
                }
                Value::String(s) => {
                    ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                    let mut s = s.clone();
                    ui.add(egui::TextEdit::singleline(&mut s).desired_width(260.0));
                    obj.insert(k, json!(s));
                }
                Value::Number(n) => {
                    ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                    let (lo, hi) = prefs::range(&path).unwrap_or((-1e9, 1e9));
                    if n.is_u64() || n.is_i64() {
                        let mut x = n.as_i64().unwrap_or(0);
                        ui.add(egui::DragValue::new(&mut x).range(lo as i64..=hi as i64).custom_parser(crate::widgets::parse_num));
                        obj.insert(k, json!(x));
                    } else {
                        let mut x = n.as_f64().unwrap_or(0.0);
                        ui.add(egui::DragValue::new(&mut x).range(lo..=hi).speed(0.1).max_decimals(3).custom_parser(crate::widgets::parse_num));
                        obj.insert(k, json!(x));
                    }
                }
                Value::Array(_) if path == "tools.penPressureCurve" => {
                    // local-image: a preset menu and a small curve graph.
                    ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                    crate::pressure_curve_ui::editor(ui, obj, &k);
                }
                Value::Array(items) if k == "disks" => {
                    ui.label(RichText::new(tl!("Scratch disks")).color(t.text_dim));
                    let mut items = items.clone();
                    ui.vertical(|ui| {
                        for d in &mut items {
                            ui.horizontal(|ui| {
                                let mut on = d.get("enabled").and_then(Value::as_bool).unwrap_or(false);
                                let mut path = d.get("path").and_then(Value::as_str).unwrap_or("").to_string();
                                crate::widgets::checkbox(ui, &mut on, "");
                                ui.add(egui::TextEdit::singleline(&mut path).desired_width(240.0));
                                *d = json!({"enabled": on, "path": path});
                            });
                        }
                        if ui.small_button(tl!("Add disk")).clicked() {
                            items.push(json!({"enabled": true, "path": ""}));
                        }
                    });
                    obj.insert(k, Value::Array(items));
                }
                Value::Array(items) => {
                    ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                    ui.label(RichText::new(li_i18n::trn(lang, items.len() as u64, "{n} item", "{n} items")).color(t.text_faint));
                    if !items.is_empty() && ui.small_button(tl!("Clear")).clicked() {
                        obj.insert(k, json!([]));
                    }
                }
                _ => continue,
            }
            ui.end_row();
        }
    });
}

/// Keyboard Shortcuts and Menus: a searchable list of commands by menu with editable shortcuts
/// (click a shortcut, press keys; ⌫ removes it), menu visibility and colours, toolbar tools.
fn shortcuts_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let mut tab = f.get("tab").and_then(Value::as_u64).unwrap_or(0);
    ui.horizontal(|ui| {
        for (i, name) in [tl!("Keyboard Shortcuts"), tl!("Menus"), tl!("Toolbar")].iter().enumerate() {
            if crate::widgets::pill_tab(ui, name, tab == i as u64).clicked() {
                tab = i as u64;
                f.insert("capture".into(), json!(false));
            }
        }
    });
    f.insert("tab".into(), json!(tab));
    ui.add_space(6.0);
    if tab == 2 {
        crate::toolsets_ui::tab(app, ui, f);
        return;
    }
    let mut filter = f.get("filter").and_then(Value::as_str).unwrap_or("").to_string();
    ui.horizontal(|ui| {
        ui.label(RichText::new(tl!("Search")).color(t.text_dim));
        ui.add(egui::TextEdit::singleline(&mut filter).desired_width(260.0).hint_text(tl!("command or shortcut")));
    });
    f.insert("filter".into(), json!(filter));
    // A malformed map (set over automation) is left as it is, so OK rejects it instead of saving
    // the empty map drawn in its place (upstream PhotoCraft 57be0f58, #956).
    let parsed = f.get("overrides").map(|v| serde_json::from_value::<BTreeMap<String, String>>(v.clone()));
    let overrides_ok = parsed.as_ref().is_none_or(Result::is_ok);
    let mut overrides = parsed.and_then(Result::ok).unwrap_or_default();
    let mut hidden: Vec<String> = f.get("hidden").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let mut colors: BTreeMap<String, String> = f.get("colors").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let mut selected = f.get("selected").and_then(Value::as_str).unwrap_or("").to_string();
    let mut capture = f.get("capture").and_then(Value::as_bool).unwrap_or(false);
    let mut message = f.get("message").and_then(Value::as_str).unwrap_or("").to_string();
    let mut items = shortcut_items(app);
    if tab == 0 {
        let host = ui.ctx().data(|d| d.get_temp::<Vec<HostShortcutItem>>(egui::Id::new("host-shortcut-catalog"))).unwrap_or_default();
        f.insert("__hostShortcuts".into(), json!(host));
        items.extend(host);
    }
    let mut scope = f.get("shortcutScope").and_then(Value::as_str).unwrap_or("editor").to_string();
    let previous_scope = scope.clone();
    let parsed_profiles = f.get("profiles").cloned().map(serde_json::from_value::<li_keys::ShortcutProfiles>);
    if parsed_profiles.as_ref().is_some_and(Result::is_err) {
        ui.label("Invalid shortcut profiles. Cancel to keep your saved profiles.");
        return;
    }
    let mut profiles = parsed_profiles.and_then(Result::ok).unwrap_or_else(|| app.session.prefs().shortcut_profiles.clone());
    if tab == 0 {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tl!("Scope")).color(t.text_dim));
            let scopes = [("global".to_string(), "Global"), ("library".to_string(), "Library + Develop"), ("editor".to_string(), "Editor")];
            crate::widgets::dropdown(ui, "shortcut-module", &mut scope, &scopes, 180.0);
        });
        if scope != previous_scope {
            capture = false;
            selected.clear();
            message.clear();
        }
        f.insert("shortcutScope".into(), json!(scope));
        if scope == "global" {
            ui.label(RichText::new("Global shortcuts are fixed and take precedence in every workspace.").color(t.text_dim));
            for (_, label, key) in li_keys::GLOBAL_SHORTCUTS {
                ui.horizontal(|ui| {
                    ui.label(*label);
                    ui.label(crate::shortcuts::pretty(key));
                });
            }
            f.insert("capture".into(), json!(false));
            f.insert("profiles".into(), json!(profiles));
            return;
        }
        let library = scope == "library";
        let workspace = profiles.workspace_mut(library);
        let previous = workspace.active.clone();
        let builtins = if library { ["Lightroom", "Capture One"] } else { ["Photoshop", "Affinity Photo"] };
        let mut names: Vec<String> = builtins.iter().map(|name| (*name).into()).collect();
        names.extend(workspace.custom.keys().cloned());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Profile").color(t.text_dim));
            // Profile names are the user's own words: shown as typed, never looked up.
            let active = workspace.active.clone();
            crate::widgets::dropdown_menu(ui, "shortcut-profile", &active, 180.0, |ui| {
                for name in names {
                    ui.selectable_value(&mut workspace.active, name.clone(), &name);
                }
            });
        });
        // The profile commands on their own row, so the buttons keep their size in a narrow dialog.
        ui.horizontal_wrapped(|ui| {
            let mut name = f.get("profileName").and_then(Value::as_str).unwrap_or("").to_string();
            let name_field = ui.add(egui::TextEdit::singleline(&mut name).desired_width(120.0).hint_text("Profile name"));
            name_field.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Profile name"));
            f.insert("profileName".into(), json!(name));
            if crate::widgets::secondary_button(ui, "Duplicate", 0.0).clicked() {
                if let Err(error) = workspace.duplicate(&name) {
                    message = error;
                } else {
                    message.clear();
                }
            }
            let custom = workspace.custom.contains_key(&workspace.active);
            if ui.add_enabled_ui(custom, |ui| crate::widgets::secondary_button(ui, "Rename", 0.0)).inner.clicked()
                && let Err(error) = workspace.rename(&name)
            {
                message = error;
            }
            if ui.add_enabled_ui(custom, |ui| crate::widgets::secondary_button(ui, "Delete Profile", 0.0)).inner.clicked() {
                workspace.delete_active();
            }
        });
        if previous != workspace.active {
            overrides.retain(|id, _| id.starts_with("library::") != library);
            if let Some(profile) = workspace.custom.get(&workspace.active) {
                overrides.extend(profile.overrides.clone());
            }
            capture = false;
            selected.clear();
            message.clear();
        }
        if workspace.parent() == "Affinity Photo" {
            ui.label(RichText::new("Supported Affinity equivalents only; gaps are unassigned. R: Rotate View; J: Healing / AI Remove.").color(t.text_dim));
        }
        if workspace.parent() == "Capture One" {
            ui.label(RichText::new("B / E / G / M apply with Develop Edit or Mask active. L also selects Linear Gradient.").color(t.text_dim));
        }
        profile_defaults(app.session.prefs(), &profiles, &mut items);
    }
    let original_overrides = overrides.clone();
    // Menu path and command label as shown in the menus, in the UI language.
    let shown = |id: &str, label: &str, path: &[String]| -> (Vec<String>, String) {
        let lang = li_i18n::current();
        (path.iter().map(|p| li_i18n::tr(lang, p).to_string()).collect(), li_i18n::tr_id(lang, id, label).to_string())
    };
    let eff = |overrides: &BTreeMap<String, String>, id: &str, def: &Option<String>| -> Option<String> {
        match overrides.get(id) {
            Some(s) if s.is_empty() => None,
            Some(s) => Some(s.clone()),
            None => def.clone(),
        }
    };
    // Key capture for the selected command.
    if capture && tab == 0 && !selected.is_empty() {
        let pressed = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key { key, pressed: true, modifiers, .. } if !is_modifier_key(*key) => Some((*key, *modifiers)),
                _ => None,
            })
        });
        if let Some((key, m)) = pressed {
            ui.input_mut(|i| {
                i.consume_key(m, key);
            });
            if key == egui::Key::Escape {
                capture = false;
            } else if matches!(key, egui::Key::Backspace | egui::Key::Delete) && m.is_none() {
                overrides.insert(selected.clone(), String::new());
                capture = false;
                message = tl!("Shortcut removed.").into();
            } else if let Some(sc) = shortcut_text(key, m) {
                if let Some(action) = crate::keymaps::global_conflict(&sc) {
                    overrides.insert(selected.clone(), sc.clone());
                    message = format!("Conflict with Global: {action}. This binding is ignored.");
                    capture = false;
                } else if let Some(action) = reserved_library_chord(&selected, &sc) {
                    message = reserved_message(action);
                    capture = false;
                } else {
                    let clash: Vec<String> = items
                        .iter()
                        .filter(|(id, _, _, def)| {
                            *id != selected
                                && same_shortcut_scope(id, &selected)
                                && eff(&overrides, id, def).as_deref().is_some_and(|old| same_chord_in_scope(&selected, old, &sc))
                        })
                        .map(|(id, label, path, _)| {
                            let (path, label) = shown(id, label, path);
                            format!("{} › {}", path.join(" › "), label.trim_end_matches('…'))
                        })
                        .collect();
                    if selected.starts_with("library::") {
                        let displaced: Vec<_> = items
                            .iter()
                            .filter(|(id, _, _, def)| {
                                *id != selected
                                    && same_shortcut_scope(id, &selected)
                                    && eff(&overrides, id, def).as_deref().is_some_and(|old| same_chord_in_scope(&selected, old, &sc))
                            })
                            .map(|(id, _, _, _)| id.clone())
                            .collect();
                        for id in displaced {
                            overrides.insert(id, String::new());
                        }
                    }
                    overrides.insert(selected.clone(), sc.clone());
                    message = if clash.is_empty() {
                        format!("{} assigned.", crate::shortcuts::pretty(&sc))
                    } else {
                        format!("{} is already in use by {} and will be removed from it when you click OK.", crate::shortcuts::pretty(&sc), clash.join(", "))
                    };
                    capture = false;
                }
            }
        }
    }
    if message.is_empty()
        && let Some(chord) = overrides.get(&selected)
        && let Some(action) = reserved_library_chord(&selected, chord)
    {
        message = format!("Saved binding retained. {}", reserved_message(action));
    }
    let needle = filter.to_ascii_lowercase();
    let name_width = (ui.available_width() - 250.0).clamp(110.0, 440.0);
    egui::ScrollArea::both().max_height(360.0).id_salt("shortcuts-scroll").show(ui, |ui| {
        let mut last_top = String::new();
        egui::Grid::new("shortcut-grid").num_columns(3).spacing([12.0, 3.0]).striped(true).show(ui, |ui| {
            for (id, label, path, def) in &items {
                if tab == 0 && crate::keymaps::is_global(id) {
                    continue;
                }
                if tab == 0 && ((scope == "library" && !id.starts_with("library::")) || (scope == "editor" && id.starts_with("library::"))) {
                    continue;
                }
                let cur = eff(&overrides, id, def);
                let (shown_path, shown_label) = shown(id, label, path);
                // Match what is shown and the English name (commands are documented in English).
                let hay = format!("{} {} {} {} {}", shown_path.join(" "), shown_label, path.join(" "), label, cur.clone().unwrap_or_default()).to_lowercase();
                if !needle.is_empty() && !hay.contains(&needle) && !id.to_ascii_lowercase().contains(&needle) {
                    continue;
                }
                if tab == 0 && path.is_empty() && def.is_none() && cur.is_none() {
                    continue;
                }
                let top = shown_path.first().cloned().unwrap_or_else(|| tl!("Other").into());
                if top != last_top {
                    ui.label(RichText::new(&top).font(crate::theme::semibold(12.5)).color(t.text));
                    ui.label("");
                    ui.label("");
                    ui.end_row();
                    last_top = top;
                }
                let name = match shown_path.get(1..) {
                    Some(rest) if !rest.is_empty() => format!("{} › {}", rest.join(" › "), shown_label),
                    _ => shown_label,
                };
                let sel = selected == *id;
                if ui
                    .add_sized(
                        [name_width, 20.0],
                        egui::Button::selectable(sel, RichText::new(format!("   {name}")).color(t.text_dim)).wrap_mode(egui::TextWrapMode::Truncate),
                    )
                    .on_hover_text(&name)
                    .clicked()
                {
                    selected = id.clone();
                }
                if tab == 0 {
                    let text = if sel && capture { tl!("Press keys…").to_string() } else { cur.as_deref().map(crate::shortcuts::pretty).unwrap_or_default() };
                    let changed = overrides.contains_key(id);
                    let r = ui.add(
                        egui::Button::new(RichText::new(tl!(&text)).size(12.0).color(if changed { t.accent_text } else { t.text })).min_size(vec2(120.0, 18.0)),
                    );
                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("shortcut-capture", id)), r.rect));
                    if r.clicked() {
                        selected = id.clone();
                        capture = true;
                        message.clear();
                    }
                    let conflict = cur.as_deref().and_then(crate::keymaps::global_conflict).map(|action| format!("Global: {action}")).or_else(|| {
                        cur.as_deref().and_then(|chord| {
                            items
                                .iter()
                                .find(|(other, _, _, default)| {
                                    *other != *id
                                        && same_shortcut_scope(other, id)
                                        && !crate::keymaps::is_global(other)
                                        && eff(&overrides, other, default).as_deref().is_some_and(|key| same_chord_in_scope(id, key, chord))
                                })
                                .map(|(_, label, _, _)| format!("Conflict: {label}"))
                        })
                    });
                    ui.add_sized(
                        [100.0, 20.0],
                        egui::Label::new(
                            RichText::new(conflict.as_deref().unwrap_or(if changed { "modified" } else { "" }))
                                .color(if conflict.is_some() { t.warning } else { t.text_faint })
                                .size(10.5),
                        )
                        .wrap(),
                    )
                    .on_hover_text(conflict.as_deref().unwrap_or(""));
                } else {
                    let mut visible = !hidden.contains(id);
                    crate::widgets::checkbox(ui, &mut visible, tl!("Visible"));
                    if visible {
                        hidden.retain(|h| h != id);
                    } else if !hidden.contains(id) {
                        hidden.push(id.clone());
                    }
                    let mut col = colors.get(id).cloned().unwrap_or_else(|| "none".into());
                    let opts: Vec<(String, &str)> =
                        ["none", "red", "orange", "yellow", "green", "blue", "violet", "gray"].iter().map(|c| (c.to_string(), *c)).collect();
                    crate::widgets::dropdown(ui, &format!("menu-color-{id}"), &mut col, &opts, 90.0);
                    if col == "none" {
                        colors.remove(id);
                    } else {
                        colors.insert(id.clone(), col);
                    }
                }
                ui.end_row();
            }
        });
    });
    ui.add_space(6.0);
    if tab == 0 {
        ui.horizontal(|ui| {
            let has_sel = !selected.is_empty();
            let action = ui.add_enabled_ui(has_sel, |ui| crate::widgets::secondary_button(ui, tl!("Use Default"), 0.0)).inner;
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("shortcut-action", "default")), action.rect));
            if action.clicked() {
                overrides.remove(&selected);
                message = tl!("Default restored.").into();
            }
            let action = ui.add_enabled_ui(has_sel, |ui| crate::widgets::secondary_button(ui, tl!("Delete Shortcut"), 0.0)).inner;
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("shortcut-action", "delete")), action.rect));
            if action.clicked() {
                overrides.insert(selected.clone(), String::new());
                message = tl!("Shortcut removed.").into();
            }
            let action = crate::widgets::secondary_button(ui, tl!("Reset All to Defaults"), 0.0);
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("shortcut-action", "reset")), action.rect));
            if action.clicked() {
                overrides.retain(|id, _| id.starts_with("library::") != (scope == "library"));
                message = tl!("All shortcuts in this profile reset to defaults.").into();
            }
        });
    } else if crate::widgets::secondary_button(ui, tl!("Show All Menu Items"), 0.0).clicked() {
        hidden.clear();
        colors.clear();
    }
    if !message.is_empty() {
        ui.label(RichText::new(&message).color(if message.contains("already in use") || message.contains("reserved") || message.contains("Conflict") {
            t.warning
        } else {
            t.text_dim
        }));
    }
    if overrides_ok {
        if tab == 0 && overrides != original_overrides {
            let library = scope == "library";
            let workspace = profiles.workspace_mut(library);
            if !workspace.custom.contains_key(&workspace.active) {
                let mut name = format!("Custom ({})", workspace.active);
                while workspace.custom.contains_key(&name) {
                    name.push('2');
                }
                let _ = workspace.duplicate(&name);
            }
            if let Some(profile) = workspace.custom.get_mut(&workspace.active) {
                profile.overrides =
                    overrides.iter().filter(|(id, _)| id.starts_with("library::") == library).map(|(id, key)| (id.clone(), key.clone())).collect();
            }
        }
        f.insert("profiles".into(), json!(profiles));
        f.insert("overrides".into(), json!(overrides));
    }
    f.insert("hidden".into(), json!(hidden));
    f.insert("colors".into(), json!(colors));
    f.insert("selected".into(), json!(selected));
    f.insert("capture".into(), json!(capture));
    f.insert("message".into(), json!(message));
}

fn presets_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let mut kind = f.get("kind").and_then(Value::as_str).unwrap_or("brushes").to_string();
    ui.horizontal(|ui| {
        ui.label(RichText::new(tl!("Preset Type")).color(t.text_dim));
        let opts = [("brushes".to_string(), tl!("Brushes")), ("customShapes".to_string(), tl!("Custom Shapes")), ("patterns".to_string(), tl!("Patterns"))];
        crate::widgets::dropdown(ui, "preset-kind", &mut kind, &opts, 180.0);
    });
    let list = app
        .session
        .execute("edit.presets.presetManager", json!({"kind": kind}))
        .ok()
        .and_then(|v| v.get(&kind).cloned())
        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
        .unwrap_or_default();
    let mut selected = f.get("selected").and_then(Value::as_u64).unwrap_or(0) as usize;
    egui::ScrollArea::vertical().max_height(260.0).id_salt("preset-list").show(ui, |ui| {
        for (i, name) in list.iter().enumerate() {
            if ui.selectable_label(i == selected, name).clicked() {
                selected = i;
                f.insert("newName".into(), json!(name));
            }
        }
        if list.is_empty() {
            ui.label(RichText::new(tl!("No presets of this type.")).color(t.text_faint));
        }
    });
    let mut new_name = f.get("newName").and_then(Value::as_str).unwrap_or("").to_string();
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut new_name).desired_width(200.0));
        if ui.add_enabled(selected < list.len() && !new_name.trim().is_empty(), egui::Button::new(tl!("Rename"))).clicked() {
            let _ = app.run("edit.presets.presetManager", json!({"action": "rename", "kind": kind, "index": selected, "newName": new_name}));
        }
        if ui.add_enabled(selected < list.len(), egui::Button::new(tl!("Delete"))).clicked() {
            let _ = app.run("edit.presets.presetManager", json!({"action": "delete", "kind": kind, "index": selected}));
        }
        // Load Photoshop brushes (.abr) into the library.
        if kind == "brushes" && ui.button(tl!("Load…")).on_hover_text(tl!("Import Photoshop brushes (.abr)")).clicked() {
            app.open_dialog_file();
        }
    });
    f.insert("kind".into(), json!(kind));
    f.insert("selected".into(), json!(selected));
    f.insert("newName".into(), json!(new_name));
}

fn presets_io_body(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let mut action = f.get("action").and_then(Value::as_str).unwrap_or("export").to_string();
    ui.horizontal(|ui| {
        ui.label(RichText::new(tl!("Action")).color(t.text_dim));
        crate::widgets::dropdown(
            ui,
            "presets-io",
            &mut action,
            &[("export".to_string(), tl!("Export Presets")), ("import".to_string(), tl!("Import Presets"))],
            180.0,
        );
    });
    for (k, label) in [("brushes", tl!("Brushes")), ("customShapes", tl!("Custom Shapes"))] {
        let mut on = f.get(k).and_then(Value::as_bool).unwrap_or(true);
        crate::widgets::checkbox(ui, &mut on, label);
        f.insert(k.into(), json!(on));
    }
    f.insert("action".into(), json!(action));
}

fn mismatch_body(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(f.get("message").and_then(Value::as_str).unwrap_or("")).color(t.text));
    ui.add_space(6.0);
    let mut action = f.get("action").and_then(Value::as_str).unwrap_or("preserve").to_string();
    let missing = f.get("missing").and_then(Value::as_bool).unwrap_or(false);
    let opts: &[(&str, &str)] = if missing {
        &[("preserve", tl!("Leave as is (don't color manage)")), ("assignWorking", tl!("Assign working space"))]
    } else {
        &[
            ("preserve", tl!("Use the embedded profile (instead of the working space)")),
            ("convert", tl!("Convert document's colors to the working space")),
            ("discard", tl!("Discard the embedded profile (don't color manage)")),
        ]
    };
    for (v, label) in opts {
        ui.radio_value(&mut action, v.to_string(), *label);
    }
    f.insert("action".into(), json!(action));
}

// ------------------------------------------------------------------ confirm

/// OK on one of our dialogs.
pub fn confirm(app: &mut PhotocraftApp, f: &Map<String, Value>) -> Result<Value, String> {
    match f.get("__prefsui").and_then(Value::as_str).unwrap_or("") {
        "prefs" => {
            let values = f.get("values").cloned().unwrap_or(Value::Null);
            if let (Some(host), Some(draft)) = (&app.host_preferences, f.get("hostValues")) {
                host.validate(draft)?;
            }
            let result = app.run("prefs.set", json!({"path": "", "value": values}))?;
            if let (Some(host), Some(draft)) = (&mut app.host_preferences, f.get("hostValues")) {
                host.apply(draft)?;
            }
            crate::ai_ui::settings_ui::apply()?;
            if !app.ui.dialogs.iter().any(|dialog| is_preferences(&dialog.fields)) {
                crate::ai_ui::settings_ui::reset();
            }
            Ok(result)
        }
        "shortcuts" => {
            let overrides = f.get("overrides").cloned().unwrap_or(json!({}));
            // Shortcuts moved to another command are taken from their old owner (shell
            // commands included, which the engine doesn't know).
            // The map replaces every stored override, so a malformed one is an error, not "none".
            let mut ov: BTreeMap<String, String> =
                serde_json::from_value(overrides).map_err(|e| format!("overrides must map command IDs to shortcut strings: {e}"))?;
            let mut profiles: li_keys::ShortcutProfiles = match f.get("profiles") {
                Some(value) => serde_json::from_value(value.clone()).map_err(|e| e.to_string())?,
                None => app.session.prefs().shortcut_profiles.clone(),
            };
            let mut defaults = shortcut_items(app);
            defaults.extend(f.get("__hostShortcuts").and_then(|v| serde_json::from_value::<Vec<HostShortcutItem>>(v.clone()).ok()).unwrap_or_default());
            profile_defaults(app.session.prefs(), &profiles, &mut defaults);
            let changed: Vec<_> = ov.iter().filter(|(_, key)| !key.is_empty()).map(|(id, key)| (id.clone(), key.clone())).collect();
            for (id, key) in changed {
                if crate::keymaps::global_conflict(&key).is_some() {
                    continue;
                }
                let unchanged_library = id.starts_with("library::")
                    && app.session.prefs().shortcut_overrides(true).get(&id).and_then(|old| li_keys::normalize_shortcut(old)).as_deref()
                        == li_keys::normalize_shortcut(&key).as_deref();
                if let Some(action) = reserved_library_chord(&id, &key)
                    && !unchanged_library
                {
                    return Err(reserved_message(action));
                }
                // Reviewing an unchanged Library binding must not silently disable a default
                // elsewhere; only a changed assignment moves keys.
                if unchanged_library && profiles.library.parent() == app.session.prefs().shortcut_profiles.library.parent() {
                    continue;
                }
                for (other, _, _, default) in &defaults {
                    if *other != id
                        && !crate::keymaps::is_global(other)
                        && same_shortcut_scope(&id, other)
                        && !ov.contains_key(other)
                        && default.as_deref().is_some_and(|old| same_chord_in_scope(&id, old, &key))
                    {
                        ov.insert(other.clone(), String::new());
                    }
                }
            }
            for library in [false, true] {
                let workspace = profiles.workspace_mut(library);
                let current: BTreeMap<_, _> = ov.iter().filter(|(id, _)| id.starts_with("library::") == library).map(|(k, v)| (k.clone(), v.clone())).collect();
                let old = workspace.custom.get(&workspace.active).map(|p| p.overrides.clone()).unwrap_or_default();
                if current != old {
                    workspace.editable().overrides = current;
                }
            }
            // Apply through a single prefs transaction: invalid profiles cannot partially save.
            app.run("prefs.set", json!({"values": {"shortcutProfiles": profiles, "menus": {"hidden": f.get("hidden").cloned().unwrap_or(json!([])), "colors": f.get("colors").cloned().unwrap_or(json!({}))}}}))
        }
        "presets" => Ok(Value::Null),
        "presetsIO" => {
            let kinds: Vec<&str> = ["brushes", "customShapes"].into_iter().filter(|k| f.get(*k).and_then(Value::as_bool).unwrap_or(true)).collect();
            if f.get("action").and_then(Value::as_str) == Some("import") {
                let (name, bytes) = app.pick_file_bytes().ok_or_else(|| "cancelled".to_string())??;
                let text = String::from_utf8(bytes).map_err(|_| format!("{name} is not a preset file"))?;
                app.run("edit.presets.exportImportPresets", json!({"action": "import", "kinds": kinds, "data": text}))
            } else {
                let out = app.run("edit.presets.exportImportPresets", json!({"action": "export", "kinds": kinds}))?;
                let text = serde_json::to_string_pretty(&out["data"]).map_err(|e| e.to_string())?;
                let path = app.services.pick_save.as_mut().and_then(|p| p("Presets.pcpresets")).ok_or("cancelled")?;
                let write = app.services.write.as_mut().ok_or("no writer configured")?;
                write(&path, text.as_bytes())?;
                app.ui.status = format!("Exported presets to {path}");
                Ok(json!({"path": path}))
            }
        }
        "clipboard" => crate::clipboard_ui::confirm(app, f),
        "mismatch" => confirm_mismatch(app, f),
        _ => Err("unknown dialog".into()),
    }
}

fn confirm_mismatch(app: &mut PhotocraftApp, fields: &Map<String, Value>) -> Result<Value, String> {
    let target = fields.get("target").ok_or("The colour decision has no document; open the file again")?;
    let doc = target["doc"].as_u64().ok_or("The colour decision has no document")?;
    let index = app.session.documents().iter().position(|state| state.doc.id.0 == doc).ok_or("The colour decision's document was closed")?;
    let viewed = app.session.active().map(|state| state.doc.id);
    app.session.set_active(index);
    let result = (|| {
        if mismatch_target(&app.session).as_ref() != Some(target) {
            return Err("The document or colour settings changed; choose its profile again".into());
        }
        let action = fields.get("action").and_then(Value::as_str).unwrap_or("preserve");
        let applied = fields.get("applied").and_then(Value::as_str).unwrap_or("preserve");
        if action == applied {
            return Ok(json!({"action":action}));
        }
        let params = json!({"action":action});
        if app.automation_input
            && let Some(authorize) = app.services.automation_command.as_ref()
        {
            authorize("color.profileMismatch", &params)?;
        }
        let gate = if app.automation_input && app.session.authorize.is_none() { app.services.automation_authorize } else { None };
        if gate.is_some() {
            app.session.authorize = gate;
        }
        let suppress = app.automation_input && app.session.prefs().script_events.enabled;
        if suppress {
            app.session.edit_prefs(|prefs| prefs.script_events.enabled = false);
        }
        let result = app.session.execute("color.profileMismatch", params).map_err(|error| error.to_string());
        if suppress {
            app.session.edit_prefs(|prefs| prefs.script_events.enabled = true);
        }
        if gate.is_some() {
            app.session.authorize = None;
        }
        result
    })();
    if let Some(index) = viewed.and_then(|id| app.session.documents().iter().position(|state| state.doc.id == id)) {
        app.session.set_active(index);
    }
    app.sync_views();
    result
}

#[cfg(test)]
#[path = "shortcut_capture_tests.rs"]
mod shortcut_capture_tests;

#[cfg(test)]
mod tests {
    #[test]
    fn colour_preferences_take_a_typed_hex_value() {
        // #668: the canvas colour (and every colour preference) showed its hex but couldn't take one.
        use egui::accesskit::Role;
        use egui_kittest::kittest::Queryable;
        let mut h = egui_kittest::Harness::new_ui_state(|ui, rgb: &mut [u8; 3]| super::hex_field(ui, "interface.canvasCustomColor", rgb), [0x28, 0x28, 0x28]);
        h.run();
        assert_eq!(h.get_by_role(Role::TextInput).value().as_deref(), Some("#282828"));
        h.get_by_role(Role::TextInput).click();
        h.run_steps(1);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.run_steps(1);
        for ch in "#80".chars() {
            h.event(egui::Event::Text(ch.to_string()));
            h.run_steps(1);
        }
        // A partial value stays as typed and leaves the colour alone.
        assert_eq!(h.get_by_role(Role::TextInput).value().as_deref(), Some("#80"));
        assert_eq!(*h.state(), [0x28, 0x28, 0x28]);
        for ch in "8080".chars() {
            h.event(egui::Event::Text(ch.to_string()));
            h.run_steps(1);
        }
        assert_eq!(*h.state(), [0x80, 0x80, 0x80]);
    }

    #[test]
    fn rendering_mode_display_respects_explicit_mode() {
        let explicit = serde_json::json!({"renderingMode": "gpu", "gpuBackend": "cpu"});
        assert_eq!(super::rendering_mode_value(explicit.as_object().unwrap()), "gpu");
        let cpu = serde_json::json!({"renderingMode": "cpu", "gpuBackend": "auto"});
        assert_eq!(super::rendering_mode_value(cpu.as_object().unwrap()), "cpu");
        let automatic = serde_json::json!({"gpuBackend": "cpu"});
        assert_eq!(super::rendering_mode_value(automatic.as_object().unwrap()), "auto");
    }
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Every generated preference label, section title and choice label has an entry in each
    /// language that claims complete menus (Japanese, Traditional Chinese, ...).
    #[test]
    fn preference_labels_are_translated() {
        let session = photocraft_engine::Session::new();
        let v: Value = serde_json::from_str(&session.prefs_to_json()).expect("prefs json");
        for lang in li_i18n::Lang::ALL.into_iter().filter(|l| l.complete_menus()) {
            let mut missing = Vec::new();
            for (sec, title) in SECTIONS {
                if !li_i18n::has(lang, title) {
                    missing.push(title.to_string());
                }
                let Some(obj) = v.get(sec).and_then(Value::as_object) else { continue };
                for k in obj.keys() {
                    let mut labels = vec![humanize(k)];
                    labels.extend(prefs::choices(&format!("{sec}.{k}")).into_iter().flatten().map(|c| choice_label(c)));
                    if sec == "general" && k == "checkForUpdatesAtStartup" {
                        // CODEX-TASK.md explicitly defers this new string to the pending list.
                        // Keep every existing caption and all choice labels under the strict gate.
                        let caption = humanize(k);
                        let pending: &[&str] = include!("i18n_pending_vector.rs");
                        assert!(pending.contains(&caption.as_str()), "new update preference must be registered for translation");
                        labels.retain(|label| *label != caption);
                    }
                    missing.extend(labels.into_iter().filter(|l| !li_i18n::has(lang, l)));
                }
            }
            missing.sort();
            missing.dedup();
            assert!(missing.is_empty(), "{}: untranslated preference labels: {missing:#?}", lang.code());
        }
    }

    fn app_with_store() -> (PhotocraftApp, Arc<Mutex<Option<String>>>) {
        app_with_saved(None)
    }

    fn app_with_saved(text: Option<String>) -> (PhotocraftApp, Arc<Mutex<Option<String>>>) {
        let store: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(text));
        (app_on(&store, Arc::default()), store)
    }

    /// An app whose preferences live in `store`; writes fail while `fail` is set.
    fn app_on(store: &Arc<Mutex<Option<String>>>, fail: Arc<std::sync::atomic::AtomicBool>) -> PhotocraftApp {
        let (a, b) = (store.clone(), store.clone());
        let services = crate::Services {
            load_prefs: Some(Box::new(move || a.lock().unwrap().clone())),
            save_prefs: Some(Box::new(move |s: &str| {
                if fail.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err("storage is full".into());
                }
                *b.lock().unwrap() = Some(s.to_string());
                Ok(())
            })),
            ..Default::default()
        };
        PhotocraftApp::new(photocraft_engine::Session::new(), services)
    }

    fn stored(store: &Arc<Mutex<Option<String>>>) -> Value {
        serde_json::from_str(store.lock().unwrap().as_deref().unwrap_or("null")).unwrap()
    }

    #[test]
    fn transient_initial_read_failure_rebases_local_edits_and_next_save() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let original = r#"{"interface":{"theme":"studio"},"shortcutProfiles":{"editor":{"active":"Mine","custom":{"Mine":{"parent":"Photoshop","overrides":{"file.new":"Ctrl+Alt+N","file.open":"Ctrl+Shift+O"}}}}},"performance":{"historyStates":12},"future":{"keep":"original"}}"#;
        let store = Arc::new(Mutex::new(Some(original.to_string())));
        let failed = Arc::new(AtomicBool::new(true));
        let writes = Arc::new(AtomicUsize::new(0));
        let (read, write, read_failure, write_count) = (store.clone(), store.clone(), failed.clone(), writes.clone());
        let mut app = PhotocraftApp::new(
            photocraft_engine::Session::new(),
            crate::Services {
                load_prefs_checked: Some(Box::new(move || {
                    if read_failure.load(Ordering::Relaxed) {
                        return Err(crate::PrefsLoadError::Unreadable("temporarily locked".into()));
                    }
                    Ok(read.lock().unwrap().clone())
                })),
                save_prefs: Some(Box::new(move |text| {
                    write_count.fetch_add(1, Ordering::Relaxed);
                    *write.lock().unwrap() = Some(text.into());
                    Ok(())
                })),
                ..Default::default()
            },
        );
        app.run("prefs.set", json!({"values":{"interface.theme":"classic"}})).unwrap();
        assert_eq!(persist(&mut app, 100.), Some(SAVE_RETRY_S));
        assert_eq!(writes.load(Ordering::Relaxed), 0, "unreadable source must never reach the writer");
        assert_eq!(store.lock().unwrap().as_deref(), Some(original));
        failed.store(false, Ordering::Relaxed);
        assert_eq!(persist(&mut app, 102.), Some(0.));
        let saved = stored(&store);
        assert_eq!(saved["interface"]["theme"], "classic");
        assert_eq!(saved["shortcutProfiles"]["editor"]["custom"]["Mine"]["overrides"], json!({"file.new":"Ctrl+Alt+N","file.open":"Ctrl+Shift+O"}));
        assert_eq!(saved["performance"]["historyStates"], 12);
        assert_eq!(saved["future"]["keep"], "original");
        assert_eq!(app.session.prefs_value()["shortcutProfiles"]["editor"], saved["shortcutProfiles"]["editor"], "live prefs must be rebased too");
        assert_eq!(app.session.prefs_value()["performance"]["historyStates"], 12);
        assert_eq!(app.prefs_rt.saved_value, Some(app.session.prefs_value()));
        // A later ordinary save must not write old default shortcuts back over the recovered file.
        app.run("prefs.set", json!({"values":{"performance.historyStates":32}})).unwrap();
        assert_eq!(persist(&mut app, 103.), None);
        let next = stored(&store);
        assert_eq!(next["shortcutProfiles"], saved["shortcutProfiles"]);
        assert_eq!(next["performance"]["historyStates"], 32);
        assert_eq!(next["future"], saved["future"]);
        assert_eq!(writes.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn checked_preferences_keep_three_way_merge_and_persistent_recovery_location() {
        let store = Arc::new(Mutex::new(Some(r#"{"interface":{"theme":"studio"},"future":{"value":1}}"#.to_string())));
        let read = store.clone();
        let write = store.clone();
        let mut app = PhotocraftApp::new(
            photocraft_engine::Session::new(),
            crate::Services {
                load_prefs: Some(Box::new(|| Some("{broken legacy loader".into()))),
                load_prefs_checked: Some(Box::new(move || Ok(read.lock().unwrap().clone()))),
                save_prefs: Some(Box::new(move |text| {
                    *write.lock().unwrap() = Some(text.into());
                    Ok(())
                })),
                prefs_recovery: Some(Box::new(|| {
                    Some(crate::PrefsRecoveryNotice {
                        backup_path: "/settings/preference-backups/original.json".into(),
                        reason: "Invalid fields used defaults".into(),
                    })
                })),
                ..Default::default()
            },
        );
        assert_eq!(app.session.prefs().interface.theme, Theme::Studio);
        app.run("prefs.set", json!({"values":{"interface.theme":"classic"}})).unwrap();
        *store.lock().unwrap() = Some(r#"{"interface":{"theme":"studio"},"future":{"value":2,"otherWindow":true}}"#.into());
        assert_eq!(persist(&mut app, 100.), None);
        let saved = stored(&store);
        assert_eq!(saved["interface"]["theme"], "classic");
        assert_eq!(saved["future"], json!({"value":2,"otherWindow":true}));
        app.ui.status = "a later action replaced the status text".into();
        show_prefs_recovery(&mut app);
        let notices: Vec<_> = app.ui.notices.iter().filter(|notice| notice.title == "Original preferences preserved").collect();
        assert_eq!(notices.len(), 1);
        assert!(notices[0].lines.iter().any(|line| line.contains("/settings/preference-backups/original.json")));
        assert!(notices[0].dismiss_pref.is_some());
    }

    #[test]
    fn merge_keeps_changes_made_elsewhere() {
        let base = json!({"a": {"x": 1, "y": 1}, "list": [1], "gone": 1, "kept": 1});
        // Here: a.x changed, `gone` removed, `new` added.
        let ours = json!({"a": {"x": 2, "y": 1}, "list": [1], "kept": 1, "new": 1});
        // There: a.y, `list` and `kept` changed, and a key this version doesn't know.
        let mut theirs = json!({"a": {"x": 1, "y": 3}, "list": [1, 2], "gone": 1, "kept": 5, "future": true});
        merge_changes(&base, &ours, &mut theirs);
        assert_eq!(theirs, json!({"a": {"x": 2, "y": 3}, "list": [1, 2], "kept": 5, "new": 1, "future": true}));
        // Arrays are leaves: a list changed here replaces the stored one whole.
        merge_changes(&json!({"list": [1]}), &json!({"list": [1, 3]}), &mut theirs);
        assert_eq!(theirs["list"], json!([1, 3]));
        // Nothing changed here: storage is left as it is, whatever it holds.
        let mut odd = json!({"x": [7]});
        merge_changes(&ours, &ours, &mut odd);
        assert_eq!(odd, json!({"x": [7]}));
        // A value that changed type here replaces the stored one.
        merge_changes(&json!({"a": 1}), &json!({"a": {"b": 2}}), &mut theirs);
        assert_eq!(theirs["a"], json!({"b": 2}));
    }

    #[test]
    fn saving_keeps_preferences_another_instance_changed() {
        let store = Arc::new(Mutex::new(None));
        let ctx = egui::Context::default();
        // Two browser tabs (or app windows) on the same storage.
        let (mut a, mut b) = (app_on(&store, Arc::default()), app_on(&store, Arc::default()));
        tick(&mut a, &ctx);
        tick(&mut b, &ctx);
        a.run("prefs.set", json!({"values": {"interface.theme": "studio"}})).unwrap();
        tick(&mut a, &ctx);
        assert_eq!(stored(&store)["interface"]["theme"], "studio");
        // The other one, never reloaded, saves an unrelated change: the theme stays.
        b.run("prefs.set", json!({"values": {"performance.historyStates": 12}})).unwrap();
        tick(&mut b, &ctx);
        let v = stored(&store);
        assert_eq!((v["interface"]["theme"].as_str(), v["performance"]["historyStates"].as_u64()), (Some("studio"), Some(12)));
        // And the first one's next save keeps the second one's change.
        a.run("prefs.set", json!({"values": {"interface.showTooltips": false}})).unwrap();
        tick(&mut a, &ctx);
        let v = stored(&store);
        assert_eq!(v["performance"]["historyStates"], 12);
        assert_eq!(v["interface"]["showTooltips"], false);
        // A value changed in both: the latest save wins.
        b.run("prefs.set", json!({"values": {"interface.theme": "classic"}})).unwrap();
        tick(&mut b, &ctx);
        assert_eq!(stored(&store)["interface"]["theme"], "classic");
        // Unreadable storage is overwritten with this instance's preferences.
        *store.lock().unwrap() = Some("not json".into());
        a.run("prefs.set", json!({"values": {"performance.historyStates": 30}})).unwrap();
        tick(&mut a, &ctx);
        let (mut c, _) = app_with_saved(store.lock().unwrap().clone());
        tick(&mut c, &ctx);
        assert_eq!(c.session.prefs().performance.history_states, 30);
        assert_eq!(c.session.prefs().interface.theme, Theme::Studio);
    }

    #[test]
    fn failed_preference_saves_are_retried() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let store = Arc::new(Mutex::new(None));
        let fail = Arc::new(AtomicBool::new(true));
        let mut app = app_on(&store, fail.clone());
        tick(&mut app, &egui::Context::default());
        app.run("prefs.set", json!({"values": {"interface.theme": "studio"}})).unwrap();
        // The write fails: reported, and retried after a backoff rather than marked saved.
        assert_eq!(persist(&mut app, 100.0), Some(SAVE_RETRY_S));
        assert!(app.ui.status.starts_with("Couldn't save preferences: storage is full") && app.ui.status_error);
        assert!(store.lock().unwrap().is_none());
        assert_eq!(persist(&mut app, 101.0), Some(1.0), "not due yet");
        // Storage works again: the pending change is written without another edit.
        fail.store(false, Ordering::Relaxed);
        assert_eq!(persist(&mut app, 102.0), None);
        assert_eq!(stored(&store)["interface"]["theme"], "studio");
        assert_eq!(app.ui.status, "Preferences saved");
        assert_eq!(persist(&mut app, 103.0), None, "nothing left to save");

        // A persistent failure backs off up to a cap and raises a notice once.
        fail.store(true, Ordering::Relaxed);
        app.run("prefs.set", json!({"values": {"interface.theme": "classic"}})).unwrap();
        let mut now = 200.0;
        let mut delays = Vec::new();
        for _ in 0..6 {
            let wait = persist(&mut app, now).unwrap();
            delays.push(wait);
            now += wait;
        }
        assert_eq!(delays, [2.0, 4.0, 8.0, 16.0, SAVE_RETRY_MAX_S, SAVE_RETRY_MAX_S]);
        assert_eq!(app.ui.notices.iter().filter(|n| n.error && n.title == "Preferences can't be saved").count(), 1);
        // A newer change retries sooner than the backoff, but not on every frame.
        app.run("prefs.set", json!({"values": {"interface.theme": "pro"}})).unwrap();
        assert_eq!(persist(&mut app, now - 29.0), Some(1.0));
        fail.store(false, Ordering::Relaxed);
        assert_eq!(persist(&mut app, now - 28.0), None);
        assert_eq!(stored(&store)["interface"]["theme"], "pro");
        assert!(app.ui.notices.is_empty(), "the notice goes once preferences are saved");
    }

    #[test]
    fn auto_scale_detects_4k_and_preserves_larger_system_dpi() {
        use prefs::UiScale::Auto;
        for size in [vec2(3840.0, 2160.0), vec2(4096.0, 2160.0), vec2(2160.0, 3840.0)] {
            assert_eq!(display_scale(Auto, Some(1.0), Some(size)), 2.0);
            for dpi in [1.25, 1.5, 2.0] {
                assert_eq!(display_scale(Auto, Some(dpi), Some(size)), 2.0);
            }
        }
        assert_eq!(display_scale(Auto, Some(3.0), Some(vec2(3840.0, 2160.0))), 3.0);
        for size in [vec2(1920.0, 1080.0), vec2(2560.0, 1440.0), vec2(3840.0, 1080.0)] {
            assert_eq!(display_scale(Auto, Some(1.0), Some(size)), 1.0);
        }
        assert_eq!(display_scale(Auto, None, None), 1.0);
        assert_eq!(display_scale(Auto, Some(f32::NAN), Some(vec2(f32::INFINITY, 2160.0))), 1.0);
    }

    #[test]
    fn scale_preferences_and_monitor_changes_apply_live() {
        let (mut app, _) = app_with_store();
        let ctx = egui::Context::default();
        {
            let mut step = |physical: egui::Vec2, native: f32, expected: f32| {
                let mut input = egui::RawInput::default();
                let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
                viewport.native_pixels_per_point = Some(native);
                viewport.monitor_size = Some(physical / (native * ctx.zoom_factor()));
                ctx.run_ui(input, |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
                // Scale changes take effect on the following pass.
                let mut input = egui::RawInput::default();
                let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
                viewport.native_pixels_per_point = Some(native);
                viewport.monitor_size = Some(physical / (native * ctx.zoom_factor()));
                ctx.run_ui(input, |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
                assert!((ctx.pixels_per_point() - expected).abs() < 1e-4);
            };
            for _ in 0..4 {
                step(vec2(3840.0, 2160.0), 1.0, 2.0);
            }
            step(vec2(1920.0, 1080.0), 1.0, 1.0);
            step(vec2(3840.0, 2160.0), 1.5, 2.0);
        }
        for (pref, expected) in [("200", 2.0), ("125", 1.25), ("150", 1.5), ("100", 1.0), ("auto", 1.5)] {
            app.run("prefs.set", json!({"values": {"interface.uiScale": pref}})).unwrap();
            let mut input = egui::RawInput::default();
            input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap().native_pixels_per_point = Some(1.5);
            ctx.run_ui(input.clone(), |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
            ctx.run_ui(input, |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
            assert!((ctx.pixels_per_point() - expected).abs() < 1e-4);
        }
    }

    #[test]
    fn ui_font_size_commands_follow_themes_and_dpi_and_reset_live() {
        let (mut app, _) = app_with_store();
        let ctx = egui::Context::default();
        PhotocraftApp::setup_context(&ctx, ThemeKind::Pro);
        let mut input = egui::RawInput::default();
        input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap().native_pixels_per_point = Some(1.5);
        for (theme, scale, expected) in
            [("pro", "100", 1.0), ("studio", "auto", 1.5), ("studioLight", "200", 2.0), ("proMedium", "100", 1.0), ("classic", "auto", 1.5)]
        {
            app.run("prefs.set", json!({"values": {"interface.uiFontSize": "large", "interface.theme": theme, "interface.uiScale": scale}})).unwrap();
            for _ in 0..3 {
                ctx.run_ui(input.clone(), |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
            }
            assert!((ctx.pixels_per_point() - expected).abs() < 1e-4);
            assert_eq!(ctx.fonts(|f| f.definitions().font_data["Inter"].tweak.scale), 16.0 / 12.0);
        }
        app.run("prefs.reset", json!({"path": "interface.uiFontSize"})).unwrap();
        for _ in 0..2 {
            ctx.run_ui(input.clone(), |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
        }
        assert_eq!(ctx.fonts(|f| f.definitions().font_data["Inter"].tweak.scale), 1.0);
        assert!((ctx.pixels_per_point() - 1.5).abs() < 1e-4, "resetting font size keeps UI Scale");
    }

    #[test]
    fn recent_files_survive_a_restart_and_honour_the_count() {
        let (mut app, store) = app_with_store();
        let ctx = egui::Context::default();
        tick(&mut app, &ctx);
        app.push_recent("/work/a.psd");
        app.push_recent("/work/b.png");
        tick(&mut app, &ctx);
        let saved = store.lock().unwrap().clone().unwrap();
        // A new app instance (a restart) gets the list back, newest first.
        let (mut app2, _) = app_with_saved(Some(saved));
        tick(&mut app2, &ctx);
        assert_eq!(app2.ui.recent_files, vec!["/work/b.png".to_string(), "/work/a.psd".to_string()]);
        // Lowering "Recent File List Contains" shortens the menu at once; 0 turns it off.
        app2.run("prefs.set", json!({"values": {"fileHandling.recentFileCount": 1}})).unwrap();
        tick(&mut app2, &ctx);
        assert_eq!(app2.ui.recent_files, vec!["/work/b.png".to_string()]);
        app2.push_recent("/work/c.tif");
        assert_eq!(app2.session.prefs().file_handling.recent_files, vec!["/work/c.tif".to_string()]);
        app2.run("prefs.set", json!({"values": {"fileHandling.recentFileCount": 0}})).unwrap();
        tick(&mut app2, &ctx);
        app2.push_recent("/work/d.tif");
        assert!(app2.ui.recent_files.is_empty());
        // Clearing the list in the Preferences dialog (or by an agent) reaches the menu.
        app2.run("prefs.set", json!({"values": {"fileHandling.recentFileCount": 20, "fileHandling.recentFiles": ["/x.psd"]}})).unwrap();
        tick(&mut app2, &ctx);
        assert_eq!(app2.ui.recent_files, vec!["/x.psd".to_string()]);
        // A hostile count from a hand-edited preferences file is capped, not trusted.
        app2.run("prefs.set", json!({"values": {"fileHandling.recentFileCount": 100}})).unwrap();
        app2.session.prefs.edit(|p| p.file_handling.recent_file_count = u32::MAX);
        assert_eq!(app2.recent_cap(), 100);
    }

    #[test]
    fn show_tooltips_preferences_turn_tooltips_off() {
        let (mut app, _) = app_with_store();
        let ctx = egui::Context::default();
        tick(&mut app, &ctx);
        assert_eq!(ctx.global_style().interaction.tooltip_delay, crate::theme::TOOLTIP_DELAY);
        for path in ["interface.showTooltips", "tools.showTooltips"] {
            app.run("prefs.set", json!({"values": {path: false}})).unwrap();
            tick(&mut app, &ctx);
            assert!(ctx.global_style().interaction.tooltip_delay.is_infinite(), "{path} off hides tooltips");
            app.run("prefs.set", json!({"values": {path: true}})).unwrap();
            tick(&mut app, &ctx);
            assert_eq!(ctx.global_style().interaction.tooltip_delay, crate::theme::TOOLTIP_DELAY);
        }
    }

    #[test]
    fn unimplemented_preferences_are_hidden_from_the_dialog() {
        let values = prefs::Preferences::default().to_json();
        assert!(has_visible_fields(&values, "general"));
        assert!(has_visible_fields(&values, "fileHandling"));
        // Every setting of these sections is still unimplemented.
        for section in ["type", "enhancedControls", "rawDefaults", "integrations", "scratchDisks"] {
            assert!(!has_visible_fields(&values, section), "{section}");
        }
        assert!(prefs::is_hidden("rawDefaults.applyAutoTone"));
        assert!(!prefs::is_hidden("general.autoShowHomeScreen"));
        assert!(!prefs::is_hidden("interface.uiScale"));
        assert!(!prefs::is_hidden("interface.uiFontSize"));
        // Hidden values still round-trip through the dialog untouched.
        let (mut app, _) = app_with_store();
        app.run("prefs.set", json!({"values": {"type.smartQuotes": false}})).unwrap();
        let id = open_preferences(&mut app, "type");
        let fields = app.ui.dialogs.iter().find(|d| d.id == id).map(|d| d.fields.clone()).unwrap();
        confirm(&mut app, &fields).unwrap();
        assert!(!app.session.prefs().type_.smart_quotes);
    }

    #[test]
    fn preferences_persist_through_services_and_theme_follows() {
        let (mut app, store) = app_with_store();
        let ctx = egui::Context::default();
        tick(&mut app, &ctx);
        app.run("prefs.set", json!({"values": {"interface.theme": "studioLight", "performance.historyStates": 12}})).unwrap();
        tick(&mut app, &ctx);
        assert_eq!(app.ui.theme, ThemeKind::StudioLight);
        let saved = store.lock().unwrap().clone().unwrap();
        assert!(saved.contains("\"historyStates\": 12"));
        // A new app instance loads them.
        let (mut app2, store2) = app_with_saved(Some(saved));
        tick(&mut app2, &ctx);
        assert_eq!(app2.session.prefs().performance.history_states, 12);
        assert_eq!(app2.ui.theme, ThemeKind::StudioLight);
        // Picking a theme from the Window menu becomes the preference.
        crate::menus::invoke(&mut app2, &ctx, "window.theme.classic", json!({})).unwrap();
        tick(&mut app2, &ctx);
        assert_eq!(app2.session.prefs().interface.theme, Theme::Classic);
        assert!(store2.lock().unwrap().as_ref().unwrap().contains("classic"));
    }

    #[test]
    fn brush_preset_store_attaches_when_loaded_and_persists() {
        use photocraft_engine::preset_store::{MemBackend, open};
        let mem = MemBackend::default();
        let ctx = egui::Context::default();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services { preset_store: Some(rx), ..Default::default() });
        // Still loading: presets made now are kept and written once the store arrives.
        tick(&mut app, &ctx);
        app.run("brush.presets.save", json!({"name": "Early"})).unwrap();
        assert!(mem.files.lock().unwrap().is_empty());
        tx.send(open(Box::new(mem.clone()))).unwrap();
        tick(&mut app, &ctx);
        assert!(app.services.preset_store.is_none() && app.session.preset_store.is_some());
        app.run("brush.presets.save", json!({"name": "Late"})).unwrap();
        let mut s2 = photocraft_engine::Session::new();
        assert!(s2.attach_preset_store(open(Box::new(mem))).is_empty());
        for n in ["Early", "Late"] {
            assert!(s2.tools.presets.iter().any(|p| p.name == n), "{n}");
        }
    }

    #[test]
    fn j16_ai_preferences_real_click_apply_updates_the_single_root() {
        use egui_kittest::{Harness, kittest::Queryable};
        let initial = serde_json::to_string(&prefs::Preferences::default()).unwrap();
        let (app, store) = app_with_saved(Some(initial));
        let read = store.clone();
        let write = store.clone();
        let backend = li_ai::settings::StoreBackend {
            load: Arc::new(move || {
                Ok(read.lock().unwrap().as_ref().and_then(|text| serde_json::from_str::<Value>(text).ok()).and_then(|root| root.get("ai").cloned()))
            }),
            save: Arc::new(move |ai| {
                let mut saved = write.lock().unwrap();
                let mut root: Value = serde_json::from_str(saved.as_ref().unwrap()).unwrap();
                root["ai"] = ai;
                *saved = Some(serde_json::to_string(&root).unwrap());
                Ok(())
            }),
        };
        li_ai::settings::with_store_backend(backend, || {
            let mut app = app;
            let id = open_preferences(&mut app, "ai.connection");
            let mut harness = Harness::builder().with_size(vec2(1100.0, 850.0)).build_ui_state(
                |ui, app: &mut PhotocraftApp| {
                    if !super::design_system_shots::fonts_ready(ui) {
                        return;
                    }
                    crate::dialogs::show(app, ui.ctx());
                },
                app,
            );
            harness.run_steps(4);
            harness.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Use a remote ComfyUI server").click();
            harness.run();
            assert_eq!(harness.state().ui.dialogs.len(), 1);
            assert_eq!(harness.state().ui.dialogs[0].id, id);
            harness.get_by_label("Apply").click();
            harness.run();
            let root: Value = serde_json::from_str(store.lock().unwrap().as_ref().unwrap()).unwrap();
            assert_eq!(root["ai"]["setup_mode"], "remote");
            assert!(root.get("library").is_some());
            assert!(root.get("general").is_some());
            assert_eq!(harness.state().ui.dialogs.len(), 1);
            harness.get_by_label("Cancel").click();
            harness.run();
            assert!(harness.state().ui.dialogs.is_empty());
        });
    }

    #[test]
    fn preferences_dialog_round_trip() {
        let (mut app, _) = app_with_store();
        let ctx = egui::Context::default();
        let r = crate::menus::invoke(&mut app, &ctx, "edit.preferences.cursors", json!({})).unwrap();
        let id = r["dialog"].as_u64().unwrap();
        let d = app.ui.dialogs.iter().find(|d| d.id == id).unwrap().clone();
        assert_eq!(d.fields["section"], "cursors");
        let mut values = d.fields["values"].clone();
        values["cursors"]["painting"] = json!("fullSizeTip");
        values["unitsAndRulers"]["rulers"] = json!("inches");
        app.ui.dialog_mut(id).unwrap().fields.insert("values".into(), values);
        crate::dialogs::confirm(&mut app, id).unwrap();
        assert_eq!(app.session.prefs().cursors.painting, prefs::PaintingCursor::FullSizeTip);
        assert_eq!(app.session.prefs().units_and_rulers.rulers, prefs::Unit::Inches);
        // Invalid values are rejected and nothing changes.
        let id = open_preferences(&mut app, "performance");
        let mut values = app.ui.dialogs.iter().find(|d| d.id == id).unwrap().fields["values"].clone();
        values["performance"]["historyStates"] = json!(0);
        app.ui.dialog_mut(id).unwrap().fields.insert("values".into(), values);
        assert!(crate::dialogs::confirm(&mut app, id).is_err());
        assert_eq!(app.session.prefs().performance.history_states, 50);
    }

    /// local-image: Performance › Processing shows the automatic values and stores the optional
    /// overrides (thread count, background priority, memory share); out-of-range values set by path
    /// are refused.
    #[test]
    fn processing_overrides_show_auto_values_and_round_trip() {
        use egui_kittest::{Harness, kittest::Queryable};

        let (mut app, _store) = app_with_store();
        assert_eq!(app.session.prefs().performance.system, prefs::SystemResources::default());
        let id = open_preferences(&mut app, "performance");
        let mut h = Harness::builder().with_size(vec2(1280.0, 900.0)).build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            app
        });
        h.run_steps(4);
        let processing = h.get_by_label(PROCESSING_THREADS).rect();
        h.hover_at(egui::pos2(processing.center().x, h.ctx.content_rect().center().y));
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -800.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(4);
        h.get_by_label("Keep full speed on battery").click();
        h.run_steps(2);
        assert_eq!(h.state().ui.dialogs.iter().find(|d| d.id == id).unwrap().fields["values"]["performance"]["system"]["fullSpeedOnBattery"], true);
        let auto = format!("Automatic ({})", lightcraft_sysmem::profile().threads_auto);
        assert!(h.query_by_label(PROCESSING_THREADS).is_some());
        // The dropdown shows its choice as the combo box's value (a label in some egui versions).
        assert!(h.query_by_value(&auto).is_some() || h.query_by_label(&auto).is_some(), "the automatic thread count is shown");
        assert!(h.query_by_label_contains("physical cores").is_some(), "the detected hardware is shown");
        let values = h.state_mut().ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap();
        values["performance"]["system"] = json!({"threads": 3, "backgroundPriority": "normal", "memoryPercent": 60});
        h.run_steps(2);
        let app = h.state_mut();
        crate::dialogs::confirm(app, id).unwrap();
        let c = &app.session.prefs().performance.system;
        assert_eq!((c.threads, c.background_priority, c.memory_percent), (3, prefs::BackgroundPriority::Normal, 60));
        assert!(app.run("prefs.set", json!({"values": {"performance.system.memoryPercent": 95}})).is_err());
        assert!(app.run("prefs.set", json!({"values": {"performance.system.backgroundPriority": "idle"}})).is_err());
        app.run("prefs.set", json!({"values": {"performance.system.threads": 0}})).unwrap();
        assert_eq!((app.session.prefs().performance.system.threads, app.session.prefs().performance.system.memory_percent), (0, 60));
    }

    #[test]
    fn preferences_apply_button_saves_without_closing_and_cancel_keeps_applied_values() {
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };

        let (mut app, store) = app_with_store();
        app.run("prefs.set", json!({"values": {"interface.language": "en", "type.smartQuotes": false}})).unwrap();
        let id = open_preferences(&mut app, "interface");
        let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            app
        });
        h.run_steps(4);
        assert!(h.get_by_label("Apply").accesskit_node().is_disabled());

        let values = h.state_mut().ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap();
        values["interface"]["theme"] = json!("studioLight");
        values["interface"]["uiFontSize"] = json!("large");
        values["performance"]["historyStates"] = json!(12);
        h.run_steps(2);
        assert!(!h.get_by_label("Apply").accesskit_node().is_disabled());
        h.get_by_label("Apply").click();
        h.run_steps(4);

        assert_eq!(h.state().session.prefs().performance.history_states, 12);
        assert_eq!(h.state().ui.theme, ThemeKind::StudioLight);
        assert_eq!(h.ctx.fonts(|f| f.definitions().font_data["Inter"].tweak.scale), 16.0 / 12.0);
        assert!(!h.state().session.prefs().type_.smart_quotes, "hidden settings round-trip unchanged");
        let d = h.state().ui.dialogs.iter().find(|d| d.id == id).unwrap();
        assert_eq!(d.fields["section"], "interface");
        assert_eq!(d.fields["values"]["performance"]["historyStates"], 12);
        assert!(h.get_by_label("Apply").accesskit_node().is_disabled());
        let saved: Value = serde_json::from_str(store.lock().unwrap().as_ref().unwrap()).unwrap();
        assert_eq!(saved["performance"]["historyStates"], 12);
        assert_eq!(saved["interface"]["theme"], "studioLight");
        assert_eq!(saved["interface"]["uiFontSize"], "large");

        // Repeated Apply starts from the validated values, not the dialog's original snapshot.
        h.state_mut().ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["performance"]["historyStates"] = json!(22);
        h.run_steps(2);
        h.get_by_label("Apply").click();
        h.run_steps(4);
        assert_eq!(h.state().session.prefs().performance.history_states, 22);
        h.state_mut().ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["performance"]["historyStates"] = json!(33);
        h.state_mut().ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["interface"]["uiFontSize"] = json!("tiny");
        h.run_steps(2);
        h.get_by_label("Cancel").click();
        h.run_steps(4);
        assert!(h.state().ui.dialogs.iter().all(|d| d.id != id));
        assert_eq!(h.state().session.prefs().performance.history_states, 22);
        assert_eq!(h.ctx.fonts(|f| f.definitions().font_data["Inter"].tweak.scale), 16.0 / 12.0);
        let saved = store.lock().unwrap().clone().unwrap();
        let (mut restarted, _) = app_with_saved(Some(saved));
        tick(&mut restarted, &egui::Context::default());
        assert_eq!(restarted.session.prefs().performance.history_states, 22);
        assert_eq!(restarted.ui.theme, ThemeKind::StudioLight);
        assert_eq!(restarted.session.prefs().interface.ui_font_size, prefs::UiFontSize::Large);
    }

    #[test]
    fn preferences_confirm_after_apply_commits_later_edits() {
        let (mut app, _) = app_with_store();
        let id = open_preferences(&mut app, "cursors");
        app.ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["cursors"]["painting"] = json!("fullSizeTip");
        apply(&mut app, id).unwrap();
        app.ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["unitsAndRulers"]["rulers"] = json!("inches");
        crate::dialogs::confirm(&mut app, id).unwrap();
        assert!(app.ui.dialogs.iter().all(|d| d.id != id));
        assert_eq!(app.session.prefs().cursors.painting, prefs::PaintingCursor::FullSizeTip);
        assert_eq!(app.session.prefs().units_and_rulers.rulers, prefs::Unit::Inches);
    }

    #[test]
    fn preferences_apply_rejects_invalid_drafts_and_keeps_them_open() {
        let (mut app, _) = app_with_store();
        let before = app.session.prefs().to_json();
        for invalid in [json!(0), json!("invalid"), Value::Null] {
            let id = open_preferences(&mut app, "performance");
            app.ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["performance"]["historyStates"] = invalid;
            let draft = app.ui.dialog_mut(id).unwrap().fields.clone();
            assert!(apply(&mut app, id).is_err());
            assert_eq!(app.session.prefs().to_json(), before);
            assert_eq!(app.ui.dialog_mut(id).unwrap().fields, draft);
            app.ui.close_dialog(id);
        }
        let id = open_preferences(&mut app, "interface");
        app.ui.dialog_mut(id).unwrap().fields.remove("values");
        assert!(apply(&mut app, id).is_err());
        assert!(app.ui.dialog_mut(id).is_some());
        assert_eq!(app.session.prefs().to_json(), before);
    }

    #[test]
    fn apply_rejects_missing_and_non_preferences_dialogs() {
        let (mut app, _) = app_with_store();
        let before = app.session.prefs().to_json();
        assert!(apply(&mut app, u64::MAX).is_err());
        let id = open_shortcuts(&mut app, 0);
        assert!(apply(&mut app, id).is_err());
        assert!(app.ui.dialog_mut(id).is_some());
        let fields = json!({"__prefsui": "prefs", "values": preference_values(app.session.prefs())}).as_object().unwrap().clone();
        let id = app.ui.open_dialog(DialogKind::About, fields);
        assert!(apply(&mut app, id).is_err());
        assert!(app.ui.dialog_mut(id).is_some());
        assert_eq!(app.session.prefs().to_json(), before);
    }

    /// Upstream 57be0f58 (#956): a non-map `overrides` was read as "no overrides" and wiped every
    /// custom shortcut.
    #[test]
    fn shortcut_dialog_rejects_malformed_overrides() {
        let (mut app, _) = app_with_store();
        let ctx = egui::Context::default();
        app.run("edit.keyboardShortcuts", json!({"set": {"file.new": "Ctrl+Shift+N"}})).unwrap();
        let before = app.session.prefs().shortcut_profiles.clone();
        assert!(!before.editor.custom.is_empty());
        for bad in [json!("bogus"), json!(["file.new"]), json!({"file.new": 5})] {
            let id = crate::menus::invoke(&mut app, &ctx, "edit.keyboardShortcuts", json!({})).unwrap()["dialog"].as_u64().unwrap();
            app.ui.dialog_mut(id).unwrap().fields.insert("overrides".into(), bad.clone());
            assert!(crate::dialogs::confirm(&mut app, id).is_err(), "{bad} accepted");
            assert_eq!(app.session.prefs().shortcut_profiles, before, "{bad} changed the shortcuts");
        }
        // Drawing the open dialog keeps the malformed value for OK to reject, instead of
        // replacing it with the empty map it shows.
        let id = crate::menus::invoke(&mut app, &ctx, "edit.keyboardShortcuts", json!({})).unwrap()["dialog"].as_u64().unwrap();
        app.ui.dialog_mut(id).unwrap().fields.insert("overrides".into(), json!("bogus"));
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1280.0, 800.0)).build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            app
        });
        h.run_steps(4);
        assert_eq!(h.state().ui.dialogs.iter().find(|d| d.id == id).unwrap().fields["overrides"], json!("bogus"));
        assert!(crate::dialogs::confirm(h.state_mut(), id).is_err());
        assert_eq!(h.state().session.prefs().shortcut_profiles, before);
    }

    #[test]
    fn shortcut_dialog_moves_shortcuts_and_shell_dispatch_follows() {
        let (mut app, _) = app_with_store();
        let ctx = egui::Context::default();
        let id = crate::menus::invoke(&mut app, &ctx, "edit.keyboardShortcuts", json!({})).unwrap()["dialog"].as_u64().unwrap();
        // ⌘O (File › Open, a shell command) given to Layer › New › Layer.
        app.ui.dialog_mut(id).unwrap().fields.insert("overrides".into(), json!({"layer.new.layer": "Cmd+O"}));
        crate::dialogs::confirm(&mut app, id).unwrap();
        assert_eq!(effective_shortcut(&app, "layer.new.layer", None).as_deref(), Some("Cmd+O"));
        assert_eq!(effective_shortcut(&app, "file.open", Some("Cmd+O")), None, "taken from File › Open");
        // Menu items show the effective shortcut.
        let items = crate::menus::menu_items(&app);
        assert!(items.iter().any(|i| i.id == "layer.new.layer" && i.shortcut.as_deref() == Some("Cmd+O")));
        assert!(items.iter().any(|i| i.id == "file.open" && i.shortcut.is_none()));
        assert_eq!(shortcut_text(egui::Key::K, egui::Modifiers { command: true, shift: true, ..Default::default() }).as_deref(), Some("Cmd+Shift+K"));
    }

    #[test]
    fn hidden_menu_items_disappear() {
        let (mut app, _) = app_with_store();
        app.run("edit.menus", json!({"hide": ["edit.fade"]})).unwrap();
        assert!(!crate::menus::menu_items(&app).iter().any(|i| i.id == "edit.fade"));
    }

    #[test]
    fn autosave_runs_for_dirty_documents() {
        type Saved = Arc<Mutex<Vec<(u64, u64)>>>;
        let saved: Saved = Arc::default();
        let s2 = saved.clone();
        let services = crate::Services {
            autosave: Some(Box::new(move |doc: &Arc<photocraft_doc::Document>, rev: u64, _path: Option<&str>| {
                s2.lock().unwrap().push((doc.id.0, rev));
                Ok(())
            })),
            ..Default::default()
        };
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
        let ctx = egui::Context::default();
        app.run("file.new", json!({"width": 8, "height": 8})).unwrap();
        tick(&mut app, &ctx);
        autosave_now(&mut app);
        tick(&mut app, &ctx);
        assert!(saved.lock().unwrap().is_empty(), "clean documents are not autosaved");
        app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
        autosave_now(&mut app);
        tick(&mut app, &ctx);
        assert_eq!(saved.lock().unwrap().len(), 1);
        autosave_now(&mut app);
        tick(&mut app, &ctx);
        assert_eq!(saved.lock().unwrap().len(), 1, "unchanged since the last autosave");

        // Loaded copies can have the same persisted identity and dirty revision.
        let original = app.session.active().unwrap().doc.clone();
        let original_id = original.id;
        app.session.add_document(original.as_ref().clone(), None);
        app.run("edit.fill", json!({"color": "#0000ff"})).unwrap();
        autosave_now(&mut app);
        tick(&mut app, &ctx);
        {
            let saved = saved.lock().unwrap();
            assert_eq!(saved.len(), 2, "both dirty copies need independent autosaves");
            assert_eq!(saved[0].0, original_id.0);
            assert_ne!(saved[0].0, saved[1].0, "recovery ownership must be distinct");
            assert_eq!(saved[0].1, saved[1].1, "the revisions must match to reproduce suppression");
        }
        app.run("prefs.set", json!({"path": "fileHandling.autosave", "value": false})).unwrap();
        app.run("edit.fill", json!({"color": "#00ff00"})).unwrap();
        autosave_now(&mut app);
        tick(&mut app, &ctx);
        assert_eq!(saved.lock().unwrap().len(), 2, "autosave off");
    }

    #[test]
    fn recovered_documents_adopt_their_entries_until_saved_or_closed() {
        type Log = Arc<Mutex<Vec<String>>>;
        let log: Log = Arc::default();
        let (l1, l2, l3) = (log.clone(), log.clone(), log.clone());
        use photocraft_doc::{Color, ColorMode, Document, SampleType, Size};
        let doc = || Document::with_background("R", Size::new(4, 4), ColorMode::Rgb, SampleType::U8, Color::WHITE);
        let services = crate::Services {
            autosave: Some(Box::new(move |d: &Arc<Document>, _: u64, _: Option<&str>| {
                l1.lock().unwrap().push(format!("save {}", d.id.0));
                Ok(())
            })),
            discard_autosave: Some(Box::new(move |id: u64| l2.lock().unwrap().push(format!("discard {id}")))),
            recover: Some(Box::new(move || ["a", "b"].map(|key| crate::Recovered { key: key.into(), path: None, doc: doc() }).into())),
            adopt_autosave: Some(Box::new(move |id: u64, key: &str| l3.lock().unwrap().push(format!("adopt {id} {key}")))),
            ..Default::default()
        };
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
        let ctx = egui::Context::default();
        let ids: Vec<u64> = app.session.documents().iter().map(|d| d.doc.id.0).collect();
        assert!(app.session.documents().iter().all(|d| d.is_dirty()), "recovered documents are unsaved");
        let take = || std::mem::take(&mut *log.lock().unwrap());
        assert_eq!(take(), [format!("adopt {} a", ids[0]), format!("adopt {} b", ids[1])]);
        // Their entries are current: nothing to autosave until they change.
        tick(&mut app, &ctx);
        autosave_now(&mut app);
        tick(&mut app, &ctx);
        assert!(take().is_empty());
        // Closing one drops its entry; editing the other autosaves over its own.
        app.run("file.close", json!({"document": 0})).unwrap();
        app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
        autosave_now(&mut app);
        tick(&mut app, &ctx);
        assert_eq!(take(), [format!("discard {}", ids[0]), format!("save {}", ids[1])]);
    }

    #[test]
    fn edit_dialogs_open() {
        let (mut app, _) = app_with_store();
        let ctx = egui::Context::default();
        app.run("file.new", json!({"width": 32, "height": 32})).unwrap();
        app.sync_views();
        let d = crate::menus::invoke(&mut app, &ctx, "edit.colorSettings", json!({})).unwrap()["dialog"].as_u64().unwrap();
        assert_eq!(app.ui.dialogs.iter().find(|x| x.id == d).unwrap().fields["workingRgb"], "srgb");
        let note = app.ui.dialogs.iter().find(|x| x.id == d).unwrap().fields["__note"].as_str().unwrap().to_string();
        assert!(note.starts_with("Monitor profile in use: sRGB") && note.contains("fallback"), "{note}");
        assert!(crate::menus::invoke(&mut app, &ctx, "edit.fade", json!({})).is_err());
        app.run("select.rect", json!({"x": 4, "y": 4, "width": 8, "height": 8})).unwrap();
        let d = crate::menus::invoke(&mut app, &ctx, "edit.contentAwareFill", json!({})).unwrap()["dialog"].as_u64().unwrap();
        assert_eq!(app.ui.dialogs.iter().find(|x| x.id == d).unwrap().fields["__preview"], true);
        crate::dialogs::confirm(&mut app, d).unwrap();
        let d = crate::menus::invoke(&mut app, &ctx, "edit.contentAwareScale", json!({})).unwrap()["dialog"].as_u64().unwrap();
        assert_eq!(app.ui.dialogs.iter().find(|x| x.id == d).unwrap().fields["width"], 8);
        let d = crate::menus::invoke(&mut app, &ctx, "edit.presets.presetManager", json!({})).unwrap()["dialog"].as_u64().unwrap();
        assert!(crate::dialogs::confirm(&mut app, d).is_ok());
    }
}

#[cfg(test)]
mod bugfix_save_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[test]
    fn bugfix_save_o046_repaired_preferences_are_written_back_once() {
        let stored = Arc::new(Mutex::new(Some(r#"{"interface":{"theme":"gone"},"general":{"beepWhenDone":true}}"#.to_string())));
        let read = stored.clone();
        let write = stored.clone();
        let mut app = PhotocraftApp::new(
            photocraft_engine::Session::new(),
            crate::Services {
                load_prefs: Some(Box::new(move || read.lock().unwrap().clone())),
                save_prefs: Some(Box::new(move |text| {
                    *write.lock().unwrap() = Some(text.into());
                    Ok(())
                })),
                ..Default::default()
            },
        );
        load(&mut app);
        assert!(app.session.prefs().general.beep_when_done);
        assert!(persist(&mut app, 0.0).is_none());
        let text = stored.lock().unwrap().clone().unwrap();
        let mut session = photocraft_engine::Session::new();
        session.load_prefs_json(&text).unwrap();
        assert!(session.prefs().general.beep_when_done);
    }
}

#[cfg(test)]
#[path = "shared_library_shortcut_tests.rs"]
mod shared_library_shortcut_tests;

#[cfg(test)]
mod tablet_compatibility_tests {
    use super::*;
    #[test]
    fn brush_batch_tablet_compatibility_dropdown_clicks_auto_on_off() {
        use egui_kittest::{Harness, kittest::Queryable};
        let values = json!({"tabletCompatibilityMode":"auto"}).as_object().unwrap().clone();
        let mut h = Harness::builder().with_size(egui::vec2(900.0, 200.0)).build_ui_state(
            |ui, values: &mut Map<String, Value>| {
                section_fields(ui, "tools", values, &["tabletCompatibilityMode".into()], li_i18n::Lang::En);
            },
            values,
        );
        h.run_steps(2);
        h.get_by_value("Auto").click();
        h.run_steps(2);
        h.get_by_label("On").click();
        h.run_steps(2);
        assert_eq!(h.state()["tabletCompatibilityMode"], "on");
        h.get_by_value("On").click();
        h.run_steps(2);
        h.get_by_label("Off").click();
        h.run_steps(2);
        assert_eq!(h.state()["tabletCompatibilityMode"], "off");
    }
}

#[cfg(test)]
#[path = "keymap_tests.rs"]
mod keymap_tests;

#[cfg(test)]
#[path = "design_system_shots.rs"]
pub(crate) mod design_system_shots;

#[cfg(test)]
mod display_info_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn display_info_is_read_only_and_visible_after_graphics_interaction() {
        let info = crate::gpu_canvas::GpuInfo {
            output_depth: 10,
            output_note: "native Wayland; panel depth is controlled by the compositor".into(),
            ..Default::default()
        };
        let lines = json!(info.lines());
        let mut h = Harness::builder().with_size(egui::vec2(400.0, 600.0)).build_ui_state(
            move |ui, values: &mut Map<String, Value>| {
                if super::design_system_shots::fonts_ready(ui) {
                    gpu_status_rows(ui, Some(&lines), None, values);
                }
            },
            Map::new(),
        );
        h.run_steps(2);
        h.get_by_label_contains("Display output: 10-bit client surface");
        h.get_by_label("Advanced").click();
        h.run_steps(2);
        h.get_by_value("Auto").click();
        h.run_steps(2);
        h.get_by_label("Vulkan").click();
        h.run_steps(2);
        assert_eq!(h.state()["gpuBackend"], "vulkan");
        assert!(!h.state().contains_key("outputDepth"));
        h.get_by_label_contains("Display output: 10-bit client surface");
    }
}

/// Both the dialog and OK resolve defaults against the profile being edited.
fn profile_defaults(prefs: &prefs::Preferences, profiles: &li_keys::ShortcutProfiles, items: &mut [HostShortcutItem]) {
    let mut working = prefs.clone();
    working.shortcut_profiles = profiles.clone();
    for (id, _, _, def) in items {
        *def = if id.starts_with("library::") {
            crate::keymaps::library::default(profiles.library.parent(), id, true, def.as_deref()).map(str::to_string)
        } else {
            crate::keymaps::default(&working, id, def.as_deref()).map(str::to_string)
        };
    }
}

#[cfg(test)]
mod j20_tests {
    use super::*;
    use egui_kittest::{
        Harness,
        kittest::{By, Queryable},
    };

    #[test]
    fn j20_capture_widget_transfers_shifted_symbol_conflict() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let id = open_shortcuts(&mut app, 0);
        let fields = app.ui.dialog_mut(id).unwrap().fields.clone();
        app.ui.dialogs.clear();
        let mut h = Harness::builder().with_size(vec2(900., 760.)).with_max_steps(64).build_ui_state(
            |ui, (app, fields): &mut (PhotocraftApp, Map<String, Value>)| {
                let ready = egui::Id::new("j20-fonts-ready");
                if !ui.ctx().data(|d| d.get_temp::<bool>(ready)).unwrap_or(false) {
                    crate::theme::install_fonts(ui.ctx());
                    crate::theme::apply(ui.ctx(), crate::theme::ThemeKind::Pro);
                    ui.ctx().data_mut(|d| d.insert_temp(ready, true));
                    ui.ctx().request_repaint();
                    return;
                }
                shortcuts_body(app, ui, fields);
            },
            (app, fields),
        );
        let items: Vec<HostShortcutItem> = vec![
            ("library::tool.brush".into(), "Brush".into(), vec!["Library".into(), "Tools".into()], Some("Shift+[".into())),
            ("library::view.maskOverlay".into(), "Mask overlay".into(), vec!["Library".into(), "View".into()], Some("O".into())),
        ];
        h.ctx.data_mut(|d| d.insert_temp(egui::Id::new("host-shortcut-catalog"), items));
        h.run_steps(4);
        h.get(By::new().role(egui::accesskit::Role::ComboBox).value("Editor")).click();
        h.run();
        h.get_by_label("Library + Develop").click();
        h.run();
        let pos = h.ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("shortcut-capture", "library::view.maskOverlay")))).unwrap().center();
        h.event(egui::Event::PointerMoved(pos));
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            h.run_steps(1);
        }
        for pressed in [true, false] {
            h.event(egui::Event::Key { key: egui::Key::OpenCurlyBracket, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::SHIFT });
            h.run_steps(2);
        }
        assert_eq!(h.state().1["overrides"]["library::tool.brush"], "", "the shared matcher detects Shift+[ / Shift+{{ overlap");
        assert_eq!(h.state().1["overrides"]["library::view.maskOverlay"], "Shift+OpenCurlyBracket");
        assert!(h.state().1["message"].as_str().unwrap().contains("already in use"));
        let (app, fields) = h.state_mut();
        confirm(app, fields).unwrap();
        let profiles = &app.session.prefs().shortcut_profiles;
        assert_eq!(profiles.library.custom[&profiles.library.active].overrides["library::tool.brush"], "");
        assert_eq!(profiles.editor.active, "Photoshop");
    }
}
