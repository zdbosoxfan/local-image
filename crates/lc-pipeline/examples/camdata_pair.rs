//! Filter the raw audit's camera-space crop using measured and image-estimated models.
use li_raster::RgbFrame;
use lightcraft_pipeline::detail::nr::{NoiseCalibration, NrParams, denoise, denoise_profiled};
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let bytes = std::fs::read(&args[0]).unwrap();
    let u32_at = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
    let (w, h) = (u32_at(0) as usize, u32_at(4) as usize);
    let mut img = RgbFrame::new(w, h);
    for (v, b) in img.data.iter_mut().flatten().zip(bytes[8..].as_chunks::<4>().0) {
        *v = f32::from_le_bytes(*b);
    }
    let metadata: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&args[1]).unwrap()).unwrap();
    let a: [f32; 3] = serde_json::from_value(metadata["profile"]["a"].clone()).unwrap();
    let b: [f32; 3] = serde_json::from_value(metadata["profile"]["b"].clone()).unwrap();
    let ratio: [f32; 3] = serde_json::from_value(metadata["normalization_ratio"].clone()).unwrap_or([1.0; 3]);
    let (a, b) = (std::array::from_fn(|i| a[i] * ratio[i]), std::array::from_fn(|i| b[i] * ratio[i]));
    let calibration = NoiseCalibration { a, b, camera_to_working: [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]] };
    let params = NrParams { lum: 0.6, col: 0.6, detail: 0.5, contrast: 0.5, col_detail: 0.5, smoothness: 0.5, scale: 1.0 };
    for mode in ["profiled", "estimated"] {
        let mut out = img.clone();
        if mode == "profiled" {
            denoise_profiled(&mut out, &params, &calibration);
        } else {
            denoise(&mut out, &params);
        }
        let mut data = Vec::new();
        data.extend_from_slice(&(w as u32).to_le_bytes());
        data.extend_from_slice(&(h as u32).to_le_bytes());
        for v in out.data.iter().flatten() {
            data.extend_from_slice(&v.to_le_bytes());
        }
        std::fs::write(format!("{}-{mode}.bin", args[2]), data).unwrap();
    }
}
