use lightcraft_preview::{DiskCache, Hasher128};
use lightcraft_raster::Rgba8;
use std::sync::Arc;
struct Scratch(std::path::PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn concurrent_rewrites_charge_only_the_current_file_and_preserve_cache_hits() {
    let root = Scratch(std::env::temp_dir().join(format!("lc-cache-rewrite-{}", std::process::id())));
    let image = Rgba8::from_fn(64, 48, |x, y| [(x * 3) as u8, (y * 5) as u8, 80, 255]);
    let encoded = lightcraft_preview::encode_jpeg(&image).unwrap();
    let cache = Arc::new(DiskCache::new(&root.0, encoded.len() as u64 * 2));
    let key = Hasher128::new().str("same-photo").finish();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                for _ in 0..20 {
                    cache.put(key, &image);
                }
            });
        }
    });
    assert_eq!(cache.size(), encoded.len() as u64);
    assert!(cache.get(key).is_some());
    let files: u64 = std::fs::read_dir(&root.0)
        .unwrap()
        .flatten()
        .flat_map(|d| std::fs::read_dir(d.path()).unwrap().flatten())
        .map(|e| e.metadata().unwrap().len())
        .sum();
    assert_eq!(cache.size(), files);
    cache.prune();
    assert!(cache.get(key).is_some());
    cache.clear();
    assert_eq!(cache.size(), 0);
}
