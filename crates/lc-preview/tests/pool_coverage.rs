use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, mpsc};
use std::time::{Duration, Instant};

use lightcraft_preview::JobPool;

#[test]
fn pool_with_zero_threads_begins_idle() {
    let pool = JobPool::<usize, i32>::new(0);
    assert_eq!(pool.queued(), 0);
    assert!(!pool.is_queued(1));
    assert!(pool.try_recv().is_none());
}

#[test]
fn submit_inline_with_zero_workers_runs_job() {
    let mut pool = JobPool::<usize, i32>::new(0);
    pool.submit(1, 100, 5, Box::new(|| 42));
    assert_eq!(pool.queued(), 1);
    assert_eq!(pool.run_inline(1), 1);
    let done = pool.try_recv().unwrap();
    assert_eq!(done.slot, 1);
    assert_eq!(done.key, 100);
    assert_eq!(done.result, 42);
    assert_eq!(done.ms, 0.0);
    assert_eq!(pool.queued(), 0);
}

#[test]
fn submit_replaces_queued_job_for_slot() {
    let mut pool = JobPool::<usize, i32>::new(0);
    pool.submit(7, 1, 0, Box::new(|| 1));
    pool.submit(7, 2, 0, Box::new(|| 2));
    assert_eq!(pool.queued(), 1);
    assert_eq!(pool.run_inline(1), 1);
    let done = pool.try_recv().unwrap();
    assert_eq!(done.slot, 7);
    assert_eq!(done.key, 2);
    assert_eq!(done.result, 2);
}

#[test]
fn different_slots_queue_independently() {
    let mut pool = JobPool::<usize, i32>::new(0);
    pool.submit(1, 10, 0, Box::new(|| 11));
    pool.submit(2, 20, 0, Box::new(|| 22));
    assert_eq!(pool.queued(), 2);
    assert_eq!(pool.run_inline(2), 2);
    let mut results: Vec<_> = (0..2).map(|_| pool.try_recv().unwrap()).collect();
    results.sort_by_key(|d| d.slot);
    assert_eq!(results[0].slot, 1);
    assert_eq!(results[1].slot, 2);
    assert_eq!(results[0].result, 11);
    assert_eq!(results[1].result, 22);
}

#[test]
fn priority_high_runs_first() {
    let mut pool = JobPool::<usize, i32>::new(0);
    pool.submit(1, 0, 1, Box::new(|| 1));
    pool.submit(2, 0, 10, Box::new(|| 2));
    pool.submit(3, 0, 5, Box::new(|| 3));
    assert_eq!(pool.run_inline(1), 1);
    let first = pool.try_recv().unwrap();
    assert_eq!(first.slot, 2);
    assert_eq!(pool.run_inline(1), 1);
    let second = pool.try_recv().unwrap();
    assert_eq!(second.slot, 3);
    assert_eq!(pool.run_inline(1), 1);
    let third = pool.try_recv().unwrap();
    assert_eq!(third.slot, 1);
}

#[test]
fn same_priority_newest_first() {
    let mut pool = JobPool::<usize, i32>::new(0);
    pool.submit(1, 0, 5, Box::new(|| 1));
    pool.submit(2, 0, 5, Box::new(|| 2));
    assert_eq!(pool.run_inline(1), 1);
    let done = pool.try_recv().unwrap();
    assert_eq!(done.slot, 2);
    assert_eq!(done.result, 2);
}

#[test]
fn reprioritize_changes_order() {
    let mut pool = JobPool::<usize, i32>::new(0);
    pool.submit(1, 0, 1, Box::new(|| 1));
    pool.submit(2, 0, 2, Box::new(|| 2));
    let dropped = pool.reprioritize(|slot, _| if *slot == 1 { Some(10) } else { Some(1) });
    assert!(dropped.is_empty());
    assert_eq!(pool.run_inline(1), 1);
    let done = pool.try_recv().unwrap();
    assert_eq!(done.slot, 1);
}

#[test]
fn reprioritize_drops_queued_job() {
    let mut pool = JobPool::<usize, i32>::new(0);
    pool.submit(1, 10, 0, Box::new(|| 1));
    pool.submit(2, 20, 0, Box::new(|| 2));
    let dropped = pool.reprioritize(|slot, _| if *slot == 1 { None } else { Some(1) });
    assert_eq!(dropped, vec![1]);
    assert_eq!(pool.queued(), 1);
    assert!(!pool.is_queued(1));
    assert!(pool.is_queued(2));
    assert_eq!(pool.run_inline(1), 1);
    let done = pool.try_recv().unwrap();
    assert_eq!(done.slot, 2);
}

#[test]
fn reprioritize_empty_pool_is_noop() {
    let pool = JobPool::<usize, i32>::new(0);
    let dropped = pool.reprioritize(|_, _| Some(1));
    assert!(dropped.is_empty());
    assert_eq!(pool.queued(), 0);
}

#[test]
fn run_inline_zero_does_nothing() {
    let mut pool = JobPool::<usize, i32>::new(0);
    pool.submit(1, 0, 0, Box::new(|| 1));
    assert_eq!(pool.run_inline(0), 0);
    assert_eq!(pool.queued(), 1);
}

