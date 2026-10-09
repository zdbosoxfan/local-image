use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use photocraft_engine::doc::Adjustment;
use photocraft_engine::{EngineError, Session, adjust_cmds};
use serde_json::json;

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

static TEMP_COUNTER: AtomicU32 = AtomicU32::new(0);

fn unique_temp_dir() -> PathBuf {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("photocraft_adj_cmds_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn selective_default() {
    let p = json!({});
    let adj = adjust_cmds::selective_from_params(&p, None);
    match adj {
        Adjustment::SelectiveColor { relative, adjustments } => {
            assert!(relative);
            assert_eq!(adjustments.len(), 9);
            assert_eq!(adjustments, [[0.0; 4]; 9]);
        }
        _ => panic!("expected SelectiveColor"),
    }
}

#[test]
fn selective_base_merge_preserves_unmentioned_ranges() {
    let base = Adjustment::SelectiveColor { relative: false, adjustments: [[1.0; 4]; 9] };
    let p = json!({"reds":[10.0, 20.0, 30.0, 40.0]});
    let adj = adjust_cmds::selective_from_params(&p, Some(&base));
    match adj {
        Adjustment::SelectiveColor { relative, adjustments } => {
            assert!(!relative);
            assert_eq!(adjustments[0], [10.0, 20.0, 30.0, 40.0]);
            for i in 1..9 {
                assert_eq!(adjustments[i], [1.0; 4]);
            }
        }
        _ => panic!("expected SelectiveColor"),
    }
}

#[test]
fn selective_method_absolute_changes_relative() {
    let p = json!({"method":"absolute"});
    let adj = adjust_cmds::selective_from_params(&p, None);
    match adj {
        Adjustment::SelectiveColor { relative, .. } => assert!(!relative),
        _ => panic!("expected SelectiveColor"),
    }
}

#[test]
fn selective_relative_bool_overrides_method() {
    let p = json!({"method":"absolute", "relative": true});
    let adj = adjust_cmds::selective_from_params(&p, None);
    match adj {
        Adjustment::SelectiveColor { relative, .. } => assert!(relative),
        _ => panic!("expected SelectiveColor"),
    }
}

#[test]
fn selective_values_clamp_to_100() {
    let p = json!({"reds":[150.0, -150.0, 50.0, -50.0]});
    let adj = adjust_cmds::selective_from_params(&p, None);
    match adj {
        Adjustment::SelectiveColor { adjustments, .. } => {
            assert_eq!(adjustments[0], [100.0, -100.0, 50.0, -50.0]);
        }
        _ => panic!("expected SelectiveColor"),
    }
}

#[test]
fn selective_ignores_non_numeric_array_entries() {
    let p = json!({"reds":["x", null, 3.0, 4.0]});
    let adj = adjust_cmds::selective_from_params(&p, None);
    match adj {
        Adjustment::SelectiveColor { adjustments, .. } => {
            assert_eq!(adjustments[0], [0.0, 0.0, 3.0, 4.0]);
        }
        _ => panic!("expected SelectiveColor"),
    }
}

#[test]
fn selective_colors_single_range_updates_one() {
    let p = json!({
        "colors": "blues",
        "cyan": 10.0,
        "magenta": 20.0,
        "yellow": 30.0,
        "black": 40.0
    });
    let adj = adjust_cmds::selective_from_params(&p, None);
    match adj {
        Adjustment::SelectiveColor { adjustments, .. } => {
            assert_eq!(adjustments[4], [10.0, 20.0, 30.0, 40.0]);
            for i in 0..9 {
                if i != 4 {
                    assert_eq!(adjustments[i], [0.0; 4]);
                }
            }
        }
        _ => panic!("expected SelectiveColor"),
    }
}

#[test]
fn selective_infinity_from_large_f64_clamps() {
    let p = json!({"reds":[1e300, -1e300, 0.0, 0.0]});
    let adj = adjust_cmds::selective_from_params(&p, None);
    match adj {
        Adjustment::SelectiveColor { adjustments, .. } => {
            assert_eq!(adjustments[0][0], 100.0);
            assert_eq!(adjustments[0][1], -100.0);
        }
        _ => panic!("expected SelectiveColor"),
    }
}

#[test]
fn selective_extra_keys_ignored() {
    let p = json!({"reds":[10.0,20.0,30.0,40.0], "foo":"bar"});
    let adj = adjust_cmds::selective_from_params(&p, None);
    match adj {
        Adjustment::SelectiveColor { adjustments, .. } => {
            assert_eq!(adjustments[0], [10.0, 20.0, 30.0, 40.0]);
            for i in 1..9 {
                assert_eq!(adjustments[i], [0.0; 4]);
            }
        }
        _ => panic!("expected SelectiveColor"),
    }
}

#[test]
fn lookup_none_clears_lut() {
    let first_id = adjust_cmds::LOOKS[0].0;
    let base = adjust_cmds::lookup_from_params(&json!({"lut": first_id}), None).unwrap();
    let adj = adjust_cmds::lookup_from_params(&json!({"lut":"none"}), Some(&base)).unwrap();
    match adj {
        Adjustment::ColorLookup { name, lut, size, .. } => {
            assert!(name.is_empty());
            assert!(lut.is_none());
            assert_eq!(size, 0);
        }
        _ => panic!("expected ColorLookup"),
    }
}

#[test]
fn lookup_unknown_lut_returns_err() {
    let p = json!({"lut":"does-not-exist"});
    let res = adjust_cmds::lookup_from_params(&p, None);
    assert!(res.is_err());
    match res.unwrap_err() {
        EngineError::BadParams { cmd, msg } => {
            assert_eq!(cmd, "colorLookup");
            assert!(msg.contains("unknown look"), "msg: {msg}");
        }
        other => panic!("expected BadParams, got {other:?}"),
    }
}

#[test]
fn lookup_builtin_success() {
    let first = adjust_cmds::LOOKS[0];
    let adj = adjust_cmds::lookup_from_params(&json!({"lut": first.0}), None).unwrap();
    match adj {
        Adjustment::ColorLookup { name, lut, size, tetrahedral, dither } => {
            assert_eq!(name, first.1);
            assert!(lut.is_some());
            assert!(size > 0);
            assert!(!tetrahedral);
            assert!(!dither);
        }
        _ => panic!("expected ColorLookup"),
    }
}

#[test]
fn lookup_data_malformed_returns_err() {
    let p = json!({"data":"this is not a lut", "fileName":"x.cube"});
    let res = adjust_cmds::lookup_from_params(&p, None);
    assert!(res.is_err());
    match res.unwrap_err() {
        EngineError::BadParams { cmd, .. } => assert_eq!(cmd, "colorLookup"),
        other => panic!("expected BadParams, got {other:?}"),
    }
}

#[test]
fn lookup_file_nonexistent_returns_err() {
    let p = json!({"file":"/definitely/not/a/real/path.cube"});
    let res = adjust_cmds::lookup_from_params(&p, None);
    assert!(res.is_err());
}

#[test]
fn lookup_file_malformed_returns_err() {
    let dir = unique_temp_dir();
    let _guard = TempDirGuard(dir.clone());
    let file_path = dir.join("malformed.cube");
    std::fs::write(&file_path, b"bad data").unwrap();
    let file_str = file_path.to_str().unwrap();
    let p = json!({"file": file_str});
    let res = adjust_cmds::lookup_from_params(&p, None);
    assert!(res.is_err());
    match res.unwrap_err() {
        EngineError::BadParams { cmd, .. } => assert_eq!(cmd, "colorLookup"),
        other => panic!("expected BadParams, got {other:?}"),
    }
}

#[test]
fn lookup_base_preserve_with_partial_params() {
    let first_id = adjust_cmds::LOOKS[0].0;
    let base = adjust_cmds::lookup_from_params(&json!({"lut": first_id}), None).unwrap();
    let adj = adjust_cmds::lookup_from_params(&json!({"interpolation":"tetrahedral","dither":true}), Some(&base)).unwrap();
    match (base, adj) {
        (Adjustment::ColorLookup { name: n1, lut: l1, size: s1, .. }, Adjustment::ColorLookup { name: n2, lut: l2, size: s2, tetrahedral, dither }) => {
            assert_eq!(n1, n2);
            assert!(l1.is_some());
            assert!(l2.is_some());
            assert_eq!(s1, s2);
            assert!(tetrahedral);
            assert!(dither);
        }
        _ => panic!("expected ColorLookup pair"),
    }
}

#[test]
fn update_adjustment_selective_merges() {
    let base = adjust_cmds::selective_from_params(&json!({"method":"absolute","reds":[10.0,0.0,0.0,0.0]}), None);
    let p = json!({"reds":[20.0,30.0,40.0,50.0]});
    let res = adjust_cmds::update_adjustment(&base, &p).unwrap().unwrap();
    match res {
        Adjustment::SelectiveColor { relative, adjustments } => {
            assert!(!relative);
            assert_eq!(adjustments[0], [20.0, 30.0, 40.0, 50.0]);
            for i in 1..9 {
                assert_eq!(adjustments[i], [0.0; 4]);
            }
        }
        _ => panic!("expected SelectiveColor"),
    }
}

#[test]
fn update_adjustment_color_lookup_merges() {
    let first_id = adjust_cmds::LOOKS[0].0;
    let base = adjust_cmds::lookup_from_params(&json!({"lut": first_id}), None).unwrap();
    let p = json!({"interpolation":"tetrahedral","dither":true});
    let res = adjust_cmds::update_adjustment(&base, &p).unwrap().unwrap();
    match res {
        Adjustment::ColorLookup { tetrahedral, dither, name, lut, size } => {
            assert!(tetrahedral);
            assert!(dither);
            assert_eq!(name, adjust_cmds::LOOKS[0].1);
            assert!(lut.is_some());
            assert!(size > 0);
        }
        _ => panic!("expected ColorLookup"),
    }
}

#[test]
fn update_adjustment_empty_params_resets_to_defaults() {
    let base = adjust_cmds::selective_from_params(&json!({"method":"absolute","reds":[10.0,20.0,30.0,40.0]}), None);
    let res = adjust_cmds::update_adjustment(&base, &json!({})).unwrap().unwrap();
    match res {
        Adjustment::SelectiveColor { relative, adjustments } => {
            assert!(relative);
            assert_eq!(adjustments, [[0.0; 4]; 9]);
        }
        _ => panic!("expected SelectiveColor"),
    }
}

#[test]
fn list_looks_command_returns_builtins() {
    let mut session = Session::new();
    let result = session.execute("image.adjustments.colorLookup.list", json!({})).unwrap();
    let arr = result.as_array().expect("array");
    assert_eq!(arr.len(), adjust_cmds::LOOKS.len());
    let first = &arr[0];
    let obj = first.as_object().unwrap();
    assert!(obj.get("id").unwrap().is_string());
    assert!(obj.get("label").unwrap().is_string());
}

#[test]
fn color_lookup_list_is_enabled_without_doc() {
    let session = Session::new();
    assert!(session.is_enabled("image.adjustments.colorLookup.list"));
    assert_eq!(session.disabled_reason("image.adjustments.colorLookup.list"), None);
}

#[test]
fn hdr_toning_disabled_without_doc() {
    let session = Session::new();
    let reason = session.disabled_reason("image.adjustments.hdrToning").unwrap();
    assert_eq!(reason, "no document open");
}

#[test]
fn replace_color_disabled_without_doc() {
    let session = Session::new();
    let reason = session.disabled_reason("image.adjustments.replaceColor").unwrap();
    assert_eq!(reason, "no document open");
}
