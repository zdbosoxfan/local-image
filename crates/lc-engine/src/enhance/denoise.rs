//! AI Denoise (Lightroom's Denoise): a one-shot background job runs the RawNIND model
//! (`li_seg::denoise`) over the photo's full-size demosaiced source (linear Rec.2020, before lens
//! correction) and stores the result ([`super::store`], keyed by the photo's content and the
//! model). Rendering then uses that result in place of the source, mixed with it by the Denoise
//! amount ([`denoised_source`]), so the amount is a live slider; when the stored result is
//! missing, the photo renders from its own source and Develop offers to run Denoise again.

use std::sync::{Arc, Mutex, Weak};

use lightcraft_catalog::PhotoId;
use lightcraft_preview::Hasher128;
use lightcraft_raster::Rgb32f;
use lightcraft_raster::resample::{Filter, resize};

use super::store::{self, Kind};
use super::{CANCELLED, Job, JobCtl, JobKind, Outcome};
use crate::Session;
use crate::media::SourceLevel;

/// Long edge of the stored preview-size copy (the loupe's source level).
pub const PREVIEW_EDGE: usize = 2560;

/// A denoiser: (pixels, width, height, progress(done, total) → keep going?) → denoised pixels.
pub type DenoiseFn = Arc<dyn Fn(&[[f32; 3]], usize, usize, &(dyn Fn(usize, usize) -> bool + Sync)) -> Result<Vec<[f32; 3]>, String> + Send + Sync>;

/// A finished Denoise: where its result is stored, for which photo content, by which model.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct DenoiseDone {
    pub key: String,
    pub source: String,
    pub model: String,
}

/// The model identity in result keys: its id and the first digits of its checksum (a new model
/// file gives new keys).
pub fn model_tag() -> String {
    match li_seg::spec(li_seg::denoise::DENOISE_ID).map(|s| s.task) {
        Some(li_seg::Task::Denoise { inner_sha256, .. }) => format!("{}@{}", li_seg::denoise::DENOISE_ID, &inner_sha256[..8]),
        _ => li_seg::denoise::DENOISE_ID.to_owned(),
    }
}

/// The content hash a photo's AI results are keyed by.
pub fn source_hash(p: &lightcraft_catalog::Photo) -> String {
    Hasher128::new().str(&crate::media::content_key(p)).finish().to_string()
}

/// Store key of the Denoise result of photo content `source` by `model`.
pub fn result_key(source: &str, model: &str) -> String {
    store::key_of(&["denoise", source, model])
}

/// Store key of the result's preview-size copy.
pub fn preview_key(key: &str) -> String {
    format!("{key}-{PREVIEW_EDGE}")
}

/// Whether photo `id` can be denoised: a raw (scene-referred) source, not a Camera Raw Filter
/// layer. `Err` says why not.
pub fn can_denoise(s: &Session, id: PhotoId) -> Result<(), String> {
    let p = s.catalog.photo(id).ok_or("no such photo")?;
    if s.is_ephemeral(id) {
        return Err("AI Denoise needs the raw photo; it isn't available in the Camera Raw Filter.".into());
    }
    if !crate::media::source_info(p).raw {
        return Err("AI Denoise works on raw photos.".into());
    }
    Ok(())
}

impl Session {
    /// The denoiser to use: the override, else the installed model. `Err`: none installed.
    pub fn denoiser(&self) -> Result<(DenoiseFn, String), String> {
        if let Some(d) = &self.enhance.denoiser {
            return Ok((d.clone(), "custom".to_owned()));
        }
        let dir = self.quick_seg_dir.as_ref().ok_or("The AI Denoise model is not installed.")?;
        if li_seg::denoise::installed(dir).is_none() {
            return Err("The AI Denoise model is not installed.".into());
        }
        let dir = dir.clone();
        let f: DenoiseFn = Arc::new(move |px, w, h, progress| {
            let d = li_seg::denoise::shared(&dir).map_err(|e| format!("{e:#}"))?;
            d.denoise(px, w, h, progress).map_err(|e| if e.is::<li_seg::denoise::Cancelled>() { CANCELLED.to_string() } else { format!("{e:#}") })
        });
        Ok((f, model_tag()))
    }

