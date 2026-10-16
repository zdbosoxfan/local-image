use super::*;

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
    assert_eq!(browse.photos.len(), 1);
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
    assert_eq!(app.session.visible().len(), 1);
    let first = app.session.media.file_loader.clone().unwrap();
    show(&mut app, stage(&workspace, ".").unwrap(), &mut hooks).unwrap();
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
    let path = path.to_str().unwrap();
    assert!(reader(path).is_err());
    assert!(app.session.media.file_bytes.as_ref().unwrap()(path).is_err());
    assert!(app.session.media.file_probe.as_ref().unwrap()(path).is_err());
    assert!(app.session.media.file_loader.as_ref().unwrap()(path, 16).is_err());
    assert!(app.session.media.file_loader_with.as_ref().unwrap()(path, 16, &Default::default()).is_err());
    assert!(app.session.media.preview_loader.as_ref().unwrap()(path, 16).is_none());
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
