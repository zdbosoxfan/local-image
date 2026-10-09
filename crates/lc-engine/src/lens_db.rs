//! Lens profiles from the lensfun database (the `lensfun` crate, a pure-Rust port of LensFun,
//! LGPL-3.0-or-later, bundling the LensFun calibration database, CC-BY-SA 3.0 — see
//! `licenses/lensfun-NOTICE.md`).
//!
//! The camera and lens are detected from the photo's EXIF camera and lens names (or picked by
//! hand); the lens's calibrations are interpolated for the shot's focal length and aperture and
//! rescaled to the image like lensfun's `lfModifier` does (`rescale_*` below, ported from
//! lensfun's `mod-coord.cpp`, `mod-subpix.cpp`, `mod-color.cpp`). The result is a
//! [`LensCorrection`] the pipeline evaluates ([`lightcraft_pipeline::lensdb`]); the database
//! is loaded once, on first use.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use lensfun::{CalibDistortion, CalibTca, CalibVignetting, Camera, Database, DistortionModel, Lens, LensType, TcaModel, VignettingModel};
use lightcraft_pipeline::lensdb::{Distortion, LensCorrection, Tca};

/// Subject distance used for vignetting when the shot doesn't record one (lensfun's convention
/// for "far").
const FAR: f32 = 1000.0;

/// The bundled database (loaded on first use; `None` if it could not be read).
pub fn database() -> Option<&'static Database> {
    static DB: OnceLock<Option<Database>> = OnceLock::new();
    DB.get_or_init(|| match Database::load_bundled() {
        Ok(db) => Some(db),
        Err(e) => {
            log::warn!("lens database: {e}");
            None
        }
    })
    .as_ref()
}

/// A camera or lens as the UI shows and the settings store it: maker and model.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Named {
    pub maker: String,
    pub model: String,
}

impl Named {
    fn of_lens(l: &Lens) -> Named {
        Named { maker: l.maker.clone(), model: l.model.clone() }
    }
    fn of_camera(c: &Camera) -> Named {
        Named { maker: c.maker.clone(), model: c.model.clone() }
    }
    /// "Maker Model" without repeating the maker.
    pub fn label(&self) -> String {
        if self.model.to_lowercase().starts_with(&self.maker.to_lowercase()) || self.maker.is_empty() {
            self.model.clone()
        } else {
            format!("{} {}", self.maker, self.model)
        }
    }
}

/// The database camera for an EXIF camera name ("Make Model", as the catalog stores it): every
/// word of the database model must appear in it; the maker's first word too; the longest model
/// wins.
pub fn find_camera<'a>(db: &'a Database, exif_camera: &str) -> Option<&'a Camera> {
    if exif_camera.trim().is_empty() {
        return None;
    }
    let words = |s: &str| -> Vec<String> { s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect() };
    let have = words(exif_camera);
    db.cameras
        .iter()
        .filter(|c| {
            let m = words(&c.model);
            !m.is_empty() && m.iter().all(|w| have.contains(w)) && words(&c.maker).first().is_none_or(|w| have.contains(w))
        })
        .max_by_key(|c| (words(&c.model).len(), c.model.len()))
}

fn find_camera_named<'a>(db: &'a Database, n: &Named) -> Option<&'a Camera> {
    db.cameras.iter().find(|c| c.maker == n.maker && c.model == n.model)
}

fn find_lens_named<'a>(db: &'a Database, n: &Named) -> Option<&'a Lens> {
    db.lenses.iter().find(|l| l.maker == n.maker && l.model == n.model)
}

/// What [`detect`] found for a photo.
#[derive(Clone, Debug, PartialEq)]
pub struct Detected {
    pub camera: Option<Named>,
    pub lens: Option<Named>,
}

