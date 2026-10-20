//! The shared measurement protocol. Both examples compile this same module: no decoder,
//! geometry, input synthesis or scoring differences can favour one backend. No new dependencies.
//! Inputs: bounded modern-RLE Radiance RGBE HDR files, -Y/+X only. Originals stay in target/.
//! RGBE reference: Radiance src/common/color.c and src/common/colrops.c.
//! This is a small quality sample, not a benchmark or a camera-raw acceptance suite.
use std::error::Error;
use std::io::{BufRead, Cursor, Read};
use std::path::Path;
use std::time::Instant;

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub const MAX_FEATURES: usize = 800;
pub const THRESHOLD: f64 = 1.5;
pub const SEED: u64 = 10;
pub const EXPOSURES: [f32; 3] = [0.0625, 1.0, 16.0];

pub struct Image {
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<[f32; 3]>,
}

pub struct Pair {
    pub w: usize,
    pub h: usize,
    pub a: Vec<f32>,
    pub b: Vec<f32>,
    /// Integer pixel centres, reference -> moving image.
    pub truth: [f64; 9],
}

#[derive(Default)]
pub struct Registration {
    pub features: [usize; 2],
    pub matches: Vec<([f64; 2], [f64; 2])>,
    pub fit: Option<[f64; 9]>,
    pub inliers: Vec<usize>,
    pub detect_ms: f64,
    pub match_ms: f64,
    pub fit_ms: f64,
}

pub struct Bracket {
    pub truth: Image,
    pub frames: Vec<Vec<[f32; 3]>>,
    pub motion: Vec<bool>,
}

pub struct Merged {
    pub rgb: Vec<[f32; 3]>,
    pub ghost_fraction: f64,
}

pub trait Backend {
    const NAME: &'static str;
    /// Native production detector/matcher + homography RANSAC; return a->b, integer centres.
    fn register(pair: &Pair) -> Registration;
    /// Native merger, aligned frames. Inputs are linear; rendered backend encodes sRGB8.
    fn merge(bracket: &Bracket, deghost: bool) -> Result<Merged>;
}

pub fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn line(reader: &mut impl BufRead) -> Result<String> {
    let mut bytes = Vec::new();
    reader.take(4097).read_until(b'\n', &mut bytes)?;
    if bytes.len() > 4096 || bytes.last() != Some(&b'\n') {
        return Err("truncated/oversized HDR header".into());
    }
    Ok(String::from_utf8(bytes)?.trim().to_owned())
}

fn decode(bytes: &[u8]) -> Result<Image> {
    let mut r = Cursor::new(bytes);
    if line(&mut r)? != "#?RADIANCE" {
        return Err("expected Radiance HDR".into());
    }
    let mut format = false;
    let mut ended = false;
    for _ in 0..64 {
        let s = line(&mut r)?;
        if s.is_empty() {
            ended = true;
            break;
        }
        format |= s == "FORMAT=32-bit_rle_rgbe";
        // The fixed samples have unit exposure/correction; reject rather than ignore other units.
        if s.starts_with("EXPOSURE=") || s.starts_with("COLORCORR=") {
            return Err("HDR exposure/color correction headers are not supported by this fixture reader".into());
        }
    }
    if !format || !ended {
        return Err("missing HDR format/header terminator".into());
    }
    let size = line(&mut r)?;
    let fields: Vec<_> = size.split_whitespace().collect();
    if fields.len() != 4 || fields[0] != "-Y" || fields[2] != "+X" {
        return Err("fixture reader requires -Y height +X width".into());
    }
    let (w, h) = (fields[3].parse::<usize>()?, fields[1].parse::<usize>()?);
    if !(8..=2048).contains(&w) || !(8..=1024).contains(&h) || w * h > 1_048_576 {
        return Err("HDR sample must be at most 1 MP (2048x1024 bounds)".into());
    }
    let mut rgb = Vec::with_capacity(w * h);
    let mut row = vec![[0u8; 4]; w];
    for _ in 0..h {
        let mut header = [0; 4];
        r.read_exact(&mut header)?;
        if header != [2, 2, (w >> 8) as u8, w as u8] {
            return Err("fixture reader requires modern RGBE scanline RLE".into());
        }
        for c in 0..4 {
            let mut x = 0;
            while x < w {
                let mut count = [0];
                r.read_exact(&mut count)?;
                let count = count[0] as usize;
                let n = if count > 128 { count - 128 } else { count };
                if n == 0 || n > w - x {
                    return Err("invalid RGBE run length".into());
                }
                if count > 128 {
                    let mut value = [0];
                    r.read_exact(&mut value)?;
                    for p in &mut row[x..x + n] {
                        p[c] = value[0];
                    }
                } else {
                    for p in &mut row[x..x + n] {
                        let mut value = [0];
                        r.read_exact(&mut value)?;
                        p[c] = value[0];
                    }
                }
                x += n;
            }
        }
        for p in &row {
            let s = if p[3] == 0 { 0.0 } else { 2f32.powi(p[3] as i32 - 136) };
            let q = [(p[0] as f32 + 0.5) * s, (p[1] as f32 + 0.5) * s, (p[2] as f32 + 0.5) * s];
            if q.iter().any(|v| !v.is_finite()) {
                return Err("non-finite HDR pixel".into());
            }
            rgb.push(q);
        }
    }
    if r.position() as usize != bytes.len() {
        return Err("trailing HDR data".into());
    }
    Ok(Image { w, h, rgb })
}

