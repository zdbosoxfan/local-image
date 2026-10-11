use std::fs;
use std::path::{Path, PathBuf};

use lightcraft_catalog::{
    Catalog, DateRun, Filter, GroupBy, Op, Photo, PhotoId, RatingOp, Sort, SortKey, Source,
    dates::{civil, civil_from_days, display_time, group_label, normalize_iso, shift_iso, weekday},
    keywords::{clean, is_under},
    query::{folder_key, folder_within, in_folder, merged_kind},
    safe_file::{fail_writes_after, same_file, write_atomic, write_new, write_new_unique},
};

fn add_photo(c: &mut Catalog, file_name: &str, captured: &str, imported: &str, rating: u8, keywords: &[&str]) -> PhotoId {
    let id = c.alloc_photo_id();
    let mut p = Photo::new(id, Source::Demo { scene: 1 }, file_name, "JPEG", 3, 2, imported);
    p.rating = rating;
    if !captured.is_empty() {
        p.captured = Some(captured.to_string());
    }
    p.meta.keywords = keywords.iter().map(|s| s.to_string()).collect();
    c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    id
}

#[test]
fn keywords_clean_and_is_under() {
    assert_eq!(clean(" a | |b "), "a|b");
    assert!(is_under("Travel|Italy", "travel"));
    assert!(is_under("Travel|Italy|Rome", "travel|italy"));
    assert!(!is_under("Travel|Italy", "travel|fra"));
    assert!(is_under("travel", "travel"));
}

#[test]
fn keyword_tree_counts_and_sorts_case_insensitively() {
    let mut c = Catalog::new();
    add_photo(&mut c, "a.jpg", "", "2026-01-01T00:00:00", 0, &["beach", "travel|Italy|Rome"]);
    add_photo(&mut c, "b.jpg", "", "2026-01-01T00:00:00", 0, &["Travel|italy"]);
    add_photo(&mut c, "c.jpg", "", "2026-01-01T00:00:00", 0, &["travel|France", "travel|italy|rome"]);
    add_photo(&mut c, "d.jpg", "", "2026-01-01T00:00:00", 0, &["beach"]);

    let tree = c.keyword_tree();
    assert_eq!(tree.len(), 2);
    assert_eq!(tree[0].name, "beach");
    assert_eq!(tree[0].count, 2);
    assert_eq!(tree[1].name, "travel");
    assert_eq!(tree[1].count, 3);
    assert_eq!(tree[1].children.len(), 2);
    assert_eq!(tree[1].children[0].path, "travel|France");
    assert_eq!(tree[1].children[0].count, 1);
    assert_eq!(tree[1].children[1].path, "travel|Italy");
    assert_eq!(tree[1].children[1].count, 3);
    assert_eq!(tree[1].children[1].children[0].path, "travel|Italy|Rome");
    assert_eq!(tree[1].children[1].children[0].count, 2);
}

#[test]
fn keyword_rename_updates_subtree_and_roundtrips() {
    let mut c = Catalog::new();
    let id1 = add_photo(&mut c, "a.jpg", "", "2026-01-01T00:00:00", 0, &["travel|italy|rome", "beach"]);
    let id2 = add_photo(&mut c, "b.jpg", "", "2026-01-01T00:00:00", 0, &["Travel|Italy"]);
    let _ = add_photo(&mut c, "c.jpg", "", "2026-01-01T00:00:00", 0, &["italia", "travel|italy"]);

    let before = c.to_snapshot();
    let op = c.rename_keyword_ops("travel|italy", "Europe|Italy").unwrap();
    let inv = c.apply(op).unwrap();

    let keywords = |id: PhotoId| c.photo(id).unwrap().meta.keywords.clone();
    assert_eq!(keywords(id1), vec!["Europe|Italy|rome", "beach"]);
    assert_eq!(keywords(id2), vec!["Europe|Italy"]);

    c.apply(inv).unwrap();
    assert_eq!(c.to_snapshot(), before);
}

#[test]
fn keyword_rename_rejects_invalid_requests() {
    let mut c = Catalog::new();
    add_photo(&mut c, "a.jpg", "", "2026-01-01T00:00:00", 0, &["beach"]);
    assert!(c.rename_keyword_ops("beach", " ").is_err());
    assert!(c.rename_keyword_ops("a", "a|b").is_err());
    assert!(c.rename_keyword_ops("  ", "x").is_err());
}

