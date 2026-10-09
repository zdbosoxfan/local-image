//! AI Remove (Lightroom's Remove tool with generative AI): a background job cuts the area around a
//! brushed, lassoed or masked removal out of the photo as a render shows it before cropping and
//! white balance ([`lightcraft_pipeline::patches::transformed`]), lets the host's AI engine repair
//! it through a reversible 8-bit view ([`AiView`]), and stores the repaired pixels as a patch
//! ([`super::store`]) that the pipeline composites at the retouching stage. The develop settings
//! only refer to the patch, so the removal stays editable (opacity, delete, regenerate, undo).

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
    /// spot). In the background, or here with `wait`.
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
        let host = self.enhance.host.clone().ok_or(NO_HOST)?;
        let engine = self.ai_remove_engine(engine)?;
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
        };
        let (kind, label) = match regenerate {
            Some(spot) => (JobKind::Regenerate { spot }, "Regenerate"),
            None => (JobKind::Remove { stroke }, "AI Remove"),
        };
        Ok(self.enhance.spawn(id, kind, label, wait, Box::new(move |ctl| job.run(ctl))))
    }
}

const NO_HOST: &str = "AI Remove needs the AI engine. Set it up in Compositing › Local AI.";

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
    host: Arc<dyn AiHost>,
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
        let (frame, img) = lightcraft_pipeline::patches::transformed(&decoded.image, &info, s);
        drop(decoded);
        let (w, h) = (img.width, img.height);
        ctl.set(0.05, "Preparing");
        let coverage = match &self.mask {
            Some(m) => mask_coverage(m, &frame, &img, &info, s),
            None => stroke_coverage(&self.stroke, &frame, w, h),
        };
        let Some(area) = bbox(&coverage, w, h) else { return Err("Paint over what you want to remove first.".into()) };
        let region = context(area, w, h);
        let (rw, rh) = (region.width(), region.height());
        let pixels: Vec<[f32; 3]> = (0..rw * rh).map(|i| img.get(region.x0 + i % rw, region.y0 + i / rw)).collect();
        let view = AiView::new(&pixels, &info, s);
        let req = RemoveRequest {
            width: rw,
            height: rh,
            rgb: pixels.iter().map(|p| view.encode(*p)).collect(),
            mask: (0..rw * rh).map(|i| (coverage[(region.y0 + i / rw) * w + region.x0 + i % rw] * 255.0 + 0.5) as u8).collect(),
            engine: self.engine.clone(),
            seed: self.seed,
        };
        ctl.set(0.1, "Removing");
        let out = self.host.remove(&req, ctl)?;
        ctl.check()?;
        if out.rgb.len() != rw * rh || out.alpha.len() != rw * rh {
            return Err("The AI engine returned an unexpected size.".into());
        }
        ctl.set(0.95, "Saving");
        // the patch: the repaired part only, back in the photo's own light
        let Some(pb) = bbox(&out.alpha.iter().map(|a| *a as f32 / 255.0).collect::<Vec<_>>(), rw, rh) else {
            return Err("The AI engine changed nothing.".into());
        };
        let (pw, ph) = (pb.width(), pb.height());
        let data: Vec<f32> = (0..pw * ph)
            .flat_map(|i| {
                let j = (pb.y0 + i / pw) * rw + pb.x0 + i % pw;
                let c = view.decode(out.rgb[j]);
                [c[0].max(0.0), c[1].max(0.0), c[2].max(0.0), out.alpha[j] as f32 / 255.0]
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

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Coverage (0..1) of a brushed or lassoed stroke on the `w × h` transformed image.
fn stroke_coverage(stroke: &AiStroke, frame: &lightcraft_pipeline::geometry::Frame, w: usize, h: usize) -> Vec<f32> {
    let to_out = frame.norm_to_out(w, h);
    let mut cov = vec![0.0f32; w * h];
    if stroke.polygon.len() >= 3 {
        let poly: Vec<Point> = stroke.polygon.iter().map(|p| to_out.apply(*p)).collect();
        let (y0, y1) = poly.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
        for y in (y0.floor().max(0.0) as usize)..(y1.ceil().max(0.0) as usize).min(h) {
            let py = y as f64 + 0.5;
            // even-odd scanline fill
            let mut xs: Vec<f64> = Vec::new();
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                if (a.y <= py) != (b.y <= py) {
                    xs.push(a.x + (py - a.y) / (b.y - a.y) * (b.x - a.x));
                }
            }
            xs.sort_by(f64::total_cmp);
            for pair in xs.as_chunks::<2>().0 {
                let (xa, xb) = ((pair[0] - 0.5).ceil().max(0.0) as usize, ((pair[1] - 0.5).floor() + 1.0).max(0.0) as usize);
                for x in xa..xb.min(w) {
                    cov[y * w + x] = 1.0;
                }
            }
        }
        return cov;
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
