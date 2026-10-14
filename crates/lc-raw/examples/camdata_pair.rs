//! Export camera-space RawNIND crops and their measured noise profile for the paired audit.
use lightcraft_raw::{Method, decode};
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let raw = decode(&std::fs::read(&args[0]).unwrap()).unwrap();
    let rgb = raw.develop(Method::Rcd).unwrap();
    let (w, h) = (512.min(rgb.width), 512.min(rgb.height));
    let crop = rgb.crop((rgb.width - w) / 2, (rgb.height - h) / 2, w, h);
    let mut data = Vec::new();
    data.extend_from_slice(&(w as u32).to_le_bytes());
    data.extend_from_slice(&(h as u32).to_le_bytes());
    for v in crop.data.iter().flatten() {
        data.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(&args[1], data).unwrap();
    let profile = lightcraft_raw::camera_data::noise_profile(
        raw.metadata.make.as_deref().unwrap_or_default(),
        raw.metadata.model.as_deref().unwrap_or_default(),
        raw.metadata.iso.unwrap_or(0),
    );
    println!(
        "{}",
        serde_json::json!({"make":raw.metadata.make.clone(),"model":raw.metadata.model.clone(),"iso":raw.metadata.iso,"black":raw.black,"white":raw.white,"exposure_time":raw.metadata.exposure_time,"f_number":raw.metadata.f_number,"profile":profile,"normalization_ratio":lightcraft_raw::color::normalization_ratios(&raw)})
    );
}
