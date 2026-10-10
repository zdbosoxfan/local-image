use std::sync::Arc;

use photocraft_engine::color_cmds::ColorState;
use photocraft_engine::display_color::{CanvasDisplay, Display, MonitorDetection, MonitorStatus, display_at};

fn sample_display(id: u32, x: f64, y: f64, w: f64, h: f64, icc: Option<Vec<u8>>) -> Display {
    Display { id, name: format!("Display {id}"), frame: [x, y, w, h], profile_name: Some(format!("Profile {id}")), icc: icc.map(Arc::new) }
}

fn default_color_state() -> ColorState {
    ColorState::default()
}

#[test]
fn display_at_empty_returns_none() {
    assert_eq!(display_at(&[], [0.0, 0.0, 10.0, 10.0]), None);
}

#[test]
fn display_at_returns_overlapping_display() {
    let d = sample_display(1, 0.0, 0.0, 100.0, 100.0, None);
    assert_eq!(display_at(&[d], [10.0, 10.0, 20.0, 20.0]), Some(1));
}

#[test]
fn display_at_ignores_touching_displays() {
    let d = sample_display(1, 100.0, 0.0, 100.0, 100.0, None);
    assert_eq!(display_at(&[d], [0.0, 0.0, 100.0, 100.0]), None);
}

#[test]
fn display_at_picks_largest_overlap() {
    let d1 = sample_display(1, 0.0, 0.0, 100.0, 100.0, None);
    let d2 = sample_display(2, 50.0, 0.0, 100.0, 100.0, None);
    // rect overlaps d1 fully and d2 partially; d1's intersection area is larger
    assert_eq!(display_at(&[d1, d2], [0.0, 0.0, 100.0, 100.0]), Some(1));
}

#[test]
fn display_at_nan_rect_returns_none() {
    let d = sample_display(1, 0.0, 0.0, 100.0, 100.0, None);
    assert_eq!(display_at(&[d], [f64::NAN, 0.0, 10.0, 10.0]), None);
}

#[test]
fn monitor_detection_default_is_unsupported() {
    assert_eq!(MonitorDetection::default(), MonitorDetection::Unsupported);
}

#[test]
fn monitor_status_summary_auto_uses_profile_name() {
    let status = MonitorStatus {
        requested: "auto".into(),
        source: "auto",
        display: Some("Main".into()),
        profile: "sRGB Profile".into(),
        profile_name: Some("sRGB IEC61966-2.1".into()),
        fingerprint: "abc".into(),
        detection: MonitorDetection::Found,
        reason: None,
    };
    assert_eq!(status.summary(), "sRGB IEC61966-2.1 (auto)");
}

#[test]
fn monitor_status_summary_manual_uses_profile() {
    let status = MonitorStatus {
        requested: "custom.icc".into(),
        source: "manual",
        display: Some("Main".into()),
        profile: "Custom Profile".into(),
        profile_name: Some("OS Name".into()),
        fingerprint: "abc".into(),
        detection: MonitorDetection::Found,
        reason: None,
    };
    assert_eq!(status.summary(), "Custom Profile (manual)");
}

#[test]
fn monitor_status_summary_reason_appended() {
    let status = MonitorStatus {
        requested: "auto".into(),
        source: "fallback",
        display: None,
        profile: "sRGB".into(),
        profile_name: None,
        fingerprint: "abc".into(),
        detection: MonitorDetection::Failed { reason: "bad profile".into() },
        reason: Some("bad profile".into()),
    };
    assert_eq!(status.summary(), "sRGB (fallback: bad profile)");
}

#[test]
fn default_monitor_is_srgb_fallback() {
    let cs = default_color_state();
    let status = cs.monitor_status_for(None);
    assert_eq!(status.source, "fallback");
    assert_eq!(status.detection, MonitorDetection::Unsupported);
    assert_eq!(status.requested, "auto");
    assert!(status.reason.is_some());
    assert!(!status.profile.is_empty());
    assert!(!status.fingerprint.is_empty());
}

