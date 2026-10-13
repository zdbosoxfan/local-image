//! Shared, dependency-free process isolation for the two engine command fuzzers.
use serde_json::{Value, json};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

pub fn params() -> Vec<Value> {
    vec![
        json!({}),
        json!({"x": -9, "y": -9, "radius": 0, "amount": 0, "angle": 0, "opacity": 0, "scale": 0, "levels": 0, "tolerance": 0, "gamma": 0, "layer": 999999, "channel": 999, "index": 999999, "count": 0}),
        json!({"x": 1e5, "y": -1e5, "radius": 300, "amount": 300, "angle": 1e5, "opacity": -100, "scale": -5, "gamma": -1}),
        json!({"points": [], "from": [0, 0], "to": [0, 0], "stops": [], "matrix": [0, 0, 0, 0, 0, 0], "colors": [], "name": "", "mode": "", "style": ""}),
    ]
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let name = format!("local-image-panic-hunt-{}-{stamp}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed));
        let root = std::env::temp_dir().join(name);
        // create_dir refuses a preexisting path; never reuse or remove somebody else's files.
        fs::create_dir(&root).unwrap();
        for sub in ["output", "models", "cache", "config", "data"] {
            fs::create_dir(root.join(sub)).unwrap();
        }
        Self(root)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove the completed fuzz case's temporary directory");
    }
}

struct Worker(Child);

impl Drop for Worker {
    fn drop(&mut self) {
        // Child has no Drop implementation. Reap even on a parent assertion/I/O failure.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn run(test: &str, id: &str, p: &Value) -> Result<(), String> {
    run_timeout(test, id, p, Duration::from_secs(4))
}

pub fn run_timeout(test: &str, id: &str, p: &Value, timeout: Duration) -> Result<(), String> {
    let root = TempDir::new();
    let log_path = root.0.join("worker.log");
    // Files avoid pipe backpressure deadlocking the child before the deadline.
    let log = File::create(&log_path).unwrap();
    let mut worker = Worker(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--ignored", "--nocapture", "--test-threads=1"])
            .env("LOCAL_IMAGE_PANIC_HUNT_ID", id)
            .env("LOCAL_IMAGE_PANIC_HUNT_PARAMS", p.to_string())
            .env("LOCAL_IMAGE_SEG_MODELS", root.0.join("models"))
            .env("XDG_CACHE_HOME", root.0.join("cache"))
            .env("XDG_CONFIG_HOME", root.0.join("config"))
            .env("XDG_DATA_HOME", root.0.join("data"))
            .env("TMPDIR", &root.0)
            .env("TMP", &root.0)
            .env("TEMP", &root.0)
            .env("RAYON_NUM_THREADS", "3")
            .env("LIGHTCRAFT_MEMORY_MB", "256")
            .env("RUST_BACKTRACE", "0")
            .current_dir(&root.0)
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .map_err(|e| format!("could not spawn worker: {e}"))?,
    );
    let start = Instant::now();
    let result = loop {
        match worker.0.try_wait() {
            Ok(Some(status)) => break if status.success() { Ok(()) } else { Err(format!("worker failed ({status})")) },
            Err(e) => break Err(format!("could not wait for worker: {e}")),
            Ok(None) if start.elapsed() >= timeout => break Err(format!("hung (>{timeout:?}); worker killed and reaped")),
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
        }
    };
    // Stop the child before reading its log or deleting its working directory.
    drop(worker);
    let log = fs::read_to_string(log_path).unwrap_or_default();
    let path = root.0.clone();
    drop(root);
    assert!(!path.exists(), "the worker's files must be removed after success, panic or timeout");
    match result {
        Ok(()) if log.contains("PANIC_HUNT_COMPLETE") => Ok(()),
        Ok(()) => Err(format!("worker did not run the requested case:\n{log}")),
        Err(why) => Err(format!("{why}\n{log}")),
    }
}

pub fn child(run: impl FnOnce(&str, &Value, &Path)) {
    let Ok(id) = std::env::var("LOCAL_IMAGE_PANIC_HUNT_ID") else { return };
    let p: Value = serde_json::from_str(&std::env::var("LOCAL_IMAGE_PANIC_HUNT_PARAMS").unwrap()).unwrap();
    let root = std::env::current_dir().unwrap().canonicalize().unwrap();
    // macOS resolves /var to /private/var for cwd while environment paths retain /var.
    assert_eq!(std::env::temp_dir().canonicalize().unwrap(), root);
    for (key, sub) in [("LOCAL_IMAGE_SEG_MODELS", "models"), ("XDG_CACHE_HOME", "cache"), ("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data")] {
        let path = PathBuf::from(std::env::var_os(key).unwrap());
        assert_eq!(path.parent().unwrap().canonicalize().unwrap(), root);
        assert_eq!(path.file_name().unwrap(), sub);
    }
    // Detect panics even when Session::execute (Library) or a job catches them internally.
    // This hook is process-local: the parent and other tests' hooks are untouched.
    let panics = Arc::new(AtomicUsize::new(0));
    let seen = panics.clone();
    let old = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        seen.fetch_add(1, Ordering::Relaxed);
        old(info);
    }));
    match id.as_str() {
        "fixture.ok" => fs::write(root.join("fixture.txt"), b"confined").unwrap(),
        "fixture.panic" => {
            let _ = std::panic::catch_unwind(|| panic!("fixture caught panic"));
        }
        "fixture.hang" => {
            println!("fixture started");
            std::thread::sleep(Duration::from_secs(30));
        }
        _ => run(&id, &p, &root),
    }
    assert_eq!(panics.load(Ordering::Relaxed), 0, "command panicked, including internally caught panics");
    println!("PANIC_HUNT_COMPLETE");
}

pub fn check_runner(test: &str) {
    assert!(run(test, "fixture.ok", &json!({})).is_ok());
    let e = run("nonexistent_panic_hunt_test", "fixture.ok", &json!({})).unwrap_err();
    assert!(e.contains("did not run the requested case"), "{e}");
    let e = run(test, "fixture.panic", &json!({})).unwrap_err();
    assert!(e.contains("fixture caught panic"), "{e}");
    let e = run_timeout(test, "fixture.hang", &json!({}), Duration::from_secs(1)).unwrap_err();
    assert!(e.contains("fixture started") && e.contains("worker killed and reaped"), "{e}");
}
