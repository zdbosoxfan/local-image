//! Embeddings belong beside the library, not in its snapshot or undo log. Each successful batch
//! atomically publishes immutable record segments; readers accept the last value for a key. Damaged caches are
//! discarded as a whole and can be recreated, with no partially trusted data escaping.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead, Read};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Store {
    dir: Option<PathBuf>,
    models: BTreeMap<String, Embeddings>,
    sharpness: BTreeMap<String, f32>,
    sharpness_pending: BTreeSet<String>,
    sharpness_reset: bool,
}

struct Embeddings {
    dim: usize,
    values: BTreeMap<String, Vec<f32>>,
    pending: BTreeSet<String>,
    /// True only after a complete validation; corrupt files must never be copied into a save.
    append: bool,
}

fn valid_model(id: &str, dim: usize) -> bool {
    !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') && (1..=16384).contains(&dim)
}

pub fn normalized(mut v: Vec<f32>, dim: usize) -> Result<Vec<f32>, String> {
    if v.len() != dim {
        return Err("embedding dimension mismatch".into());
    }
    let norm = v.iter().map(|v| v * v).sum::<f32>().sqrt();
    if !norm.is_finite() || norm <= 0.0 {
        return Err("invalid embedding".into());
    }
    for v in &mut v {
        *v /= norm;
    }
    Ok(v)
}

impl Store {
    pub fn new(library_dir: Option<&Path>) -> Self {
        let dir = library_dir.map(|d| d.join("AI"));
        let mut sharpness = dir
            .as_ref()
            .and_then(|d| std::fs::read(d.join("sharpness.json")).ok())
            .and_then(|bytes| serde_json::from_slice::<BTreeMap<String, f32>>(&bytes).ok())
            .unwrap_or_default();
        let mut sharpness_reset = false;
        if let Some(dir) = &dir {
            match read_sharpness_segments(&dir.join("sharpness.d")) {
                Ok(updates) => sharpness.extend(updates),
                Err(error) => {
                    sharpness_reset = true;
                    log::warn!("Smart Sort: ignoring damaged sharpness updates: {error}");
                }
            }
        }
        Self { dir, sharpness, sharpness_reset, ..Default::default() }
    }

    pub fn ensure(&mut self, model: &str, dim: usize) -> Result<(), String> {
        if !valid_model(model, dim) {
            return Err("invalid embedding model id or dimension".into());
        }
        if let Some(m) = self.models.get(model) {
            return if m.dim == dim { Ok(()) } else { Err("model embedding dimension changed".into()) };
        }
        let path = self.dir.as_ref().map(|d| d.join(format!("embeddings-{model}.bin")));
        let loaded = path.as_ref().map(|p| read_model(p, model, dim));
        let (values, append) = match loaded {
            Some(Ok(Some(values))) => (values, true),
            Some(Err(e)) => {
                log::warn!("Smart Sort: ignoring damaged/unreadable embedding cache for {model}: {e}");
                (BTreeMap::new(), false)
            }
            _ => (BTreeMap::new(), false),
        };
        self.models.insert(model.into(), Embeddings { dim, values, pending: BTreeSet::new(), append });
        Ok(())
    }

    pub fn get(&self, model: &str, key: &str) -> Option<&[f32]> {
        self.models.get(model)?.values.get(key).map(Vec::as_slice)
    }

    pub fn sharpness(&self, key: &str) -> Option<f32> {
        self.sharpness.get(key).copied().filter(|v| v.is_finite())
    }

    pub fn set_sharpness(&mut self, key: String, value: f32) {
        if value.is_finite() {
            self.sharpness_pending.insert(key.clone());
            self.sharpness.insert(key, value);
        }
    }

    pub fn len(&self, model: &str) -> usize {
        self.models.get(model).map_or(0, |m| m.values.len())
    }

    pub fn insert(&mut self, model: &str, key: String, v: Vec<f32>) -> Result<(), String> {
        if key.is_empty() || key.len() > u16::MAX as usize {
            return Err("invalid embedding content key".into());
        }
        let m = self.models.get_mut(model).ok_or("model cache was not loaded")?;
        let v = normalized(v, m.dim)?;
        m.values.insert(key.clone(), v);
        m.pending.insert(key);
        Ok(())
    }

