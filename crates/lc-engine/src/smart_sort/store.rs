//! Embeddings belong beside the library, not in its snapshot or undo log. Each successful batch
//! atomically appends records; readers accept the last value for a key. Damaged caches are
//! discarded as a whole and can be recreated, with no partially trusted data escaping.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead, Read};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Store {
    dir: Option<PathBuf>,
    models: BTreeMap<String, Embeddings>,
    sharpness: BTreeMap<String, f32>,
    sharpness_dirty: bool,
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
        let sharpness = dir
            .as_ref()
            .and_then(|d| std::fs::read(d.join("sharpness.json")).ok())
            .and_then(|bytes| serde_json::from_slice::<BTreeMap<String, f32>>(&bytes).ok())
            .unwrap_or_default();
        Self { dir, sharpness, ..Default::default() }
    }

    pub fn ensure(&mut self, model: &str, dim: usize) -> Result<(), String> {
        if !valid_model(model, dim) {
            return Err("invalid embedding model id or dimension".into());
        }
        if let Some(m) = self.models.get(model) {
            return if m.dim == dim { Ok(()) } else { Err("model embedding dimension changed".into()) };
        }
        let path = self.dir.as_ref().map(|d| d.join(format!("embeddings-{model}.bin")));
        let loaded = path.as_ref().map(|p| read(p, model, dim));
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
            self.sharpness.insert(key, value);
            self.sharpness_dirty = true;
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

    /// Memory-only sessions never write to disk. Failed writes leave the pending batch intact.
    pub fn save(&mut self) -> io::Result<()> {
        let Some(dir) = &self.dir else {
            return Ok(());
        };
        if !self.sharpness_dirty && self.models.values().all(|m| m.pending.is_empty()) {
            return Ok(());
        }
        std::fs::create_dir_all(dir)?;
        if self.sharpness_dirty {
            let bytes = serde_json::to_vec(&self.sharpness)?;
            lightcraft_catalog::safe_file::write_atomic(&dir.join("sharpness.json"), &bytes)?;
            self.sharpness_dirty = false;
        }
        for (model, m) in &mut self.models {
            if m.pending.is_empty() {
                continue;
            }
            let path = dir.join(format!("embeddings-{model}.bin"));
            lightcraft_catalog::safe_file::write_atomic_with(&path, &mut |w| {
                if m.append {
                    let mut original = std::fs::File::open(&path)?;
                    std::io::copy(&mut original, w)?;
                } else {
                    writeln!(w, "LIEMB1 {}", serde_json::json!({"model":model,"dim":m.dim}))?;
                }
                for key in &m.pending {
                    w.write_all(&(key.len() as u16).to_le_bytes())?;
                    w.write_all(key.as_bytes())?;
                    for v in &m.values[key] {
                        w.write_all(&v.to_le_bytes())?;
                    }
                }
                Ok(())
            })?;
            m.pending.clear();
            m.append = true;
        }
        Ok(())
    }
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
