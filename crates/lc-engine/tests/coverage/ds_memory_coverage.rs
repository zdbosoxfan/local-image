use lightcraft_engine::Session;
use lightcraft_engine::memory::{
    HeapUsage, Usage, WorkGate, cache_share, default_budget, gpu_usage, heap_stats, in_background, is_background, release, set_heap_stats,
    set_release_hook, work_gate,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

static RELEASE_HOOK_CALLS: AtomicUsize = AtomicUsize::new(0);

fn increment_release_count() {
    RELEASE_HOOK_CALLS.fetch_add(1, Ordering::SeqCst);
}

fn fixed_heap_stats() -> HeapUsage {
    HeapUsage { current: 111, peak: 222 }
}

#[test]
fn default_budget_respects_bounds() {
    let b = default_budget();
    assert!(b >= 256 << 20, "budget should be at least 256 MiB, got {b}");
    assert!(b <= 3usize << 29, "budget should be at most 1.5 GiB, got {b}");
}

#[test]
fn cache_share_is_half() {
    assert_eq!(cache_share(100), 50);
    assert_eq!(cache_share(0), 0);
    assert_eq!(cache_share(usize::MAX), usize::MAX / 2);
}

#[test]
fn work_gate_new_usage_zero_and_limit() {
    let gate = WorkGate::new(123);
    assert_eq!(gate.usage(), (0, 123));
}

#[test]
fn work_gate_set_limit_updates_usage() {
    let gate = WorkGate::new(100);
    gate.set_limit(200);
    assert_eq!(gate.usage(), (0, 200));
}

#[test]
fn work_gate_acquire_and_release_updates_usage() {
    let gate = WorkGate::new(100);
    let permit = gate.acquire(30);
    assert_eq!(gate.usage(), (30, 100));
    drop(permit);
    assert_eq!(gate.usage(), (0, 100));
}

#[test]
fn work_gate_acquire_zero_is_noop() {
    let gate = WorkGate::new(10);
    let p1 = gate.acquire(5);
    let p0 = gate.acquire(0);
    assert_eq!(gate.usage(), (5, 10));
    drop(p0);
    assert_eq!(gate.usage(), (5, 10));
    drop(p1);
    assert_eq!(gate.usage(), (0, 10));
}

#[test]
fn work_gate_first_holder_may_exceed_limit() {
    let gate = WorkGate::new(10);
    let permit = gate.acquire(100);
    assert_eq!(gate.usage(), (100, 10));
    drop(permit);
    assert_eq!(gate.usage(), (0, 10));
}

#[test]
fn work_gate_urgent_acquire_counts_and_releases() {
    let gate = WorkGate::new(10);
    let permit = gate.acquire_urgent(50);
    assert_eq!(gate.usage(), (50, 10));
    drop(permit);
    assert_eq!(gate.usage(), (0, 10));
}

#[test]
fn work_gate_acquire_blocks_until_release() {
    let gate = Arc::new(WorkGate::new(10));
    let first = gate.acquire(8);
    assert_eq!(gate.usage(), (8, 10));

    let gate_clone = Arc::clone(&gate);
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let permit = gate_clone.acquire(5); // should block until `first` dropped
        tx.send(()).unwrap();
        drop(permit);
    });

    thread::sleep(Duration::from_millis(30));
    assert!(rx.try_recv().is_err(), "worker should still be blocked");

    drop(first);
    rx.recv_timeout(Duration::from_secs(1)).expect("worker should acquire after release");
    handle.join().unwrap();
    assert_eq!(gate.usage(), (0, 10)); // after worker drops its permit
}

#[test]
fn work_gate_set_limit_wakes_waiters() {
    let gate = Arc::new(WorkGate::new(10));
    let first = gate.acquire(8);

    let gate_clone = Arc::clone(&gate);
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let permit = gate_clone.acquire(5); // blocks initially because 8+5 > 10
        tx.send(()).unwrap();
        drop(permit);
    });

    thread::sleep(Duration::from_millis(30));
    assert!(rx.try_recv().is_err(), "worker should still be blocked");

    gate.set_limit(20); // now 8+5=13 <= 20, worker should wake
    rx.recv_timeout(Duration::from_secs(1)).expect("worker should acquire after limit increase");
    handle.join().unwrap();

    drop(first);
    assert_eq!(gate.usage(), (0, 20));
}

#[test]
fn in_background_sets_and_restores_thread_local() {
    assert!(!is_background());
    let result = in_background(|| {
        assert!(is_background());
        42
    });
    assert_eq!(result, 42);
    assert!(!is_background());
}

#[test]
fn in_background_nested_restores_previous_state() {
    assert!(!is_background());
    let result = in_background(|| {
        assert!(is_background());
        let inner = in_background(|| {
            assert!(is_background());
            7
        });
        assert_eq!(inner, 7);
        assert!(is_background());
        inner + 1
    });
    assert_eq!(result, 8);
    assert!(!is_background());
}

#[test]
fn usage_new_and_default() {
    let u = Usage::new(5, 42);
    assert_eq!(u.count, 5);
    assert_eq!(u.bytes, 42);
    let d = Usage::default();
    assert_eq!(d.count, 0);
    assert_eq!(d.bytes, 0);
    assert_eq!(u, Usage { count: 5, bytes: 42 });
    let c = u; // Copy
    assert_eq!(c, u);
}

#[test]
fn heap_stats_hook_round_trip() {
    set_heap_stats(fixed_heap_stats);
    let stats = heap_stats().expect("heap stats should be Some after set");
    assert_eq!(stats.current, 111);
    assert_eq!(stats.peak, 222);
}

#[test]
fn release_hook_called_once_per_call() {
    set_release_hook(increment_release_count);
    let before = RELEASE_HOOK_CALLS.load(Ordering::SeqCst);
    release();
    assert_eq!(RELEASE_HOOK_CALLS.load(Ordering::SeqCst), before + 1);
    release();
    assert_eq!(RELEASE_HOOK_CALLS.load(Ordering::SeqCst), before + 2);
}

#[test]
fn gpu_usage_fields_are_consistent() {
    let g = gpu_usage();
    assert!(g.allocated >= g.pooled + g.retired, "allocated must include pooled and retired: {g:?}");
}

#[test]
fn session_memory_report_sums_engine_bytes() {
    let session = Session::new();
    let report = session.memory_report();
    let sum = report.thumb_sources.bytes + report.preview_sources.bytes + report.full_source.bytes + report.rendered.bytes;
    assert_eq!(report.engine_bytes, sum);
}

#[test]
fn session_set_memory_budget_updates_report() {
    let mut session = Session::new();
    let requested = 128 << 20; // 128 MiB
    let actual = session.set_memory_budget(requested);
    assert!(actual >= 64 << 20, "budget must be at least 64 MiB");
    assert_eq!(actual, requested, "requested budget should be accepted");

    let report = session.memory_report();
    assert_eq!(report.budget, actual);
    assert_eq!(report.cache_budget, cache_share(actual));
    assert_eq!(report.work_limit, actual / 4);
    let sum = report.thumb_sources.bytes + report.preview_sources.bytes + report.full_source.bytes + report.rendered.bytes;
    assert_eq!(report.engine_bytes, sum);
}

#[test]
fn work_gate_static_limit_matches_budget_quarter() {
    let b = lightcraft_engine::memory::budget();
    assert_eq!(work_gate().usage().1, b / 4);
}
