//! Whole-corpus level audit and matched sensor-bin clipped-region hue census.
#[cfg(feature = "testgen")]
fn main() {
    use lightcraft_raw::{RawData, Rect, decode, decode_without_camera_data};
    let args: Vec<_> = std::env::args().skip(1).collect();
    let levels_only = args.iter().any(|a| a == "--levels-only");
    for path in args.iter().filter(|a| !a.starts_with("--")) {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                println!("{}", serde_json::json!({"file":path,"status":"read_error","error":e.to_string()}));
                continue;
            }
        };
        let mut before = match decode_without_camera_data(&bytes) {
            Ok(r) => r,
            Err(e) => {
                println!("{}", serde_json::json!({"file":path,"status":"decoder_error","error":e.to_string()}));
                continue;
            }
        };
        let samples = std::mem::replace(&mut before.data, RawData::U16(vec![]));
        let mut static_calibration = before.clone();
        before.data = samples;
        lightcraft_raw::camera_data::apply_tables_for_audit(&mut static_calibration);
        let static_white = static_calibration.white.clone();
        static_calibration.data = std::mem::replace(&mut before.data, RawData::U16(vec![]));
        let table_plateaus = static_calibration.saturation_points();
        before.data = std::mem::replace(&mut static_calibration.data, RawData::U16(vec![]));
        let mut after = match decode(&bytes) {
            Ok(r) => r,
            Err(e) => {
                println!("{}", serde_json::json!({"file":path,"status":"calibration_error","error":e.to_string()}));
                continue;
            }
        };
        let header = lightcraft_raw::probe_info(&bytes);
        let header_agrees = header.as_ref().is_ok_and(|i| *i == after.info());
        let plateaus = before.saturation_points();
        let calibrated_plateaus = after.saturation_points();
        let changed_samples = match (&before.data, &after.data) {
            (RawData::U16(a), RawData::U16(b)) => a.iter().zip(b).filter(|(a, b)| a != b).count(),
            _ => 0,
        };
        let table_white = if after.cpp == 1 && static_white.len() == 4 {
            [static_white[0], (static_white[1] + static_white[3]) * 0.5, static_white[2]]
        } else {
            [static_white.first().copied().unwrap_or(65535.0); 3]
        };
        let discrepancies: [Option<f32>; 3] =
            std::array::from_fn(|c| table_plateaus[c].or(plateaus[c]).map(|p| (table_white[c] - p as f32).abs() / table_white[c].max(1.0)));
        let mut record = serde_json::json!({"file":path,"status":"ok","make":after.metadata.make,"model":after.metadata.model,"iso":after.metadata.iso,"bits":after.bits,"width":after.width,"height":after.height,"old_black":before.black.mean(),"new_black":after.black.mean(),"old_white":before.white,"new_white":after.white,"table_white":static_white,"table_black":static_calibration.black.mean(),"table_plateaus":table_plateaus,"plateaus":plateaus,"calibrated_plateaus":calibrated_plateaus,"discrepancies":discrepancies,"changed_samples":changed_samples,"old_active":before.active_area,"new_active":after.active_area,"header_agrees":header_agrees,"samples":after.data.len()});
        if !header_agrees {
            record["header_error"] = serde_json::json!(header.err().map(|e| e.to_string()));
        }
        // Only integer CFA raws have sensor-level clipping comparable to these tables.
        if after.cpp != 1 || after.cfa.is_none() || !matches!(after.data, RawData::U16(_)) || !after.can_bin(8) {
            record["hue_status"] = serde_json::json!("not_integer_cfa");
            println!("{record}");
            continue;
        }
        let area = after.active_area;
        let crop = after.crop.clipped(area.width, area.height);
        let roi = Rect::new(area.x + crop.x, area.y + crop.y, crop.width, crop.height);
        let full = Rect::new(0, 0, after.width, after.height);
        let reanchor = |r: &mut lightcraft_raw::RawImage| {
            let a = r.active_area;
            let black = r.black.clone();
            let rows = black.repeat_rows.max(1);
            let cols = black.repeat_cols.max(1);
            r.black.values = (0..rows)
                .flat_map(|y| (0..cols).map(move |x| (x, y)))
                .map(|(x, y)| black.at((x + cols - a.x % cols) % cols, (y + rows - a.y % rows) % rows, 0, 1))
                .collect();
            if !black.delta_h.is_empty() {
                r.black.delta_h = [vec![0.0; a.x], black.delta_h].concat();
            }
            if !black.delta_v.is_empty() {
                r.black.delta_v = [vec![0.0; a.y], black.delta_v].concat();
            }
            r.active_area = full;
            r.crop = full;
        };
        reanchor(&mut before);
        reanchor(&mut after);
        let (w, h) = (after.width / 8, after.height / 8);
        let mut mask = vec![0u8; w * h];
        if let (RawData::U16(d), Some(cfa)) = (&after.data, &after.cfa) {
            for y in roi.y.div_ceil(8) * 8..((roi.y + roi.height) / 8 * 8).min(h * 8) {
                for x in roi.x.div_ceil(8) * 8..((roi.x + roi.width) / 8 * 8).min(w * 8) {
                    if d[y * after.width + x] as f32 >= after.sensor_white(x, y, 0) * 0.99 {
                        mask[(y / 8) * w + x / 8] |= 1 << cfa.color_at(x, y).min(2);
                    }
                }
            }
        }
        record["clipped_bins"] = serde_json::json!(mask.iter().filter(|m| **m != 0).count());
        record["fully_clipped_bins"] = serde_json::json!(mask.iter().filter(|m| **m == 7).count());
        if levels_only {
            println!("{record}");
            continue;
        }
        if !before.opcodes.list3.is_empty() || !after.opcodes.list3.is_empty() {
            record["hue_status"] = serde_json::json!("post_demosaic_opcodes");
            println!("{record}");
            continue;
        }
        let render = |r: &lightcraft_raw::RawImage| {
            let mut img = r.develop_binned(8, 0.99).unwrap().unwrap();
            let t = lightcraft_raw::color::camera_transform(r, lightcraft_raw::color::as_shot_white_xy(r));
            lightcraft_raw::highlight::clip_neutral(&mut img, t.wb, 0.99);
            let m = t.matrix.to_f32();
            img.map_in_place(|p| m.map(|row| (0..3).map(|c| row[c] * p[c] * t.wb[c]).sum()));
            img
        };
        let census = |img: &lightcraft_raw::RgbFrame, full_only: bool| {
            let (mut chroma, mut count, mut bad, mut hist) = (0.0f64, 0usize, 0usize, [0u64; 12]);
            for (p, &selected) in img.data.iter().zip(&mask) {
                if selected != 0 && (!full_only || selected == 7) {
                    let max = p.iter().copied().fold(f32::MIN, f32::max);
                    let min = p.iter().copied().fold(f32::MAX, f32::min);
                    let delta = max - min;
                    let saturation = delta / max.abs().max(1e-6);
                    chroma += saturation as f64;
                    count += 1;
                    if saturation > 0.02 {
                        let hue = if max == p[0] {
                            (p[1] - p[2]) / delta
                        } else if max == p[1] {
                            2.0 + (p[2] - p[0]) / delta
                        } else {
                            4.0 + (p[0] - p[1]) / delta
                        };
                        let hue = hue.rem_euclid(6.0) * 60.0;
                        hist[(hue / 30.0) as usize % 12] += 1;
                        if (280.0..=340.0).contains(&hue) && saturation > 0.1 {
                            bad += 1;
                        }
                    }
                }
            }
            serde_json::json!({"bins":count,"mean_chroma":chroma/count.max(1) as f64,"magenta_cyan_bins":bad,"hues":hist})
        };
        let a = render(&before);
        let b = render(&after);
        record["before_any"] = census(&a, false);
        record["after_any"] = census(&b, false);
        record["before_full"] = census(&a, true);
        record["after_full"] = census(&b, true);
        println!("{record}");
    }
}
#[cfg(not(feature = "testgen"))]
fn main() {
    eprintln!("Run with --features testgen to enable decoder-only calibration baselines.");
}