    /// Start AI Denoise on photo `id` (in the background, or here with `wait`).
    pub fn start_denoise(&mut self, id: PhotoId, wait: bool) -> Result<Arc<Job>, String> {
        can_denoise(self, id)?;
        if self.enhance.running_for(id).any(|j| j.kind == JobKind::Denoise) {
            return Err("AI Denoise is already running on this photo.".into());
        }
        let (denoiser, model) = self.denoiser()?;
        let p = self.catalog.photo(id).ok_or("no such photo")?.clone();
        let source = source_hash(&p);
        let src = self.media.source_ref(&p, SourceLevel::Full);
        let to = self.ai_store_root();
        let work: super::Work = Box::new(move |ctl| run(&src, &denoiser, ctl, source, model, to.as_deref()));
        Ok(self.enhance.spawn(id, JobKind::Denoise, "AI Denoise", wait, work))
    }
}

fn run(src: &crate::media::SourceRef, denoiser: &DenoiseFn, ctl: &JobCtl, source: String, model: String, to: Option<&std::path::Path>) -> Result<Outcome, String> {
    ctl.set(0.01, "Loading the photo");
    let img = src.load()?;
    ctl.check()?;
    let (w, h) = (img.width, img.height);
    ctl.set(0.03, "Denoising");
    let progress = |n: usize, total: usize| {
        ctl.set(0.03 + 0.87 * n as f32 / total.max(1) as f32, "Denoising");
        !ctl.cancelled()
    };
    let out = denoiser(&img.data, w, h, &progress)?;
    if out.len() != w * h {
        return Err("the denoiser returned the wrong size".into());
    }
    drop(img);
    ctl.check()?;
    ctl.set(0.92, "Saving");
    let key = result_key(&source, &model);
    let full = Rgb32f { width: w, height: h, data: out };
    store::put_rgb(Kind::Denoise, &key, &full, to)?;
    let (pw, ph) = fit(w, h, PREVIEW_EDGE);
    if (pw, ph) != (w, h) {
        store::put_rgb(Kind::Denoise, &preview_key(&key), &resize(&full, pw, ph, Filter::Mitchell), to)?;
    }
    forget_levels(&key);
    ctl.set(1.0, "Done");
    Ok(Outcome::Denoise(DenoiseDone { key, source, model }))
}

fn fit(w: usize, h: usize, edge: usize) -> (usize, usize) {
    let long = w.max(h);
    if long <= edge {
        return (w, h);
    }
    let k = edge as f64 / long as f64;
    (((w as f64 * k).round() as usize).max(1), ((h as f64 * k).round() as usize).max(1))
}

/// Whether a stored result exists for `key` (cached; no file system access once known).
pub fn result_exists(key: &str) -> bool {
    store::exists(Kind::Denoise, key)
}

struct Mixed {
    src: Weak<Rgb32f>,
    key: String,
    strength: u32,
    out: Arc<Rgb32f>,
}

/// Recently mixed sources (so a view's stage cache sees the same buffer while the amount holds).
static MIXED: Mutex<Vec<Mixed>> = Mutex::new(Vec::new());
/// Recently read stored results (store key → pixels).
static LEVELS: Mutex<Vec<(String, Arc<Rgb32f>)>> = Mutex::new(Vec::new());

fn forget_levels(key: &str) {
    LEVELS.lock().unwrap_or_else(|e| e.into_inner()).retain(|(k, _)| !k.starts_with(key));
    MIXED.lock().unwrap_or_else(|e| e.into_inner()).retain(|m| m.key != key);
}

/// The stored result for `key` at a level fitting `long` px (the preview-size copy when it is
/// enough), read once and kept for a while.
fn level(key: &str, long: usize) -> Option<Arc<Rgb32f>> {
    let pk = preview_key(key);
    let candidates = if long <= PREVIEW_EDGE { vec![pk, key.to_owned()] } else { vec![key.to_owned()] };
    for k in candidates {
        if let Some(img) = LEVELS.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(c, _)| *c == k).map(|e| e.1.clone()) {
            return Some(img);
        }
        if let Some(img) = store::get_rgb(Kind::Denoise, &k) {
            let img = Arc::new(img);
            let mut l = LEVELS.lock().unwrap_or_else(|e| e.into_inner());
            l.insert(0, (k, img.clone()));
            l.truncate(2);
            return Some(img);
        }
    }
    None
}

