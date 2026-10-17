//! Separate process: no earlier test may finish initialization before this exit-barrier test.
mod support;
#[test]
fn quiesce_waits_for_warm_up_before_any_render() {
    let _gpu = support::lock();
    assert!(lightcraft_gpu::enabled(), "real GPU required but GPU rendering is disabled: {:?}", lightcraft_gpu::unavailable_reason());
    lightcraft_gpu::warm_up();
    // No sleep: the barrier must cover the interval before the compiler thread is scheduled.
    assert!(lightcraft_gpu::quiesce(std::time::Duration::from_secs(120)));
    assert!(lightcraft_gpu::ready(), "warm-up must finish before the exit barrier returns");
    let adapter = lightcraft_gpu::adapter_name().unwrap_or_else(|| panic!("real GPU required: {:?}", lightcraft_gpu::unavailable_reason()));
    eprintln!("warm-up finished on {adapter}");
}
