#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
#[test]
fn view37_outline_hairline_profile_is_one_device_pixel_at_100_and_800_percent() {
    for scale in [1., 8.] {
        // Device-space x=24.5 in both cases, independently chosen to align a 1px line.
        let path = Path::new(vec![Subpath::polyline(&[(24.5 / scale, 1.), (24.5 / scale, 7.)])]);
        let clip = Rect::new(0, 0, 64, 64);
        let out = Outline::new(&path, Affine::scale(scale), clip).unwrap().render(PixelFormat::RGBA8, clip).unwrap();
        for x in 0..64 {
            assert_eq!(out.pixel(x, (3. * scale) as i32)[3], if x == 24 { 1. } else { 0. }, "scale {scale}, column {x}");
        }
    }
}
#[test]
fn view37_outline_overlapping_shapes_exposes_bottom_path_and_no_fill() {
    let clip = Rect::new(0, 0, 40, 40);
    let bottom = Outline::new(&crate::shapes::rect(5.5, 5.5, 20., 20.), Affine::IDENTITY, clip).unwrap();
    let top = Outline::new(&crate::shapes::rect(10.5, 10.5, 20., 20.), Affine::IDENTITY, clip).unwrap();
    // Interior of top, on bottom's right edge; the top fill must not intercept it.
    let p = Point::new(25.5, 18.);
    assert!(!top.hit(p, 0.25).unwrap());
    assert!(bottom.hit(p, 0.25).unwrap());
    for outline in [bottom, top] {
        assert_eq!(outline.render(PixelFormat::RGBA8, clip).unwrap().pixel(18, 18)[3], 0.);
    }
}
#[test]
fn view37_outline_open_paths_do_not_hit_implicit_fill_closure() {
    let p = Path::new(vec![Subpath::polyline(&[(4., 4.), (24., 4.), (24., 24.)])]);
    let outline = Outline::new(&p, Affine::IDENTITY, Rect::new(0, 0, 32, 32)).unwrap();
    assert!(!outline.hit(Point::new(14., 14.), 0.5).unwrap());
    assert!(outline.hit(Point::new(15., 4.), 0.5).unwrap());
}
#[test]
fn view37_outline_raster_box_and_tile_pixels_are_deterministic() {
    let clip = Rect::new(0, 0, 64, 64);
    let path = raster_box(Rect::new(8, 8, 56, 56));
    let outline = Outline::new(&path, Affine::IDENTITY, clip).unwrap();
    assert!(outline.hit(Point::new(20., 20.), 0.01).unwrap());
    assert!(outline.hit(Point::new(20., 44.), 0.01).unwrap());
    let out = outline.render(PixelFormat::RGBA8, clip).unwrap();
    assert_eq!(out.read_region(clip), outline.render(PixelFormat::RGBA8, clip).unwrap().read_region(clip));
    for y in [0, 32] {
        for x in [0, 32] {
            let tile = Rect::new(x, y, x + 32, y + 32);
            let expected = out.read_region(tile);
            let actual = outline.render(PixelFormat::RGBA8, tile).unwrap().read_region(tile);
            // Independent tile clips can round an exact half-coverage tie to adjacent
            // RGBA8 values. Same-clip determinism above stays exact; tile parity is the
            // contract's premultiplied one-byte bound (black RGB, alpha carries coverage).
            for (i, (a, b)) in expected.iter().zip(actual).enumerate() {
                assert!(((a * 255.).round() as i16 - (b * 255.).round() as i16).abs() <= 1, "tile sample {i}: {a} vs {b}");
            }
        }
    }
}
#[test]
fn view37_outline_refuses_nonfinite_unbounded_or_singular_input() {
    let mut path = crate::shapes::rect(0., 0., 10., 10.);
    let clip = Rect::new(0, 0, 32, 32);
    for a in [Affine::scale(0.), Affine::scale(f64::INFINITY)] {
        assert!(Outline::new(&path, a, clip).is_err());
    }
    path.subpaths[0].knots[0].in_ctrl.x = f64::NAN;
    assert!(Outline::new(&path, Affine::IDENTITY, clip).is_err());
    let o = Outline::new(&Path::default(), Affine::IDENTITY, clip).unwrap();
    assert!(o.hit(Point::new(0., 0.), f64::NAN).is_err());
    assert!(o.render(PixelFormat::RGBA8, Rect::new(0, 0, 20_000, 20_000)).is_err());
}
