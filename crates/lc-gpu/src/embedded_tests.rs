//! Device tests for camera radial tables through the actual Develop geometry path.
use super::*;
use lightcraft_develop::{EmbeddedLens, EmbeddedRadial, EmbeddedWarp};
use lightcraft_geom::Point;

#[test]
fn embedded_radial_gpu_matches_cpu_geometry_and_gain() {
    // A missing/software adapter cannot count as a successful numerical check.
    let gpu_device = crate::test_device().expect("embedded radial equivalence requires a hardware GPU");
    let gpu = &*gpu_device;
    eprintln!("embedded radial device: {:?}", crate::adapter_name());
    let _scope = crate::ctx::RenderScope::new(gpu);
    let errors = crate::ctx::ErrorScopes::push(gpu);
    let src = Arc::new(Rgb32f::from_fn(129, 87, |x, y| {
        let u = x as f32 / 129.0;
        let v = y as f32 / 87.0;
        [0.12 + 0.4 * u, 0.16 + 0.3 * v, 0.08 + 0.2 * (u + v) + 0.01 * (u * 12.0).sin()]
    }));
    let mut radial = EmbeddedRadial { count: 4, vig_count: 3, center: Point::new(0.41, 0.56), radius: 0.7, ..Default::default() };
    radial.knots[..4].copy_from_slice(&[0.08, 0.19, 0.38, 0.55]);
    radial.scales[0][..4].copy_from_slice(&[0.99, 0.985, 0.971, 0.946]);
    radial.scales[1][..4].copy_from_slice(&[0.992, 0.988, 0.977, 0.95]);
    radial.scales[2][..4].copy_from_slice(&[0.995, 0.991, 0.982, 0.957]);
    radial.vig_knots[..3].copy_from_slice(&[0.06, 0.31, 0.52]);
    radial.attenuation[..3].copy_from_slice(&[0.99, 0.93, 0.82]);
    let mut cx = Cx::new(gpu);
    let uploaded = Arc::new(gpu.upload(rgb_words(&src)));
    let mut max_error = 0.0f32;
    let mut compared = 0;
    // Nonuniform knots, both clamped ends, channel-specific scales, reciprocal
    // attenuation, one-knot tables, and geometry-only/gain-only tables.
    for (count, vig_count) in [(4, 3), (1, 1), (0, 3), (4, 0)] {
        let lens = EmbeddedLens { radial: Some(EmbeddedRadial { count, vig_count, ..radial }), ..Default::default() };
        for orientation in [Orientation::Normal, Orientation::Rotate90, Orientation::FlipH, Orientation::Transverse] {
            for strength in [0.0, 50.0, 100.0, 200.0] {
                let mut settings = DevelopSettings { orientation, ..Default::default() };
                settings.optics.lens_profile = true;
                settings.optics.profile_distortion = strength;
                settings.optics.profile_vignetting = strength;
                settings.optics.distortion = -8.0;
                settings.optics.ca_red = 25.0;
                settings.geometry.horizontal = 3.0;
                let info = SourceInfo {
                    lens: Some(lens),
                    lens_camera_to_working: Some(li_color::Mat3([[0.8, 0.15, 0.05], [0.1, 0.8, 0.1], [0.05, 0.2, 0.75]])),
                    ..Default::default()
                };
                let req = RenderRequest::fit(149, 149);
                let plan = lightcraft_pipeline::plan(&src, &info, &settings, &req);
                let expected = plan.frame.sample(&src, plan.w, plan.h);
                let result = sample(&mut cx, &src, uploaded.clone(), &plan);
                let actual = cx.read::<f32>(&result, plan.w * plan.h * 3);
                for (index, (a, b)) in actual.iter().zip(expected.data.iter().flatten()).enumerate() {
                    let error = (a - b).abs();
                    max_error = max_error.max(error);
                    assert!(
                        error < 6e-5,
                        "{orientation:?}, counts {count}/{vig_count}, {strength}%, component {index}: GPU {a}, CPU {b}, error {error}"
                    );
                    compared += 1;
                }
            }
        }
    }
    eprintln!("embedded radial GPU/CPU: {compared} components, max absolute error {max_error:.8}");
    assert!(errors.pop().is_none());
    assert!(!crate::ctx::failed());
}

