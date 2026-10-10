//! Separate process: no earlier test may finish initialization before this exit-barrier test.
#[test]
fn quiesce_waits_for_warm_up_before_any_render() {
    if !lightcraft_gpu::enabled() {
        assert!(std::env::var_os("LOCAL_IMAGE_REQUIRE_GPU").is_none(), "hardware GPU required but disabled");
        eprintln!("skipped: GPU disabled");
        return;
    }
    lightcraft_gpu::warm_up();
    // No sleep: the barrier must cover the interval before the compiler thread is scheduled.
    assert!(lightcraft_gpu::quiesce(std::time::Duration::from_secs(120)));
    assert!(lightcraft_gpu::ready(), "warm-up must finish before the exit barrier returns");
    if let Some(adapter) = lightcraft_gpu::adapter_name() {
        eprintln!("warm-up finished on {adapter}");
    } else {
        assert!(std::env::var_os("LOCAL_IMAGE_REQUIRE_GPU").is_none(), "hardware GPU required: {:?}", lightcraft_gpu::unavailable_reason());
        eprintln!("no adapter: {:?}", lightcraft_gpu::unavailable_reason());
    }
}
