//! Owner review: `cargo +1.98.1 run --offline -p lightcraft-engine --example compare_highlights`.
//! Reads the named Sony raws in place; writes only JPEG crops and a self-contained HTML
//! under ignored target/compare-highlights. Optional LC_COMPARE_SYNTHETIC_ONLY skips real raws.
use lightcraft_codecs::encode::{ChromaSubsampling, EncodeImage, EncodeMeta, encode_jpeg};
use lightcraft_develop::DevelopSettings;
use lightcraft_pipeline::{
    RenderRequest, SourceInfo, metrics,
    primary::{self, HsMethod},
};
use lightcraft_raster::{
    Rgb32f, Rgba8,
    resample::{Filter, resize},
};
use std::{error::Error, fmt::Write as _, path::Path};
fn base64(bytes: &[u8]) -> String {
    let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let v = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(table[(v >> 18) as usize] as char);
        out.push(table[((v >> 12) & 63) as usize] as char);
        out.push(if c.len() > 1 { table[((v >> 6) & 63) as usize] as char } else { '=' });
        out.push(if c.len() > 2 { table[(v & 63) as usize] as char } else { '=' });
    }
    out
}
fn linear(img: &Rgba8) -> Rgb32f {
    let mat = lightcraft_color::SRGB.to_space(&lightcraft_color::REC2020);
    img.map(|p| mat.apply_f32([p[0], p[1], p[2]].map(|v| lightcraft_color::transfer::srgb_to_linear(v as f32 / 255.))))
}
fn synthetic(kind: usize, w: usize, h: usize) -> Rgb32f {
    Rgb32f::from_fn(w, h, |x, y| {
        // Middle strip: ideal step for halo measurement, free of colour/texture.
        if kind == 0 && ((x as f32 - w as f32 * 0.86).powi(2) + (y as f32 - h as f32 * 0.14).powi(2)).sqrt() < h as f32 * 0.055 {
            return [8.; 3];
        }
        if (h / 3..h * 2 / 3).contains(&y) {
            return [if x < w / 2 { 0.018 } else { 3.5 }; 3];
        }
        if kind == 0 {
            let window = x > w * 2 / 3 && y < h / 3;
            let v = if window { 4. + x as f32 / w as f32 * 4. } else { 0.012 + 0.045 * y as f32 / h as f32 };
            [v * 0.93, v, v * 1.1]
        } else if kind == 1 {
            let v = if x < w / 2 { 0.012 } else { 5. };
            let detail = 1. + 0.04 * (x as f32 * 0.9).sin() * (y as f32 * 0.7).cos();
            [v * detail; 3]
        } else {
            let colours = [[0.45, 0.22, 0.12], [0.65, 0.38, 0.24], [5., 0.5, 0.15], [0.3, 2., 4.], [4., 1., 0.3], [0.06, 0.02, 0.01]];
            colours[(x * 6 / w).min(5)]
        }
    })
}
fn metrics_scene(src: &Rgb32f, info: &SourceInfo, s: &DevelopSettings, edge: usize, method: HsMethod) -> Rgb32f {
    let plan = lightcraft_pipeline::plan(src, info, s, &RenderRequest::fit(edge, edge));
    let mut lin = plan.frame.sample(src, plan.w, plan.h);
    lightcraft_pipeline::lin_cpu(&mut lin, info, &plan);
    let proxy = primary::proxy_for(src, info, &plan);
    primary::process(&lin, &proxy, info, &plan, method)
}
fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = std::env::var("LC_COMPARE_OUTPUT").unwrap_or_else(|_| "target/compare-highlights".into());
    let dir = Path::new(&out_dir);
    std::fs::create_dir_all(dir)?;
    let mut html = String::from(
        "<!doctype html><html lang=en><meta charset=utf-8><title>Local Image — primary tone A/B</title><style>body{font:15px system-ui;background:#17191c;color:#eee;margin:32px}h1{font-size:24px}details{border-top:1px solid #555;padding:12px}summary{cursor:pointer;font-size:18px}table{border-collapse:collapse;font-variant-numeric:tabular-nums}td,th{padding:8px;border:1px solid #555}figure{display:inline-block;width:46%;margin:8px}img{width:100%}small{color:#bbb}.pass{color:#8ed69c}.fail{color:#ffb286}</style><h1>Local Image Highlights / Shadows — A/B review</h1><p>A: LI Tone, vkdt local Laplacian on log EV, 10 remap samples. B: multi-scale faithful darktable EIGF. Both use a fixed 2 MP proxy and guided gain upsampling; full sensor clipping confidence protects white speculars. Default remains A until the owner chooses.</p><p>Metrics are measured before tone mapping for halos, order and hue; display CIELab D65 ΔE2000 for colour/skin and preview/export. Δh isolates chromaticity by normalizing luminance. LOE is reversed-pair fraction on 256 deterministic samples. Halo widths/energy apply to the synthetic neutral middle strip; real portraits have no ideal step, so their halo columns are unavailable.</p>",
    );
    let mut scenes: Vec<(String, Rgb32f, SourceInfo, bool)> = (0..3)
        .map(|i| {
            (
                format!("Synthetic {}", ["HDR window / interior", "sharp edge / detail", "saturated highlights / skin patches"][i]),
                synthetic(i, 768, 512),
                SourceInfo {
                    raw: true,
                    raw_clip_level: Some(0.99),
                    clip_confidence: Some(std::sync::Arc::new(lightcraft_pipeline::ClipConfidence {
                        width: 768,
                        height: 512,
                        data: (0..768 * 512)
                            .map(|k| {
                                let (x, y) = ((k % 768) as f32, (k / 768) as f32);
                                if i == 0 && ((x - 768. * 0.86).powi(2) + (y - 512. * 0.14).powi(2)).sqrt() < 512. * 0.055 { 1. } else { 0. }
                            })
                            .collect(),
                    })),
                    ..Default::default()
                },
                true,
            )
        })
        .collect();
    if std::env::var_os("LC_COMPARE_SYNTHETIC_ONLY").is_none() {
        for stem in ["_DSC5041", "_DSC4601", "_DSC5420"] {
            if std::env::var("LC_COMPARE_SCENE").is_ok_and(|v| v != stem) {
                continue;
            }
            let path = Path::new("/home/zdavidson/Desktop/10-7-26 - SNHU Headshots").join(format!("{stem}.ARW"));
            let bytes = std::fs::read(&path)?;
            let (img, info) = lightcraft_engine::files::load_bytes(&bytes, 4096)?;
            scenes.push((stem.into(), img, info, false));
        }
    }
    if let Ok(name) = std::env::var("LC_COMPARE_SCENE") {
        scenes.retain(|(n, ..)| n == &name);
    }
    let mut csv = String::from(
        "scene,slider,value,candidate,halo_over_ev,halo_under_ev,halo_width_px,halo_energy_ev_px,loe,hue_p95_deg,delta_e_p95,preview_export_p95\n",
    );
    let mut failed_limits = 0;
    for (idx, (name, src, info, synth)) in scenes.iter().enumerate() {
        let export_edge = if *synth { 768 } else { 2048 };
        let preview_edge = if *synth { 384 } else { 512 };
        let req = RenderRequest::fit(export_edge, export_edge);
        let neutral = metrics_scene(src, info, &DevelopSettings::default(), export_edge, HsMethod::LiTone);
        let before = linear(&lightcraft_pipeline::render(src, info, &DevelopSettings::default(), &req).image);
        writeln!(html, "<details open><summary>{name}</summary>")?;
        for slider in ["Highlights", "Shadows"] {
            for value in [-100., -50., 50., 100.] {
                if std::env::var_os("LC_COMPARE_DIAGNOSTIC").is_some() && (slider != "Highlights" || value != -100.) {
                    continue;
                }
                writeln!(
                    html,
                    "<h3>{slider} {value:+}</h3><table><tr><th>Candidate<th>Halo +EV<th>Halo −EV<th>10% width (px)<th>Energy (EV·px)<th>LOE<th>Δh p95 (°), &lt;2<th>Colour/skin ΔE00 p95<th>Preview/export ΔE00 p95, &lt;1"
                )?;
                let mut images = Vec::new();
                for method in [HsMethod::LiTone, HsMethod::Eigf] {
                    let (label, key) = if method == HsMethod::LiTone { ("A — LI Tone", "A") } else { ("B — EIGF", "B") };
                    let mut s = DevelopSettings::default();
                    if slider == "Highlights" {
                        s.light.highlights = value;
                    } else {
                        s.light.shadows = value;
                    }
                    let export = lightcraft_pipeline::render_hs_candidate(src, info, &s, &req, method).image;
                    let preview =
                        lightcraft_pipeline::render_hs_candidate(src, info, &s, &RenderRequest::fit(preview_edge, preview_edge), method).image;
                    let a = linear(&export);
                    let down = resize(&a, preview.width, preview.height, Filter::Mitchell);
                    let b = linear(&preview);
                    let mut delta: Vec<_> =
                        down.data.iter().zip(&b.data).map(|(a, b)| metrics::delta_e(metrics::lab(*a), metrics::lab(*b))).collect();
                    let preview_de = metrics::p95(&mut delta);
                    let scene = metrics_scene(src, info, &s, export_edge, method);
                    let mut hue: Vec<_> = scene.data.iter().zip(&neutral.data).map(|(a, b)| metrics::hue_shift(*b, *a)).collect();
                    let hue = metrics::p95(&mut hue);
                    if !(hue < 2. && preview_de < 1.) {
                        failed_limits += 1;
                    }
                    let mut de: Vec<_> = a.data.iter().zip(&before.data).map(|(a, b)| metrics::delta_e(metrics::lab(*a), metrics::lab(*b))).collect();
                    let de = metrics::p95(&mut de);
                    // Explicit skin/colour patch ROIs, rather than folding them into a frame percentile.
                    let patch_de = if *synth && idx == 2 {
                        Some(
                            (0..6)
                                .map(|patch| {
                                    let x = (patch * 2 + 1) * a.width / 12;
                                    let y = a.height / 6;
                                    let mean = |im: &Rgb32f| {
                                        let mut v = [0f32; 3];
                                        for yy in y.saturating_sub(8)..y + 8 {
                                            for xx in x.saturating_sub(8)..x + 8 {
                                                let c = im.get(xx, yy);
                                                for k in 0..3 {
                                                    v[k] += c[k] / 256.;
                                                }
                                            }
                                        }
                                        v
                                    };
                                    metrics::delta_e(metrics::lab(mean(&a)), metrics::lab(mean(&before)))
                                })
                                .collect::<Vec<_>>(),
                        )
                    } else {
                        None
                    };
                    let loe = metrics::lightness_order(&neutral, &scene);
                    let halo = if *synth {
                        Some(metrics::halo(&neutral.map(primary::log_light), &scene.map(primary::log_light), scene.height / 2, scene.width / 2))
                    } else {
                        None
                    };
                    let hv = halo.map_or([f64::NAN; 4], |h| [h.overshoot, h.undershoot, h.width, h.energy]);
                    let format = |v: f64| if v.is_nan() { "n/a".into() } else { format!("{v:.4}") };
                    writeln!(
                        html,
                        "<tr><td>{label}<td>{}<td>{}<td>{}<td>{}<td>{loe:.5}<td class={}>{hue:.4}<td>{de:.4}<td class={}>{preview_de:.4}",
                        format(hv[0]),
                        format(hv[1]),
                        format(hv[2]),
                        format(hv[3]),
                        if hue < 2. { "pass" } else { "fail" },
                        if preview_de < 1. { "pass" } else { "fail" }
                    )?;
                    if let Some(patches) = patch_de {
                        writeln!(
                            html,
                            "<tr><td colspan=9><small>{label} patch ΔE00 (skin light / skin warm / red / blue / orange / dark skin): {}</small>",
                            patches.iter().map(|v| format!("{v:.3}")).collect::<Vec<_>>().join(" / ")
                        )?;
                    }
                    writeln!(csv, "{name},{slider},{value},{key},{},{},{},{},{loe},{hue},{de},{preview_de}", hv[0], hv[1], hv[2], hv[3])?;
                    // Crops retain export pixels: top highlight region and central portrait/skin detail.
                    let cw = export.width.min(640);
                    let ch = export.height.min(640);
                    let x0 = (export.width - cw) / 2;
                    let y0 = if slider == "Highlights" { 0 } else { (export.height - ch) / 2 };
                    let crop = Rgba8::from_fn(cw, ch, |x, y| export.get(x + x0, y + y0));
                    let jpeg = encode_jpeg(&EncodeImage::rgba8(&crop), 95, ChromaSubsampling::S444, &EncodeMeta::default())?;
                    let file = format!("scene-{idx}-{slider}-{value}-{key}.jpg");
                    std::fs::write(dir.join(&file), &jpeg)?;
                    images.push((label, base64(&jpeg)));
                    eprintln!("{name} {slider} {value:+} {key}: halo {hv:?}, LOE {loe:.5}, hue {hue:.4}°, preview ΔE {preview_de:.4}");
                }
                html.push_str("</table>");
                for (label, data) in images {
                    writeln!(html, "<figure><figcaption>{label}</figcaption><img alt=\"{label}\" src=\"data:image/jpeg;base64,{data}\"></figure>")?;
                }
            }
        }
        html.push_str("</details>");
    }
    html.push_str(
        "<p>JPEG crops are also present beside this standalone page. No original RAW bytes are stored here. Metrics CSV: metrics.csv.</p></html>",
    );
    std::fs::write(dir.join("index.html"), html)?;
    std::fs::write(dir.join("metrics.csv"), csv)?;
    eprintln!("comparison: {}", dir.join("index.html").display());
    if failed_limits > 0 {
        return Err(format!("{failed_limits} comparisons exceeded hue/preview limits; see the metrics table").into());
    }
    Ok(())
}
