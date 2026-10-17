use photocraft_engine::Session;
use photocraft_engine::layer_menu_cmds;

#[test]
fn defringe_no_transparency_is_identity() {
    let w = 2;
    let h = 2;
    let mut px = vec![[0.1, 0.2, 0.3, 1.0], [0.4, 0.5, 0.6, 1.0], [0.7, 0.8, 0.9, 1.0], [1.0, 0.0, 0.5, 1.0]];
    let original = px.clone();
    layer_menu_cmds::defringe(&mut px, w, h, 1);
    assert_eq!(px, original);
}

#[test]
fn defringe_all_transparent_is_identity_even_with_nan() {
    let w = 1;
    let h = 1;
    let mut px = vec![[f32::NAN, f32::NAN, f32::NAN, 0.0]];
    layer_menu_cmds::defringe(&mut px, w, h, 1);
    assert_eq!(px[0][3], 0.0);
    assert!(px[0][0].is_nan());
    assert!(px[0][1].is_nan());
    assert!(px[0][2].is_nan());
}

#[test]
fn defringe_preserves_alpha() {
    let w = 4;
    let h = 3;
    let mut px = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let alpha = (x + y) as f32 / 6.0;
            px.push([0.5, 0.5, 0.5, alpha]);
        }
    }
    let original_alpha: Vec<f32> = px.iter().map(|p| p[3]).collect();
    layer_menu_cmds::defringe(&mut px, w, h, 2);
    let new_alpha: Vec<f32> = px.iter().map(|p| p[3]).collect();
    assert_eq!(original_alpha, new_alpha);
}

#[test]
fn defringe_grows_core_color_into_fringe() {
    let w = 5;
    let h = 5;
    let mut px = vec![[0.0; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            if (1..=3).contains(&x) && (1..=3).contains(&y) {
                px[y * w + x] = [0.2, 0.4, 0.6, 1.0];
            } else {
                px[y * w + x] = [0.0, 0.0, 0.0, 0.0];
            }
        }
    }
    layer_menu_cmds::defringe(&mut px, w, h, 1);
    for (idx, p) in px.iter().enumerate() {
        if p[3] > 0.01 {
            assert!((p[0] - 0.2).abs() < 1e-5, "idx {}: {:?}", idx, p);
            assert!((p[1] - 0.4).abs() < 1e-5, "idx {}: {:?}", idx, p);
            assert!((p[2] - 0.6).abs() < 1e-5, "idx {}: {:?}", idx, p);
        }
    }
}

#[test]
fn defringe_odd_sizes_do_not_panic() {
    for (w, h) in [(1, 1), (1, 5), (5, 1), (2, 3)] {
        let mut px = vec![[0.0; 4]; w * h];
        for (i, p) in px.iter_mut().enumerate() {
            p[3] = if i % 2 == 0 { 0.0 } else { 1.0 };
            p[0] = 0.5;
            p[1] = 0.5;
            p[2] = 0.5;
        }
        layer_menu_cmds::defringe(&mut px, w, h, 3);
    }
}

#[test]
fn defringe_width_zero_changes_nothing() {
    let w = 3;
    let h = 3;
    let mut px = vec![[0.0; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let alpha = if x == 1 && y == 1 { 1.0 } else { 0.0 };
            px[y * w + x] = [0.2, 0.4, 0.6, alpha];
        }
    }
    let original = px.clone();
    layer_menu_cmds::defringe(&mut px, w, h, 0);
    assert_eq!(px, original);
}

#[test]
fn reveal_command_returns_supported_program() {
    let path = "some/dir/file.png";
    let (program, args) = layer_menu_cmds::reveal_command(path);
    assert!(!program.is_empty());
    assert!(matches!(program, "open" | "explorer" | "xdg-open"));
    assert!(!args.is_empty());
}

#[test]
fn disabled_reason_unknown_command() {
    let s = Session::new();
    assert_eq!(s.disabled_reason("foo.bar"), Some("unknown command `foo.bar`".to_string()));
}

#[test]
fn disabled_reason_layer_mask_apply_no_document() {
    let s = Session::new();
    assert_eq!(s.disabled_reason("layer.layerMask.apply"), Some("no document open".to_string()));
}

#[test]
fn all_layer_menu_specs_disabled_without_document() {
    let s = Session::new();
    for spec in layer_menu_cmds::specs() {
        let res = (spec.enabled)(&s);
        assert!(res.is_err(), "spec {} should be disabled without doc", spec.id);
    }
}
