use photocraft_algo::transform::Interp;
use photocraft_algo::warp::*;
use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_geom::Rect;
use photocraft_raster::Surface;

/// Create an RGB+alpha surface of given dimensions with a deterministic pattern.
fn make_rgb(sample: SampleType, w: i32, h: i32) -> Surface {
    let fmt = PixelFormat::new(ColorMode::Rgb, sample, true);
    let mut surf = Surface::new(fmt);
    for y in 0..h {
        for x in 0..w {
            let r = ((x * 7 + y * 13) % 17) as f32 / 16.0;
            let g = 1.0 - r;
            let b = (x as f32) / (w.max(1) as f32);
            surf.write_pixel(x, y, &[r, g, b, 1.0]);
        }
    }
    surf
}

#[test]
fn empty_src_rect_yields_empty_surface() {
    let src = make_rgb(SampleType::U8, 8, 8);
    let fmt = src.format();
    let out = warp_mesh_surface(&src, Rect::EMPTY, &|x, y| (x, y), Interp::Nearest);
    assert!(out.content_bounds().is_empty());
    assert_eq!(out.format(), PixelFormat::new(fmt.mode, fmt.sample, true));
}

#[test]
fn empty_triangles_yield_empty_surface() {
    let src = make_rgb(SampleType::U8, 8, 8);
    let r = src.content_bounds();
    let verts = vec![([0.0, 0.0], [0.0, 0.0]), ([1.0, 0.0], [1.0, 0.0]), ([0.0, 1.0], [0.0, 1.0])];
    let out = warp_triangles(&src, r, &verts, &[], Interp::Bilinear);
    assert!(out.content_bounds().is_empty());
}

#[test]
fn non_finite_destination_vertices_return_empty() {
    let src = make_rgb(SampleType::U8, 8, 8);
    let r = src.content_bounds();
    let verts = vec![([f64::NAN, 0.0], [0.0, 0.0]), ([1.0, 0.0], [1.0, 0.0]), ([0.0, 1.0], [0.0, 1.0])];
    let tris = [[0usize, 1, 2]];
    let out = warp_triangles(&src, r, &verts, &tris, Interp::Nearest);
    assert!(out.content_bounds().is_empty());
}

#[test]
fn warp_mesh_gray_with_empty_content_bounds_returns_empty() {
    let m = Surface::with_default(PixelFormat::GRAY8, &[0.5]);
    let out = warp_mesh_gray(&m, &|x, y| (x, y), Interp::Nearest);
    assert!(out.content_bounds().is_empty());
}

#[test]
fn warp_triangles_single_triangle_nearest() {
    let src = make_rgb(SampleType::U8, 4, 4);
    let r = src.content_bounds();
    // Map a small source triangle to a large destination triangle.
    let verts = vec![
        ([0.0, 0.0], [0.0, 0.0]), // dst, src
        ([10.0, 0.0], [3.0, 0.0]),
        ([0.0, 10.0], [0.0, 3.0]),
    ];
    let tris = [[0usize, 1, 2]];
    let out = warp_triangles(&src, r, &verts, &tris, Interp::Nearest);
    let b = out.content_bounds();
    assert!(!b.is_empty());

    // Pixel (1,1) is inside the destination triangle.
    let px = out.pixel(1, 1);
    assert!(px[3] > 0.0);

    // For (1,1), barycentric: l1 = 0.15, l2 = 0.15, l0 = 0.7.
    // Source = 0.7*(0,0) + 0.15*(3,0) + 0.15*(0,3) = (0.45,0.45) -> nearest pixel 0.
    let expected = src.pixel(0, 0);
    assert_eq!(px[0..3], expected[0..3]);
}

#[test]
fn warp_triangles_later_triangle_overwrites() {
    let src = make_rgb(SampleType::U8, 8, 8);
    let r = src.content_bounds();
    let verts = vec![
        ([0.0, 0.0], [0.0, 0.0]),
        ([5.0, 0.0], [4.0, 0.0]),
        ([0.0, 5.0], [0.0, 4.0]),
        ([0.0, 0.0], [7.0, 7.0]), // second triangle covers same area with different source
        ([5.0, 0.0], [7.0, 4.0]),
        ([0.0, 5.0], [4.0, 7.0]),
    ];
    let tris = [[0usize, 1, 2], [3, 4, 5]];
    let out = warp_triangles(&src, r, &verts, &tris, Interp::Nearest);

    // Pixel (2,2) is inside both triangles.
    // Second triangle source = 0.5*(7,4)+0.5*(4,7) = (5.5,5.5).
    let expected = src.pixel(5, 5);
    let px = out.pixel(2, 2);
    assert_eq!(px[0..3], expected[0..3]);
}

