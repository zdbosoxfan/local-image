//! DNG chapter 6 camera model for re-evaluating calibration at each chosen white.
//! Matches lc-raw's DNG model; inverse-CCT interpolation, ForwardMatrix, AB and CC.
use crate::{Mat3, Xy, SRGB, REC2020, D50, D65, cct, bradford};
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CameraColor {
    pub illuminant: [u16;2],
    pub color_matrix: [Option<Mat3>;2],
    pub forward_matrix: [Option<Mat3>;2],
    pub camera_calibration: [Option<Mat3>;2],
    pub analog_balance: Option<[f64;3]>,
}
/// Correlated colour temperature (K) of an Exif `LightSource` / DNG `CalibrationIlluminant` code.
pub fn illuminant_temperature(code: u16) -> Option<f64> {
    Some(match code {
        1 | 4 | 9 => 5500.0,
        2 | 14 => 4150.0,
        3 => 2850.0,
        10 => 6500.0,
        11 => 7500.0,
        12 => 6430.0,
        13 => 5000.0,
        15 => 3450.0,
        16 => 2940.0,
        17 => 2856.0,
        18 => 4874.0,
        19 => 6774.0,
        20 => 5503.0,
        21 => 6504.0,
        22 => 7504.0,
        23 => 5003.0,
        24 => 3200.0,
        _ => return None,
    })
}

fn lerp(a: &Mat3, b: &Mat3, g: f64) -> Mat3 {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = g * a.0[i][j] + (1.0 - g) * b.0[i][j];
        }
    }
    Mat3(r)
}

/// Weight of calibration 1 for white `xy` (1 = use matrix 1 only).
pub fn illuminant_weight(color: &CameraColor, white: Xy) -> f64 {
    let t = cct::xy_to_temp_tint(white).0;
    let (t1, t2) = (illuminant_temperature(color.illuminant[0]).unwrap_or(2856.0), illuminant_temperature(color.illuminant[1]).unwrap_or(6504.0));
    if (t1 - t2).abs() < 1.0 {
        return 1.0;
    }
    let g = (1.0 / t - 1.0 / t2) / (1.0 / t1 - 1.0 / t2);
    g.clamp(0.0, 1.0)
}

fn interp(pair: &[Option<Mat3>; 2], g: f64) -> Option<Mat3> {
    match pair {
        [Some(a), Some(b)] => Some(lerp(a, b, g)),
        [Some(a), None] | [None, Some(a)] => Some(*a),
        [None, None] => None,
    }
}

/// `AnalogBalance · CameraCalibration` for white `xy`.
fn ab_cc(color: &CameraColor, g: f64) -> Mat3 {
    let ab = color.analog_balance.map(|a| Mat3::diag(a[0], a[1], a[2])).unwrap_or(Mat3::IDENTITY);
    let cc = interp(&color.camera_calibration, g).unwrap_or(Mat3::IDENTITY);
    ab.mul(&cc)
}

/// Whether the file carries a usable colour matrix.
pub fn has_matrix(color: &CameraColor) -> bool {
    color.color_matrix.iter().any(|m| m.is_some())
}

/// `XYZtoCamera` for white `xy` (fallback: XYZ → linear sRGB).
pub fn xyz_to_camera(color: &CameraColor, white: Xy) -> Mat3 {
    let g = illuminant_weight(color, white);
    match interp(&color.color_matrix, g) {
        Some(cm) => ab_cc(color, g).mul(&cm),
        None => SRGB.from_xyz(),
    }
}

/// Camera neutral (raw camera values of a neutral surface under white `xy`), normalised to max 1.
pub fn camera_neutral(color: &CameraColor, white: Xy) -> [f64; 3] {
    let n = xyz_to_camera(color, white).apply(white.to_xyz());
    let mx = n.iter().cloned().fold(f64::MIN, f64::max);
    if mx > 0.0 { n.map(|v| v / mx) } else { [1.0; 3] }
}

/// White-balance multipliers for white `xy` (reciprocal camera neutral, minimum 1).
pub fn wb_multipliers(color: &CameraColor, white: Xy) -> [f64; 3] {
    let n = camera_neutral(color, white);
    let m = n.map(|v| if v > 1e-9 { 1.0 / v } else { 1.0 });
    let mn = m.iter().cloned().fold(f64::MAX, f64::min);
    m.map(|v| v / mn)
}

/// Convert a camera neutral to a white xy (DNG spec iteration: xy → XYZtoCamera(xy) → xy until stable).
pub fn neutral_to_xy(color: &CameraColor, neutral: [f64; 3]) -> Xy {
    let mut xy = D50;
    for _ in 0..50 {
        let Some(inv) = xyz_to_camera(color, xy).inverse() else { return xy };
        let xyz = inv.apply(neutral);
        let next = Xy::from_xyz(xyz);
        if !(next.x.is_finite() && next.y.is_finite()) || next.y <= 0.0 {
            return xy;
        }
        let next = Xy::new(next.x.clamp(0.1, 0.7), next.y.clamp(0.1, 0.7));
        let done = (next.x - xy.x).abs() < 1e-9 && (next.y - xy.y).abs() < 1e-9;
        xy = next;
        if done {
            break;
        }
    }
    xy
}

/// Camera (raw, not white balanced) → XYZ relative to D50, for scene white `xy`.
pub fn camera_to_xyz_d50(color: &CameraColor, white: Xy) -> Mat3 {
    let g = illuminant_weight(color, white);
    if has_matrix(color)
        && let Some(fm) = interp(&color.forward_matrix, g)
    {
        let abcc = ab_cc(color, g);
        if let Some(abcc_inv) = abcc.inverse() {
            let n = camera_neutral(color, white);
            let rn = abcc_inv.apply(n);
            if rn.iter().all(|v| *v > 1e-9) {
                return fm.mul(&Mat3::diag(1.0 / rn[0], 1.0 / rn[1], 1.0 / rn[2])).mul(&abcc_inv);
            }
        }
    }
    let cam_to_xyz = xyz_to_camera(color, white).inverse().unwrap_or(Mat3::IDENTITY);
    bradford(white, D50).mul(&cam_to_xyz)
}


/// Raw camera values to Rec.2020, including chosen-white WB. Gain normalization follows
/// the decoder's white-balanced matrix (mean of transformed white).
pub fn to_working(color: &CameraColor, xy: Xy) -> Mat3 {
    let wb = wb_multipliers(color, xy);
    let total = REC2020.from_xyz().mul(&bradford(D50,D65)).mul(&camera_to_xyz_d50(color,xy));
    let white = total.apply(wb.map(|v| 1.0/v));
    let k = (white[0]+white[1]+white[2])/3.0;
    if k.abs()>1e-12 { Mat3(total.0.map(|r| r.map(|v| v/k))) } else { total }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraWhite {
    pub color: CameraColor,
    /// Undo the exact decoder matrix, WB and optional file-local correction.
    pub undo: Mat3,
}
impl CameraWhite {
    pub fn change(&self, xy: Xy) -> Mat3 { to_working(&self.color,xy).mul(&self.undo) }
}
