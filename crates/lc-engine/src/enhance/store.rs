//! The content-addressed store of AI results beside the catalog: AI Remove patches and AI Denoise
//! results, so generated pixels are kept (re-editable, undoable) without TIFF copies of photos.
//!
//! ```text
//! <library>/Patches/remove/<key>.lip       one removal's patch (linear RGBA)
//! <library>/Patches/denoise/<key>.lip      a denoised photo at full size (linear RGB)
//! <library>/Patches/denoise/<key>-2560.lip the same, at most 2560 px long (previews)
//! ```
//!
//! Keys are 32 hex digits naming the inputs (source photo content, stroke / model, engine, seed),
//! so a key always names the same pixels. Files are written durably (an AI result can't always be
//! made again the same way). Without a library on disk (demo, tests, the web) results live in
//! memory for the session.
//!
//! `.lip` files: `LIPX`, format version, channels (3 or 4), encoding, two reserved bytes, width
//! and height (u32 LE), then zlib-deflated u16 LE samples, interleaved. Encoding 1 stores colour
//! as `√(v / 16)` (linear 0..16, fine steps in the shadows) and alpha linearly.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use lightcraft_preview::Hasher128;
use lightcraft_raster::Rgb32f;

/// What a stored result is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Remove,
    Denoise,
}

impl Kind {
    fn dir(self) -> &'static str {
        match self {
            Kind::Remove => "remove",
            Kind::Denoise => "denoise",
        }
    }
}

/// A float raster: `channels` (3 = RGB, 4 = RGB + alpha) interleaved samples per pixel.
#[derive(Clone, Debug, PartialEq)]
pub struct Raster {
    pub width: usize,
    pub height: usize,
    pub channels: usize,
    pub data: Vec<f32>,
}

impl Raster {
    pub fn bytes(&self) -> usize {
        self.data.len() * 4
    }
}

const MAGIC: &[u8; 4] = b"LIPX";
const VERSION: u8 = 1;
const SQRT16: u8 = 1;
/// Largest colour value kept (linear).
pub const MAX_LINEAR: f32 = 16.0;

/// Encode a raster as a `.lip` file.
pub fn encode(r: &Raster) -> Vec<u8> {
    encode_samples(r.width, r.height, r.channels, &r.data)
}

/// Encode `width × height` pixels of `channels` interleaved samples as a `.lip` file.
pub fn encode_samples(width: usize, height: usize, c: usize, data: &[f32]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(data.len() * 2);
    for (i, v) in data.iter().enumerate() {
        let q = if c == 4 && i % 4 == 3 { v.clamp(0.0, 1.0) } else { (v.max(0.0) / MAX_LINEAR).min(1.0).sqrt() };
        raw.extend_from_slice(&((q * 65535.0 + 0.5) as u16).to_le_bytes());
    }
    let mut out = Vec::with_capacity(raw.len() / 2 + 16);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&[VERSION, c as u8, SQRT16, 0, 0, 0]);
    out.extend_from_slice(&(width as u32).to_le_bytes());
    out.extend_from_slice(&(height as u32).to_le_bytes());
    out.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&raw, 3));
    out
}

/// Decode a `.lip` file.
pub fn decode(bytes: &[u8]) -> Result<Raster, String> {
    if bytes.len() < 18 || &bytes[..4] != MAGIC {
        return Err("not a Local Image patch file".into());
    }
    let (version, c, enc) = (bytes[4], bytes[5] as usize, bytes[6]);
    if version != VERSION || enc != SQRT16 || !(c == 3 || c == 4) {
        return Err(format!("unsupported patch file (version {version}, encoding {enc}, {c} channels)"));
    }
    let u32_at = |o: usize| u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]) as usize;
    let (w, h) = (u32_at(10), u32_at(14));
    let n = w.checked_mul(h).and_then(|p| p.checked_mul(c)).filter(|n| *n <= 1 << 31).ok_or("patch file too large")?;
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&bytes[18..], n * 2).map_err(|e| format!("damaged patch file: {e:?}"))?;
    if raw.len() != n * 2 {
        return Err("damaged patch file (short)".into());
    }
    let data = raw
        .as_chunks::<2>()
        .0
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let q = u16::from_le_bytes(*b) as f32 / 65535.0;
            if c == 4 && i % 4 == 3 { q } else { q * q * MAX_LINEAR }
        })
        .collect();
    Ok(Raster { width: w, height: h, channels: c, data })
}