    /// Capture only changed records. No filesystem work happens here; native UI hands this
    /// batch to its existing analysis coordinator and acknowledges it after durable publication.
    pub fn pending_save(&self) -> Option<SaveBatch> {
        let dir = self.dir.clone()?;
        let models: Vec<_> = self
            .models
            .iter()
            .filter(|(_, model)| !model.pending.is_empty())
            .map(|(name, model)| {
                let values = if model.append {
                    model.pending.iter().filter_map(|key| model.values.get(key).map(|value| (key.clone(), value.clone()))).collect()
                } else {
                    model.values.clone()
                };
                ModelBatch { name: name.clone(), dim: model.dim, reset: !model.append, values }
            })
            .collect();
        let sharpness = if self.sharpness_reset && !self.sharpness_pending.is_empty() {
            self.sharpness.clone()
        } else {
            self.sharpness_pending.iter().filter_map(|key| self.sharpness.get(key).map(|value| (key.clone(), *value))).collect::<BTreeMap<_, _>>()
        };
        if models.is_empty() && sharpness.is_empty() {
            None
        } else {
            Some(SaveBatch { dir, models, sharpness, sharpness_reset: self.sharpness_reset })
        }
    }

    /// A late acknowledgement never marks a newer replacement of the same key clean.
    pub fn acknowledge_save(&mut self, batch: &SaveBatch) {
        if self.dir.as_ref() != Some(&batch.dir) {
            return;
        }
        for saved in &batch.models {
            if let Some(model) = self.models.get_mut(&saved.name).filter(|model| model.dim == saved.dim) {
                for (key, value) in &saved.values {
                    if model.values.get(key) == Some(value) {
                        model.pending.remove(key);
                    }
                }
                model.append = true;
            }
        }
        for (key, value) in &batch.sharpness {
            if self.sharpness.get(key) == Some(value) {
                self.sharpness_pending.remove(key);
            }
        }
        if batch.sharpness_reset {
            self.sharpness_reset = false;
        }
    }

    /// Deleted cache baselines are rebuilt from the in-memory values on the next retry.
    pub fn save_failed(&mut self, batch: &SaveBatch, error: &io::Error) {
        if error.kind() == io::ErrorKind::NotFound && self.dir.as_ref() == Some(&batch.dir) {
            for saved in &batch.models {
                if let Some(model) = self.models.get_mut(&saved.name) {
                    model.append = false;
                }
            }
        }
    }

    /// Synchronous CLI/headless flush. Native Smart Sort uses pending_save on its coordinator.
    pub fn save(&mut self) -> io::Result<()> {
        for attempt in 0..2 {
            let Some(batch) = self.pending_save() else { return Ok(()) };
            match batch.write() {
                Ok(()) => {
                    self.acknowledge_save(&batch);
                    return Ok(());
                }
                Err(error) => {
                    self.save_failed(&batch, &error);
                    if attempt != 0 || error.kind() != io::ErrorKind::NotFound {
                        return Err(error);
                    }
                }
            }
        }
        Ok(())
    }
}

