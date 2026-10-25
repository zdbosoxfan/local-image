//! local-image: quick **Subject**, **Background**, **Sky** and **Depth** masks from small models
//! that run on the CPU with no AI server (`li-seg`: IS-Net for the subject, PP-MobileSeg for the sky,
//! Depth Anything V2 Small for depth, or a custom model for any of them). The model looks at the photo as developed, uncropped and without masks
//! (as SAM 3 does for Object masks), once, when the mask is added; the result is stored in the
//! mask as a [`SegMask`] (resolution-independent logits), so renders and exports never run the
//! model and the mask stays put when the photo is cropped later. Without a model the mask keeps
//! the classical estimate.

use li_raster::resample::Filter;
use lightcraft_catalog::PhotoId;
use lightcraft_develop::{MaskShape, SegMask};

use crate::Session;

/// Long edge of the photo the quick models see.
const INPUT_EDGE: usize = 1024;
/// Side of the stored logit grid for sky and depth maps (their models resolve no finer).
const GRID: usize = 256;
/// Side of the stored logit grid for subject maps (their 1024 px models resolve twice as fine).
const SUBJECT_GRID: usize = 512;
/// Refine Edges (0..100) of a new mask a model made: its edges follow the photo's instead of the
/// stored grid's (masks already in documents keep theirs).
pub const MODEL_EDGE_REFINE: f64 = 30.0;

/// Whether `shape` holds a model's segmentation (Subject, Background, Sky or Depth Range).
pub fn is_model_mask(shape: &MaskShape) -> bool {
    matches!(
        shape,
        MaskShape::Subject { seg: Some(_) }
            | MaskShape::Background { seg: Some(_) }
            | MaskShape::Sky { seg: Some(_) }
            | MaskShape::DepthRange { seg: Some(_), .. }
    )
}

/// What a quick model made of a mask shape.
#[derive(Clone, Debug, PartialEq)]
pub enum Quick {
    /// Not a quick-model shape, or no model is installed: the classical estimate stays.
    Nothing,
    /// The shape holds the model's segmentation.
    Filled,
    /// The model failed or was refused (why); the classical estimate stays.
    Failed(String),
}

/// The function a quick model serves.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Subject,
    Sky,
    Depth,
}

impl Kind {
    fn of(shape: &MaskShape) -> Option<Kind> {
        match shape {
            MaskShape::Subject { seg: None } | MaskShape::Background { seg: None } => Some(Kind::Subject),
            MaskShape::Sky { seg: None } => Some(Kind::Sky),
            MaskShape::DepthRange { seg: None, .. } => Some(Kind::Depth),
            _ => None,
        }
    }

    fn group(self) -> li_seg::Group {
        match self {
            Kind::Subject => li_seg::Group::Subject,
            Kind::Sky => li_seg::Group::Sky,
            Kind::Depth => li_seg::Group::Depth,
        }
    }
}

/// The uncropped coordinate system of a quick-model mask. A stored automatic Upright solve
/// completes the same mapping; crop and tone edits do not move these mask coordinates.
fn mask_geometry(s: &lightcraft_develop::DevelopSettings) -> String {
    let mut geometry = s.geometry.clone();
    if matches!(
        geometry.upright,
        lightcraft_develop::Upright::Auto
            | lightcraft_develop::Upright::Level
            | lightcraft_develop::Upright::Vertical
            | lightcraft_develop::Upright::Full
    ) {
        geometry.upright_transform = None;
    }
    lightcraft_preview::Hasher128::new()
        .str(&format!(
            "{:?}",
            (s.orientation, s.section_enabled("optics"), &s.optics, &s.lens_db, &s.lens_profile, s.section_enabled("geometry"), geometry)
        ))
        .finish()
        .to_string()
}

/// A quick-model run prepared on the session's thread; [`QuickRun::run`] does the work anywhere.
pub(crate) struct QuickRun {
    geometry: String,
    source: String,
    raw: u64,
    shape: MaskShape,
    kind: Kind,
    invert: bool,
    dir: std::path::PathBuf,
    render: crate::media::RenderJob,
}