#[test]
fn keyword_delete_removes_subtree_and_roundtrips() {
    let mut c = Catalog::new();
    let id1 = add_photo(&mut c, "a.jpg", "", "2026-01-01T00:00:00", 0, &["travel|italy|rome", "beach"]);
    let id2 = add_photo(&mut c, "b.jpg", "", "2026-01-01T00:00:00", 0, &["Travel|Italy"]);

    let before = c.to_snapshot();
    let op = c.delete_keyword_ops("TRAVEL").unwrap();
    let inv = c.apply(op).unwrap();

    assert!(c.photo(id1).unwrap().meta.keywords.iter().all(|k| !is_under(k, "travel")));
    assert!(c.photo(id2).unwrap().meta.keywords.is_empty());

    c.apply(inv).unwrap();
    assert_eq!(c.to_snapshot(), before);
}

#[test]
fn keyword_merge_collapses_duplicates_and_roundtrips() {
    let mut c = Catalog::new();
    let id = add_photo(&mut c, "a.jpg", "", "2026-01-01T00:00:00", 0, &["italia", "travel|italy"]);

    let before = c.to_snapshot();
    let op = c.merge_keywords_ops(&["italia".into()], "travel|italy").unwrap();
    let inv = c.apply(op).unwrap();

    assert_eq!(c.photo(id).unwrap().meta.keywords, vec!["travel|italy"]);

    c.apply(inv).unwrap();
    assert_eq!(c.to_snapshot(), before);
}

#[test]
fn keyword_suggestions_rank_prefix_and_cooccurrence() {
    let mut c = Catalog::new();
    add_photo(&mut c, "a.jpg", "", "2026-01-01T00:00:00", 0, &["dog", "park"]);
    add_photo(&mut c, "b.jpg", "", "2026-01-01T00:00:00", 0, &["dog", "park", "ball"]);
    add_photo(&mut c, "c.jpg", "", "2026-01-01T00:00:00", 0, &["dog", "beach"]);
    add_photo(&mut c, "d.jpg", "", "2026-01-01T00:00:00", 0, &["cat"]);
    add_photo(&mut c, "e.jpg", "", "2026-01-01T00:00:00", 0, &["cat"]);
    add_photo(&mut c, "f.jpg", "", "2026-01-01T00:00:00", 0, &["cat"]);
    add_photo(&mut c, "g.jpg", "", "2026-01-01T00:00:00", 0, &["parade"]);
    add_photo(&mut c, "h.jpg", "", "2026-01-01T00:00:00", 0, &["eagle"]);

    let s = c.keyword_suggestions(&["dog".into()], "", 3);
    assert_eq!(s, vec!["park".to_string(), "ball".to_string(), "beach".to_string()]);
    let s = c.keyword_suggestions(&[], "", 2);
    assert_eq!(s, vec!["cat".to_string(), "dog".to_string()]);
    let s = c.keyword_suggestions(&["park".into()], "pa", 5);
    assert_eq!(s, vec!["parade".to_string()]);
    let s = c.keyword_suggestions(&[], "E", 5);
    assert_eq!(s, vec!["eagle".to_string(), "beach".to_string(), "parade".to_string()]);
}

#[test]
fn dates_civil_weekday_shift_normalize() {
    assert_eq!(civil(0), "1970-01-01T00:00:00");
    assert_eq!(civil(951_782_400), "2000-02-29T00:00:00");
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    assert_eq!(weekday("2026-09-30"), Some("Wednesday"));
    assert_eq!(weekday("2026-10-01T09:00:00"), Some("Thursday"));
    assert_eq!(weekday("2000-02-29"), Some("Tuesday"));
    assert_eq!(shift_iso("2026-12-31T23:30:00", 3600).as_deref(), Some("2027-01-01T00:30:00"));
    assert_eq!(shift_iso("2026-03-01T00:00:00.25+02:00", -1).as_deref(), Some("2026-02-28T23:59:59.25+02:00"));
    assert_eq!(shift_iso("nope", 5), None);
    assert_eq!(normalize_iso("2026-04-01 10:05").as_deref(), Some("2026-04-01T10:05:00"));
    assert_eq!(normalize_iso("2026-02-30"), None);
}

