//! Verification: a few small test renders through the same generation path the app uses
//! ([`crate::ops::Ai`], submitting only to an idle queue), judged by image measurements that
//! catch the silent failures of a wrong text encoder, VAE or precision (black, blank, flat or
//! noise images, an edit that changes nothing, a refine that drifts). Runs only when asked, one
//! model at a time, and frees ComfyUI's memory afterwards.
//!
//! Thresholds: `luma_mean < 8` black; `luma_mean > 247 && luma_std < 6` white; `luma_std < 6`
//! flat; `noise_ratio > 0.65` noise (latent-scale noise measures about 1, natural pictures under
//! 0.4); refine difference over [`crate::refine::PASS_LIMIT`]; edit difference under 0.8; fill
//! difference inside the mask under 8. Soft checks (warnings): Create `red_centre < 0.15 ||
//! ring_luma < 150`, Edit `blue_centre < 0.15 || ring_diff > 30`.
//!
//! Measured on 2026-10-24 (ComfyUI 0.42.5, RTX 5090; FLUX.3 Klein base and distilled, Krea 2
//! Turbo and MOSS-Image SFT in BF16 through templates, FLUX.2 Klein 9B FP8 and Qwen Image 2.1 BF16
//! through builders): Create luma mean 197-242, std 38-83, noise ratio 0.025-0.032, red centre
//! 0.44-1.0, ring luma 239-255; Edit difference 13-50, blue centre 0.44-1.0, ring difference
//! 0.4-2.7; Refine difference 3.4-14.6; Fill difference inside the mask 46-102, outside 0.04-2.3.
//! The share of the most common colour (0.47-0.92: a product shot on white is mostly one colour)
//! cannot tell a correct picture from a flat one with any margin, so it is recorded but not used.
//! The live negative controls (a 16-channel VAE for MOSS-Image, a text-only encoder for FLUX.3)
//! failed in ComfyUI before an image was made and are explained by the error patterns; the black,
//! white, flat and noise checks are exercised by synthetic images only.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use image::{GrayImage, Luma, RgbImage, RgbaImage};

use super::state::{CapabilityResult, Metrics, Verified};
use crate::catalog::ModelId;
use crate::comfy::{ComfyClient, JobControl};
use crate::ops::{Ai, GenerateMode, GenerateRequest};

pub const CREATE_PROMPT: &str = "A single bright red ball in the centre of a plain white background, studio product photo, no other objects.";
pub const EDIT_PROMPT: &str = "Make the ball blue. Keep everything else exactly the same.";
pub const FILL_PROMPT: &str = "a red ball";
/// The fill test's selection (fractions of the test picture: around its sun).
pub const MASK_BOX: (f32, f32, f32, f32) = (0.70, 0.12, 0.86, 0.34);
/// Longest a single test render may take.
pub const RENDER_TIMEOUT: Duration = Duration::from_secs(600);
/// How long to wait for someone else's queue before giving up (nothing is recorded).
pub const BUSY_WAIT: Duration = Duration::from_secs(120);

const BLACK_LUMA: f64 = 8.0;
const WHITE_LUMA: f64 = 247.0;
const FLAT_STD: f64 = 6.0;
const NOISE_RATIO: f64 = 0.65;
const EDIT_NOCHANGE: f64 = 0.8;
const FILL_NOCHANGE: f64 = 8.0;
const PROMPT_RED: f64 = 0.15;
const PROMPT_RING_LUMA: f64 = 150.0;
const EDIT_BLUE: f64 = 0.15;
const EDIT_RING_DIFF: f64 = 30.0;

fn luma(p: &image::Rgb<u8>) -> f64 {
    0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2])
}

fn small(img: &RgbImage) -> RgbImage {
    let (w, h) = img.dimensions();
    let scale = 128.0 / f64::from(w.max(h).max(1));
    if scale >= 1.0 {
        return img.clone();
    }
    let (nw, nh) = (((f64::from(w) * scale).round() as u32).max(1), ((f64::from(h) * scale).round() as u32).max(1));
    image::imageops::resize(img, nw, nh, image::imageops::FilterType::Triangle)
}

