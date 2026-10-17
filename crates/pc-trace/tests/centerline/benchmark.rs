//! Original line-art fixtures plus externally supplied, licensed scans.
//! Missing external inputs are recorded as missing gates, never successful cases.
use pc_trace::{
    Image, Params, Preset,
    centerline::{CenterlineTrace, trace},
};
use photocraft_doc::{Layer, LayerContent, ShapeLayer};
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn render(out: &CenterlineTrace) -> Result<image::RgbImage> {
    let mut doc = photocraft_doc::Document::with_background(
        "centreline benchmark",
        photocraft_geom::Size::new(out.width, out.height),
        photocraft_doc::ColorMode::Rgb,
        photocraft_doc::SampleType::U8,
        photocraft_doc::Color::WHITE,
    );
    for stroke in &out.strokes {
        // The actual variable-width renderer is separate. Refuse rather
        // than measure a uniform-width approximation as the traced result.
        if stroke.profile.is_some() {
            return Err("benchmark requires package 11 width projection".into());
        }
        let mut shape = ShapeLayer { path: stroke.path.clone(), fill: None, stroke: Some(stroke.stroke.clone()), ..Default::default() };
        shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
        doc.layers.push(Layer::new("line", LayerContent::Shape(shape)));
    }
    let flat = photocraft_compose::flatten(&doc);
    let data = flat.px.iter().flat_map(|p| p[..3].iter().map(|v| (v.clamp(0., 1.) * 255.).round() as u8)).collect();
    image::RgbImage::from_raw(out.width, out.height, data).ok_or_else(|| "invalid benchmark render".into())
}
fn synthetic(size: u32, case: u32) -> image::RgbaImage {
    image::RgbaImage::from_fn(size, size, |x, y| {
        let c = size / 2;
        let r = size / 4;
        let ink = match case {
            0 => y == c && (r..size - r).contains(&x),
            1 => x.abs_diff(c) + y.abs_diff(c) == r,
            _ => (x == r || x == 3 * r) && (r..3 * r).contains(&y),
        };
        image::Rgba(if ink { [0, 0, 0, 255] } else { [255; 4] })
    })
}
fn measure(id: &str, input: &image::RgbaImage, preset: Preset, adaptive: bool) -> Result<serde_json::Value> {
    let p = Params { adaptive, ..Params::for_preset(preset) };
    let start = std::time::Instant::now();
    let out = trace(Image { width: input.width(), height: input.height(), rgba: input.as_raw() }, &p)?;
    let seconds = start.elapsed().as_secs_f64();
    let clean = image::DynamicImage::ImageRgba8(input.clone()).to_rgb8();
    let raster = render(&out)?;
    let scores = photocraft_testkit::vector::fidelity(&clean, &raster)?;
    let ms_ssim = photocraft_testkit::vector::ssim_ms(&clean, &raster)?;
    Ok(json!({"id":id,"preset":preset,"params":p,"nodes":out.nodes,"strokes":out.strokes.len(),"seconds":seconds,"fidelity":scores.fidelity,"ms_ssim":ms_ssim}))
}
#[test]
fn centerline_synthetic_benchmark_beats_blank_canvas() -> Result {
    for case in 0..3 {
        let input = synthetic(64, case);
        for preset in [Preset::LineArt, Preset::Sketch] {
            let score = measure(&format!("line-{case}"), &input, preset, false)?;
            let clean = image::DynamicImage::ImageRgba8(input.clone()).to_rgb8();
            let empty = image::RgbImage::from_pixel(64, 64, image::Rgb([255; 3]));
            let control = photocraft_testkit::vector::ssim_ms(&clean, &empty)?;
            assert!(score["ms_ssim"].as_f64().ok_or("missing score")? > control, "{score}");
        }
    }
    Ok(())
}
#[derive(Deserialize)]
struct Input {
    file: String,
    license: String,
    url: String,
}

#[test]
#[ignore = "full release line-art corpus; records missing scans/projection as incomplete"]
fn centerline_full_line_art_scan_benchmark() -> Result {
    if cfg!(debug_assertions) {
        return Err("run with --release".into());
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let corpus = root.join("tests/corpus");
    let manifest = corpus.join("inputs.json");
    let inputs: Vec<Input> = if manifest.is_file() { serde_json::from_slice(&std::fs::read(manifest)?)? } else { Vec::new() };
    let mut scores = Vec::new();
    let mut missing = Vec::new();
    let mut scan_count = 0;
    for size in [256, 512, 1024] {
        for case in 0..3 {
            for preset in [Preset::LineArt, Preset::Sketch] {
                scores.push(measure(&format!("synthetic-line-{case}-{size}"), &synthetic(size, case), preset, false)?);
            }
        }
    }
    for input in inputs {
        if input.file.is_empty() || input.file.contains(['/', '\\']) || input.file == ".." || input.license.is_empty() || input.url.is_empty() {
            return Err("scan input needs local filename, licence and URL".into());
        }
        if PathBuf::from(&input.file).extension().is_some_and(|x| x == "svg") {
            continue;
        }
        let path = corpus.join(&input.file);
        if !path.is_file() {
            missing.push(json!({"file":input.file,"reason":"licensed input file absent"}));
            continue;
        }
        let image = image::open(path)?.to_rgba8();
        scan_count += 1;
        for preset in [Preset::LineArt, Preset::Sketch] {
            match measure(&input.file, &image, preset, true) {
                Ok(score) => scores.push(score),
                Err(e) => missing.push(json!({"file":input.file,"preset":preset,"reason":e.to_string()})),
            }
        }
    }
    if scan_count < 5 {
        missing.push(json!({"reason":"coordinator has not supplied five licensed scans","supplied":scan_count,"required":5}));
    }
    missing.push(json!({"reason":"B.4 competitor comparison requires package 11 projection and matching centreline reference sets; unestablished"}));
    let report = json!({"complete":missing.is_empty(),"toolchain":"Rust 1.98.1; release; pc-vector/testkit workspace pins","external_scans":scan_count,"scores":scores,"missing":missing});
    let dir = root.join("../../target/trace-bench");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("centerline25-scores.json"), serde_json::to_vec_pretty(&report)?)?;
    eprintln!("centreline benchmark missing gates: {}", report["missing"]);
    assert_eq!(
        report["scores"].as_array().ok_or("scores")?.len(),
        18 + 2 * scan_count - report["missing"].as_array().ok_or("missing")?.iter().filter(|m| m.get("preset").is_some()).count()
    );
    Ok(())
}
