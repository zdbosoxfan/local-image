//! Reproducible photograph quality/rail audit; RAW coverage lives in engine::tests_match_look_raw.
//! CC0 originals stay external; no private catalog data is used. See CODEX-REPORT-matchlook.md.
use lightcraft_color::{REC2020, SRGB, Xy, cct::xy_to_temp_tint, perceptual::oklab_from_2020, transfer::srgb_to_linear};
use lightcraft_develop::{DevelopSettings, WbMode, Wheel};
use lightcraft_pipeline::{
    RenderRequest, SourceInfo,
    match_look::{self, Options},
    render,
};
use lightcraft_raster::{Rgb32f, Rgba8};
use std::{fmt::Write as _, sync::Arc};

const NAMES: [&str; 6] = ["portrait", "street", "branches", "glass", "pet", "product"];

fn load(name: &str) -> Arc<Rgb32f> {
    let img = image::open(
        std::path::Path::new(&std::env::var("LOCAL_IMAGE_MATCHLOOK_JPEGS").expect("set LOCAL_IMAGE_MATCHLOOK_JPEGS to the six CC0 model-eval JPEGs"))
            .join(format!("{name}.jpg")),
    )
    .unwrap()
    .to_rgb8();
    let (w, h) = (img.width() as f32, img.height() as f32);
    let s = 768.0 / w.max(h);
    let img = image::imageops::resize(&img, (w * s) as u32, (h * s) as u32, image::imageops::FilterType::Triangle);
    let m = SRGB.to_space(&REC2020);
    Arc::new(Rgb32f::from_fn(img.width() as usize, img.height() as usize, |x, y| {
        let p = img.get_pixel(x as u32, y as u32).0;
        m.apply_f32([srgb_to_linear(p[0] as f32 / 255.0), srgb_to_linear(p[1] as f32 / 255.0), srgb_to_linear(p[2] as f32 / 255.0)])
    }))
}
/// Simulate a different capture: exposure (EV) and a channel cast (linear gains).
fn perturb(src: &Arc<Rgb32f>, ev: f32, cast: [f32; 3]) -> Arc<Rgb32f> {
    let g = 2f32.powf(ev);
    Arc::new(Rgb32f::from_fn(src.width, src.height, |x, y| {
        let p = src.get(x, y);
        [p[0] * g * cast[0], p[1] * g * cast[1], p[2] * g * cast[2]]
    }))
}
fn custom_wb(d: &mut DevelopSettings, temp: f64, tint: f64) {
    d.wb.mode = WbMode::Custom;
    d.wb.temp = temp;
    d.wb.tint = tint;
}
fn looks() -> Vec<(&'static str, DevelopSettings)> {
    let mut warm = DevelopSettings::default();
    custom_wb(&mut warm, 7600.0, 8.0);
    warm.light.exposure = 0.25;
    warm.light.contrast = 18.0;
    warm.light.highlights = -25.0;
    warm.light.shadows = 15.0;
    warm.color.vibrance = 22.0;
    warm.grading.shadows = Wheel { hue: 35.0, sat: 18.0, lum: 0.0 };
    warm.grading.highlights = Wheel { hue: 50.0, sat: 22.0, lum: 0.0 };
    let mut cool = DevelopSettings::default();
    custom_wb(&mut cool, 5000.0, -6.0);
    cool.light.exposure = -0.45;
    cool.light.contrast = 28.0;
    cool.light.blacks = -18.0;
    cool.light.highlights = -15.0;
    cool.color.saturation = -18.0;
    cool.grading.shadows = Wheel { hue: 220.0, sat: 28.0, lum: 0.0 };
    cool.grading.midtones = Wheel { hue: 210.0, sat: 10.0, lum: 0.0 };
    let mut airy = DevelopSettings::default();
    custom_wb(&mut airy, 6900.0, 2.0);
    airy.light.exposure = 0.8;
    airy.light.contrast = -18.0;
    airy.light.highlights = -35.0;
    airy.light.shadows = 35.0;
    airy.light.whites = 12.0;
    airy.light.blacks = 12.0;
    airy.color.saturation = -12.0;
    airy.grading.highlights = Wheel { hue: 60.0, sat: 8.0, lum: 0.0 };
    vec![("warm", warm), ("cool", cool), ("airy", airy)]
}
#[derive(Clone, Copy, Debug, Default)]
struct Stats {
    l5: f64,
    l50: f64,
    l95: f64,
    a: f64,
    b: f64,
    chroma: f64,
    cct: f64,
}
fn stats(img: &Rgba8) -> Stats {
    let m = SRGB.to_space(&REC2020);
    let mut l = Vec::with_capacity(img.data.len());
    let (mut a, mut b, mut c, mut lin) = (0.0, 0.0, 0.0, [0.0f64; 3]);
    for p in &img.data {
        let rgb = m.apply_f32([srgb_to_linear(p[0] as f32 / 255.0), srgb_to_linear(p[1] as f32 / 255.0), srgb_to_linear(p[2] as f32 / 255.0)]);
        for i in 0..3 {
            lin[i] += rgb[i] as f64;
        }
        let lab = oklab_from_2020(rgb).map(f64::from);
        l.push(lab[0]);
        a += lab[1];
        b += lab[2];
        c += lab[1].hypot(lab[2]);
    }
    let n = l.len() as f64;
    l.sort_by(f64::total_cmp);
    let q = |f: f64| l[((l.len() - 1) as f64 * f) as usize] * 100.0;
    let avg = lin.map(|v| v / n);
    let xyz = REC2020.to_xyz().apply(avg);
    let (cct, _) = xy_to_temp_tint(Xy::from_xyz(xyz));
    Stats { l5: q(0.05), l50: q(0.5), l95: q(0.95), a: a / n * 100.0, b: b / n * 100.0, chroma: c / n * 100.0, cct }
}
/// Distribution distance (Oklab x100): median L, mean a/b.
fn de(x: Stats, r: Stats) -> f64 {
    ((x.l50 - r.l50).powi(2) + (x.a - r.a).powi(2) + (x.b - r.b).powi(2)).sqrt()
}

