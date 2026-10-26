//! The LightCraft engine façade.
//!
//! Every user-visible action is a command with a stable id (`photo.rate`, `develop.set`,
//! `album.create`, `mask.add`…) and JSON parameters. The egui UI, the CLI, the control channel and
//! the MCP server all go through [`Session::execute`].
//!
//! State: a [`Catalog`] (mutated only by ops, so every change is undoable and journaled), the
//! library view (filter/sort/source), the selection, the develop clipboard, presets, and caches
//! of decoded source proxies. Rendering is done by [`RenderJob`]s that are `Send` so frontends can
//! run them off the UI thread.
#![forbid(unsafe_code)]

pub mod auto_wb;
pub mod availability;
mod camera_preview;
pub mod camera_profiles;
pub mod cmd;
pub mod crs;
pub mod crs_masks;
pub mod demo;
pub mod devices;
mod embedded_lens;
pub mod enhance;
pub mod ephemeral;
pub mod export;
pub mod files;
pub mod fonts;
pub mod guard;
pub mod import;
mod import_move;
pub mod lens_db;
pub mod library;
pub mod library_safety;
pub mod media;
pub mod memory;
pub mod merge;
pub mod originals;
pub mod preset_import;
pub mod preset_luminar;
pub mod presets;
pub mod proof_sheet;
pub mod quickseg;
pub mod rename;
pub mod segment;
pub mod sidecar;
pub mod smart;
pub mod smart_sort;
mod view;
pub mod walk;

use std::sync::Arc;

pub use cmd::{CommandSpec, command_specs, find_command};
pub use fonts::{CRAFT_FONTS, CraftFont};
use lightcraft_catalog::{Catalog, Filter, Op, PhotoId, Sort};
use lightcraft_develop::DevelopSettings;
pub use media::{RenderJob, SourceLevel};
use serde_json::Value;
pub use view::{Browse, FilterChip, LibrarySource, Selection, filter_chips, update_soft_proof};
pub use {lightcraft_catalog as catalog, lightcraft_develop as develop, lightcraft_gpu as gpu, lightcraft_pipeline as pipeline};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("command `{0}` is not available right now: {1}")]
    Disabled(String, String),
    #[error("invalid parameters for `{cmd}`: {msg}")]
    BadParams { cmd: String, msg: String },
    #[error("{0}")]
    Catalog(#[from] lightcraft_catalog::CatalogError),
    /// The command's change is applied (in memory, undoable) but its journal records could not
    /// be written. They stay queued and are written by the next successful save.
    #[error("saved in memory but not written to disk: {0}; LightCraft will retry")]
    NotSaved(String),
    /// Another process (the app, `lightcraft-cli`, another computer) has the library open.
    #[error("{0}")]
    LibraryInUse(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, EngineError>;

/// One undo step: the inverse op and a label.
#[derive(Clone, Debug)]
pub struct UndoEntry {
    /// Background transaction identity. Only adjacent steps with the same identity coalesce.
    pub group: Option<u64>,
    pub label: String,
    pub op: Op,
    /// A folder to rename on disk (`from` → `to`) before `op` is applied: Rename / Move Folder
    /// (whose `op` relinks the photos inside). Never overwrites; refused when `to` exists.
    pub folder: Option<FolderMove>,
    /// People identity changes share the catalog step without entering its op log.
    pub people: Option<smart_sort::people::PeopleData>,
}

/// A folder renamed or moved on disk as part of an undo step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderMove {
    pub from: String,
    pub to: String,
}

impl FolderMove {
    fn reversed(&self) -> Self {
        Self { from: self.to.clone(), to: self.from.clone() }
    }
}

/// An in-progress slider drag / brush stroke: one undo step when it ends.
#[derive(Clone, Debug)]
pub struct Interaction {
    pub label: String,
    pub photo: PhotoId,
    pub original: Arc<DevelopSettings>,
}

/// Source of [`Session::visible_shared`] generations (process-wide, so two sessions never share one).
static VISIBLE_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn clock_minute(now: &str) -> &str {
    now.get(..16).unwrap_or(now)
}

fn albums_tested(filter: &Filter) -> Vec<lightcraft_catalog::AlbumId> {
    fn visit(rules: &lightcraft_catalog::RuleSet, out: &mut Vec<lightcraft_catalog::AlbumId>) {
        for rule in &rules.rules {
            match rule {
                lightcraft_catalog::Rule::Group { group } => visit(group, out),
                lightcraft_catalog::Rule::Field { field, value, .. } if field == "album" => {
                    if let Some(id) = value.as_u64() {
                        out.push(lightcraft_catalog::AlbumId(id));
                    }
                }
                lightcraft_catalog::Rule::Field { .. } => {}
            }
        }
    }
    let mut out: Vec<_> = filter.album.into_iter().collect();
    if let Some(rules) = &filter.rule_set {
        visit(rules, &mut out);
    }
    out
}

fn filter_depends_on_clock(filter: &Filter, catalog: &Catalog) -> bool {
    if filter.depends_on_now() {
        return true;
    }
    let mut pending = albums_tested(filter);
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(smart) = catalog.album(id).and_then(|album| album.smart.as_deref()) else { continue };
        if smart.depends_on_now() {
            return true;
        }
        pending.extend(albums_tested(smart));
    }
    false
}