#[test]
fn dates_group_label_display_time() {
    assert_eq!(group_label("2026-09-29"), "Tuesday, 29 September 2026");
    assert_eq!(group_label("2026-02"), "February 2026");
    assert_eq!(group_label("2026"), "2026");
    assert_eq!(group_label(""), "Unknown Date");
    assert_eq!(group_label("2024-01é0"), "2024-01é0");
    assert_eq!(group_label("202é01"), "January 202é01");
    assert_eq!(display_time("2022-03-30T22:11:11"), "March 30, 2022 at 10:11:11 PM");
    assert_eq!(display_time("2022-03-30"), "March 30, 2022");
    assert_eq!(display_time("bad"), "bad");
}

#[test]
fn date_runs_groups_by_day_month_year() {
    let mut c = Catalog::new();
    let id1 = add_photo(&mut c, "a.jpg", "2026-09-30T18:00:00", "2026-10-01T00:00:00", 0, &[]);
    let id2 = add_photo(&mut c, "b.jpg", "2026-09-30T09:00:00", "2026-10-01T00:00:00", 0, &[]);
    let id3 = add_photo(&mut c, "c.jpg", "2026-09-29T23:59:00", "2026-10-01T00:00:00", 0, &[]);
    let id4 = add_photo(&mut c, "d.jpg", "2026-08-01T10:00:00", "2026-10-01T00:00:00", 0, &[]);
    let id5 = add_photo(&mut c, "e.jpg", "2025-12-31T10:00:00", "2026-10-01T00:00:00", 0, &[]);
    let ids = vec![id1, id2, id3, id4, id5];

    let shape = |runs: &[DateRun]| runs.iter().map(|r| (r.key.clone(), r.start, r.count)).collect::<Vec<_>>();

    let day = c.date_runs(&ids, SortKey::CaptureDate, GroupBy::Day);
    assert_eq!(
        shape(&day),
        vec![("2026-09-30".to_string(), 0, 2), ("2026-09-29".to_string(), 2, 1), ("2026-08-01".to_string(), 3, 1), ("2025-12-31".to_string(), 4, 1),]
    );
    assert_eq!(day[0].label, "Wednesday, 30 September 2026");

    let month = c.date_runs(&ids, SortKey::CaptureDate, GroupBy::Month);
    assert_eq!(shape(&month), vec![("2026-09".to_string(), 0, 3), ("2026-08".to_string(), 3, 1), ("2025-12".to_string(), 4, 1),]);

    let year = c.date_runs(&ids, SortKey::CaptureDate, GroupBy::Year);
    assert_eq!(shape(&year), vec![("2026".to_string(), 0, 4), ("2025".to_string(), 4, 1),]);

    assert!(c.date_runs(&ids, SortKey::FileName, GroupBy::Day).is_empty());
    assert!(c.date_runs(&ids, SortKey::CaptureDate, GroupBy::None).is_empty());
}

#[test]
fn group_by_parse() {
    assert_eq!(GroupBy::parse("auto"), Some(GroupBy::Auto));
    assert_eq!(GroupBy::parse("none"), Some(GroupBy::None));
    assert_eq!(GroupBy::parse("off"), Some(GroupBy::None));
    assert_eq!(GroupBy::parse("day"), Some(GroupBy::Day));
    assert_eq!(GroupBy::parse("month"), Some(GroupBy::Month));
    assert_eq!(GroupBy::parse("year"), Some(GroupBy::Year));
    assert_eq!(GroupBy::parse("bogus"), None);
}

#[test]
fn filter_rating_ops() {
    let mut c = Catalog::new();
    let p1 = add_photo(&mut c, "a.jpg", "", "2026-01-01T00:00:00", 1, &[]);
    let p2 = add_photo(&mut c, "b.jpg", "", "2026-01-01T00:00:00", 3, &[]);
    let p3 = add_photo(&mut c, "c.jpg", "", "2026-01-01T00:00:00", 5, &[]);

    let mut sort = Sort { key: SortKey::Rating, ascending: true, ..Default::default() };

    let mut f = Filter { rating: 3, rating_op: RatingOp::AtLeast, ..Default::default() };
    assert_eq!(c.query(&f, &sort), vec![p2, p3]);

    f.rating_op = RatingOp::Exactly;
    assert_eq!(c.query(&f, &sort), vec![p2]);

    f.rating_op = RatingOp::AtMost;
    assert_eq!(c.query(&f, &sort), vec![p1, p2]);

    sort.ascending = false;
    f.rating_op = RatingOp::AtLeast;
    assert_eq!(c.query(&f, &sort), vec![p3, p2]);
}

