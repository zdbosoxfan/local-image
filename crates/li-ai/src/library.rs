//! The generated-image library: every AI result is kept with its prompt, model and seed so it can
//! be found, reopened, reused as a reference or recreated. Same on-disk layout as 0.7
//! (`state/generation-library/<id>/{image.png, thumbnail.png, entry.json}`), so existing
//! libraries carry over.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::settings::state_dir;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    /// Seconds since the Unix epoch.
    pub created_at: f64,
    pub width: u32,
    pub height: u32,
    /// `{model, variant, prompt, negative_prompt, width, height, seed, transparent, steps,
    /// guidance, denoise, reference_count, loras}`, or null.
    #[serde(default)]
    pub generation: Option<Value>,
    #[serde(default)]
    pub reference_attributions: Vec<Value>,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upscale: Option<Value>,
}

impl Entry {
    pub fn prompt(&self) -> &str {
        self.generation.as_ref().and_then(|g| g.get("prompt")).and_then(Value::as_str).unwrap_or("")
    }
    pub fn model(&self) -> &str {
        self.generation.as_ref().and_then(|g| g.get("model")).and_then(Value::as_str).or_else(|| self.upscale.as_ref().map(|_| "seedvr2")).unwrap_or("")
    }
    pub fn seed(&self) -> Option<u64> {
        self.generation.as_ref().and_then(|g| g.get("seed")).and_then(Value::as_u64)
    }
}

#[derive(Clone, Debug)]
pub struct Library {
    root: PathBuf,
}

/// Encoded generation held outside the visible library until its owning job is accepted.
/// Dropping a cancelled/failed job's payload removes its unpublished files.
#[derive(Debug)]
pub struct PreparedEntry {
    staging: PathBuf,
    destination: PathBuf,
    entry: Entry,
    published: bool,
}

impl PreparedEntry {
    /// A single directory rename publishes the image, thumbnail and metadata together.
    pub fn publish(mut self) -> Result<Entry> {
        li_fs::sync_dir_checked(&self.staging)?;
        li_fs::retry_rename(li_fs::RenameRetry::platform(), || std::fs::rename(&self.staging, &self.destination))?;
        self.published = true;
        if let Some(parent) = self.destination.parent() {
            li_fs::sync_dir(parent);
        }
        Ok(self.entry.clone())
    }
}

impl Drop for PreparedEntry {
    fn drop(&mut self) {
        if !self.published
            && let Err(e) = std::fs::remove_dir_all(&self.staging)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            log::warn!("Couldn't remove unpublished generation: {e}");
        }
    }
}

impl Default for Library {
    fn default() -> Self {
        Self::open(state_dir().join("generation-library"))
    }
}

fn valid_id(id: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok_and(|u| u.hyphenated().to_string() == id)
}

impl Library {
    pub fn open(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn image_path(&self, id: &str) -> PathBuf {
        self.root.join(id).join("image.png")
    }
    pub fn thumbnail_path(&self, id: &str) -> PathBuf {
        self.root.join(id).join("thumbnail.png")
    }

    /// Adds an image; returns its entry. Writes are staged and renamed into place.
    pub fn add(&self, image: &RgbaImage, name: &str, generation: Option<Value>, upscale: Option<Value>) -> Result<Entry> {
        self.prepare(image, name, generation, upscale)?.publish()
    }

    /// Encode on a worker; defer visible publication until the job's apply step.
    pub fn prepare(&self, image: &RgbaImage, name: &str, generation: Option<Value>, upscale: Option<Value>) -> Result<PreparedEntry> {
        let id = uuid::Uuid::new_v4().hyphenated().to_string();
        let png = crate::imaging::encode_png(image)?;
        let thumb = image::imageops::thumbnail(
            image,
            (480 * image.width() / image.width().max(image.height()).max(1)).max(1),
            (480 * image.height() / image.width().max(image.height()).max(1)).max(1),
        );
        let entry = Entry {
            id: id.clone(),
            name: if name.ends_with(".png") { name.to_owned() } else { format!("{name}.png") },
            created_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0),
            width: image.width(),
            height: image.height(),
            generation,
            reference_attributions: Vec::new(),
            sha256: hex::encode(Sha256::digest(&png)),
            upscale,
        };
        let staging = self.root.join(format!(".pending-{}", uuid::Uuid::new_v4().simple()));
        let prepared = PreparedEntry { staging, destination: self.root.join(&id), entry, published: false };
        std::fs::create_dir_all(&prepared.staging)?;
        li_fs::atomic_write(&prepared.staging.join("image.png"), &png)?;
        li_fs::atomic_write(&prepared.staging.join("thumbnail.png"), &crate::imaging::encode_png(&thumb)?)?;
        li_fs::atomic_write(&prepared.staging.join("entry.json"), serde_json::to_string_pretty(&prepared.entry)?.as_bytes())?;
        Ok(prepared)
    }

