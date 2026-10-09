use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use lightcraft_engine::catalog::PhotoId;
use lightcraft_engine::enhance::{
    AiHost, AiStroke, CANCELLED, Enhance, JobCtl, JobKind, Outcome, RemoveDone, RemoveEngine, RemoveRequest, RemoveResult,
};

fn done_outcome() -> Outcome {
    Outcome::Remove(RemoveDone {
        stroke: AiStroke { points: vec![], polygon: vec![], size: 0.01, feather: 50.0, opacity: 100.0, mask: None },
        key: "k".into(),
        source: "s".into(),
        rect: [0.0; 4],
        engine: "m".into(),
        seed: 1,
        geometry: "g".into(),
    })
}

fn wait_until<F: Fn() -> bool>(f: F) {
    let start = Instant::now();
    while !f() {
        if start.elapsed() > Duration::from_secs(1) {
            panic!("timed out waiting for condition");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

struct MockHost;

impl AiHost for MockHost {
    fn remove_engines(&self) -> Vec<RemoveEngine> {
        vec![RemoveEngine { key: "mock".into(), label: "Mock".into(), problem: None }]
    }

    fn remove(&self, req: &RemoveRequest, _ctl: &JobCtl) -> Result<RemoveResult, String> {
        Ok(RemoveResult { rgb: req.rgb.clone(), alpha: req.mask.clone() })
    }
}

#[test]
fn enhance_default_is_empty() {
    let mut e = Enhance::default();
    assert!(e.host.is_none());
    assert!(e.jobs().is_empty());
    assert!(!e.busy());
    assert_eq!(e.cancel(None), 0);
    assert!(e.take_finished().is_empty());
    assert_eq!(e.running_for(PhotoId(1)).count(), 0);
    assert!(e.job(1).is_none());
}

#[test]
fn job_ctl_defaults() {
    let ctl = JobCtl::default();
    assert_eq!(ctl.progress(), 0.0);
    assert_eq!(ctl.message(), "");
    assert!(!ctl.cancelled());
    assert_eq!(ctl.check(), Ok(()));
}

#[test]
fn job_ctl_set_clamps_finite_and_non_finite_floats() {
    let ctl = JobCtl::default();

    ctl.set(-0.5, "negative");
    assert_eq!(ctl.progress(), 0.0);

    ctl.set(1.5, "over");
    assert_eq!(ctl.progress(), 1.0);

    ctl.set(0.25, "quarter");
    assert_eq!(ctl.progress(), 0.25);

    ctl.set(f32::NEG_INFINITY, "neg inf");
    assert_eq!(ctl.progress(), 0.0);

    ctl.set(f32::INFINITY, "pos inf");
    assert_eq!(ctl.progress(), 1.0);

    ctl.set(f32::NAN, "nan");
    let _ = ctl.progress(); // must not panic
}

#[test]
fn job_ctl_message_reports_latest() {
    let ctl = JobCtl::default();
    ctl.set(0.0, "");
    assert_eq!(ctl.message(), "");

    ctl.set(0.5, "half");
    assert_eq!(ctl.message(), "half");

    ctl.set(0.9, "done");
    assert_eq!(ctl.message(), "done");
}

#[test]
fn job_ctl_cancel_is_idempotent_and_check_returns_cancelled() {
    let ctl = JobCtl::default();
    assert_eq!(ctl.check(), Ok(()));

    ctl.cancel();
    assert!(ctl.cancelled());
    assert_eq!(ctl.check(), Err(CANCELLED.to_string()));

    ctl.cancel();
    assert!(ctl.cancelled());
}

#[test]
fn wait_true_job_runs_synchronously_and_reports() {
    let mut e = Enhance::default();
    let j = e.spawn(
        PhotoId(7),
        JobKind::Regenerate { spot: 0 },
        "Regenerate",
        true,
        Box::new(|ctl| {
            ctl.set(0.5, "half");
            Ok(done_outcome())
        }),
    );

    assert!(j.finished());
    assert!(!e.busy());
    assert_eq!(e.running_for(PhotoId(7)).count(), 0);
    assert_eq!((j.ctl.progress(), j.ctl.message()), (0.5, "half".to_string()));

    let mut done = e.take_finished();
    assert_eq!(done.len(), 1);
    let (job, res) = done.remove(0);
    assert_eq!(job.id, j.id);
    assert_eq!(res, Ok(done_outcome()));
    assert!(e.jobs().is_empty());
}

#[test]
fn wait_true_remove_job_json_uses_camel_case() {
    let stroke = AiStroke { points: vec![], polygon: vec![], size: 0.01, feather: 50.0, opacity: 100.0, mask: None };
    let kind = JobKind::Remove { stroke };

    let mut e = Enhance::default();
    let j = e.spawn(PhotoId(2), kind, "Remove", true, Box::new(|_| Ok(done_outcome())));

    let v = j.json();
    assert_eq!(v["job"]["kind"].as_str(), Some("remove"));
    assert_eq!(v["photo"].as_u64(), Some(2));
    assert_eq!(v["running"].as_bool(), Some(false));
    assert_eq!(v["cancelled"].as_bool(), Some(false));
    assert_eq!(v["job"]["stroke"]["size"].as_f64(), Some(0.01));
    assert_eq!(v["job"]["stroke"]["opacity"].as_f64(), Some(100.0));
}

#[test]
fn wait_true_regenerate_job_json_uses_camel_case() {
    let kind = JobKind::Regenerate { spot: 3 };

    let mut e = Enhance::default();
    let j = e.spawn(PhotoId(3), kind, "Regenerate", true, Box::new(|_| Ok(done_outcome())));

    let v = j.json();
    assert_eq!(v["job"]["kind"].as_str(), Some("regenerate"));
    assert_eq!(v["job"]["spot"].as_u64(), Some(3));
    assert_eq!(v["photo"].as_u64(), Some(3));
}

#[test]
fn wait_true_job_returning_error_is_reported() {
    let mut e = Enhance::default();
    let j = e.spawn(PhotoId(4), JobKind::Regenerate { spot: 0 }, "x", true, Box::new(|_| Err("boom".to_string())));

    assert!(j.finished());
    let done = e.take_finished();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].1, Err("boom".to_string()));
}

#[test]
fn wait_true_job_panic_is_converted_to_error() {
    let mut e = Enhance::default();
    let j = e.spawn(PhotoId(5), JobKind::Regenerate { spot: 0 }, "panic", true, Box::new(|_| panic!("boom")));

    assert!(j.finished());
    let done = e.take_finished();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].1, Err("the AI job failed unexpectedly".to_string()));
}