pub struct Session {
    /// Auto Sync: edits to the active photo also change the other selected photos (the settings
    /// that changed, nothing else).
    pub auto_sync: bool,
    /// Auto white balance that a paste, sync, Auto Sync or preset brings to a photo whose source
    /// isn't decoded is queued for a background task (the app's, see [`auto_wb`]) instead of being
    /// measured inside the command. Off: measured at once.
    pub defer_auto_wb: bool,
    /// Photos waiting for that measurement, with the library generation they belong to.
    pending_auto_wb: Vec<(u64, PhotoId)>,
    pub catalog: Catalog,
    pub source: LibrarySource,
    pub filter: Filter,
    pub sort: Sort,
    pub selection: Selection,
    /// The grid order for the current source/filter/sort (cached by catalog revision).
    /// Shared, so frontends can hold it across frames without copying ([`Session::visible_shared`]).
    visible: Arc<[PhotoId]>,
    visible_key: Option<(u64, String)>,
    visible_clock_minute: Option<String>,
    /// Identifies the current `visible` list: a new value (unique in the process) every time it
    /// is recomputed, so frontends can key their per-view caches on it instead of hashing the ids.
    visible_gen: u64,
    /// Photos in the current source with no filter on (cached like `visible`).
    total: Option<((u64, String), usize, Option<String>)>,
    pub undo: Vec<UndoEntry>,
    pub redo: Vec<UndoEntry>,
    pub interaction: Option<Interaction>,
    /// Set by a command whose change must not rewrite the photo's XMP sidecar even with auto-write on
    /// (a catalog-only edit of data the sidecar writer does not emit); consumed when the command ends.
    pub(crate) skip_auto_write: bool,
    /// Copied develop settings (partial JSON) for Paste.
    pub clipboard: Option<Value>,
    /// The folder on disk the [`LibrarySource::Folder`] view browses.
    pub browse: Option<Browse>,
    /// Copied metadata (`photo.copyMetadata`): photo.setMeta params.
    pub meta_clipboard: Option<Value>,
    /// The photo that was active before the current one (Paste Settings from Previous).
    pub previous_active: Option<PhotoId>,
    /// Groups last used for Copy (Lightroom remembers them).
    pub copy_groups: Vec<lightcraft_develop::SettingsGroup>,
    pub presets: Vec<lightcraft_develop::Preset>,
    /// Favourite profile ids (persisted with the library, like preset favourites).
    pub profile_favorites: Vec<String>,
    /// Recently applied profile ids, newest first (at most [`presets::RECENT_PROFILES`]).
    pub profile_recent: Vec<String>,
    /// Executed commands (actions / debugging / replay).
    pub journal: Vec<(String, Value)>,
    /// Ops applied since the last `drain_log` (for persistence).
    pending_log: Vec<Op>,
    pub media: media::MediaCache,
    /// Current time provider (ISO 8601); injectable for tests and wasm.
    pub clock: Box<dyn Fn() -> String + Send>,
    depth: u32,
    undo_group: Option<u64>,
    /// Selected mask (Masking panel), by mask id.
    pub active_mask: Option<u32>,
    /// AI masks (SAM 3): the model and the last photo prepared for it.
    pub segmenter: segment::Segmenter,
    /// local-image: where the quick segmentation models live (`li-seg`: Subject, Background, Sky);
    /// set by the host. `None`: those masks use the classical estimate.
    pub quick_seg_dir: Option<std::path::PathBuf>,
    /// Selected spot (Remove panel), by index into the active photo's spots.
    pub active_spot: Option<usize>,
    /// The persistent library this session writes to (`None` = in-memory only).
    pub library: Option<library::Library>,
    /// Invalidates background work when a library is opened or closed.
    pub library_generation: u64,
    /// Identity for a catalog that exists only in this Session.
    memory_library_identity: String,
    /// XMP sidecar preferences (persisted with the library).
    pub xmp: sidecar::XmpPrefs,
    /// Parameters of the last export (`app.export` params, minus targets), persisted in prefs.json.
    pub last_export: Option<serde_json::Value>,
    /// The user's export presets (built-ins: [`export::builtin_presets`]), persisted in prefs.json.
    pub export_presets: Vec<export::ExportPreset>,
    /// Metadata presets (`metadata.*`), persisted in prefs.json.
    pub metadata_presets: Vec<cmd::metadata::MetadataPreset>,
    /// Saved filter-bar settings (`filter.*`), persisted in prefs.json.
    pub filter_presets: Vec<cmd::filters::FilterPreset>,
    /// The user's point-curve presets (`curve.*`; built-ins: [`cmd::curves::builtin_presets`]),
    /// persisted in prefs.json.
    pub curve_presets: Vec<cmd::curves::CurvePreset>,
    /// Saved colour-label name sets.
    pub label_sets: Vec<cmd::manage::LabelSet>,
    /// Smart Sort models and library-local embedding cache.
    pub smart: smart_sort::SmartSort,
    /// Look-alike signatures by content key (Find Similar).
    pub signatures: std::collections::HashMap<String, [f32; 64]>,
    /// Imported `.cube` LUT profiles.
    pub lut_profiles: Vec<cmd::lut_profiles::LutProfile>,
    /// The target album B adds to (`None` = the Quick Collection).
    pub target_album: Option<lightcraft_catalog::AlbumId>,
    /// Auto import: files seen in the watched folder and their size then (a file is imported once
    /// its size held between two scans).
    pub auto_import_seen: std::collections::HashMap<String, u64>,
    /// Keyword sets (⌥1–⌥9 apply the current set's keywords), the one in use (`None` = Recent
    /// Keywords) and the recently added keywords, newest first.
    pub keyword_sets: Vec<cmd::keywords::KeywordSet>,
    pub keyword_set: Option<String>,
    pub recent_keywords: Vec<String>,
    /// Before/After: the "before" settings chosen per photo (this session; default: the photo's
    /// import state). See `cmd/before.rs`.
    pub before: std::collections::HashMap<PhotoId, Arc<DevelopSettings>>,
    /// File probes from the last import review (`library.importPreview`), reused by the import.
    pub import_probes: std::collections::HashMap<String, media::ProbeInfo>,
    /// The last (or running) Build Previews.
    pub preview_build: Option<std::sync::Arc<cmd::previews::PreviewBuild>>,
    /// Automatic Upright analysed in the background after Paste, Sync and presets.
    pub upright: cmd::upright::UprightQueue,
    /// Develop defaults applied on import (persisted in prefs.json).
    pub import_defaults: import::ImportDefaults,
    /// Disk budget of the library's thumbnail cache in MB (0 = default; persisted in prefs.json).
    pub cache_mb: u32,
    /// Where smart previews are kept when not in the library folder (persisted in prefs.json).
    pub smart_previews_dir: Option<std::path::PathBuf>,
    /// Untouched Local records of folders not browsed for this many days are forgotten when the
    /// library opens (0 = never; persisted in prefs.json). See `cmd/browse.rs`.
    pub forget_local_days: u32,
    /// Photos in Recently Deleted for this many days are removed from the library when it opens
    /// (0 = never, the default; persisted in prefs.json). See [`Session::purge_deleted`].
    pub purge_deleted_days: u32,
    pub backup_preferences: library_safety::BackupPreferences,
    /// local-image: the Camera Raw Filter's temporary photo (see [`ephemeral`]).
    pub(crate) ephemeral: Option<ephemeral::Ephemeral>,
    /// local-image: AI Remove — the host's AI services and the running jobs (see
    /// [`enhance`]).
    pub enhance: enhance::Enhance,
    // Session tests also reach GPU compute indirectly through export, thumbnails
    // and render jobs. Keep them inside the same lock as fault-injection tests;
    // release the context after every session/cache field has dropped.
    #[cfg(test)]
    _gpu_test: tests_gpu::Guard,
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

impl Session {
    pub fn new() -> Session {
        #[cfg(test)]
        let gpu_test = tests_gpu::gpu_state();
        Session {
            auto_sync: false,
            defer_auto_wb: false,
            pending_auto_wb: Vec::new(),
            #[cfg(test)]
            _gpu_test: gpu_test,
            catalog: Catalog::new(),
            source: LibrarySource::All,
            filter: Filter::default(),
            sort: Sort::default(),
            selection: Selection::default(),
            visible: Vec::new().into(),
            visible_key: None,
            visible_clock_minute: None,
            visible_gen: 0,
            total: None,
            undo: Vec::new(),
            redo: Vec::new(),
            interaction: None,
            skip_auto_write: false,
            clipboard: None,
            meta_clipboard: None,
            browse: None,
            previous_active: None,
            copy_groups: lightcraft_develop::SettingsGroup::default_copy(),
            presets: presets::builtin(),
            profile_favorites: Vec::new(),
            profile_recent: Vec::new(),
            journal: Vec::new(),
            pending_log: Vec::new(),
            media: media::MediaCache::default(),
            clock: Box::new(|| "2026-09-30T12:00:00".to_string()),
            depth: 0,
            undo_group: None,
            active_mask: None,
            segmenter: segment::Segmenter::default(),
            quick_seg_dir: None,
            active_spot: None,
            library: None,
            library_generation: 0,
            memory_library_identity: format!("memory:{}", uuid::Uuid::new_v4()),
            xmp: sidecar::XmpPrefs::default(),
            last_export: None,
            export_presets: Vec::new(),
            metadata_presets: Vec::new(),
            filter_presets: Vec::new(),
            curve_presets: Vec::new(),
            label_sets: Vec::new(),
            smart: Default::default(),
            signatures: Default::default(),
            lut_profiles: Vec::new(),
            target_album: None,
            auto_import_seen: Default::default(),
            keyword_sets: Vec::new(),
            keyword_set: None,
            recent_keywords: Vec::new(),
            before: Default::default(),
            import_probes: Default::default(),
            preview_build: None,
            upright: Default::default(),
            import_defaults: import::ImportDefaults::default(),
            cache_mb: 0,
            smart_previews_dir: None,
            forget_local_days: lightcraft_catalog::DEFAULT_FORGET_DAYS,
            purge_deleted_days: 0,
            backup_preferences: Default::default(),
            ephemeral: None,
            enhance: enhance::Enhance::default(),
        }
    }

