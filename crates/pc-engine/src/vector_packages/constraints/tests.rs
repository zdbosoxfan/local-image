#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use arithmetic::resize;
#[test]
fn layout16_constraint_table_has_independent_margin_and_center_oracles() {
    let old = [10., 20., 100., 80.];
    let child = [70., 50., 20., 10.];
    // Resizing 100 -> 160: original margins are 60 and 20. Fixed children keep
    // their exact size; unpinned centres keep the same fractional parent position.
    for (left, right, fixed, x, w) in [
        (false, false, false, 126., 32.),
        (false, false, true, 132., 20.),
        (true, false, false, 90., 32.),
        (true, false, true, 90., 20.),
        (false, true, false, 138., 32.),
        (false, true, true, 150., 20.),
        (true, true, false, 90., 80.),
        (true, true, true, 120., 20.),
    ] {
        let c = Constraints { left, right, fixed_width: fixed, top: true, fixed_height: true, ..Default::default() };
        let b = resize(old, [30., 40., 160., 100.], child, c).unwrap();
        assert_eq!(b, [x, 70., w, 10.], "{c:?}");
    }
}
#[test]
fn layout16_pin_right_fixed_size_preserves_margin_over_resize_table() {
    for width in [30., 50., 99., 100., 160., 250.] {
        let c = Constraints { right: true, bottom: true, fixed_width: true, fixed_height: true, ..Default::default() };
        let b = resize([0., 0., 100., 100.], [10., 20., width, 200.], [60., 70., 20., 10.], c).unwrap();
        assert_eq!(10. + width - b[0] - b[2], 20.);
        assert_eq!(220. - b[1] - b[3], 20.);
        assert_eq!((b[2], b[3]), (20., 10.));
    }
}
#[test]
fn layout16_both_pins_fixed_precedence_centers_original_margins() {
    let c = Constraints { left: true, right: true, top: true, bottom: true, fixed_width: true, fixed_height: true };
    let b = resize([0., 0., 100., 100.], [0., 0., 200., 60.], [10., 20., 30., 40.], c).unwrap();
    assert_eq!(b, [60., 0., 30., 40.]);
    let old_center = 25.;
    let new_center = b[0] + b[2] / 2.;
    assert_eq!(new_center - old_center, 50.);
}
#[test]
fn layout16_invalid_constraint_geometry_and_rotated_parent_are_errors() {
    let both = Constraints { left: true, right: true, ..Default::default() };
    assert!(resize([0., 0., 100., 100.], [0., 0., 20., 100.], [30., 10., 20., 20.], both).is_err());
    for value in [f64::NAN, f64::INFINITY, -1., 0., 1e9] {
        assert!(resize([0., 0., 100., 100.], [0., 0., value, 100.], [1., 1., 10., 10.], Constraints::default()).is_err());
    }
    assert!(arithmetic::parent_basis(photocraft_geom::Affine::rotate(0.3)).is_err());
    assert!(arithmetic::parent_basis(photocraft_geom::Affine { m: [1., 0., 0.2, 1., 0., 0.] }).is_err());
    assert!(arithmetic::parent_basis(photocraft_geom::Affine::translate(10., 20.)).is_ok());
}

#[test]
fn layout16_256_fixed_right_properties_preserve_size_and_margin() {
    // A deterministic generated corpus (binary fractions), not a second arithmetic
    // implementation. The independent invariants are unchanged margin and dimensions.
    let mut seed = 19u64;
    for _ in 0..256 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let width = 100. + ((seed >> 32) % 400) as f64;
        let child_width = 1. + ((seed >> 24) % 40) as f64;
        let right = ((seed >> 16) % 60) as f64 / 4.;
        let old_x = ((seed >> 8) % 20) as f64;
        let next = width + ((seed >> 40) % 200) as f64;
        let child = [old_x + width - right - child_width, 5., child_width, 10.];
        let b = resize(
            [old_x, 0., width, 100.],
            [30., 0., next, 100.],
            child,
            Constraints { right: true, fixed_width: true, fixed_height: true, ..Default::default() },
        )
        .unwrap();
        assert_eq!(b[2], child_width);
        assert_eq!(b[3], 10.);
        assert_eq!(30. + next - b[0] - b[2], right);
    }
}
