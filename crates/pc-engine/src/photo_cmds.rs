//! Computational-photography automation: File › Automate › Photomerge, Merge to HDR Pro and
//! Crop and Straighten Photos. The registration and blending code
//! ([`photocraft_algo::panorama`]) is shared with Edit › Auto-Align / Auto-Blend Layers
//! (`align_cmds`).
//!
//! Inputs are files (`"paths"`: an array or a folder) or, with `"useOpenDocuments": true`, the
//! open documents (Photoshop's *Add Open Files*). Each command creates new documents; the
//! sources are untouched.

use lightcraft_merge::editor::hdr::{MergeOptions, estimate_exposures, merge_with_progress, mtb_offset};
use lightcraft_merge::editor::panorama::{AlignOptions, Layout, PreparedImage, Projection, RoiImage, blend_levels, detect_mops_many, multiband, seam_labels};
use photocraft_algo::exif;
use photocraft_algo::hdr::{self, ToneMethod};
use photocraft_algo::panorama::{self, Alignment, Placement};
use photocraft_algo::scancrop;
use photocraft_algo::tone::HdrToning;
use photocraft_algo::transform::{Homography, Interp, warp_surface};
use photocraft_algo::warp::warp_mesh_surface;
use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_doc::{Document, Layer, LayerContent, LayerMask, Size};
use photocraft_geom::Rect;
use photocraft_raster::{Surface, to_rgba};
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::file_cmds::{file_name, flattened, import, list_images, read_file};
use crate::{EngineError, Result, Session};

/// Wall-clock timer for the `ms` fields of results (always 0 on the web, where
/// `std::time::Instant` is unavailable).
pub(crate) struct Stopwatch(#[cfg(not(target_arch = "wasm32"))] std::time::Instant);

impl Stopwatch {
    pub(crate) fn start() -> Self {
        Stopwatch(
            #[cfg(not(target_arch = "wasm32"))]
            std::time::Instant::now(),
        )
    }
    pub(crate) fn ms(&self) -> f64 {
        #[cfg(not(target_arch = "wasm32"))]
        return self.0.elapsed().as_secs_f64() * 1000.0;
        #[cfg(target_arch = "wasm32")]
        0.0
    }
}

/// Longest side used for feature registration.
pub(crate) const REGISTER_SIDE: usize = 1200;

fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.into(), msg: msg.into() }
}

// Sources

/// One input image, flattened.
pub(crate) struct Source {
    pub name: String,
    pub surf: Surface,
    pub w: usize,
    pub h: usize,
    pub exif: exif::CameraInfo,
}

/// Loaded sources, the common pixel format and the first source's ICC profile.
type Loaded = (Vec<Source>, PixelFormat, Option<std::sync::Arc<Vec<u8>>>);

/// Sources flattened one at a time into the first one's format, within a memory budget.
struct Sources {
    out: Vec<Source>,
    fmt: Option<PixelFormat>,
    icc: Option<std::sync::Arc<Vec<u8>>>,
    /// `None` (Merge to HDR Pro) loads without a limit.
    budget: Option<u64>,
    /// Bytes of the flattened sources kept so far.
    retained: u64,
}

impl Sources {
    /// Flattens `d`, refusing it before the flatten allocates when it would pass the budget.
    /// `encoded` is the size of the file `d` was decoded from (0 for an open document).
    fn add(&mut self, name: String, d: &Document, encoded: usize, cmd: &str) -> Result<()> {
        let source_fmt = d.pixel_format();
        let first = self.fmt.is_none();
        let fmt = *self.fmt.get_or_insert_with(|| {
            if matches!(source_fmt.mode, ColorMode::Cmyk | ColorMode::Lab | ColorMode::Multichannel) {
                // Registration and blending work in RGB; like Photoshop, the result is RGB.
                PixelFormat::new(ColorMode::Rgb, source_fmt.sample, true)
            } else {
                source_fmt
            }
        });
        if first && source_fmt.mode == fmt.mode {
            self.icc = d.icc_profile.clone();
        }
        let (w, h) = (d.size.width as usize, d.size.height as usize);
        let surface = (w as u64).saturating_mul(h as u64).saturating_mul(fmt.bytes_per_pixel() as u64);
        if let Some(budget) = self.budget {
            // The kept surface and the flatten's working copy of it, while the file's bytes are held.
            let needed = self.retained.saturating_add(encoded as u64).saturating_add(surface.saturating_mul(2));
            if needed > budget {
                return Err(crate::allocation::error(
                    cmd,
                    &format!(
                        "loading {} source image(s) plus another {w}×{h} one needs about {} MiB, which exceeds the {} MiB budget; use fewer or smaller images, or raise \"memoryBudget\"",
                        self.out.len(),
                        needed >> 20,
                        budget >> 20
                    ),
                ));
            }
        }
        self.retained = self.retained.saturating_add(surface);
        let info = d.metadata.exif.as_ref().map(|e| exif::read(e)).unwrap_or_default();
        self.out.push(Source { name, surf: flattened(d, fmt)?, w, h, exif: info });
        Ok(())
    }
}

/// Flattens the open documents and files named by `p` into common-format sources. With a
/// `budget` (bytes) each source is admitted before it is flattened, so an oversized set is refused
/// while the earlier sources are all that is held.
fn load_sources(s: &Session, p: &Value, cmd: &str, budget: Option<u64>) -> Result<Loaded> {
    let mut src = Sources { out: Vec::new(), fmt: None, icc: None, budget, retained: 0 };
    // "Use open documents" merges the open documents; the dialog still sends its prefilled folder
    // (a string), which is not scanned then. An explicit file list still adds to them.
    let open = p.get("useOpenDocuments").and_then(Value::as_bool).unwrap_or(false);
    if open {
        for d in s.documents() {
            src.add(d.doc.name.clone(), &d.doc, 0, cmd)?;
        }
    }
    let paths: Vec<String> = match p.get("paths").or_else(|| p.get("input")).filter(|v| !(open && v.is_string())) {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        Some(Value::String(dir)) => list_images(dir)?,
        _ => Vec::new(),
    };
    for path in &paths {
        let bytes = read_file(path)?;
        let name = file_name(path);
        let doc = import(&name, &bytes)?;
        src.add(name, &doc, bytes.len(), cmd)?;
    }
    let Some(fmt) = src.fmt else {
        return Err(bad(cmd, "pass \"paths\" (files or a folder) or \"useOpenDocuments\": true"));
    };
    Ok((src.out, fmt, src.icc))
}

/// Photomerge's working-memory budget in bytes: the `memoryBudget` param, else what heavy work may
/// still plan for on this machine, at least 1 GiB.
fn photomerge_memory_budget(p: &Value) -> u64 {
    p.get("memoryBudget").and_then(Value::as_u64).unwrap_or_else(|| lightcraft_sysmem::heavy_jobs().headroom().max(1 << 30))
}

/// Estimated peak working memory of a Photomerge in bytes: the largest of its phases. Warping
/// holds the sources and the warped images (`warped`: their sizes). A blend keeps the warped
/// images and its ROIs throughout; on top of them come the accumulator pyramids with every image's
/// bands (`multiband` builds them all before it adds any), then the blended planes, the composite
/// and the output layers, and last the fill.
fn estimate_photomerge_memory(sources: &[(usize, usize)], warped: &[(usize, usize)], canvas: (usize, usize), fmt: PixelFormat, blend: bool, fill: bool) -> u64 {
    const F32: u64 = 4;
    let px_sum = |sizes: &[(usize, usize)]| sizes.iter().fold(0u64, |a, &(w, h)| a.saturating_add((w as u64).saturating_mul(h as u64)));
    let (cc, nch, bpp) = (fmt.mode.color_channels() as u64, fmt.channels() as u64, fmt.bytes_per_pixel() as u64);
    let roi_px = px_sum(warped);
    let warped_bytes = roi_px.saturating_mul(bpp);
    let warp = px_sum(sources).saturating_mul(bpp).saturating_add(warped_bytes);
    if !blend {
        return warp;
    }
    let canvas_px = (canvas.0 as u64).saturating_mul(canvas.1 as u64);
    // ROI colours and coverage, their canvas-relative copy, and the seam weights.
    let held = warped_bytes.saturating_add(roi_px.saturating_mul((2 * cc + 3) * F32));
    // The accumulator and weight-sum pyramids: 4/3 of a plane each.
    let accumulators = canvas_px.saturating_mul((cc + 1) * F32 * 4 / 3);
    // Each image on its ROI grown by the blend margin: its Laplacian and weight pyramids plus the
    // planes they are built from, about three colour-and-weight planes in all.
    let margin = 8usize << (blend_levels(canvas.0, canvas.1).min(7) - 1);
    let band_px = warped.iter().fold(0u64, |a, &(w, h)| {
        a.saturating_add((w.saturating_add(margin).min(canvas.0) as u64).saturating_mul(h.saturating_add(margin).min(canvas.1) as u64))
    });
    let bands = band_px.saturating_mul((cc + 1) * F32 * 3);
    let composite = canvas_px.saturating_mul(bpp);
    let layers = roi_px.saturating_mul(bpp + 1);
    // The blended colour and coverage, the composite's staging buffer and surface, the layers.
    let output = canvas_px.saturating_mul((cc + 1 + nch) * F32).saturating_add(composite).saturating_add(layers);
    let filling = if fill { warped_bytes.saturating_add(layers).saturating_add(composite.saturating_mul(2)).saturating_add(24 << 20) } else { 0 };
    warp.max(held.saturating_add(accumulators).saturating_add(bands)).max(held.saturating_add(output)).max(filling)
}

