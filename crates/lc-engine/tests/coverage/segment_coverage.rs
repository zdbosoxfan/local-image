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
fn detail_region_handles_extreme_aspect() {
    let seg = seg_box(100, 40, 45, 50, 55);
    for aspect in [0.0, 0.1, 10.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let r = detail_region(&seg, aspect).unwrap();
        assert!(r.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "aspect {aspect}: {r:?}");
    }
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