/// An immutable, path-owned write request. Dropping its originating Library does not redirect it.
pub struct SaveBatch {
    dir: PathBuf,
    models: Vec<ModelBatch>,
    sharpness: BTreeMap<String, f32>,
    sharpness_reset: bool,
}
struct ModelBatch {
    name: String,
    dim: usize,
    reset: bool,
    values: Values,
}
impl SaveBatch {
    /// Called only on a worker/coordinator (or an explicitly synchronous CLI caller).
    pub fn write(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        for model in &self.models {
            let base = self.dir.join(format!("embeddings-{}.bin", model.name));
            let segments = base.with_extension("d");
            let encode = |out: &mut dyn std::io::Write| -> io::Result<()> {
                writeln!(out, "LIEMB1 {}", serde_json::json!({"model":model.name,"dim":model.dim}))?;
                for (key, value) in &model.values {
                    out.write_all(&(key.len() as u16).to_le_bytes())?;
                    out.write_all(key.as_bytes())?;
                    for component in value {
                        out.write_all(&component.to_le_bytes())?;
                    }
                }
                Ok(())
            };
            if model.reset {
                for path in segment_paths(&segments, "bin")? {
                    std::fs::remove_file(path)?;
                }
                li_fs::write_atomic_with(&base, &mut |out| encode(out))?;
            } else {
                // Losing the baseline must rebuild from the owner, not silently lose old rows.
                let _ = std::fs::metadata(&base)?;
                publish_segment(&segments, "bin", &mut |out| encode(out))?;
            }
        }
        if !self.sharpness.is_empty() {
            if self.sharpness_reset {
                for path in segment_paths(&self.dir.join("sharpness.d"), "json")? {
                    std::fs::remove_file(path)?;
                }
            }
            let bytes = serde_json::to_vec(&self.sharpness)?;
            publish_segment(&self.dir.join("sharpness.d"), "json", &mut |out| out.write_all(&bytes))?;
        }
        Ok(())
    }
}

