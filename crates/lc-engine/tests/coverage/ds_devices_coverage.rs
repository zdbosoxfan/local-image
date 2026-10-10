use lightcraft_engine::devices::{Device, DeviceWatch, devices, devices_in, devices_now, on_change};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

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

fn until(mut f: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}

// ---------------------------------------------------------------------------
// Device and devices_in
// ---------------------------------------------------------------------------

#[test]
fn device_serializes_to_json() {
    let d = Device { name: "EOS".into(), path: "/Volumes/EOS/DCIM".into(), root: "/Volumes/EOS".into() };

    let v = serde_json::to_value(&d).unwrap();

    assert_eq!(v["name"], "EOS");
    assert_eq!(v["path"], "/Volumes/EOS/DCIM");
    assert_eq!(v["root"], "/Volumes/EOS");
}

#[test]
fn devices_in_finds_dcim_volumes_and_sorts_by_name() {
    let base = TempDir::new("find");
    make_volume(&base.0, "EOS_DIGITAL", "DCIM");
    make_volume(&base.0, "SD", "dcim");
    std::fs::create_dir_all(base.0.join("Backup/Photos")).unwrap();

    let found = devices_in(std::slice::from_ref(&base.0), Vec::new());
    let names: Vec<_> = found.iter().map(|d| d.name.as_str()).collect();

    assert_eq!(names, ["EOS_DIGITAL", "SD"], "{found:?}");

    let eos = &found[0];
    assert_eq!(eos.root, base.0.join("EOS_DIGITAL").to_string_lossy());
    assert_eq!(eos.path, base.0.join("EOS_DIGITAL/DCIM").to_string_lossy());
    assert!(Path::new(&eos.path).is_dir());
    assert!(Path::new(&eos.root).is_dir());

    let sd = &found[1];
    assert!(sd.path.ends_with("dcim"), "{sd:?}");
}

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

#[cfg(not(windows))]
#[test]
fn devices_in_finds_case_insensitive_dcim_on_unix() {
    let base = TempDir::new("case");
    std::fs::create_dir_all(base.0.join("Card/Dcim/100CANON")).unwrap();

    let found = devices_in(std::slice::from_ref(&base.0), Vec::new());

    assert_eq!(found.len(), 1, "{found:?}");
    let p = Path::new(&found[0].path);
    assert_eq!(p.file_name().unwrap().to_string_lossy(), "Dcim");
    assert!(p.is_dir());
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

// ---------------------------------------------------------------------------
// DeviceWatch
// ---------------------------------------------------------------------------

#[test]
fn device_watch_cold_get_is_immediate_and_background_scan_lands() {
    let started = Arc::new(AtomicBool::new(false));
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let release_rx = Arc::new(Mutex::new(release_rx));

    let started_flag = started.clone();
    let release_flag = release_rx.clone();
    let watch = DeviceWatch::new(
        move || {
            started_flag.store(true, Ordering::SeqCst);
            let _ = release_flag.lock().unwrap().recv_timeout(Duration::from_secs(5));
            vec![card("CAMERA")]
        },
        Duration::from_millis(20),
    );

    let t = Instant::now();
    let first = watch.get();
    assert!(first.is_empty());
    assert!(t.elapsed() < Duration::from_millis(500), "get() waited for the scanner: {:?}", t.elapsed());
    assert!(watch.refreshing());

    assert!(until(|| started.load(Ordering::SeqCst)), "scanner never started");
    assert!(watch.refreshing(), "scanner should still be refreshing before release");

    release_tx.send(()).unwrap();
    let landed = until(|| !watch.refreshing() && watch.get() == vec![card("CAMERA")]);
    assert!(landed, "background scan did not update the served list");
}

#[test]
fn device_watch_coalesces_requests_while_refreshing() {
    let runs = Arc::new(AtomicUsize::new(0));
    let started = Arc::new(AtomicBool::new(false));
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let release_rx = Arc::new(Mutex::new(release_rx));

    let runs_flag = runs.clone();
    let started_flag = started.clone();
    let release_flag = release_rx.clone();
    let watch = DeviceWatch::new(
        move || {
            runs_flag.fetch_add(1, Ordering::SeqCst);
            started_flag.store(true, Ordering::SeqCst);
            let _ = release_flag.lock().unwrap().recv_timeout(Duration::from_secs(5));
            vec![card("C")]
        },
        Duration::from_millis(10),
    );

    watch.get();
    assert!(until(|| started.load(Ordering::SeqCst)), "scanner never started");

    for _ in 0..20 {
        assert!(watch.get().is_empty(), "stale list should be served while rescanning");
    }
    assert_eq!(runs.load(Ordering::SeqCst), 1, "requests while refreshing were not coalesced");

    release_tx.send(()).unwrap();
    let landed = until(|| !watch.refreshing() && watch.get() == vec![card("C")]);
    assert!(landed, "background rescan did not land");
    assert_eq!(runs.load(Ordering::SeqCst), 1);
}

#[test]
fn device_watch_notifies_only_when_list_changes() {
    let list = Arc::new(Mutex::new(Vec::<Device>::new()));
    let list_flag = list.clone();
    let watch = DeviceWatch::new(move || list_flag.lock().unwrap().clone(), Duration::from_millis(20));

    let changes = Arc::new(AtomicUsize::new(0));
    let changes_flag = changes.clone();
    watch.on_change(move || {
        changes_flag.fetch_add(1, Ordering::SeqCst);
    });

    // Initial list is already empty: no change, no notification.
    assert!(watch.scan_now().is_empty());
    assert_eq!(changes.load(Ordering::SeqCst), 0);

    // New card appears: should notify exactly once.
    *list.lock().unwrap() = vec![card("EOS")];
    assert_eq!(watch.scan_now(), vec![card("EOS")]);
    assert_eq!(changes.load(Ordering::SeqCst), 1);

    // Same list again: no notification.
    assert_eq!(watch.scan_now(), vec![card("EOS")]);
    assert_eq!(changes.load(Ordering::SeqCst), 1);
}

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

// ---------------------------------------------------------------------------
// global devices API
// ---------------------------------------------------------------------------

#[test]
fn global_devices_are_available_and_return_a_list() {
    // Smoke tests for the global API. These functions are thin wrappers around a
    // process‑global DeviceWatch; we can't influence its environment without
    // unsafe code, so we only pin the behaviour that is guaranteed regardless of
    // the host filesystem: they return a valid Vec and don't panic.
    let now = devices_now();
    assert!(now.iter().all(|d| !d.path.is_empty() && !d.root.is_empty()));

    // After a synchronous scan, devices() serves at least that scanned list
    // (background rescans may later replace it, so we don't assert equality).
    let cached = devices();
    assert!(cached.iter().all(|d| !d.path.is_empty() && !d.root.is_empty()));

    // The callback can be set without panic (the notification itself depends on
    // the scanner result).
    on_change(|| {});
}