fn in_ring(x: u32, y: u32, w: u32, h: u32) -> bool {
    let (rx, ry) = ((f64::from(w) * 0.08).ceil() as u32, (f64::from(h) * 0.08).ceil() as u32);
    x < rx || y < ry || x >= w.saturating_sub(rx) || y >= h.saturating_sub(ry)
}

fn in_centre(x: u32, y: u32, w: u32, h: u32) -> bool {
    let (fx, fy) = (f64::from(x) / f64::from(w), f64::from(y) / f64::from(h));
    (0.3..0.7).contains(&fx) && (0.3..0.7).contains(&fy)
}

/// Measures an image (downsampled to 128 px), against `source` for the ring difference.
pub fn metrics(img: &RgbImage, source: Option<&RgbImage>) -> Metrics {
    let s = small(img);
    let (w, h) = s.dimensions();
    let n = f64::from(w * h).max(1.0);
    let lum: Vec<f64> = s.pixels().map(luma).collect();
    let mean = lum.iter().sum::<f64>() / n;
    let std = (lum.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / n).sqrt();
    let (mut diff, mut count) = (0.0f64, 0.0f64);
    for y in 0..h {
        for x in 0..w {
            let l = lum[(y * w + x) as usize];
            if x + 1 < w {
                diff += (lum[(y * w + x + 1) as usize] - l).abs();
                count += 1.0;
            }
            if y + 1 < h {
                diff += (lum[((y + 1) * w + x) as usize] - l).abs();
                count += 1.0;
            }
        }
    }
    let mut bins = std::collections::HashMap::<(u8, u8, u8), u32>::new();
    let (mut red, mut blue, mut centre, mut ring, mut ring_n) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (x, y, p) in s.enumerate_pixels() {
        *bins.entry((p[0] >> 4, p[1] >> 4, p[2] >> 4)).or_default() += 1;
        if in_centre(x, y, w, h) {
            centre += 1.0;
            let (r, g, b) = (i32::from(p[0]), i32::from(p[1]), i32::from(p[2]));
            if r >= 120 && r - g.max(b) >= 50 {
                red += 1.0;
            }
            if b - r.max(g) >= 40 {
                blue += 1.0;
            }
        }
        if in_ring(x, y, w, h) {
            ring += luma(p);
            ring_n += 1.0;
        }
    }
    let ring_diff = source.map(|src| {
        let src = image::imageops::resize(src, w, h, image::imageops::FilterType::Triangle);
        let (mut d, mut k) = (0.0, 0.0);
        for (x, y, p) in s.enumerate_pixels() {
            if in_ring(x, y, w, h) {
                let q = src.get_pixel(x, y);
                d += (0..3).map(|c| (f64::from(p[c]) - f64::from(q[c])).abs()).sum::<f64>() / 3.0;
                k += 1.0;
            }
        }
        d / f64::max(k, 1.0)
    });
    Metrics {
        luma_mean: mean,
        luma_std: std,
        noise_ratio: (diff / count.max(1.0)) / std.max(1.0),
        top_bin_share: f64::from(bins.values().copied().max().unwrap_or(0)) / n,
        red_centre: red / f64::max(centre, 1.0),
        blue_centre: blue / f64::max(centre, 1.0),
        ring_luma: ring / f64::max(ring_n, 1.0),
        ring_diff: ring_diff.unwrap_or(0.0),
    }
}

/// The hard image failure, if any: `black`, `white`, `flat` or `noise`.
pub fn image_failure(m: &Metrics) -> Option<&'static str> {
    if m.luma_mean < BLACK_LUMA {
        Some("black")
    } else if m.luma_mean > WHITE_LUMA && m.luma_std < FLAT_STD {
        Some("white")
    } else if m.luma_std < FLAT_STD {
        Some("flat")
    } else if m.noise_ratio > NOISE_RATIO {
        Some("noise")
    } else {
        None
    }
}

pub fn create_warnings(m: &Metrics) -> Vec<String> {
    if m.red_centre < PROMPT_RED || m.ring_luma < PROMPT_RING_LUMA { vec!["VerifyWarning:prompt".into()] } else { Vec::new() }
}