/// Quick-model runs in the background take turns: each holds a model and a rendered photo.
static QUICK_TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl QuickRun {
    /// Renders the photo and runs the model: the shape's segmentation, `Ok(None)` when no model
    /// serves the function any more. `cancel` stops the model at its next stage.
    fn run(self, cancel: &std::sync::atomic::AtomicBool) -> Result<Option<SegMask>, String> {
        let model = match self.kind {
            Kind::Subject => li_seg::shared(&self.dir),
            Kind::Sky => li_seg::shared_sky(&self.dir),
            Kind::Depth => li_seg::shared_depth(&self.dir),
        };
        let Some(model) = model.map_err(|e| format!("{e:#}"))? else { return Ok(None) };
        let img = self.render.run().rendered?.image;
        let (w, h) = (img.width, img.height);
        let rgba: Vec<u8> = img.data.iter().flatten().copied().collect();
        let prob = match self.kind {
            // stored as distance (0 = nearest), the scale the depth range reads
            Kind::Depth => model.predict_depth_with_cancel(&rgba, w, h, cancel).map(|near| near.iter().map(|n| 1.0 - n).collect()),
            Kind::Sky => model.predict_sky_with_cancel(&rgba, w, h, cancel),
            // No clear subject: an empty subject (a full background).
            Kind::Subject => model.predict_with_cancel(&rgba, w, h, cancel).map(|p| p.unwrap_or_else(|| vec![0.0; w * h])),
        }
        .map_err(|e| format!("{e:#}"))?;
        Ok(Some(to_segmask(&prob, w, h, self.invert, if self.kind == Kind::Subject { SUBJECT_GRID } else { GRID })))
    }
}

/// Fills `shape`'s segmentation with `seg`, when it is still the quick-model shape it was.
fn fill(shape: &mut MaskShape, seg: SegMask) -> bool {
    match shape {
        MaskShape::Subject { seg: s @ None }
        | MaskShape::Background { seg: s @ None }
        | MaskShape::Sky { seg: s @ None }
        | MaskShape::DepthRange { seg: s @ None, .. } => {
            *s = Some(seg);
            true
        }
        _ => false,
    }
}

/// The undo step that fills masks a quick model made in the background.
pub const QUICK_LABEL: &str = "AI Mask";

/// See [`Session::quick_start`].
pub(crate) enum QuickStart {
    Now(Quick),
    Later(Box<QuickRun>),
}

impl QuickStart {
    /// What the command reports about it (`aiWarning`: the model failed, the mask uses the
    /// classical estimate; `aiPending`: the model runs in the background).
    pub(crate) fn report(&self, out: &mut serde_json::Value) {
        match self {
            QuickStart::Now(Quick::Failed(why)) => out["aiWarning"] = serde_json::json!(format!("{why}; the mask uses its estimate")),
            QuickStart::Later(_) => out["aiPending"] = serde_json::json!(true),
            QuickStart::Now(_) => {}
        }
    }
}

impl Session {
    /// Recompute only incoming AI selections on this photo. Existing target masks stay intact.
    /// In background mode the quick models run as one job and fill the masks when it is done.
    pub(crate) fn refresh_transferred_masks(&mut self, id: PhotoId, settings: &mut lightcraft_develop::DevelopSettings, first: usize) {
        let input_settings = settings.clone();
        let mut later = Vec::new();
        for mask in settings.masks.iter_mut().skip(first) {
            for (index, component) in mask.components.iter_mut().enumerate() {
                let shape = &mut component.shape;
                if self.segmenter.background {
                    if let Some(run) = self.quick_run(id, shape, Some(&input_settings)) {
                        later.push((mask.id, index, run));
                    }
                } else if let Quick::Failed(why) = self.quick_segment_with_settings(id, shape, Some(&input_settings)) {
                    log::warn!("{QUICK_LABEL}: {why}");
                }
                match shape {
                    MaskShape::Object { hint, exclude, seg, detail, .. } => {
                        *seg = None;
                        detail.clear();
                        if !hint.is_empty() {
                            *seg = self
                                .segment_clicks(
                                    id,
                                    &hint
                                        .iter()
                                        .map(|p| crate::segment::Click { at: *p, include: true })
                                        .chain(exclude.iter().map(|p| crate::segment::Click { at: *p, include: false }))
                                        .collect::<Vec<_>>(),
                                )
                                .ok();
                        }
                    }
                    MaskShape::Prompt { text, seg, detail, .. } => {
                        *seg = self.segment_text(id, text).ok().flatten();
                        detail.clear();
                    }
                    _ => {}
                }
            }
        }
        self.quick_segment_later(id, later, false);
    }

