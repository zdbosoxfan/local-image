use photocraft_engine::doc::{Color, Fill, GradientStyle, Rect};
use photocraft_engine::gradient_fill_cmds::{CREATE, GET, SET, STOP, apply_stop, describe_fill, specs, style_name};
use serde_json::json;

fn red() -> Color {
    Color::rgba(1.0, 0.0, 0.0, 1.0)
}
fn green() -> Color {
    Color::rgba(0.0, 1.0, 0.0, 1.0)
}
fn blue() -> Color {
    Color::rgba(0.0, 0.0, 1.0, 1.0)
}

fn test_fill() -> Fill {
    Fill::Gradient {
        stops: vec![(0.0, red()), (0.5, green()), (1.0, blue())],
        angle: 0.0,
        scale: 1.0,
        style: GradientStyle::Linear,
        reverse: false,
        opacity_stops: vec![],
        midpoints: vec![0.3, 0.7],
        offset: (0.0, 0.0),
        dither: false,
        align: true,
    }
}

fn stops_of(f: &Fill) -> &Vec<(f32, Color)> {
    match f {
        Fill::Gradient { stops, .. } => stops,
        _ => panic!("not a gradient"),
    }
}

#[test]
fn specs_has_four_commands_in_order() {
    let s = specs();
    assert_eq!(s.len(), 4);
    assert_eq!(s[0].id, CREATE);
    assert_eq!(s[1].id, SET);
    assert_eq!(s[2].id, STOP);
    assert_eq!(s[3].id, GET);
}

#[test]
fn specs_labels_non_empty() {
    for spec in specs() {
        assert!(!spec.label.is_empty());
    }
}

#[test]
fn specs_params_mention_key_parameters() {
    let s = specs();
    assert!(s[0].params.contains("from"));
    assert!(s[0].params.contains("to"));
    assert!(s[1].params.contains("angle"));
    assert!(s[2].params.contains("action"));
    assert!(s[3].params.contains("stops"));
}

#[test]
fn specs_journal_flags_are_correct() {
    let s = specs();
    assert!(s[0].journal);
    assert!(s[1].journal);
    assert!(s[2].journal);
    assert!(!s[3].journal);
}

#[test]
fn constants_match_spec_ids() {
    assert_eq!(CREATE, "gradient.fill.create");
    assert_eq!(SET, "gradient.fill.set");
    assert_eq!(STOP, "gradient.fill.stop");
    assert_eq!(GET, "gradient.fill.get");
}

#[test]
fn style_name_returns_expected_strings() {
    assert_eq!(style_name(GradientStyle::Linear), "linear");
    assert_eq!(style_name(GradientStyle::Radial), "radial");
    assert_eq!(style_name(GradientStyle::Angle), "angle");
    assert_eq!(style_name(GradientStyle::Reflected), "reflected");
    assert_eq!(style_name(GradientStyle::Diamond), "diamond");
}

#[test]
fn apply_stop_add_color_stop_without_color_samples_ramp() {
    // Use a fill without midpoints so the ramp is linear and the sample at t=0.25 is exactly halfway.
    let fill = Fill::Gradient {
        stops: vec![(0.0, red()), (0.5, green()), (1.0, blue())],
        angle: 0.0,
        scale: 1.0,
        style: GradientStyle::Linear,
        reverse: false,
        opacity_stops: vec![],
        midpoints: vec![],
        offset: (0.0, 0.0),
        dither: false,
        align: true,
    };
    let result = apply_stop(&fill, &json!({"action":"add","location":0.25}), [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]).unwrap();
    let stops = stops_of(&result);
    assert_eq!(stops.len(), 4);
    let new_stop = stops.iter().find(|(t, _)| (*t - 0.25).abs() < 1e-6).expect("stop at 0.25");
    let rgb = new_stop.1.to_rgb();
    assert!((rgb[0] - 0.5).abs() < 1e-6 && (rgb[1] - 0.5).abs() < 1e-6 && rgb[2].abs() < 1e-6);
}

#[test]
fn apply_stop_add_color_stop_with_explicit_color() {
    let fill = test_fill();
    let result = apply_stop(&fill, &json!({"action":"add","location":0.75,"color":"#ff0000"}), [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]).unwrap();
    let stops = stops_of(&result);
    assert_eq!(stops.len(), 4);
    let new_stop = stops.iter().find(|(t, _)| (*t - 0.75).abs() < 1e-6).expect("stop at 0.75");
    let rgb = new_stop.1.to_rgb();
    assert!((rgb[0] - 1.0).abs() < 1e-6 && rgb[1].abs() < 1e-6 && rgb[2].abs() < 1e-6);
}

#[test]
fn apply_stop_move_reorders_stops() {
    let fill = test_fill();
    let result = apply_stop(&fill, &json!({"action":"move","index":0,"location":0.8}), [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]).unwrap();
    let stops = stops_of(&result);
    assert_eq!(stops.len(), 3);
    let times: Vec<f32> = stops.iter().map(|(t, _)| *t).collect();
    assert_eq!(times, vec![0.5, 0.8, 1.0]);
}