    /// A session with the procedurally generated demo library loaded.
    pub fn with_demo() -> Session {
        let mut s = Session::new();
        demo::load(&mut s);
        s
    }

    /// Run a command by id. THE entry point for every frontend.
    pub fn execute(&mut self, id: &str, params: &Value) -> Result<Value> {
        let spec = find_command(id).ok_or_else(|| EngineError::UnknownCommand(id.to_string()))?;
        (spec.enabled)(self).map_err(|why| EngineError::Disabled(id.to_string(), why))?;
        let empty = Value::Object(Default::default());
        let params = if params.is_null() { &empty } else { params };
        self.run_command(id, spec.journal.then_some(params), |s| (spec.run)(s, params)).map_err(|e| match e {
            // a shared parameter check ([`Session::targets`]) names the command it ran for
            EngineError::BadParams { cmd, msg } if cmd.is_empty() => EngineError::BadParams { cmd: id.to_string(), msg },
            e => e,
        })
    }

    /// Run `f` as command `id` with what [`Session::execute`] does around every command: the
    /// panic guard, auto versions, XMP sidecar auto-write and the durable save (a failed save is
    /// `NotSaved`). Not journaled. The app's import task commits its batches this way.
    pub fn execute_fn(&mut self, id: &str, f: impl FnOnce(&mut Session) -> Result<Value>) -> Result<Value> {
        self.run_command(id, None, f)
    }

