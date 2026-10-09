use photocraft_engine::Session;
use photocraft_engine::analysis_cmds::{
    self, AnalysisState, DataPoints, Feature, LogRow, available, log_columns, log_csv, measure_features, now_iso, ruler_info, straighten_angle,
};
use photocraft_engine::doc::{MeasurementScale, Ruler};
use serde_json::{Map, Value, json};

fn approx_eq(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() < eps
}

#[test]
fn available_returns_common_plus_source_specific() {
    let common: Vec<&str> = vec!["label", "dateTime", "document", "source", "scale", "scaleUnits", "scaleFactor"];
    let selection_extra: Vec<&str> =
        vec!["count", "area", "perimeter", "circularity", "height", "width", "grayMin", "grayMax", "grayMean", "grayMedian", "integratedDensity", "histogram"];
    let expected: Vec<&str> = common.iter().chain(selection_extra.iter()).copied().collect();
    assert_eq!(available("selection"), expected);
}

#[test]
fn available_unknown_source_returns_count_only() {
    let expected: Vec<&str> = vec!["label", "dateTime", "document", "source", "scale", "scaleUnits", "scaleFactor", "count"];
    assert_eq!(available("bogus"), expected);
}

#[test]
fn data_points_default_contains_all_available() {
    let dp = DataPoints::default();
    assert_eq!(dp.selection, available("selection").iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert_eq!(dp.ruler, available("ruler").iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert_eq!(dp.count, available("count").iter().map(|s| s.to_string()).collect::<Vec<_>>());
}

#[test]
fn data_points_serde_round_trip() {
    let dp = DataPoints::default();
    let json_str = serde_json::to_string(&dp).unwrap();
    let dp2: DataPoints = serde_json::from_str(&json_str).unwrap();
    assert_eq!(dp, dp2);
}

#[test]
fn analysis_state_default_empty_log_and_default_datapoints() {
    let st = AnalysisState::default();
    assert!(st.log.is_empty());
    assert_eq!(st.data_points, DataPoints::default());
}

#[test]
fn feature_circularity_known_values() {
    let f = Feature { area: 1.0, perimeter: 4.0, ..Default::default() };
    assert!(approx_eq(f.circularity(), std::f64::consts::PI / 4.0, 1e-9));

    let g = Feature { area: 1.0, perimeter: 0.0, ..Default::default() };
    assert_eq!(g.circularity(), 0.0);

    let h = Feature { area: 100.0, perimeter: 1.0, ..Default::default() };
    assert!(h.circularity() <= 1.0);
}

#[test]
fn measure_features_single_pixel() {
    let mask = vec![true];
    let gray = vec![0.5f32];
    let feats = measure_features(&mask, &gray, 1, 1, (10, 20), 255.0);
    assert_eq!(feats.len(), 1);
    let f = &feats[0];
    assert_eq!(f.area, 1.0);
    let expected_perim = 4.0 * std::f64::consts::FRAC_1_SQRT_2;
    assert!(approx_eq(f.perimeter, expected_perim, 1e-9));
    assert_eq!((f.bounds.x0, f.bounds.y0, f.bounds.x1, f.bounds.y1), (10, 20, 11, 21));
    assert!(approx_eq(f.gray_min, 127.5, 1e-9));
    assert!(approx_eq(f.gray_max, 127.5, 1e-9));
    assert!(approx_eq(f.gray_mean, 127.5, 1e-9));
    assert!(approx_eq(f.gray_median, (32768.0 / 65535.0) * 255.0, 1e-6));
    assert_eq!(f.histogram[128], 1);
    assert_eq!(f.histogram.iter().sum::<u64>(), 1);
}

#[test]
fn measure_features_two_components_diagonal_perimeter() {
    let mask = vec![true, false, false, false, false, false, false, false, true];
    let gray = vec![0.25f32; 9];
    let feats = measure_features(&mask, &gray, 3, 3, (0, 0), 255.0);
    assert_eq!(feats.len(), 2);
    let p = 4.0 * std::f64::consts::FRAC_1_SQRT_2;
    for f in &feats {
        assert_eq!(f.area, 1.0);
        assert!(approx_eq(f.perimeter, p, 1e-9));
    }
    assert_eq!((feats[0].bounds.x0, feats[0].bounds.y0, feats[0].bounds.x1, feats[0].bounds.y1,), (0, 0, 1, 1));
    assert_eq!((feats[1].bounds.x0, feats[1].bounds.y0, feats[1].bounds.x1, feats[1].bounds.y1,), (2, 2, 3, 3));
}

#[test]
fn measure_features_empty_returns_empty() {
    let mask = vec![false; 4];
    let gray = vec![0.0f32; 4];
    let feats = measure_features(&mask, &gray, 2, 2, (0, 0), 1.0);
    assert!(feats.is_empty());
}

#[test]
fn measure_features_clips_gray_and_origin_bounds() {
    let feats = measure_features(&[true], &[2.0f32], 1, 1, (5, 6), 1.0);
    assert_eq!(feats.len(), 1);
    let f = &feats[0];
    assert_eq!(f.gray_min, 2.0);
    assert_eq!(f.gray_max, 2.0);
    assert_eq!(f.gray_mean, 2.0);
    assert_eq!(f.gray_median, 1.0);
    assert_eq!(f.histogram[255], 1);
}

#[test]
fn ruler_info_single_line_angle_and_length() {
    let sc = MeasurementScale::default();
    let r = Ruler { start: [0.0, 0.0], end: [10.0, 0.0], protractor: None };
    let info = ruler_info(&r, &sc);
    assert_eq!(info["x"], json!(0.0));
    assert_eq!(info["y"], json!(0.0));
    assert_eq!(info["w"], json!(10.0));
    assert_eq!(info["h"], json!(0.0));
    assert_eq!(info["angle"], json!(0.0));
    assert_eq!(info["l1"], json!(10.0));
    assert_eq!(info["l2"], Value::Null);
    assert!(approx_eq(info["length"].as_f64().unwrap(), 10.0 * sc.factor(), 1e-9));
    assert_eq!(info["protractor"], json!(false));
}

#[test]
fn ruler_info_vertical_line_angle() {
    let sc = MeasurementScale::default();
    let r = Ruler { start: [0.0, 0.0], end: [0.0, 10.0], protractor: None };
    let info = ruler_info(&r, &sc);
    assert!(approx_eq(info["angle"].as_f64().unwrap(), -90.0, 1e-9));
}

#[test]
fn ruler_info_protractor_angle() {
    let sc = MeasurementScale::default();
    let r = Ruler { start: [0.0, 0.0], end: [10.0, 0.0], protractor: Some([0.0, 10.0]) };
    let info = ruler_info(&r, &sc);
    assert!(approx_eq(info["angle"].as_f64().unwrap(), 90.0, 1e-9));
    assert!(approx_eq(info["l1"].as_f64().unwrap(), 10.0, 1e-9));
    assert!(approx_eq(info["l2"].as_f64().unwrap(), 10.0, 1e-9));
    assert_eq!(info["protractor"], json!(true));
}

#[test]
fn straighten_angle_nearest_axis() {
    let r0 = Ruler { start: [0.0, 0.0], end: [10.0, 0.0], protractor: None };
    assert_eq!(straighten_angle(&r0), 0.0);

    let r45 = Ruler { start: [0.0, 0.0], end: [10.0, 10.0], protractor: None };
    assert!(approx_eq(straighten_angle(&r45), 45.0, 1e-9));

    let rsmall = Ruler { start: [0.0, 0.0], end: [10.0, 5.0], protractor: None };
    let a = (-5.0f64).atan2(10.0).to_degrees();
    assert!(approx_eq(straighten_angle(&rsmall), a, 1e-9));
}

#[test]
fn log_columns_orders_by_columns() {
    let mut values = Map::new();
    values.insert("area".into(), json!(1.0));
    values.insert("label".into(), json!("x"));
    values.insert("angle".into(), json!(2.0));
    let row = LogRow { id: 1, values };
    let cols = log_columns(std::slice::from_ref(&row));
    assert_eq!(cols, vec!["label", "area", "angle"]);
}

#[test]
fn log_csv_formats_fields_and_quoting() {
    let mut values1 = Map::new();
    values1.insert("label".into(), json!("Simple"));
    values1.insert("histogram".into(), json!([1, 2]));
    values1.insert("scaleFactor".into(), Value::Null);

    let mut values2 = Map::new();
    values2.insert("label".into(), json!("a,b"));
    values2.insert("histogram".into(), json!([]));
    values2.insert("scaleFactor".into(), json!(0.5));

    let rows = vec![LogRow { id: 1, values: values1 }, LogRow { id: 2, values: values2 }];
    let expected = "Label,Scale Factor,Histogram\nSimple,,1 2\n\"a,b\",0.5,\n";
    assert_eq!(log_csv(&rows), expected);
}

#[test]
fn now_iso_matches_expected_format() {
    let s = now_iso();
    assert_eq!(s.len(), 20);
    let b = s.as_bytes();
    assert_eq!(&s[4..5], "-");
    assert_eq!(&s[7..8], "-");
    assert_eq!(&s[10..11], "T");
    assert_eq!(&s[13..14], ":");
    assert_eq!(&s[16..17], ":");
    assert_eq!(&s[19..], "Z");
    for (i, &c) in b.iter().enumerate() {
        if i == 4 || i == 7 {
            assert_eq!(c, b'-');
        } else if i == 10 {
            assert_eq!(c, b'T');
        } else if i == 13 || i == 16 {
            assert_eq!(c, b':');
        } else if i == 19 {
            assert_eq!(c, b'Z');
        } else {
            assert!(c.is_ascii_digit());
        }
    }
}

#[test]
fn specs_are_unique_and_known() {
    let specs = analysis_cmds::specs();
    assert!(!specs.is_empty());
    let mut ids = std::collections::HashSet::new();
    for s in &specs {
        assert!(ids.insert(s.id), "duplicate id {}", s.id);
    }
    assert!(ids.contains("image.analysis.setMeasurementScale"));
    assert!(ids.contains("image.analysis.selectDataPoints"));
    assert!(ids.contains("image.analysis.recordMeasurements"));
    assert!(ids.contains("image.analysis.rulerTool"));
    assert!(ids.contains("image.analysis.countTool"));
    assert!(ids.contains("image.analysis.placeScaleMarker"));
    assert!(ids.contains("image.analysis.straightenLayer"));
    assert!(ids.contains("image.analysis.info"));
    assert!(ids.contains("count.add"));
    assert!(ids.contains("count.remove"));
    assert!(ids.contains("count.move"));
    assert!(ids.contains("count.clear"));
    assert!(ids.contains("count.newGroup"));
    assert!(ids.contains("count.deleteGroup"));
    assert!(ids.contains("count.setGroup"));
    assert!(ids.contains("measurementLog.list"));
    assert!(ids.contains("measurementLog.delete"));
    assert!(ids.contains("measurementLog.export"));
}

#[test]
fn select_data_points_set_full_list() {
    let mut s = Session::new();
    s.execute("image.analysis.selectDataPoints", json!({"selection": ["area", "label"]})).unwrap();
    assert_eq!(s.analysis.data_points.selection, vec!["label".to_string(), "area".to_string()]);
    assert_eq!(s.analysis.data_points.ruler, available("ruler").iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert_eq!(s.analysis.data_points.count, available("count").iter().map(|s| s.to_string()).collect::<Vec<_>>());
}

#[test]
fn select_data_points_object_changes() {
    let mut s = Session::new();
    s.execute("image.analysis.selectDataPoints", json!({"selection": {"area": false}})).unwrap();
    assert!(!s.analysis.data_points.selection.contains(&"area".to_string()));
}

#[test]
fn select_data_points_reset() {
    let mut s = Session::new();
    s.execute("image.analysis.selectDataPoints", json!({"selection": ["label"], "ruler": ["angle"]})).unwrap();
    assert_eq!(s.analysis.data_points.selection, vec!["label".to_string()]);

    s.execute("image.analysis.selectDataPoints", json!({"reset": true})).unwrap();
    assert_eq!(s.analysis.data_points.selection, available("selection").iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert_eq!(s.analysis.data_points.ruler, available("ruler").iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert_eq!(s.analysis.data_points.count, available("count").iter().map(|s| s.to_string()).collect::<Vec<_>>());
}

#[test]
fn select_data_points_invalid_key_errors() {
    let mut s = Session::new();
    assert!(s.execute("image.analysis.selectDataPoints", json!({"selection": ["bogus"]})).is_err());
    assert!(s.execute("image.analysis.selectDataPoints", json!({"selection": [1]})).is_err());
    assert!(s.execute("image.analysis.selectDataPoints", json!({"selection": {"area": "yes"}})).is_err());
}

#[test]
fn measurement_log_list_returns_rows() {
    let mut s = Session::new();
    s.analysis.log.push(LogRow { id: 7, values: Map::new() });
    let res = s.execute("measurementLog.list", json!({})).unwrap();
    assert_eq!(res["rows"][0]["id"], json!(7));
}

#[test]
fn measurement_log_delete_by_ids() {
    let mut s = Session::new();
    for id in 1..=3 {
        s.analysis.log.push(LogRow { id, values: Map::new() });
    }
    let res = s.execute("measurementLog.delete", json!({"rows": [1, 3]})).unwrap();
    assert_eq!(res["deleted"], json!(2));
    assert_eq!(res["remaining"], json!(1));
    assert_eq!(s.analysis.log.len(), 1);
    assert_eq!(s.analysis.log[0].id, 2);
}

#[test]
fn measurement_log_delete_all() {
    let mut s = Session::new();
    for id in 1..=3 {
        s.analysis.log.push(LogRow { id, values: Map::new() });
    }
    let res = s.execute("measurementLog.delete", json!({"all": true})).unwrap();
    assert_eq!(res["deleted"], json!(3));
    assert!(s.analysis.log.is_empty());
}

#[test]
fn measurement_log_delete_missing_errors() {
    let mut s = Session::new();
    assert!(s.execute("measurementLog.delete", json!({"all": true})).is_err());
}

#[test]
fn measurement_log_export_returns_csv() {
    let mut s = Session::new();
    let mut values = Map::new();
    values.insert("label".into(), json!("Hello"));
    values.insert("area".into(), json!(3.5));
    s.analysis.log.push(LogRow { id: 1, values });

    let res = s.execute("measurementLog.export", json!({})).unwrap();
    assert_eq!(res["rows"], json!(1));
    assert_eq!(res["csv"], json!(log_csv(&s.analysis.log)));
}

#[test]
fn measurement_log_export_csv_to_temp_file() {
    let mut s = Session::new();
    let mut values = Map::new();
    values.insert("label".into(), json!("Temp"));
    values.insert("angle".into(), json!(12.5));
    s.analysis.log.push(LogRow { id: 1, values });

    let unique =
        format!("photocraft_analysis_test_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("log.csv");
    let path_str = path.to_str().unwrap().to_string();

    let res = s.execute("measurementLog.export", json!({"path": path_str})).unwrap();
    assert_eq!(res["rows"], json!(1));
    assert_eq!(res["path"], json!(path_str));
    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, log_csv(&s.analysis.log));

    let _ = std::fs::remove_dir_all(&dir);
}
