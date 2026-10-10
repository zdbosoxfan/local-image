use lightcraft_engine::Session;
use lightcraft_engine::catalog::{Catalog, Op, Photo, PhotoId, Source};
use lightcraft_engine::rename::{
    self, DATE_DIRECTIVES, TEMPLATE_NOTES, TOKENS, expand, expand_folder, expand_tokens, folder_template_error, move_file, plan_rename_photos,
    rename_photos, token_help_json, token_summary, unknown_tokens,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, time};

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn unique_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "{}-{}-{}",
        name,
        std::process::id(),
        time::SystemTime::now().duration_since(time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    fs::create_dir_all(&d).unwrap();
    d
}

fn write_file(path: &Path, contents: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn file_photo(id: u64, path: &Path, name: &str) -> Photo {
    Photo::new(PhotoId(id), Source::File { path: path.to_string_lossy().to_string() }, name, "JPG", 1, 1, "2026-01-01T00:00:00")
}

// -----------------------------------------------------------------------------
// Token expansion and template machinery
// -----------------------------------------------------------------------------

#[test]
fn expand_tokens_knows_every_documented_token_and_alias() {
    let p = rename::sample_photo();
    for t in TOKENS {
        for tag in std::iter::once(&t.tag).chain(t.aliases) {
            let v = expand_tokens(tag, &p, 1, 1);
            assert!(!v.contains('{'), "{tag} left literal: {v:?}");
            assert!(!v.is_empty(), "{tag} should expand to a non-empty example");
        }
    }
}

#[test]
fn expand_templates_with_sample_photo() {
    let p = rename::sample_photo();
    assert_eq!(expand("{name}", &p, 1), "IMG_0042.CR3");
    assert_eq!(expand("{seq:3}", &p, 1), "001.CR3");
    assert_eq!(expand("{date}", &p, 1), "20260114.CR3");
    assert_eq!(expand("{date:%Y%m%d_%H%M%S}", &p, 1), "20260114_055848.CR3");
    assert_eq!(expand("{camera} {title}", &p, 1), "Canon EOS R5 Harbour.CR3");
    assert_eq!(expand("{ext}", &p, 1), "CR3.CR3"); // extension preserved at the end too
}

#[test]
fn expand_sanitizes_forbidden_characters() {
    let mut p = rename::sample_photo();
    p.file_name = "A:B*C?.jpg".to_string();
    p.meta.camera = "X/Y\\Z".to_string();
    p.meta.title = "Hello:World".to_string();
    let out = expand("{camera}-{title}", &p, 1);
    assert!(!out.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']), "forbidden chars remain: {out}");
    assert_eq!(out, "X-Y-Z-Hello-World.jpg");
}

#[test]
fn expand_falls_back_when_result_empty() {
    let p = rename::sample_photo();
    // empty template
    assert_eq!(expand("   ", &p, 1), "IMG_0042.CR3");
    // template that expands to empty (all missing)
    let mut p2 = Photo::new(PhotoId(9), Source::Demo { scene: 0 }, "some.jpg", "jpg", 1, 1, "2026-01-01T00:00:00");
    p2.meta.camera = String::new();
    p2.meta.title = String::new();
    assert_eq!(expand("{camera}{title}", &p2, 1), "some.jpg");
}

#[test]
fn unknown_tokens_are_kept_literal_and_reported() {
    let p = rename::sample_photo();
    let template = "{name}-{bogus}-{seq:2}";
    assert_eq!(expand(template, &p, 1), "IMG_0042-{bogus}-01.CR3");
    assert_eq!(unknown_tokens(template), vec!["{bogus}".to_string()]);
    assert!(unknown_tokens("{name}{date:%Y}").is_empty());
}

#[test]
fn sequence_padding_is_clamped_to_nine_digits() {
    let p = rename::sample_photo();
    assert_eq!(expand_tokens("{seq:3}", &p, 7, 1), "007");
    assert_eq!(expand_tokens("{seq:12}", &p, 7, 1), "000000007");
    assert_eq!(expand_tokens("{seq}", &p, 7, 3), "007");
}

#[test]
fn date_directives_format_all_supported_codes() {
    let p = rename::sample_photo();
    for (dir, _) in DATE_DIRECTIVES {
        let template = format!("{{date:{}}}", dir);
        let v = expand_tokens(&template, &p, 1, 1);
        if *dir == "%%" {
            assert_eq!(v, "%");
        } else {
            assert!(!v.contains('%'), "directive {dir} not expanded in {v:?}");
        }
    }
    // unknown directive remains literal
    assert_eq!(expand_tokens("{date:%q}", &p, 1, 1), "%q");
}

#[test]
fn malformed_braces_do_not_panic() {
    let p = rename::sample_photo();
    assert_eq!(expand_tokens("{name", &p, 1, 1), "{name");
    assert_eq!(expand_tokens("name}", &p, 1, 1), "name}");
    // The parser treats the first `{` as an opening brace, the second `{` as part of the
    // token name, and the first `}` as the closing brace. The replacement for `{name` is
    // `{{name}`, which then gets the leftover `}` appended, resulting in the original string.
    assert_eq!(expand_tokens("{{name}}", &p, 1, 1), "{{name}}");
    assert_eq!(expand_tokens("{na{me}}", &p, 1, 1), "{na{me}}");
}

#[test]
fn folder_template_error_rejects_unsafe_paths() {
    let good = ["{date:%Y}/{date:%Y%m%d}", "Trips/{camera}", "{date}", "a//b"];
    for g in good {
        assert_eq!(folder_template_error(g), None, "{g:?} should be allowed");
    }

    let bad = ["", "  ", "/abs/{date}", "\\\\server\\x", "C:/x", "c:x", "~/x", "../{date}", "{date}/../x", "a/./b", "{date:%Y}/.."];
    for b in bad {
        assert!(folder_template_error(b).is_some(), "{b:?} should be rejected");
    }
}

#[test]
fn expand_folder_component_sanitization_and_fallback() {
    let mut p = rename::sample_photo();
    p.meta.camera = "../../etc/x".into();
    p.meta.title = "..".into();
    assert_eq!(expand_folder("{camera}/{title}/{date:%Y}", &p, 1), vec!["-..-etc-x".to_string(), "unknown".to_string(), "2026".to_string()]);
    p.meta.camera = "C:\\Windows".into();
    assert_eq!(expand_folder("{camera}", &p, 1), vec!["C--Windows".to_string()]);
    p.meta.camera.clear();
    p.captured = None;
    assert_eq!(expand_folder("{camera}//{date:%Y%m%d}", &p, 1), vec!["unknown".to_string(), "20260120".to_string()]);
    // rejected levels are skipped, not followed
    assert_eq!(expand_folder("/../{date:%Y}/./x", &p, 1), vec!["2026".to_string(), "x".to_string()]);
}

#[test]
fn token_help_json_and_summary_are_consistent() {
    let json = token_help_json();
    assert_eq!(json["tokens"].as_array().unwrap().len(), TOKENS.len());
    assert_eq!(json["tokens"][0]["example"], "IMG_0042");
    assert_eq!(json["dateDirectives"].as_array().unwrap().len(), DATE_DIRECTIVES.len());
    assert_eq!(json["notes"].as_array().unwrap().len(), TEMPLATE_NOTES.len());
    let summary = token_summary();
    for t in TOKENS {
        assert!(summary.contains(t.tag), "summary missing {}", t.tag);
    }
}

// -----------------------------------------------------------------------------
// File moving
// -----------------------------------------------------------------------------

#[test]
fn move_file_renames_photo_and_sidecars() {
    let dir = unique_dir("move-sidecars");
    let _cleanup = Cleanup(dir.clone());
    let photo = dir.join("IMG_1.jpg");
    let side_stem = dir.join("IMG_1.xmp");
    let side_full = dir.join("IMG_1.jpg.xmp");
    write_file(&photo, b"photo");
    write_file(&side_stem, b"stem-sidecar");
    write_file(&side_full, b"full-sidecar");

    let dst = dir.join("Trip_1.jpg");
    move_file(photo.to_str().unwrap(), dst.to_str().unwrap()).unwrap();

    assert!(dst.exists());
    assert!(!photo.exists());
    assert!(dir.join("Trip_1.xmp").exists());
    assert!(!side_stem.exists());
    assert!(dir.join("Trip_1.jpg.xmp").exists());
    assert!(!side_full.exists());
    assert_eq!(fs::read(&dst).unwrap(), b"photo");
    assert_eq!(fs::read(dir.join("Trip_1.xmp")).unwrap(), b"stem-sidecar");
    assert_eq!(fs::read(dir.join("Trip_1.jpg.xmp")).unwrap(), b"full-sidecar");
}

#[test]
fn move_file_never_overwrites_existing_target() {
    let dir = unique_dir("move-collision");
    let _cleanup = Cleanup(dir.clone());
    let a = dir.join("a.jpg");
    let b = dir.join("b.jpg");
    write_file(&a, b"content-a");
    write_file(&b, b"content-b");

    let err = move_file(a.to_str().unwrap(), b.to_str().unwrap()).unwrap_err();
    assert!(err.to_string().contains("already exists"), "{err}");
    assert_eq!(fs::read(&a).unwrap(), b"content-a");
    assert_eq!(fs::read(&b).unwrap(), b"content-b");
}

#[test]
fn move_file_case_only_rename_succeeds_on_any_volume() {
    let dir = unique_dir("move-case");
    let _cleanup = Cleanup(dir.clone());
    let upper = dir.join("UPPER.JPG");
    let lower = dir.join("upper.JPG");
    write_file(&upper, b"image");

    move_file(upper.to_str().unwrap(), lower.to_str().unwrap()).unwrap();

    // On case-insensitive volumes, lower path exists and is the same file; on case-sensitive it is
    // the new name. Either way, the contents are accessible there and the old name is gone.
    assert!(lower.exists());
    assert!(!upper.exists());
    assert_eq!(fs::read(&lower).unwrap(), b"image");
}

#[test]
fn move_file_shared_stem_sidecar_is_copied_not_taken() {
    let dir = unique_dir("move-shared-sidecar");
    let _cleanup = Cleanup(dir.clone());
    let raw = dir.join("IMG_1.CR3");
    let jpg = dir.join("IMG_1.JPG");
    let xmp = dir.join("IMG_1.xmp");
    write_file(&raw, b"raw");
    write_file(&jpg, b"jpg");
    write_file(&xmp, b"shared-xmp");

    // Rename the raw: xmp is shared with jpg -> must be copied.
    let raw_dst = dir.join("Trip_1.CR3");
    move_file(raw.to_str().unwrap(), raw_dst.to_str().unwrap()).unwrap();
    assert!(raw_dst.exists());
    assert!(xmp.exists(), "shared xmp should stay for the jpg");
    assert!(dir.join("Trip_1.xmp").exists(), "copied xmp for the raw");
    assert_eq!(fs::read(&xmp).unwrap(), b"shared-xmp");
    assert_eq!(fs::read(dir.join("Trip_1.xmp")).unwrap(), b"shared-xmp");

    // Rename the jpg: now it is the last user -> xmp moves.
    let jpg_dst = dir.join("Trip_2.JPG");
    move_file(jpg.to_str().unwrap(), jpg_dst.to_str().unwrap()).unwrap();
    assert!(jpg_dst.exists());
    assert!(!xmp.exists(), "shared xmp should now move with the last sibling");
    assert!(dir.join("Trip_2.xmp").exists());
    assert_eq!(fs::read(dir.join("Trip_2.xmp")).unwrap(), b"shared-xmp");
}

// -----------------------------------------------------------------------------
// Session rename planning and application
// -----------------------------------------------------------------------------

#[test]
fn session_plan_rename_resolves_collisions() {
    let dir = unique_dir("plan-collisions");
    let _cleanup = Cleanup(dir.clone());
    let a = dir.join("a.jpg");
    let b = dir.join("b.jpg");
    write_file(&a, b"A");
    write_file(&b, b"B");

    let mut s = Session::new();
    s.catalog.apply(Op::AddPhoto { photo: Box::new(file_photo(1, &a, "a.jpg")) }).unwrap();
    s.catalog.apply(Op::AddPhoto { photo: Box::new(file_photo(2, &b, "b.jpg")) }).unwrap();

    // Both want the same target name "same.jpg"
    let plans = s.plan_rename(&[PhotoId(1), PhotoId(2)], "same", 1);
    assert_eq!(plans.len(), 2);
    assert_eq!(plans[0].to, "same.jpg");
    assert_eq!(plans[1].to, "same-1.jpg");
    // The planned paths must not collide with each other and must not exist yet
    let p0 = PathBuf::from(plans[0].to_path.as_ref().unwrap());
    let p1 = PathBuf::from(plans[1].to_path.as_ref().unwrap());
    assert_ne!(p0, p1);
    assert!(!p0.exists());
    assert!(!p1.exists());
}

#[test]
fn session_apply_rename_moves_files_updates_catalog_and_undo_works() {
    let dir = unique_dir("apply-undo");
    let _cleanup = Cleanup(dir.clone());
    let a = dir.join("a.jpg");
    let b = dir.join("b.jpg");
    write_file(&a, b"A");
    write_file(&b, b"B");

    let mut s = Session::new();
    s.catalog.apply(Op::AddPhoto { photo: Box::new(file_photo(1, &a, "a.jpg")) }).unwrap();
    s.catalog.apply(Op::AddPhoto { photo: Box::new(file_photo(2, &b, "b.jpg")) }).unwrap();

    let ids = [PhotoId(1), PhotoId(2)];
    let plans = s.plan_rename(&ids, "Trip-{seq}", 1);
    let moved = s.apply_rename(&plans).unwrap();
    assert_eq!(moved, 2);

    let new_a = dir.join("Trip-1.jpg");
    let new_b = dir.join("Trip-2.jpg");
    assert!(new_a.exists());
    assert!(new_b.exists());
    assert!(!a.exists());
    assert!(!b.exists());
    assert_eq!(fs::read(&new_a).unwrap(), b"A");
    assert_eq!(fs::read(&new_b).unwrap(), b"B");

    let check_catalog = |s: &Session| {
        let p1 = s.catalog.photo(PhotoId(1)).unwrap();
        let p2 = s.catalog.photo(PhotoId(2)).unwrap();
        match &p1.source {
            Source::File { path } => assert_eq!(Path::new(path), new_a),
            _ => panic!("wrong source"),
        }
        assert_eq!(p1.file_name, "Trip-1.jpg");
        match &p2.source {
            Source::File { path } => assert_eq!(Path::new(path), new_b),
            _ => panic!("wrong source"),
        }
        assert_eq!(p2.file_name, "Trip-2.jpg");
    };
    check_catalog(&s);

    // Undo: files move back and catalog reverts
    let label = s.undo_step().unwrap();
    assert!(label.contains("Rename 2"));
    assert!(a.exists());
    assert!(b.exists());
    assert!(!new_a.exists());
    assert!(!new_b.exists());
    assert_eq!(fs::read(&a).unwrap(), b"A");
    assert_eq!(fs::read(&b).unwrap(), b"B");

    let p1 = s.catalog.photo(PhotoId(1)).unwrap();
    match &p1.source {
        Source::File { path } => assert_eq!(Path::new(path), a),
        _ => panic!("wrong source after undo"),
    }
    assert_eq!(p1.file_name, "a.jpg");
    let p2 = s.catalog.photo(PhotoId(2)).unwrap();
    match &p2.source {
        Source::File { path } => assert_eq!(Path::new(path), b),
        _ => panic!("wrong source after undo"),
    }
    assert_eq!(p2.file_name, "b.jpg");
}

#[test]
fn rename_photos_includes_each_file_once() {
    let mut c = Catalog::new();
    // Two virtual copies sharing the same file
    let path = Path::new("/virtual/photo.jpg");
    c.apply(Op::AddPhoto { photo: Box::new(file_photo(1, path, "photo.jpg")) }).unwrap();
    c.apply(Op::AddPhoto { photo: Box::new(file_photo(2, path, "photo.jpg")) }).unwrap();

    let photos = rename_photos(&c, &[PhotoId(1), PhotoId(2)]);
    assert_eq!(photos.len(), 1);
    assert_eq!(photos[0].id, PhotoId(1));
}

#[test]
fn plan_rename_photos_with_fake_fs_avoids_taken_names() {
    let mut p1 = file_photo(1, Path::new("/p/IMG_1.JPG"), "IMG_1.JPG");
    let mut p2 = file_photo(2, Path::new("/p/IMG_2.JPG"), "IMG_2.JPG");
    p1.captured = Some("2026-01-01T00:00:00".into());
    p2.captured = Some("2026-01-02T00:00:00".into());

    let photos = vec![Arc::new(p1), Arc::new(p2)];
    // Simulate an empty disk: nothing exists, so the planner should assign names without
    // disk collisions, but still avoid duplicates within the batch.
    let exists = |_p: &str| false;

    let plans = plan_rename_photos(&photos, "same", 1, &exists);
    assert_eq!(plans.len(), 2);
    // Extension is preserved exactly (uppercase from the original file name)
    assert_eq!(plans[0].to, "same.JPG");
    assert_eq!(plans[1].to, "same-1.JPG");
}

#[test]
fn move_file_rollback_when_sidecar_missing_does_not_lose_photo() {
    // Real filesystem: sidecar removal should be fine; but we can simulate a failure by making a
    // sidecar read-only? Not reliable. Instead test a partial move: if the target sidecar cannot be
    // created because its parent is missing, the photo should be moved back.
    let dir = unique_dir("move-rollback");
    let _cleanup = Cleanup(dir.clone());
    let photo = dir.join("photo.jpg");
    let sidecar = dir.join("photo.xmp");
    write_file(&photo, b"photo");
    write_file(&sidecar, b"sidecar");

    // Make the target directory non-existent to force a failure when moving sidecar.
    let dst_dir = dir.join("nonexistent");
    let dst = dst_dir.join("renamed.jpg");
    let err = move_file(photo.to_str().unwrap(), dst.to_str().unwrap()).unwrap_err();
    assert!(err.to_string().contains("rename /") || err.to_string().contains("rename "), "{err}");
    // The original photo and sidecar must still be present.
    assert!(photo.exists());
    assert!(sidecar.exists());
    assert_eq!(fs::read(&photo).unwrap(), b"photo");
    assert_eq!(fs::read(&sidecar).unwrap(), b"sidecar");
}

#[test]
fn expand_tokens_handles_unicode_and_empty_input() {
    let p = rename::sample_photo();
    assert_eq!(expand_tokens("", &p, 1, 1), "");
    assert_eq!(expand_tokens("名字", &p, 1, 1), "名字");
    assert_eq!(expand_tokens("{name}é", &p, 1, 1), "IMG_0042é");
}
