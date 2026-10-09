//! Read-only user DCP directory. Profiles are bounded, matched by their embedded camera model,
//! and kept in memory; malformed profiles never prevent the raw itself from loading.
use std::{collections::BTreeMap, sync::{OnceLock,RwLock,PoisonError,Arc}};
use lightcraft_raw::{dcp::Dcp,RawImage,camera_matrices::normalized};
fn registry() -> &'static RwLock<BTreeMap<String,Arc<Dcp>>> {
    static R: OnceLock<RwLock<BTreeMap<String,Arc<Dcp>>>>=OnceLock::new(); R.get_or_init(Default::default)
}
static KEY: std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
pub fn cache_key() -> u64 { KEY.load(std::sync::atomic::Ordering::Relaxed) }

pub fn configure(folder: &str) {
    let mut profiles=BTreeMap::new();
    let mut hash=0u64;
    if let Ok(entries)=std::fs::read_dir(folder) {
        let mut paths:Vec<_>=entries.flatten().map(|e|e.path()).filter(|p|p.extension().is_some_and(|e|e.eq_ignore_ascii_case("dcp"))).collect(); paths.sort();
        for p in paths.into_iter().take(4096) {
            if p.metadata().is_ok_and(|m|m.is_file() && m.len()<=32*1024*1024) && let Ok(b)=std::fs::read(&p) && let Ok(d)=Dcp::parse(&b) && !d.model.is_empty() {
                for byte in &b { hash=(hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3); }
                profiles.insert(normalized(&d.model),Arc::new(d));
            }
        }
    }
    KEY.store(hash,std::sync::atomic::Ordering::Relaxed);
    *registry().write().unwrap_or_else(PoisonError::into_inner)=profiles;
}
pub fn apply(raw: &mut RawImage) {
    let (make,model)=(raw.metadata.make.as_deref().unwrap_or_default(),raw.metadata.model.as_deref().unwrap_or_default());
    let reg=registry().read().unwrap_or_else(PoisonError::into_inner);
    let profile=reg.get(&normalized(model)).or_else(||reg.get(&normalized(&format!("{make}{model}")))).cloned().or_else(||raw.color.profile.is_empty().then(||Dcp::bundled(make,model)).flatten().map(Arc::new));
    if let Some(d)=profile {
        raw.color.color_matrix=d.color.color_matrix; raw.color.illuminant=d.color.illuminant;
        raw.color.forward_matrix=d.color.forward_matrix;
        raw.color.baseline_exposure+=d.color.baseline_exposure;
        raw.color.profile=d.color.profile.clone();
        // DNG DefaultBlackRender=1 means retain scene black; all our looks already retain it.
    }
}
