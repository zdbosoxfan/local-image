//! Compare the real official darktable restore output at the sensor stage, before colour/tone.
use li_raster::RgbFrame;
use lightcraft_engine::enhance::denoise::{camera_rgb_to_working, pack_bayer, unpack_camera};
use lightcraft_engine::files::prepare_raw;
use std::path::Path;
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    let bytes = std::fs::read(args.get(1).ok_or("source raw")?).map_err(|e| e.to_string())?;
    let mut raw = prepare_raw(&bytes, &Default::default())?;
    let source_raw = raw.clone();
    let n = raw.normalized().map_err(|e| e.to_string())?;
    let (_, w, h, phase) = pack_bayer(&n)?;
    let own = std::fs::read(args.get(2).ok_or("Local Image camera RGB float output")?).map_err(|e| e.to_string())?;
    let cfa = n.cfa.as_ref().ok_or("raw has no CFA pattern")?;
    if own.len() < 8 || own.len() - 8 != w.checked_mul(h).and_then(|n| n.checked_mul(4 * 3 * 4)).ok_or("output dimensions overflow")? {
        return Err("invalid Local Image output length".into());
    }
    let read32 = |b: &[u8]| u32::from_le_bytes(b.try_into().unwrap()) as usize;
    if (read32(&own[..4]), read32(&own[4..8])) != (w * 2, h * 2) {
        return Err("Local Image output dimensions differ".into());
    }
    let values: Vec<f32> = own[8..].as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect();
    let mut ours = unpack_camera(values.as_chunks::<3>().0.to_vec(), w * 2, h * 2, n.width, n.height, phase);
    let mut remosaic = n.clone();
    for y in 0..n.height {
        for x in 0..n.width {
            remosaic.data[y * n.width + x] = ours.data[y * n.width + x][cfa.color_at(x, y) as usize];
        }
    }
    let mut remosaic = lightcraft_raw::demosaic(&remosaic, lightcraft_raw::Method::Ahd);
    let dt = std::fs::read(args.get(3).ok_or("darktable CFA output")?).map_err(|e| e.to_string())?;
    if dt.len() < 8 || dt.len() - 8 != raw.width.checked_mul(raw.height).and_then(|n| n.checked_mul(2)).ok_or("sensor dimensions overflow")? {
        return Err("invalid darktable output length".into());
    }
    if n.width <= 256 || n.height <= 256 {
        return Err("sensor is too small for the comparison margin".into());
    }
    let (dw, dh) = (read32(&dt[..4]), read32(&dt[4..8]));
    println!("raw {}x{} active {:?}, darktable {dw}x{dh}", raw.width, raw.height, raw.active_area);
    if (dw, dh) != (raw.width, raw.height) {
        return Err("decoder sensor dimensions differ".into());
    }
    if let Some(path) = args.get(5) {
        let original = std::fs::read(path).map_err(|e| e.to_string())?;
        if original.len() < 8 || !(original.len() - 8).is_multiple_of(2) {
            return Err("invalid original sensor output length".into());
        }
        let samples: Vec<u16> = original[8..].as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect();
        let lightcraft_raw::RawData::U16(input) = &raw.data else { return Err("expected integer sensor input".into()) };
        if samples.len() != input.len() {
            return Err("input sensor lengths differ".into());
        }
        let different = samples.iter().zip(input).filter(|(a, b)| a != b).count();
        let maximum = samples.iter().zip(input).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
        println!(
            "Original sensor decoder agreement: {different}/{} samples differ, maximum ADC difference {maximum}; black {:?} white {:?}",
            input.len(),
            raw.black,
            raw.white
        );
    }
    let adc: Vec<u16> = dt[8..].as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect();
    let mut mse = [0f64; 3];
    let mut count = [0usize; 3];
    let mut means = [[0f64; 3]; 2];
    for y in 128..n.height - 128 {
        for x in 128..n.width - 128 {
            let c = cfa.color_at(x, y) as usize;
            let a = ours.data[y * n.width + x][c];
            let b =
                (adc[(y + raw.active_area.y) * dw + x + raw.active_area.x] as f32 - raw.black.at(x, y, 0, 1)) / (raw.white_at(0) - raw.black.mean());
            mse[c] += (a - b).powi(2) as f64;
            count[c] += 1;
            means[0][c] += a as f64;
            means[1][c] += b as f64;
        }
    }
    println!(
        "CFA agreement PSNR {:?} means Local Image {:?} darktable {:?}",
        std::array::from_fn::<_, 3, _>(|c| -10. * (mse[c] / count[c] as f64).log10()),
        std::array::from_fn::<_, 3, _>(|c| means[0][c] / count[c] as f64),
        std::array::from_fn::<_, 3, _>(|c| means[1][c] / count[c] as f64)
    );
    raw.data = lightcraft_raw::RawData::U16(adc);
    let mut reference = raw.develop(lightcraft_raw::Method::default()).map_err(|e| e.to_string())?;
    camera_rgb_to_working(&source_raw, &bytes, &mut ours);
    camera_rgb_to_working(&source_raw, &bytes, &mut remosaic);
    camera_rgb_to_working(&source_raw, &bytes, &mut reference);
    let crop = raw.crop.clipped(ours.width, ours.height);
    ours = ours.into_crop(crop.x, crop.y, crop.width, crop.height);
    remosaic = remosaic.into_crop(crop.x, crop.y, crop.width, crop.height);
    let (input, _) = lightcraft_engine::files::load_bytes(&bytes, usize::MAX)?;
    let side = 768;
    if [&input, &ours, &remosaic, &reference].iter().any(|img| img.width < side || img.height < side) {
        return Err("image is too small for a 768 px comparison crop".into());
    }
    let mut wavelet = input.clone().into_crop((input.width - side) / 2, (input.height - side) / 2, side, side);
    let mut settings = lightcraft_develop::DevelopSettings::default();
    settings.detail.nr_luminance = 50.;
    settings.detail.nr_color = 25.;
    let params = lightcraft_pipeline::detail::nr::nr_params(&settings, 1.).unwrap();
    lightcraft_pipeline::detail::nr::denoise(&mut wavelet, &params);
    let out = Path::new(args.get(4).ok_or("output folder")?);
    save(&out.join("sample-wavelet.png"), &wavelet)?;
    let (ix, iy) = ((input.width - side) / 2, (input.height - side) / 2);
    save(&out.join("sample-input.png"), &input.into_crop(ix, iy, side, side))?;
    for (name, img) in [("local-image", ours), ("darktable", reference), ("remosaic", remosaic)] {
        let side = 768;
        let x = (img.width - side) / 2;
        let y = (img.height - side) / 2;
        let img = img.into_crop(x, y, side, side);
        save(&out.join(format!("sample-{name}.png")), &img)?;
    }
    Ok(())
}
fn save(path: &Path, img: &RgbFrame) -> Result<(), String> {
    let m = li_color::SRGB.from_xyz().mul(&li_color::REC2020.to_xyz());
    let samples: Vec<u8> =
        img.data.iter().flat_map(|p| m.apply_f32(*p).map(|v| (li_color::transfer::linear_to_srgb(v).clamp(0., 1.) * 255. + 0.5) as u8)).collect();
    let bytes = lightcraft_codecs::encode_png(
        &lightcraft_codecs::EncodeImage::new(img.width as u32, img.height as u32, 3, lightcraft_codecs::Samples::U8(&samples)),
        &Default::default(),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}
