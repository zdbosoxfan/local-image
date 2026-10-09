//! Read-only user DCP directory. Profiles are bounded, matched by their embedded camera model,
//! and kept in memory; malformed profiles never prevent the raw itself from loading.
use lightcraft_raw::{RawImage, camera_matrices::normalized, dcp::Dcp};
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock, PoisonError, RwLock},
};
#[derive(Default)]
struct Registry {
    profiles: BTreeMap<String, Arc<Dcp>>,
    key: u64,
}

fn registry() -> &'static RwLock<Registry> {
    static R: OnceLock<RwLock<Registry>> = OnceLock::new();
    R.get_or_init(Default::default)
}
pub fn cache_key() -> u64 {
    registry().read().unwrap_or_else(PoisonError::into_inner).key
}

pub fn configure(folder: &str) {
    *registry().write().unwrap_or_else(PoisonError::into_inner) = read_folder(folder);
}

fn read_folder(folder: &str) -> Registry {
    let mut profiles = BTreeMap::new();
    let mut hash = 0u64;
    if let Ok(entries) = std::fs::read_dir(folder) {
        let mut paths: Vec<_> =
            entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dcp"))).collect();
        paths.sort();
        for p in paths.into_iter().take(4096) {
            if p.metadata().is_ok_and(|m| m.is_file() && m.len() <= 32 * 1024 * 1024)
                && let Ok(b) = std::fs::read(&p)
                && let Ok(d) = Dcp::parse(&b)
                && !d.model.is_empty()
            {
                for byte in &b {
                    hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
                }
                profiles.insert(normalized(&d.model), Arc::new(d));
            }
        }
    }
    Registry { profiles, key: hash }
}
pub fn apply(raw: &mut RawImage) {
    let (make, model) = (raw.metadata.make.as_deref().unwrap_or_default(), raw.metadata.model.as_deref().unwrap_or_default());
    let reg = registry().read().unwrap_or_else(PoisonError::into_inner);
    let profile = reg
        .profiles
        .get(&normalized(model))
        .or_else(|| reg.profiles.get(&normalized(&format!("{make}{model}"))))
        .cloned()
        .or_else(|| raw.color.profile.is_empty().then(|| Dcp::bundled(make, model)).flatten().map(Arc::new));
    if let Some(d) = profile {
        raw.color.color_matrix = d.color.color_matrix;
        raw.color.illuminant = d.color.illuminant;
        raw.color.forward_matrix = d.color.forward_matrix;
        raw.color.baseline_exposure += d.color.baseline_exposure;
        raw.color.profile = d.color.profile.clone();
        // DNG DefaultBlackRender=1 means retain scene black; all our looks already retain it.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_folder_is_read_only_bounded_and_skips_invalid_profiles() {
        let dir = std::env::temp_dir().join(format!("lc-user-dcp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bytes = include_bytes!("../../lc-raw/data/dcp/NIKON_D7500.dcp");
        let file = dir.join("camera.DCP");
        std::fs::write(&file, bytes).unwrap();
        std::fs::write(dir.join("broken.dcp"), b"IIRC").unwrap();
        std::fs::write(dir.join("ignored.bin"), bytes).unwrap();
        let oversized = dir.join("oversized.dcp");
        std::fs::File::create(&oversized).unwrap().set_len(32 * 1024 * 1024 + 1).unwrap();
        let before = std::fs::metadata(&file).unwrap().modified().unwrap();
        let first = read_folder(dir.to_str().unwrap());
        assert_eq!(first.profiles.len(), 1);
        assert_eq!(first.profiles["NIKOND7500"].model, "NIKON D7500");
        assert_ne!(first.key, 0);
        assert_eq!(read_folder(dir.to_str().unwrap()).key, first.key);
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
        assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(), before);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 4);
        std::fs::remove_file(&file).unwrap();
        assert_eq!(read_folder(dir.to_str().unwrap()).key, 0);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(read_folder(dir.to_str().unwrap()).profiles.is_empty());
    }
}
