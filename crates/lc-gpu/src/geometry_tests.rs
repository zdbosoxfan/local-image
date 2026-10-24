//! Device tests for the geometry resample (`sample`) against `Frame::sample` on the CPU.
use super::*;

/// Sample `src` for `s` at `req` on the device and on the CPU; returns both (GPU, CPU) after
/// checking they agree within `tol` (the GPU computes sample positions in f32, so across a steep
/// edge the tolerance scales with the step).
fn sampled(cx: &mut Cx<'_>, src: &Arc<RgbFrame>, info: &SourceInfo, s: &DevelopSettings, req: &RenderRequest, tol: f32) -> (RgbFrame, RgbFrame) {
    let plan = lightcraft_pipeline::plan(src, info, s, req);
    let cpu = plan.frame.sample(src, plan.w, plan.h);
    let uploaded = Arc::new(cx.gpu.upload(rgb_words(src)));
    let result = sample(cx, src, uploaded, &plan);
    let gpu = cx.read_rgb(&result, plan.w, plan.h);
    for (i, (a, b)) in gpu.data.iter().zip(&cpu.data).enumerate() {
        for c in 0..3 {
            assert!(
                (a[c] - b[c]).abs() <= tol,
                "{:?} pixel {i} channel {c}: GPU {a:?}, CPU {b:?}",
                plan.frame.sample_plan(src.width, src.height, plan.w, plan.h).mode
            );
        }
    }
    (gpu, cpu)
}

/// A window six to eight stops over the wall, rendered reduced: neither renderer's prefilter rings
/// the wall beside the window (it went below zero: a black outline), unrotated, straightened,
/// cropped and flipped. Across a diagonal edge the second pass sees the first pass's moderate
/// contrast and keeps a little of the kernel's overshoot, a percent or two at most.
#[test]
fn reduced_renders_do_not_ring_bright_windows_on_the_device() {
    let gpu_device = crate::test_device().unwrap_or_else(|| panic!("real GPU required: {:?}", crate::unavailable_reason()));
    let gpu = &*gpu_device;
    let _scope = crate::ctx::RenderScope::new(gpu);
    let errors = crate::ctx::ErrorScopes::push(gpu);
    let mut cx = Cx::new(gpu);
    let (wall, window) = ([0.05f32, 0.04, 0.03], [0.18 * 256.0, 0.9 * 64.0, 1.2 * 100.0]);
    let src = Arc::new(RgbFrame::from_fn(1203, 301, |x, y| if x + y / 7 < 640 { wall } else { window }));
    let info = SourceInfo { raw: true, ..Default::default() };
    for (angle, crop, flip) in [(0.0, false, false), (2.0, false, false), (0.0, true, true), (-3.0, true, false)] {
        let mut s = DevelopSettings::default();
        s.crop.geometry.angle = angle;
        s.crop.flip_h = flip;
        if crop {
            s.crop.geometry.rect = lightcraft_geom::Rect::new(0.13, 0.07, 0.91, 0.88);
        }
        for req in [RenderRequest::fit(401, 401), RenderRequest::fit(250, 250)] {
            let (out, _) = sampled(&mut cx, &src, &info, &s, &req, 1e-2);
            for (i, p) in out.data.iter().enumerate() {
                for c in 0..3 {
                    assert!(p[c] >= wall[c] * 0.98 && p[c] <= window[c] * 1.02, "angle {angle}, crop {crop}, pixel {i}: {p:?}");
                }
            }
        }
    }
    assert!(errors.pop().is_none());
    assert!(!crate::ctx::failed());
}