    /// Commit a background batch without absorbing edits made between its arrivals.
    pub fn execute_grouped(&mut self, id: &str, group: u64, label: &str, f: impl FnOnce(&mut Session) -> Result<Value>) -> Result<Value> {
        let previous = self.undo_group.replace(group);
        let result = self.execute_fn(id, f);
        self.undo_group = previous;
        let n = self.undo.iter().rev().take_while(|e| e.group == Some(group)).count();
        self.merge_undo(n, label);
        result
    }

    fn run_command(&mut self, id: &str, journal: Option<&Value>, f: impl FnOnce(&mut Session) -> Result<Value>) -> Result<Value> {
        let log_start = self.pending_log.len();
        let was_active = self.active();
        self.depth += 1;
        // last-resort guard: a panic in a command is that command's error, not a crash
        let r = guard::catch(&format!("`{id}`"), || f(self)).unwrap_or_else(|e| Err(EngineError::Other(e)));
        self.depth -= 1;
        if self.depth == 0 && was_active.is_some() && self.active() != was_active {
            self.previous_active = was_active;
            if let Some(left) = was_active {
                self.auto_version(left);
            }
        }
        if r.is_ok()
            && let Some(params) = journal
            && self.depth == 0
        {
            self.journal.push((id.to_string(), params.clone()));
            if self.journal.len() > 10_000 {
                self.journal.drain(..1000);
            }
        }
        if self.depth == 0 {
            self.start_upright();
            let skip = std::mem::take(&mut self.skip_auto_write);
            if r.is_ok() && !skip && self.xmp.auto_write && self.interaction.is_none() && self.pending_log.len() > log_start {
                self.auto_write_sidecars(&self.pending_log[log_start..]);
            }
        }
        if self.depth == 0 && self.library.is_some() {
            // Make the command durable before reporting success. When the command's own records
            // can't be appended, it fails with `NotSaved`: the change stays applied in memory and
            // queued, and the next save (any later command, or the frame loop) retries it. Other
            // persistence trouble (a failed compaction, an older queued change still unwritten by
            // a command that changed nothing) doesn't fail the command; `library.info` reports it.
            let produced = self.pending_log.len() > log_start;
            if let Err(e @ EngineError::NotSaved(_)) = self.persist()
                && produced
                && r.is_ok()
            {
                return Err(e);
            }
        }
        r
    }

    pub fn commands(&self) -> Vec<li_commands::CommandInfo> {
        command_specs().iter().map(|c| c.info(self)).collect()
    }

    // ops, undo

    /// Apply an op as one undoable step.
    pub fn commit(&mut self, label: &str, op: Op) -> Result<()> {
        let filter = self.ephemeral_photo();
        let scope = filter.map(|photo| ephemeral::op_scope(&op, photo)).unwrap_or((false, true));
        if scope == (true, true) {
            return Err(EngineError::Other("Camera Raw and Library edits must be separate operations".into()));
        }
        let fwd = op.clone();
        let inv = self.catalog.apply(op)?;
        self.pending_log.push(fwd);
        self.undo.push(UndoEntry { group: self.undo_group, label: label.to_string(), op: inv, folder: None, people: None });
        let same_scope = |e: &UndoEntry| filter.is_none_or(|photo| ephemeral::only_photo(&e.op, photo) == scope.0);
        if self.undo.len() > 1000
            && self.undo.iter().filter(|e| same_scope(e)).count() > 1000
            && let Some(index) = self.undo.iter().position(same_scope)
        {
            self.undo.remove(index);
        }
        self.redo.retain(|e| !same_scope(e));
        Ok(())
    }

    /// [`Session::commit`] for an op that goes with a folder already renamed on disk; the undo
    /// step renames it back (`folder` is the undo direction: current place → old place).
    pub(crate) fn commit_with_folder(&mut self, label: &str, op: Op, folder: FolderMove) -> Result<()> {
        self.commit(label, op)?;
        if let Some(e) = self.undo.last_mut() {
            e.folder = Some(folder);
        }
        Ok(())
    }