/// Luminance and coverage of `surf` over `area`, box-downsampled by `k`.
pub(crate) fn luma_alpha(surf: &Surface, area: Rect, k: usize) -> (usize, usize, Vec<f32>, Vec<f32>) {
    let fmt = surf.format();
    let n = fmt.channels();
    let (w, h) = (area.width() as usize, area.height() as usize);
    let (sw, sh) = (w.div_ceil(k).max(1), h.div_ceil(k).max(1));
    let (mut l, mut a, mut cnt) = (vec![0.0f32; sw * sh], vec![0.0f32; sw * sh], vec![0.0f32; sw * sh]);
    // Row by row keeps memory small for large images.
    for y in 0..h {
        let row = surf.read_region(Rect::new(area.x0, area.y0 + y as i32, area.x1, area.y0 + y as i32 + 1));
        for x in 0..w {
            let p = to_rgba(&fmt, &row[x * n..(x + 1) * n]);
            let i = (y / k) * sw + x / k;
            l[i] += (li_color::luminance_709([p[0], p[1], p[2]])) * p[3] + (1.0 - p[3]) * 0.0;
            a[i] += p[3];
            cnt[i] += 1.0;
        }
    }
    for i in 0..sw * sh {
        let c = cnt[i].max(1.0);
        l[i] = if a[i] > 0.0 { l[i] / a[i] } else { 0.0 };
        a[i] /= c;
    }
    (sw, sh, l, a)
}

/// Registers `images` (surface, content area) and returns full-resolution placements.
/// `focal35` is the 35 mm-equivalent focal length when known (EXIF).
pub(crate) fn register(images: &[(&Surface, Rect)], layout: Layout, reference: Option<usize>, geometric: bool, focal35: Option<f64>) -> Option<Alignment> {
    let side = images.iter().map(|(_, r)| r.width().max(r.height()) as usize).max().unwrap_or(1);
    let k = side.div_ceil(REGISTER_SIDE).max(1);
    let prepared: Vec<PreparedImage> = images
        .iter()
        .map(|(s, r)| {
            let (w, h, l, a) = luma_alpha(s, *r, k);
            let valid = a.iter().any(|v| *v < 0.5).then(|| a.iter().map(|v| *v > 0.5).collect());
            (w, h, l, valid)
        })
        .collect();
    let feats = detect_mops_many(&prepared, 800);
    let matches = panorama::match_mops_pairs(&feats);
    let focal = focal35.map(|f| f / 36.0 * side as f64 / k as f64);
    let a = panorama::align_mops(&feats, &matches, &AlignOptions { layout, focal, reference, geometric })?;
    let kf = k as f64;
    Some(Alignment { placements: a.placements.iter().map(|p| p.as_ref().map(|p| p.scaled(kf))).collect(), focal: a.focal * kf, rms: a.rms * kf, ..a })
}

/// The 35 mm focal length agreed by the sources' EXIF (median), if any.
fn exif_focal(srcs: &[Source]) -> Option<f64> {
    let mut f: Vec<f64> = srcs.iter().filter_map(|s| s.exif.focal_length_35mm).collect();
    f.sort_by(f64::total_cmp);
    f.get(f.len() / 2).copied()
}

/// Warps the content of `surf` inside `rect` by a placement shifted by `offset`; image
/// coordinates are surface coordinates minus `origin`.
pub(crate) fn warp_placed(surf: &Surface, rect: Rect, origin: (f64, f64), pl: &Placement, offset: (f64, f64), interp: Interp) -> Surface {
    let pl = pl.translated(offset.0, offset.1);
    if let Some(h) = plane_matrix(&pl, origin) {
        return warp_surface(surf, rect, &h, interp);
    }
    warp_mesh_surface(surf, rect, &|x, y| pl.forward(x - origin.0, y - origin.1), interp)
}

/// The projective matrix of a planar, distortion-free placement (surface px → panorama px).
pub(crate) fn plane_matrix(pl: &Placement, origin: (f64, f64)) -> Option<Homography> {
    (pl.projection == Projection::Plane && pl.k1 == 0.0).then(|| {
        let t = Homography([1.0, 0.0, -pl.center[0] - origin.0, 0.0, 1.0, -pl.center[1] - origin.1, 0.0, 0.0, 1.0]);
        pl.h.mul(&t)
    })
}

/// Splits a surface region into colour channels and coverage.
fn roi_of(surf: &Surface, area: Rect) -> RoiImage {
    let fmt = surf.format();
    let n = fmt.channels();
    let cc = fmt.mode.color_channels();
    let raw = surf.read_region(area);
    let (w, h) = (area.width() as usize, area.height() as usize);
    let mut px = Vec::with_capacity(w * h * cc);
    let mut alpha = Vec::with_capacity(w * h);
    for q in raw.chunks_exact(n) {
        px.extend_from_slice(&q[..cc]);
        alpha.push(if fmt.alpha { q[n - 1] } else { 1.0 });
    }
    RoiImage { x0: area.x0, y0: area.y0, w, h, ch: cc, px, alpha }
}

/// Placements, source sizes and the panorama offset (for vignette removal).
pub(crate) type BlendGeometry<'a> = (&'a [Placement], &'a [(usize, usize)], (f64, f64));

/// Result of [`seam_blend`]: per input, the pixels to keep (blended inside its seam region)
/// and its mask; plus the blended composite.
pub(crate) struct Blended {
    pub layers: Vec<(Surface, Surface)>,
    pub composite: Surface,
    pub gains: Vec<Vec<f32>>,
    pub vignette: [f64; 2],
}