/// Detect the camera and lens of a photo from its EXIF camera and lens names.
pub fn detect(exif_camera: &str, exif_lens: &str) -> Detected {
    let Some(db) = database() else { return Detected { camera: None, lens: None } };
    let camera = find_camera(db, exif_camera);
    let lens = if exif_lens.trim().is_empty() { None } else { db.find_lenses(camera, exif_lens).into_iter().next() };
    Detected { camera: camera.map(Named::of_camera), lens: lens.map(Named::of_lens) }
}

/// Lenses for the manual picker: those that fit `camera` (its mount and compatible ones; every
/// lens without a camera) whose maker and model contain every word of `query`, sorted.
pub fn search(camera: Option<&Named>, query: &str, limit: usize) -> Vec<Named> {
    let Some(db) = database() else { return Vec::new() };
    let cam = camera.and_then(|c| find_camera_named(db, c));
    let mounts: Vec<&str> = match cam {
        Some(c) => {
            let mut v = vec![c.mount.as_str()];
            if let Some(m) = db.mounts.iter().find(|m| m.name == c.mount) {
                v.extend(m.compat.iter().map(String::as_str));
            }
            v
        }
        None => Vec::new(),
    };
    let q: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut out: Vec<Named> = db
        .lenses
        .iter()
        .filter(|l| mounts.is_empty() || l.mounts.iter().any(|m| mounts.contains(&m.as_str())))
        .filter(|l| {
            let hay = format!("{} {}", l.maker, l.model).to_lowercase();
            q.iter().all(|w| hay.contains(w.as_str()))
        })
        .map(Named::of_lens)
        .collect();
    out.sort_by(|a, b| a.label().to_lowercase().cmp(&b.label().to_lowercase()));
    out.dedup();
    out.truncate(limit);
    out
}

/// One photo's lens profile request.
#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    /// EXIF camera ("Make Model") and lens names.
    pub exif_camera: String,
    pub exif_lens: String,
    /// The user's picks (override detection).
    pub camera: Option<Named>,
    pub lens: Option<Named>,
    pub focal: f32,
    pub aperture: Option<f32>,
    pub distance: Option<f32>,
}

/// The rescaled correction for a shot, or `None` when the camera or lens isn't known, the focal
/// length is missing, or the lens has no usable calibration. Cached per request.
pub fn correction(q: &Query) -> Option<LensCorrection> {
    type Key = (String, String, Option<Named>, Option<Named>, u32, u32, u32);
    static CACHE: OnceLock<Mutex<HashMap<Key, Option<LensCorrection>>>> = OnceLock::new();
    let key: Key = (
        q.exif_camera.clone(),
        q.exif_lens.clone(),
        q.camera.clone(),
        q.lens.clone(),
        q.focal.to_bits(),
        q.aperture.unwrap_or(0.0).to_bits(),
        q.distance.unwrap_or(0.0).to_bits(),
    );
    let cache = CACHE.get_or_init(Default::default);
    if let Some(c) = cache.lock().ok().and_then(|m| m.get(&key).copied()) {
        return c;
    }
    let r = resolve(q);
    if let Ok(mut m) = cache.lock() {
        if m.len() > 4096 {
            m.clear();
        }
        m.insert(key, r);
    }
    r
}

fn resolve(q: &Query) -> Option<LensCorrection> {
    let db = database()?;
    if q.focal.is_nan() || q.focal <= 0.0 {
        return None;
    }
    let camera = match &q.camera {
        Some(n) => find_camera_named(db, n),
        None => find_camera(db, &q.exif_camera),
    };
    let lens = match &q.lens {
        Some(n) => find_lens_named(db, n),
        None if q.exif_lens.trim().is_empty() => None,
        None => db.find_lenses(camera, &q.exif_lens).into_iter().next(),
    }?;
    // the image's crop factor: the camera's, else the lens calibration's
    let crop = camera.map(|c| c.crop_factor).filter(|c| *c > 0.0).unwrap_or(lens.crop_factor).max(0.1);
    correction_for(lens, q.focal, crop, q.aperture, q.distance)
}

