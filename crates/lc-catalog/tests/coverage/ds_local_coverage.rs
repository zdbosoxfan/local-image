use lightcraft_catalog::{
    AlbumId, Catalog, Op, PhotoId,
    local::{DEFAULT_FORGET_DAYS, ForgetPlan},
};

fn set_browsed(c: &mut Catalog, folder: &str, at: &str) {
    c.apply(Op::SetBrowsed { folder: folder.to_string(), at: Some(at.to_string()) }).unwrap();
}

#[test]
fn default_forget_days_is_30() {
    assert_eq!(DEFAULT_FORGET_DAYS, 30);
}

#[test]
fn last_browsed_returns_none_for_unknown_folder() {
    let c = Catalog::new();
    assert_eq!(c.last_browsed("/unknown"), None);
}

#[test]
fn set_browsed_then_last_browsed_some() {
    let mut c = Catalog::new();
    set_browsed(&mut c, "/photos", "2024-01-01T00:00:00Z");
    assert_eq!(c.last_browsed("/photos"), Some("2024-01-01T00:00:00Z"));
}

#[test]
fn browsed_folders_empty_initially() {
    let c = Catalog::new();
    assert!(c.browsed_folders().is_empty());
}

#[test]
fn browsed_folders_after_set() {
    let mut c = Catalog::new();
    set_browsed(&mut c, "/a", "2024-01-01T00:00:00Z");
    set_browsed(&mut c, "/b", "2024-01-02T00:00:00Z");
    assert_eq!(c.browsed_folders().len(), 2);
    assert_eq!(c.last_browsed("/a"), Some("2024-01-01T00:00:00Z"));
    assert_eq!(c.last_browsed("/b"), Some("2024-01-02T00:00:00Z"));
}

#[test]
fn forget_plan_empty_catalog() {
    let c = Catalog::new();
    let plan = c.forget_local_plan("2024-01-01T00:00:00Z", DEFAULT_FORGET_DAYS, &|_| false);
    assert_eq!(plan.local, 0);
    assert!(plan.evict.is_empty());
    assert_eq!(plan.kept_recent, 0);
    assert_eq!(plan.kept_touched, 0);
    assert_eq!(plan.kept_in_use, 0);
    assert!(plan.stamp.is_empty());
    assert!(plan.unstamp.is_empty());
}

#[test]
fn forget_plan_no_browsed_time_noops() {
    // No local photos exist, so nothing is stamped or unstamped even with no browsed times.
    let c = Catalog::new();
    let plan = c.forget_local_plan("2024-01-01T00:00:00Z", DEFAULT_FORGET_DAYS, &|_| false);
    assert_eq!(plan.local, 0);
    assert!(plan.evict.is_empty());
    assert_eq!(plan.kept_recent, 0);
    assert_eq!(plan.kept_touched, 0);
    assert_eq!(plan.kept_in_use, 0);
    assert!(plan.stamp.is_empty());
    assert!(plan.unstamp.is_empty());
}

#[test]
fn forget_plan_days_zero_noops() {
    // With days = 0, even an old folder should not be considered old, but the
    // folder's timestamp is still unstamped because no local records remain in it.
    let mut c = Catalog::new();
    set_browsed(&mut c, "/old", "2023-01-01T00:00:00Z");
    let plan = c.forget_local_plan("2024-01-01T00:00:00Z", 0, &|_| false);
    assert_eq!(plan.local, 0);
    assert!(plan.evict.is_empty());
    assert_eq!(plan.kept_recent, 0);
    assert_eq!(plan.kept_touched, 0);
    assert_eq!(plan.kept_in_use, 0);
    assert!(plan.stamp.is_empty());
    assert_eq!(plan.unstamp, vec!["/old".to_string()]);
}

#[test]
fn forget_plan_invalid_now_noops() {
    let mut c = Catalog::new();
    set_browsed(&mut c, "/old", "2023-01-01T00:00:00Z");
    let plan = c.forget_local_plan("not-a-date", DEFAULT_FORGET_DAYS, &|_| false);
    assert_eq!(plan.local, 0);
    assert!(plan.evict.is_empty());
    assert_eq!(plan.kept_recent, 0);
    assert_eq!(plan.kept_touched, 0);
    assert_eq!(plan.kept_in_use, 0);
    assert!(plan.stamp.is_empty());
    assert_eq!(plan.unstamp, vec!["/old".to_string()]);
}

#[test]
fn forget_plan_unstamps_empty_browsed_folder() {
    let mut c = Catalog::new();
    set_browsed(&mut c, "/empty", "2023-01-01T00:00:00Z");
    let plan = c.forget_local_plan("2024-01-01T00:00:00Z", DEFAULT_FORGET_DAYS, &|_| false);
    assert_eq!(plan.local, 0);
    assert!(plan.evict.is_empty());
    assert_eq!(plan.unstamp, vec!["/empty".to_string()]);
}