/// Gain compensation (+ optional vignetting), seams in `order`, multi-band blending. Inputs are
/// surfaces already in panorama coordinates with alpha. `geometry` enables vignette removal.
pub(crate) fn seam_blend(warped: &[Surface], canvas: Rect, order: &[usize], seamless: bool, gains: bool, geometry: Option<BlendGeometry>) -> Blended {
    let n = warped.len();
    let fmt = warped.first().map_or(PixelFormat::RGBA8, Surface::format);
    let mut rois: Vec<RoiImage> = warped
        .iter()
        .map(|s| {
            let b = s.content_bounds().intersect(&canvas);
            if b.is_empty() { RoiImage { x0: 0, y0: 0, w: 0, h: 0, ch: fmt.mode.color_channels(), px: Vec::new(), alpha: Vec::new() } } else { roi_of(s, b) }
        })
        .collect();
    // Photometric model in panorama coordinates relative to the canvas origin used by placements.
    let shifted: Option<Vec<Placement>> = geometry.map(|(pl, _, off)| pl.iter().map(|p| p.translated(off.0, off.1)).collect());
    let geo = match (&shifted, geometry) {
        (Some(pl), Some((_, sizes, _))) => Some((pl.as_slice(), sizes)),
        _ => None,
    };
    let photo = panorama::photometric(&rois, geo, seamless && gains, geo.is_some());
    let vig = photo.vignette != [0.0, 0.0];
    for (i, roi) in rois.iter_mut().enumerate() {
        let ch = roi.ch;
        if photo.gains[i].iter().all(|g| (*g - 1.0).abs() < 1e-6) && !vig {
            continue;
        }
        for y in 0..roi.h {
            for x in 0..roi.w {
                let r2 = if vig && let Some((pl, sizes)) = geo {
                    pl[i].inverse((roi.x0 + x as i32) as f64 + 0.5, (roi.y0 + y as i32) as f64 + 0.5).map_or(0.0, |(sx, sy)| {
                        let (w, h) = (sizes[i].0 as f64, sizes[i].1 as f64);
                        let (dx, dy) = (sx - w / 2.0, sy - h / 2.0);
                        (dx * dx + dy * dy) / (w * w / 4.0 + h * h / 4.0)
                    })
                } else {
                    0.0
                };
                let k = y * roi.w + x;
                for c in 0..ch {
                    let f = photo.factor(i, c, r2);
                    let v = &mut roi.px[k * ch + c];
                    *v = if fmt.mode == ColorMode::Cmyk { 1.0 - (1.0 - *v) * f } else { *v * f };
                    if fmt.sample != SampleType::F32 {
                        *v = v.clamp(0.0, 1.0);
                    }
                }
            }
        }
    }
    // Seams on a ≤ 1000 px grid.
    let (cw, chh) = (canvas.width().max(1) as usize, canvas.height().max(1) as usize);
    let s = cw.max(chh).div_ceil(1000).max(1);
    let (gw, gh) = (cw.div_ceil(s), chh.div_ceil(s));
    let mut luma = vec![vec![0.0f32; gw * gh]; n];
    let mut cover = vec![vec![false; gw * gh]; n];
    for (i, roi) in rois.iter().enumerate() {
        let mut acc = vec![(0.0f32, 0.0f32, 0u32); gw * gh];
        for y in 0..roi.h {
            for x in 0..roi.w {
                let (u, v) = ((roi.x0 - canvas.x0) as usize + x, (roi.y0 - canvas.y0) as usize + y);
                let g = (v / s).min(gh - 1) * gw + (u / s).min(gw - 1);
                let k = y * roi.w + x;
                let l = roi.px[k * roi.ch..(k + 1) * roi.ch].iter().sum::<f32>() / roi.ch as f32;
                acc[g].0 += l;
                acc[g].1 += roi.alpha[k];
                acc[g].2 += 1;
            }
        }
        for g in 0..gw * gh {
            let (l, a, c) = acc[g];
            if c > 0 {
                luma[i][g] = l / c as f32;
                cover[i][g] = a / (s * s) as f32 > 0.98;
            }
        }
    }
    let labels = seam_labels(gw, gh, &luma, &cover, order);
    // Full-resolution weights: the seam label, falling back to the earliest covering image in
    // `order` where the label's image doesn't cover the pixel.
    let rank: Vec<usize> = {
        let mut r = vec![usize::MAX; n];
        for (k, &i) in order.iter().enumerate() {
            r[i] = k;
        }
        r
    };
    let alpha_at = |j: usize, x: i32, y: i32| -> f32 {
        let r = &rois[j];
        let (lx, ly) = (x - r.x0, y - r.y0);
        if lx < 0 || ly < 0 || lx as usize >= r.w || ly as usize >= r.h { 0.0 } else { r.alpha[ly as usize * r.w + lx as usize] }
    };
    let weights: Vec<Vec<f32>> = rois
        .iter()
        .enumerate()
        .map(|(i, roi)| {
            (0..roi.w * roi.h)
                .map(|k| {
                    let (x, y) = (roi.x0 + (k % roi.w) as i32, roi.y0 + (k / roi.w) as i32);
                    if roi.alpha[k] < 0.5 {
                        return 0.0;
                    }
                    let (u, v) = ((x - canvas.x0) as usize, (y - canvas.y0) as usize);
                    let lab = labels[(v / s).min(gh - 1) * gw + (u / s).min(gw - 1)];
                    if lab == i as i32 {
                        return 1.0;
                    }
                    if lab >= 0 && alpha_at(lab as usize, x, y) >= 0.5 {
                        return 0.0;
                    }
                    // Fallback: the earliest covering image wins.
                    let first = (0..n).filter(|&j| alpha_at(j, x, y) >= 0.5).min_by_key(|&j| rank[j]);
                    if first == Some(i) { 1.0 } else { 0.0 }
                })
                .collect()
        })
        .collect();
    // Shift ROIs to canvas-relative coordinates for the blend.
    let local: Vec<RoiImage> = rois.iter().map(|r| RoiImage { x0: r.x0 - canvas.x0, y0: r.y0 - canvas.y0, ..r.clone() }).collect();
    let levels = if seamless { blend_levels(cw, chh).min(7) } else { 1 };
    let (blend, cov) = multiband(cw, chh, &local, &weights, levels);
    let cc = fmt.mode.color_channels();
    let nch = fmt.channels();
    let mut composite = Surface::new(fmt);
    {
        let mut data = vec![0.0f32; cw * chh * nch];
        for i in 0..cw * chh {
            for c in 0..cc {
                let v = blend[i * cc + c];
                data[i * nch + c] = if fmt.sample == SampleType::F32 { v } else { v.clamp(0.0, 1.0) };
            }
            data[i * nch + nch - 1] = cov[i];
        }
        composite.write_region(canvas, &data);
        composite.prune();
    }
    let layers = rois
        .iter()
        .zip(&weights)
        .map(|(roi, wt)| {
            let area = Rect::new(roi.x0, roi.y0, roi.x0 + roi.w as i32, roi.y0 + roi.h as i32);
            let mut px = vec![0.0f32; roi.w * roi.h * nch];
            let mut mask = vec![0.0f32; roi.w * roi.h];
            for k in 0..roi.w * roi.h {
                let (u, v) = ((roi.x0 - canvas.x0) as usize + k % roi.w, (roi.y0 - canvas.y0) as usize + k / roi.w);
                let g = v * cw + u;
                let inside = wt[k] > 0.5;
                for c in 0..cc {
                    px[k * nch + c] = if inside && seamless { data_clamp(blend[g * cc + c], fmt) } else { roi.px[k * cc + c] };
                }
                px[k * nch + nch - 1] = roi.alpha[k];
                mask[k] = wt[k];
            }
            let mut ls = Surface::new(fmt);
            if !area.is_empty() {
                ls.write_region(area, &px);
            }
            ls.prune();
            let mut ms = Surface::with_default(PixelFormat::GRAY8, &[0.0]);
            if !area.is_empty() {
                ms.write_region(area, &mask);
            }
            ms.prune();
            (ls, ms)
        })
        .collect();
    Blended { layers, composite, gains: photo.gains, vignette: photo.vignette }
}

fn data_clamp(v: f32, fmt: PixelFormat) -> f32 {
    if fmt.sample == SampleType::F32 { v } else { v.clamp(0.0, 1.0) }
}

/// Seam order: the reference first, then by distance of each image's centre from it.
pub(crate) fn seam_order(warped: &[Surface], reference: usize) -> Vec<usize> {
    let centre = |s: &Surface| {
        let b = s.content_bounds();
        ((b.x0 + b.x1) as f64 / 2.0, (b.y0 + b.y1) as f64 / 2.0)
    };
    let rc = centre(&warped[reference.min(warped.len() - 1)]);
    let mut order: Vec<usize> = (0..warped.len()).collect();
    order.sort_by(|&a, &b| {
        let (ca, cb) = (centre(&warped[a]), centre(&warped[b]));
        let (da, db) = ((ca.0 - rc.0).hypot(ca.1 - rc.1), (cb.0 - rc.0).hypot(cb.1 - rc.1));
        (a != reference).cmp(&(b != reference)).then(da.total_cmp(&db))
    });
    order
}

// Photomerge