/// `src` (a decoded source of the photo, any level) with the Denoise result `key` mixed in by
/// `amount` (0..100). `None` when there is nothing to mix (amount 0, or no stored result: render
/// from `src` itself).
pub fn denoised_source(src: &Arc<Rgb32f>, key: &str, amount: f64) -> Option<Arc<Rgb32f>> {
    let t = (amount / 100.0).clamp(0.0, 1.0) as f32;
    if t <= 0.0 || !result_exists(key) {
        return None;
    }
    let strength = t.to_bits();
    {
        let mut m = MIXED.lock().unwrap_or_else(|e| e.into_inner());
        m.retain(|e| e.src.strong_count() > 0);
        if let Some(e) = m.iter().find(|e| e.key == key && e.strength == strength && e.src.upgrade().is_some_and(|s| Arc::ptr_eq(&s, src))) {
            return Some(e.out.clone());
        }
    }
    let den = level(key, src.width.max(src.height))?;
    let den = if (den.width, den.height) == (src.width, src.height) { den } else { Arc::new(resize(&den, src.width, src.height, Filter::Mitchell)) };
    let mut out = (**src).clone();
    for (o, d) in out.data.iter_mut().zip(&den.data) {
        for c in 0..3 {
            o[c] += (d[c] - o[c]) * t;
        }
    }
    let out = Arc::new(out);
    let mut m = MIXED.lock().unwrap_or_else(|e| e.into_inner());
    m.insert(0, Mixed { src: Arc::downgrade(src), key: key.to_owned(), strength, out: out.clone() });
    m.truncate(3);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in denoiser: every pixel becomes the image mean (noise gone, brightness kept).
    fn flatten() -> DenoiseFn {
        Arc::new(|px, _, _, progress| {
            let n = px.len().max(1) as f32;
            let m = px.iter().fold([0.0f32; 3], |a, p| [a[0] + p[0], a[1] + p[1], a[2] + p[2]]).map(|v| v / n);
            if !progress(1, 1) {
                return Err(CANCELLED.into());
            }
            Ok(vec![m; px.len()])
        })
    }

    fn demo_raw(s: &mut Session) -> PhotoId {
        s.visible_cloned().into_iter().find(|id| can_denoise(s, *id).is_ok()).expect("a scene-referred demo photo")
    }

    #[test]
    fn denoise_runs_stores_and_mixes_by_amount() {
        let mut s = Session::with_demo();
        let id = demo_raw(&mut s);
        assert!(s.denoiser().is_err(), "no model installed");
        s.enhance.denoiser = Some(flatten());
        let job = s.start_denoise(id, true).expect("starts");
        assert!(job.finished());
        let done = match s.enhance.take_finished().pop().map(|(_, r)| r) {
            Some(Ok(Outcome::Denoise(d))) => d,
            other => panic!("{other:?}"),
        };
        let p = s.catalog.photo(id).unwrap().clone();
        assert_eq!(done.source, source_hash(&p));
        assert_eq!(done.key, result_key(&done.source, "custom"));
        assert!(result_exists(&done.key));
        // a preview-level source gets the result scaled to its size, mixed by the amount
        let src = s.source_now(id, SourceLevel::Thumb).unwrap();
        let flat = denoised_source(&src, &done.key, 100.0).expect("mixed");
        assert_eq!((flat.width, flat.height), (src.width, src.height));
        let spread = |img: &Rgb32f| {
            let (lo, hi) = img.data.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[1]), b.max(p[1])));
            hi - lo
        };
        assert!(spread(&flat) < spread(&src) * 0.2, "{} vs {}", spread(&flat), spread(&src));
        let half = denoised_source(&src, &done.key, 50.0).unwrap();
        let i = src.data.len() / 3;
        assert!((half.data[i][1] - (src.data[i][1] + flat.data[i][1]) / 2.0).abs() < 1e-4);
        // the same request gives the same buffer (stage caches stay warm)
        assert!(Arc::ptr_eq(&half, &denoised_source(&src, &done.key, 50.0).unwrap()));
        assert!(denoised_source(&src, &done.key, 0.0).is_none());
        // the result gone: render from the source itself
        store::delete(Kind::Denoise, &done.key);
        assert!(denoised_source(&src, &done.key, 50.0).is_none());
    }

    #[test]
    fn denoise_is_refused_where_it_cannot_run_and_can_be_cancelled() {
        let mut s = Session::with_demo();
        let id = demo_raw(&mut s);
        let slow: DenoiseFn = Arc::new(|px, _, _, progress| {
            for i in 0..1000 {
                if !progress(i, 1000) {
                    return Err(CANCELLED.into());
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Ok(px.to_vec())
        });
        s.enhance.denoiser = Some(slow);
        let job = s.start_denoise(id, false).unwrap();
        assert!(s.start_denoise(id, false).is_err(), "one at a time");
        s.enhance.cancel(Some(job.id));
        while !job.finished() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(s.enhance.take_finished().pop().map(|(_, r)| r), Some(Err(CANCELLED.to_string())));
        assert!(s.start_denoise(PhotoId(u64::MAX), true).is_err());
    }
}
