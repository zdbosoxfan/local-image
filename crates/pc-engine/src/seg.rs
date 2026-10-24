//! local-image: learned subject segmentation (`li-seg`, IS-Net or a custom model on the CPU) behind
//! Select › Subject, Remove Background and the Object Selection tool's click mode. It is used when
//! a segmentation model is installed (Local AI window › Selection models); otherwise those commands
//! keep PhotoCraft's classical saliency + GrabCut.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use photocraft_algo::selection::{self as sel, Region};
use photocraft_doc::{Document, LayerId};
use photocraft_geom::Rect;
use serde_json::Value;

/// Where segmentation models live: `<AI model folder>/segmentation/`.
pub fn models_dir() -> PathBuf {
    li_ai::AiSettings::cached_model_dir()
}

/// The installed model the commands would use, if any.
pub fn installed() -> Option<li_seg::InUse> {
    li_seg::Group::Subject.in_use(&models_dir())
}

/// `"engine"`: `auto` (learned when installed), `classic` or `learned` (an error without a model).
pub fn wanted(p: &Value) -> std::result::Result<bool, String> {
    match p.get("engine").and_then(Value::as_str).unwrap_or("auto") {
        "classic" => Ok(false),
        "learned" if SubjectModel::current().is_none() => Err("no selection model is installed (Edit › Preferences › Local AI…)".into()),
        "learned" => Ok(true),
        _ => Ok(SubjectModel::current().is_some()),
    }
}

/// 8-bit RGBA of `layer` (or the composite with `all_layers`, or when it has no pixels).
fn rgba8(doc: &Document, layer: Option<LayerId>, all_layers: bool) -> (Rect, Vec<u8>) {
    let area = doc.bounds();
    let px: Vec<[u8; 4]> = match (all_layers, layer.and_then(|id| doc.layer(id)).and_then(|l| l.surface())) {
        (false, Some(surf)) => sel::rgba8_image(surf, area),
        _ => {
            let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            photocraft_compose::render(doc, area).px.iter().map(|p| p.map(q)).collect()
        }
    };
    (area, px.into_iter().flatten().collect())
}

/// The subject model a command uses. It is resolved on the caller's thread when the command
/// starts (so a test's [`mock_subject_model_on_thread`] reaches the job's worker) and moved into
/// the job; the worker loads the model itself.
#[derive(Clone)]
pub struct SubjectModel(ModelKind);

#[derive(Clone)]
enum ModelKind {
    Installed(Box<li_seg::ModelSnapshot>),
    Mock(Arc<MockModel>),
}

impl ModelKind {
    fn is_current(&self) -> bool {
        match self {
            Self::Installed(snapshot) => snapshot.is_current(),
            Self::Mock(_) => true,
        }
    }
}