#[test]
fn embedded_camera_basis_gpu_corrects_physically_misregistered_planes() {
    let gpu_device = crate::test_device().expect("camera-plane CA equivalence requires a hardware GPU");
    let gpu = &*gpu_device;
    eprintln!("camera-plane CA device: {:?}", crate::adapter_name());
    let _scope = crate::ctx::RenderScope::new(gpu);
    let errors = crate::ctx::ErrorScopes::push(gpu);
    let (w, h) = (129, 87);
    let centre = Point::new(w as f64 * 0.5, h as f64 * 0.5);
    let scales = [1.04, 1.0, 0.96];
    let forward = li_color::Mat3([[0.8, 0.15, 0.05], [0.1, 0.8, 0.1], [0.05, 0.2, 0.75]]);
    let brightness = |x: f64, y: f64| 0.2 + 0.15 * x / w as f64 + 0.1 * y / h as f64;
    // Construct independently shifted CAMERA planes of an achromatic ramp,
    // then mix them into working RGB as the raw loader does. The physical
    // ground truth is the original ramp transformed by the camera matrix.
    let src = Arc::new(Rgb32f::from_fn(w, h, |x, y| {
        let camera =
            scales.map(|scale| brightness(centre.x + (x as f64 + 0.5 - centre.x) / scale, centre.y + (y as f64 + 0.5 - centre.y) / scale) as f32);
        forward.apply_f32(camera)
    }));
    let mut radial = EmbeddedRadial { count: 2, center: Point::new(0.5, 0.5), radius: 1.0, ..Default::default() };
    radial.knots[1] = 1.0;
    for (channel, scale) in radial.scales.iter_mut().zip(scales) {
        channel[..2].fill(scale);
    }
    let lens = EmbeddedLens { radial: Some(radial), ..Default::default() };
    let info = SourceInfo { lens: Some(lens), lens_camera_to_working: Some(forward), ..Default::default() };
    let mut settings = DevelopSettings::default();
    settings.optics.lens_profile = true;
    let req = RenderRequest::fit(w, h);
    let plan = lightcraft_pipeline::plan(&src, &info, &settings, &req);
    assert_eq!((plan.w, plan.h), (w, h));
    assert!(plan.frame.warp.as_ref().is_some_and(|warp| warp.camera_basis.is_some()));
    let expected_cpu = plan.frame.sample(&src, w, h);
    let mut cx = Cx::new(gpu);
    let uploaded = Arc::new(gpu.upload(rgb_words(&src)));
    let result = sample(&mut cx, &src, uploaded, &plan);
    let actual = cx.read::<f32>(&result, w * h * 3);
    let mut max_gpu_error = 0.0f32;
    let mut max_cpu_error = 0.0f32;
    // Keep the test independent of edge-clamp policy; all three source
    // positions are strictly inside the image in this region.
    for y in 5..h - 5 {
        for x in 5..w - 5 {
            let truth = forward.apply_f32([brightness(x as f64 + 0.5, y as f64 + 0.5) as f32; 3]);
            for (ch, expected) in truth.into_iter().enumerate() {
                let gpu_error = (actual[(y * w + x) * 3 + ch] - expected).abs();
                let cpu_error = (expected_cpu.data[y * w + x][ch] - expected).abs();
                max_gpu_error = max_gpu_error.max(gpu_error);
                max_cpu_error = max_cpu_error.max(cpu_error);
                assert!(gpu_error < 2e-6 && cpu_error < 2e-6, "{x},{y} channel {ch}: GPU residual {gpu_error}, CPU residual {cpu_error}");
            }
        }
    }
    eprintln!("camera-plane CA vs physical ramp: max GPU {max_gpu_error:.8}, max CPU {max_cpu_error:.8}");
    assert!(errors.pop().is_none());
    assert!(!crate::ctx::failed());
}