fn photomerge(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "file.automate.photomerge";
    let t0 = Stopwatch::start();
    let layout_s = p.get("layout").and_then(Value::as_str).unwrap_or("auto");
    let layout =
        Layout::parse(layout_s).ok_or_else(|| bad(cmd, format!("unknown layout `{layout_s}` (auto|perspective|cylindrical|spherical|collage|reposition)")))?;
    let blend = p.get("blend").and_then(Value::as_bool).unwrap_or(true);
    let vignette = p.get("vignetteRemoval").and_then(Value::as_bool).unwrap_or(false);
    let geometric = p.get("geometricCorrection").and_then(Value::as_bool).unwrap_or(false);
    let fill = p.get("contentAwareFill").and_then(Value::as_bool).unwrap_or(false);
    // Resolved before loading: the sources are flattened and kept one at a time.
    let budget = photomerge_memory_budget(p);
    let (mut srcs, fmt, icc) = load_sources(s, p, cmd, Some(budget))?;
    if srcs.len() < 2 {
        return Err(bad(cmd, "Photomerge needs two or more images"));
    }
    let focal_param = p.get("focalLength").and_then(Value::as_f64).filter(|f| *f > 0.0);
    let doc_name = format!("Untitled_Panorama{}", s.documents().len() + 1);
    // A background job when started with `Session::start` (#210): alignment, warping and
    // blending run on a worker, checking for cancellation between stages and images.
    crate::jobs::run(
        s,
        "Photomerge",
        false,
        move |ctx| {
            ctx.report(0.0, "Photomerge: aligning images");
            let focal35 = focal_param.or_else(|| exif_focal(&srcs));
            let images: Vec<(&Surface, Rect)> = srcs.iter().map(|s| (&s.surf, Rect::new(0, 0, s.w as i32, s.h as i32))).collect();
            let al = register(&images, layout, None, geometric, focal35)
                .ok_or_else(|| EngineError::Other("Photomerge couldn't find enough matching detail between the images".into()))?;
            let t_reg = t0.ms() / 1000.0;
            ctx.check()?;
            ctx.report(0.35, "Photomerge: warping images");
            let placed: Vec<usize> = (0..srcs.len()).filter(|&i| al.placements[i].is_some()).collect();
            // Parallel to `placed`.
            let placements: Vec<&Placement> = placed.iter().filter_map(|&i| al.placements[i].as_ref()).collect();
            let failed: Vec<String> = (0..srcs.len()).filter(|&i| al.placements[i].is_none()).map(|i| srcs[i].name.clone()).collect();
            // Canvas: union of the placed images.
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            // Each placed image's warped extent.
            let mut extents: Vec<(f64, f64)> = Vec::with_capacity(placed.len());
            for (&i, pl) in placed.iter().zip(&placements) {
                let b = pl.bounds(srcs[i].w as f64, srcs[i].h as f64);
                extents.push((b[2] - b[0], b[3] - b[1]));
                x0 = x0.min(b[0]);
                y0 = y0.min(b[1]);
                x1 = x1.max(b[2]);
                y1 = y1.max(b[3]);
            }
            let (cw, ch) = ((x1 - x0).ceil(), (y1 - y0).ceil());
            if !(cw.is_finite() && ch.is_finite()) || cw > 30_000.0 || ch > 30_000.0 || cw * ch > 6.0e8 {
                return Err(EngineError::Other(format!("the panorama would be {cw}×{ch} px; try another layout")));
            }
            let canvas = Rect::new(0, 0, cw as i32, ch as i32);
            let offset = (-x0.floor(), -y0.floor());
            // Refused here, before the warp and blend buffers exist. The estimate follows the
            // warped images (a pixel of slack for sub-pixel bounds), clipped to the canvas.
            let source_sizes: Vec<(usize, usize)> = srcs.iter().map(|s| (s.w, s.h)).collect();
            let warped_sizes: Vec<(usize, usize)> =
                extents.iter().map(|&(w, h)| ((w.ceil() + 1.0).max(0.0).min(cw) as usize, (h.ceil() + 1.0).max(0.0).min(ch) as usize)).collect();
            let needed = estimate_photomerge_memory(&source_sizes, &warped_sizes, (cw as usize, ch as usize), fmt, blend, fill);
            if needed > budget {
                return Err(crate::allocation::error(
                    "Photomerge",
                    &format!(
                        "estimated memory of {} MiB exceeds the {} MiB budget; use fewer or smaller images, or raise \"memoryBudget\"",
                        needed >> 20,
                        budget >> 20
                    ),
                ));
            }
            // A source is freed once it is warped (an unplaced one now), as the estimate assumes.
            for (i, src) in srcs.iter_mut().enumerate() {
                if al.placements[i].is_none() {
                    src.surf = Surface::new(fmt);
                }
            }
            let mut warped: Vec<Surface> = Vec::with_capacity(placed.len());
            for (k, (&i, pl)) in placed.iter().zip(&placements).enumerate() {
                ctx.check()?;
                ctx.report(0.35 + 0.25 * k as f32 / placed.len().max(1) as f32, "");
                warped.push(warp_placed(&srcs[i].surf, Rect::new(0, 0, srcs[i].w as i32, srcs[i].h as i32), (0.0, 0.0), pl, offset, Interp::Bicubic));
                srcs[i].surf = Surface::new(fmt);
            }
            let t_warp = t0.ms() / 1000.0;
            ctx.check()?;
            ctx.report(0.6, "Photomerge: blending images");
            let mut doc = Document::new(doc_name, Size::new(cw as u32, ch as u32), fmt.mode, fmt.sample);
            doc.icc_profile = icc;
            let mut info = json!({});
            let ref_pos = placed.iter().position(|&i| i == al.reference).unwrap_or(0);
            if blend {
                let pls: Vec<Placement> = placements.iter().map(|p| (*p).clone()).collect();
                let sizes: Vec<(usize, usize)> = placed.iter().map(|&i| (srcs[i].w, srcs[i].h)).collect();
                let order = seam_order(&warped, ref_pos);
                let geo = vignette.then_some((pls.as_slice(), sizes.as_slice(), offset));
                let b = seam_blend(&warped, canvas, &order, true, true, geo);
                // `layers[0]` is the bottom layer, and the first source goes on top.
                for (k, (px, mask)) in b.layers.into_iter().enumerate().rev() {
                    let mut l = Layer::new(srcs[placed[k]].name.clone(), LayerContent::Raster(px));
                    let mut m = LayerMask::hide_all();
                    m.surface = mask;
                    l.mask = Some(m);
                    doc.layers.push(l);
                }
                info = json!({"gains": b.gains, "vignette": b.vignette});
                ctx.check()?;
                if fill {
                    ctx.report(0.85, "Photomerge: filling edges");
                    let filled = content_aware_fill(&b.composite, canvas);
                    doc.layers.push(Layer::new("Content-Aware Fill", LayerContent::Raster(filled)));
                }
            } else {
                for (k, px) in warped.into_iter().enumerate().rev() {
                    doc.layers.push(Layer::new(srcs[placed[k]].name.clone(), LayerContent::Raster(px)));
                }
            }
            let n = doc.layers.len();
            ctx.check()?;
            ctx.report(1.0, "");
            Ok((
                doc,
                json!({
                    "document": Value::Null,
                    "layout": al.layout.name(),
                    "reference": srcs[al.reference].name,
                    "focalLength35": al.focal / (srcs.iter().map(|s| s.w.max(s.h)).max().unwrap_or(1) as f64) * 36.0,
                    "geometricK1": al.k1,
                    "rms": al.rms,
                    "pairs": al.pairs,
                    "placed": placed.iter().map(|&i| srcs[i].name.clone()).collect::<Vec<_>>(),
                    "failed": failed,
                    "width": cw as u32,
                    "height": ch as u32,
                    "layers": n,
                    "photometric": info,
                    "ms": {"register": t_reg * 1000.0, "warp": (t_warp - t_reg) * 1000.0, "total": t0.ms()},
                }),
            ))
        },
        |s, (doc, mut info): (Document, Value)| {
            info["document"] = json!(s.add_document(doc, None));
            Ok(info)
        },
    )
}

/// Fills the transparent areas of a composite (inside its bounding rectangle) with
/// content-aware fill, at a reduced scale for large panoramas.
fn content_aware_fill(comp: &Surface, canvas: Rect) -> Surface {
    use photocraft_algo::content_aware::{FillOptions, fill};
    let fmt = comp.format();
    let n = fmt.channels();
    let (w, h) = (canvas.width() as usize, canvas.height() as usize);
    let k = ((w * h) as f64 / 1.5e6).sqrt().ceil().max(1.0) as usize;
    let (sw, sh) = (w.div_ceil(k), h.div_ceil(k));
    let full = comp.read_region(canvas);
    let cc = n - 1;
    let mut small = vec![0.0f32; sw * sh * cc];
    let mut hole = vec![false; sw * sh];
    let mut acc_a = vec![0.0f32; sw * sh];
    let mut cnt = vec![0.0f32; sw * sh];
    for y in 0..h {
        for x in 0..w {
            let i = (y / k) * sw + x / k;
            let px = &full[(y * w + x) * n..(y * w + x + 1) * n];
            let a = px[cc];
            for c in 0..cc {
                small[i * cc + c] += px[c] * a;
            }
            acc_a[i] += a;
            cnt[i] += 1.0;
        }
    }
    for i in 0..sw * sh {
        let a = acc_a[i];
        for c in 0..cc {
            small[i * cc + c] = if a > 0.0 { small[i * cc + c] / a } else { 0.0 };
        }
        hole[i] = a / cnt[i].max(1.0) < 0.99;
    }
    let source: Vec<bool> = hole.iter().map(|h| !h).collect();
    let filled = fill(sw, sh, cc, &small, &hole, &source, &FillOptions::default());
    let mut out = vec![0.0f32; w * h * n];
    for y in 0..h {
        for x in 0..w {
            let o = (y * w + x) * n;
            let a = full[o + cc];
            let i = (y / k) * sw + x / k;
            for c in 0..cc {
                // Bilinear upsampling of the fill under partially covered pixels.
                let fx = ((x as f32 + 0.5) / k as f32 - 0.5).clamp(0.0, (sw - 1) as f32);
                let fy = ((y as f32 + 0.5) / k as f32 - 0.5).clamp(0.0, (sh - 1) as f32);
                let (xi, yi) = (fx as usize, fy as usize);
                let (x2, y2) = ((xi + 1).min(sw - 1), (yi + 1).min(sh - 1));
                let (tx, ty) = (fx - xi as f32, fy - yi as f32);
                let at = |xx: usize, yy: usize| filled[(yy * sw + xx) * cc + c];
                let fv = (at(xi, yi) * (1.0 - tx) + at(x2, yi) * tx) * (1.0 - ty) + (at(xi, y2) * (1.0 - tx) + at(x2, y2) * tx) * ty;
                let _ = i;
                out[o + c] = full[o + c] * a + fv * (1.0 - a);
            }
            out[o + cc] = 1.0;
        }
    }
    let mut s = Surface::new(fmt);
    s.write_region(canvas, &out);
    s.prune();
    s
}