    /// Leaving photo `id` after editing it: keep its settings as an automatic version (when they
    /// differ from its latest version; at most [`AUTO_VERSIONS`] auto versions, oldest dropped).
    /// Saved with the library but not an undo step.
    pub fn auto_version(&mut self, id: PhotoId) {
        if self.is_ephemeral(id) {
            return;
        }
        let Some(p) = self.catalog.photo(id) else { return };
        if !p.is_edited() || p.versions.last().is_some_and(|v| *v.settings == *p.develop) || self.interaction.is_some() {
            return;
        }
        let mut versions = p.versions.clone();
        let created = (self.clock)();
        let name = lightcraft_catalog::dates::display_time(&created);
        versions.push(lightcraft_catalog::Version { name, created, settings: p.develop.clone(), auto: true });
        let autos = versions.iter().filter(|v| v.auto).count();
        if autos > AUTO_VERSIONS
            && let Some(i) = versions.iter().position(|v| v.auto)
        {
            versions.remove(i);
        }
        let op = Op::SetVersions { id, versions };
        if self.catalog.apply(op.clone()).is_ok() {
            self.pending_log.push(op);
        }
    }

    /// Fold the last `n` undo steps into one (commands that commit step by step because each op
    /// depends on the state the previous one left).
    pub fn merge_undo(&mut self, n: usize, label: &str) {
        if n < 2 || n > self.undo.len() {
            return;
        }
        if let Some(photo) = self.ephemeral_photo() {
            let tail = &self.undo[self.undo.len() - n..];
            let scope = ephemeral::only_photo(&tail[0].op, photo);
            if tail.iter().any(|e| ephemeral::only_photo(&e.op, photo) != scope) {
                return;
            }
        }
        // a step that moves a folder on disk stays on its own
        if self.undo[self.undo.len() - n..].iter().any(|e| e.folder.is_some() || e.people.is_some()) {
            return;
        }
        let group = self.undo.last().and_then(|e| e.group);
        let tail = self.undo.split_off(self.undo.len() - n);
        let ops = tail.into_iter().rev().map(|e| e.op).collect();
        self.undo.push(UndoEntry { group, label: label.to_string(), op: Op::Batch { ops }, folder: None, people: None });
    }

    /// Remove the photos that have been in Recently Deleted for `days` days or more (default
    /// [`Session::purge_deleted_days`]; 0 = never) from the library, as Remove from Library does:
    /// one undo step, and the files on disk are never touched. A deletion whose time isn't known
    /// (made before it was recorded) is first given the current time — logged, not an undo step —
    /// so it goes `days` after the first purge that sees it, never at once. Returns the photos
    /// removed.
    pub fn purge_deleted(&mut self, days: Option<u32>) -> Result<Vec<PhotoId>> {
        let days = days.unwrap_or(self.purge_deleted_days);
        if days == 0 {
            return Ok(Vec::new());
        }
        let now = (self.clock)();
        let undated = self.catalog.undated_deleted();
        for chunk in undated.chunks(2000) {
            let ops = chunk.iter().map(|id| Op::SetDeleted { id: *id, deleted: true, deleted_at: Some(now.clone()) }).collect();
            let op = Op::Batch { ops };
            self.catalog.apply(op.clone())?;
            self.pending_log.push(op);
        }
        let ids = self.catalog.deleted_before(&now, days);
        if ids.is_empty() {
            return Ok(ids);
        }
        let op = self.catalog.delete_photos_permanently_ops(&ids);
        self.commit("Empty Recently Deleted", op)?;
        let gone: std::collections::HashSet<PhotoId> = ids.iter().copied().collect();
        self.selection.ids.retain(|id| !gone.contains(id));
        self.selection.active = self.selection.active.filter(|id| !gone.contains(id));
        log::info!("removed {} photo(s) in Recently Deleted for {days} days or more from the library", ids.len());
        Ok(ids)
    }

    /// Apply without recording undo (interactive previews).
    fn apply_silent(&mut self, op: Op) -> Result<()> {
        self.catalog.apply(op)?;
        Ok(())
    }

    fn history_index(&self, entries: &[UndoEntry]) -> Option<usize> {
        match self.ephemeral_photo() {
            Some(photo) => entries.iter().rposition(|e| ephemeral::only_photo(&e.op, photo)),
            None => entries.len().checked_sub(1),
        }
    }

    /// The Undo/Redo available in the visible editing scope. A Camera Raw session
    /// cannot consume Library steps, including commits arriving in the background.
    pub fn undo_entry(&self) -> Option<&UndoEntry> {
        self.history_index(&self.undo).and_then(|i| self.undo.get(i))
    }

    pub fn redo_entry(&self) -> Option<&UndoEntry> {
        self.history_index(&self.redo).and_then(|i| self.redo.get(i))
    }

    /// Selection is UI state: undo, sync and history can remove its layer.
    pub fn resolve_active_mask(&mut self) {
        if let Some(mid) = self.active_mask
            && !self.active().and_then(|id| self.develop_of(id)).is_some_and(|d| d.masks.iter().any(|m| m.id == mid))
        {
            self.active_mask = None;
        }
    }

    pub fn layer_label(&self, label: &str, mid: Option<u32>) -> String {
        mid.and_then(|mid| {
            self.active()
                .and_then(|id| self.develop_of(id))
                .and_then(|d| d.masks.iter().find(|m| m.id == mid).map(|m| format!("{label} – {}", m.name)))
        })
        .unwrap_or_else(|| label.to_string())
    }

    pub fn undo_step(&mut self) -> Result<String> {
        let index = self.history_index(&self.undo).ok_or_else(|| EngineError::Other("nothing to undo".into()))?;
        let e = self.undo.remove(index);
        let (redo, people) = match self.apply_people_history(&e.op, e.folder.as_ref(), e.people.as_ref()) {
            Ok(r) => r,
            Err(err) => {
                self.undo.insert(index, e);
                return Err(err);
            }
        };
        self.pending_log.push(e.op);
        self.redo.push(UndoEntry { group: e.group, label: e.label.clone(), op: redo, folder: e.folder.as_ref().map(FolderMove::reversed), people });
        self.resolve_active_mask();
        Ok(e.label)
    }

