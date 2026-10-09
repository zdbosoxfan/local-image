//! AI Remove (Lightroom's Remove tool with generative AI): a background job cuts the area around a
//! brushed, lassoed or masked removal out of the photo as a render shows it before cropping and
//! white balance ([`lightcraft_pipeline::patches::transformed`]), lets the host's AI engine repair
//! it through a reversible 8-bit view ([`AiView`]), and stores the repaired pixels as a patch
//! ([`super::store`]) that the pipeline composites at the retouching stage. The develop settings
//! only refer to the patch, so the removal stays editable (opacity, delete, regenerate, undo).
//!
//! The same path makes Develop's content-aware **Heal** ([`LOCAL`] engine): instead of asking the
//! host, the worker fills the stroke with Compositing's Spot Healing Brush — multi-scale PatchMatch
//! completion (`photocraft_algo::inpaint::complete`) and a Poisson seamless blend — on this
//! computer, and stores the result as a patch like an AI removal (Regenerate, undo, geometry
//! staleness and copies work the same).
//!
//! Both see the photo with the patches of earlier removals and heals already in place, so a second
//! removal next to (or over) a first one never brings back what the first one removed.

use std::sync::Arc;

use lightcraft_catalog::PhotoId;
use lightcraft_develop::DevelopSettings;
use lightcraft_geom::Point;
use lightcraft_pipeline::patches::{AiView, PatchPixels, PatchSource};
use lightcraft_preview::Hasher128;
use lightcraft_raster::resample::{Filter, resize};
use lightcraft_raster::{Plane, Rgb32f};

use super::store::{self, Kind};
use super::{AiHost, AiStroke, Job, JobCtl, JobKind, Outcome, RemoveDone, RemoveRequest};
use crate::Session;
use crate::media::SourceLevel;

/// The AI sees at least this much around a removal (px), like the editor's AI Remove.
const CONTEXT_MIN: usize = 640;
/// Long edge at which a develop layer's mask is evaluated for "remove inside mask".
const MASK_EDGE: usize = 1536;
/// The engine key of content-aware healing on this computer (no AI host needed).
pub const LOCAL: &str = "local";
/// Margin of texture around a heal for the patch search, as Compositing's Spot Healing Brush:
/// three brush diameters, within these bounds (px).
const HEAL_MARGIN: (f64, f64) = (48.0, 600.0);

/// The store as the pipeline's patch source. Installed once per process ([`install`]).
pub struct Patches;

impl PatchSource for Patches {
    fn load(&self, key: &str) -> Option<Arc<PatchPixels>> {
        let r = store::get(Kind::Remove, key)?;
        (r.channels == 4).then(|| Arc::new(PatchPixels { width: r.width, height: r.height, data: r.data.as_chunks::<4>().0.to_vec() }))
    }
}

/// Let renders find stored patches.
pub fn install() {
    lightcraft_pipeline::patches::set_source(Some(Arc::new(Patches)));
}

/// What the photo's geometry was (orientation, lens corrections, perspective): a patch made under
/// another geometry no longer lines up and is offered for regeneration.
pub fn geometry_tag(s: &DevelopSettings) -> String {
    let o = &s.optics;
    let key = format!(
        "{:?}|{}|{}|{:?}|{}|{:?}",
        s.orientation,
        s.section_enabled("optics"),
        o.lens_profile,
        [o.profile_distortion, o.distortion],
        s.section_enabled("geometry"),
        s.geometry
    );
    Hasher128::new().str(&key).finish().to_string()
}

/// Store key of a patch: the photo, the stroke, the engine, the seed and the geometry it was
/// made with.
pub fn patch_key(source: &str, stroke: &AiStroke, engine: &str, seed: u64, geometry: &str) -> String {
    let stroke = serde_json::to_string(stroke).unwrap_or_default();
    store::key_of(&["remove", source, &stroke, engine, &seed.to_string(), geometry])
}

/// A new 48-bit seed.
fn new_seed(salt: u64) -> u64 {
    let t = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    let h = Hasher128::new().u64(t).u64(salt).finish();
    (h.0 as u64) & ((1 << 48) - 1)
}

