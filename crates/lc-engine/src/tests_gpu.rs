//! The toolset upgrades through the engine's own render path on the GPU (`media::develop` via a
//! view's [`RenderJob`] with its stage cache), against the CPU pipeline: raw options decoded again
//! when switched, lens profiles from the real lens database, AI Remove patches and AI Denoise
//! results (both put in place before the renderers split). Same bounds as `lightcraft-gpu`'s
//! equivalence tests (mean |Δ| < 0.5 LSB, max |Δ| ≤ 3 LSB). Skips when no GPU adapter exists.

use std::sync::Arc;

use lightcraft_catalog::PhotoId;
use lightcraft_pipeline::{RenderRequest, SourceInfo, StageCache};
use lightcraft_raster::{Rgb32f, Rgba8};
use serde_json::json;

use crate::Session;
use crate::media::{RenderJob, SourceRef};

/// The GPU's process-wide state (in use, or stopped after a failure) is shared by every test:
/// tests that need the GPU path and tests that inject a GPU failure
/// (`tests_export::export_falls_back_to_the_cpu_when_gpu_work_is_lost`) take this lock, so a
/// simulated failure never turns the GPU off under another test.
pub(crate) static GPU_STATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) fn gpu_state() -> std::sync::MutexGuard<'static, ()> {
    GPU_STATE.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn gpu() -> bool {
    let ok = lightcraft_gpu::available();
    if !ok {
        eprintln!("skipped: no GPU adapter ({:?})", lightcraft_gpu::unavailable_reason());
    }
    ok
}

/// (mean |Δ|, max |Δ|).
fn diff(a: &Rgba8, b: &Rgba8) -> (f64, u8) {
    assert_eq!((a.width, a.height), (b.width, b.height));
    let (mut sum, mut max) = (0u64, 0u8);
    for (p, q) in a.data.iter().zip(&b.data) {
        for c in 0..3 {
            let d = p[c].abs_diff(q[c]);
            sum += d as u64;
            max = max.max(d);
        }
    }
    (sum as f64 / (a.data.len() * 3) as f64, max)
}

fn assert_close(name: &str, cpu: &Rgba8, gpu: &Rgba8) {
    let (mean, max) = diff(cpu, gpu);
    eprintln!("{name:<40} {}x{}  mean {mean:.4}  max {max}", cpu.width, cpu.height);
    assert!(mean < 0.5 && max <= 3, "{name}: mean {mean:.4} LSB, max {max} LSB");
}

/// A view's render of `id` (on the GPU when there is one), with the GPU's and the CPU's render of
/// exactly what the job renders (its decoded source after `enhance::for_render`, its settings).
struct Views {
    view: Rgba8,
    gpu: Rgba8,
    cpu: Rgba8,
    source: Arc<Rgb32f>,
}

fn views(s: &mut Session, id: PhotoId, size: usize, stages: &Arc<StageCache>) -> Views {
    let mut job: RenderJob = s.render_job(id, size, size, false, true).expect("job").with_stages(stages.clone());
    let decoded = job.source.load_source().expect("source");
    let info: SourceInfo = decoded.info_or(job.info.clone());
    job.source = SourceRef::Loaded(Box::new(decoded.clone()));
    let (src, settings) = crate::enhance::for_render(&decoded.image, &job.settings, job.source_key);
    let req: RenderRequest = job.request;
    let gpu = lightcraft_gpu::render(&src, &info, &settings, &req, None)
        .unwrap_or_else(|| panic!("gpu render ({:?})", lightcraft_gpu::last_fallback()))
        .image;
    let cpu = lightcraft_pipeline::render(&src, &info, &settings, &req).image;
    let r = job.run();
    let view = r.rendered.as_ref().expect("view render").image.clone();
    s.accept(&r);
    Views { view, gpu, cpu, source: decoded.image }
}