    pub(crate) fn transfer_settings(
        &mut self,
        id: PhotoId,
        current: &lightcraft_develop::DevelopSettings,
        partial: &serde_json::Value,
    ) -> lightcraft_develop::DevelopSettings {
        let mut partial = partial.clone();
        if let Some(object) = partial.as_object_mut() {
            lightcraft_develop::presets::strip_photo_bound(object);
        }
        let mut settings = lightcraft_develop::apply_partial(current, &partial, 1.0);
        self.resolve_transferred_wb(id, &partial, &mut settings);
        if partial.get("masks").is_some() {
            self.refresh_transferred_masks(id, &mut settings, 0);
        }
        if partial.get("geometry").is_some() {
            self.queue_upright(id, &settings);
        }
        settings
    }

    /// Fills a Subject / Background / Sky / Depth Range shape's segmentation from the quick
    /// models when one is installed, here and now (other shapes, or no model: unchanged).
    pub fn quick_segment(&mut self, id: PhotoId, shape: &mut MaskShape) -> Quick {
        let settings = self.segment_key(id).map(|(_, settings)| settings);
        self.quick_segment_with_settings(id, shape, settings.as_ref())
    }

    pub(crate) fn quick_segment_with_settings(
        &mut self,
        id: PhotoId,
        shape: &mut MaskShape,
        settings: Option<&lightcraft_develop::DevelopSettings>,
    ) -> Quick {
        let Some(run) = self.quick_run(id, shape, settings) else { return Quick::Nothing };
        match run.run(&std::sync::atomic::AtomicBool::new(false)) {
            Ok(Some(seg)) => {
                fill(shape, seg);
                Quick::Filled
            }
            Ok(None) => Quick::Nothing,
            Err(why) => {
                log::warn!("{QUICK_LABEL}: {why}");
                Quick::Failed(why)
            }
        }
    }

    /// How a new shape of `mask.add` / `mask.addComponent` gets its segmentation: here and now, or
    /// (in background mode, as the app runs) later as a job, leaving the command's thread free.
    pub(crate) fn quick_start(&mut self, id: PhotoId, shape: &mut MaskShape) -> QuickStart {
        if !self.segmenter.background {
            return QuickStart::Now(self.quick_segment(id, shape));
        }
        let settings = self.segment_key(id).map(|(_, settings)| settings);
        self.quick_run(id, shape, settings.as_ref()).map_or(QuickStart::Now(Quick::Nothing), |run| QuickStart::Later(Box::new(run)))
    }

    /// The quick-model run that would fill `shape` (`None`: not such a shape, no model installed
    /// for it, or the photo can't be rendered).
    pub(crate) fn quick_run(&mut self, id: PhotoId, shape: &MaskShape, settings: Option<&lightcraft_develop::DevelopSettings>) -> Option<QuickRun> {
        let kind = Kind::of(shape)?;
        let dir = self.quick_seg_dir.clone()?;
        if !li_seg::available(kind.group(), &dir) {
            return None;
        }
        let mut settings = settings?.clone();
        settings.masks.clear();
        let render = self.preview_job(id, INPUT_EDGE, INPUT_EDGE, false, &settings)?;
        Some(QuickRun {
            geometry: mask_geometry(&settings),
            source: crate::enhance::source_hash(self.catalog.photo(id)?),
            raw: crate::enhance::denoise::source_options(&settings),
            shape: shape.clone(),
            kind,
            invert: matches!(shape, MaskShape::Background { .. }),
            dir,
            render,
        })
    }

    /// Runs `runs` (component `.1` of mask `.0` on photo `id`) as one background job, after any
    /// other quick-model job; the session fills the masks when it is done ([`QUICK_LABEL`]).
    /// `new_masks`: the masks were just added ([`crate::enhance::MaskFill::new_mask`]).
    pub(crate) fn quick_segment_later(&mut self, id: PhotoId, runs: Vec<(u32, usize, QuickRun)>, new_masks: bool) {
        if runs.is_empty() {
            return;
        }
        let work: crate::enhance::Work = Box::new(move |ctl| {
            ctl.report(0.0, "Waiting for the AI model");
            let _turn = QUICK_TURN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let total = runs.len();
            let mut fills = Vec::with_capacity(total);
            for (i, (mask, comp, run)) in runs.into_iter().enumerate() {
                ctl.check()?;
                ctl.report(i as f32 / total as f32, "Finding the mask");
                fills.push(crate::enhance::MaskFill {
                    geometry: run.geometry.clone(),
                    source: run.source.clone(),
                    raw: run.raw,
                    shape: run.shape.clone(),
                    mask,
                    comp,
                    seg: run.run(ctl.cancel_flag()),
                    new_mask: new_masks,
                });
            }
            Ok(crate::enhance::Outcome::Masks(fills))
        });
        self.enhance.spawn(id, crate::enhance::JobKind::Masks, QUICK_LABEL, false, work);
    }

