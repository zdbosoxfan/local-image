//! Sample linear chart patches through the actual raw colour/profile implementation.
//! `profile_chart RAW ROIS.csv PROFILE.dcp [camera|neutral] > patches.csv`
//! ROIs: name,x,y,width,height (fractional coordinates in oriented developed image).
//! Outputs linear XYZ D50; `camera` instead emits sensor RGB in the same three columns.
//! `neutral` sets WB from camera RGB in D05 before evaluating each calibration.
//! No tone curve, look table, clipping, exposure or retouching.
use li_color::{D50, D65, REC2020, bradford};
use lightcraft_raw::{Method, color, dcp::Dcp, decode, profile::ProfileTables};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if !(3..=4).contains(&a.len()) {
        return Err("usage: profile_chart RAW ROIS.csv PROFILE.dcp [camera|neutral]".into());
    }
    let mut raw = decode(&std::fs::read(&a[0])?)?;
    let image = raw.develop(Method::Rcd)?.oriented(raw.orientation);
    let rois = std::fs::read_to_string(&a[1])?;
    let baseline = raw.color.clone();
    let neutral = if a.get(3).is_some_and(|v| v == "neutral") {
        let line = rois.lines().skip(1).find(|line| line.split(',').next() == Some("D05")).ok_or("neutral mode requires D05 ROI")?;
        let fields: Vec<&str> = line.split(',').collect();
        let [x, y, w, h] = roi_rect(&fields, image.width, image.height)?;
        let mut sum = [0.0; 3];
        for yy in y..y + h {
            for xx in x..x + w {
                let p = image.get(xx, yy);
                for c in 0..3 {
                    sum[c] += f64::from(p[c]);
                }
            }
        }
        if sum.iter().any(|v| !v.is_finite() || *v <= 0.0) {
            return Err("D05 must have positive finite camera channels".into());
        }
        Some(sum.map(|v| v / sum[1]))
    } else {
        None
    };
    if a.get(3).is_some_and(|v| v != "camera" && v != "neutral") {
        return Err("optional mode must be camera or neutral".into());
    }
    let mut dcp = Dcp::parse(&std::fs::read(&a[2])?)?;
    // Isolate the measured calibration; creative looks must not influence accuracy.
    dcp.color.profile.look_table = None;
    dcp.color.profile.tone_curve = None;
    let mut selected = baseline.clone();
    selected.color_matrix = dcp.color.color_matrix;
    selected.illuminant = dcp.color.illuminant;
    selected.forward_matrix = dcp.color.forward_matrix;
    selected.illuminant_3 = dcp.color.illuminant_3;
    selected.illuminant_white_xy = dcp.color.illuminant_white_xy;
    selected.color_matrix_3 = dcp.color.color_matrix_3;
    selected.forward_matrix_3 = dcp.color.forward_matrix_3;
    selected.profile_calibration_signature = dcp.color.profile_calibration_signature;
    selected.profile = dcp.color.profile;
    // Preserve the file's AnalogBalance, CameraCalibration/signature and as-shot WB.
    // Exposure is intentionally omitted from this accuracy measurement.
    println!("method,patch,X,Y,Z");
    let modes = if a.get(3).is_some_and(|v| v == "camera") { vec![("camera", baseline)] } else { vec![("matrix", baseline), ("dcp", selected)] };
    for (method, calibration) in modes {
        raw.color = calibration;
        if let Some(neutral) = neutral {
            raw.color.as_shot_white_xy = None;
            raw.color.as_shot_neutral = Some(neutral);
        }
        let xy = color::as_shot_white_xy(&raw);
        let transform = color::camera_transform(&raw, xy);
        if neutral.is_some() {
            eprintln!("{method}: D05 neutral {neutral:?}, WB {:?}, white xy {xy:?}", transform.wb);
        }
        let tables = ProfileTables::new_weighted(&raw.color.profile, color::illuminant_weights(&raw.color, xy));
        let to_xyz = bradford(D65, D50).mul(&REC2020.to_xyz());
        for line in rois.lines().skip(1).filter(|l| !l.trim().is_empty()) {
            let fields: Vec<&str> = line.split(',').collect();
            let [x, y, w, h] = roi_rect(&fields, image.width, image.height)?;
            let mut sum = [0.0f64; 3];
            for yy in y..y + h {
                for xx in x..x + w {
                    let p = image.get(xx, yy);
                    let p = transform.matrix.apply_f32(std::array::from_fn(|c| p[c] * transform.wb[c]));
                    let p = tables.as_ref().map_or(p, |t| t.apply(p, 1.0));
                    let xyz = if method == "camera" { image.get(xx, yy).map(f64::from) } else { to_xyz.apply(p.map(f64::from)) };
                    for c in 0..3 {
                        sum[c] += xyz[c];
                    }
                }
            }
            let q = sum.map(|v| v / (w * h) as f64);
            println!("{method},{},{:.9},{:.9},{:.9}", fields[0], q[0], q[1], q[2]);
        }
    }
    Ok(())
}

fn roi_rect(fields: &[&str], width: usize, height: usize) -> Result<[usize; 4], Box<dyn std::error::Error>> {
    if fields.len() != 5 {
        return Err("ROI row must have five columns".into());
    }
    let mut r = [0usize; 4];
    for (i, size) in [width, height, width, height].into_iter().enumerate() {
        let f = fields[i + 1].parse::<f64>()?;
        if !f.is_finite() || !(0.0..=1.0).contains(&f) {
            return Err("ROI coordinates must be fractions in [0,1]".into());
        }
        r[i] = (f * size as f64).round() as usize;
    }
    let [x, y, w, h] = r;
    if w == 0 || h == 0 || x + w > width || y + h > height {
        return Err("empty or out-of-bounds ROI".into());
    }
    Ok(r)
}