impl PartialEq for SubjectModel {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (ModelKind::Installed(a), ModelKind::Installed(b)) => a == b,
            (ModelKind::Mock(a), ModelKind::Mock(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl SubjectModel {
    /// The model the commands would use now (a test's mock on this thread first), if any.
    pub fn current() -> Option<Self> {
        if let Some(m) = MOCK.with(|m| m.borrow().clone()) {
            return Some(Self(ModelKind::Mock(m)));
        }
        li_seg::snapshot(li_seg::Group::Subject, &models_dir()).map(|snapshot| Self(ModelKind::Installed(Box::new(snapshot))))
    }

    /// Foreground probability (0–1) at the image's size, or `Ok(None)` when the model sees no clear
    /// subject. `Err` when `cancel` was set (the model stops at its next stage) or the model failed
    /// to load or run (logged): neither is an answer to remember for these pixels.
    fn predict(&self, rgba: &[u8], w: usize, h: usize, cancel: &AtomicBool) -> std::result::Result<Option<Vec<f32>>, NoAnswer> {
        match &self.0 {
            ModelKind::Installed(snapshot) => {
                let seg = match snapshot.segmenter() {
                    Ok(Some(seg)) => seg,
                    Ok(None) => return Err(NoAnswer),
                    Err(e) => return Err(failed("subject", &e)),
                };
                seg.predict_with_cancel(rgba, w, h, cancel).map_err(|e| if li_seg::is_cancelled(&e) { NoAnswer } else { failed("subject", &e) })
            }
            ModelKind::Mock(m) => m.predict(rgba, w, h, cancel).map_err(|_| NoAnswer),
        }
    }
}

/// A model run that gave no answer: cancelled, or the model failed.
struct NoAnswer;

fn failed(what: &str, e: &anyhow::Error) -> NoAnswer {
    log::warn!("The {what} model failed: {e:#}");
    NoAnswer
}

/// Headless-test stand-in for the subject model: counts its runs and can hold each run until a
/// gate opens (so a test can see the job pending, cancel it or edit the document meanwhile).
#[doc(hidden)]
pub struct MockModel {
    calls: AtomicUsize,
    gate: Option<Arc<AtomicBool>>,
    #[allow(clippy::type_complexity)]
    map: Box<dyn Fn(usize, usize) -> Option<Vec<f32>> + Send + Sync>,
}

impl MockModel {
    /// A model whose map for a `w`×`h` image is `map(w, h)` (row-major, 0–1; `None`: no clear
    /// subject); `gate` (if any) holds every run until it is set.
    pub fn new(gate: Option<Arc<AtomicBool>>, map: impl Fn(usize, usize) -> Option<Vec<f32>> + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self { calls: AtomicUsize::new(0), gate, map: Box::new(map) })
    }

    /// How many times the model ran.
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn predict(&self, _rgba: &[u8], w: usize, h: usize, cancel: &AtomicBool) -> std::result::Result<Option<Vec<f32>>, lightcraft_sysmem::jobs::Cancelled> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = &self.gate {
            // Bounded, so a failed test can't leave a worker spinning forever. Like the real
            // model, a cancel stops the run while it waits.
            let t = std::time::Instant::now();
            while !gate.load(Ordering::SeqCst) && !cancel.load(Ordering::Relaxed) && t.elapsed() < std::time::Duration::from_secs(60) {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(lightcraft_sysmem::jobs::Cancelled);
        }
        Ok((self.map)(w, h))
    }
}

thread_local! {
    static MOCK: std::cell::RefCell<Option<Arc<MockModel>>> = const { std::cell::RefCell::new(None) };
}

/// Headless-test hook: commands started on this thread use `model` as the subject model
/// (`None` restores the installed one). Other threads (other tests) are unaffected.
#[doc(hidden)]
pub fn mock_subject_model_on_thread(model: Option<Arc<MockModel>>) {
    MOCK.with(|m| *m.borrow_mut() = model);
}

/// A cancel flag nobody sets, for callers that can't cancel.
static NEVER: AtomicBool = AtomicBool::new(false);

/// Foreground probability over the canvas, or `None` (no model, or no clear subject).
pub fn probability(doc: &Document, layer: Option<LayerId>, all_layers: bool) -> Option<(Rect, Vec<f32>)> {
    let p = probability_with(&SubjectModel::current()?, doc, layer, all_layers, &NEVER)?;
    Some((*p).clone())
}

/// The tight region of the non-zero coverage in `prob` (over `area`).
pub fn region(area: Rect, prob: &[f32]) -> Option<Region> {
    let w = area.width() as usize;
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for (i, v) in prob.iter().enumerate() {
        if *v >= 1.0 / 255.0 {
            let (x, y) = (i % w, i / w);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
    }
    if x0 == usize::MAX {
        return None;
    }
    let bw = x1 - x0 + 1;
    let mut mask = Vec::with_capacity(bw * (y1 - y0 + 1));
    for y in y0..=y1 {
        mask.extend(prob[y * w + x0..=y * w + x1].iter().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8));
    }
    Some(Region { bbox: Rect::new(area.x0 + x0 as i32, area.y0 + y0 as i32, area.x0 + x1 as i32 + 1, area.y0 + y1 as i32 + 1), mask })
}

/// Select Subject with the learned model.
pub fn subject(doc: &Document, layer: Option<LayerId>, all_layers: bool) -> Option<Region> {
    subject_with(&SubjectModel::current()?, doc, layer, all_layers, &NEVER)
}

/// Select Subject with `model`; `None` too when `cancel` stopped the model.
pub fn subject_with(model: &SubjectModel, doc: &Document, layer: Option<LayerId>, all_layers: bool, cancel: &AtomicBool) -> Option<Region> {
    let p = probability_with(model, doc, layer, all_layers, cancel)?;
    region(p.0, &p.1)
}

/// The object under document point `(x, y)` (Object Selection click mode): the connected part of
/// the salient map around the click.
pub fn object_at(doc: &Document, layer: Option<LayerId>, all_layers: bool, x: i32, y: i32) -> Option<Region> {
    object_at_with(&SubjectModel::current()?, doc, layer, all_layers, x, y, &NEVER)
}

/// [`object_at`] with `model`. Repeated clicks on the same pixels reuse the model's map; `None` too
/// when `cancel` stopped the model.
pub fn object_at_with(model: &SubjectModel, doc: &Document, layer: Option<LayerId>, all_layers: bool, x: i32, y: i32, cancel: &AtomicBool) -> Option<Region> {
    if !doc.bounds().contains(x, y) {
        return None;
    }
    let p = probability_with(model, doc, layer, all_layers, cancel)?;
    let (area, prob) = (p.0, &p.1);
    let (w, h) = (area.width() as usize, area.height() as usize);
    let obj = li_seg::object_at(prob, w, h, (x - area.x0) as usize, (y - area.y0) as usize)?;
    region(area, &obj)
}

/// What a subject probability map was computed from: the model, the canvas and the sampled
/// pixels (the layer, or every layer for the composite). Compared by value; unchanged pixels
/// share their tiles, so the comparison is cheap. Selection and view changes don't touch it, so
/// an Object Selection click in Add mode doesn't invalidate the map for the next click.
#[derive(PartialEq)]
struct ProbKey {
    model: SubjectModel,
    area: Rect,
    layer: Option<photocraft_raster::Surface>,
    layers: Option<Vec<photocraft_doc::Layer>>,
}

impl ProbKey {
    fn new(model: &SubjectModel, doc: &Document, layer: Option<LayerId>, all_layers: bool) -> Self {
        let surf = layer.and_then(|id| doc.layer(id)).and_then(|l| l.surface()).filter(|_| !all_layers);
        Self { model: model.clone(), area: doc.bounds(), layer: surf.cloned(), layers: surf.is_none().then(|| doc.layers.clone()) }
    }
}

/// A computed map (or "no clear subject"), shared by Quick Selection's prior, Object Selection's
/// clicks, Select Subject and Remove Background. One entry: a 24 MP map is about 96 MB.
type SubjectMap = Option<Arc<(Rect, Vec<f32>)>>;
type ProbCache = Mutex<Option<(ProbKey, SubjectMap)>>;
static PROBABILITY: ProbCache = Mutex::new(None);

/// The cached map for these pixels: `Some(map)` on a hit (the map itself may be `None`: the model
/// found no clear subject), `None` when the model would have to run.
pub fn cached_probability(model: &SubjectModel, doc: &Document, layer: Option<LayerId>, all_layers: bool) -> Option<SubjectMap> {
    if !model.0.is_current() {
        return None;
    }
    let key = ProbKey::new(model, doc, layer, all_layers);
    lookup(&key)
}

fn lookup(key: &ProbKey) -> Option<SubjectMap> {
    lookup_in(&PROBABILITY, key)
}

fn lookup_in(cache: &ProbCache, key: &ProbKey) -> Option<SubjectMap> {
    let cache = cache.lock().unwrap_or_else(PoisonError::into_inner);
    cache.as_ref().filter(|(k, _)| k == key).map(|(_, v)| v.clone())
}

/// Foreground probability over the canvas from `model`: once per pixel state, later calls on the
/// same pixels reuse it. Runs the model on the calling thread (a job's worker); `cancel` (the job's
/// flag) stops the model at its next stage, and then nothing is cached and the result is `None`.
pub fn probability_with(model: &SubjectModel, doc: &Document, layer: Option<LayerId>, all_layers: bool, cancel: &AtomicBool) -> SubjectMap {
    if !model.0.is_current() {
        return None;
    }
    let key = ProbKey::new(model, doc, layer, all_layers);
    if let Some(hit) = lookup(&key) {
        return hit;
    }
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    let (area, rgba) = rgba8(doc, layer, all_layers);
    let (w, h) = (area.width() as usize, area.height() as usize);
    let Ok(prob) = model.predict(&rgba, w, h, cancel) else { return None };
    if !model.0.is_current() {
        return None;
    }
    let map = prob.map(|prob| Arc::new((area, prob)));
    // The lock is not held while the model runs.
    *PROBABILITY.lock().unwrap_or_else(PoisonError::into_inner) = Some((key, map.clone()));
    map
}

/// The sky model a command uses; like [`SubjectModel`] it is resolved on the caller's thread (so a
/// test's [`mock_sky_model_on_thread`] reaches the job's worker) and moved into the job.
#[derive(Clone)]
pub struct SkyModel(ModelKind);

impl SkyModel {
    /// The model the commands would use now (a test's mock on this thread first), if any.
    pub fn current() -> Option<Self> {
        if let Some(m) = SKY_MOCK.with(|m| m.borrow().clone()) {
            return Some(Self(ModelKind::Mock(m)));
        }
        li_seg::snapshot(li_seg::Group::Sky, &models_dir()).map(|snapshot| Self(ModelKind::Installed(Box::new(snapshot))))
    }
}

thread_local! {
    static SKY_MOCK: std::cell::RefCell<Option<Arc<MockModel>>> = const { std::cell::RefCell::new(None) };
}

/// Headless-test hook: commands started on this thread use `model` as the sky model (`None`
/// restores the installed one). Other threads (other tests) are unaffected.
#[doc(hidden)]
pub fn mock_sky_model_on_thread(model: Option<Arc<MockModel>>) {
    SKY_MOCK.with(|m| *m.borrow_mut() = model);
}

/// Sky probability over the canvas from `model` (resolved earlier, on the thread that started the
/// job), its edges snapped to the image with a colour-guided filter (the model sees a 256–512 px
/// copy). Stops the model at its next stage once `cancel` (a job's flag) is set (then `None`).
pub fn sky_probability_with(model: &SkyModel, doc: &Document, layer: Option<LayerId>, all_layers: bool, cancel: &AtomicBool) -> Option<(Rect, Vec<f32>)> {
    let (area, rgba) = rgba8(doc, layer, all_layers);
    let (w, h) = (area.width() as usize, area.height() as usize);
    let prob = match &model.0 {
        ModelKind::Mock(mock) => mock.predict(&rgba, w, h, cancel).ok()??,
        ModelKind::Installed(snapshot) => {
            let seg = snapshot.segmenter().map_err(|e| failed("sky", &e)).ok()??;
            seg.predict_sky_with_cancel(&rgba, w, h, cancel)
                .map_err(|e| {
                    if !li_seg::is_cancelled(&e) {
                        failed("sky", &e);
                    }
                })
                .ok()?
        }
    };
    if !model.0.is_current() {
        return None;
    }
    let guide = photocraft_algo::segment::RgbImage {
        w,
        h,
        px: rgba.as_chunks::<4>().0.iter().map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]).collect(),
    };
    let r = (w.max(h) / 160).clamp(2, 24);
    let refined = photocraft_algo::matting::guided_filter_color(&guide, &prob, r, 1e-3);
    Some((area, refined.into_iter().map(|v| v.clamp(0.0, 1.0)).collect()))
}

