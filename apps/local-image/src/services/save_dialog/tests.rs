use super::*;

/// Upstream 26e00305 (#5039): a name typed without an extension gets the suggested file's, so
/// the save picks its format; a name with one is left alone.
#[test]
fn a_save_name_without_an_extension_gets_the_suggested_one() {
    assert_eq!(with_suggested_extension("/pics/cat".into(), "Untitled-1.psd"), "/pics/cat.psd");
    assert_eq!(with_suggested_extension("/pics/cat".into(), "/docs/Shot.PNG"), "/pics/cat.png");
    assert_eq!(with_suggested_extension("/pics/dog.png".into(), "Untitled-1.psd"), "/pics/dog.png");
    assert_eq!(with_suggested_extension("/pics/cat".into(), "Untitled"), "/pics/cat", "nothing to suggest");
}

/// Upstream 58f855a4 (#6121): the file type picked gives the name its extension where the
/// dialog left the one it opened with, and only there.
#[test]
fn the_picked_file_type_gives_the_name_its_extension() {
    let exts = |e: &[&str]| e.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let (png, psd) = (exts(&["png"]), exts(&["psd", "psb"]));
    let opened = Some("Untitled-1.psd");
    let cases = [
        // PNG picked, the name as the dialog opened (or renamed, its extension kept).
        ("/pics/Untitled-1.psd", opened, &png, "/pics/Untitled-1.png"),
        ("/pics/art.psd", opened, &png, "/pics/art.png"),
        ("/pics/ART.PSD", opened, &png, "/pics/ART.png"),
        // No extension: the type's. One Local Image doesn't save is part of the name.
        ("/pics/art", opened, &png, "/pics/art.png"),
        ("/pics/scan.2024", opened, &png, "/pics/scan.2024.png"),
        // Typed, an extension Local Image saves is the user's: kept, whatever the type says.
        ("/pics/art.png", opened, &psd, "/pics/art.png"),
        ("/pics/art.jpg", opened, &png, "/pics/art.jpg"),
        ("/pics/art.pcraft", opened, &png, "/pics/art.pcraft"),
        // Already the type's: kept, including its other extensions.
        ("/pics/art.png", opened, &png, "/pics/art.png"),
        ("/pics/art.PNG", opened, &png, "/pics/art.PNG"),
        ("/pics/big.psb", opened, &psd, "/pics/big.psb"),
        // The dialog opened on a .pcraft: that is the extension the type replaces.
        ("/pics/art.pcraft", Some("art.pcraft"), &png, "/pics/art.png"),
        ("/pics/art.psd", Some("art.pcraft"), &png, "/pics/art.psd"),
    ];
    for (typed, opened, chosen, want) in cases {
        assert_eq!(with_chosen_type(Path::new(typed), opened, chosen), PathBuf::from(want), "{typed} (opened {opened:?})");
    }
    assert_eq!(with_chosen_type(Path::new("/pics/art.psd"), opened, &[]), PathBuf::from("/pics/art.psd"), "no type picked");
}

#[test]
fn the_portal_dialog_opens_on_the_stem() {
    assert_eq!(dialog_file_name("/pics/test.psd").as_deref(), Some("test"));
    assert_eq!(dialog_file_name("scan.jpg.psd").as_deref(), Some("scan.jpg.psd"), "a stem with a dot keeps the whole name");
    assert_eq!(dialog_file_name("Shot.PNG").as_deref(), Some("Shot.PNG"), "the user's spelling");
    assert_eq!(dialog_file_name("Untitled").as_deref(), Some("Untitled"));
}

#[cfg(unix)]
#[test]
fn file_uris_become_local_paths() {
    assert_eq!(file_uri_path("file:///home/me/Untitled-1.psd"), Some(PathBuf::from("/home/me/Untitled-1.psd")));
    assert_eq!(file_uri_path("file:///home/me/A%20b%C3%A9.png"), Some(PathBuf::from("/home/me/A bé.png")));
    assert_eq!(file_uri_path("file://localhost/tmp/x.png"), Some(PathBuf::from("/tmp/x.png")));
    assert_eq!(file_uri_path("sftp://host/x.png"), None);
    assert_eq!(file_uri_path("file://host/x.png"), None);
}

