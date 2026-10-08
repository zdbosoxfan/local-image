//! local-image: the filmstrip under the canvas when a folder is open (File › Open Folder…),
//! modelled on LightCraft's: fixed 120 pt cells with the file name and format, the active image
//! highlighted and kept in view, thumbnails decoded lazily on worker threads from the camera's or
//! the JPEG's embedded preview first (instant), else a DCT-scaled decode. Click (or Alt+←/→)
//! opens the image, or switches to its tab when it's already open. Window › Filmstrip toggles it.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

use egui::{Color32, RichText, Sense, Stroke, StrokeKind, vec2};

use crate::PhotocraftApp;
use crate::theme::Tokens;

const CELL_W: f32 = 120.0;
const STRIP_H: f32 = 132.0;
const THUMB_EDGE: u32 = 256;

#[derive(Default)]
struct Strip {
    dir: Option<PathBuf>,
    files: Vec<PathBuf>,
    hidden: bool,
    textures: HashMap<PathBuf, egui::TextureHandle>,
    requested: HashSet<PathBuf>,
    follow: Option<PathBuf>,
}

struct Worker {
    queue: Mutex<VecDeque<PathBuf>>,
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
            let _ = std::thread::Builder::new().name(format!("filmstrip-{i}")).spawn(move || loop {
                let path = {
                    let mut q = w.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    while q.is_empty() {
                        q = w.wake.wait(q).unwrap_or_else(std::sync::PoisonError::into_inner);
                    }
                    q.pop_front()
                };
                let Some(path) = path else { continue };
                let img = thumbnail(&path);
                w.done.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push((path, img));
                if let Some(ctx) = w.ctx.lock().ok().and_then(|c| c.clone()) {
                    ctx.request_repaint();
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

/// Shows `files` (a folder's images) in the filmstrip.
pub fn set_folder(dir: &Path, files: Vec<PathBuf>) {
    strip(|s| {
        s.dir = Some(dir.to_path_buf());
        s.follow = files.first().cloned();
        s.files = files;
        s.hidden = false;
        s.requested.clear();
        s.textures.clear();
    });
    if let Ok(mut q) = worker().queue.lock() {
        q.clear();
    }
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

/// The files of the open folder (for automation and tests).
pub fn files() -> Vec<PathBuf> {
    strip(|s| s.files.clone())
}

fn active_path(app: &PhotocraftApp) -> Option<PathBuf> {
    app.session.active().and_then(|d| d.path.as_ref()).map(PathBuf::from)
}

/// Opens `path`, or switches to it when it's already open.
pub fn open(app: &mut PhotocraftApp, path: &Path) {
    let key = path.display().to_string();
    if let Some(i) = app.session.documents().iter().position(|d| d.path.as_deref() == Some(key.as_str())) {
        app.session.set_active(i);
        app.sync_views();
        return;
    }
    match std::fs::read(path) {
        Ok(bytes) => {
            if let Err(e) = app.open_file(&key, &bytes) {
                app.ui.status = e;
                app.ui.status_error = true;
            }
        }
        Err(e) => {
            app.ui.status = format!("Could not read {}: {e}", path.display());
            app.ui.status_error = true;
        }
    }
    strip(|s| s.follow = Some(path.to_path_buf()));
}

/// Moves to the previous (-1) or next (+1) image.
pub fn step(app: &mut PhotocraftApp, delta: i32) {
    let files = files();
    if files.is_empty() {
        return;
    }
    let cur = active_path(app).and_then(|p| files.iter().position(|f| *f == p));
    let next = match cur {
        Some(i) => (i as i64 + delta as i64).clamp(0, files.len() as i64 - 1) as usize,
        None => 0,
    };
    let p = files[next].clone();
    open(app, &p);
}

/// The strip itself: a bottom panel between the toolbar and the dock.
pub fn panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if !is_open() {
        return;
    }
    let ctx = ui.ctx().clone();
    if let Ok(mut c) = worker().ctx.lock()
        && c.is_none()
    {
        *c = Some(ctx.clone());
    }
    // Keyboard: Alt+←/→ (Page Up/Down too) step through the folder.
    if !ctx.egui_wants_keyboard_input() {
        let (prev, next) = ctx.input(|i| {
            (
                (i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft)) || i.key_pressed(egui::Key::PageUp),
                (i.modifiers.alt && i.key_pressed(egui::Key::ArrowRight)) || i.key_pressed(egui::Key::PageDown),
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
    let active = active_path(app);
    let open_paths: HashSet<String> = app.session.documents().iter().filter_map(|d| d.path.clone()).collect();
    let dirty: HashSet<String> = app.session.documents().iter().filter(|d| d.is_dirty()).filter_map(|d| d.path.clone()).collect();
    egui::Panel::bottom("filmstrip").exact_size(STRIP_H).frame(egui::Frame::NONE.fill(t.canvas)).show(ui, |ui| {
        let r = ui.max_rect();
        ui.painter().line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, t.separator));
        // Header: folder, position, close.
        let (files, dir) = strip(|s| (s.files.clone(), s.dir.clone()));
        let pos = active.as_ref().and_then(|a| files.iter().position(|f| f == a));
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let (ir, _) = ui.allocate_exact_size(vec2(14.0, 18.0), Sense::hover());
            crate::icons::paint(ui, ir, "folder", 12.0, t.text_dim);
            let name = dir.as_ref().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            ui.label(RichText::new(name).color(t.text_dim).size(11.5));
            ui.label(RichText::new(match pos {
                Some(i) => format!("{} / {}", i + 1, files.len()),
                None => format!("{} images", files.len()),
            })
            .color(t.text_faint)
            .size(11.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                if crate::icons::button(ui, "x", 18.0, false, "Hide the filmstrip (Window › Filmstrip)").clicked() {
                    toggle();
                }
                if crate::icons::button(ui, "chevron-right", 18.0, false, "Next image (Alt+→)").clicked() {
                    step(app, 1);
                }
                if crate::icons::button(ui, "chevrons-left", 18.0, false, "Previous image (Alt+←)").clicked() {
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
                let key = path.display().to_string();
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
                // Badges: open in a tab, unsaved edits.
                if open_paths.contains(&key) {
                    let c = if dirty.contains(&key) { t.accent } else { t.text_faint };
                    ui.painter().circle_filled(area.left_bottom() + vec2(4.0, -4.0), 3.0, c);
                }
                let resp = resp.on_hover_text(path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
                if resp.clicked() {
                    clicked = Some(path.clone());
                }
                if follow.as_ref() == Some(path) || (is_active && follow.is_some()) {
                    ui.scroll_to_rect(cell, Some(egui::Align::Center));
                }
            }
            // Visible thumbnails are decoded first, the active one before the rest.
            wanted.sort_by_key(|(_, a)| !*a);
            let new: Vec<PathBuf> = strip(|s| wanted.into_iter().map(|(p, _)| p).filter(|p| s.requested.insert(p.clone())).collect());
            if !new.is_empty()
                && let Ok(mut q) = worker().queue.lock()
            {
                for p in new {
                    q.push_back(p);
                }
                worker().wake.notify_all();
            }
        });
        if let Some(p) = clicked {
            open(app, &p);
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
}