    pub fn redo_step(&mut self) -> Result<String> {
        let index = self.history_index(&self.redo).ok_or_else(|| EngineError::Other("nothing to redo".into()))?;
        let e = self.redo.remove(index);
        let (undo, people) = match self.apply_people_history(&e.op, e.folder.as_ref(), e.people.as_ref()) {
            Ok(r) => r,
            Err(err) => {
                self.redo.insert(index, e);
                return Err(err);
            }
        };
        self.pending_log.push(e.op);
        self.undo.push(UndoEntry { group: e.group, label: e.label.clone(), op: undo, folder: e.folder.as_ref().map(FolderMove::reversed), people });
        self.resolve_active_mask();
        Ok(e.label)
    }

    /// Apply an undo/redo op, first renaming the step's folder and moving the files its renames
    /// imply (all or nothing). Files that can't be moved back after a failure are reported and the
    /// library follows them (a logged change outside the undo history), so none goes missing; the
    /// folder is only moved back when no file was left behind at its new path.
    fn apply_with_files(&mut self, op: &Op, folder: Option<&FolderMove>) -> Result<Op> {
        let fs = rename::RealFs;
        if let Some(f) = folder {
            cmd::browse::rename_folder_on_disk(&f.from, &f.to).map_err(|e| EngineError::Other(format!("can't move the folder back: {e}")))?;
        }
        let undo_folder = || {
            if let Some(f) = folder {
                let _ = cmd::browse::rename_folder_on_disk(&f.to, &f.from);
            }
        };
        let moves = self.file_moves(op);
        if let Err(e) = rename::move_all(&fs, &moves) {
            if e.stuck.is_empty() {
                undo_folder();
            }
            return Err(EngineError::Other(format!("can't move the files back: {}", self.follow_stuck(op, e, false))));
        }
        match self.catalog.apply(op.clone()) {
            Ok(inv) => {
                if let Some(f) = folder {
                    cmd::browse::follow_folder(self, &f.from, &f.to);
                }
                Ok(inv)
            }
            Err(e) => {
                let back: Vec<(String, String)> = moves.iter().rev().map(|(a, b)| (b.clone(), a.clone())).collect();
                if let Err(be) = rename::move_all(&fs, &back) {
                    return Err(EngineError::Other(format!("{e}; the files could not all be moved back: {}", be.message)));
                }
                undo_folder();
                Err(e.into())
            }
        }
    }

    /// Ops applied since the last call, for the op-log store.
    pub fn drain_log(&mut self) -> Vec<Op> {
        self.strip_ephemeral_log();
        std::mem::take(&mut self.pending_log)
    }

    // develop edits

    /// The photo being edited (the active photo).
    pub fn active(&self) -> Option<PhotoId> {
        self.selection.active.filter(|id| self.catalog.photo(*id).is_some())
    }

    pub fn develop_of(&self, id: PhotoId) -> Option<Arc<DevelopSettings>> {
        self.catalog.photo(id).map(|p| p.develop.clone())
    }

    /// Change a photo's develop settings. During an interaction the change is previewed without an
    /// undo step; otherwise it is committed with a history entry.
    pub fn set_develop(&mut self, id: PhotoId, settings: DevelopSettings, label: &str) -> Result<()> {
        let now = (self.clock)();
        let settings = Arc::new(settings);
        if let Some(i) = &self.interaction
            && i.photo == id
        {
            return self.apply_silent(Op::SetDevelop { id, settings, label: label.into(), edited: Some(now) });
        }
        let mut ops = vec![self.develop_op(id, (*settings).clone(), label).ok_or(lightcraft_catalog::CatalogError::NoPhoto(id))?];
        ops.extend(self.auto_sync_ops(id, &settings, label));
        let op = if ops.len() == 1 { ops.remove(0) } else { Op::Batch { ops } };
        let r = self.commit(label, op);
        if self.depth == 0 {
            // outside a command (a drag ending): the Upright analyses Auto Sync queued join this step
            self.start_upright();
        }
        r
    }

    /// With Auto Sync on, the ops that carry an edit of the active photo `id` (to `new`) over to the
    /// other selected photos: only the settings that changed; never spot removal or red eye (they
    /// belong to one photo's pixels), nor history / snapshot restores. Masks / layers never auto
    /// sync either: `json_delta` takes arrays whole, so any layer edit would replace every other
    /// photo's own layer stack. Layers go to other photos only through an explicit Sync / Copy.
    fn auto_sync_ops(&mut self, id: PhotoId, new: &DevelopSettings, label: &str) -> Vec<Op> {
        if !self.auto_sync
            || self.active() != Some(id)
            || self.selection.ids.len() < 2
            || label.starts_with("History:")
            || label.starts_with("Restore ")
        {
            return Vec::new();
        }
        let Some(old) = self.develop_of(id) else { return Vec::new() };
        let Some(mut delta) = json_delta(&old.to_json_full(), &new.to_json_full()) else { return Vec::new() };
        if let Some(o) = delta.as_object_mut() {
            for k in ["masks", "spots", "red_eye", "version"] {
                o.remove(k);
            }
            lightcraft_develop::presets::strip_photo_bound(o);
            if o.get("enhance").is_some_and(|e| e.as_object().is_some_and(|e| e.is_empty())) {
                o.remove("enhance");
            }
            if o.is_empty() {
                return Vec::new();
            }
        }
        let targets: Vec<_> = self.selection.ids.iter().copied().filter(|x| *x != id).filter_map(|x| self.develop_of(x).map(|d| (x, d))).collect();
        targets
            .into_iter()
            .filter_map(|(x, d)| {
                let synced = self.transfer_settings(x, &d, &delta);
                (synced != *d).then(|| self.develop_op(x, synced, label)).flatten()
            })
            .collect()
    }

