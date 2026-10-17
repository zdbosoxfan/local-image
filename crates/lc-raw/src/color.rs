//! DNG colour model (Adobe DNG Specification 1.7, chapter 6 "Mapping Camera Color Space to CIE XYZ Space").
//!
//! - `ColorMatrix1/2/3` (XYZ → reference camera) and `CameraCalibration1/2` / `ForwardMatrix1/2` are interpolated
//!   with inverse-CCT dual weights or DNG 1.6 scaled temperature/tint triple weights.
//! - `XYZtoCamera = AnalogBalance · CameraCalibration · ColorMatrix`.
//! - White balance multipliers are the reciprocal of the camera neutral for the chosen white.
//! - Camera → XYZ(D50): through `ForwardMatrix` when present, else the inverse of `XYZtoCamera` followed by
//!   Bradford adaptation from the white to D50.
//! - The as-shot white comes from `AsShotWhiteXY`, or `AsShotNeutral` converted to xy by the spec's iteration.
//!
//! Cameras without a colour matrix (non-DNG files until our own calibration DB exists) use a generic
//! "camera RGB ≈ linear sRGB" model and are flagged with `matrix_is_fallback`.

use crate::{ColorData, Mat3, RawImage};
use li_color::{D50, D65, REC2020, Xy, bradford};
use serde::{Deserialize, Serialize};

/// Shared camera calibration, carried through the develop white-balance stage unchanged.
pub fn camera_color(color: &ColorData) -> li_color::camera::CameraColor {
    // DNG chapter 6 / SDK dng_color_spec: calibrations belong to the profile's
    // reference camera space only when both calibration signatures match.
    let calibration_matches = color.camera_calibration_signature == color.profile_calibration_signature;
    li_color::camera::CameraColor {
        illuminant: color.illuminant,
        illuminant_3: color.illuminant_3,
        illuminant_white_xy: color.illuminant_white_xy,
        color_matrix: color.color_matrix,
        color_matrix_3: color.color_matrix_3,
        forward_matrix: color.forward_matrix,
        forward_matrix_3: color.forward_matrix_3,
        camera_calibration: if calibration_matches { color.camera_calibration } else { [None; 2] },
        camera_calibration_3: if calibration_matches { color.camera_calibration_3 } else { None },
        analog_balance: color.analog_balance,
    }
}

pub use li_color::camera::illuminant_temperature;

/// Weight of calibration 1 for legacy dual-profile callers.
pub fn illuminant_weight(color: &ColorData, white: Xy) -> f64 {
    li_color::camera::illuminant_weight(&camera_color(color), white)
}

/// DNG single/dual/triple calibration weights, shared by matrices and hue/saturation maps.
pub fn illuminant_weights(color: &ColorData, white: Xy) -> [f64; 3] {
    li_color::camera::illuminant_weights(&camera_color(color), white)
}

/// Whether a usable primary colour matrix is available.
pub fn has_matrix(color: &ColorData) -> bool {
    li_color::camera::has_matrix(&camera_color(color))
}

/// `AnalogBalance · CameraCalibration · ColorMatrix` for the chosen white.
pub fn xyz_to_camera(color: &ColorData, white: Xy) -> Mat3 {
    li_color::camera::xyz_to_camera(&camera_color(color), white)
}

/// Camera neutral for the chosen white, normalized to max 1.
pub fn camera_neutral(color: &ColorData, white: Xy) -> [f64; 3] {
    li_color::camera::camera_neutral(&camera_color(color), white)
}

/// White-balance multipliers, minimum 1.
pub fn wb_multipliers(color: &ColorData, white: Xy) -> [f64; 3] {
    li_color::camera::wb_multipliers(&camera_color(color), white)
}

/// DNG iterative inverse from camera neutral to chromaticity.
pub fn neutral_to_xy(color: &ColorData, neutral: [f64; 3]) -> Xy {
    li_color::camera::neutral_to_xy(&camera_color(color), neutral)
}

/// The as-shot white point: `AsShotWhiteXY`, `AsShotNeutral`, vendor multipliers, else D65.
pub fn as_shot_white_xy(raw: &RawImage) -> Xy {
    as_shot_white(&raw.color, raw.wb_multipliers)
}

/// [`as_shot_white_xy`] from a header-only [`crate::RawInfo`].
pub fn as_shot_white_xy_of(info: &crate::RawInfo) -> Xy {
    as_shot_white(&info.color, info.wb_multipliers)
}

