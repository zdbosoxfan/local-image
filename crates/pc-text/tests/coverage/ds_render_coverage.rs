use photocraft_text::render::{FAUX_BOLD_RADIUS, FAUX_ITALIC_DEG, PathEl};

#[test]
fn faux_italic_deg_has_expected_value() {
    assert_eq!(FAUX_ITALIC_DEG, 12.0);
}

#[test]
fn faux_bold_radius_has_expected_value() {
    assert_eq!(FAUX_BOLD_RADIUS, 0.018);
}

#[test]
fn path_el_move_to_is_copy_clone_debug_eq() {
    let p = PathEl::MoveTo([1.5, -2.25]);
    let q = p; // Copy
    assert_eq!(p, q); // PartialEq
    let r = p; // Copy again (Clone not needed)
    assert_eq!(p, r);
    let debug = format!("{:?}", p);
    assert!(debug.contains("MoveTo"));
}

#[test]
fn path_el_curve_to_is_copy_clone_debug_eq() {
    let curve = PathEl::CurveTo([0.0, 0.0], [1.0, 2.0], [3.0, 4.0]);
    let copied = curve;
    assert_eq!(curve, copied);
    let copied_again = curve;
    assert_eq!(curve, copied_again);
    let debug = format!("{:?}", curve);
    assert!(debug.contains("CurveTo"));
}

#[test]
fn path_el_close_is_copy_clone_debug_eq() {
    let close = PathEl::Close;
    let copied = close;
    assert_eq!(close, copied);
    let copied_again = close;
    assert_eq!(close, copied_again);
    assert_eq!(format!("{:?}", close), "Close");
}