impl Session {
    /// The AI Remove engine to use: `wanted` if the host has it, else the first ready one.
    pub fn ai_remove_engine(&self, wanted: Option<&str>) -> Result<String, String> {
        if wanted == Some(LOCAL) {
            return Ok(LOCAL.into());
        }
        let host = self.enhance.host.as_ref().ok_or(NO_HOST)?;
        let engines = host.remove_engines();
        let e = match wanted {
            Some(w) => engines.iter().find(|e| e.key == w).ok_or_else(|| format!("unknown AI Remove engine `{w}`"))?,
            None => engines.iter().find(|e| e.problem.is_none()).or(engines.first()).ok_or(NO_HOST)?,
        };
        match &e.problem {
            Some(p) => Err(p.clone()),
            None => Ok(e.key.clone()),
        }
    }

    /// Start an AI removal of `stroke` on photo `id` (`regenerate`: a new variation of that AI
    /// spot). In the background, or here with `wait`. With engine [`LOCAL`] it is a content-aware
    /// heal, made on this computer (no AI host needed).
    pub fn start_ai_remove(
        &mut self,
        id: PhotoId,
        stroke: AiStroke,
        engine: Option<&str>,
        seed: Option<u64>,
        regenerate: Option<usize>,
        wait: bool,
    ) -> Result<Arc<Job>, String> {
        if self.is_ephemeral(id) {
            return Err("AI Remove isn't available in the Camera Raw Filter: use Compositing's AI Remove on the layer.".into());
        }
        if stroke.points.is_empty() && stroke.polygon.len() < 3 && stroke.mask.is_none() {
            return Err("Paint over what you want to remove first.".into());
        }
        let engine = self.ai_remove_engine(engine)?;
        let host = if engine == LOCAL { None } else { Some(self.enhance.host.clone().ok_or(NO_HOST)?) };
        let p = self.catalog.photo(id).ok_or("no such photo")?.clone();
        let settings = self.develop_of(id).unwrap_or_default();
        let mask = match stroke.mask {
            Some(m) => Some(settings.masks.iter().find(|x| x.id == m).cloned().ok_or("no such mask")?),
            None => None,
        };
        let seed = seed.unwrap_or_else(|| new_seed(id.0 ^ self.enhance.jobs().len() as u64));
        install();
        let job = Prepared {
            to: self.ai_store_root(),
            src: self.media.source_ref(&p, SourceLevel::Full),
            header: crate::media::source_info(&p),
            settings,
            source: super::denoise::source_hash(&p),
            stroke: stroke.clone(),
            mask,
            engine,
            seed,
            host,
            exclude: regenerate,
        };
        let (kind, label) = match regenerate {
            Some(spot) => (JobKind::Regenerate { spot }, "Regenerate"),
            None if job.engine == LOCAL => (JobKind::Remove { stroke }, HEAL_LABEL),
            None => (JobKind::Remove { stroke }, "AI Remove"),
        };
        Ok(self.enhance.spawn(id, kind, label, wait, Box::new(move |ctl| job.run(ctl))))
    }
}

const NO_HOST: &str = "AI Remove needs the AI engine. Set it up in Compositing › Local AI.";
/// The job (and undo step) label of a content-aware heal.
pub const HEAL_LABEL: &str = "Heal";

/// Everything the worker needs, detached from the session.
struct Prepared {
    /// Where the patch is kept (the library's `Patches` folder; `None`: in memory).
    to: Option<std::path::PathBuf>,
    src: crate::media::SourceRef,
    header: lightcraft_pipeline::SourceInfo,
    settings: Arc<DevelopSettings>,
    source: String,
    stroke: AiStroke,
    mask: Option<lightcraft_develop::Mask>,
    engine: String,
    seed: u64,
    /// `None` for the [`LOCAL`] engine.
    host: Option<Arc<dyn AiHost>>,
    /// The AI spot being regenerated: its own patch is left out of what the engine sees.
    exclude: Option<usize>,
}

