use lightcraft_raw::{ColorData, Mat3, camera_matrices};

#[test]
fn normalized_removes_non_alphanumeric_and_uppercases() {
    assert_eq!(camera_matrices::normalized("Canon EOS-5D"), "CANONEOS5D");
    assert_eq!(camera_matrices::normalized("  apple  QuickTake_100 "), "APPLEQUICKTAKE100");
}

#[test]
fn normalized_empty_string_returns_empty() {
    assert_eq!(camera_matrices::normalized(""), "");
    assert_eq!(camera_matrices::normalized("   "), "");
    assert_eq!(camera_matrices::normalized("!@#$%^&*()"), "");
}

#[test]
fn normalized_filters_non_ascii() {
    assert_eq!(camera_matrices::normalized("Càmera100"), "CMERA100");
    assert_eq!(camera_matrices::normalized("Fünf"), "FNF");
}

#[test]
fn get_known_canon_5d_returns_some_with_expected_shape() {
    let result = camera_matrices::get("Canon", "EOS 5D");
    assert!(result.is_some());
    let (matrices, illuminants) = result.unwrap();
    assert!(matches!(matrices, [Some(_), Some(_)]));
    assert_eq!(illuminants, [17, 21]);
}

#[test]
fn get_known_apple_quicktake_has_only_second_matrix() {
    let result = camera_matrices::get("Apple", "QuickTake 100");
    assert!(result.is_some());
    let (matrices, illuminants) = result.unwrap();
    assert!(matches!(matrices, [None, Some(_)]));
    assert_eq!(illuminants, [0, 21]);
}

#[test]
fn get_unknown_make_model_returns_none() {
    assert_eq!(camera_matrices::get("Unknown", "Camera"), None);
    assert_eq!(camera_matrices::get("Canon", "Nonexistent Model"), None);
}

#[test]
fn get_is_case_and_symbol_insensitive() {
    let a = camera_matrices::get("Canon", "EOS 5D");
    let b = camera_matrices::get("canon", "eos5d");
    let c = camera_matrices::get("CANON", "EOS-5D");
    assert_eq!(a, b);
    assert_eq!(a, c);
    assert!(a.is_some());
}

#[test]
fn get_with_empty_make_or_model_returns_none() {
    assert_eq!(camera_matrices::get("", "EOS 5D"), None);
    assert_eq!(camera_matrices::get("Canon", ""), None);
    assert_eq!(camera_matrices::get("", ""), None);
}

#[test]
fn fill_populates_default_color_data() {
    let mut color = ColorData::default();
    camera_matrices::fill(&mut color, "Canon", "EOS 5D");
    assert!(color.color_matrix[0].is_some());
    assert!(color.color_matrix[1].is_some());
    assert_eq!(color.illuminant, [17, 21]);
}

#[test]
fn fill_does_nothing_when_color_matrix_already_present() {
    let mut color = ColorData::default();
    color.color_matrix[0] = Some(Mat3([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]));
    let before = color.clone();
    camera_matrices::fill(&mut color, "Canon", "EOS 5D");
    assert_eq!(color, before);
}

#[test]
fn fill_keeps_second_matrix_none_if_first_is_some() {
    let mut color = ColorData::default();
    color.color_matrix[0] = Some(Mat3([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]));
    camera_matrices::fill(&mut color, "Apple", "QuickTake 100");
    assert!(color.color_matrix[1].is_none());
    assert_eq!(color.illuminant, [0, 0]);
}

#[test]
fn fill_unknown_camera_leaves_color_data_unchanged() {
    let mut color = ColorData::default();
    let before = color.clone();
    camera_matrices::fill(&mut color, "Unknown", "Camera");
    assert_eq!(color, before);
}

#[test]
fn fill_with_empty_make_model_leaves_defaults() {
    let mut color = ColorData::default();
    camera_matrices::fill(&mut color, "", "");
    assert!(color.color_matrix.iter().all(Option::is_none));
    assert_eq!(color.illuminant, [0, 0]);
}

#[test]
fn data_is_sorted_and_keys_are_normalized() {
    let mut prev: Option<&str> = None;
    for entry in camera_matrices::DATA.iter() {
        let key = entry.0;
        assert_eq!(camera_matrices::normalized(key), key);
        if let Some(prev_key) = prev {
            assert!(prev_key <= key);
        }
        prev = Some(key);
    }
}

#[test]
fn data_entries_have_valid_shape() {
    for &(make_model, matrices, illuminants) in camera_matrices::DATA {
        assert!(!make_model.is_empty());
        // `matrices` and `illuminants` are already of the correct fixed length by type.
        let _ = (matrices, illuminants);
    }
}
