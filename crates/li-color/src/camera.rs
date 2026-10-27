//! DNG chapter 6 camera model for re-evaluating calibration at each chosen white.
//! Matches lc-raw's DNG model; inverse-CCT interpolation, ForwardMatrix, AB and CC.
use crate::{D50, D65, Mat3, REC2020, SRGB, Xy, bradford, cct};
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CameraColor {
    pub illuminant: [u16; 2],
    /// DNG 1.6 third calibration illuminant; additive for old catalogs.
    #[serde(default)]
    pub illuminant_3: u16,
    /// Custom `IlluminantData1/2/3` whites (used only for code 255).
    #[serde(default)]
    pub illuminant_white_xy: [Option<Xy>; 3],
    #[serde(default)]
    pub color_matrix_3: Option<Mat3>,
    #[serde(default)]
    pub forward_matrix_3: Option<Mat3>,
    #[serde(default)]
    pub camera_calibration_3: Option<Mat3>,
    pub color_matrix: [Option<Mat3>; 2],
    pub forward_matrix: [Option<Mat3>; 2],
    pub camera_calibration: [Option<Mat3>; 2],
    pub analog_balance: Option<[f64; 3]>,
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

// Robertson 1968 isotherms, Wyszecki & Stiles, Color Science (2nd ed.), p.228.
// DNG uses this coordinate system for triple weights, independently of the UI's Kim locus.
const ROBERTSON_ISOTHERMS: [[f64; 4]; 31] = [
    [0.0, 0.18006, 0.26352, -0.24341],
    [10.0, 0.18066, 0.26589, -0.25479],
    [20.0, 0.18133, 0.26846, -0.26876],
    [30.0, 0.18208, 0.27119, -0.28539],
    [40.0, 0.18293, 0.27407, -0.30470],
    [50.0, 0.18388, 0.27709, -0.32675],
    [60.0, 0.18494, 0.28021, -0.35156],
    [70.0, 0.18611, 0.28342, -0.37915],
    [80.0, 0.18740, 0.28668, -0.40955],
    [90.0, 0.18880, 0.28997, -0.44278],
    [100.0, 0.19032, 0.29326, -0.47888],
    [125.0, 0.19462, 0.30141, -0.58204],
    [150.0, 0.19962, 0.30921, -0.70471],
    [175.0, 0.20525, 0.31647, -0.84901],
    [200.0, 0.21142, 0.32312, -1.0182],
    [225.0, 0.21807, 0.32909, -1.2168],
    [250.0, 0.22511, 0.33439, -1.4512],
    [275.0, 0.23247, 0.33904, -1.7298],
    [300.0, 0.24010, 0.34308, -2.0637],
    [325.0, 0.24702, 0.34655, -2.4681],
    [350.0, 0.25591, 0.34951, -2.9641],
    [375.0, 0.26400, 0.35200, -3.5814],
    [400.0, 0.27218, 0.35407, -4.3633],
    [425.0, 0.28039, 0.35577, -5.3762],
    [450.0, 0.28863, 0.35714, -6.7262],
    [475.0, 0.29685, 0.35823, -8.5955],
    [500.0, 0.30505, 0.35907, -11.324],
    [525.0, 0.31320, 0.35968, -15.628],
    [550.0, 0.32129, 0.36011, -23.325],
    [575.0, 0.32931, 0.36038, -40.770],
    [600.0, 0.33724, 0.36051, -116.45],
];

fn dng_temp_tint(xy: Xy) -> (f64, f64) {
    let (u, v) = cct::xy_to_uv(xy);
    let mut previous_distance = 0.0;
    let mut previous_direction = [0.0; 2];
    for i in 1..ROBERTSON_ISOTHERMS.len() {
        let [mired, lu, lv, slope] = ROBERTSON_ISOTHERMS[i];
        let length = (1.0 + slope * slope).sqrt();
        let direction = [1.0 / length, slope / length];
        let distance = -(u - lu) * direction[1] + (v - lv) * direction[0];
        if distance <= 0.0 || i == 30 {
            let below = -distance.min(0.0);
            let fraction = if i == 1 { 0.0 } else { below / (previous_distance + below) };
            let [pm, pu, pv, _] = ROBERTSON_ISOTHERMS[i - 1];
            let temp = 1e6 / (fraction * pm + (1.0 - fraction) * mired);
            let delta = [u - (fraction * pu + (1.0 - fraction) * lu), v - (fraction * pv + (1.0 - fraction) * lv)];
            let d = std::array::from_fn::<_, 2, _>(|j| fraction * previous_direction[j] + (1.0 - fraction) * direction[j]);
            let norm = d[0].hypot(d[1]);
            return (temp, -3000.0 * (delta[0] * d[0] + delta[1] * d[1]) / norm);
        }
        previous_distance = distance;
        previous_direction = direction;
    }
    (5000.0, 0.0)
}

fn dng_temperature_xy(temp: f64) -> Xy {
    let mired = 1e6 / temp;
    let i = ROBERTSON_ISOTHERMS.partition_point(|row| row[0] <= mired).clamp(1, 30);
    let [am, au, av, _] = ROBERTSON_ISOTHERMS[i - 1];
    let [bm, bu, bv, _] = ROBERTSON_ISOTHERMS[i];
    let g = (bm - mired) / (bm - am);
    cct::uv_to_xy(g * au + (1.0 - g) * bu, g * av + (1.0 - g) * bv)
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

/// Calibration chromaticity, including non-Planckian fluorescent and custom whites.
/// Values follow Adobe DNG SDK dng_illuminant_data's standard illuminants.
pub fn illuminant_xy(code: u16, custom: Option<Xy>) -> Option<Xy> {
    Some(match code {
        3 | 17 => Xy::new(0.4476, 0.4074),
        18 => Xy::new(0.348483, 0.351747),
        19 => Xy::new(0.310061, 0.316150),
        23 => D50,
        1 | 4 | 9 | 20 => Xy::new(0.3324, 0.3474),
        10 | 21 => D65,
        11 | 22 => Xy::new(0.2990, 0.3149),
        12 => Xy::new(0.31310, 0.33727),
        13 => Xy::new(0.34588, 0.35875),
        14 => Xy::new(0.37417, 0.37281),
        2 => Xy::new(0.37208, 0.37529),
        15 => Xy::new(0.40910, 0.39430),
        16 => Xy::new(0.44018, 0.40329),
        24 => dng_temperature_xy(3200.0),
        255 => custom.filter(|xy| valid_white(*xy))?,
        _ => return None,
    })
}

fn valid_white(xy: Xy) -> bool {
    xy.x.is_finite() && xy.y.is_finite() && xy.x > 0.0 && xy.y > 0.0 && xy.x + xy.y < 1.0
}

/// Distinct, usable calibration whites required by DNG 1.6's triple model.
pub fn triple_illuminant_whites(color: &CameraColor) -> Option<[Xy; 3]> {
    if color.color_matrix.iter().any(Option::is_none) || color.color_matrix_3.is_none() {
        return None;
    }
    let whites = [
        illuminant_xy(color.illuminant[0], color.illuminant_white_xy[0])?,
        illuminant_xy(color.illuminant[1], color.illuminant_white_xy[1])?,
        illuminant_xy(color.illuminant_3, color.illuminant_white_xy[2])?,
    ];
    (whites[0] != whites[1] && whites[0] != whites[2] && whites[1] != whites[2]).then_some(whites)
}

/// Weight of calibration 1 for a dual profile. Custom IlluminantData whites determine CCT.
pub fn illuminant_weight(color: &CameraColor, white: Xy) -> f64 {
    let t = cct::xy_to_temp_tint(white).0;
    let temperature = |i| {
        if color.illuminant[i] == 255 {
            illuminant_xy(255, color.illuminant_white_xy[i]).map(|xy| dng_temp_tint(xy).0)
        } else {
            illuminant_temperature(color.illuminant[i])
        }
    };
    let (t1, t2) = (temperature(0).unwrap_or(2856.0), temperature(1).unwrap_or(6504.0));
    if (t1 - t2).abs() < 1.0 {
        return 1.0;
    }
    ((1.0 / t - 1.0 / t2) / (1.0 / t1 - 1.0 / t2)).clamp(0.0, 1.0)
}

/// DNG 1.6 weights in scaled inverse-temperature/tint space. Dual profiles retain inverse-CCT interpolation.
pub fn illuminant_weights(color: &CameraColor, white: Xy) -> [f64; 3] {
    let Some(whites) = triple_illuminant_whites(color).filter(|_| valid_white(white)) else {
        let g = illuminant_weight(color, white);
        return [g, 1.0 - g, 0.0];
    };
    triple_weights(white, whites)
}

fn triple_weights(white: Xy, whites: [Xy; 3]) -> [f64; 3] {
    let point = |xy| {
        let (temp, tint) = dng_temp_tint(xy);
        [(1500.0 / temp).min(1.0), tint / 200.0]
    };
    let p = point(white);
    let mut weights = whites.map(|xy| {
        let q = point(xy);
        1.0 / ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + 1e-8)
    });
    let sum: f64 = weights.iter().sum();
    weights = weights.map(|w| {
        let w = w / sum;
        ((w * w * (3.0 - 2.0 * w) - 0.02) / 0.98).clamp(0.0, 1.0)
    });
    let sum: f64 = weights.iter().sum();
    [weights[0] / sum, weights[1] / sum, (1.0 - weights[0] / sum - weights[1] / sum).max(0.0)]
}

fn interp(pair: &[Option<Mat3>; 2], third: Option<Mat3>, weights: [f64; 3]) -> Option<Mat3> {
    if let [Some(a), Some(b), Some(c)] = [pair[0], pair[1], third] {
        return Some(Mat3(std::array::from_fn(|i| {
            std::array::from_fn(|j| weights[0] * a.0[i][j] + weights[1] * b.0[i][j] + weights[2] * c.0[i][j])
        })));
    }
    match pair {
        [Some(a), Some(b)] => Some(lerp(a, b, weights[0])),
        [Some(a), None] | [None, Some(a)] => Some(*a),
        [None, None] => None,
    }
}

/// `AnalogBalance · CameraCalibration` for the selected calibration weights.
fn ab_cc(color: &CameraColor, weights: [f64; 3]) -> Mat3 {
    let ab = color.analog_balance.map(|a| Mat3::diag(a[0], a[1], a[2])).unwrap_or(Mat3::IDENTITY);
    let cc = if triple_illuminant_whites(color).is_some() {
        interp(
            &color.camera_calibration.map(|m| Some(m.unwrap_or(Mat3::IDENTITY))),
            Some(color.camera_calibration_3.unwrap_or(Mat3::IDENTITY)),
            weights,
        )
    } else {
        interp(&color.camera_calibration, None, weights)
    }
    .unwrap_or(Mat3::IDENTITY);
    ab.mul(&cc)
}

/// Whether the file carries a usable colour matrix.
pub fn has_matrix(color: &CameraColor) -> bool {
    color.color_matrix.iter().any(Option::is_some)
}

/// `XYZtoCamera` for white `xy` (fallback: XYZ → linear sRGB).
pub fn xyz_to_camera(color: &CameraColor, white: Xy) -> Mat3 {
    let weights = illuminant_weights(color, white);
    if triple_illuminant_whites(color).is_some() {
        // The SDK interpolates each calibrated XYZ→camera transform, avoiding cross terms
        // from multiplying independently interpolated ColorMatrix and CameraCalibration.
        let matrices = [color.color_matrix[0], color.color_matrix[1], color.color_matrix_3];
        let calibration = [color.camera_calibration[0], color.camera_calibration[1], color.camera_calibration_3];
        let calibrated: [Option<Mat3>; 3] = std::array::from_fn(|i| matrices[i].map(|cm| calibration[i].unwrap_or(Mat3::IDENTITY).mul(&cm)));
        let cm = interp(&[calibrated[0], calibrated[1]], calibrated[2], weights).unwrap_or(Mat3::IDENTITY);
        let ab = color.analog_balance.map(|a| Mat3::diag(a[0], a[1], a[2])).unwrap_or(Mat3::IDENTITY);
        return ab.mul(&cm);
    }
    match interp(&color.color_matrix, None, weights) {
        Some(cm) => ab_cc(color, weights).mul(&cm),
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
    let weights = illuminant_weights(color, white);
    let triple = triple_illuminant_whites(color).is_some();
    let valid_forward = !triple || (color.forward_matrix.iter().all(Option::is_some) && color.forward_matrix_3.is_some());
    if has_matrix(color)
        && valid_forward
        && let Some(fm) = interp(&color.forward_matrix, color.forward_matrix_3, weights)
    {
        let abcc = ab_cc(color, weights);
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
    let total = REC2020.from_xyz().mul(&bradford(D50, D65)).mul(&camera_to_xyz_d50(color, xy));
    let white = total.apply(wb.map(|v| 1.0 / v));
    let k = (white[0] + white[1] + white[2]) / 3.0;
    if k.abs() > 1e-12 { Mat3(total.0.map(|r| r.map(|v| v / k))) } else { total }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CameraWhite {
    pub color: CameraColor,
    /// Undo the exact decoder matrix, WB and optional file-local correction.
    pub undo: Mat3,
}
impl CameraWhite {
    pub fn change(&self, xy: Xy) -> Mat3 {
        to_working(&self.color, xy).mul(&self.undo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn color() -> CameraColor {
        CameraColor {
            illuminant: [17, 21],
            color_matrix: [
                Some(Mat3([[0.9, 0.2, -0.15], [-0.3, 1.25, 0.08], [0.02, -0.12, 0.85]])),
                Some(Mat3([[0.7, 0.3, -0.1], [-0.35, 1.3, 0.1], [0.05, -0.2, 1.]])),
            ],
            ..Default::default()
        }
    }
    fn triple_color() -> CameraColor {
        CameraColor {
            illuminant: [17, 21],
            illuminant_3: 255,
            illuminant_white_xy: [None, None, Some(Xy::new(0.3457, 0.39))],
            color_matrix: [Some(Mat3::diag(1.0, 2.0, 3.0)), Some(Mat3::diag(2.0, 3.0, 4.0))],
            color_matrix_3: Some(Mat3::diag(4.0, 5.0, 6.0)),
            ..Default::default()
        }
    }

    #[test]
    fn triple_illuminant_weights_match_adobe_sdk_reference() {
        // Original SDK dng_temperature + CalculateTripleIlluminantWeights compiled externally
        // under target/refvec/dng-triple, pinned Android Adobe SDK de700ad461e35af50b28b861943a0b0753b10929.
        let c = triple_color();
        for (xy, expected) in [
            (Xy::new(0.4476, 0.4074), [1.0, 0.0, 0.0]),
            (D65, [0.0, 1.0, 0.0]),
            (Xy::new(0.3457, 0.39), [0.0, 0.0, 1.0]),
            (Xy::new(0.38, 0.36), [0.6688305452612884, 0.31174871481836863, 0.01942073992034299]),
            (Xy::new(0.30, 0.37), [0.0006142598103962339, 0.05818100873037222, 0.9412047314592316]),
            (Xy::new(0.25, 0.26), [0.0021102494317923972, 0.8694698871854961, 0.1284198633827115]),
        ] {
            let actual = illuminant_weights(&c, xy);
            for i in 0..3 {
                assert!((actual[i] - expected[i]).abs() < 2e-14, "{xy:?}: {actual:?} vs {expected:?}");
            }
            assert!((actual.iter().sum::<f64>() - 1.0).abs() < 1e-14);
        }
    }

    #[test]
    fn triple_illuminant_calibrated_matrices_and_forward_path() {
        let mut c = triple_color();
        c.camera_calibration = [Some(Mat3::diag(2.0, 1.0, 0.5)), Some(Mat3::diag(3.0, 2.0, 1.0))];
        c.camera_calibration_3 = Some(Mat3::diag(4.0, 3.0, 2.0));
        c.analog_balance = Some([1.1, 0.9, 1.2]);
        let xy = Xy::new(0.38, 0.36);
        let w = illuminant_weights(&c, xy);
        let actual = xyz_to_camera(&c, xy);
        let expected = [
            1.1 * (2.0 * w[0] + 6.0 * w[1] + 16.0 * w[2]),
            0.9 * (2.0 * w[0] + 6.0 * w[1] + 15.0 * w[2]),
            1.2 * (1.5 * w[0] + 4.0 * w[1] + 12.0 * w[2]),
        ];
        for (i, value) in expected.into_iter().enumerate() {
            assert!((actual.0[i][i] - value).abs() < 1e-12);
        }
        let [x, _, z] = D50.to_xyz();
        c.forward_matrix = [
            Some(Mat3([[0.8 * x, 0.1 * x, 0.1 * x], [0.2, 0.7, 0.1], [0.1 * z, 0.2 * z, 0.7 * z]])),
            Some(Mat3([[0.7 * x, 0.2 * x, 0.1 * x], [0.1, 0.8, 0.1], [0.2 * z, 0.1 * z, 0.7 * z]])),
        ];
        c.forward_matrix_3 = Some(Mat3([[0.6 * x, 0.3 * x, 0.1 * x], [0.1, 0.7, 0.2], [0.3 * z, 0.2 * z, 0.5 * z]]));
        // Original SDK FindXYZtoCamera_Triple and SetWhiteXY's forward-matrix block,
        // compiled independently in target/refvec/dng-triple/forward_reference.cpp.
        let expected_sdk = Mat3([
            [0.7376292802528215, 0.1689540454013967, 0.18306682359750376],
            [0.16688305452612887, 0.9485408530249334, 0.19353204761596285],
            [0.11143781857186845, 0.18070945728587434, 1.0904103370035687],
        ]);
        let actual = camera_to_xyz_d50(&c, xy);
        for (a, b) in actual.0.iter().flatten().zip(expected_sdk.0.iter().flatten()) {
            assert!((a - b).abs() < 1e-13, "{actual:?} vs {expected_sdk:?}");
        }
        let fm = Mat3::diag(D50.to_xyz()[0], 1.0, D50.to_xyz()[2]);
        c.forward_matrix = [Some(fm), Some(fm)];
        c.forward_matrix_3 = Some(fm);
        let white = Xy::new(0.3457, 0.39);
        let neutral = camera_neutral(&c, white);
        let transformed = camera_to_xyz_d50(&c, white).apply(neutral);
        for i in 0..3 {
            assert!((transformed[i] - D50.to_xyz()[i]).abs() < 1e-12);
        }
        c.illuminant_white_xy[2] = Some(D65);
        assert!(triple_illuminant_whites(&c).is_none());
        assert_eq!(illuminant_weights(&c, white)[2], 0.0);
    }

    #[test]
    fn kim_tint_has_the_dng_sdk_sign() {
        // A white above the Planckian locus (higher v) is a green illuminant: positive tint in
        // both the UI's Kim model and the SDK's Robertson model.
        for t in [2500.0, 3500.0, 5000.0, 6500.0, 9000.0] {
            let (u, v) = cct::xy_to_uv(cct::temp_tint_to_xy(t, 0.0));
            for dv in [-0.008, -0.003, 0.003, 0.008] {
                let xy = cct::uv_to_xy(u, v + dv);
                let (_, kim) = cct::xy_to_temp_tint(xy);
                let (_, sdk) = dng_temp_tint(xy);
                assert!(kim * dv > 0.0 && sdk * dv > 0.0, "{t} K, dv {dv}: Kim {kim} vs SDK {sdk}");
                assert!((kim - sdk).abs() < 0.1 * sdk.abs() + 1.0, "{t} K, dv {dv}: Kim {kim} vs SDK {sdk}");
            }
        }
    }

    #[test]
    fn inverse_cct_matches_rawtherapee_reference() {
        for row in include_str!("../tests/fixtures/illuminants.csv").lines() {
            let v: Vec<f64> = row.split(',').map(|v| v.parse().unwrap()).collect();
            let white = cct::temp_tint_to_xy(v[0], 0.0);
            let g = illuminant_weight(&color(), white);
            assert!((g - v[1]).abs() < 1e-6, "{} K weight {g} vs {}", v[0], v[1]);
            let got = xyz_to_camera(&color(), white);
            for i in 0..3 {
                for j in 0..3 {
                    assert!((got.0[i][j] - v[2 + 3 * i + j]).abs() < 1e-6);
                }
            }
        }
    }
    #[test]
    fn matrix_is_rederived_and_forward_matrix_is_supported() {
        let mut color = color();
        let shot = cct::temp_tint_to_xy(5500., 0.);
        let total = to_working(&color, shot);
        let camera = CameraWhite { color, undo: total.inverse().unwrap() };
        let id = camera.change(shot);
        for i in 0..3 {
            for j in 0..3 {
                assert!((id.0[i][j] - if i == j { 1. } else { 0. }).abs() < 1e-9);
            }
        }
        assert_ne!(camera.change(cct::temp_tint_to_xy(3200., 0.)), camera.change(cct::temp_tint_to_xy(9000., 0.)));
        color.forward_matrix = [Some(Mat3::IDENTITY), Some(Mat3::IDENTITY)];
        for t in [2856., 4500., 6504.] {
            let xy = cct::temp_tint_to_xy(t, 0.);
            let n = camera_neutral(&color, xy);
            let xyz = camera_to_xyz_d50(&color, xy).apply(n);
            assert!((xyz[0] - xyz[1]).abs() < 1e-9 && (xyz[1] - xyz[2]).abs() < 1e-9);
        }
    }
}
