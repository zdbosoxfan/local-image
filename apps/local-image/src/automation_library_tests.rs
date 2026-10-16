use super::*;

fn finish_scan(app: &mut lightcraft_ui_egui::LightcraftApp) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let ctx = egui::Context::default();
    while app.scan.is_some() && std::time::Instant::now() < deadline {
        lightcraft_ui_egui::import::poll_scan(app, &ctx);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(app.scan.is_none(), "scoped scan did not finish");
    assert!(app.import.is_none(), "scoped metadata must not enter ambient import processing");
}

fn fixture() -> (PathBuf, PathBuf, AuthorizedWorkspace) {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let base = std::env::temp_dir().join(format!("scoped-library-{}-{nonce}", std::process::id()));
    let root = base.join("inside");
    let outside = base.join("outside");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([120, 30, 80, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    std::fs::write(root.join("photo.png"), bytes.get_ref()).unwrap();
    std::fs::write(outside.join("private.png"), bytes.get_ref()).unwrap();
    let workspace = AuthorizedWorkspace::new(Some(&root), None).unwrap();
    (root, outside, workspace)
}

#[test]
fn capability_browse_preserves_in_root_photos_and_reuses_hooks() {
    let (root, outside, workspace) = fixture();
    assert!(stage(&AuthorizedWorkspace::default(), ".").is_err());
    for invalid in ["../outside", outside.to_str().unwrap(), "inside/../outside"] {
        assert!(stage(&workspace, invalid).is_err());
    }
    let browse = stage(&workspace, ".").unwrap();
    assert_eq!(browse.files.len(), 1);
    let mut app = lightcraft_ui_egui::LightcraftApp::new(lightcraft_engine::Session::new().with_fs(), Default::default());
    let preview_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let calls = preview_calls.clone();
    let native_preview = app.session.media.preview_loader.clone().unwrap();
    app.session.media.preview_loader = Some(Arc::new(move |path, edge| {
        calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        native_preview(path, edge)
    }));
    let mut hooks = None;
    show(&mut app, browse, &mut hooks).unwrap();
    finish_scan(&mut app);
    assert_eq!(app.session.visible().len(), 1);
    let first = app.session.media.file_loader.clone().unwrap();
    show(&mut app, stage(&workspace, ".").unwrap(), &mut hooks).unwrap();
    finish_scan(&mut app);
    assert!(Arc::ptr_eq(&first, app.session.media.file_loader.as_ref().unwrap()));
    assert_eq!(app.session.catalog.photos().count(), 1);
    assert!(app.session.media.file_loader.as_ref().unwrap()(root.join("photo.png").to_str().unwrap(), 16).is_ok());
    // Ordinary native reads elsewhere remain available to the human Library UI.
    let outside_path = outside.join("private.png");
    let outside_path = outside_path.to_str().unwrap();
    assert!(app.session.media.file_loader.as_ref().unwrap()(outside_path, 16).is_ok());
    assert!(app.session.media.file_loader_with.as_ref().unwrap()(outside_path, 16, &Default::default()).is_ok());
    assert!(app.session.media.file_probe.as_ref().unwrap()(outside_path).is_ok());
    assert_eq!(app.session.media.file_bytes.as_ref().unwrap()(outside_path).unwrap(), std::fs::read(outside_path).unwrap());
    let _ = app.session.media.preview_loader.as_ref().unwrap()(outside_path, 16);
    assert_eq!(preview_calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[cfg(unix)]
#[test]
fn retained_browse_reader_rejects_escaping_symlinks_after_staging() {
    use std::os::unix::fs::symlink;
    let (root, outside, workspace) = fixture();
    symlink(&outside, root.join("escape")).unwrap();
    assert!(stage(&workspace, "escape").is_err());
    let browse = stage(&workspace, ".").unwrap();
    let reader = browse.read.clone();
    let mut app = lightcraft_ui_egui::LightcraftApp::new(lightcraft_engine::Session::new().with_fs(), Default::default());
    show(&mut app, browse, &mut None).unwrap();
    let path = root.join("photo.png");
    std::fs::remove_file(&path).unwrap();
    symlink(outside.join("private.png"), &path).unwrap();
    let canonical = workspace.read_root_path().unwrap().join("photo.png");
    // Both the native source spelling (/var on macOS) and canonical /private/var
    // spelling must retain capability authority after the source becomes a link.
    for requested in [&path, &canonical] {
        let requested = requested.to_str().unwrap();
        assert!(reader(requested).is_err());
        assert!(app.session.media.file_bytes.as_ref().unwrap()(requested).is_err());
        assert!(app.session.media.file_probe.as_ref().unwrap()(requested).is_err());
        assert!(app.session.media.file_loader.as_ref().unwrap()(requested, 16).is_err());
        assert!(app.session.media.file_loader_with.as_ref().unwrap()(requested, 16, &Default::default()).is_err());
        assert!(app.session.media.preview_loader.as_ref().unwrap()(requested, 16).is_none());
    }
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[cfg(unix)]
#[test]
fn retained_browse_reader_cannot_block_on_a_fifo_substitution() {
    let (root, _, workspace) = fixture();
    let browse = stage(&workspace, ".").unwrap();
    let path = root.join("photo.png");
    std::fs::remove_file(&path).unwrap();
    assert!(std::process::Command::new("mkfifo").arg(&path).status().unwrap().success());
    let (send, receive) = std::sync::mpsc::channel();
    let requested = path.to_string_lossy().into_owned();
    std::thread::spawn(move || {
        let _ = send.send((browse.read)(&requested).is_err());
    });
    assert!(receive.recv_timeout(std::time::Duration::from_secs(2)).expect("FIFO substitution blocked the capability reader"));
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn stage_validates_paths_without_reading_or_probing_photo_contents() {
    let (root, _, workspace) = fixture();
    std::fs::write(root.join("not-decodable.png"), b"not an image").unwrap();
    // Admission succeeds even though probing this regular file must fail on the worker.
    let browse = stage(&workspace, ".").unwrap();
    assert_eq!(browse.files.len(), 2);
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

struct BlockedRead {
    gate: Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    entered: std::sync::mpsc::Receiver<()>,
    active: Arc<std::sync::atomic::AtomicUsize>,
    peak: Arc<std::sync::atomic::AtomicUsize>,
    calls: Arc<std::sync::Mutex<Vec<String>>>,
}

impl BlockedRead {
    fn release(&self) {
        let (lock, wake) = &*self.gate;
        *lock.lock().unwrap() = true;
        wake.notify_all();
    }
}

impl Drop for BlockedRead {
    fn drop(&mut self) {
        self.release();
    }
}

fn block_reader(browse: &mut photocraft_ui_egui::AutomationBrowse, path: String) -> BlockedRead {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let gate = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let (entered, receive) = std::sync::mpsc::channel();
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let hold = BlockedRead { gate: gate.clone(), entered: receive, active: active.clone(), peak: peak.clone(), calls: calls.clone() };
    let original = browse.read.clone();
    browse.read = Arc::new(move |requested| {
        peak.fetch_max(active.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
        calls.lock().unwrap().push(requested.to_string());
        if requested == path {
            let _ = entered.send(());
            let (lock, wake) = &*gate;
            let (_released, timeout) = wake.wait_timeout_while(lock.lock().unwrap(), std::time::Duration::from_secs(2), |released| !*released).unwrap();
            if timeout.timed_out() {
                active.fetch_sub(1, Ordering::SeqCst);
                return Err("synthetic blocked reader expired".into());
            }
        }
        let result = original(requested);
        active.fetch_sub(1, Ordering::SeqCst);
        result
    });
    hold
}

#[test]
fn blocked_reader_keeps_admission_responsive_and_only_latest_browse_can_publish() {
    use std::sync::atomic::Ordering;
    let (root, _, workspace) = fixture();
    for folder in ["superseded", "latest"] {
        std::fs::create_dir(root.join(folder)).unwrap();
        std::fs::copy(root.join("photo.png"), root.join(folder).join("photo.png")).unwrap();
    }
    let mut browse = stage(&workspace, ".").unwrap();
    let actual_path = browse.files[0].clone();
    let blocked = block_reader(&mut browse, actual_path);
    let mut app = lightcraft_ui_egui::LightcraftApp::new(lightcraft_engine::Session::new().with_fs(), Default::default());
    let mut hooks = None;
    let admitted = std::time::Instant::now();
    show(&mut app, browse, &mut hooks).unwrap();
    assert!(admitted.elapsed() < std::time::Duration::from_millis(500), "UI admission waited on source I/O");
    blocked.entered.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
    show(&mut app, stage(&workspace, "superseded").unwrap(), &mut hooks).unwrap();
    show(&mut app, stage(&workspace, "latest").unwrap(), &mut hooks).unwrap();
    blocked.release();
    finish_scan(&mut app);
    let paths: Vec<_> = app
        .session
        .catalog
        .photos()
        .filter_map(|photo| match &photo.source {
            lightcraft_catalog::Source::File { path } => Some(path.clone()),
            _ => None,
        })
        .collect();
    let canonical = workspace.read_root_path().unwrap();
    assert_eq!(paths, vec![canonical.join("latest/photo.png").to_string_lossy().into_owned()]);
    assert_eq!(app.session.browse.as_ref().unwrap().path, canonical.join("latest").to_string_lossy());
    assert_eq!(blocked.peak.load(Ordering::SeqCst), 1, "repeated requests multiplied workers");
    assert_eq!(blocked.calls.lock().unwrap().len(), 2, "the replaced pending scan performed source reads");
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn late_worker_completion_cannot_apply_after_library_generation_changes() {
    use std::sync::atomic::Ordering;
    let (root, outside, workspace) = fixture();
    let mut browse = stage(&workspace, ".").unwrap();
    let actual_path = browse.files[0].clone();
    let blocked = block_reader(&mut browse, actual_path);
    let mut app = lightcraft_ui_egui::LightcraftApp::new(lightcraft_engine::Session::new().with_fs(), Default::default());
    let mut hooks = None;
    show(&mut app, browse, &mut hooks).unwrap();
    blocked.entered.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
    app.session.library_generation += 1;
    app.session.browse = Some(lightcraft_engine::Browse { path: outside.to_string_lossy().into_owned(), subfolders: false });
    let ctx = egui::Context::default();
    lightcraft_ui_egui::import::poll_scan(&mut app, &ctx);
    assert!(app.scan.is_none());
    blocked.release();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while blocked.active.load(Ordering::SeqCst) != 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    lightcraft_ui_egui::import::poll_scan(&mut app, &ctx);
    assert_eq!(app.session.catalog.photos().count(), 0);
    assert_eq!(app.session.browse.as_ref().unwrap().path, outside.to_string_lossy());
    assert!(app.import.is_none());
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