/// A key from its inputs (32 hex digits).
pub fn key_of(parts: &[&str]) -> String {
    let mut h = Hasher128::new();
    h.str("local-image-ai-result-v1");
    for p in parts {
        h.str(p);
    }
    h.finish().to_string()
}

/// A key is 32 lowercase hex digits (anything else never names a file).
pub fn valid_key(key: &str) -> bool {
    (32..=40).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || b == b'-')
}

struct State {
    root: Option<PathBuf>,
    /// Results kept in memory (no library on disk), and the decoded files recently read.
    mem: HashMap<(Kind, String), Arc<Raster>>,
}

static STATE: RwLock<Option<State>> = RwLock::new(None);
/// Whether a file exists, as last seen (so the UI never stats files every frame).
static EXISTS: Mutex<Option<HashMap<(Kind, String), bool>>> = Mutex::new(None);

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = STATE.write().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(|| State { root: None, mem: HashMap::new() }))
}

/// The library's `Patches` folder (`None`: results stay in memory). Set when a library opens.
///
/// Renders read from it; writes go where the writing session's library is (see [`put`]), so
/// sessions without a library (the demo, tests) keep theirs in memory whatever is open.
pub fn set_root(dir: Option<PathBuf>) {
    with_state(|s| s.root = dir);
    *EXISTS.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

pub fn root() -> Option<PathBuf> {
    STATE.read().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|s| s.root.clone())
}

/// `<library>/Patches` for a library folder.
pub fn root_for_library(dir: &Path) -> PathBuf {
    dir.join("Patches")
}

fn path(root: &Path, kind: Kind, key: &str) -> PathBuf {
    root.join(kind.dir()).join(format!("{key}.lip"))
}

fn note_exists(kind: Kind, key: &str, yes: bool) {
    EXISTS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert((kind, key.to_owned()), yes);
}

/// Store `r` under `key`: in the `Patches` folder `to` (a library's), else in memory.
pub fn put(kind: Kind, key: &str, r: &Raster, to: Option<&Path>) -> Result<(), String> {
    put_samples(kind, key, r.width, r.height, r.channels, &r.data, to)
}

/// Store an RGB image under `key` (see [`put`]).
pub fn put_rgb(kind: Kind, key: &str, img: &Rgb32f, to: Option<&Path>) -> Result<(), String> {
    put_samples(kind, key, img.width, img.height, 3, img.data.as_flattened(), to)
}

/// The RGB image stored under `key` (an RGBA one loses its alpha).
pub fn get_rgb(kind: Kind, key: &str) -> Option<Rgb32f> {
    let r = get(kind, key)?;
    let data = r.data.chunks_exact(r.channels).map(|p| [p[0], p[1], p[2]]).collect();
    Some(Rgb32f { width: r.width, height: r.height, data })
}

fn put_samples(kind: Kind, key: &str, width: usize, height: usize, channels: usize, data: &[f32], to: Option<&Path>) -> Result<(), String> {
    if !valid_key(key) {
        return Err(format!("invalid key `{key}`"));
    }
    match to {
        #[cfg(not(target_arch = "wasm32"))]
        Some(root) => {
            let p = path(root, kind, key);
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            lightcraft_catalog::safe_file::write_atomic(&p, &encode_samples(width, height, channels, data))
                .map_err(|e| format!("{}: {e}", p.display()))?;
        }
        _ => {
            let r = Raster { width, height, channels, data: data.to_vec() };
            with_state(|s| s.mem.insert((kind, key.to_owned()), Arc::new(r)));
        }
    }
    note_exists(kind, key, true);
    Ok(())
}

/// The result stored in the `Patches` folder `root` under `key`.
pub fn get_at(kind: Kind, key: &str, root: &Path) -> Option<Raster> {
    if !valid_key(key) {
        return None;
    }
    std::fs::read(path(root, kind, key)).ok().and_then(|b| decode(&b).ok())
}

