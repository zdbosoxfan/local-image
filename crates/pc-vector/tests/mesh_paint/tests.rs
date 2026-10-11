use super::*;
fn mesh(rows: usize, cols: usize) -> Mesh {
    let mut m = Mesh::regular([0., 0., 100., 100.], rows, cols, Appearance::Flat, 0., |_| [0.2, 0.4, 0.6, 1.]).unwrap();
    m.dither = false;
    m
}
fn rectangle() -> Path {
    Path::new(vec![photocraft_doc::Subpath::polygon(&[(0., 0.), (100., 0.), (100., 100.), (0., 100.)])])
}
fn sampler(m: &Mesh) -> CompiledMesh {
    CompiledMesh::new(m, &rectangle(), Rect::new(0, 0, 100, 100)).unwrap()
}
#[test]
fn mesh43_two_by_two_linear_bilinear_oracle() {
    let mut m = mesh(2, 2);
    let colors = [[1., 0., 0., 1.], [0., 1., 0., 1.], [0., 0., 1., 1.], [1., 1., 1., 1.]];
    // Independent oracle: bilinear interpolation of the four outer colours, in linear light.
    for r in 0..=2 {
        for c in 0..=2 {
            let u = c as f32 / 2.;
            let v = r as f32 / 2.;
            let i = m.index(r, c);
            m.points[i].rgba = std::array::from_fn(|ch| {
                if ch == 3 {
                    1.
                } else {
                    linear_to_srgb(colors[0][ch] * (1. - u) * (1. - v) + colors[1][ch] * u * (1. - v) + colors[2][ch] * (1. - u) * v + colors[3][ch] * u * v)
                }
            });
        }
    }
    let p = sampler(&m);
    for (x, y) in [(11.3, 17.1), (32.6, 81.3), (49.9, 50.1), (76.2, 66.7), (90.5, 7.2)] {
        let actual = p.at(x, y);
        let u = x as f32 / 100.;
        let v = y as f32 / 100.;
        for ch in 0..3 {
            let expected = colors[0][ch] * (1. - u) * (1. - v) + colors[1][ch] * u * (1. - v) + colors[2][ch] * (1. - u) * v + colors[3][ch] * u * v;
            assert!((srgb_to_linear(actual[ch]) - expected).abs() <= 1. / 255.);
        }
    }
}
#[test]
fn mesh43_premultiplied_linear_opacity_has_no_transparent_blue_fringe() {
    let mut m = mesh(1, 1);
    m.points[0].rgba = [1., 0., 0., 1.];
    for p in &mut m.points[1..] {
        p.rgba = [0., 0., 1., 1.];
        p.opacity = 0.;
    }
    let p = sampler(&m).at(50., 50.);
    assert!((p[0] - 1.).abs() < 1e-6);
    assert!(p[2] < 1e-6);
    assert!((p[3] - 0.25).abs() < 1e-6);
}
#[test]
fn mesh43_one_handle_changes_only_adjacent_patches_and_exact_boundaries() {
    let mut m = mesh(3, 3);
    let before = m.clone();
    let i = m.index(1, 1);
    m.move_point(i, Some(RIGHT), [45., 28.]).unwrap();
    for r in 0..3 {
        for c in 0..3 {
            let a = before.patch(r, c).unwrap().point(0.4, 0.6);
            let b = m.patch(r, c).unwrap().point(0.4, 0.6);
            if c == 1 && r <= 1 {
                assert!(distance(a, b) > 0.1);
            } else {
                assert!(distance(a, b) < 1e-9);
            }
        }
    }
    let a = m.patch(0, 1).unwrap();
    let b = m.patch(1, 1).unwrap();
    for i in 0..=20 {
        let t = i as f64 / 20.;
        assert!(distance(a.point(t, 1.), b.point(t, 0.)) < 1e-9);
    }
}
fn distance(a: P, b: P) -> f64 {
    length(sub(a, b))
}
#[test]
fn mesh43_shift_insert_preserves_warped_geometry_linear_colour_and_opacity() {
    let mut m = mesh(1, 1);
    m.move_point(0, Some(RIGHT), [30., 12.]).unwrap();
    m.points[0].rgba = [1., 0., 0., 0.8];
    m.points[0].opacity = 0.3;
    m.points[1].rgba = [0., 1., 0., 1.];
    m.points[2].rgba = [0., 0., 1., 0.7];
    let original = m.patch(0, 0).unwrap();
    let (split_u, split_v) = (0.37, 0.62);
    let i = m.insert(0, 0, split_u, split_v, None).unwrap();
    assert_eq!((m.rows, m.cols), (2, 2));
    assert_eq!(i, 4);
    for (u, v) in [(0.15, 0.2), (0.7, 0.8), (0.45, 0.55), (0.9, 0.05)] {
        let (c, fu) = if u < split_u { (0, u / split_u) } else { (1, (u - split_u) / (1. - split_u)) };
        let (r, fv) = if v < split_v { (0, v / split_v) } else { (1, (v - split_v) / (1. - split_v)) };
        let p = m.patch(r, c).unwrap();
        assert!(distance(p.point(fu, fv), original.point(u, v)) < 1e-7);
        for (a, b) in p.linear_rgba(fu, fv).into_iter().zip(original.linear_rgba(u, v)) {
            assert!((a - b).abs() < 2e-6);
        }
    }
}
#[test]
fn mesh43_insert_colour_delete_lines_and_point_handles_follow() {
    let mut m = mesh(1, 1);
    let i = m.insert(0, 0, 0.5, 0.5, Some([1., 0., 0., 1.])).unwrap();
    assert_eq!(m.points[i].rgba, [1., 0., 0., 1.]);
    let h = m.points[i].handles;
    m.move_point(i, None, [60., 55.]).unwrap();
    assert_eq!(m.points[i].handles, h);
    m.delete(i).unwrap();
    assert_eq!((m.rows, m.cols, m.points.len()), (1, 1, 4));
    m.validate().unwrap();
    assert!(m.delete(0).is_err());
}
#[test]
fn mesh43_circle_clip_and_bounds_limited_surface() {
    let m = mesh(2, 2);
    let path = crate::shapes::ellipse(0., 0., 100., 100.);
    let p = CompiledMesh::new(&m, &path, Rect::new(-20, -20, 120, 120)).unwrap();
    let image = p.render_rgba(Rect::new(0, 0, 100, 100)).unwrap();
    assert!(image[0][3].abs() <= 1e-7);
    assert!(image[50 * 100 + 50][3] > 0.99);
    assert!(image[99][3].abs() <= 1e-7);
    let fmt = PixelFormat::new(photocraft_color::ColorMode::Rgb, photocraft_color::SampleType::F32, true);
    let s = p.render(fmt).unwrap();
    assert!(s.tile_count() <= 4);
    assert_eq!(s.rgba(-1, 50)[3], 0.);
}
#[test]
fn mesh43_seeded_shared_gradient_dither_is_deterministic() {
    let mut m = mesh(1, 1);
    m.dither = true;
    let plain = sampler(&mesh(1, 1)).at(20.5, 30.5);
    let p = sampler(&m);
    let a = p.at(20.5, 30.5);
    let noise = (photocraft_color::dither_noise(20, 30) - 0.5) / 255.;
    for ch in 0..3 {
        assert!((a[ch] - plain[ch] - noise).abs() < 1e-6);
    }
    assert_eq!(a, p.at(20.5, 30.5));
    m.seed = 738;
    let b = sampler(&m);
    assert_ne!(a, b.at(20.5, 30.5));
    assert_eq!(b.at(20.5, 30.5), sampler(&m).at(20.5, 30.5));
}
#[test]
fn mesh43_tiled_untiled_and_transformed_oracle_at_09_scales() {
    for scale in [1., 2., 4., 8.] {
        let m = mesh(2, 2).transform(Affine::scale(scale)).unwrap();
        let path = crate::shapes::rect(0., 0., 100. * scale, 100. * scale);
        let size = (100. * scale) as i32;
        let p = CompiledMesh::new(&m, &path, Rect::new(0, 0, size, size)).unwrap();
        let full = p.render_rgba(Rect::new(0, 0, size, size)).unwrap();
        let tile = p.render_rgba(Rect::new(size / 3, size / 4, size / 3 + 7, size / 4 + 11)).unwrap();
        for (i, px) in tile.iter().enumerate() {
            assert_eq!(*px, full[((size / 4 + i as i32 / 7) * size + size / 3 + i as i32 % 7) as usize]);
        }
        let a = p.at(scale * 32.7, scale * 41.8);
        let b = sampler(&mesh(2, 2)).at(32.7, 41.8);
        assert!(a.into_iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-6));
    }
}
#[test]
fn mesh43_validation_nan_dimensions_bad_indices_and_budget_are_atomic() {
    let mut m = mesh(1, 1);
    let original = m.clone();
    assert!(m.move_point(8, None, [0., 0.]).is_err());
    assert!(m.move_point(0, Some(4), [0., 0.]).is_err());
    assert!(m.set_color(0, [f32::NAN, 0., 0., 1.], 1.).is_err());
    assert!(m.insert(0, 0, f64::NAN, 0.5, None).is_err());
    assert_eq!(m, original);
    m.rows = usize::MAX;
    assert!(m.validate().is_err());
    assert!(Mesh::regular([0., 0., 1., 1.], 51, 1, Appearance::Flat, 0., |_| [1.; 4]).is_err());
    let huge = Mesh::regular([0., 0., 1_000_000., 1_000_000.], 1, 1, Appearance::Flat, 0., |_| [1.; 4]).unwrap();
    let huge_path = crate::shapes::rect(0., 0., 1_000_000., 1_000_000.);
    assert!(CompiledMesh::new(&huge, &huge_path, Rect::new(0, 0, 1_000_000, 1_000_000)).err().unwrap().contains("reference budget"));
}
#[test]
fn mesh43_highlight_appearance_has_expected_linear_white_mix() {
    for (appearance, center, edge) in [(Appearance::Flat, 0., 0.), (Appearance::ToCenter, 0.5, 0.), (Appearance::ToEdge, 0., 0.5)] {
        let m = Mesh::regular([0., 0., 10., 10.], 2, 2, appearance, 50., |_| [0., 0., 0., 1.]).unwrap();
        assert!((srgb_to_linear(m.points[m.index(1, 1)].rgba[0]) - center).abs() < 1e-6);
        assert!((srgb_to_linear(m.points[0].rgba[0]) - edge).abs() < 1e-6);
    }
}
#[test]
fn mesh43_export_fallback_warning_aggregates_once_per_export() {
    for format in [ExportFormat::Pdf, ExportFormat::Svg, ExportFormat::Psd] {
        let mut w = Vec::new();
        for _ in 0..10 {
            raster_fallback_warning(&mut w, format);
        }
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].feature, "gradient_mesh");
        assert!(w[0].message.contains("rasterized"));
    }
}