#[test]
fn run_inline_runs_up_to_n_jobs() {
    let mut pool = JobPool::<usize, i32>::new(0);
    pool.submit(1, 0, 0, Box::new(|| 1));
    pool.submit(2, 0, 0, Box::new(|| 2));
    pool.submit(3, 0, 0, Box::new(|| 3));
    assert_eq!(pool.run_inline(2), 2);
    assert_eq!(pool.queued(), 1);
    assert_eq!(pool.run_inline(1), 1);
    assert_eq!(pool.queued(), 0);
}

#[test]
fn try_recv_consumes_result() {
    let mut pool = JobPool::<usize, i32>::new(0);
    pool.submit(1, 0, 0, Box::new(|| 1));
    pool.run_inline(1);
    assert!(pool.try_recv().is_some());
    assert!(pool.try_recv().is_none());
}

#[test]
fn worker_pool_runs_job_and_reports_ms() {
    let mut pool = JobPool::<usize, i32>::new(1);
    pool.submit(1, 7, 0, Box::new(|| 42));
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if let Some(done) = pool.try_recv() {
            assert_eq!(done.slot, 1);
            assert_eq!(done.key, 7);
            assert_eq!(done.result, 42);
            assert!(done.ms >= 0.0);
            break;
        }
        assert!(Instant::now() < deadline, "timed out waiting for worker");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(pool.queued(), 0);
}

#[test]
fn worker_pool_with_two_workers_runs_jobs_concurrently() {
    let mut pool = JobPool::<usize, i32>::new(2);
    let barrier = Arc::new(Barrier::new(2)); // two jobs must meet, so sequential execution would deadlock
    for slot in 1..=2 {
        let b = barrier.clone();
        pool.submit(
            slot,
            slot as u64,
            0,
            Box::new(move || {
                b.wait();
                slot as i32
            }),
        );
    }

    let deadline = Instant::now() + Duration::from_secs(1);
    let mut received = 0;
    while received < 2 {
        if pool.try_recv().is_some() {
            received += 1;
            continue;
        }
        assert!(Instant::now() < deadline, "timed out waiting for concurrent workers");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn submit_while_job_running_queues_second_job_for_same_slot() {
    let mut pool = JobPool::<usize, i32>::new(1);
    let started = Arc::new(AtomicBool::new(false));
    let started_clone = started.clone();
    pool.submit(
        1,
        1,
        0,
        Box::new(move || {
            started_clone.store(true, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(100));
            1
        }),
    );

    let deadline = Instant::now() + Duration::from_secs(1);
    while !started.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline, "job did not start");
        std::thread::sleep(Duration::from_millis(5));
    }

    pool.submit(1, 2, 0, Box::new(|| 2));

    let mut results = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(2);
    while results.len() < 2 {
        if let Some(done) = pool.try_recv() {
            results.push(done.result);
            continue;
        }
        assert!(Instant::now() < deadline, "timed out waiting for both jobs");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(results, vec![1, 2]);
}

#[test]
fn drop_with_queued_jobs_completes_within_timeout() {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut pool = JobPool::<usize, i32>::new(0);
        pool.submit(1, 1, 0, Box::new(|| 1));
        drop(pool);
        tx.send(()).unwrap();
    });
    assert!(rx.recv_timeout(Duration::from_secs(1)).is_ok(), "drop did not complete within timeout");
}

#[test]
fn drop_with_running_job_waits_for_completion() {
    let (finish_tx, finish_rx) = mpsc::channel();
    let started = Arc::new(AtomicBool::new(false));
    let started_clone = started.clone();
    let finish_tx_for_job = finish_tx.clone();

    let mut pool = JobPool::<usize, i32>::new(1);
    pool.submit(
        1,
        1,
        0,
        Box::new(move || {
            started_clone.store(true, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(100));
            finish_tx_for_job.send(42).unwrap();
            42
        }),
    );

    let deadline = Instant::now() + Duration::from_secs(1);
    while !started.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline, "job did not start");
        std::thread::sleep(Duration::from_millis(5));
    }

    drop(pool);

    assert_eq!(finish_rx.recv_timeout(Duration::from_secs(1)).unwrap(), 42, "running job did not finish before pool drop returned");
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
struct Slot(u32);

#[test]
fn custom_slot_type_works() {
    let mut pool = JobPool::<Slot, String>::new(0);
    pool.submit(Slot(1), 99, 3, Box::new(|| "hello".to_string()));
    pool.submit(Slot(2), 100, 3, Box::new(|| "world".to_string()));
    assert_eq!(pool.run_inline(1), 1);
    let done = pool.try_recv().unwrap();
    assert_eq!(done.slot, Slot(2));
    assert_eq!(done.result, "world");
    assert_eq!(pool.run_inline(1), 1);
    let done = pool.try_recv().unwrap();
    assert_eq!(done.slot, Slot(1));
    assert_eq!(done.result, "hello");
}

#[test]
fn default_thread_pool_runs_job() {
    let mut pool = JobPool::<usize, i32>::new(JobPool::<usize, i32>::default_threads());
    pool.submit(1, 1, 0, Box::new(|| 7));
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if let Some(done) = pool.try_recv() {
            assert_eq!(done.result, 7);
            break;
        }
        assert!(Instant::now() < deadline, "timed out waiting for default worker pool");
        std::thread::sleep(Duration::from_millis(5));
    }
}
