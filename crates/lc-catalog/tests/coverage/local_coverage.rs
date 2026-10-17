use lightcraft_catalog::{
    AlbumId, Catalog, Op, PhotoId,
    local::{DEFAULT_FORGET_DAYS, ForgetPlan},
};

fn set_browsed(c: &mut Catalog, folder: &str, at: &str) {
    c.apply(Op::SetBrowsed { folder: folder.to_string(), at: Some(at.to_string()) }).unwrap();
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
    assert!(plan.unstamp.is_empty());
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
    let plan =
        ForgetPlan { local: 1, evict: vec![PhotoId(1)], kept_recent: 0, kept_touched: 0, kept_in_use: 0, unstamp: vec!["/unstamped".to_string()] };
    let ops = plan.ops();
    assert_eq!(ops.len(), 2);

    match &ops[0] {
        Op::Batch { ops: batch } => {
            assert_eq!(batch.len(), 1);
            match &batch[0] {
                Op::RemovePhoto { id } => assert_eq!(*id, PhotoId(1)),
                other => panic!("expected RemovePhoto, got {:?}", other),
            }
        }
        other => panic!("expected Batch, got {:?}", other),
    }

    match &ops[1] {
        Op::SetBrowsed { folder, at } => {
            assert_eq!(folder, "/unstamped");
            assert_eq!(at, &None);
        }
        other => panic!("expected SetBrowsed, got {:?}", other),
    }
}

#[test]
fn photo_ids_set_album_cover_collects_id() {
    let op = Op::SetAlbumCover { id: AlbumId(2), cover: Some(PhotoId(9)) };
    let mut ids = Vec::new();
    op.photo_ids(&mut |id| ids.push(id));
    assert_eq!(ids, vec![PhotoId(9)]);
}

#[test]
fn photo_ids_set_browsed_collects_nothing() {
    let op = Op::SetBrowsed { folder: "/x".to_string(), at: Some("2024-01-01T00:00:00Z".to_string()) };
    let mut ids = Vec::new();
    op.photo_ids(&mut |id| ids.push(id));
    assert!(ids.is_empty());
}
