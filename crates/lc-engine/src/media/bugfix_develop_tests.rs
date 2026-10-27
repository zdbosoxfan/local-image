use super::*;
#[test]
fn bugfix_develop_panicking_preview_returns_failure_and_pool_survives() {
    let mut pool = lightcraft_preview::JobPool::new(0);
    let job = QuickJob {
        request_id: 17,
        photo: PhotoId(1),
        key: 23,
        cached: vec![],
        embedded: Some(("bad.raw".into(), Arc::new(|_, _| panic!("damaged decoder")), 64)),
        small: None,
    };
    pool.submit(0, 23, 1, Box::new(move || job.run()));
    pool.submit(1, 24, 0, Box::new(|| QuickJob { request_id: 18, photo: PhotoId(2), key: 24, cached: vec![], embedded: None, small: None }.run()));
    assert_eq!(pool.run_inline(2), 2);
    let bad = pool.try_recv().unwrap();
    assert_eq!((bad.result.request_id, bad.result.key), (17, 23));
    assert!(bad.result.rendered.err().unwrap().contains("damaged decoder"));
    assert!(pool.try_recv().is_some());
}
