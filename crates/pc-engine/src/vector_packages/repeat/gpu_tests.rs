//! Strict GPU gate for the same repeat source projection used by the app hooks. The full
//! pointer/preview/release app gate is supplied in the UI integration handoff, separately.
use super::*;
#[test]
#[ignore = "owner speed rules: integration RTX 5090 Vulkan gate"]
fn repeat19_gpu_projection_source_edit_and_expand_parity_rtx5090() {
    let _gpu_gate = li_testkit::gpu::lock();
    let (device, queue) = crate::gpu_test_support::compositor_device_adapter_limits("repeat19");
    let mut comp = photocraft_gpu::Compositor::new(&device);
    let (mut s, id) = fixture();
    let group = create(&mut s, id);
    for step in 0..3 {
        let c = Capture::new(&s, &[group]).unwrap();
        let spec = RepeatSpec::Grid { rows: 2, cols: 3, spacing: [4. + f64::from(step), 2.] };
        let projected = if step == 2 {
            s.execute("vector.repeat.expand", c.params()).unwrap();
            project_document(&s.active().unwrap().doc).unwrap()
        } else {
            preview(&s, &c, &spec).unwrap()
        };
        let cpu = photocraft_compose::flatten(&projected);
        let gpu = photocraft_gpu::render_to_vec(&mut comp, &device, &queue, &projected, projected.bounds()).unwrap();
        for (a, b) in cpu.px.iter().zip(gpu) {
            for k in 0..4 {
                let ca = if k == 3 { a[k] } else { a[k] * a[3] };
                let cb = if k == 3 { b[k] } else { b[k] * b[3] };
                assert!((ca - cb).abs() <= 1. / 255.);
            }
        }
        if step == 0 {
            s.execute(
                "shape.applyPaths",
                json!({"edits":[{"layer":id.0,"path":Path::new(vec![Subpath::polygon(&[(10.,12.),(24.,12.),(24.,18.),(10.,18.)])])}]}),
            )
            .unwrap();
        }
        if step == 1 {
            let mut p = c.params();
            p["spec"] = serde_json::to_value(spec).unwrap();
            s.execute("vector.repeat.set", p).unwrap();
        }
    }
}