/// An inclusive-exclusive pixel box.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PxBox {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

impl PxBox {
    fn width(&self) -> usize {
        self.x1 - self.x0
    }
    fn height(&self) -> usize {
        self.y1 - self.y0
    }
}

impl Prepared {
    fn run(self, ctl: &JobCtl) -> Result<Outcome, String> {
        ctl.set(0.01, "Loading the photo");
        let decoded = self.src.load_source()?;
        let info = decoded.info_or(self.header);
        ctl.check()?;
        let s = &*self.settings;
        let (frame, mut img) = lightcraft_pipeline::patches::transformed(&decoded.image, &info, s);
        drop(decoded);
        // what earlier removals and heals left (not this spot's own patch when regenerating, nor
        // patches made on another photo): the engine works on the photo as it looks now
        let earlier: Vec<lightcraft_develop::Spot> = s
            .spots
            .iter()
            .enumerate()
            .filter(|(i, sp)| Some(*i) != self.exclude && sp.patch.as_ref().is_some_and(|p| p.source == self.source))
            .map(|(_, sp)| sp.clone())
            .collect();
        lightcraft_pipeline::patches::apply_spots(&mut img, &earlier, &frame);
        let (w, h) = (img.width, img.height);
        ctl.set(0.05, "Preparing");
        let coverage = match &self.mask {
            Some(m) => mask_coverage(m, &frame, &img, &info, s),
            None => stroke_coverage(&self.stroke, &frame, w, h),
        };
        let Some(area) = bbox(&coverage, w, h) else { return Err("Paint over what you want to remove first.".into()) };
        let local = self.engine == LOCAL;
        let region = if local {
            // as Compositing's Spot Healing Brush: three brush diameters of texture around it
            let size =
                if self.stroke.points.is_empty() { area.width().min(area.height()) as f64 } else { 2.0 * self.stroke.size * frame.px_per_long(w) };
            let margin = (size * 3.0).ceil().clamp(HEAL_MARGIN.0, HEAL_MARGIN.1) as usize;
            inflate(area, margin, w, h)
        } else {
            context(area, w, h)
        };
        let (rw, rh) = (region.width(), region.height());
        let pixels: Vec<[f32; 3]> = (0..rw * rh).map(|i| img.get(region.x0 + i % rw, region.y0 + i / rw)).collect();
        let cov: Vec<f32> = (0..rw * rh).map(|i| coverage[(region.y0 + i / rw) * w + region.x0 + i % rw]).collect();
        drop(img);
        // the repaired region (in the photo's own light) and where it replaces the photo (0..1)
        let (out, alpha): (Vec<[f32; 3]>, Vec<f32>) = if local {
            ctl.set(0.1, "Healing");
            (heal(&pixels, rw, rh, &cov, self.seed, ctl)?, cov)
        } else {
            let host = self.host.as_ref().ok_or(NO_HOST)?;
            let view = AiView::new(&pixels, &info, s);
            let req = RemoveRequest {
                width: rw,
                height: rh,
                rgb: pixels.iter().map(|p| view.encode(*p)).collect(),
                mask: cov.iter().map(|c| (c * 255.0 + 0.5) as u8).collect(),
                engine: self.engine.clone(),
                seed: self.seed,
            };
            ctl.set(0.1, "Removing");
            let out = host.remove(&req, ctl)?;
            ctl.check()?;
            if out.rgb.len() != rw * rh || out.alpha.len() != rw * rh {
                return Err("The AI engine returned an unexpected size.".into());
            }
            (out.rgb.iter().map(|c| view.decode(*c)).collect(), out.alpha.iter().map(|a| *a as f32 / 255.0).collect())
        };
        ctl.check()?;
        ctl.set(0.95, "Saving");
        // the patch: the repaired part only
        let Some(pb) = bbox(&alpha, rw, rh) else {
            return Err("The AI engine changed nothing.".into());
        };
        let (pw, ph) = (pb.width(), pb.height());
        let data: Vec<f32> = (0..pw * ph)
            .flat_map(|i| {
                let j = (pb.y0 + i / pw) * rw + pb.x0 + i % pw;
                let c = out[j];
                [c[0].max(0.0), c[1].max(0.0), c[2].max(0.0), alpha[j]]
            })
            .collect();
        let geometry = geometry_tag(s);
        let key = patch_key(&self.source, &self.stroke, &self.engine, self.seed, &geometry);
        let raster = store::Raster { width: pw, height: ph, channels: 4, data };
        store::put(Kind::Remove, &key, &raster, self.to.as_deref())?;
        let pixels = PatchPixels { width: pw, height: ph, data: raster.data.as_chunks::<4>().0.to_vec() };
        lightcraft_pipeline::patches::insert(&key, Arc::new(pixels));
        let to_norm = frame.out_to_norm(w, h);
        let a = to_norm.apply(Point::new((region.x0 + pb.x0) as f64, (region.y0 + pb.y0) as f64));
        let b = to_norm.apply(Point::new((region.x0 + pb.x1) as f64, (region.y0 + pb.y1) as f64));
        let rect = [a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)];
        ctl.set(1.0, "Done");
        Ok(Outcome::Remove(RemoveDone { stroke: self.stroke, key, source: self.source, rect, engine: self.engine, seed: self.seed, geometry }))
    }
}