#[test]
fn monitor_for_none_is_cached_and_consistent() {
    let cs = default_color_state();
    let p1 = cs.monitor_for(None);
    let p2 = cs.monitor_for(None);
    assert!(Arc::ptr_eq(&p1, &p2));
    let s1 = cs.monitor_status_for(None);
    let s2 = cs.monitor_status_for(None);
    assert_eq!(s1, s2);
}

#[test]
fn set_displays_empty_reports_failure_and_fallback() {
    let mut cs = default_color_state();
    let r = cs.set_displays(Ok(vec![]));
    assert_eq!(r, None);
    let status = cs.monitor_status_for(None);
    assert_eq!(status.source, "fallback");
    assert_eq!(status.detection, MonitorDetection::Failed { reason: "the platform reported no displays".to_string() });
    assert!(status.reason.is_some());
}

#[test]
fn set_displays_error_reports_failure_and_fallback() {
    let mut cs = default_color_state();
    let r = cs.set_displays(Err("platform reader failed".to_string()));
    assert_eq!(r, None);
    let status = cs.monitor_status_for(None);
    assert_eq!(status.source, "fallback");
    assert_eq!(status.detection, MonitorDetection::Failed { reason: "platform reader failed".to_string() });
    assert!(status.reason.is_some());
}

#[test]
fn set_displays_records_displays_and_retains_after_error() {
    let mut cs = default_color_state();
    let d = sample_display(7, 0.0, 0.0, 100.0, 100.0, None);
    let r = cs.set_displays(Ok(vec![d.clone()]));
    assert_eq!(r, None);

    let status = cs.monitor_status_for(Some(7));
    assert_eq!(status.detection, MonitorDetection::Found);
    assert_eq!(status.source, "fallback");
    assert!(status.reason.as_deref().unwrap_or("").contains("has no ICC profile"));

    let r = cs.set_displays(Err("re-read failed".to_string()));
    assert_eq!(r, Some("re-read failed".to_string()));

    let status_after = cs.monitor_status_for(Some(7));
    assert_eq!(status_after.detection, MonitorDetection::Retained { reason: "re-read failed".to_string() });
    assert_eq!(status_after.source, "fallback");
    assert!(status_after.reason.as_deref().unwrap_or("").contains("has no ICC profile"));
    assert_eq!(status_after.display.as_deref(), Some("Display 7"));
}

#[test]
fn display_statuses_returns_one_per_display() {
    let mut cs = default_color_state();
    let d1 = sample_display(1, 0.0, 0.0, 100.0, 100.0, None);
    let d2 = sample_display(2, 100.0, 0.0, 100.0, 100.0, None);
    let _ = cs.set_displays(Ok(vec![d1, d2]));

    let statuses = cs.display_statuses();
    assert_eq!(statuses.len(), 2);

    let first = statuses[0].as_object().expect("first status object");
    assert_eq!(first["id"].as_u64(), Some(1));
    assert_eq!(first["name"].as_str(), Some("Display 1"));
    let frame = first["frame"].as_array().expect("frame array");
    assert_eq!(frame.len(), 4);
    assert_eq!(frame[0].as_f64(), Some(0.0));
    assert_eq!(frame[1].as_f64(), Some(0.0));
    assert_eq!(frame[2].as_f64(), Some(100.0));
    assert_eq!(frame[3].as_f64(), Some(100.0));

    let nested = first["status"].as_object().expect("nested status object");
    assert_eq!(nested["source"].as_str(), Some("fallback"));
    assert_eq!(nested["requested"].as_str(), Some("auto"));

    let second = statuses[1].as_object().expect("second status object");
    assert_eq!(second["id"].as_u64(), Some(2));
    assert_eq!(second["name"].as_str(), Some("Display 2"));
}

#[test]
fn canvas_display_identity_flag_reflects_encoding_and_transform() {
    let cs = default_color_state();
    let profile = cs.monitor_for(None);

    let no_op = CanvasDisplay { encode_srgb: false, source: profile.clone(), monitor: profile.clone(), transform: None, key: 1, texture_key: 2 };
    assert!(no_op.is_identity());

    let encoded = CanvasDisplay { encode_srgb: true, source: profile.clone(), monitor: profile.clone(), transform: None, key: 3, texture_key: 4 };
    assert!(!encoded.is_identity());
}
