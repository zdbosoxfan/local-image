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
/// cropped and flipped.
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
            // the separable path's two passes may each use the slack across a diagonal edge
            let slack = 2.0 * li_raster::resample::ANTIRING_SLACK + 1e-3;
            for (i, p) in out.data.iter().enumerate() {
                for c in 0..3 {
                    assert!(p[c] >= wall[c] * (1.0 - slack) && p[c] <= window[c] * (1.0 + slack), "angle {angle}, crop {crop}, pixel {i}: {p:?}");
                }
            }
        }
    }
    assert!(errors.pop().is_none());
    assert!(!crate::ctx::failed());
}

/// The device resample matches the CPU on every path: separable crops and flips, straighten, and
/// the warp (perspective, lens profile, camera-plane CA from the profile or the sliders,
/// vignetting) including the widened kernel where the perspective minifies, at reduced and full
/// size.
#[test]
fn every_resample_path_matches_the_cpu() {
    let gpu_device = crate::test_device().unwrap_or_else(|| panic!("real GPU required: {:?}", crate::unavailable_reason()));
    let gpu = &*gpu_device;
    let _scope = crate::ctx::RenderScope::new(gpu);
    let errors = crate::ctx::ErrorScopes::push(gpu);
    let mut cx = Cx::new(gpu);
    let src = Arc::new(RgbFrame::from_fn(613, 409, |x, y| {
        let (u, v) = (x as f32, y as f32);
        [0.1 + 0.08 * (u * 0.9).sin() * (v * 0.7).cos(), 0.2 + 0.1 * ((u + v) * 0.45).sin(), if (x / 37 + y / 23) % 2 == 0 { 0.02 } else { 1.5 }]
    }));
    let forward = li_color::Mat3([[0.8, 0.15, 0.05], [0.1, 0.8, 0.1], [0.05, 0.2, 0.75]]);
    let mut radial = lightcraft_develop::EmbeddedRadial {
        count: 2,
        vig_count: 2,
        center: lightcraft_geom::Point::new(0.5, 0.5),
        radius: 0.8,
        ..Default::default()
    };
    radial.knots[1] = 1.0;
    radial.scales[0][..2].copy_from_slice(&[1.004, 1.003]);
    radial.scales[1][..2].fill(1.0);
    radial.scales[2][..2].copy_from_slice(&[0.997, 0.996]);
    radial.vig_knots[1] = 1.0;
    radial.attenuation[..2].copy_from_slice(&[1.0, 0.6]);
    let lens = lightcraft_develop::EmbeddedLens { radial: Some(radial), ..Default::default() };
    let info = SourceInfo { raw: true, lens: Some(lens), lens_camera_to_working: Some(forward), ..Default::default() };
    let mut cases: Vec<(&str, DevelopSettings)> = Vec::new();
    let mut s = DevelopSettings::default();
    s.crop.geometry.rect = lightcraft_geom::Rect::new(0.1013, 0.07, 0.8817, 0.93);
    s.crop.flip_v = true;
    cases.push(("separable crop + flip", s.clone()));
    s.crop.geometry.angle = 2.5;
    cases.push(("straightened", s));
    let mut s = DevelopSettings { orientation: Orientation::Rotate90, ..Default::default() };
    s.geometry.vertical = -80.0;
    s.geometry.horizontal = 25.0;
    s.optics.lens_profile = true;
    s.optics.ca_blue = 40.0;
    cases.push(("perspective + lens + CA", s));
    let mut s = DevelopSettings::default();
    s.optics.ca_red = 60.0;
    s.optics.ca_blue = -40.0;
    s.crop.geometry.angle = 1.0;
    cases.push(("manual CA on camera planes", s));
    for (name, s) in &cases {
        for req in [RenderRequest::fit(613, 613), RenderRequest::fit(400, 400), RenderRequest::fit(250, 250)] {
            let (gpu_out, cpu_out) = sampled(&mut cx, &src, &info, s, &req, 2e-4);
            assert_eq!((gpu_out.width, gpu_out.height), (cpu_out.width, cpu_out.height), "{name}");
        }
    }
    assert!(errors.pop().is_none());
    assert!(!crate::ctx::failed());
}