pub fn edit_warnings(m: &Metrics) -> Vec<String> {
    if m.blue_centre < EDIT_BLUE || m.ring_diff > EDIT_RING_DIFF { vec!["VerifyWarning:edit".into()] } else { Vec::new() }
}

/// The fill test's selection for an image of `w x h`.
pub fn fill_mask(w: u32, h: u32) -> GrayImage {
    let (x0, y0, x1, y1) = MASK_BOX;
    GrayImage::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        Luma([if (x0..x1).contains(&fx) && (y0..y1).contains(&fy) { 255 } else { 0 }])
    })
}

/// Mean absolute difference inside and outside `mask`.
pub fn masked_difference(a: &RgbImage, b: &RgbImage, mask: &GrayImage) -> (f64, f64) {
    let b = if b.dimensions() == a.dimensions() { b.clone() } else { image::imageops::resize(b, a.width(), a.height(), image::imageops::FilterType::Triangle) };
    let (mut inside, mut ni, mut outside, mut no) = (0.0, 0.0, 0.0, 0.0);
    for (x, y, p) in a.enumerate_pixels() {
        let q = b.get_pixel(x, y);
        let d = (0..3).map(|c| (f64::from(p[c]) - f64::from(q[c])).abs()).sum::<f64>() / 3.0;
        if mask.get_pixel(x, y)[0] > 127 {
            inside += d;
            ni += 1.0;
        } else {
            outside += d;
            no += 1.0;
        }
    }
    (inside / f64::max(ni, 1.0), outside / f64::max(no, 1.0))
}

/// The create test's size: the model's native size, at most 1 MP, snapped to its grid, short
/// side at least 512.
pub fn test_size(native: u32, grid: u32) -> (u32, u32) {
    let side = native.clamp(512, 1024);
    crate::imaging::snap_size(side, side, grid.max(8))
}

/// Samples ComfyUI's free GPU memory every 500 ms while alive; the lowest free value gives the
/// memory in use during the test (all processes on that GPU).
struct VramSampler {
    stop: Arc<AtomicBool>,
    min_free: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl VramSampler {
    fn start(host: &str) -> Self {
        let (stop, min_free, total) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicU64::new(u64::MAX)), Arc::new(AtomicU64::new(0)));
        let (s, m, t, client) = (stop.clone(), min_free.clone(), total.clone(), ComfyClient::new(host));
        let thread = std::thread::Builder::new()
            .name("verify-vram".into())
            .spawn(move || {
                while !s.load(Ordering::SeqCst) {
                    if let Ok(stats) = client.system_stats()
                        && let Some(d) = stats["devices"].as_array().and_then(|d| d.first())
                    {
                        t.store(d["vram_total"].as_u64().unwrap_or(0), Ordering::SeqCst);
                        m.fetch_min(d["vram_free"].as_u64().unwrap_or(u64::MAX), Ordering::SeqCst);
                    }
                    std::thread::sleep(Duration::from_millis(500));
                }
            })
            .ok();
        Self { stop, min_free, total, thread }
    }

    /// GB in use at the busiest sample.
    fn finish(mut self) -> Option<f64> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        let (total, free) = (self.total.load(Ordering::SeqCst), self.min_free.load(Ordering::SeqCst));
        (total > 0 && free != u64::MAX).then(|| total.saturating_sub(free) as f64 / 1e9)
    }
}