// Merge to HDR Pro

fn shift_rgba(px: &[[f32; 4]], w: usize, h: usize, dx: i32, dy: i32) -> Vec<[f32; 4]> {
    (0..w * h)
        .map(|i| {
            let (x, y) = ((i % w) as i32 - dx, (i / w) as i32 - dy);
            if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 { [0.0; 4] } else { px[y as usize * w + x as usize] }
        })
        .collect()
}

fn merge_to_hdr(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "file.automate.mergeToHdrPro";
    let t0 = Stopwatch::start();
    let (srcs, _fmt, icc) = load_sources(s, p, cmd, None)?;
    if srcs.len() < 2 {
        return Err(bad(cmd, "Merge to HDR Pro needs two or more exposures"));
    }
    let name = format!("Untitled_HDR{}", s.documents().len() + 1);
    let params = p.clone();
    crate::jobs::run(
        s,
        "Merge to HDR Pro",
        false,
        move |ctx| merge_hdr_document(srcs, icc, &params, &name, t0, ctx),
        |s, (doc, mut info)| {
            info["document"] = json!(s.add_document(doc, None));
            Ok(info)
        },
    )
}

fn merge_hdr_document(
    srcs: Vec<Source>,
    icc: Option<std::sync::Arc<Vec<u8>>>,
    p: &Value,
    name: &str,
    t0: Stopwatch,
    ctx: &lightcraft_sysmem::JobCtl,
) -> Result<(Document, Value)> {
    let cmd = "file.automate.mergeToHdrPro";
    ctx.check()?;
    ctx.report(0.0, "HDR: preparing exposures");
    let (w, h) = (srcs[0].w, srcs[0].h);
    if srcs.iter().any(|s| s.w != w || s.h != h) {
        return Err(EngineError::Other("the exposures must have the same pixel dimensions".into()));
    }
    let area = Rect::new(0, 0, w as i32, h as i32);
    let mut imgs: Vec<Vec<[f32; 4]>> = srcs
        .iter()
        .map(|s| {
            let mut v = vec![[0.0f32; 4]; w * h];
            s.surf.read_rgba_into(area, &mut v);
            v
        })
        .collect();
    // Exposures: explicit EV, EXIF, or estimated.
    let (exposures, source): (Vec<f64>, &str) = match p.get("exposures").and_then(Value::as_array) {
        Some(a) if a.len() == srcs.len() => (a.iter().map(|v| 2f64.powf(v.as_f64().unwrap_or(0.0))).collect(), "params"),
        Some(_) => return Err(bad(cmd, "`exposures` needs one EV value per image")),
        None => match srcs.iter().map(|s| s.exif.exposure_factor()).collect::<Option<Vec<f64>>>() {
            Some(e) => (e, "exif"),
            None => {
                let refs: Vec<&[[f32; 4]]> = imgs.iter().map(Vec::as_slice).collect();
                (estimate_exposures(&refs), "estimated")
            }
        },
    };
    if exposures.iter().any(|e| !(e.is_finite() && *e > 0.0)) {
        return Err(bad(cmd, "exposures must be positive"));
    }
    ctx.check()?;
    ctx.report(0.15, "HDR: aligning exposures");
    // Shared feature registration handles sub-pixel translation, rotation and scale.
    // Keep Ward MTB as a fallback for brackets without enough matching texture.
    let mut order: Vec<usize> = (0..srcs.len()).collect();
    order.sort_by(|&a, &b| exposures[a].total_cmp(&exposures[b]));
    let mid = order[order.len() / 2];
    let mut offsets = vec![(0, 0); srcs.len()];
    let mut alignment = vec![json!({"model": "identity"}); srcs.len()];
    if p.get("align").and_then(Value::as_bool).unwrap_or(true) {
        let images: Vec<(&Surface, Rect)> = srcs.iter().map(|s| (&s.surf, area)).collect();
        let registered = register(&images, Layout::Collage, Some(mid), false, None);
        ctx.check()?;
        let k = w.max(h).div_ceil(1536).max(1);
        let grey = |im: &[[f32; 4]]| -> (usize, usize, Vec<f32>) {
            let (sw, sh) = (w.div_ceil(k), h.div_ceil(k));
            let mut g = vec![0.0f32; sw * sh];
            let mut c = vec![0.0f32; sw * sh];
            for (i, q) in im.iter().enumerate() {
                let j = (i / w / k) * sw + (i % w) / k;
                g[j] += (54.0 * q[0] + 183.0 * q[1] + 19.0 * q[2]) / 256.0;
                c[j] += 1.0;
            }
            (sw, sh, g.iter().zip(&c).map(|(a, b)| a / b.max(1.0)).collect())
        };
        let (sw, sh, gref) = grey(&imgs[mid]);
        for i in 0..srcs.len() {
            if i == mid {
                continue;
            }
            ctx.check()?;
            ctx.report(0.15 + 0.2 * i as f32 / srcs.len() as f32, "HDR: aligning exposures");
            if let Some(pl) = registered.as_ref().and_then(|a| a.placements[i].as_ref()) {
                let warped = warp_placed(&srcs[i].surf, area, (0.0, 0.0), pl, (0.0, 0.0), Interp::Bicubic);
                warped.read_rgba_into(area, &mut imgs[i]);
                let centre = (w as f64 / 2.0, h as f64 / 2.0);
                let mapped = pl.forward(centre.0, centre.1);
                offsets[i] = ((mapped.0 - centre.0).round() as i32, (mapped.1 - centre.1).round() as i32);
                alignment[i] = json!({"model": "similarity", "matrix": plane_matrix(pl, (0.0, 0.0)).map(|h| h.0), "rmsPx": registered.as_ref().map(|a| a.rms)});
                continue;
            }
            let (_, _, gi) = grey(&imgs[i]);
            let (dx, dy) = mtb_offset(sw, sh, &gref, &gi, 5);
            offsets[i] = (dx * k as i32, dy * k as i32);
            alignment[i] = json!({"model": "mtb", "offset": offsets[i]});
            if offsets[i] != (0, 0) {
                imgs[i] = shift_rgba(&imgs[i], w, h, offsets[i].0, offsets[i].1);
            }
        }
    }
    let ghost_base = p.get("ghostBase").and_then(Value::as_u64).map(|v| v as usize);
    if ghost_base.is_some_and(|g| g >= srcs.len()) {
        return Err(bad(cmd, "`ghostBase` is an image index"));
    }
    ctx.check()?;
    ctx.report(0.4, "HDR: merging radiance");
    let refs: Vec<&[[f32; 4]]> = imgs.iter().map(Vec::as_slice).collect();
    let merged = merge_with_progress(
        w,
        h,
        &refs,
        &MergeOptions {
            exposures: exposures.clone(),
            remove_ghosts: p.get("removeGhosts").and_then(Value::as_bool).unwrap_or(false),
            ghost_base,
            response: None,
        },
        &|fraction, stage| {
            ctx.report(0.4 + 0.4 * fraction, stage);
            !ctx.is_cancelled()
        },
    )
    .map_err(|e| bad(cmd, e))?;
    ctx.check()?;
    ctx.report(0.8, "HDR: preparing document");
    let mode = p.get("mode").and_then(|v| v.as_str().map(str::to_string).or_else(|| v.as_u64().map(|n| n.to_string()))).unwrap_or_else(|| "32".into());
    let (depth, mut px) = match mode.as_str() {
        "32" => (SampleType::F32, merged.px.clone()),
        "16" | "8" => {
            let method = p.get("method").and_then(Value::as_str).unwrap_or("localAdaptation");
            let f = |k: &str, d: f64| p.get(k).and_then(Value::as_f64).unwrap_or(d) as f32;
            let tm = match method {
                "localAdaptation" => ToneMethod::LocalAdaptation(HdrToning {
                    radius: f("radius", 7.0).clamp(1.0, 500.0),
                    strength: f("strength", 0.52).clamp(0.1, 4.0),
                    gamma: f("gamma", 1.0).clamp(0.1, 2.0),
                    exposure: f("exposure", 0.0).clamp(-5.0, 5.0),
                    detail: f("detail", 30.0).clamp(-100.0, 300.0),
                    shadow: f("shadow", 0.0).clamp(-100.0, 100.0),
                    highlight: f("highlight", 0.0).clamp(-100.0, 100.0),
                    vibrance: f("vibrance", 0.0).clamp(-100.0, 100.0),
                    saturation: f("saturation", 20.0).clamp(-100.0, 100.0),
                    curve: Vec::new(),
                }),
                "exposureGamma" => ToneMethod::ExposureGamma { exposure: f("exposure", 0.0), gamma: f("gamma", 1.0) },
                "highlightCompression" => ToneMethod::HighlightCompression,
                "equalizeHistogram" => ToneMethod::EqualizeHistogram,
                other => return Err(bad(cmd, format!("unknown method `{other}` (localAdaptation|exposureGamma|highlightCompression|equalizeHistogram)"))),
            };
            let mut v = merged.px.clone();
            hdr::tone_map(&mut v, w, h, &tm);
            (if mode == "16" { SampleType::U16 } else { SampleType::U8 }, v)
        }
        other => return Err(bad(cmd, format!("`mode` must be 32|16|8, not {other}"))),
    };
    for q in px.iter_mut() {
        q[3] = 1.0;
    }
    let mut doc = Document::new(name, Size::new(w as u32, h as u32), ColorMode::Rgb, depth);
    doc.icc_profile = if depth == SampleType::F32 { Some(photocraft_cms::builtin::Builtin::LinearSrgb.profile().to_bytes()) } else { icc };
    let fmt = doc.pixel_format();
    let n = fmt.channels();
    let mut data = vec![0.0f32; w * h * n];
    for (q, o) in px.iter().zip(data.chunks_exact_mut(n)) {
        photocraft_raster::from_rgba_into(&fmt, *q, o);
    }
    let mut surf = Surface::new(fmt);
    surf.write_region(area, &data);
    doc.layers.push(Layer::new("Background", LayerContent::Raster(surf)));
    ctx.check()?;
    ctx.report(1.0, "HDR: done");
    let ev0 = exposures[order[0]];
    Ok((
        doc,
        json!({
            "document": Value::Null,
            "mode": mode,
            "exposureSource": source,
            "ev": exposures.iter().map(|e| (e / ev0).log2()).collect::<Vec<_>>(),
            "offsets": offsets,
            "alignment": alignment,
            "ghostBase": merged.ghost_base,
            "ghostFraction": merged.ghost_fraction,
            "stops": merged.stops,
            "ms": t0.ms(),
        }),
    ))
}