fn as_shot_white(c: &ColorData, wb_multipliers: Option<[f32; 3]>) -> Xy {
    if let Some(xy) = c.as_shot_white_xy {
        return xy;
    }
    if let Some(n) = c.as_shot_neutral {
        return neutral_to_xy(c, n);
    }
    if let Some(m) = wb_multipliers.filter(|m| m.iter().all(|v| *v > 0.0 && v.is_finite())) {
        return neutral_to_xy(c, [1.0 / m[0] as f64, 1.0 / m[1] as f64, 1.0 / m[2] as f64]);
    }
    D65
}

/// Camera (raw, not white balanced) → XYZ relative to D50, for scene white `xy`.
pub fn camera_to_xyz_d50(color: &ColorData, white: Xy) -> Mat3 {
    li_color::camera::camera_to_xyz_d50(&camera_color(color), white)
}

/// Everything the pipeline needs to go from raw camera RGB to the working space.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraTransform {
    /// White-balanced camera RGB → linear Rec.2020 D65; maps (1, 1, 1) to (1, 1, 1).
    pub matrix: Mat3,
    /// Per-channel white-balance multipliers applied before `matrix` (minimum 1).
    pub wb: [f32; 3],
    pub white_xy: Xy,
    /// No colour matrix was available: a generic camera ≈ sRGB model was used.
    pub matrix_is_fallback: bool,
    pub baseline_exposure: f64,
}

/// Normalization changes vendor sensor coordinates, not file calibration metadata.
/// Keep this derived gain out of RawInfo: frame-measured black is absent from headers.
pub fn normalization_ratios(raw: &RawImage) -> [f64; 3] {
    if raw.format == crate::RawFormat::Dng || raw.cpp != 1 || raw.white.len() != 4 {
        return [1.0; 3];
    }
    let Some(cfa) = raw.cfa.as_ref() else {
        return [1.0; 3];
    };
    let a = raw.active_area;
    let (mut sum, mut count) = ([0.0f64; 3], [0u32; 3]);
    for y in 0..cfa.height {
        for x in 0..cfa.width {
            let c = cfa.color_at(a.x + x, a.y + y).min(2) as usize;
            sum[c] += (raw.sensor_white(a.x + x, a.y + y, 0) - raw.black.at(x, y, 0, 1)).max(1.0) as f64;
            count[c] += 1;
        }
    }
    let ranges: [f64; 3] = std::array::from_fn(|c| sum[c] / count[c].max(1) as f64);
    std::array::from_fn(|c| ranges[1] / ranges[c].max(1.0))
}
/// The colour model for normalized samples; stored file tags remain untouched.
pub fn normalized_color(raw: &RawImage) -> ColorData {
    let mut color = raw.color.clone();
    let ratio = normalization_ratios(raw);
    if ratio != [1.0; 3] {
        let ab = color.analog_balance.unwrap_or([1.0; 3]);
        color.analog_balance = Some(std::array::from_fn(|i| ab[i] * ratio[i]));
        if let Some(n) = color.as_shot_neutral {
            color.as_shot_neutral = Some(std::array::from_fn(|i| n[i] * ratio[i]));
        }
    }
    color
}

/// Camera transform for white `wb_xy`: `rec2020 = matrix · (wb ⊙ camera)`.
pub fn camera_transform(raw: &RawImage, wb_xy: Xy) -> CameraTransform {
    let normalized = normalized_color(raw);
    let color = &normalized;
    let fallback = !has_matrix(color);
    let wb = wb_multipliers(color, wb_xy);
    let to_d50 = camera_to_xyz_d50(color, wb_xy);
    let m = REC2020.from_xyz().mul(&bradford(D50, D65)).mul(&to_d50).mul(&Mat3::diag(1.0 / wb[0], 1.0 / wb[1], 1.0 / wb[2]));
    let white = m.apply([1.0; 3]);
    let k = (white[0] + white[1] + white[2]) / 3.0;
    let matrix = if k.abs() > 1e-12 { Mat3(m.0.map(|r| r.map(|v| v / k))) } else { m };
    CameraTransform { matrix, wb: wb.map(|v| v as f32), white_xy: wb_xy, matrix_is_fallback: fallback, baseline_exposure: color.baseline_exposure }
}

/// `(matrix, multipliers)` with `rec2020 = matrix · (multipliers ⊙ camera)`, linear Rec.2020 D65.
pub fn camera_to_rec2020(raw: &RawImage, wb_xy: Xy) -> (Mat3, [f32; 3]) {
    let t = camera_transform(raw, wb_xy);
    (t.matrix, t.wb)
}