#[test]
fn filter_text_fielded_tokens() {
    let mut c = Catalog::new();
    let id = c.alloc_photo_id();
    let mut p = Photo::new(id, Source::Demo { scene: 1 }, "IMG_123.jpg", "JPEG", 3, 2, "2026-01-01T00:00:00");
    p.rating = 4;
    p.captured = Some("2026-04-01T00:00:00".to_string());
    p.meta.keywords = vec!["travel|italy".to_string()];
    p.meta.iso = Some(1600u32);
    p.meta.camera = "Canon".to_string();
    p.meta.title = "Sunset".to_string();
    c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();

    let mut f = Filter { text: "rating:4".to_string(), ..Default::default() };
    assert_eq!(c.query(&f, &Sort::default()), vec![id]);

    f.text = "rating:>3".to_string();
    assert!(c.query(&f, &Sort::default()).contains(&id));

    f.text = "iso:>800".to_string();
    assert!(c.query(&f, &Sort::default()).contains(&id));

    f.text = "camera:canon".to_string();
    assert!(c.query(&f, &Sort::default()).contains(&id));

    f.text = "keyword:travel".to_string();
    assert!(c.query(&f, &Sort::default()).contains(&id));

    f.text = "date:2026".to_string();
    assert!(c.query(&f, &Sort::default()).contains(&id));

    f.text = "name:IMG".to_string();
    assert!(c.query(&f, &Sort::default()).contains(&id));

    f.text = "file:IMG".to_string();
    assert!(c.query(&f, &Sort::default()).contains(&id));
}

#[test]
fn filter_free_text_search() {
    let mut c = Catalog::new();
    let id = c.alloc_photo_id();
    let mut p = Photo::new(id, Source::Demo { scene: 1 }, "IMG_123.jpg", "JPEG", 3, 2, "2026-01-01T00:00:00");
    p.meta.title = "Sunset Beach".to_string();
    p.meta.caption = "Nice view".to_string();
    p.meta.keywords = vec!["beach".to_string()];
    p.meta.camera = "Canon EOS".to_string();
    p.meta.lens = "50mm".to_string();
    p.format = "JPEG".to_string();
    c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();

    let mut f = Filter { text: "sunset".to_string(), ..Default::default() };
    assert_eq!(c.query(&f, &Sort::default()), vec![id]);

    f.text = "beach nice".to_string();
    assert_eq!(c.query(&f, &Sort::default()), vec![id]);

    f.text = "canon 50mm".to_string();
    assert_eq!(c.query(&f, &Sort::default()), vec![id]);

    f.text = "jpg".to_string();
    assert_eq!(c.query(&f, &Sort::default()), vec![id]);
}

#[test]
fn filter_deleted_and_local() {
    let mut c = Catalog::new();
    let id1 = add_photo(&mut c, "a.jpg", "", "2026-01-01T00:00:00", 0, &[]);
    let id2 = add_photo(&mut c, "b.jpg", "", "2026-01-01T00:00:00", 0, &[]);
    let id3 = add_photo(&mut c, "c.jpg", "", "2026-01-01T00:00:00", 0, &[]);

    // Mark id2 as deleted, id3 as local
    c.apply(Op::SetDeleted { id: id2, deleted: true }).unwrap();
    c.apply(Op::SetLocal { id: id3, local: true }).unwrap();

    let f = Filter::default();
    assert_eq!(c.query(&f, &Sort::default()), vec![id1]);

    let f = Filter { deleted: true, ..Default::default() };
    assert_eq!(c.query(&f, &Sort::default()), vec![id2]);

    let f = Filter { folder: Some("/some/dir".to_string()), ..Default::default() };
    // id3 is local but folder set; should not appear in folder view? Actually local photos
    // only show in folder views when they are in that folder. We'll just test that default
    // filter excludes local.
    assert!(!c.query(&f, &Sort::default()).contains(&id3));
}