#[test]
#[ignore = "set LOCAL_IMAGE_MATCHLOOK_JPEGS to the six CC0 JPEGs; real raws use the engine test"]
fn matchlook_quality() {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/matchlook-quality");
    std::fs::create_dir_all(&out).unwrap();
    let images = NAMES.map(load);
    let info = SourceInfo::default();
    let req = RenderRequest::fit(512, 512);
    let perturbations = [(-1.2, [1.25, 1.0, 0.75]), (0.7, [0.8, 1.0, 1.3]), (-3.0, [1.0; 3]), (-0.5, [0.7, 1.0, 1.45])];
    let mut csv = String::from(
        "look,consistent,target,deltaE,before_deltaE,sync_deltaE,dL5,dL50,dL95,dChroma,dCCT,solve_ms,renders,temp,contrast,saturation\n",
    );
    let (mut total, mut before, mut synced, mut n, mut rails) = (0.0, 0.0, 0.0, 0, 0);
    let mut invalid_temperatures = 0;
    for (name, edited) in looks() {
        let refs = [match_look::analyze(&images[0], &info, &edited), match_look::analyze(&images[1], &info, &edited)];
        for consistent in [false, true] {
            let opts = Options { consistent_batch: consistent, ..Default::default() };
            for (index, (ev, cast)) in perturbations.iter().enumerate() {
                let src = perturb(&images[index + 2], *ev, *cast);
                let base = DevelopSettings::default();
                let start = std::time::Instant::now();
                let result = match_look::solve(&src, &info, &base, &refs, opts, &|| false).unwrap();
                let solve_ms = start.elapsed().as_secs_f64() * 1000.0;
                let r0 = stats(&render(&images[0], &info, &edited, &req).image);
                let r1 = stats(&render(&images[1], &info, &edited, &req).image);
                let w = &result.weights;
                let goal = Stats {
                    l5: r0.l5 * w[0] + r1.l5 * w[1],
                    l50: r0.l50 * w[0] + r1.l50 * w[1],
                    l95: r0.l95 * w[0] + r1.l95 * w[1],
                    a: r0.a * w[0] + r1.a * w[1],
                    b: r0.b * w[0] + r1.b * w[1],
                    chroma: r0.chroma * w[0] + r1.chroma * w[1],
                    cct: r0.cct * w[0] + r1.cct * w[1],
                };
                let after = stats(&render(&src, &info, &result.settings, &req).image);
                let original = stats(&render(&src, &info, &base, &req).image);
                let sync = stats(&render(&src, &info, &edited, &req).image);
                total += de(after, goal);
                before += de(original, goal);
                synced += de(sync, goal);
                n += 1;
                rails += usize::from(result.settings.light.contrast.abs() >= 99.0);
                invalid_temperatures += usize::from(result.settings.wb.temp <= 2040.0 || result.settings.wb.temp >= 49000.0);
                writeln!(
                    csv,
                    "{name},{consistent},{},{:.4},{:.4},{:.4},{:.3},{:.3},{:.3},{:.3},{:.1},{solve_ms:.2},{},{:.1},{:.2},{:.2}",
                    NAMES[index + 2],
                    de(after, goal),
                    de(original, goal),
                    de(sync, goal),
                    after.l5 - goal.l5,
                    after.l50 - goal.l50,
                    after.l95 - goal.l95,
                    after.chroma - goal.chroma,
                    after.cct - goal.cct,
                    result.renders,
                    result.settings.wb.temp,
                    result.settings.light.contrast,
                    result.settings.color.saturation
                )
                .unwrap();
            }
        }
    }
    let mean = total / n as f64;
    let summary = format!(
        "samples={n} mean_deltaE={mean:.4} before={:.4} sync={:.4} contrast_rails={rails} temperature_rails={invalid_temperatures}\n",
        before / n as f64,
        synced / n as f64
    );
    std::fs::write(out.join("metrics.csv"), csv).unwrap();
    std::fs::write(out.join("summary.txt"), &summary).unwrap();
    println!("{summary}");
    assert!(mean <= 3.0, "{summary}");
    assert_eq!(rails, 0);
    assert_eq!(invalid_temperatures, 0);
}