    /// The op that sets a photo's develop settings and appends a History entry (for batches).
    pub fn develop_op(&self, id: PhotoId, settings: DevelopSettings, label: &str) -> Option<Op> {
        self.catalog.photo(id)?;
        let settings = Arc::new(settings);
        let step = lightcraft_catalog::HistoryStep { label: label.into(), settings: settings.clone() };
        Some(Op::Batch {
            ops: vec![Op::SetDevelop { id, settings, label: label.into(), edited: Some((self.clock)()) }, Op::PushHistory { id, step }],
        })
    }

    pub fn begin_interaction(&mut self, label: &str) -> Result<()> {
        if self.interaction.is_some() {
            self.end_interaction()?;
        }
        let id = self.active().ok_or_else(|| EngineError::Other("no active photo".into()))?;
        let original = self.develop_of(id).unwrap_or_default();
        self.interaction = Some(Interaction { label: label.into(), photo: id, original });
        Ok(())
    }

    /// Commit the interaction as one undo step (no-op if nothing changed).
    pub fn end_interaction(&mut self) -> Result<()> {
        let Some(i) = self.interaction.take() else { return Ok(()) };
        let Some(cur) = self.develop_of(i.photo) else { return Ok(()) };
        if *cur == *i.original {
            return Ok(());
        }
        // Restore the original silently, then commit the final value as one step.
        self.apply_silent(Op::SetDevelop { id: i.photo, settings: i.original.clone(), label: i.label.clone(), edited: None })?;
        self.set_develop(i.photo, (*cur).clone(), &i.label)
    }

    pub fn cancel_interaction(&mut self) -> Result<()> {
        if let Some(i) = self.interaction.take() {
            self.apply_silent(Op::SetDevelop { id: i.photo, settings: i.original, label: i.label, edited: None })?;
        }
        Ok(())
    }

    // library view

    /// Photos shown in the grid/filmstrip for the current source, filter and sort.
    pub fn visible(&mut self) -> &[PhotoId] {
        let mut key = (self.catalog.revision, format!("{:?}|{:?}|{:?}|{:?}", self.source, self.filter, self.sort, self.browse));
        if self.source == LibrarySource::Missing && self.media.availability.is_background() {
            // the view fills in as the background checks find files gone
            key.1.push_str(&format!("|{}", self.media.availability.generation()));
        }
        let key_changed = self.visible_key.as_ref() != Some(&key);
        let current_now = (key_changed || self.visible_clock_minute.is_some()).then(|| (self.clock)());
        let current_minute = current_now.as_deref().map(clock_minute);
        let clock_changed = self.visible_clock_minute.as_deref().is_some_and(|cached| current_minute != Some(cached));
        if key_changed || clock_changed {
            // Evaluate and stamp relative rules with the same clock reading.
            let now = current_now.unwrap_or_else(|| (self.clock)());
            lightcraft_catalog::rules::set_now(Some(now.clone()));
            let mut f = self.source.to_filter(&self.filter, &self.catalog);
            smart_sort::people::resolve_filter_ids(&mut f, &self.smart.people.data);
            if self.source == LibrarySource::Folder {
                // no folder chosen: nothing (an empty path matches nothing)
                let b = self.browse.clone().unwrap_or_default();
                f.folder = Some(b.path);
                f.subfolders = b.subfolders;
            }
            let mut visible = self.catalog.query(&f, &self.sort);
            if matches!(self.source, LibrarySource::Album(_))
                && self.sort.key == lightcraft_catalog::SortKey::CaptureDate
                && let LibrarySource::Album(a) = self.source
                && let Some(al) = self.catalog.album(a)
                && !al.is_smart()
                && self.filter == Filter::default()
            {
                let mut order = std::collections::HashMap::with_capacity(al.photos.len());
                for (i, id) in al.photos.iter().enumerate() {
                    order.entry(*id).or_insert(i);
                }
                visible.sort_by_key(|id| order.get(id).copied().unwrap_or(usize::MAX));
                if !self.sort.ascending {
                    visible.reverse();
                }
            }
            if self.source == LibrarySource::RecentlyAdded {
                // newest import first, whatever the sort (the grid groups by import day)
                let cat = &self.catalog;
                visible.sort_by(|a, b| {
                    let key = |id: &PhotoId| cat.photo(*id).map(|p| p.imported.clone()).unwrap_or_default();
                    key(b).cmp(&key(a)).then(a.cmp(b))
                });
            }
            if self.source == LibrarySource::Missing {
                // only the photos the query kept (library photos, not Local browse records) are checked
                let (cat, avail) = (&self.catalog, &self.media.availability);
                visible.retain(|id| cmd::missing::is_missing(cat, avail, *id));
            }
            if self.source != LibrarySource::RecentlyDeleted {
                visible = self.catalog.arrange_stacks(&visible);
            }
            self.visible = visible.into();
            self.visible_gen = VISIBLE_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.visible_clock_minute = filter_depends_on_clock(&f, &self.catalog).then(|| clock_minute(&now).to_owned());
            self.visible_key = Some(key);
        }
        &self.visible
    }