#[test]
fn query_sort_orders_by_key() {
    let mut c = Catalog::new();

    let mut p = Photo::new(c.alloc_photo_id(), Source::Demo { scene: 1 }, "b.jpg", "JPEG", 3, 2, "2026-01-03T00:00:00");
    p.rating = 5;
    p.file_size = 300;
    p.captured = Some("2026-01-02T00:00:00".to_string());
    let id1 = p.id;
    c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();

    let mut p = Photo::new(c.alloc_photo_id(), Source::Demo { scene: 1 }, "a.jpg", "JPEG", 3, 2, "2026-01-02T00:00:00");
    p.rating = 3;
    p.file_size = 100;
    p.captured = Some("2026-02-01T00:00:00".to_string());
    let id2 = p.id;
    c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();

    let mut p = Photo::new(c.alloc_photo_id(), Source::Demo { scene: 1 }, "c.jpg", "JPEG", 3, 2, "2026-01-01T00:00:00");
    p.rating = 4;
    p.file_size = 200;
    p.captured = Some("2026-01-01T00:00:00".to_string());
    let id3 = p.id;
    c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();

    let mut sort = Sort { key: SortKey::CaptureDate, ascending: true, ..Default::default() };

    assert_eq!(c.query(&Filter::default(), &sort), vec![id3, id1, id2]);

    sort.ascending = false;
    assert_eq!(c.query(&Filter::default(), &sort), vec![id2, id1, id3]);

    sort.key = SortKey::ImportDate;
    sort.ascending = true;
    assert_eq!(c.query(&Filter::default(), &sort), vec![id3, id2, id1]);

    sort.key = SortKey::FileName;
    sort.ascending = true;
    assert_eq!(c.query(&Filter::default(), &sort), vec![id2, id1, id3]);

    sort.key = SortKey::Rating;
    sort.ascending = true;
    assert_eq!(c.query(&Filter::default(), &sort), vec![id2, id3, id1]);

    sort.key = SortKey::FileSize;
    sort.ascending = true;
    assert_eq!(c.query(&Filter::default(), &sort), vec![id2, id3, id1]);
}

#[test]
fn query_random_deterministic() {
    let mut c = Catalog::new();
    add_photo(&mut c, "a.jpg", "2026-01-01T00:00:00", "2026-01-01T00:00:00", 0, &[]);
    add_photo(&mut c, "b.jpg", "2026-01-02T00:00:00", "2026-01-02T00:00:00", 0, &[]);
    add_photo(&mut c, "c.jpg", "2026-01-03T00:00:00", "2026-01-03T00:00:00", 0, &[]);

    let mut sort = Sort { key: SortKey::Random, seed: 42, ..Default::default() };

    let first = c.query(&Filter::default(), &sort);
    let second = c.query(&Filter::default(), &sort);
    assert_eq!(first, second);

    sort.seed = 43;
    let different = c.query(&Filter::default(), &sort);
    assert_ne!(first, different);
}

#[test]
fn date_groups_counts() {
    let mut c = Catalog::new();
    add_photo(&mut c, "a.jpg", "2026-01-05T00:00:00", "2026-01-01T00:00:00", 0, &[]);
    add_photo(&mut c, "b.jpg", "2026-01-06T00:00:00", "2026-01-01T00:00:00", 0, &[]);
    add_photo(&mut c, "c.jpg", "2025-12-31T00:00:00", "2026-01-01T00:00:00", 0, &[]);

    let groups = c.date_groups();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].year, "2026");
    assert_eq!(groups[0].count, 2);
    assert_eq!(groups[0].months, vec![("2026-01".to_string(), 2)]);
    assert_eq!(groups[0].days, vec![("2026-01-06".to_string(), 1), ("2026-01-05".to_string(), 1)]);
    assert_eq!(groups[1].year, "2025");
    assert_eq!(groups[1].count, 1);
}