#[test]
fn background_job_runs_and_reports_running_until_finished() {
    let mut e = Enhance::default();
    let (start_tx, start_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();

    let j = e.spawn(
        PhotoId(10),
        JobKind::Regenerate { spot: 0 },
        "background",
        false,
        Box::new(move |_| {
            start_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Ok(done_outcome())
        }),
    );

    start_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(e.running_for(PhotoId(10)).count() >= 1);
    assert!(e.busy());
    assert_eq!(j.json()["running"].as_bool(), Some(true));

    release_tx.send(()).unwrap();
    wait_until(|| j.finished());

    assert!(j.finished());
    assert!(!e.busy());
    let done = e.take_finished();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].1, Ok(done_outcome()));
}

#[test]
fn jobs_are_returned_oldest_first() {
    let mut e = Enhance::default();

    let j1 = e.spawn(PhotoId(1), JobKind::Regenerate { spot: 0 }, "first", true, Box::new(|_| Ok(done_outcome())));
    let j2 = e.spawn(PhotoId(2), JobKind::Regenerate { spot: 0 }, "second", true, Box::new(|_| Ok(done_outcome())));

    let jobs = e.jobs().to_vec();
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0].id, j1.id);
    assert_eq!(jobs[1].id, j2.id);
}

#[test]
fn job_lookup_by_id_works() {
    let mut e = Enhance::default();
    let j = e.spawn(PhotoId(3), JobKind::Regenerate { spot: 0 }, "lookup", true, Box::new(|_| Ok(done_outcome())));

    assert!(e.job(j.id).is_some());
    assert!(e.job(9999).is_none());
}

