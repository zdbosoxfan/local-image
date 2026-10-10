use std::fs;
use std::path::{Path, PathBuf};

use lightcraft_engine::Session;
use lightcraft_engine::catalog::PhotoId;
use lightcraft_engine::develop::SegMask;
use lightcraft_engine::segment::{self, Segmenter, detail_region};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lc_seg_api_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn seg_box(side: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> SegMask {
    let logits: Vec<f32> = (0..side * side)
        .map(|i| {
            let x = i % side;
            let y = i / side;
            if (x0..x1).contains(&x) && (y0..y1).contains(&y) { 8.0 } else { -8.0 }
        })
        .collect();
    SegMask::from_logits(side, &logits)
}

#[test]
fn detail_region_no_logits_returns_none() {
    let seg = SegMask { side: 50, data: "?".into(), rect: None };
    assert!(detail_region(&seg, 1.0).is_none());
}

#[test]
fn detail_region_empty_selection_returns_none() {
    let seg = seg_box(50, 0, 0, 0, 0);
    assert!(detail_region(&seg, 1.0).is_none());
}

#[test]
fn detail_region_large_selection_returns_none() {
    let seg = seg_box(50, 2, 2, 48, 48);
    assert!(detail_region(&seg, 1.0).is_none());
}

#[test]
fn detail_region_frames_small_center_object() {
    let seg = seg_box(100, 40, 45, 50, 55);
    let r = detail_region(&seg, 1.0).unwrap();
    assert!(r[0] < 0.4 && r[2] > 0.5 && r[1] < 0.45 && r[3] > 0.55, "contains it: {r:?}");
    assert!(r[2] - r[0] < 0.25 && r[3] - r[1] < 0.25, "zoomed in: {r:?}");
    assert!(r.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn detail_region_clamps_to_photo_edges() {
    let seg = seg_box(100, 0, 0, 5, 5);
    let r = detail_region(&seg, 1.5).unwrap();
    assert!(r[0] >= 0.0 && r[1] >= 0.0 && r[0] < 1e-9 && r[1] < 1e-9);
    assert!(r.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn detail_region_respects_aspect_ratio() {
    let seg = seg_box(100, 10, 50, 90, 52);
    let r = detail_region(&seg, 1.5).unwrap();
    let w = (r[2] - r[0]) * 1.5;
    let h = r[3] - r[1];
    assert!(w / h <= 2.0 + 1e-9, "{w} / {h} <= 2");
    assert!(r.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn detail_region_handles_extreme_aspect() {
    let seg = seg_box(100, 40, 45, 50, 55);
    for aspect in [0.0, 0.1, 10.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let r = detail_region(&seg, aspect).unwrap();
        assert!(r.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "aspect {aspect}: {r:?}");
    }
}

#[test]
fn segmenter_default_state() {
    let s = Segmenter::default();
    assert!(!s.installed());
    assert!(!s.busy());
    assert!(!s.analyzing());
    assert!(!s.detail_busy());
    assert!(!s.loaded());
    assert!(s.pending_clicks().is_none());

    let status = s.download_status();
    assert!(!status.running);
    assert_eq!(status.done, 0);
    assert_eq!(status.total, 0);
    assert_eq!(status.file, "");
    assert_eq!(status.error, None);
    assert!(!status.finished);

    assert!(s.mirrors().is_empty());
    assert!(!s.cancel_download());
}

#[test]
fn segmenter_model_dir_without_dir_errors() {
    let s = Segmenter::default();
    let err = s.model_dir().unwrap_err();
    if Segmenter::AVAILABLE {
        assert_eq!(err, "no folder is set for the SAM 3 model");
    } else {
        assert_eq!(err, "AI masks are not available in this build");
    }
}

#[test]
fn segmenter_model_dir_with_uninstalled_dir() {
    let tmp = TempDir::new();
    let mut s = Segmenter::default();
    s.dir = Some(tmp.path().to_path_buf());
    let result = s.model_dir();
    if Segmenter::AVAILABLE {
        let err = result.unwrap_err();
        assert!(err.starts_with(segment::NOT_INSTALLED), "{err}");
    } else {
        assert_eq!(result.unwrap_err(), "AI masks are not available in this build");
    }
}

#[test]
fn segmenter_installed_false_for_empty_dir() {
    let tmp = TempDir::new();
    let mut s = Segmenter::default();
    s.dir = Some(tmp.path().to_path_buf());
    assert!(!s.installed());
}

#[test]
fn segmenter_start_download_without_dir_errors() {
    let s = Segmenter::default();
    let err = s.start_download().unwrap_err();
    if Segmenter::AVAILABLE {
        assert_eq!(err, "no folder is set for the SAM 3 model");
    } else {
        assert_eq!(err, "AI masks are not available in this build");
    }
}

#[test]
fn session_segment_poll_default() {
    let mut session = Session::new();
    let polled = session.segment_poll();
    assert!(!polled.changed);
    assert!(polled.messages.is_empty());
    assert!(polled.refine.is_none());
}

#[test]
fn session_segment_prepare_no_dir_errors() {
    let mut session = Session::new();
    let err = session.segment_prepare(PhotoId(0)).unwrap_err();
    if Segmenter::AVAILABLE {
        assert_eq!(err, "no folder is set for the SAM 3 model");
    } else {
        assert_eq!(err, "AI masks are not available in this build");
    }
}

#[test]
fn session_segment_clicks_no_dir_errors() {
    let mut session = Session::new();
    let err = session.segment_clicks(PhotoId(0), &[]).unwrap_err();
    if Segmenter::AVAILABLE {
        assert_eq!(err, "no folder is set for the SAM 3 model");
    } else {
        assert_eq!(err, "AI masks are not available in this build");
    }
}

#[test]
fn session_segment_text_no_dir_errors() {
    let mut session = Session::new();
    let err = session.segment_text(PhotoId(0), "hello").unwrap_err();
    if Segmenter::AVAILABLE {
        assert_eq!(err, "no folder is set for the SAM 3 model");
    } else {
        assert_eq!(err, "AI masks are not available in this build");
    }
}

#[test]
fn session_segment_detail_no_dir_errors() {
    let mut session = Session::new();
    let err = session.segment_detail(PhotoId(0), 0, 0).unwrap_err();
    if Segmenter::AVAILABLE {
        assert_eq!(err, "no folder is set for the SAM 3 model");
    } else {
        assert_eq!(err, "AI masks are not available in this build");
    }
}

#[test]
fn session_segment_requests_with_uninstalled_dir_error() {
    let tmp = TempDir::new();
    let mut session = Session::new();
    session.segmenter.dir = Some(tmp.path().to_path_buf());

    let err = session.segment_prepare(PhotoId(0)).unwrap_err();
    if Segmenter::AVAILABLE {
        assert!(err.starts_with(segment::NOT_INSTALLED), "{err}");
    } else {
        assert_eq!(err, "AI masks are not available in this build");
    }

    let err = session.segment_clicks(PhotoId(0), &[]).unwrap_err();
    if Segmenter::AVAILABLE {
        assert!(err.starts_with(segment::NOT_INSTALLED), "{err}");
    } else {
        assert_eq!(err, "AI masks are not available in this build");
    }

    let err = session.segment_text(PhotoId(0), "hello").unwrap_err();
    if Segmenter::AVAILABLE {
        assert!(err.starts_with(segment::NOT_INSTALLED), "{err}");
    } else {
        assert_eq!(err, "AI masks are not available in this build");
    }

    let err = session.segment_detail(PhotoId(0), 0, 0).unwrap_err();
    if Segmenter::AVAILABLE {
        assert!(err.starts_with(segment::NOT_INSTALLED), "{err}");
    } else {
        assert_eq!(err, "AI masks are not available in this build");
    }
}
