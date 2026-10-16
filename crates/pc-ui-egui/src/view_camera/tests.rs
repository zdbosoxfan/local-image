use super::*;

fn close(a: Pos2, b: Pos2) {
    assert!(a.distance(b) < 0.001, "{a:?} != {b:?}");
}

#[test]
fn clockwise_reflected_mapping_and_inverse_match_independent_coordinates() {
    let rect = Rect::from_min_max(pos2(13.25, 21.5), pos2(613.25, 421.5));
    for (angle, flip, zoom) in [(0.0_f32, false, 1.0), (37.0, false, 1.7), (-90.0, true, 2.0)] {
        let center = [72.25, -13.5];
        let camera = Camera::new(rect, zoom, center, flip, angle.to_radians());
        let (sin, cos) = f64::from(angle).to_radians().sin_cos();
        for point in [pos2(0.0, 0.0), pos2(140.0, 80.0), pos2(-50.25, 12.5)] {
            let x = f64::from((point.x - center[0]) * zoom) * if flip { -1.0 } else { 1.0 };
            let y = f64::from((point.y - center[1]) * zoom);
            let expected = rect.center() + vec2((cos * x - sin * y) as f32, (sin * x + cos * y) as f32);
            close(camera.to_screen(point), expected);
            close(camera.to_document(expected), point);
            close(camera.document_delta(camera.screen_delta(point.to_vec2())).to_pos2(), point);
        }
    }
    let camera = Camera::new(Rect::from_center_size(Pos2::ZERO, vec2(10.0, 10.0)), 1.0, [0.0, 0.0], false, std::f32::consts::FRAC_PI_2);
    close(camera.to_screen(pos2(20.0, 0.0)), pos2(0.0, 20.0));
}

#[test]
fn device_mapping_preserves_upright_origin_and_inverse_at_fractional_scale() {
    let rect = Rect::from_min_max(pos2(13.25, 21.5), pos2(613.25, 421.5));
    let center = [73.125, 49.25];
    for (zoom, ppp, flip) in [(1.0, 1.25, false), (1.7, 2.0, true), (2.0, 1.0, false)] {
        let physical = zoom * ppp;
        let direction = if flip { -1.0 } else { 1.0 };
        let expected = pos2((rect.center().x * ppp - center[0] * physical * direction).round(), (rect.center().y * ppp - center[1] * physical).round());
        let camera = Camera::new(rect, zoom, center, flip, 0.0).device_pixels(ppp);
        assert_eq!(camera.origin(), expected, "legacy device origin must be byte-stable");
        assert_eq!(camera.scale(), physical);
        let point = pos2(25.5, 76.25);
        close(camera.to_document(camera.to_screen(point)), point);
    }
    let logical = Camera::new(rect, 1.7, center, true, 0.37);
    let point = pos2(12.25, 34.5);
    close(logical.device_pixels(1.25).to_screen(point), logical.to_screen(point) * 1.25);
}

#[test]
fn four_corner_bounds_cover_rotated_image_and_inverse_viewport() {
    let viewport = Rect::from_center_size(pos2(200.0, 180.0), vec2(160.0, 90.0));
    let document = Rect::from_min_size(Pos2::ZERO, vec2(120.0, 80.0));
    let camera = Camera::new(viewport, 2.0, [60.0, 40.0], true, 37.0_f32.to_radians());
    let bounds = camera.screen_bounds(document);
    let opposite_only = Rect::from_two_pos(camera.to_screen(document.min), camera.to_screen(document.max));
    assert!(camera.quad(document).iter().any(|p| !opposite_only.contains(*p)), "fixture must catch two-corner culling");
    for point in camera.quad(document) {
        assert!(bounds.contains(point));
    }
    let visible = camera.document_bounds(viewport);
    for point in [viewport.left_top(), viewport.right_top(), viewport.right_bottom(), viewport.left_bottom()] {
        assert!(visible.contains(camera.to_document(point)));
    }
    assert!(normalized_angle(f32::INFINITY).is_finite());
    assert_eq!(normalized_angle(f32::NAN), 0.0);
    assert!((normalized_angle(std::f32::consts::TAU + 0.25) - 0.25).abs() < 1e-6);
    let invalid = Camera::new(viewport, f32::NAN, [f32::INFINITY, f32::NAN], false, f32::NAN);
    assert_eq!(invalid.sin_cos(), [0.0, 1.0]);
    close(invalid.to_screen(Pos2::ZERO), viewport.center());
}