#[test]
fn cancel_none_cancels_all_running_jobs() {
    let mut e = Enhance::default();
    let (start1_tx, start1_rx) = mpsc::channel();
    let (start2_tx, start2_rx) = mpsc::channel();

    let j1 = e.spawn(
        PhotoId(11),
        JobKind::Regenerate { spot: 0 },
        "a",
        false,
        Box::new(move |ctl| {
            start1_tx.send(()).unwrap();
            while !ctl.cancelled() {
                std::thread::sleep(Duration::from_millis(2));
            }
            ctl.check().map(|_| done_outcome())
        }),
    );
    let j2 = e.spawn(
        PhotoId(12),
        JobKind::Regenerate { spot: 0 },
        "b",
        false,
        Box::new(move |ctl| {
            start2_tx.send(()).unwrap();
            while !ctl.cancelled() {
                std::thread::sleep(Duration::from_millis(2));
            }
            ctl.check().map(|_| done_outcome())
        }),
    );

    start1_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    start2_rx.recv_timeout(Duration::from_secs(1)).unwrap();

    assert_eq!(e.cancel(None), 2);
    wait_until(|| j1.finished() && j2.finished());

    let done = e.take_finished();
    assert_eq!(done.len(), 2);
    for (_, res) in done {
        assert_eq!(res, Err(CANCELLED.to_string()));
    }
}

#[test]
fn cancel_specific_does_not_cancel_others() {
    let mut e = Enhance::default();
    let (start1_tx, start1_rx) = mpsc::channel();
    let (start2_tx, start2_rx) = mpsc::channel();

    let j1 = e.spawn(
        PhotoId(21),
        JobKind::Regenerate { spot: 0 },
        "a",
        false,
        Box::new(move |ctl| {
            start1_tx.send(()).unwrap();
            while !ctl.cancelled() {
                std::thread::sleep(Duration::from_millis(2));
            }
            ctl.check().map(|_| done_outcome())
        }),
    );
    let j2 = e.spawn(
        PhotoId(22),
        JobKind::Regenerate { spot: 0 },
        "b",
        false,
        Box::new(move |ctl| {
            start2_tx.send(()).unwrap();
            while !ctl.cancelled() {
                std::thread::sleep(Duration::from_millis(2));
            }
            ctl.check().map(|_| done_outcome())
        }),
    );

    start1_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    start2_rx.recv_timeout(Duration::from_secs(1)).unwrap();

    assert_eq!(e.cancel(Some(j1.id)), 1);
    wait_until(|| j1.finished());

    assert!(!j2.finished());
    assert!(e.running_for(PhotoId(22)).any(|j| j.id == j2.id));

    e.cancel(Some(j2.id));
    wait_until(|| j2.finished());
}

#[test]
fn take_finished_only_removes_finished_jobs() {
    let mut e = Enhance::default();
    let (start_tx, start_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();

    let j_finished = e.spawn(PhotoId(30), JobKind::Regenerate { spot: 0 }, "done", true, Box::new(|_| Ok(done_outcome())));
    let j_running = e.spawn(
        PhotoId(31),
        JobKind::Regenerate { spot: 0 },
        "blocked",
        false,
        Box::new(move |_| {
            start_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Ok(done_outcome())
        }),
    );

    start_rx.recv_timeout(Duration::from_secs(1)).unwrap();

    let taken = e.take_finished();
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].0.id, j_finished.id);

    assert!(e.jobs().iter().any(|j| j.id == j_running.id));
    assert!(!e.jobs().iter().any(|j| j.id == j_finished.id));

    release_tx.send(()).unwrap();
    wait_until(|| j_running.finished());

    let taken = e.take_finished();
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].0.id, j_running.id);
}

#[test]
fn ai_host_trait_can_be_implemented_and_used() {
    let host = MockHost;
    let engines = host.remove_engines();
    assert_eq!(engines.len(), 1);
    assert_eq!(engines[0].key, "mock");
    assert_eq!(engines[0].problem, None);

    let ctl = JobCtl::default();
    let req = RemoveRequest { width: 2, height: 3, rgb: vec![[0u8; 3]; 6], mask: vec![255u8; 6], engine: "mock".into(), seed: 42 };
    let res = host.remove(&req, &ctl).unwrap();
    assert_eq!(res.rgb.len(), 6);
    assert_eq!(res.alpha.len(), 6);

    let mut e = Enhance::default();
    e.host = Some(Arc::new(host));
    assert!(e.host.is_some());
}