/// A per-render job that is cancelled after [`RENDER_TIMEOUT`] or when `parent` is.
fn timed(parent: &JobControl) -> (JobControl, Arc<AtomicBool>) {
    let ctl = JobControl::new();
    let timed_out = Arc::new(AtomicBool::new(false));
    let (c, p, flag) = (ctl.clone(), parent.clone(), timed_out.clone());
    let _ = std::thread::Builder::new().name("verify-timeout".into()).spawn(move || {
        let start = Instant::now();
        while !c.is_cancelled() {
            if p.is_cancelled() {
                c.cancel();
                break;
            }
            if start.elapsed() > RENDER_TIMEOUT {
                flag.store(true, Ordering::SeqCst);
                c.cancel();
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });
    (ctl, timed_out)
}

/// Waits for ComfyUI's queue to be empty (up to [`BUSY_WAIT`]).
pub fn wait_idle(client: &ComfyClient, ctl: &JobControl) -> Result<bool> {
    let start = Instant::now();
    loop {
        ctl.check()?;
        if crate::refine::queue_idle(client)? {
            return Ok(true);
        }
        if start.elapsed() > BUSY_WAIT {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// A cross-process verification lock (`<state>/ai-verify.lock`: pid and start time; stale after
/// ten minutes).
pub struct VerifyLock(std::path::PathBuf);

impl VerifyLock {
    pub fn acquire(path: &std::path::Path) -> Option<Self> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        if let Ok(text) = std::fs::read_to_string(path) {
            let started = text.split_whitespace().nth(1).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            if now.saturating_sub(started) < 600 {
                return None;
            }
            let _ = std::fs::remove_file(path);
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(path).ok()?;
        use std::io::Write;
        let _ = write!(f, "{} {now}", std::process::id());
        Some(Self(path.to_owned()))
    }
}

impl Drop for VerifyLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// What to verify.
pub struct Target<'a> {
    pub model: ModelId,
    /// The preset that loads the file under test (empty: the model's default preset).
    pub variant: &'a str,
    pub label: &'a str,
    /// For messages: the main file's precision and the companions in use.
    pub quant: String,
    pub te: String,
    pub vae: String,
}

/// One finished capability, for progress output.
pub struct Step<'a> {
    pub capability: &'a str,
    pub result: &'a CapabilityResult,
}

fn rgb(img: &RgbaImage) -> RgbImage {
    image::DynamicImage::ImageRgba8(crate::imaging::over_white(img)).to_rgb8()
}

fn failure_message(e: &anyhow::Error) -> String {
    crate::comfy::friendly_of(e).map(|f| f.message.clone()).unwrap_or_else(|| format!("{e:#}"))
}

/// Renders one capability and records it.
struct Runner<'a> {
    ai: Ai,
    host: &'a str,
    ctl: &'a JobControl,
    cold: bool,
    out: Verified,
    progress: &'a mut dyn FnMut(Step),
}

impl Runner<'_> {
    fn record(&mut self, name: &str, mut r: CapabilityResult) {
        r.pass = r.failure.is_none();
        (self.progress)(Step { capability: name, result: &r });
        self.out.capabilities.insert(name.to_owned(), r);
    }

    fn run(&mut self, name: &str, req: &GenerateRequest, judge: &dyn Fn(&RgbaImage, &mut CapabilityResult)) -> Result<Option<RgbaImage>> {
        let (job, timed_out) = timed(self.ctl);
        let sampler = VramSampler::start(self.host);
        let start = Instant::now();
        let result = self.ai.generate(req, &job);
        let mut r = CapabilityResult { seconds: start.elapsed().as_secs_f64(), cold: self.cold, peak_vram_gb: sampler.finish(), ..Default::default() };
        job.cancel();
        self.cold = false;
        let image = match result {
            Ok(img) => {
                r.size = Some(img.dimensions());
                judge(&img, &mut r);
                Some(img)
            }
            Err(_) if timed_out.load(Ordering::SeqCst) => {
                r.failure = Some("VerifyTimeout".into());
                None
            }
            Err(_) if self.ctl.is_cancelled() => bail!(lightcraft_sysmem::jobs::Cancelled),
            Err(e) => {
                r.failure = Some("VerifyFailed:error".into());
                r.message = Some(failure_message(&e));
                None
            }
        };
        self.record(name, r);
        Ok(image)
    }
}

/// Runs the tests for one model and returns what they measured (the caller frees ComfyUI's
/// memory afterwards). Errors only when the run could not start or was cancelled; a failed render is a
/// failed capability.
pub fn verify(host: &str, t: &Target, seed: u64, ctl: &JobControl, progress: &mut dyn FnMut(Step)) -> Result<Verified> {
    let _job = crate::refine::TEST_JOB.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let client = ComfyClient::new(host);
    if !wait_idle(&client, ctl)? {
        bail!(super::Busy);
    }
    let info = t.model.info();
    let f = &info.resolved;
    let mut runner = Runner { ai: Ai::idle_only(host), host, ctl, cold: true, out: Verified { seed, ..Default::default() }, progress };
    let (w, h) = test_size(f.sizes.native, f.sizes.multiple);
    let variant = if t.variant.is_empty() { crate::catalog::default_variant(t.model) } else { t.variant };
    let mut create = GenerateRequest::new(t.model, CREATE_PROMPT);
    create.variant = variant.to_owned();
    create.width = w;
    create.height = h;
    create.seed = seed;
    create.negative = f.prompt.negative.clone();
    let mut made = None;
    if info.text_to_image {
        let grid = f.sizes.multiple.max(8);
        let judge = |img: &RgbaImage, r: &mut CapabilityResult| {
            let m = metrics(&rgb(img), None);
            r.requested = Some((w, h));
            if img.width().abs_diff(w) > grid || img.height().abs_diff(h) > grid {
                r.failure = Some("VerifyFailed:wrong_size".into());
            } else if let Some(fail) = image_failure(&m) {
                r.failure = Some(format!("VerifyFailed:{fail}"));
            } else {
                r.warnings = create_warnings(&m);
            }
            r.metrics = Some(m);
        };
        made = runner.run("create", &create, &judge)?;
    }
    if f.capabilities.edit {
        let source = made.clone().unwrap_or_else(|| image::DynamicImage::ImageRgb8(crate::refine::test_image()).to_rgba8());
        let src = rgb(&source);
        let req = GenerateRequest { mode: GenerateMode::Edit, prompt: EDIT_PROMPT.into(), source: Some(source), seed: seed.wrapping_add(1), ..create.clone() };
        let judge = |img: &RgbaImage, r: &mut CapabilityResult| {
            let out = image::imageops::resize(&rgb(img), src.width(), src.height(), image::imageops::FilterType::Triangle);
            let m = metrics(&out, Some(&src));
            let changed = crate::refine::mean_abs_difference(&src, &out);
            r.difference = Some(changed);
            if let Some(fail) = image_failure(&m) {
                r.failure = Some(format!("VerifyFailed:{fail}"));
            } else if changed < EDIT_NOCHANGE {
                r.failure = Some("VerifyFailed:edit_nochange".into());
            } else {
                r.warnings = edit_warnings(&m);
            }
            r.metrics = Some(m);
        };
        runner.run("edit", &req, &judge)?;
    }
    // Refine: the recipe the app's refine checker uses, on its test picture.
    if let Some(recipe) = refine_recipe(t.model, variant, &client) {
        let (job, timed_out) = timed(ctl);
        let sampler = VramSampler::start(host);
        let start = Instant::now();
        let res = crate::refine::preservation(&client, &recipe, seed.wrapping_add(2), &job);
        job.cancel();
        let mut r = CapabilityResult { seconds: start.elapsed().as_secs_f64(), peak_vram_gb: sampler.finish(), ..Default::default() };
        match res {
            Ok((d, img)) => {
                r.size = Some(img.dimensions());
                r.difference = Some(d);
                if d > crate::refine::PASS_LIMIT {
                    r.failure = Some("VerifyFailed:refine_drift".into());
                }
            }
            Err(_) if timed_out.load(Ordering::SeqCst) => r.failure = Some("VerifyTimeout".into()),
            Err(_) if ctl.is_cancelled() => bail!(lightcraft_sysmem::jobs::Cancelled),
            Err(e) => {
                r.failure = Some("VerifyFailed:error".into());
                r.message = Some(failure_message(&crate::ops::explain_with_recipe(e, info.recipe.as_deref())));
            }
        }
        let upscale = CapabilityResult { pass: r.failure.is_none(), via: Some("refine".into()), ..Default::default() };
        runner.record("refine", r);
        runner.out.capabilities.insert("upscale".into(), upscale);
    }
    if f.capabilities.inpaint {
        let picture = crate::refine::test_image();
        let mask = fill_mask(picture.width(), picture.height());
        let req = GenerateRequest {
            mode: GenerateMode::Inpaint,
            prompt: FILL_PROMPT.into(),
            source: Some(image::DynamicImage::ImageRgb8(picture.clone()).to_rgba8()),
            mask: Some(mask.clone()),
            denoise: 1.0,
            seed: seed.wrapping_add(3),
            ..create.clone()
        };
        let judge = |img: &RgbaImage, r: &mut CapabilityResult| {
            let (inside, outside) = masked_difference(&picture, &rgb(img), &mask);
            r.difference = Some(inside);
            r.outside_change = Some(outside);
            if inside < FILL_NOCHANGE {
                r.failure = Some("VerifyFailed:inpaint_nochange".into());
            }
        };
        runner.run("inpaint", &req, &judge)?;
    }
    Ok(runner.out)
}

/// The refine recipe the app would test for `model`'s `variant` (its compiled template recipe, or
/// its family builder).
fn refine_recipe(model: ModelId, variant: &str, client: &ComfyClient) -> Option<crate::refine::Recipe> {
    let info = client.object_info().ok()?;
    let target = crate::refine::target(model, variant, &info).ok()?;
    crate::refine::family_recipe(model, variant, &target, &info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn ball(r: u8, g: u8, b: u8) -> RgbImage {
        RgbImage::from_fn(256, 256, |x, y| {
            let (dx, dy) = (x as f32 - 128.0, y as f32 - 128.0);
            if dx * dx + dy * dy < 60.0 * 60.0 {
                let shade = (1.0 - (dx * dx + dy * dy).sqrt() / 90.0).clamp(0.4, 1.0);
                Rgb([(f32::from(r) * shade) as u8, (f32::from(g) * shade) as u8, (f32::from(b) * shade) as u8])
            } else {
                Rgb([245 - (y / 32) as u8, 245 - (y / 32) as u8, 245])
            }
        })
    }

    fn noise() -> RgbImage {
        let mut s = 0x2545_f491_u32;
        RgbImage::from_fn(256, 256, |_, _| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let v = (s & 0xff) as u8;
            Rgb([v, v.wrapping_mul(7), v.wrapping_mul(13)])
        })
    }

    #[test]
    fn measurements_separate_pictures_from_broken_renders() {
        let red = metrics(&ball(230, 30, 30), None);
        assert_eq!(image_failure(&red), None, "{red:?}");
        assert!(create_warnings(&red).is_empty(), "{red:?}");
        assert_eq!(image_failure(&metrics(&RgbImage::new(64, 64), None)), Some("black"));
        assert_eq!(image_failure(&metrics(&RgbImage::from_pixel(64, 64, Rgb([255, 255, 255])), None)), Some("white"));
        assert_eq!(image_failure(&metrics(&RgbImage::from_pixel(64, 64, Rgb([90, 140, 60])), None)), Some("flat"));
        let n = metrics(&noise(), None);
        assert_eq!(image_failure(&n), Some("noise"), "{n:?}");
        // A blue ball on the same background passes Create but warns: it is not what was asked.
        let blue = metrics(&ball(30, 40, 230), None);
        assert_eq!(create_warnings(&blue), ["VerifyWarning:prompt"]);
        let edited = metrics(&ball(30, 40, 230), Some(&ball(230, 30, 30)));
        assert!(edit_warnings(&edited).is_empty(), "{edited:?}");
    }

    #[test]
    fn the_fill_mask_covers_the_test_pictures_sun() {
        let m = fill_mask(512, 288);
        assert_eq!(m.get_pixel(400, 60)[0], 255);
        assert_eq!(m.get_pixel(100, 200)[0], 0);
        let a = crate::refine::test_image();
        let mut b = a.clone();
        for (x, y, p) in b.enumerate_pixels_mut() {
            if m.get_pixel(x, y)[0] > 0 {
                *p = Rgb([200, 0, 0]);
            }
        }
        let (inside, outside) = masked_difference(&a, &b, &m);
        assert!(inside > FILL_NOCHANGE && outside == 0.0, "{inside} {outside}");
        assert_eq!(test_size(1536, 16), (1024, 1024));
        assert_eq!(test_size(1024, 64), (1024, 1024));
        assert_eq!(test_size(300, 8), (512, 512));
    }
}