/// The name and starting filter of each dialog the scripted dialog was opened with.
type Seen = std::rc::Rc<std::cell::RefCell<Vec<(Option<String>, Option<usize>)>>>;

/// A scripted dialog: each call answers the next `(path, filter label)` (`None` is Cancel).
fn scripted(script: Vec<Option<(PathBuf, &'static str)>>) -> (impl FnMut(&SaveRequest) -> Result<Option<Saved>, String>, Seen) {
    let seen = Seen::default();
    let log = seen.clone();
    let mut script = script.into_iter();
    let show = move |req: &SaveRequest| {
        log.borrow_mut().push((req.name.clone(), req.current));
        Ok(script.next().flatten().map(|(path, label)| Saved { path, filter: Some(label.to_string()) }))
    };
    (show, seen)
}

fn request(name: &str, suggested: &str) -> SaveRequest {
    let filters = [("Photoshop", &["psd", "psb"][..]), ("PNG", &["png"]), ("JPEG", &["jpg"])];
    SaveRequest {
        name: Some(name.into()),
        suggested: Some(suggested.into()),
        folder: Some(PathBuf::from("/tmp")),
        filters: filters.iter().map(|(l, e)| (l.to_string(), e.iter().map(|s| s.to_string()).collect())).collect(),
        current: Some(0),
    }
}

/// Upstream 58f855a4: the type picked gives the extension; the suggested extension still in the
/// name with PNG picked becomes PNG; a name that then exists is asked about again under that name.
#[test]
fn save_as_follows_the_picked_type_and_reasks_for_an_existing_target() {
    let dir = std::env::temp_dir().join(format!("li-save-dialog-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("test.psd"), b"open document").unwrap();
    let (show, seen) = scripted(vec![
        Some((dir.join("test"), "PNG")),
        Some((dir.join("test.psd"), "PNG")),
        Some((dir.join("test"), "Photoshop")),
        Some((dir.join("test.psd"), "Photoshop")),
        None,
    ]);
    let mut show = show;
    let opened = || request("test", "test.psd");
    assert_eq!(save_as(&mut show, opened()).unwrap(), Some(dir.join("test.png")));
    assert_eq!(save_as(&mut show, opened()).unwrap(), Some(dir.join("test.png")), "not a PSD written over test.psd");
    // The type untouched: test.psd exists, so the dialog asks again under that name.
    assert_eq!(save_as(&mut show, opened()).unwrap(), Some(dir.join("test.psd")));
    assert_eq!(save_as(&mut show, opened()).unwrap(), None, "Cancel");
    let names: Vec<_> = seen.borrow().iter().map(|(n, _)| n.clone()).collect();
    assert_eq!(names, [Some("test".into()), Some("test".into()), Some("test".into()), Some("test.psd".into()), Some("test".into())]);
    assert_eq!(seen.borrow()[3].1, Some(0), "asked again on the picked type");
    std::fs::remove_dir_all(&dir).ok();
}

/// An extension the user typed wins over an untouched type, and with only one type offered a
/// typed extension is kept.
#[test]
fn typed_extensions_are_kept() {
    let (mut show, _) = scripted(vec![Some((PathBuf::from("/tmp/li-none/art.png"), "Photoshop"))]);
    assert_eq!(save_as(&mut show, request("Untitled-1", "Untitled-1.psd")).unwrap(), Some(PathBuf::from("/tmp/li-none/art.png")));
    let (mut show, _) = scripted(vec![Some((PathBuf::from("/tmp/li-none/Swatches.ase"), "Color Swatches"))]);
    let req = SaveRequest { filters: vec![("Color Swatches".into(), vec!["aco".into()])], ..request("Swatches", "Swatches.aco") };
    assert_eq!(save_as(&mut show, req).unwrap(), Some(PathBuf::from("/tmp/li-none/Swatches.ase")));
}
