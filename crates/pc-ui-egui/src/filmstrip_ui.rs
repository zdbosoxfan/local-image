//! local-image: the filmstrip under the canvas, for working through a folder: File › Open
//! Folder…, or any photo opened from a folder (Preferences: the filmstrip follows the folder).
//! Modelled on LightCraft's: fixed 120 pt cells with the file name and format, the active image
//! highlighted and kept in view. The host supplies thumbnails through the Library preview service,
//! including developed looks, persistent caching and prioritized background decoding.
//!
//! The folder workflow: click (or Alt+←/→, Page Up/Down) opens an image, or switches to its tab.
//! Stepping on closes the image you leave when it has no unsaved edits, so the tabs don't pile up.
//! **Save & Next** (⌘⌥→ / Ctrl+Alt+→) saves the edit and moves on: by default to a copy in an
//! `Edited` folder beside the originals (the strip then shows and reopens the edited copy, with a
//! badge), or over the original, or through Save As. **Review in Library** shows the folder in the
//! Library's grid (without importing it) to compare and rate the results. Window › Filmstrip
//! toggles the strip.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use egui::{Color32, RichText, Sense, Stroke, StrokeKind, vec2};

use crate::PhotocraftApp;
use crate::theme::Tokens;
use crate::widgets;

const CELL_W: f32 = 120.0;
const STRIP_H: f32 = 132.0;
const THUMB_EDGE: u32 = 256;

#[derive(Default)]
struct Strip {
    dir: Option<PathBuf>,
    files: Vec<PathBuf>,
    hidden: bool,
    /// Thumbnails by original file (showing its edited copy when there is one).
    textures: HashMap<PathBuf, egui::TextureHandle>,
    /// Visible requests only; the host owns decoding, deduplication and caching.
    wanted: Vec<PreviewRequest>,
    visible_sources: HashMap<PathBuf, PathBuf>,
    epoch: u64,
    revisions: HashMap<PathBuf, u64>,
    /// Originals with an edited copy in `Edited/`.
    edited: HashSet<PathBuf>,
    follow: Option<PathBuf>,
    /// After stepping: the document left behind, closed once the next one is showing (if it is
    /// still unchanged), and the file being opened.
    pending_close: Option<(u64, PathBuf)>,
    /// Saved revision per open strip document, to refresh its thumbnail after a save.
    saved: HashMap<u64, u64>,
}

/// A visible cell's thumbnail request. Replies must retain this identity across background work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewRequest {
    pub original: PathBuf,
    pub source: PathBuf,
    pub epoch: u64,
    pub revision: u64,
    pub edge: usize,
    pub priority: u32,
}

fn strip<R>(f: impl FnOnce(&mut Strip) -> R) -> R {
    thread_local! { static S: std::cell::RefCell<Strip> = std::cell::RefCell::new(Strip::default()); }
    S.with(|s| f(&mut s.borrow_mut()))
}

/// Take this frame's visible/prefetch requests. Hidden panels request nothing.
pub fn take_preview_requests() -> Vec<PreviewRequest> {
    strip(|s| std::mem::take(&mut s.wanted))
}

/// Share the host's already colour-managed texture; obsolete folder/save replies are ignored.
pub fn accept_preview(ctx: &egui::Context, request: &PreviewRequest, texture: Option<egui::TextureHandle>) -> bool {
    strip(|s| {
        if s.epoch != request.epoch
            || s.revisions.get(&request.original).copied().unwrap_or(0) != request.revision
            || s.visible_sources.get(&request.original) != Some(&request.source)
        {
            return false;
        }
        let changed = s.textures.get(&request.original).map(egui::TextureHandle::id) != texture.as_ref().map(egui::TextureHandle::id);
        if changed {
            ctx.request_repaint();
        }
        match texture {
            Some(texture) => {
                s.textures.insert(request.original.clone(), texture);
            }
            None => {
                s.textures.remove(&request.original);
            }
        }
        true
    })
}

/// A small upright sRGB thumbnail: the raw file's embedded JPEG, else the JPEG's EXIF thumbnail
/// or a DCT-scaled decode, else any other format decoded and reduced.
pub fn thumbnail(path: &Path) -> Option<egui::ColorImage> {
    let bytes = photocraft_format::read_file(path).ok()?;
    let jpeg = lightcraft_raw::embedded_preview(&bytes);
    let src: &[u8] = jpeg.as_deref().unwrap_or(&bytes);
    let opts = lightcraft_codecs::ThumbnailOptions { min_embedded_edge: 160, ..lightcraft_codecs::ThumbnailOptions::new(THUMB_EDGE) };
    let thumb = lightcraft_codecs::decode_thumbnail_with(src, &opts).ok()?;
    // The shared codec implements all eight orientations; do not keep a second pixel walker.
    let mut img = photocraft_codecs::Image::new(
        thumb.image.width as u32,
        thumb.image.height as u32,
        photocraft_codecs::ChannelLayout::Rgba,
        photocraft_codecs::SampleType::U8,
    )
    .ok()?;
    img.data_mut().copy_from_slice(&thumb.image.as_bytes());
    codec_thumbnail(img, thumb.orientation)
}

fn codec_thumbnail(img: photocraft_codecs::Image, orientation: u16) -> Option<egui::ColorImage> {
    let img = img.oriented(orientation).ok()?;
    Some(egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.data()))
}