#[test]
fn forget_plan_ops_construction() {
    let plan = ForgetPlan {
        local: 1,
        evict: vec![PhotoId(1)],
        kept_recent: 0,
        kept_touched: 0,
        kept_in_use: 0,
        stamp: vec!["/stamped".to_string()],
        unstamp: vec!["/unstamped".to_string()],
        now: "2024-01-01T00:00:00Z".to_string(),
    };
    let ops = plan.ops();
    assert_eq!(ops.len(), 3);

    match &ops[0] {
        Op::SetBrowsed { folder, at } => {
            assert_eq!(folder, "/stamped");
            assert_eq!(at, &Some("2024-01-01T00:00:00Z".to_string()));
        }
        other => panic!("expected SetBrowsed, got {:?}", other),
    }

    match &ops[1] {
        Op::Batch { ops: batch } => {
            assert_eq!(batch.len(), 1);
            match &batch[0] {
                Op::RemovePhoto { id } => assert_eq!(*id, PhotoId(1)),
                other => panic!("expected RemovePhoto, got {:?}", other),
            }
        }
        other => panic!("expected Batch, got {:?}", other),
    }

    match &ops[2] {
        Op::SetBrowsed { folder, at } => {
            assert_eq!(folder, "/unstamped");
            assert_eq!(at, &None);
        }
        other => panic!("expected SetBrowsed, got {:?}", other),
    }
}

#[test]
fn forget_plan_ops_apply_unstamps() {
    let mut c = Catalog::new();
    set_browsed(&mut c, "/empty", "2023-01-01T00:00:00Z");
    let plan = c.forget_local_plan("2024-01-01T00:00:00Z", DEFAULT_FORGET_DAYS, &|_| false);
    let ops = plan.ops();
    for op in ops {
        c.apply(op).unwrap();
    }
    assert_eq!(c.last_browsed("/empty"), None);
}

#[test]
fn photo_ids_remove_photo_collects_id() {
    let op = Op::RemovePhoto { id: PhotoId(42) };
    let mut ids = Vec::new();
    op.photo_ids(&mut |id| ids.push(id));
    assert_eq!(ids, vec![PhotoId(42)]);
}

#[test]
fn photo_ids_set_rating_collects_id() {
    let op = Op::SetRating { id: PhotoId(7), rating: 5 };
    let mut ids = Vec::new();
    op.photo_ids(&mut |id| ids.push(id));
    assert_eq!(ids, vec![PhotoId(7)]);
}

#[test]
fn photo_ids_set_album_photos_collects_ids() {
    let op = Op::SetAlbumPhotos { id: AlbumId(1), photos: vec![PhotoId(1), PhotoId(2), PhotoId(3)] };
    let mut ids = Vec::new();
    op.photo_ids(&mut |id| ids.push(id));
    assert_eq!(ids, vec![PhotoId(1), PhotoId(2), PhotoId(3)]);
}

#[test]
fn photo_ids_set_album_cover_collects_id() {
    let op = Op::SetAlbumCover { id: AlbumId(2), cover: Some(PhotoId(9)) };
    let mut ids = Vec::new();
    op.photo_ids(&mut |id| ids.push(id));
    assert_eq!(ids, vec![PhotoId(9)]);
}

#[test]
fn photo_ids_batch_collects_nested_ids() {
    let batch = Op::Batch {
        ops: vec![
            Op::RemovePhoto { id: PhotoId(10) },
            Op::SetRating { id: PhotoId(20), rating: 3 },
            Op::SetAlbumPhotos { id: AlbumId(5), photos: vec![PhotoId(30)] },
        ],
    };
    let mut ids = Vec::new();
    batch.photo_ids(&mut |id| ids.push(id));
    // Order should follow the nesting: first batch ops in order, then nested photos.
    assert_eq!(ids, vec![PhotoId(10), PhotoId(20), PhotoId(30)]);
}

#[test]
fn photo_ids_set_browsed_collects_nothing() {
    let op = Op::SetBrowsed { folder: "/x".to_string(), at: Some("2024-01-01T00:00:00Z".to_string()) };
    let mut ids = Vec::new();
    op.photo_ids(&mut |id| ids.push(id));
    assert!(ids.is_empty());
}

#[test]
fn photo_ids_set_label_name_collects_nothing() {
    // SetLabelName references no photo.
    // ColorLabel is not imported; use a simple invalid? We need a valid label variant.
    // We'll use a variant that exists: ColorLabel::Green.
    use lightcraft_catalog::ColorLabel;
    let op = Op::SetLabelName { label: ColorLabel::Green, name: None };
    let mut ids = Vec::new();
    op.photo_ids(&mut |id| ids.push(id));
    assert!(ids.is_empty());
}