    /// Fills the masks of photo `id` that a background quick job made, on the masks as they are
    /// now (a mask removed or changed meanwhile is left alone). The models' errors.
    pub(crate) fn apply_mask_fills(&mut self, id: PhotoId, fills: Vec<crate::enhance::MaskFill>) -> (bool, Vec<String>) {
        let Some(d) = self.develop_of(id) else { return (false, vec![]) };
        let source = self.catalog.photo(id).map(|p| crate::enhance::source_hash(p));
        let raw = crate::enhance::denoise::source_options(&d);
        let geometry = mask_geometry(&d);
        let mut d = (*d).clone();
        let (mut changed, mut errors) = (false, Vec::new());
        for f in fills {
            if source.as_ref() != Some(&f.source) || raw != f.raw || geometry != f.geometry {
                continue;
            }
            let Some(m) = d.masks.iter_mut().find(|m| m.id == f.mask) else { continue };
            let single = m.components.len() == 1;
            let Some(c) = m.components.get_mut(f.comp) else { continue };
            // A depth map is independent of its selected distance band.
            let depth = matches!((&c.shape, &f.shape), (MaskShape::DepthRange { seg: None, .. }, MaskShape::DepthRange { seg: None, .. }));
            if c.shape != f.shape && !depth {
                continue;
            }
            match f.seg {
                Ok(Some(seg)) => {
                    if fill(&mut c.shape, seg) {
                        changed = true;
                        if f.new_mask && single && m.refine == 0.0 {
                            m.refine = MODEL_EDGE_REFINE;
                        }
                    }
                }
                Ok(None) => {}
                Err(why) if why == crate::enhance::CANCELLED => {}
                Err(why) => errors.push(format!("{why}; the mask keeps its estimate")),
            }
        }
        if changed && let Err(e) = self.set_develop(id, d, QUICK_LABEL) {
            errors.push(e.to_string());
            changed = false;
        }
        (changed, errors)
    }
}