#[test]
fn cpu_textured_quad_rotates_uvs_and_leaves_aabb_corners_clear() {
    use li_testkit::softpaint as cpu;
    let colors = [Color32::RED, Color32::GREEN, Color32::BLUE, Color32::YELLOW];
    let image = egui::ColorImage::new([16, 12], (0..12).flat_map(|y| (0..16).map(move |x| colors[usize::from(x >= 8) + 2 * usize::from(y >= 6)])).collect());
    let mut textures = cpu::TextureStore::default();
    let id = TextureId::User(771);
    textures.insert(id, cpu::CpuTexture::linear(std::sync::Arc::new(image)));
    let viewport = Rect::from_min_size(Pos2::ZERO, vec2(200.0, 160.0));
    let document = Rect::from_min_size(Pos2::ZERO, vec2(16.0, 12.0));
    let background = Color32::from_rgb(17, 31, 47);
    for (angle, flip, ppp) in [(31.0_f32, false, 1.25), (-90.0, true, 2.0)] {
        let camera = Camera::new(viewport, 5.0, [8.0, 6.0], flip, angle.to_radians());
        let mesh = camera.textured_quad(id, document, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
        let output = cpu::paint(
            &[egui::ClippedPrimitive { clip_rect: viewport, primitive: egui::epaint::Primitive::Mesh(mesh) }],
            &textures,
            [(200.0 * ppp) as usize, (160.0 * ppp) as usize],
            ppp,
            background,
        );
        let (sin, cos) = f64::from(angle).to_radians().sin_cos();
        let (mut inside, mut outside) = (0, 0);
        for y in 0..output.size[1] {
            for x in 0..output.size[0] {
                let sx = (x as f64 + 0.5) / f64::from(ppp) - 100.0;
                let sy = (y as f64 + 0.5) / f64::from(ppp) - 80.0;
                let dx = (cos * sx + sin * sy) / 5.0 * if flip { -1.0 } else { 1.0 } + 8.0;
                let dy = (-sin * sx + cos * sy) / 5.0 + 6.0;
                let actual = output.pixels[y * output.size[0] + x];
                if dx < -1.0 || dy < -1.0 || dx > 17.0 || dy > 13.0 {
                    assert_eq!(actual, background, "rotated AABB must not fill the pasteboard");
                    outside += 1;
                } else if (1.0..15.0).contains(&dx) && (1.0..11.0).contains(&dy) && (dx - 8.0).abs() > 1.0 && (dy - 6.0).abs() > 1.0 {
                    assert_eq!(actual, colors[usize::from(dx >= 8.0) + 2 * usize::from(dy >= 6.0)], "{angle}/{flip} pixel {x},{y} maps to {dx},{dy}");
                    inside += 1;
                }
            }
        }
        assert!(inside > 1000 && outside > 10000, "nonvacuous inside/outside coverage {inside}/{outside}");
    }
}

#[test]
fn upright_logical_mapping_preserves_existing_view_coordinates_exactly() {
    let rect = Rect::from_min_max(pos2(13.25, 21.5), pos2(613.25, 421.5));
    let center = [73.125, 49.25];
    for (zoom, flip) in [(0.5, false), (1.7, true), (8.0, false)] {
        let camera = Camera::new(rect, zoom, center, flip, 0.0);
        let direction = if flip { -1.0 } else { 1.0 };
        for point in [pos2(0.0, 0.0), pos2(140.25, 80.5), pos2(-50.25, 12.5)] {
            let screen = rect.center() + vec2((point.x - center[0]) * zoom * direction, (point.y - center[1]) * zoom);
            assert_eq!(camera.to_screen(point), screen);
            let d = (screen - rect.center()) / zoom;
            let original = pos2(d.x * direction + center[0], d.y + center[1]);
            assert_eq!(camera.to_document(screen), original);
        }
    }
}
