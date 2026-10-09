//! Sensitive face data stays in the library's AI directory. Atomic append batches leave the
//! previous file intact on failure; a malformed file is discarded completely before a rebuild.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead, Read};
use std::path::{Path, PathBuf};

pub const DIM: usize = 128;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoredFace {
    /// Normalised full, upright frame: x, y, width, height.
    pub rect: [f32; 4],
    pub score: f32,
    /// Never returned by commands, logged or copied into catalog/XMP/export metadata.
    pub embedding: Vec<f32>,
}

impl StoredFace {
    pub fn validate(&self) -> Result<(), String> {
        let [x, y, w, h] = self.rect;
        if !self.rect.iter().all(|v| v.is_finite())
            || x < 0.0
            || y < 0.0
            || w <= 0.0
            || h <= 0.0
            || x + w > 1.00001
            || y + h > 1.00001
            || !self.score.is_finite()
            || !(0.0..=1.0).contains(&self.score)
        {
            return Err("invalid face rectangle or score".into());
        }
        super::store::normalized(self.embedding.clone(), DIM)?;
        Ok(())
    }
    pub fn bounds(&self) -> lightcraft_meta::Rect {
        let [x, y, w, h] = self.rect.map(f64::from);
        lightcraft_meta::Rect::from_xywh(x, y, w, h)
    }
}

#[derive(Default)]
pub struct FacesStore {
    dir: Option<PathBuf>,
    models: BTreeMap<String, Records>,
}
#[derive(Default)]
struct Records {
    values: BTreeMap<String, Vec<StoredFace>>,
    pending: BTreeSet<String>,
    append: bool,
}

impl FacesStore {
    pub fn new(library: Option<&Path>) -> Self {
        Self { dir: library.map(|p| p.join("AI")), ..Default::default() }
    }
    pub fn ensure(&mut self, model: &str) -> Result<(), String> {
        if model.is_empty() || model.len() > 128 || !model.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
            return Err("invalid face model id".into());
        }
        if self.models.contains_key(model) {
            return Ok(());
        }
        let loaded = self.dir.as_ref().map(|p| read(&p.join(format!("faces-{model}.bin")), model));
        let (values, append) = match loaded {
            Some(Ok(Some(values))) => (values, true),
            Some(Err(_)) => {
                // Neither paths, names nor face data belong in diagnostics.
                log::warn!("Smart Sort: ignoring damaged or unreadable face cache");
                (BTreeMap::new(), false)
            }
            _ => (BTreeMap::new(), false),
        };
        self.models.insert(model.into(), Records { values, append, ..Default::default() });
        Ok(())
    }
    pub fn get(&self, model: &str, key: &str) -> Option<&[StoredFace]> {
        self.models.get(model)?.values.get(key).map(Vec::as_slice)
    }
    pub fn entries(&self, model: &str) -> impl Iterator<Item = (&String, &Vec<StoredFace>)> {
        self.models.get(model).into_iter().flat_map(|m| m.values.iter())
    }
    pub fn len(&self, model: &str) -> usize {
        self.models.get(model).map_or(0, |m| m.values.len())
    }
    pub fn insert(&mut self, model: &str, key: String, mut faces: Vec<StoredFace>) -> Result<(), String> {
        if key.is_empty() || key.len() > u16::MAX as usize || faces.len() > u8::MAX as usize {
            return Err("invalid face record size".into());
        }
        for face in &mut faces {
            face.validate()?;
            face.embedding = super::store::normalized(face.embedding.clone(), DIM)?;
        }
        let m = self.models.get_mut(model).ok_or("face model cache was not loaded")?;
        m.values.insert(key.clone(), faces);
        m.pending.insert(key);
        Ok(())
    }
    pub fn save(&mut self) -> io::Result<()> {
        let Some(dir) = &self.dir else {
            return Ok(());
        };
        if self.models.values().all(|m| m.pending.is_empty()) {
            return Ok(());
        }
        std::fs::create_dir_all(dir)?;
        for (model, m) in &mut self.models {
            if m.pending.is_empty() {
                continue;
            }
            let path = dir.join(format!("faces-{model}.bin"));
            lightcraft_catalog::safe_file::write_atomic_with(&path, &mut |w| {
                if m.append {
                    std::io::copy(&mut std::fs::File::open(&path)?, w)?;
                } else {
                    writeln!(w, "LIFACE1 {}", serde_json::json!({"model":model,"dim":DIM}))?;
                }
                for key in &m.pending {
                    w.write_all(&(key.len() as u16).to_le_bytes())?;
                    w.write_all(key.as_bytes())?;
                    w.write_all(&[m.values[key].len() as u8])?;
                    for face in &m.values[key] {
                        for v in face.rect.iter().chain(std::iter::once(&face.score)).chain(&face.embedding) {
                            w.write_all(&v.to_le_bytes())?;
                        }
                    }
                }
                Ok(())
            })?;
            m.pending.clear();
            m.append = true;
        }
        Ok(())
    }
    /// Removes all model versions, plus interrupted atomic-write scratch files. Unrelated AI
    /// embeddings remain. In-memory state is dropped only after deletion succeeds.
    pub fn clear(&mut self) -> io::Result<()> {
        if let Some(dir) = &self.dir {
            match std::fs::read_dir(dir) {
                Ok(entries) => {
                    for entry in entries {
                        let entry = entry?;
                        let name = entry.file_name();
                        let name = name.to_string_lossy();
                        if (name.starts_with("faces-") && (name.ends_with(".bin") || name.contains(".bin.")))
                            || (name.starts_with(".lightcraft-") && name.ends_with(".tmp"))
                        {
                            std::fs::remove_file(entry.path())?;
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        self.models.clear();
        Ok(())
    }
}

type Values = BTreeMap<String, Vec<StoredFace>>;
fn read(path: &Path, model: &str) -> io::Result<Option<Values>> {
    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid face cache");
    let mut r = io::BufReader::new(f);
    let mut header = String::new();
    Read::by_ref(&mut r).take(4096).read_line(&mut header)?;
    if !header.ends_with('\n') {
        return Err(invalid());
    }
    let v: serde_json::Value = serde_json::from_str(header.strip_prefix("LIFACE1 ").ok_or_else(invalid)?).map_err(|_| invalid())?;
    if v["model"].as_str() != Some(model) || v["dim"].as_u64() != Some(DIM as u64) {
        return Err(invalid());
    }
    let mut values = Values::new();
    loop {
        let mut len = [0; 2];
        if r.read(&mut len[..1])? == 0 {
            break;
        }
        r.read_exact(&mut len[1..])?;
        let n = u16::from_le_bytes(len) as usize;
        if n == 0 {
            return Err(invalid());
        }
        let mut key = vec![0; n];
        r.read_exact(&mut key)?;
        let key = String::from_utf8(key).map_err(|_| invalid())?;
        let mut count = [0];
        r.read_exact(&mut count)?;
        let mut faces = Vec::new();
        for _ in 0..count[0] {
            let mut floats = [0f32; 5 + DIM];
            for v in &mut floats {
                let mut b = [0; 4];
                r.read_exact(&mut b)?;
                *v = f32::from_le_bytes(b);
            }
            let face = StoredFace { rect: [floats[0], floats[1], floats[2], floats[3]], score: floats[4], embedding: floats[5..].to_vec() };
            face.validate().map_err(|_| invalid())?;
            faces.push(face);
        }
        values.insert(key, faces);
    }
    Ok(Some(values))
}