#[test]
fn raw_options_decode_again_and_render_on_the_gpu() {
    let _gpu = gpu_state();
    if !gpu() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("lc-gpu-raw-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.dng");
    std::fs::write(&path, crate::tests_toolset::textured_dng()).unwrap();
    let mut s = Session::new().with_fs();
    let r = s.execute("library.import", &json!({"paths": [path.to_string_lossy()]})).unwrap();
    let id = PhotoId(r["imported"][0].as_u64().unwrap());
    s.execute("library.select", &json!({"ids": [id.0]})).unwrap();
    s.execute("develop.merge", &json!({"settings": {"light": {"exposure": 0.3}, "effects": {"clarity": 15.0}}})).unwrap();
    let stages = Arc::new(StageCache::default());
    let first = views(&mut s, id, 640, &stages);
    assert_close("raw default", &first.cpu, &first.view);
    assert_eq!(first.view, first.gpu, "the view rendered on the GPU");
    assert!(lightcraft_gpu::stage_bytes(&stages) > 0, "the view's GPU stages are in use");
    let mut sources = vec![first.source.clone()];
    for (name, raw) in [
        ("rcd", json!({"demosaic": "rcd"})),
        ("dual", json!({"demosaic": "dualRcd", "dual_threshold": 30.0})),
        ("opposed", json!({"demosaic": "auto", "highlights": "opposed"})),
        ("clip", json!({"highlights": "clip"})),
        ("rcd + opposed", json!({"demosaic": "rcd", "highlights": "opposed"})),
    ] {
        s.execute("develop.merge", &json!({"settings": {"raw": raw}})).unwrap();
        let v = views(&mut s, id, 640, &stages);
        assert!(sources.iter().all(|o| !Arc::ptr_eq(o, &v.source) && o.data != v.source.data), "{name}: decoded again");
        assert_close(&format!("raw {name}"), &v.cpu, &v.view);
        assert_eq!(v.view, v.gpu, "{name}: the view (cached stages) equals a fresh GPU render");
        assert_ne!(v.view, first.view, "{name}: changes the render");
        sources.push(v.source);
    }
    // back to the defaults: the original image, bit for bit
    s.execute("develop.merge", &json!({"settings": {"raw": {"demosaic": "auto", "highlights": "reconstruct"}}})).unwrap();
    let back = views(&mut s, id, 640, &stages);
    assert_eq!(back.source.data, first.source.data, "the default decode");
    assert_eq!(back.view, first.view, "the original render, bit for bit");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lens_database_profiles_render_on_the_gpu() {
    let _gpu = gpu_state();
    if !gpu() {
        return;
    }
    // a fully calibrated lens from the bundled database (distortion, TCA, vignetting)
    let db = crate::lens_db::database().unwrap();
    let lens = db.lenses.iter().find(|l| l.calib_distortion.len() > 1 && l.calib_vignetting.len() > 2 && !l.calib_tca.is_empty()).unwrap();
    let cam = db.cameras.iter().find(|c| lens.mounts.contains(&c.mount)).unwrap();
    let mut p =
        lightcraft_catalog::Photo::new(PhotoId(1), lightcraft_catalog::Source::File { path: "x.nef".into() }, "x.nef", "NEF", 900, 600, "2026-10-09");
    p.meta.camera = format!("{} {}", cam.maker, cam.model);
    p.meta.lens = lens.model.clone();
    p.meta.focal_mm = Some(lens.calib_distortion[0].focal);
    p.meta.aperture = Some(lens.calib_vignetting[0].aperture);
    let src = Arc::new(lightcraft_scenes::demo_library()[2].render(900, 600));
    let req = RenderRequest::fit(720, 720);
    let mut s = lightcraft_develop::DevelopSettings::default();
    s.lens_db.enabled = true;
    s.lens_db.lens = Some(lightcraft_develop::LensName { maker: lens.maker.clone(), model: lens.model.clone() });
    let info = SourceInfo { raw: true, ..crate::media::job_info(&p, &s) };
    let c = info.lens_db.expect("a correction");
    eprintln!("lens: {} {} on {} {}: {c:?}", lens.maker, lens.model, cam.maker, cam.model);
    for strength in [0.0, 100.0, 200.0] {
        s.lens_db.distortion = strength;
        s.lens_db.tca = strength;
        s.lens_db.vignetting = strength;
        for crop in [false, true] {
            let mut s = s.clone();
            if crop {
                s.crop.geometry.rect = lightcraft_geom::Rect::new(0.1, 0.05, 0.85, 0.9);
                s.crop.geometry.angle = -6.0;
                s.orientation = lightcraft_geom::Orientation::Rotate90;
            }
            let cpu = lightcraft_pipeline::render(&src, &info, &s, &req).image;
            let gpu = crate::media::develop(&src, &info, &s, &req, None, true).image;
            assert_eq!(gpu, lightcraft_gpu::render(&src, &info, &s, &req, None).expect("gpu").image, "develop() used the GPU");
            assert_close(&format!("lens db {strength}% crop={crop}"), &cpu, &gpu);
        }
    }
}

fn magenta(c: [u8; 4]) -> bool {
    c[0] as i32 > c[1] as i32 + 60 && c[2] as i32 > c[1] as i32 + 60
}

/// Paints the masked area magenta (`tests_enhance`'s mock engine).
struct Mock;

impl crate::enhance::AiHost for Mock {
    fn remove_engines(&self) -> Vec<crate::enhance::RemoveEngine> {
        vec![crate::enhance::RemoveEngine { key: "mock".into(), label: "Mock".into(), problem: None }]
    }
    fn remove(&self, req: &crate::enhance::RemoveRequest, _: &crate::enhance::JobCtl) -> Result<crate::enhance::RemoveResult, String> {
        let rgb = req.rgb.iter().zip(&req.mask).map(|(c, m)| if *m > 127 { [250, 0, 250] } else { *c }).collect();
        Ok(crate::enhance::RemoveResult { rgb, alpha: req.mask.iter().map(|m| if *m > 127 { 255 } else { 0 }).collect() })
    }
    fn start_model_download(&self, _: &str) -> Result<(), String> {
        Err("not in tests".into())
    }
    fn model_download(&self, _: &str) -> Option<crate::enhance::Download> {
        None
    }
    fn cancel_model_download(&self, _: &str) {}
}

#[test]
fn ai_remove_and_denoise_are_in_the_gpu_render() {
    let _gpu = gpu_state();
    if !gpu() {
        return;
    }
    let mut s = Session::with_demo();
    s.enhance.host = Some(Arc::new(Mock));
    // (a raw demo photo that `enhance::denoise`'s and `tests_enhance`' tests don't use, so the
    // stand-in's stored result is this test's own)
    let ids: Vec<_> = s.visible_cloned().into_iter().filter(|id| crate::enhance::denoise::can_denoise(&s, *id).is_ok()).collect();
    let id = ids[ids.len() / 2];
    s.execute("library.select", &json!({"ids": [id.0]})).unwrap();
    let stages = Arc::new(StageCache::default());
    let plain = views(&mut s, id, 480, &stages);
    let mid = |img: &Rgba8| img.get(img.width / 2, img.height / 2);
    assert!(!magenta(mid(&plain.view)));

    // AI Remove: the stub patch is composited in the linear stage (CPU), then rendered on the GPU
    s.execute("spot.add", &json!({"mode": "ai", "points": [[0.5, 0.5]], "size": 0.06, "engine": "mock", "seed": 3, "wait": true})).unwrap();
    let v = views(&mut s, id, 480, &stages);
    assert!(magenta(mid(&v.gpu)), "the GPU render shows the patch: {:?}", mid(&v.gpu));
    assert_eq!(v.view, v.gpu, "the view (cached stages) shows the patch");
    assert_close("ai remove", &v.cpu, &v.gpu);
    s.execute("spot.update", &json!({"opacity": 50})).unwrap();
    let v = views(&mut s, id, 480, &stages);
    assert_eq!(v.view, v.gpu);
    assert_close("ai remove, opacity 50", &v.cpu, &v.gpu);
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    let v = views(&mut s, id, 480, &stages);
    assert_eq!(v.view, plain.view, "undone: the photo as before");

    // AI Denoise: a stand-in result (mid grey) mixed into the source by the amount
    s.enhance.denoiser = Some(Arc::new(|px, _, _, _| Ok(vec![[0.18, 0.18, 0.18]; px.len()])));
    s.execute("enhance.denoise", &json!({"wait": true})).unwrap();
    let mut prev = plain.view.clone();
    for amount in [60.0, 100.0, 20.0] {
        s.execute("develop.set", &json!({"control": "enhance.denoise", "value": amount})).unwrap();
        let v = views(&mut s, id, 480, &stages);
        assert_eq!(v.view, v.gpu, "{amount}%: the view renders the mixed source");
        assert_close(&format!("ai denoise {amount}%"), &v.cpu, &v.gpu);
        assert_ne!(v.view, prev, "{amount}%: the amount changes the render");
        prev = v.view;
    }
    s.execute("develop.set", &json!({"control": "enhance.denoise", "value": 0})).unwrap();
    assert_eq!(views(&mut s, id, 480, &stages).view, plain.view, "amount 0: as before");
}