/// Dilate a `w × h` mask by `r` pixels (square structuring element, separable).
fn dilate(w: usize, h: usize, m: &[bool], r: usize) -> Vec<bool> {
    let mut tmp = vec![false; w * h];
    for y in 0..h {
        for x in (0..w).filter(|x| m[y * w + x]) {
            tmp[y * w + x.saturating_sub(r)..y * w + (x + r + 1).min(w)].fill(true);
        }
    }
    let mut out = vec![false; w * h];
    for y in 0..h {
        for x in (0..w).filter(|x| tmp[y * w + x]) {
            for yy in y.saturating_sub(r)..(y + r + 1).min(h) {
                out[yy * w + x] = true;
            }
        }
    }
    out
}

/// Content-aware healing of `cov` (the stroke's coverage) in `pixels` (`w × h`, linear, the
/// source's space), as Compositing's Spot Healing Brush does it: PatchMatch completion of the
/// stroke from the texture around it, then a Poisson seamless blend into the surroundings. Works
/// through a perceptual encoding (the area's bright end at 1, a 2.2 gamma) so the patch search
/// weighs shadows as the eye does. Returns the region healed (as it was away from the stroke).
pub(crate) fn heal(pixels: &[[f32; 3]], w: usize, h: usize, cov: &[f32], seed: u64, ctl: &JobCtl) -> Result<Vec<[f32; 3]>, String> {
    use photocraft_algo::inpaint::{CompleteParams, complete_with};
    use photocraft_algo::poisson;
    const GAMMA: f32 = 2.2;
    let Some(hb) = bbox(cov, w, h) else { return Ok(pixels.to_vec()) };
    let hole: Vec<bool> = cov.iter().map(|c| *c > 0.004).collect();
    // the encoding: the area's 99th percentile at 1
    let mut peak: Vec<f32> = pixels.iter().map(|p| p[0].max(p[1]).max(p[2])).filter(|v| v.is_finite()).collect();
    let k = if peak.is_empty() {
        1.0
    } else {
        let i = ((peak.len() - 1) as f32 * 0.99) as usize;
        let (_, p99, _) = peak.select_nth_unstable_by(i, f32::total_cmp);
        1.0 / p99.max(1e-6)
    };
    let img: Vec<f32> = pixels.iter().flat_map(|p| p.map(|v| if v.is_finite() { (v.max(0.0) * k).powf(1.0 / GAMMA) } else { 0.0 })).collect();
    // fill a slightly larger domain than the blend's, so the blend's border carries filled values
    // that differ from the surroundings: that mismatch is what the membrane corrects
    let domain = dilate(w, h, &hole, 2);
    let blend = dilate(w, h, &hole, 1);
    let cancel = || ctl.cancelled();
    let progress = |f: f32| ctl.set(0.1 + 0.8 * f, "Healing");
    let interrupt = photocraft_raster::Interrupt::new(&cancel, &progress);
    let params = CompleteParams { seed, ..CompleteParams::default() };
    let filled = complete_with(w, h, 3, &img, &domain, &params, &interrupt)
        .map_err(|_| super::CANCELLED.to_string())?
        .unwrap_or_else(|| poisson::membrane_fill(w, h, 3, &img, &domain));
    ctl.check()?;
    // the seamless clone over the stroke's box (with a border of known pixels around it)
    let r = inflate(hb, 2, w, h);
    let (cw, ch) = (r.width(), r.height());
    let crop = |src: &[f32]| -> Vec<f32> { (r.y0..r.y1).flat_map(|y| src[(y * w + r.x0) * 3..(y * w + r.x1) * 3].iter().copied()).collect() };
    let mask: Vec<bool> = (r.y0..r.y1).flat_map(|y| blend[y * w + r.x0..y * w + r.x1].iter().copied()).collect();
    let healed = poisson::seamless_clone(cw, ch, 3, &crop(&filled), &crop(&img), &mask);
    let mut out = pixels.to_vec();
    for y in 0..ch {
        for x in 0..cw {
            if mask[y * cw + x] {
                let j = (y * cw + x) * 3;
                out[(r.y0 + y) * w + r.x0 + x] = [0, 1, 2].map(|c| healed[j + c].max(0.0).powf(GAMMA) / k);
            }
        }
    }
    Ok(out)
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Coverage (0..1) of a brushed and/or lassoed stroke on the `w × h` transformed image: the union
/// of the lasso outline and the brush dabs. The outline is filled by the non-zero winding rule, so
/// several lassos joined into one outline (each closed, consistently oriented, linked by a bridge
/// walked there and back) cover their union.
fn stroke_coverage(stroke: &AiStroke, frame: &lightcraft_pipeline::geometry::Frame, w: usize, h: usize) -> Vec<f32> {
    let to_out = frame.norm_to_out(w, h);
    let mut cov = vec![0.0f32; w * h];
    if stroke.polygon.len() >= 3 {
        let poly: Vec<Point> = stroke.polygon.iter().map(|p| to_out.apply(*p)).collect();
        let (y0, y1) = poly.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
        for y in (y0.floor().max(0.0) as usize)..(y1.ceil().max(0.0) as usize).min(h) {
            let py = y as f64 + 0.5;
            // non-zero winding scanline fill: crossings with the edges' directions
            let mut xs: Vec<(f64, i32)> = Vec::new();
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                if (a.y <= py) != (b.y <= py) {
                    xs.push((a.x + (py - a.y) / (b.y - a.y) * (b.x - a.x), if b.y > a.y { 1 } else { -1 }));
                }
            }
            xs.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut winding = 0;
            for pair in xs.windows(2) {
                winding += pair[0].1;
                if winding == 0 {
                    continue;
                }
                let (xa, xb) = ((pair[0].0 - 0.5).ceil().max(0.0) as usize, ((pair[1].0 - 0.5).floor() + 1.0).max(0.0) as usize);
                for x in xa..xb.min(w) {
                    cov[y * w + x] = 1.0;
                }
            }
        }
    }
    let r = (stroke.size * frame.px_per_long(w)).max(1.0) as f32;
    let inner = r * (1.0 - (stroke.feather / 100.0).clamp(0.0, 1.0) as f32 * 0.8);
    for p in &stroke.points {
        let q = to_out.apply(*p);
        let (cx, cy) = (q.x as f32, q.y as f32);
        let (x0, x1) = (((cx - r).floor().max(0.0)) as usize, ((cx + r).ceil().max(0.0) as usize).min(w));
        let (y0, y1) = (((cy - r).floor().max(0.0)) as usize, ((cy + r).ceil().max(0.0) as usize).min(h));
        for y in y0..y1 {
            for x in x0..x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let a = 1.0 - smooth(inner, r, d);
                let c = &mut cov[y * w + x];
                *c = c.max(a);
            }
        }
    }
    cov
}