/// Grey-world white balance estimate (multipliers, min 1) from camera RGB, ignoring clipped and very dark pixels.
pub fn grey_world(img: &li_raster::RgbFrame) -> [f32; 3] {
    let mut s = [0f64; 3];
    let step = (img.data.len() / 200_000).max(1);
    for p in img.data.iter().step_by(step) {
        if p.iter().all(|&v| v > 0.02 && v < 0.95) {
            for c in 0..3 {
                s[c] += p[c] as f64;
            }
        }
    }
    if s.iter().any(|&v| v <= 0.0) {
        return [1.0; 3];
    }
    let m = [s[1] / s[0], 1.0, s[1] / s[2]];
    let mn = m.iter().cloned().fold(f64::MAX, f64::min);
    m.map(|v| (v / mn) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use li_color::{SRGB, cct};

    /// A plausible camera: XYZ → camera matrix (rows sum to something sensible), invertible.
    fn cam_a() -> Mat3 {
        Mat3([[0.9, 0.2, -0.15], [-0.3, 1.25, 0.08], [0.02, -0.12, 0.85]])
    }
    fn cam_b() -> Mat3 {
        Mat3([[0.7, 0.3, -0.1], [-0.35, 1.3, 0.1], [0.05, -0.2, 1.0]])
    }

    fn color_single(m: Mat3, ill: u16) -> ColorData {
        ColorData { illuminant: [ill, 0], color_matrix: [Some(m), None], ..Default::default() }
    }

    fn raw_with(color: ColorData) -> RawImage {
        RawImage {
            format: crate::RawFormat::Dng,
            width: 1,
            height: 1,
            cpp: 1,
            data: crate::RawData::U16(vec![0]),
            cfa: None,
            bits: 16,
            black: Default::default(),
            white: vec![65535.0],
            active_area: crate::Rect::new(0, 0, 1, 1),
            crop: crate::Rect::new(0, 0, 1, 1),
            orientation: Default::default(),
            color,
            wb_multipliers: None,
            linearized: false,
            opcodes: Default::default(),
            vendor_lens: None,
            metadata: Default::default(),
        }
    }

    fn close(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < tol)
    }

    #[test]
    fn neutral_maps_to_white_and_primaries_are_recovered() {
        let raw = raw_with(color_single(cam_a(), 21));
        let t = camera_transform(&raw, D65);
        assert!(!t.matrix_is_fallback);
        // a surface with XYZ = Rec.2020 red primary under D65, seen by the camera
        let red_xyz = REC2020.to_xyz().apply([1.0, 0.0, 0.0]);
        let cam = cam_a().apply(red_xyz);
        let wbd = [cam[0] * t.wb[0] as f64, cam[1] * t.wb[1] as f64, cam[2] * t.wb[2] as f64];
        let out = t.matrix.apply(wbd);
        let k = out[0];
        assert!(close([out[0] / k, out[1] / k, out[2] / k], [1.0, 0.0, 0.0], 1e-6), "{out:?}");
        // white
        let n = cam_a().apply(D65.to_xyz());
        let wbd = [n[0] * t.wb[0] as f64, n[1] * t.wb[1] as f64, n[2] * t.wb[2] as f64];
        let out = t.matrix.apply(wbd);
        assert!((out[0] - out[1]).abs() < 1e-6 && (out[1] - out[2]).abs() < 1e-6, "{out:?}");
        assert!(close(t.matrix.apply([1.0; 3]), [1.0; 3], 1e-12));
        assert!(t.wb.iter().cloned().fold(f32::MAX, f32::min) == 1.0);
    }

    #[test]
    fn dual_illuminant_interpolation() {
        let color = ColorData { illuminant: [17, 21], color_matrix: [Some(cam_a()), Some(cam_b())], ..Default::default() };
        assert!((illuminant_weight(&color, cct::temp_tint_to_xy(2856.0, 0.0)) - 1.0).abs() < 0.01);
        assert!(illuminant_weight(&color, cct::temp_tint_to_xy(6504.0, 0.0)) < 0.01);
        assert_eq!(illuminant_weight(&color, cct::temp_tint_to_xy(10000.0, 0.0)), 0.0);
        let mid = cct::temp_tint_to_xy(4000.0, 0.0);
        let g = illuminant_weight(&color, mid);
        let expect = (1.0 / 4000.0 - 1.0 / 6504.0) / (1.0 / 2856.0 - 1.0 / 6504.0);
        assert!((g - expect).abs() < 0.02, "{g} vs {expect}");
        let m = xyz_to_camera(&color, mid);
        let l = Mat3(std::array::from_fn(|i| std::array::from_fn(|j| g * cam_a().0[i][j] + (1.0 - g) * cam_b().0[i][j])));
        assert!(close(m.0[0], l.0[0], 1e-12));
    }

    #[test]
    fn neutral_xy_roundtrip() {
        let color = ColorData {
            illuminant: [17, 21],
            color_matrix: [Some(cam_a()), Some(cam_b())],
            analog_balance: Some([1.0, 1.05, 0.98]),
            ..Default::default()
        };
        for t in [2600.0, 3200.0, 4500.0, 5500.0, 7000.0, 9000.0] {
            for tint in [-10.0, 0.0, 15.0] {
                let xy = cct::temp_tint_to_xy(t, tint);
                let n = camera_neutral(&color, xy);
                let back = neutral_to_xy(&color, n);
                assert!((back.x - xy.x).abs() < 1e-6 && (back.y - xy.y).abs() < 1e-6, "{t} {tint}: {back:?} vs {xy:?}");
            }
        }
        let mut raw = raw_with(color.clone());
        raw.color.as_shot_neutral = Some(camera_neutral(&color, cct::temp_tint_to_xy(3300.0, 5.0)));
        let xy = as_shot_white_xy(&raw);
        let (t, tint) = cct::xy_to_temp_tint(xy);
        assert!((t - 3300.0).abs() < 5.0 && (tint - 5.0).abs() < 0.5, "{t} {tint}");
        raw.color.as_shot_white_xy = Some(D50);
        assert_eq!(as_shot_white_xy(&raw), D50);
    }

    #[test]
    fn forward_matrix_path_agrees_with_color_matrix_path() {
        let a = cam_a();
        let n = a.apply(D65.to_xyz());
        let fm = bradford(D65, D50).mul(&a.inverse().unwrap()).mul(&Mat3::diag(n[0], n[1], n[2]));
        // DNG forward matrices map (1,1,1) to D50 white (with Y=1): normalise like a real file would
        let w = fm.apply([1.0; 3]);
        let s = D50.to_xyz()[1] / w[1];
        let fm = Mat3(fm.0.map(|r| r.map(|v| v * s)));
        let cm_only = camera_transform(&raw_with(color_single(a, 21)), D65);
        let mut with_fm = color_single(a, 21);
        with_fm.forward_matrix = [Some(fm), None];
        let fm_t = camera_transform(&raw_with(with_fm), D65);
        for i in 0..3 {
            assert!(close(cm_only.matrix.0[i], fm_t.matrix.0[i], 1e-9), "{:?} vs {:?}", cm_only.matrix, fm_t.matrix);
        }
        assert_eq!(cm_only.wb, fm_t.wb);
    }

    #[test]
    fn camera_calibration_and_analog_balance_are_respected() {
        // AB · CC scales channels: the neutral must scale accordingly
        let mut c = color_single(cam_a(), 21);
        let base = camera_neutral(&c, D65);
        c.camera_calibration = [Some(Mat3::diag(1.0, 0.5, 1.0)), None];
        let n = camera_neutral(&c, D65);
        assert!((n[1] / n[0] - 0.5 * base[1] / base[0]).abs() < 1e-9);
    }

    #[test]
    fn fallback_without_matrix() {
        let mut raw = raw_with(ColorData::default());
        raw.wb_multipliers = Some([2.0, 1.0, 1.5]);
        let xy = as_shot_white_xy(&raw);
        let t = camera_transform(&raw, xy);
        assert!(t.matrix_is_fallback);
        assert!((t.wb[0] / t.wb[1] - 2.0).abs() < 1e-3 && (t.wb[2] / t.wb[1] - 1.5).abs() < 1e-3, "{:?}", t.wb);
        let t = camera_transform(&raw, D65);
        let expect = SRGB.to_space(&REC2020);
        for i in 0..3 {
            assert!(close(t.matrix.0[i], expect.0[i], 1e-6));
        }
        assert_eq!(camera_to_rec2020(&raw, D65).1, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn illuminant_codes() {
        assert_eq!(illuminant_temperature(21), Some(6504.0));
        assert_eq!(illuminant_temperature(17), Some(2856.0));
        assert_eq!(illuminant_temperature(0), None);
        assert_eq!(illuminant_temperature(255), None);
    }

    #[test]
    fn grey_world_balances() {
        let img = li_raster::RgbFrame::from_fn(20, 20, |x, _| {
            let v = 0.2 + 0.02 * x as f32;
            [v * 0.5, v, v * 0.8]
        });
        let m = grey_world(&img);
        assert!((m[0] - 2.0).abs() < 1e-3 && (m[1] - 1.0).abs() < 1e-6 && (m[2] - 1.25).abs() < 1e-3, "{m:?}");
    }
}
