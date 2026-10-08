//! local-image: the filmstrip under the canvas, for working through a folder: File › Open
//! Folder…, or any photo opened from a folder (Preferences: the filmstrip follows the folder).
//! Modelled on LightCraft's: fixed 120 pt cells with the file name and format, the active image
//! highlighted and kept in view, thumbnails decoded lazily on worker threads from the camera's or
//! the JPEG's embedded preview first (instant), else a DCT-scaled decode.
//!
//! The folder workflow: click (or Alt+←/→, Page Up/Down) opens an image, or switches to its tab.
//! Stepping on closes the image you leave when it has no unsaved edits, so the tabs don't pile up.
//! **Save & Next** (⌘⌥→ / Ctrl+Alt+→) saves the edit and moves on: by default to a copy in an
//! `Edited` folder beside the originals (the strip then shows and reopens the edited copy, with a
//! badge), or over the original, or through Save As. **Review in Library** shows the folder in the
//! Library's grid (without importing it) to compare and rate the results. Window › Filmstrip
//! toggles the strip.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

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
    requested: HashSet<PathBuf>,
    /// Originals with an edited copy in `Edited/`.
    edited: HashSet<PathBuf>,
    follow: Option<PathBuf>,
    /// After stepping: the document left behind, closed once the next one is showing (if it is
    /// still unchanged), and the file being opened.
    pending_close: Option<(u64, PathBuf)>,
    /// Saved revision per open strip document, to refresh its thumbnail after a save.
    saved: HashMap<u64, u64>,
}

struct Worker {
    /// (original, file to decode).
    queue: Mutex<VecDeque<(PathBuf, PathBuf)>>,
    wake: Condvar,
    done: Mutex<Vec<(PathBuf, Option<egui::ColorImage>)>>,
    ctx: Mutex<Option<egui::Context>>,
}

fn strip<R>(f: impl FnOnce(&mut Strip) -> R) -> R {
    thread_local! { static S: std::cell::RefCell<Strip> = std::cell::RefCell::new(Strip::default()); }
    S.with(|s| f(&mut s.borrow_mut()))
}

fn worker() -> &'static Arc<Worker> {
    static W: std::sync::OnceLock<Arc<Worker>> = std::sync::OnceLock::new();
    W.get_or_init(|| {
        let w = Arc::new(Worker { queue: Mutex::new(VecDeque::new()), wake: Condvar::new(), done: Mutex::new(Vec::new()), ctx: Mutex::new(None) });
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 3);
        for i in 0..threads {
            let w = w.clone();
            let _ = std::thread::Builder::new().name(format!("filmstrip-{i}")).spawn(move || {
                loop {
                    let path = {
                        let mut q = w.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                        while q.is_empty() {
                            q = w.wake.wait(q).unwrap_or_else(std::sync::PoisonError::into_inner);
                        }
                        q.pop_front()
                    };
                    let Some((path, source)) = path else { continue };
                    let img = thumbnail(&source);
                    w.done.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push((path, img));
                    if let Some(ctx) = w.ctx.lock().ok().and_then(|c| c.clone()) {
                        ctx.request_repaint();
                    }
                }
            });
        }
        w
    })
}

/// A small upright sRGB thumbnail: the raw file's embedded JPEG, else the JPEG's EXIF thumbnail
/// or a DCT-scaled decode, else any other format decoded and reduced.
pub fn thumbnail(path: &Path) -> Option<egui::ColorImage> {
    let bytes = std::fs::read(path).ok()?;
    let jpeg = lightcraft_raw::embedded_preview(&bytes);
    let src: &[u8] = jpeg.as_deref().unwrap_or(&bytes);
    let opts = lightcraft_codecs::ThumbnailOptions { min_embedded_edge: 160, ..lightcraft_codecs::ThumbnailOptions::new(THUMB_EDGE) };
    let (w, h, px, orientation) = match lightcraft_codecs::decode_thumbnail_with(src, &opts) {
        Ok(t) => (t.image.width, t.image.height, t.image.data.iter().flatten().copied().collect::<Vec<u8>>(), t.orientation),
        Err(_) => {
            let img = image::load_from_memory(&bytes).ok()?.thumbnail(THUMB_EDGE, THUMB_EDGE).to_rgba8();
            (img.width() as usize, img.height() as usize, img.into_raw(), 1)
        }
    };
    let img = image::RgbaImage::from_raw(w as u32, h as u32, px)?;
    let img = orient(img, orientation);
    Some(egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()))
}