/// The result stored under `key` (in memory, or in the open library's folder), if any.
pub fn get(kind: Kind, key: &str) -> Option<Arc<Raster>> {
    if !valid_key(key) {
        return None;
    }
    if let Some(r) = STATE.read().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|s| s.mem.get(&(kind, key.to_owned())).cloned()) {
        return Some(r);
    }
    let root = root()?;
    let r = std::fs::read(path(&root, kind, key)).ok().and_then(|b| decode(&b).ok());
    note_exists(kind, key, r.is_some());
    r.map(Arc::new)
}

/// Whether `key` is stored (answered from memory after the first look).
pub fn exists(kind: Kind, key: &str) -> bool {
    if !valid_key(key) {
        return false;
    }
    if let Some(v) = EXISTS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(&(kind, key.to_owned())).copied()) {
        return v;
    }
    let yes = STATE.read().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|s| s.mem.contains_key(&(kind, key.to_owned())))
        || root().is_some_and(|r| path(&r, kind, key).is_file());
    note_exists(kind, key, yes);
    yes
}

/// Delete `key` (missing is fine).
pub fn delete(kind: Kind, key: &str) {
    if !valid_key(key) {
        return;
    }
    with_state(|s| s.mem.remove(&(kind, key.to_owned())));
    if let Some(root) = root() {
        let _ = std::fs::remove_file(path(&root, kind, key));
    }
    note_exists(kind, key, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raster(c: usize) -> Raster {
        let (w, h) = (37, 11);
        Raster {
            width: w,
            height: h,
            channels: c,
            data: (0..w * h * c).map(|i| if c == 4 && i % 4 == 3 { (i % 5) as f32 / 4.0 } else { (i % 97) as f32 * 0.013 }).collect(),
        }
    }

    #[test]
    fn files_round_trip_within_the_encoding_steps() {
        for c in [3, 4] {
            let r = raster(c);
            let back = decode(&encode(&r)).unwrap();
            assert_eq!((back.width, back.height, back.channels), (r.width, r.height, c));
            for (a, b) in r.data.iter().zip(&back.data) {
                assert!((a - b).abs() <= a.abs() * 3e-4 + 2e-5, "{a} {b}");
            }
        }
        // out of range: negatives clip to 0, colour above 16 to 16
        let r = Raster { width: 1, height: 1, channels: 3, data: vec![-1.0, 20.0, 1e-5] };
        let back = decode(&encode(&r)).unwrap();
        assert_eq!(back.data[0], 0.0);
        assert!((back.data[1] - MAX_LINEAR).abs() < 1e-3);
        assert!((back.data[2] - 1e-5).abs() < 2e-6);
        assert!(decode(b"LIPX").is_err() && decode(b"nope nope nope nope").is_err());
    }

    #[test]
    fn keys_name_their_inputs() {
        let a = key_of(&["photo", "stroke"]);
        assert_eq!(a.len(), 32);
        assert!(valid_key(&a));
        assert_ne!(a, key_of(&["photos", "troke"]));
        assert_eq!(a, key_of(&["photo", "stroke"]));
        assert!(!valid_key("../../etc/passwd") && !valid_key("ABC"));
    }

    #[test]
    fn results_are_kept_in_memory_or_in_a_library_folder() {
        let key = key_of(&["store-test", &std::process::id().to_string()]);
        assert!(!exists(Kind::Remove, &key) && get(Kind::Remove, &key).is_none());
        put(Kind::Remove, &key, &raster(4), None).unwrap();
        assert!(exists(Kind::Remove, &key));
        assert!(!exists(Kind::Denoise, &key), "kinds are separate");
        assert_eq!(get(Kind::Remove, &key).unwrap().width, 37);
        delete(Kind::Remove, &key);
        assert!(!exists(Kind::Remove, &key) && get(Kind::Remove, &key).is_none());
        // a library's folder: a file, read back as it was
        let dir = std::env::temp_dir().join(format!("lc-enhance-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let key2 = key_of(&["store-test-disk"]);
        put(Kind::Denoise, &key2, &raster(3), Some(&dir)).unwrap();
        assert!(dir.join("denoise").join(format!("{key2}.lip")).is_file());
        let back = get_at(Kind::Denoise, &key2, &dir).unwrap();
        assert_eq!((back.width, back.channels), (37, 3));
        assert!(put(Kind::Remove, "../../x", &raster(4), Some(&dir)).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