/// Shapes a probability map with Select › Sky's threshold (0–100, higher = stricter) and softness
/// (0–100, the width of the soft edge).
pub fn shape(prob: &mut [f32], threshold: f32, softness: f32) {
    let t = (threshold / 100.0).clamp(0.02, 0.98);
    let s = (softness / 100.0 * 0.5).clamp(0.01, 0.5);
    for v in prob.iter_mut() {
        *v = ((*v - (t - s / 2.0)) / s).clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_probability_cache_uses_captured_artifact_identity() {
        let doc = Document::with_background(
            "identity",
            photocraft_doc::Size::new(4, 4),
            photocraft_doc::ColorMode::Rgb,
            photocraft_doc::SampleType::U8,
            photocraft_doc::Color::WHITE,
        );
        let root = std::env::temp_dir().join(format!("review-probability-{}-{}", std::process::id(), doc.id.0));
        let mut models = Vec::new();
        for name in ["first", "second"] {
            let dir = root.join(name);
            let custom = dir.join("segmentation/custom");
            std::fs::create_dir_all(&custom).unwrap();
            std::fs::write(custom.join("subject-same.onnx"), b"not an ONNX plan").unwrap();
            std::fs::write(custom.join("custom.txt"), "subject\tsubject-same.onnx\tSame\t32\tisnet\t1\t0\n").unwrap();
            models.push(SubjectModel(ModelKind::Installed(Box::new(li_seg::snapshot(li_seg::Group::Subject, &dir).unwrap()))));
        }
        // Use an isolated cache to avoid racing the process-wide cache in parallel tests.
        // No-subject entries matter too: a different artifact must not inherit a negative hit.
        let first = ProbKey::new(&models[0], &doc, None, true);
        let second = ProbKey::new(&models[1], &doc, None, true);
        let cache = Mutex::new(Some((first, None)));
        assert!(matches!(lookup_in(&cache, &ProbKey::new(&models[0], &doc, None, true)), Some(None)));
        assert!(lookup_in(&cache, &second).is_none());
        std::fs::write(root.join("first/segmentation/custom/subject-same.onnx"), b"changed").unwrap();
        assert!(cached_probability(&models[0], &doc, None, true).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn region_is_the_tight_box_of_the_coverage() {
        let area = Rect::new(10, 20, 15, 24);
        let mut p = vec![0.0f32; 5 * 4];
        p[5 + 1] = 1.0;
        p[2 * 5 + 3] = 0.5;
        let r = region(area, &p).unwrap();
        assert_eq!(r.bbox, Rect::new(11, 21, 14, 23));
        assert_eq!(r.mask, vec![255, 0, 0, 0, 0, 128]);
        assert!(region(area, &[0.0; 20]).is_none());
    }

    #[test]
    fn classic_engine_needs_no_model() {
        assert_eq!(wanted(&serde_json::json!({"engine": "classic"})), Ok(false));
    }
}
