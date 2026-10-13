//! Each case gets a fresh process: the process-wide profile and Rayon pools freeze at startup.
use lightcraft_sysmem::{Overrides, ThreadSource, background_pool, init};
use rayon::prelude::*;
use std::collections::HashSet;
use std::process::Command;
use std::sync::Barrier;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn compute_and_background_pools_honor_the_budget() {
    let Ok(case) = std::env::var("LI_THREAD_TEST_CASE") else {
        for case in ["one", "four", "auto", "preference", "environment", "rayon"] {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "compute_and_background_pools_honor_the_budget", "--nocapture"])
                .env("LI_THREAD_TEST_CASE", case)
                .env_remove("LOCAL_IMAGE_THREADS")
                .env_remove("RAYON_NUM_THREADS");
            if case == "environment" {
                command.env("LOCAL_IMAGE_THREADS", "4").env("RAYON_NUM_THREADS", "2");
            } else if case == "rayon" {
                command.env("RAYON_NUM_THREADS", "3");
            }
            let output = command.output().unwrap();
            assert!(output.status.success(), "{case}: {}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }
        return;
    };
    // Environment changes belong in the child launcher, before any Rayon call.
    let profile = init(Overrides {
        threads: match case.as_str() {
            "one" => Some(1),
            "four" => Some(4),
            "preference" => Some(usize::MAX),
            _ => None,
        },
        ..Overrides::default()
    });
    assert!(profile.pool_built, "startup must own the global pool");
    let expected = match case.as_str() {
        "one" => 1,
        "four" => 4.min(profile.hardware.logical),
        "preference" => profile.hardware.logical,
        "environment" => 4,
        "rayon" => 3,
        _ => profile.threads_auto,
    };
    assert_eq!(profile.threads, expected);
    assert_eq!(
        profile.threads_from,
        match case.as_str() {
            "auto" => ThreadSource::Auto,
            "environment" => ThreadSource::LocalImageThreads,
            "rayon" => ThreadSource::RayonNumThreads,
            _ => ThreadSource::Setting,
        }
    );
    assert_eq!(rayon::current_num_threads(), expected);
    assert_eq!(init(Overrides { threads: Some(1), ..Overrides::default() }).threads, expected, "profile stays frozen");
    let barrier = Barrier::new(expected);
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let workers = rayon::broadcast(|_| {
        let n = active.fetch_add(1, Ordering::SeqCst) + 1;
        peak.fetch_max(n, Ordering::SeqCst);
        barrier.wait();
        barrier.wait();
        active.fetch_sub(1, Ordering::SeqCst);
        std::thread::current().name().unwrap_or_default().to_owned()
    });
    assert_eq!(peak.load(Ordering::SeqCst), expected, "workers really run concurrently");
    assert_eq!(workers.iter().collect::<HashSet<_>>().len(), expected);
    assert!(workers.iter().all(|name| name.starts_with("li-compute-")));
    let serial: u64 = (0..1_000_000u64).map(|v| v * 3 + 7).sum();
    assert_eq!((0..1_000_000u64).into_par_iter().map(|v| v * 3 + 7).sum::<u64>(), serial);
    let pool = background_pool().unwrap();
    assert_eq!(pool.current_num_threads(), expected);
    let background = pool.broadcast(|_| std::thread::current().name().unwrap_or_default().to_owned());
    assert_eq!(background.iter().collect::<HashSet<_>>().len(), expected);
    assert!(background.iter().all(|name| name.starts_with("li-background-")));
    assert_eq!(pool.install(|| (0..1_000_000u64).into_par_iter().map(|v| v * 3 + 7).sum::<u64>()), serial);
    // Occupy every background worker while the caller and foreground pool continue servicing
    // work. A channel deadline makes a broken pool fail instead of hanging the whole suite.
    let (started, ready) = std::sync::mpsc::channel();
    let release = (std::sync::Mutex::new(false), std::sync::Condvar::new());
    std::thread::scope(|scope| {
        scope.spawn(|| {
            pool.broadcast(|_| {
                started.send(()).unwrap();
                let (lock, wake) = &release;
                drop(wake.wait_while(lock.lock().unwrap(), |done| !*done).unwrap());
            })
        });
        let all_ready = (0..expected).all(|_| ready.recv_timeout(std::time::Duration::from_secs(10)).is_ok());
        let foreground = rayon::broadcast(|_| std::thread::current().name().unwrap_or_default().to_owned());
        let (lock, wake) = &release;
        *lock.lock().unwrap() = true;
        wake.notify_all();
        assert!(all_ready, "background workers did not start within deadline");
        assert!(foreground.iter().all(|name| name.starts_with("li-compute-")), "background work must not capture the foreground pool");
    });
    println!(
        "{case}: {} concurrent compute workers; {} background workers; identical serial/parallel result",
        peak.load(Ordering::SeqCst),
        background.len()
    );
}
