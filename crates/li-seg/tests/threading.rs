//! Exercise tract's real row dispatcher without downloading model weights.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![expect(clippy::allow_attributes_without_reason, reason = "baseline 2026-10-25: unreviewed")]

use std::collections::HashSet;
use std::process::Command;
use std::sync::Mutex;
use tract_linalg::multithread::{Executor, current_tract_executor, par_chunks_mut};

#[test]
fn tract_uses_the_callers_pool_without_extra_workers() {
    if std::env::var_os("LI_TRACT_THREAD_TEST_CHILD").is_none() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tract_uses_the_callers_pool_without_extra_workers", "--nocapture"])
            .env("LI_TRACT_THREAD_TEST_CHILD", "1")
            .env("LOCAL_IMAGE_THREADS", "4")
            .env_remove("LOCAL_IMAGE_SEG_THREADS")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        print!("{}", String::from_utf8_lossy(&output.stdout));
        return;
    }
    lightcraft_sysmem::init(Default::default());
    li_seg::init_executor();
    assert!(matches!(current_tract_executor(), Executor::RayonGlobal));
    let run = |prefix: &str| {
        let seen = Mutex::new(HashSet::new());
        let mut output = vec![0u32; 1024 * 1024];
        let total = output.len();
        par_chunks_mut(&mut output, 1024, total, |row, chunk| {
            seen.lock().unwrap().insert(std::thread::current().name().unwrap_or_default().to_owned());
            // Keep each chunk alive long enough to observe real distribution across workers.
            std::thread::sleep(std::time::Duration::from_millis(2));
            for (i, value) in chunk.iter_mut().enumerate() {
                *value = (row * 1024 + i) as u32 * 3 + 7;
            }
            Ok(())
        })
        .unwrap();
        assert!(output.iter().enumerate().all(|(i, &v)| v == i as u32 * 3 + 7));
        let workers = seen.into_inner().unwrap();
        assert!(workers.len() > 1 && workers.len() <= 4, "{workers:?}");
        assert!(workers.iter().all(|name| name.starts_with(prefix)), "unexpected private pool: {workers:?}");
        println!("tract {prefix}: {} workers, bit-identical output", workers.len());
    };
    run("li-compute-");
    lightcraft_sysmem::in_background_pool(|| run("li-background-"));
}