/// Coverage of a develop layer's mask on the transformed image (evaluated smaller, scaled up).
fn mask_coverage(
    m: &lightcraft_develop::Mask,
    frame: &lightcraft_pipeline::geometry::Frame,
    img: &Rgb32f,
    info: &lightcraft_pipeline::SourceInfo,
    s: &DevelopSettings,
) -> Vec<f32> {
    let (ew, eh) = frame.fit(MASK_EDGE, MASK_EDGE);
    let mut small = resize(img, ew, eh, Filter::Box);
    lightcraft_pipeline::local::white_balance(&mut small, info, s);
    let log_l = small.map(lightcraft_pipeline::local::log_lum);
    let alpha = lightcraft_pipeline::masks::evaluate_one(m, frame, ew, eh, &small, &log_l, s.light.exposure as f32);
    let peak = alpha.data.iter().fold(0.0f32, |a, v| a.max(*v)).max(1e-6);
    let alpha = Plane { width: ew, height: eh, data: alpha.data.iter().map(|v| (v / peak).clamp(0.0, 1.0)).collect() };
    let full = resize(&alpha, img.width, img.height, Filter::Bilinear);
    full.data
}

/// The box of coverage above a trace, `None` when there is none.
fn bbox(cov: &[f32], w: usize, h: usize) -> Option<PxBox> {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if cov[y * w + x] > 0.004 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    (x1 > x0 && y1 > y0).then_some(PxBox { x0, y0, x1, y1 })
}