/// The folder edited copies go to (Save & Next's default).
pub const EDITED_DIR: &str = "Edited";

/// Where Save & Next puts the edited copy of `original`: `Edited/<name>` beside it (raw files
/// become `Edited/<stem>.tif`).
pub fn edited_path(original: &Path) -> PathBuf {
    let dir = original.parent().unwrap_or(Path::new("")).join(EDITED_DIR);
    let ext = original.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if photocraft_codecs::raw_from_extension(&ext) {
        let stem = original.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        dir.join(format!("{stem}.tif"))
    } else {
        dir.join(original.file_name().unwrap_or_default())
    }
}

/// Shows `files` (a folder's images) in the filmstrip (File › Open Folder…).
pub fn set_folder(dir: &Path, files: Vec<PathBuf>) {
    show_folder(dir, files, true);
}

fn show_folder(dir: &Path, files: Vec<PathBuf>, unhide: bool) {
    let edited = files.iter().filter(|f| edited_path(f).is_file()).cloned().collect();
    strip(|s| {
        s.dir = Some(dir.to_path_buf());
        s.follow = files.first().cloned();
        s.files = files;
        if unhide {
            s.hidden = false;
        }
        s.epoch = s.epoch.wrapping_add(1);
        s.revisions.clear();
        s.wanted.clear();
        s.visible_sources.clear();
        s.textures.clear();
        s.edited = edited;
    });
}

/// A file was opened from disk: the strip shows its folder (when that holds more than one image
/// and Preferences › Interface › the filmstrip follows the folder is on) and keeps it in view.
/// An edited copy (`Edited/x.jpg`) counts as its original's folder.
pub fn follow_file(app: &PhotocraftApp, path: &str) {
    let path = Path::new(path);
    let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) else { return };
    let dir = if parent.file_name().is_some_and(|n| n == EDITED_DIR) { parent.parent().unwrap_or(parent) } else { parent };
    let current = strip(|s| s.dir.clone());
    if current.as_deref() == Some(dir) {
        let original = original_of(path);
        strip(|s| s.follow = Some(original));
        return;
    }
    if !app.session.prefs().interface.filmstrip_shows_folder || !path.is_file() {
        return;
    }
    let files = crate::generate_ui::folder_images(dir);
    if files.len() >= 2 {
        show_folder(dir, files, false);
        let original = original_of(path);
        strip(|s| s.follow = Some(original));
    }
}

/// The strip file a document path stands for: itself, or the original of an edited copy.
fn original_of(path: &Path) -> PathBuf {
    strip(|s| s.files.iter().find(|f| f.as_path() == path || edited_path(f) == path).cloned()).unwrap_or_else(|| path.to_path_buf())
}

pub fn is_open() -> bool {
    strip(|s| s.dir.is_some() && !s.hidden)
}

pub fn has_folder() -> bool {
    strip(|s| s.dir.is_some())
}

pub fn toggle() {
    strip(|s| s.hidden = !s.hidden);
}

/// The open folder.
pub fn folder() -> Option<PathBuf> {
    strip(|s| s.dir.clone())
}

/// The files of the open folder (for automation and tests).
pub fn files() -> Vec<PathBuf> {
    strip(|s| s.files.clone())
}

/// The strip file the active document is (an original, or the original of its edited copy).
fn active_file(app: &PhotocraftApp) -> Option<PathBuf> {
    let p = app.session.active().and_then(|d| d.path.as_ref()).map(PathBuf::from)?;
    let o = original_of(&p);
    strip(|s| s.files.contains(&o)).then_some(o)
}

/// The open document showing strip file `file` (its original or its edited copy).
fn document_of(app: &PhotocraftApp, file: &Path) -> Option<usize> {
    let edited = edited_path(file);
    app.session.documents().iter().position(|d| d.path.as_deref().is_some_and(|p| Path::new(p) == file || Path::new(p) == edited))
}

/// Opens `path` (its edited copy when there is one, so you carry on from your edit), or switches
/// to it when it's already open.
pub fn open(app: &mut PhotocraftApp, path: &Path) {
    if let Some(i) = document_of(app, path) {
        app.session.set_active(i);
        app.sync_views();
        strip(|s| s.follow = Some(path.to_path_buf()));
        return;
    }
    let edited = edited_path(path);
    let target = if edited.is_file() { edited } else { path.to_path_buf() };
    let key = target.display().to_string();
    if let Err(e) = app.open_path(&key) {
        app.ui.status = e;
        app.ui.status_error = true;
    }
    strip(|s| s.follow = Some(path.to_path_buf()));
}

/// Moves to the previous (-1) or next (+1) image. The image left behind is closed once the next
/// one shows, unless it has unsaved edits (then it stays open in its tab, marked in the strip).
pub fn step(app: &mut PhotocraftApp, delta: i32) -> bool {
    let current = active_file(app);
    step_from(app, current.as_deref(), delta)
}