#[test]
fn apply_stop_delete_reduces_count() {
    let fill = test_fill();
    let result = apply_stop(&fill, &json!({"action":"delete","index":0}), [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]).unwrap();
    let stops = stops_of(&result);
    assert_eq!(stops.len(), 2);
    assert_eq!(stops[0].0, 0.5);
    assert_eq!(stops[1].0, 1.0);
}

#[test]
fn apply_stop_delete_last_two_stops_errors() {
    let fill = test_fill();
    let fill2 = apply_stop(&fill, &json!({"action":"delete","index":0}), [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]).unwrap();
    let result = apply_stop(&fill2, &json!({"action":"delete","index":0}), [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]);
    assert!(result.is_err());
}

#[test]
fn apply_stop_color_changes_stop() {
    let fill = test_fill();
    let result = apply_stop(&fill, &json!({"action":"color","index":1,"color":"#0000ff"}), [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]).unwrap();
    let stops = stops_of(&result);
    let rgb = stops[1].1.to_rgb();
    assert!(rgb[0].abs() < 1e-6 && rgb[1].abs() < 1e-6 && (rgb[2] - 1.0).abs() < 1e-6);
}

#[test]
fn apply_stop_midpoint_sets_segment() {
    let fill = test_fill();
    let result = apply_stop(&fill, &json!({"action":"midpoint","index":0,"location":0.1}), [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]).unwrap();
    match &result {
        Fill::Gradient { midpoints, .. } => {
            assert_eq!(midpoints, &vec![0.1, 0.7]);
        }
        _ => panic!("expected gradient"),
    }
}

#[test]
fn apply_stop_opacity_stop_add() {
    let fill = test_fill();
    let result = apply_stop(&fill, &json!({"action":"add","kind":"opacity","location":0.5,"opacity":50}), [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]).unwrap();
    match &result {
        Fill::Gradient { opacity_stops, .. } => {
            assert_eq!(opacity_stops.len(), 3);
            assert_eq!(opacity_stops[0].0, 0.0);
            assert_eq!(opacity_stops[1].0, 0.5);
            assert_eq!(opacity_stops[2].0, 1.0);
            assert!((opacity_stops[1].1 - 0.5).abs() < 1e-6);
        }
        _ => panic!("expected gradient"),
    }
}

#[test]
fn apply_stop_opacity_stop_delete_errors_when_two_left() {
    let fill = test_fill();
    let fill2 = apply_stop(&fill, &json!({"action":"add","kind":"opacity","location":0.5,"opacity":50}), [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]).unwrap();
    let fill3 = apply_stop(&fill2, &json!({"action":"delete","kind":"opacity","index":1}), [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]).unwrap();
    let result = apply_stop(&fill3, &json!({"action":"delete","kind":"opacity","index":0}), [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]);
    assert!(result.is_err());
}

#[test]
fn apply_stop_missing_action_returns_error() {
    let fill = test_fill();
    let result = apply_stop(&fill, &json!({"location":0.5}), [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]);
    assert!(result.is_err());
}

#[test]
fn apply_stop_invalid_action_returns_error() {
    let fill = test_fill();
    let result = apply_stop(&fill, &json!({"action":"bogus"}), [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]);
    assert!(result.is_err());
}

#[test]
fn apply_stop_index_out_of_bounds_errors() {
    let fill = test_fill();
    let result = apply_stop(&fill, &json!({"action":"color","index":10,"color":"#ffffff"}), [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]);
    assert!(result.is_err());
}

#[test]
fn describe_fill_output_matches_expected_structure() {
    // Use exactly representable midpoints to avoid f32->f64 precision mismatches.
    let fill = Fill::Gradient {
        stops: vec![(0.0, red()), (0.5, green()), (1.0, blue())],
        angle: 0.0,
        scale: 1.0,
        style: GradientStyle::Linear,
        reverse: false,
        opacity_stops: vec![],
        midpoints: vec![0.25, 0.75],
        offset: (0.0, 0.0),
        dither: false,
        align: true,
    };
    let frame = Rect::EMPTY;
    let v = describe_fill(&fill, frame);
    assert!(v.is_object());
    let obj = v.as_object().unwrap();

    assert_eq!(obj["style"].as_str().unwrap(), "linear");
    assert_eq!(obj["angle"].as_f64().unwrap(), 0.0);
    assert_eq!(obj["scale"].as_f64().unwrap(), 100.0);
    assert_eq!(obj["offset"].as_array().unwrap(), &vec![json!(0.0), json!(0.0)]);
    assert!(!obj["reverse"].as_bool().unwrap());
    assert!(!obj["dither"].as_bool().unwrap());
    assert!(obj["align"].as_bool().unwrap());

    assert!(obj["from"].is_array());
    assert!(obj["to"].is_array());

    let stops = obj["stops"].as_array().unwrap();
    assert_eq!(stops.len(), 3);
    assert_eq!(stops[0][0].as_f64().unwrap(), 0.0);
    assert_eq!(stops[0][1].as_str().unwrap(), "#ff0000");

    let transparency = obj["transparency"].as_array().unwrap();
    assert_eq!(transparency.len(), 0);

    let midpoints = obj["midpoints"].as_array().unwrap();
    assert_eq!(midpoints, &vec![json!(0.25), json!(0.75)]);
}