/// The correction a photo's settings ask for (`lens_db` on, Optics not switched off), from its
/// EXIF camera, lens, focal length and aperture and the user's picks.
pub fn for_photo(p: &lightcraft_catalog::Photo, s: &lightcraft_develop::DevelopSettings) -> Option<LensCorrection> {
    let l = &s.lens_db;
    if !l.enabled || !s.section_enabled("optics") {
        return None;
    }
    let named = |n: &lightcraft_develop::LensName| Named { maker: n.maker.clone(), model: n.model.clone() };
    correction(&Query {
        exif_camera: p.meta.camera.clone(),
        exif_lens: p.meta.lens.clone(),
        camera: l.camera.as_ref().map(named),
        lens: l.lens.as_ref().map(named),
        focal: p.meta.focal_mm.unwrap_or(0.0),
        aperture: p.meta.aperture,
        distance: None,
    })
}

/// [`correction`] for a known lens (lensfun's `lfModifier` set-up).
pub fn correction_for(lens: &Lens, focal: f32, crop: f32, aperture: Option<f32>, distance: Option<f32>) -> Option<LensCorrection> {
    let calib_dist = lens.interpolate_distortion(focal);
    let real_focal = calib_dist.and_then(|d| d.real_focal).unwrap_or(focal) as f64;
    let lens_crop = if lens.crop_factor > 0.0 { lens.crop_factor } else { crop };
    // fisheyes and other projections would need a geometry conversion: distortion is left out
    let rectilinear = matches!(lens.lens_type, LensType::Rectilinear);
    let distortion = calib_dist.filter(|_| rectilinear).map(|d| rescale_distortion(&d, lens.aspect_ratio, lens_crop, real_focal)).unwrap_or_default();
    let tca = lens.interpolate_tca(focal).map(|t| rescale_tca(&t, lens.aspect_ratio, lens_crop, real_focal)).unwrap_or_default();
    let vignetting = aperture
        .filter(|a| *a > 0.0)
        .and_then(|a| lens.interpolate_vignetting(focal, a, distance.filter(|d| *d > 0.0).unwrap_or(FAR)))
        .and_then(|v| rescale_vignetting(&v, lens_crop, real_focal));
    let c = LensCorrection {
        diag_norm: 36f64.hypot(24.0) / crop as f64 / real_focal,
        center: [lens.center_x as f64, lens.center_y as f64],
        distortion,
        tca,
        vignetting,
    };
    (!c.is_identity()).then_some(c)
}

/// lensfun's `rescale_polynomial_coefficients` for distortion (`mod-coord.cpp`): calibration
/// units (Hugin's, half the short side of the calibration frame) → focal-length units.
fn rescale_distortion(d: &CalibDistortion, aspect: f32, crop: f32, real_focal: f64) -> Distortion {
    let hs = (real_focal / (36f64.hypot(24.0) / crop as f64 / (aspect as f64).hypot(1.0) / 2.0)) as f32 as f64;
    match d.model {
        DistortionModel::None => Distortion::None,
        DistortionModel::Poly3 { k1 } => {
            let dd = 1.0 - k1 as f64;
            if k1 == 0.0 {
                return Distortion::None;
            }
            Distortion::Poly3 { k1: (k1 as f64 * hs.powi(2) / dd.powi(3)) as f32 as f64 }
        }
        DistortionModel::Poly5 { k1, k2 } => {
            Distortion::Poly5 { k1: (k1 as f64 * hs.powi(2)) as f32 as f64, k2: (k2 as f64 * hs.powi(4)) as f32 as f64 }
        }
        DistortionModel::Ptlens { a, b, c } => {
            let dd = 1.0 - a as f64 - b as f64 - c as f64;
            Distortion::Ptlens {
                a: (a as f64 * hs.powi(3) / dd.powi(4)) as f32 as f64,
                b: (b as f64 * hs.powi(2) / dd.powi(3)) as f32 as f64,
                c: (c as f64 * hs / dd.powi(2)) as f32 as f64,
            }
        }
    }
}