fn step_from(app: &mut PhotocraftApp, current: Option<&Path>, delta: i32) -> bool {
    let files = files();
    if files.is_empty() {
        return false;
    }
    let cur = current.and_then(|p| files.iter().position(|f| f == p));
    let next = match cur {
        Some(i) => (i as i64 + delta as i64).clamp(0, files.len() as i64 - 1) as usize,
        None => 0,
    };
    if Some(next) == cur {
        app.ui.status = if delta > 0 { tl!("That was the last image in the folder.") } else { tl!("That was the first image in the folder.") }.into();
        app.ui.status_error = false;
        return false;
    }
    let leaving = cur.and_then(|_| app.session.active()).filter(|d| !d.is_dirty()).map(|d| d.doc.id.0);
    let p = files[next].clone();
    open(app, &p);
    strip(|s| s.pending_close = leaving.map(|id| (id, p)));
    true
}

/// Closes the document stepped away from once the next image is the active one.
fn settle(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some((id, target)) = strip(|s| s.pending_close.clone()) else { return };
    let showing = active_file(app).is_some_and(|f| f == target);
    let still_opening = !showing && !app.jobs.opens.is_empty();
    if still_opening {
        return;
    }
    strip(|s| s.pending_close = None);
    if !showing {
        return;
    }
    let docs = app.session.documents();
    if let Some(i) = docs.iter().position(|d| d.doc.id.0 == id)
        && !docs[i].is_dirty()
        && Some(i) != app.session.active_index()
    {
        let active = app.session.active().map(|d| d.doc.id.0);
        let _ = crate::menus::invoke(app, ctx, "file.close", serde_json::json!({ "document": i }));
        // Closing a tab must not move the focus off the image just opened.
        if let Some(j) = active.and_then(|a| app.session.documents().iter().position(|d| d.doc.id.0 == a)) {
            app.session.set_active(j);
            app.sync_views();
        }
    }
}

/// Where Save & Next writes: `editedFolder`, `original` or `saveAs`.
fn save_to(app: &PhotocraftApp) -> String {
    app.session.prefs().interface.save_and_next_saves_to.clone()
}

/// Save & Next: saves the active image's edits (see [`save_to`]), then steps to the next image.
/// An image without edits just moves on. Returns false while saving is pending, or if it failed
/// or was cancelled. A flattened copy keeps its unsaved working tab open.
pub fn save_and_next(app: &mut PhotocraftApp) -> bool {
    let Some(file) = active_file(app) else {
        app.ui.status = tl!("Open an image from the filmstrip's folder first.").into();
        app.ui.status_error = true;
        return false;
    };
    if app.session.active().is_some_and(|d| d.is_dirty()) {
        let saved = match save_to(app).as_str() {
            "original" => app.write_document(file.display().to_string(), &crate::ExportSettings::default()).map(|_| serde_json::Value::Null),
            "saveAs" => app.save_as(None),
            _ => {
                let to = edited_path(&file);
                std::fs::create_dir_all(to.parent().unwrap_or(Path::new("")))
                    .map_err(|e| e.to_string())
                    .and_then(|()| app.write_document(to.display().to_string(), &crate::ExportSettings::default()).map(|_| serde_json::Value::Null))
            }
        };
        // Save As asks where first: the step waits for its answer.
        if crate::file_dialog::waiting(&saved) {
            let _ = app.after_file_dialog(saved, move |app, saved| {
                step_after_save(app, saved, file);
                Ok(serde_json::Value::Null)
            });
            return false;
        }
        return step_after_save(app, saved, file);
    }
    step_from(app, Some(&file), 1);
    true
}

/// Save & Next once `file`'s edits are saved: steps to the next image, or leaves that to the save
/// still running or the prompt it waits on. False when the save failed (reported), was cancelled
/// or is still pending.
fn step_after_save(app: &mut PhotocraftApp, saved: Result<serde_json::Value, String>, file: PathBuf) -> bool {
    if let Err(e) = saved {
        if e != crate::file_dialog::CANCELLED {
            app.ui.status = li_i18n::fmt(tl!("Couldn't save: {error}"), &[("error", &e)]);
            app.ui.status_error = true;
        }
        return false;
    }
    if let Some(pending) = app.pending_save.as_mut() {
        pending.next_file = Some(file);
        return false;
    }
    if let Some(prompt) = app.tiff_options.as_mut() {
        prompt.next_file = Some(file);
        return false;
    }
    refresh(&file);
    step_from(app, Some(&file), 1);
    true
}

pub(crate) fn saved_and_next(app: &mut PhotocraftApp, file: &Path) {
    refresh(file);
    step_from(app, Some(file), 1);
}

/// Re-reads `file`'s thumbnail (and whether it has an edited copy).
fn refresh(file: &Path) {
    let edited = edited_path(file).is_file();
    strip(|s| {
        s.textures.remove(file);
        let revision = s.revisions.entry(file.to_path_buf()).or_default();
        *revision = revision.wrapping_add(1);
        if edited {
            s.edited.insert(file.to_path_buf());
        } else {
            s.edited.remove(file);
        }
    });
}

/// Thumbnails of strip images saved in the editor (Save, Save As, Save & Next) are refreshed.
fn watch_saves(app: &PhotocraftApp) {
    let docs: Vec<(u64, u64, PathBuf)> =
        app.session.documents().iter().filter_map(|d| d.path.as_ref().map(|p| (d.doc.id.0, d.saved_revision, PathBuf::from(p)))).collect();
    for (id, rev, path) in docs {
        let changed = strip(|s| s.saved.insert(id, rev).is_some_and(|old| old != rev));
        if changed {
            let o = original_of(&path);
            if strip(|s| s.files.contains(&o)) {
                refresh(&o);
            }
        }
    }
}