    /// All entries, newest first. Damaged entries are skipped.
    pub fn list(&self) -> Vec<Entry> {
        let Ok(rd) = std::fs::read_dir(&self.root) else { return Vec::new() };
        let mut out: Vec<Entry> = rd
            .flatten()
            .filter(|e| e.file_name().to_str().is_some_and(valid_id))
            .filter_map(|e| std::fs::read(e.path().join("entry.json")).ok())
            .filter_map(|b| serde_json::from_slice::<Entry>(&b).ok())
            .filter(|e| valid_id(&e.id))
            .collect();
        out.sort_by(|a, b| b.created_at.total_cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
        out
    }

    /// Loads an entry's image after checking it against its recorded hash.
    pub fn load(&self, entry: &Entry) -> Result<RgbaImage> {
        if !valid_id(&entry.id) {
            bail!("invalid library entry");
        }
        let bytes = std::fs::read(self.image_path(&entry.id)).context("The library image is missing")?;
        if hex::encode(Sha256::digest(&bytes)) != entry.sha256 {
            bail!("The library copy of {} is damaged.", entry.name);
        }
        Ok(image::load_from_memory(&bytes)?.to_rgba8())
    }

    pub fn delete(&self, ids: &[String]) -> Result<usize> {
        for id in ids {
            if !valid_id(id) {
                bail!("invalid library entry");
            }
        }
        let mut n = 0;
        for id in ids {
            let dir = self.root.join(id);
            for f in ["image.png", "thumbnail.png", "entry.json"] {
                let _ = std::fs::remove_file(dir.join(f));
            }
            if std::fs::remove_dir(&dir).is_ok() {
                n += 1;
            }
        }
        Ok(n)
    }

    /// Total bytes on disk.
    pub fn size(&self) -> u64 {
        walk_size(&self.root)
    }
}

fn walk_size(p: &Path) -> u64 {
    std::fs::read_dir(p)
        .map(|rd| rd.flatten().map(|e| if e.path().is_dir() { walk_size(&e.path()) } else { e.metadata().map(|m| m.len()).unwrap_or(0) }).sum())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn j11_generation_transaction_durable_golden_and_fault_cleanup() {
        let dir = std::env::temp_dir().join(format!("j11-generation-{}", uuid::Uuid::new_v4()));
        let library = Library::open(&dir);
        let image = RgbaImage::from_pixel(8, 8, image::Rgba([12, 34, 56, 128]));
        {
            let _fault = li_fs::fail_writes_after(3);
            assert!(library.prepare(&image, "failed", None, None).is_err());
        }
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let before = li_fs::syncs_on_this_thread();
        let prepared = library.prepare(&image, "accepted", None, None).unwrap();
        assert!(library.list().is_empty());
        assert_eq!(std::fs::read(prepared.staging.join("image.png")).unwrap(), crate::imaging::encode_png(&image).unwrap());
        let entry = prepared.publish().unwrap();
        assert!(li_fs::syncs_on_this_thread() >= before + 4);
        assert_eq!(library.load(&entry).unwrap(), image);
        assert_eq!(library.list().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn prepared_entries_are_invisible_until_publish_and_drop_cleans_them() {
        let root = std::env::temp_dir().join(format!("li-prepared-{}", uuid::Uuid::new_v4()));
        let lib = Library::open(&root);
        let img = RgbaImage::from_pixel(8, 8, image::Rgba([12, 34, 56, 128]));
        let pending = lib.prepare(&img, "cancelled", None, None).unwrap();
        assert!(lib.list().is_empty());
        assert!(pending.staging.join("image.png").is_file());
        drop(pending);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        let entry = lib.prepare(&img, "accepted", None, None).unwrap().publish().unwrap();
        assert_eq!(lib.list().len(), 1);
        assert_eq!(lib.load(&entry).unwrap(), img);
        let mut pending = lib.prepare(&img, "failed", None, None).unwrap();
        let staging = pending.staging.clone();
        // A failed publication leaves the existing entry and its bytes intact.
        pending.destination = root.join(&entry.id);
        assert!(pending.publish().is_err());
        assert!(!staging.exists());
        assert_eq!(lib.list().len(), 1);
        assert_eq!(lib.load(&entry).unwrap(), img);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn add_list_load_delete() {
        let root = std::env::temp_dir().join(format!("li-lib-{}", uuid::Uuid::new_v4()));
        let lib = Library::open(&root);
        let img = RgbaImage::from_pixel(64, 32, image::Rgba([10, 20, 30, 255]));
        let a = lib.add(&img, "first", Some(serde_json::json!({"model": "qwen", "prompt": "a fox", "seed": 42})), None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let b = lib.add(&img, "second.png", None, None).unwrap();
        let list = lib.list();
        assert_eq!(list.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec![b.id.as_str(), a.id.as_str()]);
        assert_eq!(list[1].prompt(), "a fox");
        assert_eq!(list[1].seed(), Some(42));
        assert_eq!(lib.load(&list[0]).unwrap().dimensions(), (64, 32));
        assert!(lib.thumbnail_path(&a.id).exists());
        assert_eq!(lib.delete(std::slice::from_ref(&a.id)).unwrap(), 1);
        assert_eq!(lib.list().len(), 1);
        assert!(lib.delete(&["../x".into()]).is_err());
        std::fs::remove_dir_all(root).ok();
    }
}
