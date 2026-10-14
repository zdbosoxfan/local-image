//! Bounded gallery decode queue. One worker decodes files; UI paint only polls and uploads
//! thumbnail-sized images. Missing files are cached briefly to avoid a retry every frame.
use egui::{ColorImage, Context, TextureHandle};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{OnceLock, mpsc};
use std::time::{Duration, Instant};
struct Request {
    path: PathBuf,
    ctx: Context,
    result: mpsc::Sender<Option<ColorImage>>,
}
fn sender() -> Option<&'static mpsc::SyncSender<Request>> {
    static TX: OnceLock<Option<mpsc::SyncSender<Request>>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<Request>(32);
        std::thread::Builder::new()
            .name("generation-thumbnails".into())
            .spawn(move || {
                lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
                while let Ok(req) = rx.recv() {
                    let image = std::panic::catch_unwind(|| {
                        let img = image::open(&req.path).ok()?.to_rgba8();
                        let thumb = image::imageops::thumbnail(&img, 160.min(img.width()).max(1), (160 * img.height() / img.width().max(1)).clamp(1, 160));
                        Some(ColorImage::from_rgba_unmultiplied([thumb.width() as usize, thumb.height() as usize], thumb.as_raw()))
                    })
                    .ok()
                    .flatten();
                    let _ = req.result.send(image);
                    req.ctx.request_repaint();
                }
            })
            .ok()
            .map(|_| tx)
    })
    .as_ref()
}
#[derive(Default)]
pub(super) struct Thumbs {
    textures: HashMap<String, (TextureHandle, Instant)>,
    pending: HashMap<String, mpsc::Receiver<Option<ColorImage>>>,
    failed: HashMap<String, Instant>,
}
impl Thumbs {
    #[cfg(test)]
    pub(super) fn loaded(&self, key: &str) -> bool {
        self.textures.contains_key(key)
    }
    pub fn remove(&mut self, key: &str) {
        self.textures.remove(key);
        self.pending.remove(key);
        self.failed.remove(key);
    }
    pub fn get(&mut self, ctx: &Context, key: &str, path: &std::path::Path) -> Option<TextureHandle> {
        if let Some((tex, used)) = self.textures.get_mut(key) {
            *used = Instant::now();
            return Some(tex.clone());
        }
        if let Some(rx) = self.pending.get(key) {
            match rx.try_recv() {
                Ok(Some(image)) => {
                    self.pending.remove(key);
                    let tex = ctx.load_texture(format!("gen-{key}"), image, egui::TextureOptions::LINEAR);
                    // Bound thumbnail memory by a small hardware-derived RAM share, <= 512 tiles.
                    let cap = (lightcraft_sysmem::profile().memory_budget() / 1024 / (160 * 160 * 4)).clamp(16, 512) as usize;
                    if self.textures.len() >= cap
                        && let Some(old) = self.textures.iter().min_by_key(|(_, (_, used))| *used).map(|(k, _)| k.clone())
                    {
                        self.textures.remove(&old);
                    }
                    self.textures.insert(key.to_owned(), (tex.clone(), Instant::now()));
                    return Some(tex);
                }
                Ok(None) | Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending.remove(key);
                    self.failed.insert(key.to_owned(), Instant::now());
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(100));
                    return None;
                }
            }
        }
        self.failed.retain(|_, at| at.elapsed() < Duration::from_secs(5));
        if self.failed.contains_key(key) {
            return None;
        }
        let (tx, rx) = mpsc::channel();
        if sender()?.try_send(Request { path: path.to_owned(), ctx: ctx.clone(), result: tx }).is_ok() {
            self.pending.insert(key.to_owned(), rx);
        }
        ctx.request_repaint_after(Duration::from_millis(100));
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gallery_thumbnail_decode_is_queued_and_missing_files_do_not_spin() {
        let root = std::env::temp_dir().join(format!("li-gallery-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("test.png");
        image::RgbaImage::from_pixel(401, 203, image::Rgba([80, 90, 100, 255])).save(&path).unwrap();
        let ctx = Context::default();
        let mut thumbs = Thumbs::default();
        assert!(thumbs.get(&ctx, "photo", &path).is_none());
        assert!(thumbs.pending.contains_key("photo"));
        let start = Instant::now();
        loop {
            if let Some(tex) = thumbs.get(&ctx, "photo", &path) {
                assert_eq!(tex.size()[0], 160);
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(10));
        }
        let missing = root.join("missing.png");
        assert!(thumbs.get(&ctx, "missing", &missing).is_none());
        while !thumbs.failed.contains_key("missing") {
            thumbs.get(&ctx, "missing", &missing);
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(10));
        }
        for _ in 0..10 {
            assert!(thumbs.get(&ctx, "missing", &missing).is_none());
        }
        assert!(!thumbs.pending.contains_key("missing"));
        thumbs.remove("photo");
        assert!(!thumbs.textures.contains_key("photo"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