#[test]
fn embedded_dng_fisheye_gpu_matches_independent_mapping() {
    let gpu_device = crate::test_device().expect("DNG fisheye equivalence requires a hardware GPU");
    let gpu = &*gpu_device;
    eprintln!("DNG fisheye device: {:?}", crate::adapter_name());
    let _scope = crate::ctx::RenderScope::new(gpu);
    let errors = crate::ctx::ErrorScopes::push(gpu);
    let (w, h) = (129, 87);
    let camera = Rgb32f::from_fn(w, h, |x, y| {
        let u = x as f32 / w as f32;
        let v = y as f32 / h as f32;
        [0.12 + 0.3 * u, 0.14 + 0.2 * v, 0.11 + 0.1 * (u + v)]
    });
    let forward = li_color::Mat3([[0.8, 0.15, 0.05], [0.1, 0.8, 0.1], [0.05, 0.2, 0.75]]);
    let src = Arc::new(camera.map(|rgb| forward.apply_f32(rgb)));
    let planes = [[1.01, 0.02, -0.005, 0.001, 0.0, 0.0], [1.0, 0.02, -0.005, 0.001, 0.0, 0.0], [0.99, 0.02, -0.005, 0.001, 0.0, 0.0]];
    let warp = EmbeddedWarp { fisheye: true, planes, center: Point::new(0.5, 0.5), radius: 0.6 };
    let lens = EmbeddedLens { warp: Some(warp), ..Default::default() };
    let info = SourceInfo { lens: Some(lens), lens_camera_to_working: Some(forward), ..Default::default() };
    let req = RenderRequest::fit(w, h);
    let centre = Point::new(w as f64 * 0.5, h as f64 * 0.5);
    let radius = 0.6 * w.max(h) as f64;
    let mut cx = Cx::new(gpu);
    let uploaded = Arc::new(gpu.upload(rgb_words(&src)));
    let mut max_gpu_error = 0.0f32;
    let mut max_cpu_error = 0.0f32;
    for strength in [0.0, 0.5, 1.0, 2.0] {
        let mut settings = DevelopSettings::default();
        settings.optics.lens_profile = true;
        settings.optics.profile_distortion = strength * 100.0;
        let plan = lightcraft_pipeline::plan(&src, &info, &settings, &req);
        assert_eq!((plan.w, plan.h), (w, h));
        let cpu = plan.frame.sample(&src, w, h);
        let result = sample(&mut cx, &src, uploaded.clone(), &plan);
        let actual = cx.read::<f32>(&result, w * h * 3);
        for y in 0..h {
            for x in 0..w {
                // Independent DNG opcode-2 equation, including the exact
                // optical-centre limit and distinct coefficients per plane.
                let (dx, dy) = (x as f64 + 0.5 - centre.x, y as f64 + 0.5 - centre.y);
                let r = dx.hypot(dy) / radius;
                let theta = r.atan();
                let t2 = theta * theta;
                let mut truth = [0.0; 3];
                for (ch, coefficients) in planes.iter().enumerate() {
                    let [k0, k1, k2, k3, _, _] = *coefficients;
                    let scale = if r == 0.0 { k0 } else { theta / r * (k0 + k1 * t2 + k2 * t2.powi(2) + k3 * t2.powi(3)) };
                    let scale = 1.0 + strength * (scale - 1.0);
                    truth[ch] = camera.sample_bilinear((centre.x + dx * scale) as f32, (centre.y + dy * scale) as f32)[ch];
                }
                let truth = forward.apply_f32(truth);
                for (ch, expected) in truth.into_iter().enumerate() {
                    let gpu_error = (actual[(y * w + x) * 3 + ch] - expected).abs();
                    let cpu_error = (cpu.data[y * w + x][ch] - expected).abs();
                    max_gpu_error = max_gpu_error.max(gpu_error);
                    max_cpu_error = max_cpu_error.max(cpu_error);
                    assert!(
                        gpu_error < 2e-6 && cpu_error < 2e-6,
                        "{x},{y}, {strength}, channel {ch}: GPU residual {gpu_error}, CPU residual {cpu_error}"
                    );
                }
            }
        }
    }
    eprintln!("DNG fisheye vs independent mapping: max GPU {max_gpu_error:.8}, max CPU {max_cpu_error:.8}");
    assert!(errors.pop().is_none());
    assert!(!crate::ctx::failed());
}