/// A `w × h` probability map as a `side`² logit grid over the whole photo (`invert`: 1 − p). The
/// probabilities are area-averaged into the grid's cells before they become logits, so a thin
/// structure keeps its share of a cell instead of being sampled in or out.
pub fn to_segmask(prob: &[f32], w: usize, h: usize, invert: bool, side: usize) -> SegMask {
    let grid = li_raster::GrayFrame { width: w, height: h, data: prob.to_vec() }.resize(side, side, Filter::Box);
    let logits: Vec<f32> = grid
        .data
        .iter()
        .map(|p| {
            let p = if invert { 1.0 - p } else { *p }.clamp(1e-6, 1.0 - 1e-6);
            (p / (1.0 - p)).ln()
        })
        .collect();
    SegMask::from_logits(side, &logits)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stored grid keeps the probabilities' sides: high where the map is high, low elsewhere,
    /// and Background is its complement.
    #[test]
    fn probabilities_become_a_logit_grid() {
        let (w, h) = (40, 20);
        let prob: Vec<f32> = (0..w * h).map(|i| if i % w < w / 2 { 0.95 } else { 0.05 }).collect();
        for invert in [false, true] {
            let seg = to_segmask(&prob, w, h, invert, GRID);
            assert_eq!(seg.side as usize, GRID);
            let logits = seg.logits().expect("decodes");
            let (left, right) = (logits[GRID * GRID / 2 + 10], logits[GRID * GRID / 2 + GRID - 10]);
            if invert {
                assert!(left < 0.0 && right > 0.0, "{left} {right}");
            } else {
                assert!(left > 0.0 && right < 0.0, "{left} {right}");
            }
        }
    }

    /// A sky over a sloped roofline at 3600 × 2400 (sky at 4.0, wall at 0.05), what a 1024 px model
    /// sees of it (the sky's share of each of its pixels, or with `soft` a logistic edge that many
    /// model pixels wide) and where each column's 50 % sky contour should be.
    fn roofline(soft: Option<f32>) -> (li_raster::RgbFrame, Vec<f32>, (usize, usize), impl Fn(f32) -> f32) {
        let (w, h) = (3600usize, 2400usize);
        let edge = move |x: f32| 0.42 * h as f32 + 0.06 * x;
        let img = li_raster::RgbFrame::from_fn(w, h, |x, y| if (y as f32 + 0.5) < edge(x as f32 + 0.5) { [4.0; 3] } else { [0.05; 3] });
        let (mw, mh) = (1024usize, 683usize);
        let (sx, sy) = (w as f32 / mw as f32, h as f32 / mh as f32);
        let prob = (0..mw * mh)
            .map(|i| {
                let (x, y) = ((i % mw) as f32 * sx, (i / mw) as f32 * sy);
                match soft {
                    Some(soft) => 1.0 / (1.0 + (-(edge(x + sx / 2.0) - y - sy / 2.0) / (soft * sy)).exp()),
                    None => {
                        (0..16).filter(|k| y + (*k / 4) as f32 * sy / 4.0 + sy / 8.0 < edge(x + (*k % 4) as f32 * sx / 4.0 + sx / 8.0)).count() as f32
                            / 16.0
                    }
                }
            })
            .collect();
        (img, prob, (mw, mh), edge)
    }

    /// The mean alpha error within 20 px of the roofline, and the farthest any column's 50 %
    /// contour lies from it.
    fn edge_error(alpha: &li_raster::GrayFrame, edge: &impl Fn(f32) -> f32) -> (f32, f32) {
        let (mut sum, mut n, mut worst) = (0.0f32, 0usize, 0.0f32);
        for x in (40..alpha.width - 40).step_by(7) {
            let e = edge(x as f32 + 0.5);
            let column = |y: usize| alpha.get(x, y);
            for y in (e as usize).saturating_sub(20)..(e as usize + 20).min(alpha.height) {
                let truth = (e - y as f32).clamp(0.0, 1.0);
                sum += (column(y) - truth).abs();
                n += 1;
            }
            let contour = (1..alpha.height).find(|&y| column(y) < 0.5).map_or(alpha.height as f32, |y| {
                let (a, b) = (column(y - 1), column(y));
                y as f32 - 0.5 + (a - 0.5) / (a - b).max(1e-6)
            });
            worst = worst.max((contour - e).abs());
        }
        (sum / n.max(1) as f32, worst)
    }

    #[test]
    fn model_masks_follow_a_roofline_at_full_resolution() {
        use lightcraft_develop::{Mask, MaskComponent, MaskOp};
        let mask = |shape: MaskShape, refine: f64| Mask {
            components: vec![MaskComponent { name: None, op: MaskOp::Add, invert: false, shape }],
            refine,
            ..Default::default()
        };
        // a sky model's soft edge (3 model pixels) on the sky grid, and a sharp subject model on the
        // subject grid, each against what masks stored before: a 256 grid drawn without refinement
        for (soft, side, max_ratio) in [(Some(3.0), GRID, 0.6), (None, SUBJECT_GRID, 0.7)] {
            let (img, prob, (mw, mh), edge) = roofline(soft);
            let (w, h) = (img.width, img.height);
            let log_l = img.map(lightcraft_pipeline::local::log_lum);
            let frame = lightcraft_pipeline::geometry::Frame::new(w, h, &Default::default(), true);
            let error = |seg: SegMask, refine: f64| {
                let shape = if soft.is_some() { MaskShape::Sky { seg: Some(seg) } } else { MaskShape::Subject { seg: Some(seg) } };
                edge_error(&lightcraft_pipeline::masks::evaluate_one(&mask(shape, refine), &frame, w, h, &img, &log_l, 0.0), &edge)
            };
            let before = error(to_segmask(&prob, mw, mh, false, GRID), 0.0);
            let (mae, worst) = error(to_segmask(&prob, mw, mh, false, side), MODEL_EDGE_REFINE);
            assert!(mae < before.0 * max_ratio, "soft {soft:?}, grid {side}: band error {mae} vs {} before", before.0);
            assert!(worst < 3.0 && worst < before.1, "soft {soft:?}, grid {side}: contour off by {worst} px vs {} before", before.1);
        }
    }

    #[test]
    fn new_model_masks_refine_their_edges_and_other_masks_do_not() {
        use serde_json::json;
        let mut s = Session::with_demo();
        let id = s.active().unwrap();
        let prob: Vec<f32> = (0..64 * 32).map(|i| if i / 64 < 16 { 0.95 } else { 0.05 }).collect();
        let seg = serde_json::to_value(to_segmask(&prob, 64, 32, false, GRID)).unwrap();
        let last = |s: &Session| s.develop_of(id).unwrap().masks.last().cloned().unwrap();
        s.execute("mask.add", &json!({"kind": "sky", "seg": seg})).unwrap();
        assert!(matches!(last(&s).components[0].shape, MaskShape::Sky { seg: Some(_) }));
        assert_eq!(last(&s).refine, MODEL_EDGE_REFINE);
        s.execute("mask.add", &json!({"kind": "radial"})).unwrap();
        assert_eq!(last(&s).refine, 0.0, "only model-made masks");
        s.execute("mask.add", &json!({"kind": "sky"})).unwrap();
        assert_eq!(last(&s).refine, 0.0, "no model, no segmentation: the classical estimate as before");
        // an empty layer's first model-made shape is refined; a shape added to another isn't changed
        s.execute("mask.add", &json!({"kind": "empty"})).unwrap();
        let empty = last(&s).id;
        s.execute("mask.addComponent", &json!({"id": empty, "kind": "subject", "seg": seg})).unwrap();
        assert_eq!(last(&s).refine, MODEL_EDGE_REFINE);
        s.execute("mask.add", &json!({"kind": "linear"})).unwrap();
        let linear = last(&s).id;
        s.execute("mask.addComponent", &json!({"id": linear, "kind": "subject", "seg": seg})).unwrap();
        assert_eq!(last(&s).refine, 0.0);
    }

    #[test]
    fn without_models_masks_keep_the_classical_estimate() {
        let mut s = Session::new();
        let mut shape = MaskShape::Subject { seg: None };
        assert_eq!(s.quick_segment(PhotoId(1), &mut shape), Quick::Nothing);
        assert_eq!(shape, MaskShape::Subject { seg: None });
    }

    /// A models folder whose custom sky model is a broken file.
    fn broken_sky_model() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("li-quickseg-broken-{}-{:?}", std::process::id(), std::thread::current().id()));
        let custom = li_seg::custom_dir(&dir);
        std::fs::create_dir_all(&custom).unwrap();
        std::fs::write(custom.join("sky-broken.onnx"), b"not a model").unwrap();
        std::fs::write(custom.join("custom.txt"), "sky\tsky-broken.onnx\tbroken.onnx\t32\timagenet\t1\t0\tlogits\n").unwrap();
        dir
    }

    #[test]
    fn a_failing_model_is_reported_and_the_mask_keeps_its_estimate() {
        use serde_json::json;
        let dir = broken_sky_model();
        let mut s = Session::with_demo();
        s.quick_seg_dir = Some(dir.clone());
        let id = s.active().unwrap();
        let out = s.execute("mask.add", &json!({"kind": "sky"})).unwrap();
        let warning = out["aiWarning"].as_str().expect("the failure is reported");
        assert!(warning.contains("broken.onnx") && warning.contains("estimate"), "{warning}");
        let sky = |s: &Session| s.develop_of(id).unwrap().masks.last().unwrap().components[0].shape.clone();
        assert_eq!(sky(&s), MaskShape::Sky { seg: None });
        // in the app the model runs in the background: the mask is there at once, the failure follows
        s.segmenter.background = true;
        let out = s.execute("mask.add", &json!({"kind": "sky"})).unwrap();
        assert_eq!(out["aiPending"], json!(true));
        assert_eq!(sky(&s), MaskShape::Sky { seg: None });
        while s.enhance.busy() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let polled = s.enhance_poll();
        assert!(polled.errors.iter().any(|e| e.starts_with(QUICK_LABEL) && e.contains("broken.onnx")), "{:?}", polled.errors);
        assert_eq!(sky(&s), MaskShape::Sky { seg: None });
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn background_mask_results_leave_changed_shapes_alone() {
        use crate::enhance::MaskFill;
        use serde_json::json;
        let mut s = Session::with_demo();
        let id = s.active().unwrap();
        s.execute("mask.add", &json!({"kind": "sky"})).unwrap();
        let mask = s.develop_of(id).unwrap().masks.last().unwrap().id;
        let pending = MaskFill {
            geometry: mask_geometry(&s.develop_of(id).unwrap()),
            source: crate::enhance::source_hash(s.catalog.photo(id).unwrap()),
            raw: crate::enhance::denoise::source_options(&s.develop_of(id).unwrap()),
            shape: MaskShape::Sky { seg: None },
            mask,
            comp: 0,
            seg: Ok(Some(to_segmask(&[0.9; 64], 8, 8, false, GRID))),
            new_mask: true,
        };
        s.execute("mask.update", &json!({"id": mask, "shape": {"kind": "subject"}})).unwrap();
        let (changed, errors) = s.apply_mask_fills(id, vec![pending]);
        assert!(!changed && errors.is_empty(), "late sky coverage must not become subject coverage");
        assert_eq!(s.develop_of(id).unwrap().masks[0].components[0].shape, MaskShape::Subject { seg: None });
    }

    #[test]
    fn background_mask_results_leave_replaced_sources_alone() {
        use crate::enhance::MaskFill;
        use lightcraft_catalog::Op;
        use serde_json::json;
        let mut s = Session::with_demo();
        let id = s.active().unwrap();
        s.execute("mask.add", &json!({"kind": "sky"})).unwrap();
        let photo = s.catalog.photo(id).unwrap().clone();
        let pending = MaskFill {
            geometry: mask_geometry(&s.develop_of(id).unwrap()),
            source: crate::enhance::source_hash(&photo),
            raw: crate::enhance::denoise::source_options(&photo.develop),
            shape: MaskShape::Sky { seg: None },
            mask: photo.develop.masks.last().unwrap().id,
            comp: 0,
            seg: Ok(Some(to_segmask(&[0.9; 64], 8, 8, false, GRID))),
            new_mask: true,
        };
        s.catalog
            .apply(Op::SetContent {
                id,
                width: photo.width,
                height: photo.height,
                file_size: photo.file_size,
                content_hash: Some("replacement".into()),
                preview_only: None,
            })
            .unwrap();
        let (changed, errors) = s.apply_mask_fills(id, vec![pending]);
        assert!(!changed && errors.is_empty(), "late coverage belongs to the original source");
        assert_eq!(s.develop_of(id).unwrap().masks[0].components[0].shape, MaskShape::Sky { seg: None });
    }

    #[test]
    fn background_mask_results_leave_changed_mapping_and_decoding_alone() {
        use crate::enhance::MaskFill;
        use serde_json::json;
        for change in ["orientation", "geometry", "lens", "raw"] {
            let mut s = Session::with_demo();
            let id = s.active().unwrap();
            s.execute("mask.add", &json!({"kind": "sky"})).unwrap();
            let original = s.develop_of(id).unwrap();
            let pending = MaskFill {
                geometry: mask_geometry(&original),
                source: crate::enhance::source_hash(s.catalog.photo(id).unwrap()),
                raw: crate::enhance::denoise::source_options(&original),
                shape: MaskShape::Sky { seg: None },
                mask: original.masks[0].id,
                comp: 0,
                seg: Ok(Some(to_segmask(&[0.9; 64], 8, 8, false, GRID))),
                new_mask: true,
            };
            let mut edited = (*original).clone();
            match change {
                "orientation" => edited.orientation = lightcraft_geom::Orientation::Rotate90,
                "geometry" => edited.geometry.vertical = 12.0,
                "lens" => edited.optics.distortion = 20.0,
                "raw" => edited.raw.demosaic = lightcraft_develop::Demosaic::Rcd,
                _ => unreachable!(),
            }
            s.set_develop(id, edited, "Edit").unwrap();
            let (changed, errors) = s.apply_mask_fills(id, vec![pending]);
            assert!(!changed && errors.is_empty(), "late coverage must not survive changed {change}");
            assert_eq!(s.develop_of(id).unwrap().masks[0].components[0].shape, MaskShape::Sky { seg: None });
        }
    }

    #[test]
    fn background_mask_results_keep_crop_tone_depth_and_completed_upright_edits() {
        use crate::enhance::MaskFill;
        use serde_json::json;
        for change in ["crop", "tone", "depth", "upright"] {
            let mut s = Session::with_demo();
            let id = s.active().unwrap();
            s.execute("mask.add", &json!({"kind": if change == "depth" { "depth" } else { "sky" }})).unwrap();
            let mut original = (*s.develop_of(id).unwrap()).clone();
            if change == "upright" {
                original.geometry.upright = lightcraft_develop::Upright::Auto;
                s.set_develop(id, original.clone(), "Auto Upright").unwrap();
            }
            let seg = to_segmask(&[0.9; 64], 8, 8, false, GRID);
            let pending = MaskFill {
                geometry: mask_geometry(&original),
                source: crate::enhance::source_hash(s.catalog.photo(id).unwrap()),
                raw: crate::enhance::denoise::source_options(&original),
                shape: original.masks[0].components[0].shape.clone(),
                mask: original.masks[0].id,
                comp: 0,
                seg: Ok(Some(seg.clone())),
                new_mask: true,
            };
            let mut edited = original;
            match change {
                "crop" => edited.crop.geometry.rect = lightcraft_geom::Rect::new(0.1, 0.1, 0.9, 0.9),
                "tone" => {
                    edited.light.exposure = 1.0;
                    edited.masks[0].opacity = 40.0;
                }
                "depth" => {
                    if let MaskShape::DepthRange { lo, hi, .. } = &mut edited.masks[0].components[0].shape {
                        *lo = 0.2;
                        *hi = 0.8;
                    }
                }
                "upright" => edited.geometry.upright_transform = Some(lightcraft_geom::Homography::IDENTITY),
                _ => unreachable!(),
            }
            s.set_develop(id, edited.clone(), "Edit").unwrap();
            let (changed, errors) = s.apply_mask_fills(id, vec![pending]);
            assert!(changed && errors.is_empty(), "a {change} edit does not invalidate coverage");
            fill(&mut edited.masks[0].components[0].shape, seg);
            edited.masks[0].refine = MODEL_EDGE_REFINE;
            assert_eq!(*s.develop_of(id).unwrap(), edited, "the user's {change} edit must survive filling");
        }
    }

    #[test]
    fn background_masks_fill_only_the_shapes_they_were_made_for() {
        use crate::enhance::MaskFill;
        use serde_json::json;
        let mut s = Session::with_demo();
        let id = s.active().unwrap();
        for kind in ["sky", "subject", "radial"] {
            s.execute("mask.add", &json!({"kind": kind})).unwrap();
        }
        let ids: Vec<u32> = s.develop_of(id).unwrap().masks.iter().map(|m| m.id).collect();
        let seg = to_segmask(&[0.9; 64], 8, 8, false, GRID);
        let source = crate::enhance::source_hash(s.catalog.photo(id).unwrap());
        let raw = crate::enhance::denoise::source_options(&s.develop_of(id).unwrap());
        let fills = [ids[0], ids[1], ids[2], 999]
            .map(|mask| MaskFill {
                geometry: mask_geometry(&s.develop_of(id).unwrap()),
                source: source.clone(),
                raw,
                shape: s
                    .develop_of(id)
                    .unwrap()
                    .masks
                    .iter()
                    .find(|m| m.id == mask)
                    .map(|m| m.components[0].shape.clone())
                    .unwrap_or(MaskShape::Sky { seg: None }),
                mask,
                comp: 0,
                seg: Ok(Some(seg.clone())),
                new_mask: true,
            })
            .to_vec();
        let (changed, errors) = s.apply_mask_fills(id, fills);
        assert!(changed && errors.is_empty());
        let d = s.develop_of(id).unwrap();
        assert_eq!(d.masks[0].components[0].shape, MaskShape::Sky { seg: Some(seg.clone()) });
        assert_eq!(d.masks[1].components[0].shape, MaskShape::Subject { seg: Some(seg.clone()) });
        assert!(d.masks[..2].iter().all(|m| m.refine == MODEL_EDGE_REFINE), "filled model masks get edge refinement");
        assert!(!is_model_mask(&d.masks[2].components[0].shape) && d.masks[2].refine == 0.0, "a radial mask is not a model's");
        // a mask that has its segmentation already (filled meanwhile) is left alone
        let other = to_segmask(&[0.1; 64], 8, 8, false, GRID);
        let (changed, _) = s.apply_mask_fills(
            id,
            vec![MaskFill {
                geometry: mask_geometry(&s.develop_of(id).unwrap()),
                source: source.clone(),
                raw,
                shape: MaskShape::Sky { seg: None },
                mask: ids[0],
                comp: 0,
                seg: Ok(Some(other)),
                new_mask: true,
            }],
        );
        assert!(!changed);
        assert_eq!(s.develop_of(id).unwrap().masks[0].components[0].shape, MaskShape::Sky { seg: Some(seg.clone()) });
        // a mask pasted, synced or applied from a preset keeps its own Refine Edges
        s.execute("mask.add", &json!({"kind": "sky"})).unwrap();
        let pasted = s.develop_of(id).unwrap().masks.last().unwrap().id;
        let (changed, _) = s.apply_mask_fills(
            id,
            vec![MaskFill {
                geometry: mask_geometry(&s.develop_of(id).unwrap()),
                source: source.clone(),
                raw,
                shape: MaskShape::Sky { seg: None },
                mask: pasted,
                comp: 0,
                seg: Ok(Some(seg)),
                new_mask: false,
            }],
        );
        assert!(changed);
        let d = s.develop_of(id).unwrap();
        let m = d.masks.last().unwrap();
        assert!(is_model_mask(&m.components[0].shape) && m.refine == 0.0, "a transferred mask is filled but keeps refine {}", m.refine);
    }
}