fn segment_paths(dir: &Path, extension: &str) -> io::Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.strip_suffix(&format!(".{extension}")).is_some_and(|stem| stem.len() == 20 && stem.bytes().all(|byte| byte.is_ascii_digit()))
            && entry.file_type()?.is_file()
        {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

/// Sequence allocation scans each directory once per process, not once per incoming result.
/// Exclusive atomic publication resolves another process racing the same sequence safely.
fn publish_segment(dir: &Path, extension: &str, encode: &mut dyn FnMut(&mut dyn std::io::Write) -> io::Result<()>) -> io::Result<()> {
    use std::sync::{Mutex, OnceLock, PoisonError};
    static NEXT: OnceLock<Mutex<BTreeMap<PathBuf, u64>>> = OnceLock::new();
    std::fs::create_dir_all(dir)?;
    loop {
        let sequence = {
            let mut next = NEXT.get_or_init(Mutex::default).lock().unwrap_or_else(PoisonError::into_inner);
            if !next.contains_key(dir) {
                // Retain a small number of recently encountered library paths. An evicted
                // directory simply rescans its sequence on next use; no pixel data lives here.
                if next.len() >= 128 {
                    next.pop_first();
                }
                let last = segment_paths(dir, extension)?
                    .last()
                    .and_then(|path| path.file_stem())
                    .and_then(|stem| stem.to_str())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
                next.insert(dir.to_path_buf(), last);
            }
            let value = next.get_mut(dir).ok_or_else(|| io::Error::other("Missing cache sequence"))?;
            *value = value.checked_add(1).ok_or_else(|| io::Error::other("Cache segment sequence exhausted"))?;
            *value
        };
        let path = dir.join(format!("{sequence:020}.{extension}"));
        match li_fs::write_new_with(&path, encode, &mut |_| Ok(())) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

fn read_model(path: &Path, model: &str, dim: usize) -> io::Result<Option<Values>> {
    let Some(mut values) = read(path, model, dim)? else { return Ok(None) };
    for segment in segment_paths(&path.with_extension("d"), "bin")? {
        let rows = read(&segment, model, dim)?.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Missing embedding segment"))?;
        values.extend(rows);
    }
    Ok(Some(values))
}
fn read_sharpness_segments(dir: &Path) -> io::Result<BTreeMap<String, f32>> {
    let mut values = BTreeMap::new();
    for path in segment_paths(dir, "json")? {
        let bytes = li_fs::read::file(&path, 64 << 20, &lightcraft_sysmem::jobs::Interrupt::NONE)?;
        let rows: BTreeMap<String, f32> = serde_json::from_slice(&bytes)?;
        if rows.iter().any(|(key, value)| key.is_empty() || !value.is_finite()) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid sharpness segment"));
        }
        values.extend(rows);
    }
    Ok(values)
}

type Values = BTreeMap<String, Vec<f32>>;
fn read(path: &Path, model: &str, dim: usize) -> io::Result<Option<Values>> {
    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid or truncated embedding file");
    let mut reader = io::BufReader::new(f);
    let mut header = String::new();
    Read::by_ref(&mut reader).take(4096).read_line(&mut header)?;
    if !header.ends_with('\n') {
        return Err(invalid());
    }
    let value: serde_json::Value = serde_json::from_str(header.strip_prefix("LIEMB1 ").ok_or_else(invalid)?).map_err(|_| invalid())?;
    if value["model"].as_str() != Some(model) || value["dim"].as_u64() != Some(dim as u64) {
        return Err(invalid());
    }
    let mut values = Values::new();
    loop {
        let mut len = [0u8; 2];
        if reader.read(&mut len[..1])? == 0 {
            break;
        }
        reader.read_exact(&mut len[1..])?;
        let n = u16::from_le_bytes(len) as usize;
        if n == 0 {
            return Err(invalid());
        }
        let mut key = vec![0; n];
        reader.read_exact(&mut key)?;
        let key = String::from_utf8(key).map_err(|_| invalid())?;
        let mut v = Vec::with_capacity(dim);
        for _ in 0..dim {
            let mut b = [0; 4];
            reader.read_exact(&mut b)?;
            v.push(f32::from_le_bytes(b));
        }
        // Reject invalid values, retaining exact finite bytes rather than normalising on reload.
        let norm = v.iter().map(|v| v * v).sum::<f32>();
        if v.iter().any(|v| !v.is_finite()) || !norm.is_finite() || norm <= 0.0 {
            return Err(invalid());
        }
        values.insert(key, v);
    }
    Ok(Some(values))
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    fn directory() -> PathBuf {
        std::env::temp_dir().join(format!("li-smart-store-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn incremental_segments_keep_the_base_unchanged_and_survive_reopen() {
        let dir = directory();
        let mut store = Store::new(Some(&dir));
        store.ensure("test", 512).unwrap();
        store.insert("test", "first".into(), vec![1.0; 512]).unwrap();
        store.save().unwrap();
        let base = std::fs::read(dir.join("AI/embeddings-test.bin")).unwrap();
        for n in 0..32 {
            let key = format!("item-{n}");
            store.insert("test", key.clone(), vec![1.0; 512]).unwrap();
            store.set_sharpness(key, n as f32);
            store.save().unwrap();
            assert_eq!(std::fs::read(dir.join("AI/embeddings-test.bin")).unwrap(), base, "no previous embeddings copied on append");
        }
        let segments = segment_paths(&dir.join("AI/embeddings-test.d"), "bin").unwrap();
        let total = segments.iter().map(|path| std::fs::metadata(path).unwrap().len()).sum::<u64>() + base.len() as u64;
        assert!(total < 33 * 512 * 4 * 2, "storage remains proportional to new records, including headers");
        let mut loaded = Store::new(Some(&dir));
        loaded.ensure("test", 512).unwrap();
        assert_eq!(loaded.len("test"), 33);
        assert_eq!(loaded.sharpness("item-31"), Some(31.0));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_batches_retry_and_late_acknowledgements_preserve_newer_changes() {
        let dir = directory();
        let mut store = Store::new(Some(&dir));
        store.ensure("test", 2).unwrap();
        store.insert("test", "key".into(), vec![1.0, 0.0]).unwrap();
        store.save().unwrap();
        store.insert("test", "key".into(), vec![0.0, 1.0]).unwrap();
        let batch = store.pending_save().unwrap();
        {
            let _fault = li_fs::fail_writes_after(4);
            assert!(batch.write().is_err());
        }
        assert!(store.pending_save().is_some());
        batch.write().unwrap();
        store.insert("test", "key".into(), vec![1.0, 1.0]).unwrap();
        store.acknowledge_save(&batch);
        assert!(store.pending_save().is_some(), "older persisted value cannot clean the newer pending one");
        store.save().unwrap();
        let mut loaded = Store::new(Some(&dir));
        loaded.ensure("test", 2).unwrap();
        assert_eq!(loaded.get("test", "key"), store.get("test", "key"));
        assert!(store.pending_save().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