/// EXIF orientation 1–8 applied.
fn orient(img: image::RgbaImage, o: u16) -> image::RgbaImage {
    use image::imageops::*;
    match o {
        2 => flip_horizontal(&img),
        3 => rotate180(&img),
        4 => flip_vertical(&img),
        5 => flip_horizontal(&rotate90(&img)),
        6 => rotate90(&img),
        7 => flip_horizontal(&rotate270(&img)),
        8 => rotate270(&img),
        _ => img,
    }
}

/// The folder edited copies go to (Save & Next's default).
pub const EDITED_DIR: &str = "Edited";

/// Raw files can't be written back: their edited copies are TIFFs.
const RAW_EXTS: &[&str] = &["dng", "cr2", "cr3", "nef", "nrw", "arw", "raf", "orf", "rw2", "rwl", "raw", "pef", "srw", "x3f", "3fr", "iiq", "erf", "mos"];

/// Where Save & Next puts the edited copy of `original`: `Edited/<name>` beside it (raw files
/// become `Edited/<stem>.tif`).
pub fn edited_path(original: &Path) -> PathBuf {
    let dir = original.parent().unwrap_or(Path::new("")).join(EDITED_DIR);
    let ext = original.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if RAW_EXTS.contains(&ext.as_str()) {
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
        s.requested.clear();
        s.textures.clear();
        s.edited = edited;
    });
    if let Ok(mut q) = worker().queue.lock() {
        q.clear();
    }
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
    if !app.session.prefs().interface.filmstrip_follows_open || !path.is_file() {
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
    let files = files();
    if files.is_empty() {
        return false;
    }
    let cur = active_file(app).and_then(|p| files.iter().position(|f| *f == p));
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

/// Where Save & Next writes: `edited`, `overwrite` or `ask`.
fn save_to(app: &PhotocraftApp) -> String {
    app.session.prefs().interface.filmstrip_save_to.clone()
}

/// Save & Next: saves the active image's edits (see [`save_to`]), then steps to the next image.
/// An image without edits just moves on. Returns false when the save failed or was cancelled.
pub fn save_and_next(app: &mut PhotocraftApp) -> bool {
    let Some(file) = active_file(app) else {
        app.ui.status = tl!("Open an image from the filmstrip's folder first.").into();
        app.ui.status_error = true;
        return false;
    };
    if app.session.active().is_some_and(|d| d.is_dirty()) {
        let saved = match save_to(app).as_str() {
            "overwrite" => app.write_document(file.display().to_string(), &crate::ExportSettings::default()).map(|_| ()),
            "ask" => app.save_as(None).map(|_| ()),
            _ => {
                let to = edited_path(&file);
                std::fs::create_dir_all(to.parent().unwrap_or(Path::new(""))).map_err(|e| e.to_string()).and_then(|()| app.write_document(to.display().to_string(), &crate::ExportSettings::default()).map(|_| ()))
            }
        };
        if let Err(e) = saved {
            if e != "cancelled" {
                app.ui.status = crate::i18n::fmt(tl!("Couldn't save: {error}"), &[("error", &e)]);
                app.ui.status_error = true;
            }
            return false;
        }
        refresh(&file);
    }
    step(app, 1);
    true
}

/// Re-reads `file`'s thumbnail (and whether it has an edited copy).
fn refresh(file: &Path) {
    let edited = edited_path(file).is_file();
    strip(|s| {
        s.textures.remove(file);
        s.requested.remove(file);
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

/// The strip itself: a bottom panel between the toolbar and the dock.
pub fn panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    settle(app, &ctx);
    if !is_open() {
        return;
    }
    if let Ok(mut c) = worker().ctx.lock()
        && c.is_none()
    {
        *c = Some(ctx.clone());
    }
    watch_saves(app);
    // Keyboard: ⌘⌥→ / Ctrl+Alt+→ Save & Next; Alt+←/→ (Page Up/Down too) step through the folder.
    if !ctx.egui_wants_keyboard_input() {
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
    // Finished thumbnails become textures.
    let done: Vec<_> = std::mem::take(&mut *worker().done.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    for (path, img) in done {
        if let Some(img) = img {
            let tex = ctx.load_texture(format!("film-{}", path.display()), img, egui::TextureOptions::LINEAR);
            strip(|s| s.textures.insert(path, tex));
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
                None => crate::i18n::fmt(tl!("{n} images"), &[("n", &files.len().to_string())]),
            };
            ui.label(RichText::new(count).color(t.text_faint).size(11.0));
            if !edited.is_empty() {
                ui.label(RichText::new(crate::i18n::fmt(tl!("{n} edited"), &[("n", &edited.len().to_string())])).color(t.accent).size(11.0));
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
                    app.switch_to_library = true;
                }
                ui.add_space(4.0);
                let tip = crate::i18n::fmt(
                    match save_to(app).as_str() {
                        "overwrite" => tl!("Save this image over the original and open the next one ({key})"),
                        "ask" => tl!("Save this image (Save As) and open the next one ({key})"),
                        _ => tl!("Save a copy in the Edited folder and open the next one ({key})"),
                    },
                    &[("key", &save_next_key)],
                );
                if widgets::primary_button(ui, tl!("Save & Next"), 0.0).on_hover_text(tip).clicked() {
                    save_and_next(app);
                }
                ui.add_space(4.0);
                if crate::icons::button(ui, "chevron-right", 18.0, false, &crate::i18n::fmt(tl!("Next image ({key})"), &[("key", &format!("{alt}+→"))])).clicked() {
                    step(app, 1);
                }
                if crate::icons::button(ui, "chevron-left", 18.0, false, &crate::i18n::fmt(tl!("Previous image ({key})"), &[("key", &format!("{alt}+←"))])).clicked() {
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
            let mut wanted = Vec::new();
            for (i, path) in files.iter().enumerate().take(last).skip(first) {
                let cell = egui::Rect::from_min_size(egui::pos2(all.left() + i as f32 * CELL_W, all.top()), vec2(CELL_W, h));
                let resp = ui.interact(cell, ui.id().with(("film", i)), Sense::click());
                let is_active = active.as_ref() == Some(path);
                if is_active {
                    ui.painter().rect_filled(cell.shrink(2.0), t.radius, t.row_selected);
                } else if resp.hovered() {
                    ui.painter().rect_filled(cell.shrink(2.0), t.radius, t.row_selected.gamma_multiply(0.6));
                }
                // Name and format.
                let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let stem = if stem.chars().count() > 14 { format!("{}…", stem.chars().take(13).collect::<String>()) } else { stem };
                let ext = path.extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
                ui.painter().text(cell.left_top() + vec2(8.0, 4.0), egui::Align2::LEFT_TOP, stem, egui::FontId::proportional(10.0), t.text_dim);
                ui.painter().text(cell.right_top() + vec2(-8.0, 5.0), egui::Align2::RIGHT_TOP, ext, crate::theme::semibold(8.5), t.text_faint);
                let area = egui::Rect::from_min_max(cell.left_top() + vec2(10.0, 20.0), cell.right_bottom() - vec2(10.0, 8.0));
                match strip(|s| s.textures.get(path).cloned()) {
                    Some(tex) => {
                        let sz = tex.size_vec2();
                        let k = (area.width() / sz.x).min(area.height() / sz.y);
                        let img = egui::Rect::from_center_size(area.center(), sz * k);
                        ui.painter().image(tex.id(), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                        if is_active {
                            ui.painter().rect_stroke(img, 0.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Outside);
                        }
                    }
                    None => {
                        ui.painter().rect_filled(area.shrink(8.0), 2.0, t.field);
                        wanted.push((path.clone(), is_active));
                    }
                }
                // Badges: edited copy saved (check), open in a tab (dot; accent = unsaved edits).
                if edited.contains(path) {
                    let c = area.right_bottom() + vec2(-7.0, -7.0);
                    ui.painter().circle_filled(c, 6.0, t.accent);
                    crate::icons::paint(ui, egui::Rect::from_center_size(c, vec2(9.0, 9.0)), "check", 9.0, Color32::WHITE);
                }
                if open_files.contains(path) {
                    let c = if dirty.contains(path) { t.accent } else { t.text_faint };
                    ui.painter().circle_filled(area.left_bottom() + vec2(4.0, -4.0), 3.0, c);
                }
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let tip = if edited.contains(path) { crate::i18n::fmt(tl!("{name} · edited copy saved"), &[("name", &name)]) } else { name };
                let resp = resp.on_hover_text(tip);
                if resp.clicked() {
                    clicked = Some(path.clone());
                }
                if follow.as_ref() == Some(path) || (is_active && follow.is_some()) {
                    ui.scroll_to_rect(cell, Some(egui::Align::Center));
                }
            }
            // Visible thumbnails are decoded first, the active one before the rest; an edited
            // copy shows instead of its original.
            wanted.sort_by_key(|(_, a)| !*a);
            let new: Vec<PathBuf> = strip(|s| wanted.into_iter().map(|(p, _)| p).filter(|p| s.requested.insert(p.clone())).collect());
            if !new.is_empty()
                && let Ok(mut q) = worker().queue.lock()
            {
                for p in new {
                    let e = edited_path(&p);
                    let source = if e.is_file() { e } else { p.clone() };
                    q.push_back((p, source));
                }
                worker().wake.notify_all();
            }
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
            ("edited", tl!("A copy in the Edited folder (originals untouched)")),
            ("overwrite", tl!("The original file")),
            ("ask", tl!("Ask each time (Save As)")),
        ] {
            if ui.radio(current == key, label).clicked() {
                app.session.prefs.edit(|p| p.interface.filmstrip_save_to = key.into());
            }
        }
        ui.separator();
        let mut follows = app.session.prefs().interface.filmstrip_follows_open;
        if ui.checkbox(&mut follows, tl!("Show the folder when opening a photo")).changed() {
            app.session.prefs.edit(|p| p.interface.filmstrip_follows_open = follows);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnails_decode_and_orient() {
        let dir = std::env::temp_dir().join(format!("li-film-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.png");
        image::RgbaImage::from_pixel(600, 300, image::Rgba([200, 10, 10, 255])).save(&p).unwrap();
        let t = thumbnail(&p).unwrap();
        assert_eq!(t.size[0].max(t.size[1]), THUMB_EDGE as usize);
        assert!(t.size[0] > t.size[1]);
        let o = orient(image::RgbaImage::new(4, 2), 6);
        assert_eq!(o.dimensions(), (2, 4));
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
            export: Some(Box::new(|_d: &Document, _p: &str, _s: &crate::ExportSettings| Ok((b"edited".to_vec(), Vec::new())))),
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
        assert!(save_and_next(&mut app));
        settle(&mut app, &ctx);
        assert_eq!(std::fs::read(dir.join(EDITED_DIR).join("b.png")).unwrap(), b"edited");
        assert_eq!(std::fs::read(dir.join("b.png")).unwrap()[..4], [0x89, b'P', b'N', b'G'], "the original is untouched");
        assert_eq!(active_file(&app), Some(dir.join("c.png")));
        assert_eq!(app.session.documents().len(), 1);
        assert!(strip(|s| s.edited.contains(&dir.join("b.png"))), "the strip marks it edited");

        // Unsaved edits keep their tab when stepping away.
        app.run("layer.new.layer", serde_json::json!({})).unwrap();
        assert!(step(&mut app, -1));
        settle(&mut app, &ctx);
        assert_eq!(app.session.documents().len(), 2, "c has unsaved edits, so it stays open");
        // Going back to b opens its edited copy.
        assert_eq!(app.session.active().and_then(|d| d.path.clone()), Some(dir.join(EDITED_DIR).join("b.png").display().to_string()));
        assert_eq!(active_file(&app), Some(dir.join("b.png")));
        // The last image says so instead of wrapping.
        assert!(!step(&mut app, -1) || active_file(&app) == Some(a.clone()));
        std::fs::remove_dir_all(dir).ok();
    }
}
