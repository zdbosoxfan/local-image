use lightcraft_engine::devices::{Device, DeviceWatch, devices_in};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// helpers

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("lc-devices-it-{}-{}-{}", std::process::id(), tag, id));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn card(name: &str) -> Device {
    Device { name: name.to_string(), path: format!("/v/{name}/DCIM"), root: format!("/v/{name}") }
}

fn make_volume(base: &Path, name: &str, dcim: &str) {
    std::fs::create_dir_all(base.join(name).join(dcim)).unwrap();
}

// Device and devices_in

#[test]
fn devices_in_extra_roots_are_scanned_without_parents() {
    let base = TempDir::new("extra");
    make_volume(&base.0, "CARD", "DCIM");

    let found = devices_in(&[], vec![base.0.join("CARD")]);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "CARD");
    assert_eq!(found[0].path, base.0.join("CARD/DCIM").to_string_lossy());
    assert_eq!(found[0].root, base.0.join("CARD").to_string_lossy());
}

#[test]
fn devices_in_dedupes_by_root() {
    let base = TempDir::new("dedupe");
    make_volume(&base.0, "CARD", "DCIM");
    let root = base.0.join("CARD");

    let found = devices_in(std::slice::from_ref(&base.0), vec![root.clone()]);

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].root, root.to_string_lossy());
}

#[test]
fn devices_in_missing_or_empty_parents_return_empty() {
    assert!(devices_in(&[], Vec::new()).is_empty());

    let missing = std::env::temp_dir().join(format!("lc-devices-no-such-dir-{}-{}", std::process::id(), NEXT_ID.fetch_add(1, Ordering::Relaxed)));
    assert!(devices_in(std::slice::from_ref(&missing), Vec::new()).is_empty());
}

#[cfg(unix)]
#[test]
fn devices_in_skips_symlink_children() {
    let parent = TempDir::new("symlink-parent");
    let real = TempDir::new("symlink-real");
    make_volume(&real.0, "RealCard", "DCIM");
    std::os::unix::fs::symlink(real.0.join("RealCard"), parent.0.join("AliasCard")).unwrap();

    let found = devices_in(std::slice::from_ref(&parent.0), Vec::new());

    assert!(found.is_empty(), "symlink volume should be skipped: {found:?}");
}

// DeviceWatch

#[test]
fn device_watch_scan_now_is_synchronous_and_served_by_get() {
    let list = Arc::new(Mutex::new(vec![card("A")]));
    let list_flag = list.clone();
    let runs = Arc::new(AtomicUsize::new(0));
    let runs_flag = runs.clone();

    let watch = DeviceWatch::new(
        move || {
            runs_flag.fetch_add(1, Ordering::SeqCst);
            list_flag.lock().unwrap().clone()
        },
        Duration::from_secs(3600),
    );

    assert_eq!(watch.scan_now(), vec![card("A")]);
    assert_eq!(runs.load(Ordering::SeqCst), 1);

    // Fresh scan is served without running the scanner again.
    assert_eq!(watch.get(), vec![card("A")]);
    assert!(!watch.refreshing());
    assert_eq!(runs.load(Ordering::SeqCst), 1);

    *list.lock().unwrap() = vec![card("B")];
    assert_eq!(watch.scan_now(), vec![card("B")]);
    assert_eq!(runs.load(Ordering::SeqCst), 2);
}

#[test]
fn device_watch_on_change_replaces_previous_callback() {
    let list = Arc::new(Mutex::new(Vec::<Device>::new()));
    let list_flag = list.clone();
    let watch = DeviceWatch::new(move || list_flag.lock().unwrap().clone(), Duration::from_secs(3600));

    let first = Arc::new(AtomicUsize::new(0));
    let second = Arc::new(AtomicUsize::new(0));

    let first_flag = first.clone();
    watch.on_change(move || {
        first_flag.fetch_add(1, Ordering::SeqCst);
    });
    let second_flag = second.clone();
    watch.on_change(move || {
        second_flag.fetch_add(1, Ordering::SeqCst);
    });

    *list.lock().unwrap() = vec![card("X")];
    watch.scan_now();

    assert_eq!(first.load(Ordering::SeqCst), 0, "the first callback should have been replaced");
    assert_eq!(second.load(Ordering::SeqCst), 1);
}

#[test]
fn device_watch_callback_can_call_get_without_deadlock() {
    let list = Arc::new(Mutex::new(Vec::<Device>::new()));
    let list_flag = list.clone();
    let watch = DeviceWatch::new(move || list_flag.lock().unwrap().clone(), Duration::from_secs(3600));

    let seen = Arc::new(AtomicUsize::new(0));
    let watch_flag = watch.clone();
    let seen_flag = seen.clone();
    watch.on_change(move || {
        seen_flag.store(watch_flag.get().len(), Ordering::SeqCst);
    });

    *list.lock().unwrap() = vec![card("A"), card("B")];
    watch.scan_now();

    assert_eq!(seen.load(Ordering::SeqCst), 2);
}