/// lensfun's rescaling for TCA (`mod-subpix.cpp`), correction direction.
fn rescale_tca(t: &CalibTca, aspect: f32, crop: f32, real_focal: f64) -> Tca {
    let hs = (real_focal / (36f64.hypot(24.0) / crop as f64 / (aspect as f64).hypot(1.0) / 2.0)) as f32 as f64;
    match t.model {
        TcaModel::None => Tca::None,
        TcaModel::Linear { kr, kb } => Tca::Linear { kr: kr as f64, kb: kb as f64 },
        TcaModel::Poly3 { red, blue } => {
            let ch = |c: [f32; 3]| [c[0] as f64, (c[1] as f64 * hs) as f32 as f64, (c[2] as f64 * hs.powi(2)) as f32 as f64];
            Tca::Poly3 { red: ch(red), blue: ch(blue) }
        }
    }
}

/// lensfun's rescaling for vignetting (`mod-color.cpp`).
fn rescale_vignetting(v: &CalibVignetting, crop: f32, real_focal: f64) -> Option<[f64; 3]> {
    let hs = (real_focal / (36f64.hypot(24.0) / crop as f64 / 2.0)) as f32 as f64;
    match v.model {
        VignettingModel::None => None,
        VignettingModel::Pa { k1, k2, k3 } => {
            Some([(k1 as f64 * hs.powi(2)) as f32 as f64, (k2 as f64 * hs.powi(4)) as f32 as f64, (k3 as f64 * hs.powi(6)) as f32 as f64])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lensfun::Modifier;

    /// A bundled lens with distortion, TCA and vignetting calibrations, and its camera.
    fn calibrated() -> (&'static Lens, &'static Camera) {
        let db = database().expect("bundled database");
        let lens = db
            .lenses
            .iter()
            .find(|l| {
                matches!(l.lens_type, LensType::Rectilinear)
                    && l.calib_distortion.iter().any(|c| !matches!(c.model, DistortionModel::None))
                    && l.calib_tca.iter().any(|c| !matches!(c.model, TcaModel::None))
                    && l.calib_vignetting.len() > 2
                    && l.center_x == 0.0
                    && l.center_y == 0.0
            })
            .expect("a fully calibrated lens");
        let cam = db.cameras.iter().find(|c| lens.mounts.contains(&c.mount)).expect("a camera for it");
        (lens, cam)
    }

    #[test]
    fn database_loads_and_detects_cameras() {
        let t = std::time::Instant::now();
        let db = database().expect("bundled database");
        eprintln!("lens database: {} cameras, {} lenses, {:?}", db.cameras.len(), db.lenses.len(), t.elapsed());
        assert!(db.cameras.len() > 500 && db.lenses.len() > 500);
        let c = find_camera(db, "NIKON CORPORATION NIKON D750").expect("D750");
        assert!(c.model.contains("D750"), "{c:?}");
        let c = find_camera(db, "Canon Canon EOS 5D Mark IV").expect("5D IV");
        assert!(c.model.contains("5D Mark IV"), "{}", c.model);
        assert!(find_camera(db, "").is_none() && find_camera(db, "Nonexistent Cam 9000").is_none());
        let d = detect("Canon Canon EOS 5D Mark IV", "EF24-70mm f/2.8L II USM");
        assert!(d.lens.as_ref().is_some_and(|l| l.model.contains("24-70")), "{d:?}");
        let found = search(d.camera.as_ref(), "24-70", 50);
        assert!(found.iter().any(|l| l.model.contains("24-70")), "{found:?}");
    }

    /// Our correction matches lensfun's own `Modifier` in correction mode.
    #[test]
    fn matches_the_lensfun_modifier() {
        let (lens, cam) = calibrated();
        let focal = lens.calib_distortion.iter().find(|c| !matches!(c.model, DistortionModel::None)).map(|c| c.focal).unwrap();
        let aperture = lens.calib_vignetting[0].aperture;
        let (w, h) = (3000u32, 2000u32);
        let ours = correction_for(lens, focal, cam.crop_factor, Some(aperture), None).expect("a correction");
        // distortion
        let mut m = Modifier::new(lens, focal, cam.crop_factor, w, h, false);
        assert!(m.enable_distortion_correction(lens));
        let only_dist = LensCorrection { tca: Tca::None, vignetting: None, ..ours }.on(w as f64, h as f64);
        let mut coords = vec![0f32; 2 * w as usize];
        for y in [0u32, 333, 1000, 1999] {
            m.apply_geometry_distortion(0.0, y as f32, w as usize, 1, &mut coords);
            for x in [0usize, 7, 1500, 2999] {
                let (sx, sy) = only_dist.to_source(x as f64 + 0.5, y as f64 + 0.5, 1, 1.0, 1.0);
                let (lx, ly) = (coords[2 * x] as f64 + 0.5, coords[2 * x + 1] as f64 + 0.5);
                assert!((sx - lx).abs() < 0.02 && (sy - ly).abs() < 0.02, "({x}, {y}): ours ({sx}, {sy}) lensfun ({lx}, {ly})");
            }
        }
        // TCA
        let mut m = Modifier::new(lens, focal, cam.crop_factor, w, h, false);
        assert!(m.enable_tca_correction(lens));
        let only_tca = LensCorrection { distortion: Distortion::None, vignetting: None, ..ours }.on(w as f64, h as f64);
        let mut coords = vec![0f32; 6 * w as usize];
        m.apply_subpixel_distortion(0.0, 10.0, w as usize, 1, &mut coords);
        for x in [0usize, 100, 2999] {
            for ch in [0usize, 2] {
                let (sx, sy) = only_tca.to_source(x as f64 + 0.5, 10.5, ch, 1.0, 1.0);
                let (lx, ly) = (coords[6 * x + 2 * ch] as f64 + 0.5, coords[6 * x + 2 * ch + 1] as f64 + 0.5);
                assert!((sx - lx).abs() < 0.02 && (sy - ly).abs() < 0.02, "tca ch {ch} x {x}: ours ({sx}, {sy}) lensfun ({lx}, {ly})");
            }
        }
        // vignetting: lensfun's correction multiplier at each pixel
        let mut m = Modifier::new(lens, focal, cam.crop_factor, w, h, false);
        assert!(m.enable_vignetting_correction(lens, aperture, FAR));
        let mut px = vec![1f32; 3 * w as usize];
        m.apply_color_modification_f32(&mut px, 0.0, 0.0, w as usize, 1, 3);
        let map = ours.on(w as f64, h as f64);
        for x in [0usize, 500, 1500] {
            let g = map.gain(x as f64 + 0.5, 0.5, 1.0);
            assert!((g - px[3 * x] as f64).abs() < 1e-3, "vignetting at {x}: ours {g} lensfun {}", px[3 * x]);
        }
        assert!(map.gain(0.5, 0.5, 1.0) > 1.0, "corners are brightened");
    }

    #[test]
    fn unknown_or_incomplete_requests_give_nothing() {
        let q = Query {
            exif_camera: "Nonexistent Cam".into(),
            exif_lens: "Mystery 50mm".into(),
            camera: None,
            lens: None,
            focal: 50.0,
            aperture: Some(2.8),
            distance: None,
        };
        assert_eq!(correction(&q), None);
        let (lens, cam) = calibrated();
        let picked = Query { camera: Some(Named::of_camera(cam)), lens: Some(Named::of_lens(lens)), focal: 0.0, ..q.clone() };
        assert_eq!(correction(&picked), None, "no focal length");
        let focal = lens.calib_distortion[0].focal;
        let picked = Query { focal, ..picked };
        assert!(correction(&picked).is_some());
        assert_eq!(correction(&picked), correction(&picked), "cached");
    }
}