// Crop and Straighten Photos

fn crop_and_straighten(s: &mut Session, _p: &Value) -> Result<Value> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let doc = st.doc.clone();
    let fmt = doc.pixel_format();
    let flat = flattened(&doc, fmt)?;
    let area = doc.bounds();
    let (w, h) = (area.width() as usize, area.height() as usize);
    let mut rgba = vec![[0.0f32; 4]; w * h];
    flat.read_rgba_into(area, &mut rgba);
    // Transparent pixels count as background.
    for q in rgba.iter_mut() {
        if q[3] < 1.0 {
            let a = q[3];
            for v in &mut q[..3] {
                *v = *v * a + (1.0 - a);
            }
        }
    }
    let found = scancrop::find_photos(w, h, &rgba);
    if found.is_empty() {
        return Err(EngineError::Other("no photos found on a uniform background".into()));
    }
    let mut out = Vec::new();
    for (k, f) in found.iter().enumerate() {
        let (pw, ph) = (f.width.round().max(1.0) as u32, f.height.round().max(1.0) as u32);
        let corners = f.corners();
        let hq = Homography::rect_to_quad([0.0, 0.0, pw as f64, ph as f64], corners).and_then(|h| h.inverse());
        let Some(hm) = hq else { continue };
        let warped = warp_surface(&flat, area, &hm, Interp::Bicubic);
        let keep = Rect::new(0, 0, pw as i32, ph as i32);
        let mut px = Surface::new(fmt);
        px.write_region(keep, &warped.read_region(keep));
        // Opaque, like a scanned print.
        let n = fmt.channels();
        let mut data = px.read_region(keep);
        for q in data.chunks_exact_mut(n) {
            q[n - 1] = 1.0;
        }
        px.write_region(keep, &data);
        let mut nd = Document::new(format!("{} copy {}", doc.name, k + 1), Size::new(pw, ph), doc.mode, doc.depth);
        nd.icc_profile = doc.icc_profile.clone();
        nd.resolution_dpi = doc.resolution_dpi;
        let mut bg = Layer::new("Background", LayerContent::Raster(px));
        bg.locks.transparency = true;
        bg.locks.position = true;
        nd.layers.push(bg);
        let idx = s.add_document(nd, None);
        out.push(json!({"document": idx, "width": pw, "height": ph, "angle": f.angle, "center": f.center}));
    }
    Ok(json!({"photos": out}))
}

// Specs

fn has_doc(s: &Session) -> std::result::Result<(), String> {
    s.active().map(|_| ()).ok_or_else(|| "no document open".into())
}