    /// Photos in the current source (folder, album, …) before the filter bar and search narrow
    /// them; `None` where that is not a separate number (Missing Photos).
    pub fn source_total(&mut self) -> Option<usize> {
        if self.source == LibrarySource::Missing {
            return None;
        }
        let key = (self.catalog.revision, format!("{:?}|{:?}", self.source, self.browse));
        let key_changed = self.total.as_ref().map(|total| &total.0) != Some(&key);
        let cached_minute = self.total.as_ref().and_then(|total| total.2.as_deref());
        let current_now = cached_minute.map(|_| (self.clock)());
        let clock_changed = cached_minute.is_some_and(|cached| current_now.as_deref().map(clock_minute) != Some(cached));
        if key_changed || clock_changed {
            let now = current_now.unwrap_or_else(|| (self.clock)());
            lightcraft_catalog::rules::set_now(Some(now.clone()));
            let mut f = self.source.to_filter(&Filter::default(), &self.catalog);
            if self.source == LibrarySource::Folder {
                let b = self.browse.clone().unwrap_or_default();
                f.folder = Some(b.path);
                f.subfolders = b.subfolders;
            }
            let n = self.catalog.query(&f, &self.sort).len();
            let clocked = filter_depends_on_clock(&f, &self.catalog);
            self.total = Some((key, n, clocked.then(|| clock_minute(&now).to_owned())));
        }
        self.total.as_ref().map(|t| t.1)
    }

    pub fn visible_cloned(&mut self) -> Vec<PhotoId> {
        self.visible().to_vec()
    }

    /// The visible photos without copying them, with their generation: equal generations mean
    /// the very same list (same catalog revision, source, filter, sort), so a frontend can cache
    /// whatever it derives from the list under that number.
    pub fn visible_shared(&mut self) -> (u64, Arc<[PhotoId]>) {
        self.visible();
        (self.visible_gen, self.visible.clone())
    }

    /// Targets of photo commands: the explicit `ids` (photo ids) or `id` (a photo id) param, else
    /// [`Self::selected`]. An `ids` or `id` that is there but isn't photo ids is an error, never the
    /// selection or part of the list: a mistyped target mustn't turn a command meant for one photo
    /// into one for every selected photo.
    pub fn targets(&self, p: &Value) -> Result<Vec<PhotoId>> {
        let invalid = |what: &str| EngineError::BadParams { cmd: String::new(), msg: format!("`{what}` must be photo ids (whole numbers)") };
        match p.get("ids") {
            None | Some(Value::Null) => {}
            Some(Value::Array(a)) => return a.iter().map(|v| v.as_u64().map(PhotoId).ok_or_else(|| invalid("ids"))).collect(),
            Some(_) => return Err(invalid("ids")),
        }
        match p.get("id") {
            None | Some(Value::Null) => Ok(self.selected()),
            Some(v) => v.as_u64().map(|id| vec![PhotoId(id)]).ok_or_else(|| invalid("id")),
        }
    }

    /// The photos commands act on when a call names none: the selection, else the active photo.
    pub fn selected(&self) -> Vec<PhotoId> {
        if self.selection.ids.is_empty() { self.selection.active.into_iter().collect() } else { self.selection.ids.clone() }
    }
}

/// Automatic versions kept per photo.
pub const AUTO_VERSIONS: usize = 20;

/// The parts of `new` that differ from `old` (objects recurse; anything else is taken whole).
pub fn json_delta(old: &Value, new: &Value) -> Option<Value> {
    match (old, new) {
        (Value::Object(a), Value::Object(b)) => {
            let m: serde_json::Map<String, Value> =
                b.iter().filter_map(|(k, nv)| json_delta(a.get(k).unwrap_or(&Value::Null), nv).map(|d| (k.clone(), d))).collect();
            (!m.is_empty()).then_some(Value::Object(m))
        }
        (a, b) if a == b => None,
        (_, b) => Some(b.clone()),
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_color;
#[cfg(test)]
mod tests_enhance;
#[cfg(test)]
mod tests_export;
#[cfg(test)]
mod tests_forget_local;
#[cfg(test)]
mod tests_gpu;
#[cfg(test)]
mod tests_import;
#[cfg(test)]
mod tests_import_move;
#[cfg(test)]
mod tests_layers;
#[cfg(test)]
mod tests_libops;
#[cfg(test)]
mod tests_library;
#[cfg(test)]
mod tests_merge;
#[cfg(test)]
mod tests_organize;
#[cfg(test)]
mod tests_per_photo;
#[cfg(test)]
mod tests_persist;
#[cfg(test)]
mod tests_prefs;
#[cfg(test)]
mod tests_save_over;
#[cfg(test)]
mod tests_segment;
#[cfg(test)]
mod tests_settings_files;
#[cfg(test)]
mod tests_spots;
#[cfg(test)]
mod tests_toolset;
#[cfg(test)]
mod tests_xmp;

pub mod dcp_profiles;

#[cfg(test)]
mod tests_view_follow;