#[test]
fn keywords_usage_counts() {
    let mut c = Catalog::new();
    add_photo(&mut c, "a.jpg", "", "2026-01-01T00:00:00", 0, &["beach", "travel"]);
    add_photo(&mut c, "b.jpg", "", "2026-01-01T00:00:00", 0, &["beach"]);
    add_photo(&mut c, "c.jpg", "", "2026-01-01T00:00:00", 0, &["travel", "mountain"]);

    let mut kw = c.keywords();
    kw.sort();
    assert_eq!(kw, vec![("beach".to_string(), 2), ("mountain".to_string(), 1), ("travel".to_string(), 2),]);
}

#[test]
fn path_helpers_normalize_and_match() {
    assert!(in_folder("/a/b/c.jpg", "/a/b", false));
    assert!(!in_folder("/a/b/c/d.jpg", "/a/b", false));
    assert!(in_folder("/a/b/c/d.jpg", "/a/b", true));
    assert!(!in_folder("/a/bc/d.jpg", "/a/b", true));

    assert_eq!(folder_key("/a//b/../c/"), "/a/c");
    assert_eq!(folder_key("C:\\Foo\\Bar"), if cfg!(windows) { "c:/foo/bar".to_string() } else { "c:/Foo/Bar".to_string() });
    assert_eq!(folder_key("//server/share/dir/"), "//server/share/dir");

    assert!(folder_within("/a/b/c", "/a/b"));
    assert!(folder_within("/a/b", "/a/b"));
    assert!(!folder_within("/a/bc", "/a/b"));
}

#[test]
fn merged_kind_identifies_names() {
    assert_eq!(merged_kind("IMG_1-HDR.dng"), Some("hdr"));
    assert_eq!(merged_kind("IMG_1-Pano.dng"), Some("panorama"));
    assert_eq!(merged_kind("IMG_1-HDR-Pano.dng"), Some("hdrPanorama"));
    assert_eq!(merged_kind("IMG_1.dng"), None);
    assert_eq!(merged_kind("IMG_1-HDR-Pano-2.dng"), Some("hdrPanorama"));
}

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("lc-coverage-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn list_names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
    v.sort();
    v
}

#[test]
fn safe_file_atomic_write_replaces_and_leaves_no_temp() {
    let d = temp_dir("atomic");
    let p = d.join("out.jpg");
    write_atomic(&p, b"one").unwrap();
    write_atomic(&p, b"two two").unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"two two");
    assert_eq!(list_names(&d), vec!["out.jpg"]);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn safe_file_new_never_replaces() {
    let d = temp_dir("new");
    let p = d.join("a.dng");
    fs::write(&p, b"keep").unwrap();
    assert_eq!(write_new(&p, b"other").unwrap_err().kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&p).unwrap(), b"keep");

    let mut candidates = (1..).map(|k| if k == 1 { d.join("a.dng") } else { d.join(format!("a-{k}.dng")) });
    let got = write_new_unique(&mut candidates, b"fresh").unwrap();
    assert_eq!(got, d.join("a-2.dng"));
    assert_eq!(fs::read(&got).unwrap(), b"fresh");
    assert_eq!(list_names(&d), vec!["a-2.dng", "a.dng"]);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn safe_file_fault_injection_keeps_old_file() {
    let d = temp_dir("fault");
    let p = d.join("photo.jpg");
    fs::write(&p, b"the original bytes").unwrap();
    {
        let _guard = fail_writes_after(4);
        let e = write_atomic(&p, &[7u8; 10_000]).unwrap_err();
        assert!(e.to_string().contains("injected"), "{e}");
        assert!(write_new(&d.join("new.dng"), &[1u8; 100]).is_err());
    }
    assert_eq!(fs::read(&p).unwrap(), b"the original bytes");
    assert_eq!(list_names(&d), vec!["photo.jpg"]);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn safe_file_same_file_detects_same_file() {
    let d = temp_dir("same");
    let p = d.join("x.jpg");
    fs::write(&p, b"x").unwrap();
    fs::write(d.join("y.jpg"), b"x").unwrap();
    fs::create_dir_all(d.join("sub")).unwrap();

    assert!(same_file(&p, &d.join("sub/../x.jpg")));
    assert!(!same_file(&p, &d.join("y.jpg")));
    assert!(!same_file(&p, &d.join("missing.jpg")));
    let _ = fs::remove_dir_all(&d);
}