fn always(_: &Session) -> std::result::Result<(), String> {
    Ok(())
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "file.automate.photomerge",
            label: "Photomerge…",
            menu: &["File", "Automate"],
            shortcut: None,
            params: r##"{"paths":[str]|folder,"useOpenDocuments":bool=false,"layout":"auto|perspective|cylindrical|spherical|collage|reposition","blend":bool=true,"vignetteRemoval":bool=false,"geometricCorrection":bool=false,"contentAwareFill":bool=false,"focalLength":mm=0 (35 mm equivalent; 0 = EXIF or estimated),"memoryBudget":bytes?=the heavy-job memory headroom} → new document, one masked layer per image"##,
            enabled: always,
            run: photomerge,
            journal: true,
        },
        CommandSpec {
            id: "file.automate.mergeToHdrPro",
            label: "Merge to HDR Pro…",
            menu: &["File", "Automate"],
            shortcut: None,
            params: r##"{"paths":[str]|folder,"useOpenDocuments":bool=false,"exposures":[ev]? (default EXIF, else estimated),"align":bool=true,"removeGhosts":bool=false,"ghostBase":int?,"mode":"32|16|8","method":"localAdaptation|exposureGamma|highlightCompression|equalizeHistogram","radius":1..500=7,"strength":0.1..4=0.52,"gamma":0.1..2=1,"exposure":-5..5=0,"detail":-100..300=30,"shadow":-100..100=0,"highlight":-100..100=0,"vibrance":-100..100=0,"saturation":-100..100=20} → new document (32-bit linear, or tone-mapped 16/8-bit)"##,
            enabled: always,
            run: merge_to_hdr,
            journal: true,
        },
        CommandSpec {
            id: "file.automate.cropAndStraightenPhotos",
            label: "Crop and Straighten Photos",
            menu: &["File", "Automate"],
            shortcut: None,
            params: "{} → one new document per photo found on the scan",
            enabled: has_doc,
            run: crop_and_straighten,
            journal: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A textured RGB scene (deterministic blobs on a gradient).
    fn scene(w: usize, h: usize, seed: u64) -> Vec<[f32; 4]> {
        let mut z = seed;
        let mut rnd = || {
            z = z.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (z >> 33) as f64 / (1u64 << 31) as f64
        };
        let mut px: Vec<[f32; 4]> = (0..w * h)
            .map(|i| {
                let x = (i % w) as f32 / w as f32;
                [0.3 + 0.2 * x, 0.35, 0.4 - 0.1 * x, 1.0]
            })
            .collect();
        for _ in 0..(w * h / 700) {
            let (cx, cy, r) = (rnd() * w as f64, rnd() * h as f64, 3.0 + rnd() * 9.0);
            let col = [rnd() as f32, rnd() as f32, rnd() as f32, 1.0];
            for y in (cy - r).max(0.0) as usize..((cy + r) as usize + 1).min(h) {
                for x in (cx - r).max(0.0) as usize..((cx + r) as usize + 1).min(w) {
                    if (x as f64 - cx).hypot(y as f64 - cy) < r {
                        px[y * w + x] = col;
                    }
                }
            }
        }
        px
    }

    fn doc_from(name: &str, px: &[[f32; 4]], w: usize, h: usize, depth: SampleType) -> Document {
        let mut d = Document::new(name, Size::new(w as u32, h as u32), ColorMode::Rgb, depth);
        let fmt = d.pixel_format();
        let n = fmt.channels();
        let mut data = vec![0.0f32; w * h * n];
        for (q, o) in px.iter().zip(data.chunks_exact_mut(n)) {
            photocraft_raster::from_rgba_into(&fmt, *q, o);
        }
        let mut s = Surface::new(fmt);
        s.write_region(Rect::new(0, 0, w as i32, h as i32), &data);
        d.layers.push(Layer::new("Background", LayerContent::Raster(s)));
        d
    }

    fn crop(px: &[[f32; 4]], sw: usize, x0: usize, y0: usize, w: usize, h: usize, gain: f32) -> Vec<[f32; 4]> {
        (0..w * h)
            .map(|i| {
                let q = px[(y0 + i / w) * sw + x0 + i % w];
                [q[0] * gain, q[1] * gain, q[2] * gain, 1.0]
            })
            .collect()
    }

    #[test]
    fn photomerge_open_documents_reposition_and_perspective() {
        for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
            let (sw, sh) = (520, 220);
            let sc = scene(sw, sh, 5);
            let mut s = Session::new();
            // Three overlapping crops, the middle one 15 % brighter (gain compensation).
            for (k, (x0, g)) in [(0usize, 1.0f32), (130, 1.15), (260, 1.0)].into_iter().enumerate() {
                s.add_document(doc_from(&format!("img{k}"), &crop(&sc, sw, x0, 10, 220, 200, g), 220, 200, depth), None);
            }
            let r = s.execute("file.automate.photomerge", json!({"useOpenDocuments": true, "layout": "reposition"})).unwrap();
            assert_eq!(r["placed"].as_array().unwrap().len(), 3, "{r}");
            let (w, h) = (r["width"].as_u64().unwrap(), r["height"].as_u64().unwrap());
            assert!((w as i64 - 480).abs() <= 2 && (h as i64 - 200).abs() <= 2, "{depth:?} {w}×{h}");
            let d = &s.active().unwrap().doc;
            assert_eq!(d.layers.len(), 3);
            assert!(d.layers.iter().all(|l| l.mask.is_some()));
            // Masks partition the panorama: every covered pixel is shown by exactly one layer.
            for &(x, y) in &[(20, 100), (200, 50), (380, 150), (470, 120)] {
                let shown: f32 = d.layers.iter().map(|l| l.mask.as_ref().unwrap().value(x, y) * l.surface().unwrap().rgba(x, y)[3]).sum();
                assert!((shown - 1.0).abs() < 0.01, "({x},{y}) {shown}");
            }
            // The composite matches the scene up to the global gain.
            let comp = photocraft_compose::flatten(d);
            let at = |x: i32, y: i32| comp.px[(y * comp.rect.width() as i32 + x) as usize];
            let gains = r["photometric"]["gains"].as_array().unwrap();
            assert!(gains[1][0].as_f64().unwrap() > gains[0][0].as_f64().unwrap() * 1.05, "{r}");
            let probe = at(160, 100);
            let truth = sc[110 * sw + 160];
            assert!((probe[1] - truth[1]).abs() < 0.08, "{probe:?} vs {truth:?}");
        }
        // Perspective + no blending: plain layers, no masks.
        let (sw, sh) = (520, 220);
        let sc = scene(sw, sh, 8);
        let mut s = Session::new();
        for (k, x0) in [0usize, 180].into_iter().enumerate() {
            s.add_document(doc_from(&format!("p{k}"), &crop(&sc, sw, x0, 10, 260, 200, 1.0), 260, 200, SampleType::U8), None);
        }
        let r = s.execute("file.automate.photomerge", json!({"useOpenDocuments": true, "layout": "perspective", "blend": false})).unwrap();
        assert_eq!(r["layout"], "perspective");
        let d = &s.active().unwrap().doc;
        assert!(d.layers.iter().all(|l| l.mask.is_none()));
        assert!((r["width"].as_i64().unwrap() - 440).abs() <= 8, "{r}");
        assert!(s.execute("file.automate.photomerge", json!({"useOpenDocuments": true, "layout": "zigzag"})).is_err());
        let mut empty = Session::new();
        assert!(empty.execute("file.automate.photomerge", json!({})).is_err());
    }

    /// Photomerge refuses a job past its memory budget before it allocates: at the source that
    /// would pass it while loading, or at the estimate once the canvas is known.
    #[test]
    fn photomerge_refuses_jobs_past_its_memory_budget() {
        let (sw, sh) = (520, 220);
        let sc = scene(sw, sh, 5);
        let mut s = Session::new();
        for (k, (x0, g)) in [(0usize, 1.0f32), (130, 1.15), (260, 1.0)].into_iter().enumerate() {
            s.add_document(doc_from(&format!("img{k}"), &crop(&sc, sw, x0, 10, 220, 200, g), 220, 200, SampleType::U8), None);
        }
        let p = |budget: u64| json!({"useOpenDocuments": true, "layout": "reposition", "memoryBudget": budget});
        // A 220 × 200 RGBA8 source is 176000 bytes, twice that while it is flattened.
        let err = s.execute("file.automate.photomerge", p(300_000)).unwrap_err().to_string();
        assert!(err.contains("loading 0 source image(s) plus another 220×200 one"), "{err}");
        // The three sources load within 704000 bytes; warping them needs over a megabyte.
        let err = s.execute("file.automate.photomerge", p(800_000)).unwrap_err().to_string();
        assert!(err.contains("estimated memory of"), "{err}");
        assert_eq!(s.documents().len(), 3);
        s.execute("file.automate.photomerge", p(1 << 30)).unwrap();
        assert_eq!(s.documents().len(), 4);
    }

    /// With "Use open documents" the dialog's prefilled folder is not scanned: a folder that
    /// cannot be read no longer fails the merge of the open documents.
    #[test]
    fn photomerge_open_documents_ignores_the_folder() {
        let (sw, sh) = (520, 220);
        let sc = scene(sw, sh, 5);
        for paths in [json!("/no/such/folder"), json!("")] {
            let mut s = Session::new();
            for (k, x0) in [0usize, 180].into_iter().enumerate() {
                s.add_document(doc_from(&format!("o{k}"), &crop(&sc, sw, x0, 10, 260, 200, 1.0), 260, 200, SampleType::U8), None);
            }
            let r = s.execute("file.automate.photomerge", json!({"useOpenDocuments": true, "layout": "reposition", "paths": paths})).unwrap();
            assert_eq!(r["placed"].as_array().map(Vec::len), Some(2), "{r}");
        }
    }

    /// As in Photoshop the first source is the top layer, blended or not.
    #[test]
    fn photomerge_puts_the_first_source_on_top() {
        let (sw, sh) = (520, 220);
        let sc = scene(sw, sh, 5);
        for blend in [true, false] {
            let mut s = Session::new();
            for (k, x0) in [0usize, 130, 260].into_iter().enumerate() {
                s.add_document(doc_from(&format!("img{k}"), &crop(&sc, sw, x0, 10, 220, 200, 1.0), 220, 200, SampleType::U8), None);
            }
            s.execute("file.automate.photomerge", json!({"useOpenDocuments": true, "layout": "reposition", "blend": blend})).unwrap();
            let names: Vec<&str> = s.active().unwrap().doc.layers.iter().map(|l| l.name.as_str()).collect();
            assert_eq!(names, ["img2", "img1", "img0"], "blend {blend}");
        }
    }

    #[test]
    fn photomerge_cylindrical_with_fill() {
        let (sw, sh) = (900, 260);
        let sc = scene(sw, sh, 21);
        let f = 260.0f64;
        let mut s = Session::new();
        let (tw, th) = (260usize, 200usize);
        for (k, pan) in [-0.45f64, 0.0, 0.45].into_iter().enumerate() {
            let px: Vec<[f32; 4]> = (0..tw * th)
                .map(|i| {
                    let (dx, dy) = ((i % tw) as f64 - tw as f64 / 2.0, (i / tw) as f64 - th as f64 / 2.0);
                    let (u, v) = (sw as f64 / 2.0 + f * (dx.atan2(f) + pan), sh as f64 / 2.0 + f * dy / dx.hypot(f));
                    sc[(v.round().clamp(0.0, sh as f64 - 1.0) as usize) * sw + u.round().clamp(0.0, sw as f64 - 1.0) as usize]
                })
                .collect();
            s.add_document(doc_from(&format!("c{k}"), &px, tw, th, SampleType::U8), None);
        }
        let r = s.execute("file.automate.photomerge", json!({"useOpenDocuments": true, "layout": "cylindrical", "contentAwareFill": true})).unwrap();
        assert_eq!(r["layout"], "cylindrical");
        assert_eq!(r["placed"].as_array().unwrap().len(), 3, "{r}");
        let d = &s.active().unwrap().doc;
        let top = d.layers.last().unwrap();
        assert_eq!(top.name, "Content-Aware Fill");
        // The fill layer is opaque everywhere (the bow-tie corners are filled).
        let b = d.bounds();
        assert!(top.surface().unwrap().rgba(1, 1)[3] > 0.99 && top.surface().unwrap().rgba(b.x1 - 2, b.y1 - 2)[3] > 0.99);
    }

    #[test]
    fn hdr_merge_from_synthetic_exposures() {
        let (w, h) = (96usize, 64usize);
        let radiance: Vec<[f32; 3]> = (0..w * h)
            .map(|i| {
                let x = (i % w) as f32 / w as f32;
                let b = 0.003 * 2f32.powf(x * 11.0) * (1.0 + 0.2 * ((i / w) as f32 * 0.3).sin());
                [b, b * 0.9, b * 0.7]
            })
            .collect();
        let shoot = |dt: f32| -> Vec<[f32; 4]> {
            radiance
                .iter()
                .map(|r| {
                    let f = |v: f32| photocraft_algo_srgb((v * dt).min(1.0));
                    [f(r[0]), f(r[1]), f(r[2]), 1.0]
                })
                .collect()
        };
        let mut s = Session::new();
        for (k, dt) in [0.25f32, 1.0, 4.0].into_iter().enumerate() {
            let mut d = doc_from(&format!("e{k}"), &shoot(dt), w, h, SampleType::U8);
            d.metadata.exif = Some(std::sync::Arc::new(exif::build(&exif::CameraInfo {
                exposure_time: Some(dt as f64 / 100.0),
                f_number: Some(8.0),
                iso: Some(100.0),
                ..Default::default()
            })));
            s.add_document(d, None);
        }
        let r = s.execute("file.automate.mergeToHdrPro", json!({"useOpenDocuments": true})).unwrap();
        assert_eq!(r["exposureSource"], "exif");
        let ev: Vec<f64> = r["ev"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert!((ev[1] - 2.0).abs() < 1e-6 && (ev[2] - 4.0).abs() < 1e-6, "{ev:?}");
        let d = &s.active().unwrap().doc;
        assert_eq!(d.depth, SampleType::F32);
        let l = d.layers[0].surface().unwrap();
        // Linear output: ratios across the ramp follow the radiance.
        let (a, b) = (l.rgba(20, 30)[1] as f64, l.rgba(80, 30)[1] as f64);
        let expect = radiance[30 * w + 80][1] as f64 / radiance[30 * w + 20][1] as f64;
        assert!(((b / a) / expect).ln().abs() < 0.3, "{} vs {expect}", b / a);
        assert!(r["stops"].as_f64().unwrap() > 7.0, "{r}");
        let out = r["document"].as_u64().unwrap() as usize;
        s.close(out);
        // 8-bit tone-mapped output with explicit EVs.
        for method in ["localAdaptation", "exposureGamma", "highlightCompression", "equalizeHistogram"] {
            let r = s
                .execute("file.automate.mergeToHdrPro", json!({"paths": [], "useOpenDocuments": true, "exposures": [0, 2, 4], "mode": "8", "method": method}))
                .unwrap();
            assert_eq!(r["exposureSource"], "params");
            let d = &s.active().unwrap().doc;
            assert_eq!(d.depth, SampleType::U8);
            let out = r["document"].as_u64().unwrap() as usize;
            s.close(out);
        }
        assert!(s.execute("file.automate.mergeToHdrPro", json!({"useOpenDocuments": true, "exposures": [0, 1]})).is_err());
    }

    fn photocraft_algo_srgb(v: f32) -> f32 {
        if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
    }

    #[test]
    fn hdr_alignment_keeps_uncovered_pixels_transparent() {
        let pixels = vec![[0.3, 0.4, 0.5, 1.0]; 4 * 3];
        let shifted = shift_rgba(&pixels, 4, 3, 1, -1);
        assert_eq!(shifted[0], [0.0; 4]);
        assert_eq!(shifted[1], pixels[5]);
        assert_eq!(shifted[9], [0.0; 4]);
    }

    #[test]
    fn shared_registration_retains_subpixel_layer_coordinates() {
        let (sw, sh) = (280usize, 220usize);
        let source = scene(sw, sh, 31);
        let (w, h) = (220usize, 160usize);
        let sample = |x: f64, y: f64| {
            let (x0, y0) = (x.floor() as usize, y.floor() as usize);
            let (tx, ty) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);
            let mut q = [0.0; 4];
            for (c, value) in q.iter_mut().enumerate() {
                let a = source[y0 * sw + x0][c] * (1.0 - tx) + source[y0 * sw + x0 + 1][c] * tx;
                let b = source[(y0 + 1) * sw + x0][c] * (1.0 - tx) + source[(y0 + 1) * sw + x0 + 1][c] * tx;
                *value = a * (1.0 - ty) + b * ty;
            }
            q
        };
        let frames: Vec<_> = [(20.0, 25.0), (23.25, 22.5)]
            .into_iter()
            .map(|(x0, y0)| {
                let pixels: Vec<_> = (0..w * h).map(|i| sample(x0 + (i % w) as f64, y0 + (i / w) as f64)).collect();
                let doc = doc_from("bracket", &pixels, w, h, SampleType::U16);
                flattened(&doc, doc.pixel_format()).unwrap()
            })
            .collect();
        let area = Rect::new(0, 0, w as i32, h as i32);
        let images = [(&frames[0], area), (&frames[1], area)];
        let alignment = register(&images, Layout::Collage, Some(0), false, None).unwrap();
        let placement = alignment.placements[1].as_ref().unwrap();
        let (x, y) = placement.forward(110.0, 80.0);
        assert!((x - 113.25).hypot(y - 77.5) < 0.5, "{x}, {y}, rms={}", alignment.rms);
        assert!(plane_matrix(placement, (0.0, 0.0)).is_some());
    }

    #[test]
    fn hdr_background_job_cancellation_leaves_sources_untouched() {
        let mut session = Session::new();
        let pixels = scene(96, 64, 19);
        for ev in [-2.0f32, 0.0, 2.0] {
            let pixels: Vec<_> = pixels.iter().map(|q| [q[0] * 2f32.powf(ev), q[1] * 2f32.powf(ev), q[2] * 2f32.powf(ev), 1.0]).collect();
            session.add_document(doc_from("source", &pixels, 96, 64, SampleType::U8), None);
        }
        let started = session.start("file.automate.mergeToHdrPro", json!({"useOpenDocuments": true, "exposures": [-2, 0, 2], "align": false})).unwrap();
        let crate::jobs::Started::Job(id) = started else { panic!("HDR must register a background worker") };
        assert!(session.cancel_job(id));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if let Some(event) = session.poll_jobs().into_iter().find(|event| event.id == id) {
                assert_eq!(event.outcome, crate::jobs::JobOutcome::Cancelled);
                break;
            }
            assert!(std::time::Instant::now() < deadline, "cancelled HDR worker did not finish");
            std::thread::yield_now();
        }
        assert_eq!(session.documents().len(), 3);
    }

    #[test]
    fn crop_and_straighten_makes_documents() {
        let (w, h) = (500usize, 360usize);
        let mut px = vec![[0.98f32, 0.98, 0.98, 1.0]; w * h];
        let photos = [([140.0f64, 120.0f64], 160.0f64, 110.0f64, 6.0f64), ([360.0, 240.0], 150.0, 120.0, -9.0)];
        for (c, pw, ph, ang) in photos {
            let (s, co) = ang.to_radians().sin_cos();
            for y in 0..h {
                for x in 0..w {
                    let (dx, dy) = (x as f64 + 0.5 - c[0], y as f64 + 0.5 - c[1]);
                    let (u, v) = (co * dx + s * dy, -s * dx + co * dy);
                    if u.abs() < pw / 2.0 && v.abs() < ph / 2.0 {
                        px[y * w + x] = [0.2 + (u / pw) as f32 * 0.3, 0.3, 0.6, 1.0];
                    }
                }
            }
        }
        let mut s = Session::new();
        s.add_document(doc_from("scan", &px, w, h, SampleType::U16), None);
        assert!(s.is_enabled("file.automate.cropAndStraightenPhotos"));
        let r = s.execute("file.automate.cropAndStraightenPhotos", json!({})).unwrap();
        let found = r["photos"].as_array().unwrap();
        assert_eq!(found.len(), 2, "{r}");
        assert_eq!(s.documents().len(), 3);
        assert!((found[0]["angle"].as_f64().unwrap() - 6.0).abs() < 1.0);
        let d = &s.documents()[1].doc;
        assert!((d.size.width as i64 - 158).abs() <= 4 && (d.size.height as i64 - 108).abs() <= 4, "{:?}", d.size);
        assert_eq!(d.depth, SampleType::U16);
        // Upright: the photo's horizontal gradient runs along x.
        let l = d.layers[0].surface().unwrap();
        let (a, b) = (l.rgba(10, d.size.height as i32 / 2)[0], l.rgba(d.size.width as i32 - 10, d.size.height as i32 / 2)[0]);
        assert!(b > a + 0.15, "{a} {b}");
        let (c, e) = (l.rgba(d.size.width as i32 / 2, 6)[0], l.rgba(d.size.width as i32 / 2, d.size.height as i32 - 6)[0]);
        assert!((c - e).abs() < 0.05);
    }
}