/// Pending navigation and saved-thumbnail maintenance continue while the panel is hidden.
fn maintain(app: &mut PhotocraftApp, ctx: &egui::Context) {
    settle(app, ctx);
    if !is_open() {
        return;
    }
    watch_saves(app);
}

/// The strip itself: a bottom panel between the toolbar and the dock.
pub fn panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    maintain(app, &ctx);
    strip(|s| {
        s.wanted.clear();
        s.visible_sources.clear();
    });
    if !is_open() || !app.ui.view.panels_visible() {
        return;
    }
    // Keyboard: ⌘⌥→ / Ctrl+Alt+→ Save & Next; Alt+←/→ (Page Up/Down too) step through the folder.
    if !crate::restoration_ui::is_open(app) && app.ui.text_edit.is_none() && crate::shortcuts::canvas_keyboard_available(&ctx) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::ALT, egui::Key::ArrowRight)) {
            save_and_next(app);
        }
        let (prev, next) = ctx.input(|i| {
            (
                (i.modifiers.alt && !i.modifiers.command && i.key_pressed(egui::Key::ArrowLeft)) || i.key_pressed(egui::Key::PageUp),
                (i.modifiers.alt && !i.modifiers.command && i.key_pressed(egui::Key::ArrowRight)) || i.key_pressed(egui::Key::PageDown),
            )
        });
        if prev {
            step(app, -1);
        }
        if next {
            step(app, 1);
        }
    }
    let t = Tokens::get(&ctx);
    let active = active_file(app);
    // Strip files open in a tab, and those with unsaved edits.
    let mut open_files = HashSet::new();
    let mut dirty = HashSet::new();
    for d in app.session.documents() {
        if let Some(p) = &d.path {
            let o = original_of(Path::new(p));
            if d.is_dirty() {
                dirty.insert(o.clone());
            }
            open_files.insert(o);
        }
    }
    let alt = crate::shortcuts::pretty("Alt");
    let save_next_key = format!("{}+→", crate::shortcuts::pretty("Cmd+Alt")).replace("⌘⌥+", "⌘⌥");
    egui::Panel::bottom("filmstrip").exact_size(STRIP_H).frame(egui::Frame::NONE.fill(t.canvas)).show(ui, |ui| {
        let r = ui.max_rect();
        ui.painter().line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, t.separator));
        // Header: folder, position, Save & Next, navigation, options, close.
        let (files, dir, edited) = strip(|s| (s.files.clone(), s.dir.clone(), s.edited.clone()));
        let pos = active.as_ref().and_then(|a| files.iter().position(|f| f == a));
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let (ir, _) = ui.allocate_exact_size(vec2(14.0, 18.0), Sense::hover());
            crate::icons::paint(ui, ir, "folder", 12.0, t.text_dim);
            let name = dir.as_ref().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            ui.label(RichText::new(name).color(t.text_dim).size(11.5));
            let count = match pos {
                Some(i) => format!("{} / {}", i + 1, files.len()),
                None => li_i18n::fmt(tl!("{n} images"), &[("n", &files.len().to_string())]),
            };
            ui.label(RichText::new(count).color(t.text_faint).size(11.0));
            if !edited.is_empty() {
                ui.label(RichText::new(li_i18n::fmt(tl!("{n} edited"), &[("n", &edited.len().to_string())])).color(t.accent).size(11.0));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                if crate::icons::button(ui, "x", 18.0, false, tl!("Hide the filmstrip (Window › Filmstrip)")).clicked() {
                    toggle();
                }
                options_menu(app, ui);
                if app.host_modes
                    && let Some(d) = &dir
                    && crate::icons::button(ui, "grid-2x2", 18.0, false, tl!("Review in Library: see this folder in the Library's grid to compare and rate"))
                        .clicked()
                {
                    app.browse_in_library = Some(d.display().to_string());
                    app.switch_module = Some(crate::Module::Library);
                }
                ui.add_space(4.0);
                let tip = li_i18n::fmt(
                    match save_to(app).as_str() {
                        "original" => tl!("Save this image over the original and open the next one ({key})"),
                        "saveAs" => tl!("Save this image (Save As) and open the next one ({key})"),
                        _ => tl!("Save a copy in the Edited folder and open the next one ({key})"),
                    },
                    &[("key", &save_next_key)],
                );
                if widgets::primary_button(ui, tl!("Save & Next"), 0.0).on_hover_text(tip).clicked() {
                    save_and_next(app);
                }
                ui.add_space(4.0);
                if crate::icons::button(ui, "chevron-right", 18.0, false, &li_i18n::fmt(tl!("Next image ({key})"), &[("key", &format!("{alt}+→"))])).clicked()
                {
                    step(app, 1);
                }
                if crate::icons::button(ui, "chevron-left", 18.0, false, &li_i18n::fmt(tl!("Previous image ({key})"), &[("key", &format!("{alt}+←"))]))
                    .clicked()
                {
                    step(app, -1);
                }
            });
        });
        let follow = strip(|s| s.follow.take());
        let mut clicked: Option<PathBuf> = None;
        egui::ScrollArea::horizontal().id_salt("filmstrip").auto_shrink([false, false]).show_viewport(ui, |ui, viewport| {
            ui.style_mut().always_scroll_the_only_direction = true;
            let h = ui.available_height();
            let (all, _) = ui.allocate_exact_size(vec2(files.len() as f32 * CELL_W, h), Sense::hover());
            let first = ((viewport.min.x / CELL_W).floor() as isize - 4).max(0) as usize;
            let last = (((viewport.max.x / CELL_W).ceil() as usize) + 4).min(files.len());
            if let Some(index) = follow.as_ref().and_then(|target| files.iter().position(|p| p == target)) {
                let cell = egui::Rect::from_min_size(all.left_top() + vec2(index as f32 * CELL_W, 0.0), vec2(CELL_W, h));
                ui.scroll_to_rect(cell, Some(egui::Align::Center));
            }
            let mut wanted = Vec::new();
            for (i, path) in files.iter().enumerate().take(last).skip(first) {
                let cell = egui::Rect::from_min_size(egui::pos2(all.left() + i as f32 * CELL_W, all.top()), vec2(CELL_W, h));
                let resp = ui.interact(cell, ui.id().with(("film", i)), Sense::click());
                let is_active = active.as_ref() == Some(path);
                if is_active {
                    ui.painter().rect_filled(cell.shrink(2.0), t.radius, t.row_selected);
                } else if resp.hovered() {
                    ui.painter().rect_filled(cell.shrink(2.0), t.radius, t.hover);
                }
                // Name and format.
                let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let stem = if stem.chars().count() > 14 { format!("{}…", stem.chars().take(13).collect::<String>()) } else { stem };
                let ext = path.extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
                ui.painter().text(cell.left_top() + vec2(8.0, 4.0), egui::Align2::LEFT_TOP, stem, crate::theme::semibold(10.5), t.text_dim);
                ui.painter().text(cell.right_top() + vec2(-8.0, 5.0), egui::Align2::RIGHT_TOP, ext, crate::theme::semibold(10.5), t.text_faint);
                let area = egui::Rect::from_min_max(cell.left_top() + vec2(10.0, 20.0), cell.right_bottom() - vec2(10.0, 8.0));
                match strip(|s| s.textures.get(path).cloned()) {
                    Some(tex) => {
                        let sz = tex.size_vec2();
                        let k = (area.width() / sz.x).min(area.height() / sz.y);
                        let img = egui::Rect::from_center_size(area.center(), sz * k);
                        ui.painter().image(tex.id(), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                        if is_active {
                            ui.painter().rect_stroke(img, 0.0, Stroke::new(1.0, t.accent_border), StrokeKind::Outside);
                        }
                    }
                    None => {
                        ui.painter().rect_filled(area.shrink(ui.spacing().item_spacing.x), t.radius_sm, t.field);
                    }
                }
                let source = if edited.contains(path) { edited_path(path) } else { path.clone() };
                let visible = cell.intersects(ui.clip_rect());
                wanted.push((
                    path.clone(),
                    source,
                    if is_active {
                        12
                    } else if visible {
                        10
                    } else {
                        2
                    },
                ));
                resp.widget_info(|| {
                    egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, is_active, path.file_name().unwrap_or_default().to_string_lossy())
                });
                // Badges: edited copy saved (check), open in a tab (dot; accent = unsaved edits).
                if edited.contains(path) {
                    let c = area.right_bottom() + vec2(-7.0, -7.0);
                    ui.painter().circle_filled(c, 6.0, t.accent);
                    crate::icons::paint(ui, egui::Rect::from_center_size(c, vec2(9.0, 9.0)), "check", 9.0, t.accent_text);
                }
                if open_files.contains(path) {
                    let c = if dirty.contains(path) { t.accent } else { t.text_faint };
                    ui.painter().circle_filled(area.left_bottom() + vec2(4.0, -4.0), 3.0, c);
                }
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let tip = if edited.contains(path) { li_i18n::fmt(tl!("{name} · edited copy saved"), &[("name", &name)]) } else { name };
                let resp = resp.on_hover_text(tip);
                if resp.clicked() {
                    clicked = Some(path.clone());
                }
            }
            strip(|s| {
                s.visible_sources = wanted.iter().map(|(p, source, _)| (p.clone(), source.clone())).collect();
                s.textures.retain(|path, _| s.visible_sources.contains_key(path));
                s.wanted = wanted
                    .into_iter()
                    .map(|(original, source, priority)| PreviewRequest {
                        revision: s.revisions.get(&original).copied().unwrap_or(0),
                        epoch: s.epoch,
                        original,
                        source,
                        priority,
                        edge: ((CELL_W * ui.ctx().pixels_per_point()).ceil() as usize).clamp(128, THUMB_EDGE as usize * 2),
                    })
                    .collect();
            });
        });
        if let Some(p) = clicked {
            let leaving = app.session.active().filter(|d| !d.is_dirty() && active.is_some()).map(|d| d.doc.id.0);
            open(app, &p);
            strip(|s| s.pending_close = leaving.filter(|_| active.as_ref() != Some(&p)).map(|id| (id, p.clone())));
        }
    });
}