/// `b` grown by `m` on every side, within the image.
fn inflate(b: PxBox, m: usize, w: usize, h: usize) -> PxBox {
    PxBox { x0: b.x0.saturating_sub(m), y0: b.y0.saturating_sub(m), x1: (b.x1 + m).min(w), y1: (b.y1 + m).min(h) }
}

/// The area the AI sees around `b`: twice its size on every side, at least [`CONTEXT_MIN`],
/// within the image.
fn context(b: PxBox, w: usize, h: usize) -> PxBox {
    let pad = (b.width().max(b.height()) * 2).max(CONTEXT_MIN / 2);
    PxBox { x0: b.x0.saturating_sub(pad), y0: b.y0.saturating_sub(pad), x1: (b.x1 + pad).min(w), y1: (b.y1 + pad).min(h) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(points: Vec<Point>) -> AiStroke {
        AiStroke { points, polygon: vec![], size: 0.05, feather: 0.0, opacity: 100.0, mask: None }
    }

    #[test]
    fn strokes_and_lassos_cover_their_area() {
        let frame = lightcraft_pipeline::geometry::Frame::new(200, 100, &DevelopSettings::default(), false);
        let c = stroke_coverage(&stroke(vec![Point::new(0.5, 0.5)]), &frame, 200, 100);
        assert_eq!(c[50 * 200 + 100], 1.0);
        assert_eq!(c[50 * 200 + 120], 0.0, "radius 10 px");
        let b = bbox(&c, 200, 100).unwrap();
        assert!(b.x0 >= 89 && b.x1 <= 111, "{b:?}");
        let lasso =
            AiStroke { polygon: vec![Point::new(0.1, 0.1), Point::new(0.3, 0.1), Point::new(0.3, 0.5), Point::new(0.1, 0.5)], ..stroke(vec![]) };
        let c = stroke_coverage(&lasso, &frame, 200, 100);
        assert_eq!(c[30 * 200 + 40], 1.0);
        assert_eq!(c[30 * 200 + 70], 0.0);
        assert_eq!(bbox(&c, 200, 100), Some(PxBox { x0: 20, y0: 10, x1: 60, y1: 50 }));
        let ctx = context(PxBox { x0: 20, y0: 10, x1: 60, y1: 50 }, 200, 100);
        assert_eq!(ctx, PxBox { x0: 0, y0: 0, x1: 200, y1: 100 });
    }

    #[test]
    fn joined_lassos_and_dabs_cover_their_union() {
        let frame = lightcraft_pipeline::geometry::Frame::new(200, 100, &DevelopSettings::default(), false);
        let sq = |x0: f64, y0: f64, x1: f64, y1: f64| vec![Point::new(x0, y0), Point::new(x1, y0), Point::new(x1, y1), Point::new(x0, y1)];
        // two overlapping squares, the second wound the same way, joined by a bridge: A…, A0, B…, B0
        let (a, b) = (sq(0.1, 0.1, 0.3, 0.5), sq(0.2, 0.3, 0.5, 0.8));
        let mut poly = a.clone();
        poly.push(a[0]);
        poly.extend(b.iter().copied());
        poly.push(b[0]);
        let st = AiStroke { polygon: poly, ..stroke(vec![Point::new(0.9, 0.2)]) };
        let c = stroke_coverage(&st, &frame, 200, 100);
        for (x, y) in [(30, 20), (50, 40), (80, 70), (180, 20)] {
            assert_eq!(c[y * 200 + x], 1.0, "({x}, {y}) is covered");
        }
        for (x, y) in [(90, 20), (30, 70), (150, 50)] {
            assert_eq!(c[y * 200 + x], 0.0, "({x}, {y}) is not");
        }
        // a single lasso is filled whichever way it was drawn
        let mut cw = sq(0.1, 0.1, 0.3, 0.5);
        cw.reverse();
        let c = stroke_coverage(&AiStroke { polygon: cw, ..stroke(vec![]) }, &frame, 200, 100);
        assert_eq!(c[30 * 200 + 40], 1.0);
    }

    #[test]
    fn local_heal_fills_from_the_surroundings() {
        // vertical stripes with a dark blob in the middle
        let (w, h) = (96usize, 96usize);
        let px: Vec<[f32; 3]> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                let d = ((x as f32 - 48.0).powi(2) + (y as f32 - 48.0).powi(2)).sqrt();
                if d < 6.0 { [0.01; 3] } else { [if (x / 3).is_multiple_of(2) { 0.2 } else { 0.4 }; 3] }
            })
            .collect();
        let cov: Vec<f32> =
            (0..w * h).map(|i| if ((i % w) as f32 - 48.0).powi(2) + ((i / w) as f32 - 48.0).powi(2) < 81.0 { 1.0 } else { 0.0 }).collect();
        let out = heal(&px, w, h, &cov, 1, &JobCtl::default()).unwrap();
        let centre: Vec<f32> = (44..52).flat_map(|y| (44..52).map(move |x| (x, y))).map(|(x, y)| out[y * w + x][1]).collect();
        let mean = centre.iter().sum::<f32>() / centre.len() as f32;
        assert!((mean - 0.3).abs() < 0.06, "the blob is gone: {mean}");
        let spread = centre.iter().fold(0.0f32, |a, v| a.max(*v)) - centre.iter().fold(1.0f32, |a, v| a.min(*v));
        assert!(spread > 0.1, "the stripes continue: {spread}");
        assert_eq!(out[5 * w + 5], px[5 * w + 5], "untouched away from the stroke");
    }

    #[test]
    fn keys_change_with_every_input() {
        let s = stroke(vec![Point::new(0.5, 0.5)]);
        let k = patch_key("src", &s, "klein", 1, "g");
        assert_ne!(k, patch_key("src2", &s, "klein", 1, "g"));
        assert_ne!(k, patch_key("src", &s, "klein", 2, "g"));
        assert_ne!(k, patch_key("src", &stroke(vec![Point::new(0.5, 0.6)]), "klein", 1, "g"));
        let mut d = DevelopSettings::default();
        let g = geometry_tag(&d);
        d.crop.geometry.rect = lightcraft_geom::Rect::new(0.1, 0.1, 0.9, 0.9);
        assert_eq!(g, geometry_tag(&d), "a crop keeps patches in place");
        d.optics.distortion = 20.0;
        assert_ne!(g, geometry_tag(&d));
    }
}