#[test]
fn warp_triangles_zero_area_triangle_no_output() {
    let src = make_rgb(SampleType::U8, 8, 8);
    let r = src.content_bounds();
    let verts = vec![([1.0, 1.0], [0.0, 0.0]), ([1.0, 1.0], [1.0, 0.0]), ([1.0, 1.0], [0.0, 1.0])];
    let tris = [[0usize, 1, 2]];
    let out = warp_triangles(&src, r, &verts, &tris, Interp::Bilinear);
    assert!(out.content_bounds().is_empty());
}

#[test]
fn warp_triangles_source_outside_bounds_gives_empty_output() {
    let src = make_rgb(SampleType::U8, 4, 4);
    let r = Rect::new(0, 0, 4, 4); // content bounds
    let verts = vec![
        ([0.0, 0.0], [10.0, 10.0]), // source far outside
        ([5.0, 0.0], [11.0, 10.0]),
        ([0.0, 5.0], [10.0, 11.0]),
    ];
    let tris = [[0usize, 1, 2]];
    let out = warp_triangles(&src, r, &verts, &tris, Interp::Nearest);
    // Since all source positions are outside the source rect, no pixels are produced.
    assert!(out.content_bounds().is_empty());
}

#[test]
fn bilinear_interpolation_is_used() {
    // A 2x2 source with distinct colors.
    let mut src = Surface::new(PixelFormat::new(ColorMode::Rgb, SampleType::U8, true));
    src.write_pixel(0, 0, &[1.0, 0.0, 0.0, 1.0]);
    src.write_pixel(1, 0, &[0.0, 1.0, 0.0, 1.0]);
    src.write_pixel(0, 1, &[0.0, 0.0, 1.0, 1.0]);
    src.write_pixel(1, 1, &[1.0, 1.0, 1.0, 1.0]);
    let r = src.content_bounds();

    // Map the 2x2 source to a destination scaled by 2, causing interpolation.
    let out = warp_mesh_surface(&src, r, &|x, y| (x * 2.0, y * 2.0), Interp::Bilinear);

    let px = out.pixel(1, 1);
    assert!(px[0] > 0.0 && px[0] < 1.0, "red channel should be mixed, got {}", px[0]);
    assert!(px[1] > 0.0 && px[1] < 1.0, "green channel should be mixed, got {}", px[1]);
    assert!(px[2] > 0.0 && px[2] < 1.0, "blue channel should be mixed, got {}", px[2]);
    assert_eq!(px[3], 1.0);
}

#[test]
fn premultiplied_alpha_is_applied() {
    let mut src = Surface::new(PixelFormat::new(ColorMode::Rgb, SampleType::F32, true));
    // A single pixel with color 1.0, alpha 0.5.
    src.write_pixel(0, 0, &[1.0, 0.0, 0.0, 0.5]);
    let r = src.content_bounds();
    let out = warp_mesh_surface(&src, r, &|x, y| (x, y), Interp::Nearest);
    let px = out.pixel(0, 0);
    // Output should have unpremultiplied color (same as input), but alpha preserved.
    assert!((px[0] - 1.0).abs() < 1e-6);
    assert!(px[3] - 0.5 < 1e-6);
}

#[test]
fn warp_mesh_surface_small_1x1_rect() {
    let src = make_rgb(SampleType::U8, 8, 8);
    let tiny_rect = Rect::new(2, 3, 3, 4);
    let out = warp_mesh_surface(&src, tiny_rect, &|x, y| (x, y), Interp::Bilinear);
    // Output content bounds should be the same tiny rect.
    assert_eq!(out.content_bounds(), tiny_rect);
    // Pixel at (2,3) should match original.
    let expected = src.pixel(2, 3);
    let actual = out.pixel(2, 3);
    assert_eq!(actual, expected);
}

#[test]
fn no_panic_on_large_but_bounded_coordinates() {
    let src = make_rgb(SampleType::U8, 4, 4);
    let r = src.content_bounds();
    // Use a moderate scale factor to avoid excessive memory allocation.
    let out = warp_mesh_surface(&src, r, &|x, y| (x * 100.0, y * 100.0), Interp::Nearest);
    let b = out.content_bounds();
    assert!(!b.is_empty());
    assert!(b.width() < 1000 && b.height() < 1000);
}