/// The strip's options: where Save & Next saves, and whether opening a file shows its folder.
fn options_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let resp = crate::icons::button(ui, "settings", 18.0, false, tl!("Filmstrip options"));
    egui::Popup::menu(&resp).show(|ui| {
        ui.set_min_width(260.0);
        ui.label(RichText::new(tl!("Save & Next saves to")).strong());
        let current = save_to(app);
        for (key, label) in [
            ("editedFolder", tl!("A copy in the Edited folder (originals untouched)")),
            ("original", tl!("The original file")),
            ("saveAs", tl!("Ask each time (Save As)")),
        ] {
            if widgets::command_menu_item(ui, label, None, Some(current == key), None).clicked() {
                app.session.prefs.edit(|p| p.interface.save_and_next_saves_to = key.into());
            }
        }
        ui.separator();
        let mut follows = app.session.prefs().interface.filmstrip_shows_folder;
        if widgets::checkbox(ui, &mut follows, tl!("Show the folder when opening a photo")).changed() {
            app.session.prefs.edit(|p| p.interface.filmstrip_shows_folder = follows);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn shared_orientation_matches_all_legacy_golden_pixels() {
        // Asymmetric labelled 3x2 pixels, including partial alpha. TIFF/EXIF's eight layouts.
        let expected = [
            vec![0, 1, 2, 3, 4, 5],
            vec![2, 1, 0, 5, 4, 3],
            vec![5, 4, 3, 2, 1, 0],
            vec![3, 4, 5, 0, 1, 2],
            vec![0, 3, 1, 4, 2, 5],
            vec![3, 0, 4, 1, 5, 2],
            vec![5, 2, 4, 1, 3, 0],
            vec![2, 5, 1, 4, 0, 3],
        ];
        for (i, indices) in expected.iter().enumerate() {
            let mut img = photocraft_codecs::Image::new(3, 2, photocraft_codecs::ChannelLayout::Rgba, photocraft_codecs::SampleType::U8).unwrap();
            for (n, pixel) in img.data_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
                pixel.copy_from_slice(&[n as u8 * 30, 40, 50, 128]);
            }
            let out = codec_thumbnail(img, i as u16 + 1).unwrap();
            assert_eq!(out.size, if i >= 4 { [2, 3] } else { [3, 2] });
            let golden: Vec<_> = indices.iter().map(|n| Color32::from_rgba_unmultiplied(n * 30, 40, 50, 128)).collect();
            assert_eq!(out.pixels, golden, "EXIF orientation {}", i + 1);
        }
    }

    #[test]
    fn ui_click_refresh_hide_and_scroll_bound_preview_requests() {
        use egui_kittest::kittest::Queryable;
        let (mut h, _, _, b) = modal_strip_harness();
        // Click the actual cell through accessibility, then verify navigation and host requests.
        h.get_by_label("modal-strip-b.png").click();
        h.run_steps(3);
        assert_eq!(h.state().session.active().unwrap().doc.id.0, b);
        let requests = take_preview_requests();
        assert_eq!(requests.len(), 2);
        let old = requests.iter().find(|r| r.original.ends_with("modal-strip-b.png")).unwrap().clone();
        let tex = h.ctx.load_texture("filmstrip", egui::ColorImage::filled([2, 2], Color32::GRAY), Default::default());
        assert!(accept_preview(&h.ctx, &old, Some(tex.clone())));
        refresh(&old.original);
        assert!(!accept_preview(&h.ctx, &old, Some(tex.clone())), "late pre-save pixels must be rejected");
        h.run_steps(1);
        let fresh = take_preview_requests().into_iter().find(|r| r.original == old.original).unwrap();
        assert_eq!(fresh.revision, old.revision + 1);
        assert!(accept_preview(&h.ctx, &fresh, Some(tex.clone())));
        h.get_by_label("Hide the filmstrip (Window › Filmstrip)").click();
        h.run_steps(2);
        assert!(take_preview_requests().is_empty());
        toggle();
        let paths = (0..10000).map(|i| PathBuf::from(format!("/fixture/large-{i:05}.png"))).collect();
        set_folder(Path::new("/fixture"), paths);
        strip(|s| s.follow = Some(PathBuf::from("/fixture/large-09000.png")));
        h.run_steps(5);
        let requests = take_preview_requests();
        assert!(requests.len() <= 22, "10,000 files request only a viewport plus prefetch: {}", requests.len());
        assert!(requests.iter().any(|r| r.original.ends_with("large-09000.png")), "follow keeps a distant active file visible");
        assert!(!accept_preview(&h.ctx, &fresh, Some(tex)), "late previous-folder pixels must be rejected");
        assert!(strip(|s| s.textures.len()) <= requests.len());
        strip(|s| *s = Strip::default());
    }

    #[test]
    fn filmstrip_raw_copy_paths_use_complete_shared_registry() {
        for ext in photocraft_codecs::RAW_EXTENSIONS {
            let source = PathBuf::from(format!("/fixture/photo.{}", ext.to_ascii_uppercase()));
            assert_eq!(edited_path(&source), PathBuf::from("/fixture/Edited/photo.tif"));
        }
        assert_eq!(edited_path(Path::new("/fixture/photo.png")), PathBuf::from("/fixture/Edited/photo.png"));
    }

    #[test]
    fn bugfix_save_o007_filmstrip_waits_for_tiff_then_steps_from_original_file() {
        for discard_layers in [false, true] {
            let mut app = PhotocraftApp::new(
                photocraft_engine::Session::new(),
                crate::Services {
                    export: Some(std::sync::Arc::new(|_, _, _| Ok((vec![1], vec![])))),
                    write: Some(Box::new(|_, _| Ok(()))),
                    file_dialog: Some(crate::file_dialog::answering(|_| Some(crate::FileDialogAnswer::SaveTo("/fixture/copy.tif".into())))),
                    ..Default::default()
                },
            );
            app.run("file.new", serde_json::json!({"width":8,"height":8})).unwrap();
            app.run("layer.new.layer", serde_json::json!({})).unwrap();
            app.session.active_mut().unwrap().path = Some("/fixture/a.png".into());
            let first = app.session.active().unwrap().doc.id;
            app.run("file.new", serde_json::json!({"width":8,"height":8})).unwrap();
            app.session.active_mut().unwrap().path = Some("/fixture/b.png".into());
            let next = app.session.active().unwrap().doc.id;
            app.session.set_active(0);
            app.session.edit_prefs(|p| p.interface.save_and_next_saves_to = "saveAs".into());
            set_folder(Path::new("/fixture"), vec![PathBuf::from("/fixture/a.png"), PathBuf::from("/fixture/b.png")]);
            assert!(!save_and_next(&mut app));
            assert_eq!(app.session.active().unwrap().doc.id, first);
            assert!(app.tiff_options.is_some());
            app.tiff_options.as_mut().unwrap().discard_layers = discard_layers;
            let ctx = egui::Context::default();
            PhotocraftApp::setup_context(&ctx, Default::default());
            let mut output = ctx.run_ui(Default::default(), |ui| crate::tiff_options_ui::show(&mut app, ui.ctx()));
            output.textures_delta.clear();
            let raw = egui::RawInput {
                events: vec![egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }],
                ..Default::default()
            };
            let mut output = ctx.run_ui(raw, |ui| crate::tiff_options_ui::show(&mut app, ui.ctx()));
            output.textures_delta.clear();
            assert_eq!(app.session.active().unwrap().doc.id, next);
            assert_eq!(app.session.documents()[0].is_dirty(), discard_layers, "a flattened copy keeps its working tab unsaved");
            strip(|s| *s = Strip::default());
        }
    }

    #[test]
    fn thumbnails_decode_and_orient() {
        let dir = std::env::temp_dir().join(format!("li-film-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.png");
        image::RgbaImage::from_pixel(600, 300, image::Rgba([200, 10, 10, 255])).save(&p).unwrap();
        let t = thumbnail(&p).unwrap();
        assert_eq!(t.size[0].max(t.size[1]), THUMB_EDGE as usize);
        assert!(t.size[0] > t.size[1]);

        std::fs::remove_dir_all(dir).ok();
    }

    /// The folder workflow: opening a photo shows its folder, Save & Next saves an edited copy
    /// and moves on, the clean image left behind closes, and going back reopens the edited copy.
    #[test]
    fn save_and_next_works_through_a_folder() {
        use photocraft_color::{ColorMode, SampleType};
        use photocraft_doc::Document;
        use photocraft_geom::Size;
        let dir = std::env::temp_dir().join(format!("li-film-flow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["a", "b", "c"] {
            image::RgbaImage::from_pixel(8, 8, image::Rgba([200, 10, 10, 255])).save(dir.join(format!("{n}.png"))).unwrap();
        }
        let services = crate::Services {
            import: Some(Box::new(|name: &str, _b: &[u8]| Ok((Document::new(name, Size::new(8, 8), ColorMode::Rgb, SampleType::U8), Vec::new())))),
            export: Some(std::sync::Arc::new(|_d: &Document, _p: &str, _s: &crate::ExportSettings| Ok((b"edited".to_vec(), Vec::new())))),
            write: Some(Box::new(|p: &str, b: &[u8]| std::fs::write(p, b).map_err(|e| e.to_string()))),
            ..Default::default()
        };
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
        app.background_jobs = false;
        let ctx = egui::Context::default();
        let a = dir.join("a.png");
        app.open_path(&a.display().to_string()).unwrap();
        assert_eq!(files().len(), 3, "opening a photo shows its folder");
        assert_eq!(active_file(&app), Some(a.clone()));

        // An unedited image just moves on, and closes behind you.
        assert!(save_and_next(&mut app));
        settle(&mut app, &ctx);
        assert_eq!(active_file(&app), Some(dir.join("b.png")));
        assert_eq!(app.session.documents().len(), 1, "the clean image left behind is closed");

        // An edited one is saved to Edited/ first.
        app.run("layer.new.layer", serde_json::json!({})).unwrap();
        let working = app.session.active().unwrap().doc.clone();
        assert!(save_and_next(&mut app));
        settle(&mut app, &ctx);
        assert_eq!(std::fs::read(dir.join(EDITED_DIR).join("b.png")).unwrap(), b"edited");
        assert_eq!(std::fs::read(dir.join("b.png")).unwrap()[..4], [0x89, b'P', b'N', b'G'], "the original is untouched");
        assert_eq!(active_file(&app), Some(dir.join("c.png")));
        assert_eq!(app.session.documents().len(), 2, "a PNG copy keeps the unsaved working document open");
        assert!(app.session.documents().iter().find(|d| d.doc.id == working.id).unwrap().is_dirty());
        assert!(strip(|s| s.edited.contains(&dir.join("b.png"))), "the strip marks it edited");

        // Unsaved edits keep their tab when stepping away.
        app.run("layer.new.layer", serde_json::json!({})).unwrap();
        assert!(step(&mut app, -1));
        settle(&mut app, &ctx);
        assert_eq!(app.session.documents().len(), 2, "c has unsaved edits, so it stays open");
        // Going back to b resumes the complete working document.
        assert_eq!(app.session.active().and_then(|d| d.path.clone()), Some(dir.join("b.png").display().to_string()));
        assert_eq!(app.session.active().unwrap().doc, working);
        assert!(app.session.active().unwrap().is_dirty());
        assert_eq!(active_file(&app), Some(dir.join("b.png")));
        // The last image says so instead of wrapping.
        assert!(!step(&mut app, -1) || active_file(&app) == Some(a.clone()));
        std::fs::remove_dir_all(dir).ok();
    }

    fn modal_strip_harness() -> (egui_kittest::Harness<'static, PhotocraftApp>, Arc<std::sync::atomic::AtomicUsize>, u64, u64) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let writes = Arc::new(AtomicUsize::new(0));
        let sink = writes.clone();
        let mut app = PhotocraftApp::new(
            photocraft_engine::Session::new(),
            crate::Services {
                export: Some(std::sync::Arc::new(|_, _, _| Ok((vec![1, 2, 3], vec![])))),
                write: Some(Box::new(move |_, _| {
                    sink.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })),
                ..Default::default()
            },
        );
        let paths = [PathBuf::from("/fixture/modal-strip-a.png"), PathBuf::from("/fixture/modal-strip-b.png")];
        let mut ids = Vec::new();
        for path in &paths {
            app.run("file.new", serde_json::json!({"width":16,"height":16})).unwrap();
            let state = app.session.active_mut().unwrap();
            state.path = Some(path.to_string_lossy().into_owned());
            state.saved_revision = state.revision;
            ids.push(state.doc.id.0);
        }
        app.session.set_active(0);
        app.session.edit_prefs(|prefs| prefs.interface.save_and_next_saves_to = "original".into());
        // Already-open documents avoid file loads; thumbnail requests are serviced by the host.
        strip(|state| *state = Strip { dir: Some(PathBuf::from("/fixture")), files: paths.to_vec(), ..Default::default() });
        let mut h = egui_kittest::Harness::builder().with_size(vec2(1200., 850.)).build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            app
        });
        h.run_steps(5);
        (h, writes, ids[0], ids[1])
    }

    #[test]
    fn restoration_modal_owns_filmstrip_page_and_save_next_keys() {
        use std::sync::atomic::Ordering;
        let (mut h, writes, a, _) = modal_strip_harness();
        h.state_mut().run("layer.new.layer", serde_json::json!({"name":"Unsaved work"})).unwrap();
        let doc = h.state().session.active().unwrap().doc.clone();
        assert!(h.state().session.active().unwrap().is_dirty());
        crate::restoration_ui::open(h.state_mut());
        h.run_steps(3);
        for (key, mods) in [
            (egui::Key::PageDown, egui::Modifiers::NONE),
            (egui::Key::ArrowRight, egui::Modifiers::ALT),
            (egui::Key::ArrowRight, egui::Modifiers::COMMAND | egui::Modifiers::ALT),
        ] {
            h.key_press_modifiers(mods, key);
            h.run_steps(3);
            assert_eq!(h.state().session.active().unwrap().doc.id.0, a);
            assert!(Arc::ptr_eq(&h.state().session.active().unwrap().doc, &doc));
            assert_eq!(writes.load(Ordering::SeqCst), 0);
            assert!(h.state().tiff_options.is_none() && h.state().overwrite_prompt.is_none());
            assert!(crate::restoration_ui::is_open(h.state()));
        }
        strip(|state| *state = Strip::default());
    }

    #[test]
    fn tab_hidden_filmstrip_settles_completed_step_without_closing_dirty_work() {
        for dirty in [false, true] {
            let (mut h, _, a, b) = modal_strip_harness();
            assert!(step(h.state_mut(), 1));
            assert!(strip(|state| state.pending_close.is_some()));
            assert_eq!(h.state().session.active().unwrap().doc.id.0, b);
            if dirty {
                h.state_mut().session.set_active(0);
                h.state_mut().run("layer.new.layer", serde_json::json!({"name":"Late edit"})).unwrap();
                h.state_mut().session.set_active(1);
            }
            strip(|state| {
                state.saved.remove(&b);
            });
            h.state_mut().ui.view.panels_hidden = true;
            h.run_steps(3);
            assert_eq!(h.state().session.active().unwrap().doc.id.0, b);
            assert_eq!(h.state().session.documents().iter().any(|state| state.doc.id.0 == a), dirty);
            assert!(!strip(|state| state.pending_close.is_some()));
            assert!(h.state().ui.view.panels_hidden && is_open());
            assert!(strip(|state| state.saved.contains_key(&b)), "save observation continues while hidden");
            strip(|state| *state = Strip::default());
        }
    }
}