pub fn load(path: &Path) -> Result<Image> {
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("HDR file exceeds sample budget (16 MiB)".into());
    }
    decode(&bytes)
}

pub fn apply(h: &[f64; 9], p: [f64; 2]) -> [f64; 2] {
    let z = h[6] * p[0] + h[7] * p[1] + h[8];
    [(h[0] * p[0] + h[1] * p[1] + h[2]) / z, (h[3] * p[0] + h[4] * p[1] + h[5]) / z]
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn sample(v: &[f32], w: usize, h: usize, p: [f64; 2]) -> f32 {
    if p[0] < 0.0 || p[1] < 0.0 || p[0] > (w - 1) as f64 || p[1] > (h - 1) as f64 {
        return 0.0;
    }
    let (x, y) = (p[0].floor() as usize, p[1].floor() as usize);
    let (dx, dy) = ((p[0] - x as f64) as f32, (p[1] - y as f64) as f32);
    let at = |x: usize, y: usize| v[y.min(h - 1) * w + x.min(w - 1)];
    let a = at(x, y) * (1.0 - dx) + at(x + 1, y) * dx;
    let b = at(x, y + 1) * (1.0 - dx) + at(x + 1, y + 1) * dx;
    a * (1.0 - dy) + b * dy
}

fn median(mut v: Vec<f32>) -> f32 {
    v.sort_by(f32::total_cmp);
    v[v.len() / 2]
}

fn pair(image: &Image, dx: f64, dy: f64, angle: f64, scale: f64, gain: f32) -> Pair {
    let k = 0.18 / median(image.rgb.iter().map(|q| q[1]).collect()).max(1e-6);
    let a: Vec<f32> = image.rgb.iter().map(|q| ((0.25 * q[0] + 0.6 * q[1] + 0.15 * q[2]) * k).clamp(0.0, 1.0).powf(1.0 / 2.2)).collect();
    let (s, c) = angle.to_radians().sin_cos();
    let (s, c) = (s * scale, c * scale);
    let (cx, cy) = ((image.w - 1) as f64 * 0.5, (image.h - 1) as f64 * 0.5);
    let (tx, ty) = (cx + dx - c * cx + s * cy, cy + dy - s * cx - c * cy);
    let truth = [c, -s, tx, s, c, ty, 0.0, 0.0, 1.0];
    let det = c * c + s * s;
    let inv = [c / det, s / det, -(c * tx + s * ty) / det, -s / det, c / det, (s * tx - c * ty) / det, 0.0, 0.0, 1.0];
    let warped = |i: usize| sample(&a, image.w, image.h, apply(&inv, [(i % image.w) as f64, (i / image.w) as f64]));
    let b = (0..image.w * image.h).map(|i| (warped(i) * gain).min(1.0)).collect();
    Pair { w: image.w, h: image.h, a, b, truth }
}

struct MatchQuality {
    good: usize,
    cells: usize,
    residual: Option<f64>,
    grid: Option<f64>,
}

fn match_quality(pair: &Pair, r: &Registration) -> MatchQuality {
    let mut cells = [false; 16];
    let good = r
        .matches
        .iter()
        .filter(|(a, b)| {
            let yes = distance(apply(&pair.truth, *a), *b) <= THRESHOLD;
            if yes {
                let x = ((a[0].max(0.0) / pair.w as f64 * 4.0) as usize).min(3);
                let y = ((a[1].max(0.0) / pair.h as f64 * 4.0) as usize).min(3);
                cells[y * 4 + x] = true;
            }
            yes
        })
        .count();
    let squared = |h: &[f64; 9], i: usize| distance(apply(h, r.matches[i].0), r.matches[i].1).powi(2);
    let mean = |h: &[f64; 9]| r.inliers.iter().map(|&i| squared(h, i)).sum::<f64>() / r.inliers.len() as f64;
    let residual = r.fit.and_then(|h| (!r.inliers.is_empty()).then(|| mean(&h).sqrt()));
    let grid = r.fit.and_then(|h| {
        let mut errors = Vec::new();
        for y in 1..=5 {
            for x in 1..=5 {
                let p = [x as f64 * (pair.w - 1) as f64 / 6.0, y as f64 * (pair.h - 1) as f64 / 6.0];
                let q = apply(&pair.truth, p);
                if q[0] >= 0.0 && q[1] >= 0.0 && q[0] < pair.w as f64 && q[1] < pair.h as f64 {
                    errors.push(distance(apply(&h, p), q).powi(2));
                }
            }
        }
        (!errors.is_empty()).then(|| (errors.iter().sum::<f64>() / errors.len() as f64).sqrt())
    });
    MatchQuality { good, cells: cells.iter().filter(|v| **v).count(), residual, grid }
}

fn noise(i: usize, frame: usize, c: usize) -> f32 {
    // Independent fixed uniform noise with unit variance; not a calibrated camera model.
    let mut z = (i as u64).wrapping_add((frame as u64 + 1) << 32).wrapping_add((c as u64) << 48).wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    (((z ^ (z >> 31)) >> 40) as f32 / 16_777_216.0 - 0.5) * 12f32.sqrt()
}

pub fn bracket(image: &Image, moving: bool) -> Bracket {
    let (w, h) = (256.min(image.w), 128.min(image.h));
    let mut rgb: Vec<[f32; 3]> = (0..w * h).map(|i| image.rgb[(i / w * image.h / h) * image.w + i % w * image.w / w]).collect();
    let k = 0.18 / median(rgb.iter().map(|q| q[1]).collect()).max(1e-6);
    // Keep all highlights recoverable by the shortest exposure.
    let peak = rgb.iter().flatten().copied().fold(0.0f32, f32::max);
    let k = k.min(0.95 / (peak.max(1e-6) * EXPOSURES[0]));
    for q in &mut rgb {
        *q = q.map(|v| v * k);
    }
    let mut motion = vec![false; w * h];
    let frames = EXPOSURES
        .iter()
        .enumerate()
        .map(|(f, &e)| {
            rgb.iter()
                .enumerate()
                .map(|(i, q)| {
                    let (x, y) = (i % w, i / w);
                    let object = moving && f != 1 && x >= w / 3 + f * 16 && x < w / 3 + f * 16 + 20 && (h / 3..h / 3 + 24).contains(&y);
                    motion[i] |= object;
                    let q = if object { [0.7, 0.15, 0.04] } else { *q };
                    std::array::from_fn(|c| {
                        let v = q[c] * e;
                        (v + noise(i, f, c) * (v * 2e-5 + 1e-7).sqrt()).clamp(0.0, 1.0)
                    })
                })
                .collect()
        })
        .collect();
    Bracket { truth: Image { w, h, rgb }, frames, motion }
}

fn rms(v: &[f64]) -> Option<f64> {
    (!v.is_empty()).then(|| (v.iter().map(|v| v * v).sum::<f64>() / v.len() as f64).sqrt())
}
fn number(v: Option<f64>) -> String {
    v.filter(|v| v.is_finite()).map_or_else(|| "NA".into(), |v| format!("{v:.6}"))
}
fn log_error(actual: f32, truth: f32) -> f64 {
    (actual.max(1e-6) as f64 / truth.max(1e-6) as f64).log2()
}
fn dynamic_range(rgb: &[[f32; 3]]) -> f64 {
    let mut v: Vec<_> = rgb.iter().map(|q| q[1].max(1e-6)).collect();
    v.sort_by(f32::total_cmp);
    (v[v.len() * 999 / 1000] as f64 / v[v.len() / 100] as f64).log2()
}

fn hdr_row<B: Backend>(name: &str, b: &Bracket, deghost: bool) -> Result<()> {
    let t = Instant::now();
    let out = B::merge(b, deghost)?;
    let ms = elapsed_ms(t);
    if out.rgb.len() != b.truth.rgb.len() || out.rgb.iter().flatten().any(|v| !v.is_finite() || *v < 0.0) {
        return Err("backend returned malformed/non-finite HDR result".into());
    }
    let mut all = Vec::new();
    let mut shadow = Vec::new();
    let mut highlight = Vec::new();
    let mut edge = Vec::new();
    let mut motion = Vec::new();
    for (i, (a, q)) in out.rgb.iter().zip(&b.truth.rgb).enumerate() {
        let err = log_error(a[1], q[1]);
        if b.motion[i] {
            motion.push(err);
            continue;
        }
        all.push(err);
        if q[1] < 0.02 {
            shadow.push((a[1] as f64 - q[1] as f64) / q[1].max(1e-6) as f64);
        }
        if q[1] > 0.8 {
            highlight.push(err);
        }
        if i % b.truth.w + 1 < b.truth.w && log_error(b.truth.rgb[i + 1][1], q[1]).abs() > 0.5 {
            edge.push(err);
        }
    }
    let input: Vec<_> = b.frames[1].iter().zip(&b.truth.rgb).map(|(a, q)| log_error(a[1], q[1])).collect();
    println!(
        "hdr,{},{name},{},{},{},{},{:.6},{:.6},{},{},{},{},{},{},{},{},{},{ms:.3}",
        B::NAME,
        b.truth.w,
        b.truth.h,
        if b.motion.iter().any(|v| *v) { "motion" } else { "static" },
        deghost,
        dynamic_range(&b.truth.rgb),
        dynamic_range(&out.rgb),
        number(rms(&all)),
        number(rms(&shadow)),
        shadow.len(),
        number(rms(&highlight)),
        highlight.len(),
        number(rms(&edge)),
        number(rms(&motion)),
        number(rms(&input)),
        out.ghost_fraction
    );
    Ok(())
}

pub fn run<B: Backend>(paths: &[String]) -> Result<()> {
    if paths.is_empty() || paths.len() > 4 {
        return Err("pass 1..4 CC0 .hdr paths (e.g. target/hdr/lebombo_1k.hdr target/hdr/kiara_1_dawn_1k.hdr)".into());
    }
    println!(
        "registration,backend,source,case,w,h,features_a,features_b,matches,true_matches,precision,inliers,cells_4x4,residual_rms_px,truth_grid_rms_px,success,detect_ms,match_ms,fit_ms"
    );
    println!(
        "hdr,backend,source,w,h,case,deghost,truth_dr_stops,output_dr_stops,log_rmse_stops,shadow_relative_rmse,shadow_n,highlight_log_rmse_stops,highlight_n,edge_log_rmse_stops,motion_log_rmse_stops,middle_log_rmse_stops,ghost_fraction,merge_ms"
    );
    for path in paths {
        let p = Path::new(path);
        let name = p.file_name().and_then(|n| n.to_str()).ok_or("invalid filename")?;
        if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) {
            return Err("source filename must be CSV-safe ASCII".into());
        }
        let image = load(p)?;
        for (case, dx, dy, angle, scale, gain) in [
            ("subpixel", 7.25, -4.5, 0.0, 1.0, 1.0),
            ("rotation", 3.25, -2.5, 2.0, 1.0, 1.0),
            ("scale", -3.5, 2.25, -1.0, 1.08, 1.0),
            ("overlap_gain", image.w as f64 * 0.18, 0.0, 0.0, 1.0, 1.4),
        ] {
            let pair = pair(&image, dx, dy, angle, scale, gain);
            let r = B::register(&pair);
            let q = match_quality(&pair, &r);
            let precision = if r.matches.is_empty() { 0.0 } else { q.good as f64 / r.matches.len() as f64 };
            let success = q.good >= 10 && q.grid.is_some_and(|v| v.is_finite() && v <= 1.0);
            println!(
                "registration,{},{name},{case},{},{},{},{},{},{},{precision:.6},{},{},{},{},{success},{:.3},{:.3},{:.3}",
                B::NAME,
                pair.w,
                pair.h,
                r.features[0],
                r.features[1],
                r.matches.len(),
                q.good,
                r.inliers.len(),
                q.cells,
                number(q.residual),
                number(q.grid),
                r.detect_ms,
                r.match_ms,
                r.fit_ms
            );
        }
        let static_bracket = bracket(&image, false);
        hdr_row::<B>(name, &static_bracket, false)?;
        let moving = bracket(&image, true);
        hdr_row::<B>(name, &moving, false)?;
        hdr_row::<B>(name, &moving, true)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rgbe() -> Vec<u8> {
        let mut v = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 8 +X 8\n".to_vec();
        for _ in 0..8 {
            v.extend([2, 2, 0, 8, 136, 128, 136, 64, 136, 32, 136, 129]);
        }
        v
    }
    #[test]
    fn rgbe_bounds_and_radiance() -> Result<()> {
        let v = rgbe();
        let p = decode(&v)?;
        assert_eq!((p.w, p.h), (8, 8));
        assert!(p.rgb.iter().all(|q| *q == [1.00390625, 0.50390625, 0.25390625]));
        assert!(decode(&v[..v.len() - 1]).is_err());
        let mut zero = v.clone();
        let header_len = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 8 +X 8\n".len();
        zero[header_len + 4] = 0;
        assert!(decode(&zero).is_err());
        let huge = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 999999 +X 999999\n";
        assert!(decode(huge).is_err());
        Ok(())
    }
    #[test]
    fn wrong_consensus_cannot_pass() {
        let p = Pair { w: 100, h: 100, a: vec![], b: vec![], truth: [1.0, 0.0, 7.25, 0.0, 1.0, -4.5, 0.0, 0.0, 1.0] };
        let matches = vec![([30.0, 30.0], [30.0, 30.0]), ([70.0, 70.0], [70.0, 70.0])];
        let r = Registration { matches, fit: Some([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]), inliers: vec![0, 1], ..Default::default() };
        let q = match_quality(&p, &r);
        assert_eq!(q.good, 0);
        assert_eq!(q.residual, Some(0.0));
        assert!(q.grid.is_some_and(|v| v > 8.0));
        assert!(match_quality(&p, &Registration::default()).grid.is_none());
    }
    #[test]
    fn subpixel_warp_and_hdr_reference() {
        let image = Image { w: 128, h: 64, rgb: (0..128 * 64).map(|i| [(i % 128) as f32 / 128.0 + 0.01; 3]).collect() };
        let p = pair(&image, 7.25, -4.5, 0.0, 1.0, 1.0);
        let q = apply(&p.truth, [40.0, 20.0]);
        assert_eq!(q, [47.25, 15.5]);
        assert!((sample(&p.b, p.w, p.h, q) - p.a[20 * p.w + 40]).abs() < 0.001);
        let b = bracket(&image, true);
        assert_eq!(b.frames.len(), 3);
        assert_eq!(b.motion.iter().filter(|v| **v).count(), 960);
        assert!(b.frames.iter().flatten().flatten().all(|v| (0.0..=1.0).contains(v)));
        assert_eq!(noise(17, 2, 1), noise(17, 2, 1));
        assert_ne!(noise(17, 2, 1), noise(17, 1, 1));
        assert_eq!(log_error(0.5, 0.25), 1.0);
    }
}
