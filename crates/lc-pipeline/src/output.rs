//! Output encoding: the colour space (primaries + transfer curve) and sample format a render is
//! delivered in. Previews are always 8-bit sRGB; exports may ask for a wide-gamut space and/or
//! 16-bit or linear float samples.
//!
//! The per-pixel stage works in scene-linear Rec.2020 and ends by converting to the target's
//! primaries, gamut mapping into the *target* gamut, and encoding. Tone curves and grain operate on
//! sRGB-curve-encoded values of the target primaries (identical to the sRGB path when the target is
//! sRGB); afterwards the values are re-encoded with the target's own curve.

pub use li_color::space::{OutputSpace, OutputTrc};
use serde::{Deserialize, Serialize};

/// A validated, bidirectional ICC proof profile. Bytes travel with saved settings, so
/// relocating or replacing the original file cannot silently change an existing proof.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "std::sync::Arc<Vec<u8>>", into = "std::sync::Arc<Vec<u8>>")]
pub struct ProofProfile(std::sync::Arc<photocraft_cms::Profile>);

impl TryFrom<std::sync::Arc<Vec<u8>>> for ProofProfile {
    type Error = photocraft_cms::CmsError;
    fn try_from(bytes: std::sync::Arc<Vec<u8>>) -> Result<Self, Self::Error> {
        use photocraft_cms::{CmsError, Intent, ProfileClass};
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(CmsError::Invalid("proof profile exceeds 16 MiB".into()));
        }
        let profile = photocraft_cms::Profile::parse(&bytes)?;
        if matches!(profile.class, ProfileClass::DeviceLink | ProfileClass::NamedColor | ProfileClass::Abstract) {
            return Err(CmsError::Unsupported("proof requires a device or colour-space profile".into()));
        }
        // Validate both directions for every supported intent using the CMS's stage
        // builders. No device-link tables are built on the UI thread.
        for intent in Intent::ALL {
            profile.device_to_pcs(intent)?;
            profile.pcs_to_device(intent)?;
        }
        Ok(Self(std::sync::Arc::new(profile)))
    }
}
impl From<ProofProfile> for std::sync::Arc<Vec<u8>> {
    fn from(profile: ProofProfile) -> Self {
        profile.0.to_bytes()
    }
}
impl PartialEq for ProofProfile {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0) || self.0.to_bytes() == other.0.to_bytes()
    }
}
impl Eq for ProofProfile {}
impl std::hash::Hash for ProofProfile {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.0.content_hash().hash(h);
    }
}
impl ProofProfile {
    pub fn label(&self) -> &str {
        &self.0.description
    }
    pub fn key(&self) -> u64 {
        self.0.content_hash()
    }
    /// A trusted monitor profile supplied by the shared display service.
    pub fn monitor(profile: std::sync::Arc<photocraft_cms::Profile>) -> Self {
        Self(profile)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProofIntent {
    Perceptual,
    #[default]
    RelativeColorimetric,
    Saturation,
    AbsoluteColorimetric,
}
impl ProofIntent {
    pub fn cms(self) -> photocraft_cms::Intent {
        match self {
            Self::Perceptual => photocraft_cms::Intent::Perceptual,
            Self::RelativeColorimetric => photocraft_cms::Intent::RelativeColorimetric,
            Self::Saturation => photocraft_cms::Intent::Saturation,
            Self::AbsoluteColorimetric => photocraft_cms::Intent::AbsoluteColorimetric,
        }
    }
}

/// Develop's adapter to the shared CMS proof transform. Existing named-space JSON
/// remains valid; custom RGB, gray and printer CMYK profiles use the same backend.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Proof {
    pub space: OutputSpace,
    pub dest_warning: bool,
    pub display_warning: bool,
    pub profile: Option<ProofProfile>,
    pub intent: ProofIntent,
    pub bpc: bool,
    pub simulate_paper: bool,
    /// Window monitor, supplied at render submission and never persisted as a preference.
    #[serde(skip)]
    pub monitor: Option<ProofProfile>,
}
impl Default for Proof {
    fn default() -> Self {
        Self {
            space: OutputSpace::default(),
            dest_warning: false,
            display_warning: false,
            profile: None,
            intent: ProofIntent::default(),
            bpc: true,
            simulate_paper: false,
            monitor: None,
        }
    }
}

/// Warning colours in device-encoded values: Lightroom's destination red, monitor
/// blue, and pink when both devices lose the colour.
pub const PROOF_DEST_WARNING: [f32; 3] = [1.0, 0.0, 0.0];
pub const PROOF_DISPLAY_WARNING: [f32; 3] = [0.0, 0.25, 1.0];
pub const PROOF_BOTH_WARNING: [f32; 3] = [1.0, 0.0, 1.0];

/// Precomputed shared CMS transform and optional gamut checks; evaluated precisely
/// on the finished float edit, before quantization. Cached independently of image size.
#[derive(Debug)]
pub struct ProofParams {
    transform: photocraft_cms::Transform,
    dest: Option<photocraft_cms::GamutCheck>,
    display: Option<photocraft_cms::GamutCheck>,
    out_trc: OutputTrc,
}
impl ProofParams {
    /// Linear Rec.2020 → destination device values, with optional warning replacement.
    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let dest = self.dest.as_ref().is_some_and(|g| g.out_of_gamut(&rgb));
        let display = self.display.as_ref().is_some_and(|g| g.out_of_gamut(&rgb));
        match (dest, display) {
            (true, true) => PROOF_BOTH_WARNING,
            (true, false) => PROOF_DEST_WARNING,
            (false, true) => PROOF_DISPLAY_WARNING,
            (false, false) => {
                let mut out = [0.0; 3];
                self.transform.eval(&rgb, &mut out);
                out
            }
        }
    }
    /// Finish's compatibility boundary uses the sRGB curve, irrespective of target
    /// primaries. Preserve CMS device values through its final encoding adapter.
    pub(crate) fn finish(&self, rgb: [f32; 3]) -> [f32; 3] {
        self.apply(rgb).map(|v| li_color::transfer::linear_to_srgb(self.out_trc.decode(v)))
    }
}
impl Proof {
    pub fn label(&self) -> &str {
        self.profile.as_ref().map_or(self.space.label(), ProofProfile::label)
    }
    pub fn key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut h);
        h.finish()
    }
    /// Compatibility entry point to fallible CMS construction.
    pub fn params(&self, out: OutputSpace) -> Result<std::sync::Arc<ProofParams>, photocraft_cms::CmsError> {
        self.try_params(out)
    }
    /// Build the selected proof or return its linking error; never substitute another device.
    pub fn try_params(&self, out: OutputSpace) -> Result<std::sync::Arc<ProofParams>, photocraft_cms::CmsError> {
        use photocraft_cms::Builtin;
        use std::sync::{Arc, Mutex, OnceLock};
        type Cache = Vec<((Proof, OutputSpace), Arc<ProofParams>)>;
        static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
        let cache = CACHE.get_or_init(Default::default);
        let key = (self.clone(), out);
        // Serialize cold construction so two preview workers do not build the same
        // expensive CMYK link concurrently. Only 8 recent settings retain tables.
        let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = cache.iter().position(|(k, _)| *k == key) {
            let entry = cache.remove(i);
            let params = entry.1.clone();
            cache.push(entry);
            return Ok(params);
        }
        let src = photocraft_cms::builtin::linear_rec2020();
        let profile = self.profile.as_ref().map_or(Builtin::from_named(self.space.named()).profile(), |p| p.0.as_ref());
        let dst = self.monitor.as_ref().map_or(Builtin::from_named(out.named()).profile(), |p| p.0.as_ref());
        if dst.color_space != photocraft_cms::ColorSpace::Rgb {
            return Err(photocraft_cms::CmsError::Unsupported("proof monitor must be RGB".into()));
        }
        let params = Self::build(src, profile, dst, self, out)?;
        let params = Arc::new(params);
        if cache.len() == 8 {
            cache.remove(0);
        }
        cache.push((key, params.clone()));
        Ok(params)
    }
    fn build(
        src: &photocraft_cms::Profile,
        proof: &photocraft_cms::Profile,
        dst: &photocraft_cms::Profile,
        options: &Proof,
        out: OutputSpace,
    ) -> Result<ProofParams, photocraft_cms::CmsError> {
        use photocraft_cms::{GamutCheck, Transform};
        let threshold = photocraft_cms::gamut::DEFAULT_THRESHOLD;
        Ok(ProofParams {
            transform: Transform::proof(src, proof, dst, options.intent.cms(), options.bpc, options.simulate_paper)?,
            dest: options.dest_warning.then(|| GamutCheck::new(src, proof, threshold)).transpose()?,
            display: options.display_warning.then(|| GamutCheck::new(src, dst, threshold)).transpose()?,
            out_trc: out.trc(),
        })
    }
}

/// Sample format of a render.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputDepth {
    /// 8-bit display-encoded ([`crate::Rendered::image`] only).
    #[default]
    U8,
    /// 16-bit display-encoded RGB in [`crate::Rendered::deep`].
    U16,
    /// 32-bit float *linear* RGB (target primaries, 0..1) in [`crate::Rendered::deep`].
    F32Linear,
}

/// High-bit-depth RGB samples (3 per pixel, interleaved, row-major).
#[derive(Clone, Debug, PartialEq)]
pub enum DeepSamples {
    U16(Vec<u16>),
    F32(Vec<f32>),
}

/// A high-bit-depth render ([`OutputDepth::U16`] / [`OutputDepth::F32Linear`]).
#[derive(Clone, Debug, PartialEq)]
pub struct DeepImage {
    pub width: usize,
    pub height: usize,
    pub space: OutputSpace,
    pub samples: DeepSamples,
}

impl DeepImage {
    /// The same image reduced to 8 bits, display-encoded with the space's curve.
    pub fn to_rgba8(&self) -> li_raster::Rgba8 {
        let mut out = li_raster::Rgba8::new(self.width, self.height);
        let trc = self.space.trc();
        match &self.samples {
            DeepSamples::U16(v) => {
                for (p, c) in out.data.iter_mut().zip(v.as_chunks::<3>().0) {
                    *p = [c[0], c[1], c[2]].map(|x| ((x as u32 * 255 + 32767) / 65535) as u8).into_rgba();
                }
            }
            DeepSamples::F32(v) => {
                for (p, c) in out.data.iter_mut().zip(v.as_chunks::<3>().0) {
                    *p = [c[0], c[1], c[2]].map(|x| (trc.encode(x) * 255.0 + 0.5) as u8).into_rgba();
                }
            }
        }
        out
    }
}

trait IntoRgba {
    fn into_rgba(self) -> [u8; 4];
}

impl IntoRgba for [u8; 3] {
    fn into_rgba(self) -> [u8; 4] {
        [self[0], self[1], self[2], 255]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(b: photocraft_cms::Builtin) -> ProofProfile {
        ProofProfile::try_from(b.profile().to_bytes()).unwrap()
    }

    #[test]
    fn j22_proof_matches_shared_cms_presets_and_printer_exactly() {
        use photocraft_cms::{Builtin, Transform};
        let samples = [[0.0; 3], [0.18; 3], [1.0; 3], [0.8, 0.02, 0.04], [0.02, 0.8, 0.04], [0.01, 0.02, 0.9], [0.6, 0.3, 0.02]];
        let mut max_error = 0.0f32;
        let mut count = 0;
        for profile in Builtin::ALL {
            if matches!(profile, Builtin::LabD50 | Builtin::LinearSrgb) {
                continue;
            }
            let custom_profile = custom(profile);
            for out in [OutputSpace::Srgb, OutputSpace::DisplayP3] {
                for intent in [ProofIntent::Perceptual, ProofIntent::RelativeColorimetric, ProofIntent::Saturation, ProofIntent::AbsoluteColorimetric]
                {
                    for (bpc, simulate_paper) in [(false, false), (true, false), (true, true)] {
                        let proof = Proof { profile: Some(custom_profile.clone()), intent, bpc, simulate_paper, ..Default::default() };
                        let params = proof.params(out).unwrap();
                        let reference = Transform::proof(
                            photocraft_cms::builtin::linear_rec2020(),
                            custom_profile.0.as_ref(),
                            Builtin::from_named(out.named()).profile(),
                            intent.cms(),
                            bpc,
                            simulate_paper,
                        )
                        .unwrap();
                        for sample in samples {
                            let mut expected = [0.0; 3];
                            reference.eval(&sample, &mut expected);
                            let actual = params.apply(sample);
                            for c in 0..3 {
                                max_error = max_error.max((actual[c] - expected[c]).abs());
                            }
                            assert_eq!(actual, expected, "{profile:?} {out:?} {intent:?}");
                            count += 1;
                        }
                    }
                }
            }
        }
        println!("J22 precise CMS parity: {count} samples, max device-channel error {max_error}");
    }

    #[test]
    fn j22_proof_old_matrix_quality_comparison() {
        use photocraft_cms::{Builtin, Intent, Transform};
        let proof = Proof::default();
        let params = proof.params(OutputSpace::Srgb).unwrap();
        let matrix = OutputSpace::Srgb.from_working();
        let lab = Transform::new(Builtin::Srgb.profile(), Builtin::LabD50.profile(), Intent::RelativeColorimetric, false).unwrap();
        let mut max_delta_e = 0.0f32;
        let mut sum_delta_e = 0.0;
        let mut count = 0;
        let mut old_time = std::time::Duration::ZERO;
        let mut cms_time = std::time::Duration::ZERO;
        // Small fixed chart: neutrals, primaries, secondaries and intermediate patches.
        for r in [0.02, 0.18, 0.5, 0.9] {
            for g in [0.02, 0.18, 0.5, 0.9] {
                for b in [0.02, 0.18, 0.5, 0.9] {
                    let rgb = [r, g, b];
                    let old_start = std::time::Instant::now();
                    let old_linear = matrix.map(|row| row.into_iter().zip(rgb).map(|(a, b)| a * b).sum());
                    let old = crate::finish::gamut_map(old_linear, OutputSpace::Srgb.luma()).0.map(li_color::transfer::linear_to_srgb);
                    old_time += old_start.elapsed();
                    let cms_start = std::time::Instant::now();
                    let new = params.apply(rgb);
                    cms_time += cms_start.elapsed();
                    let (mut a, mut z) = ([0.0; 3], [0.0; 3]);
                    lab.eval(&old, &mut a);
                    lab.eval(&new, &mut z);
                    let de = ((a[0] - z[0]) * 100.0).hypot((a[1] - z[1]) * 255.0).hypot((a[2] - z[2]) * 255.0);
                    max_delta_e = max_delta_e.max(de);
                    sum_delta_e += de;
                    count += 1;
                }
            }
        }
        println!(
            "J22 old matrix vs shared CMS reference: {count} chart patches, mean DeltaE76 {}, max {max_delta_e}; new exact-reference error 0",
            sum_delta_e / count as f32
        );
        println!(
            "J22 64-patch scalar evaluation (not interactive): legacy matrix {} us; precise CMS {} us",
            old_time.as_secs_f64() * 1e6,
            cms_time.as_secs_f64() * 1e6
        );
        assert!(max_delta_e > 1.0, "chart detects the replaced mapping policy");
    }

    #[test]
    fn j22_proof_profiles_roundtrip_cache_and_keys() {
        let old: Proof = serde_json::from_str(r#"{"space":"displayP3","destWarning":true}"#).unwrap();
        assert!(old.bpc && old.profile.is_none());
        let proof = Proof { profile: Some(custom(photocraft_cms::Builtin::CoatedCmyk)), ..old };
        let saved = serde_json::to_vec(&proof).unwrap();
        let restored: Proof = serde_json::from_slice(&saved).unwrap();
        assert_eq!(proof, restored);
        assert_eq!(proof.key(), restored.key());
        assert!(std::sync::Arc::ptr_eq(&proof.params(OutputSpace::Srgb).unwrap(), &restored.params(OutputSpace::Srgb).unwrap()));
        let key = proof.key();
        for variant in [
            Proof { bpc: false, ..proof.clone() },
            Proof { simulate_paper: true, ..proof.clone() },
            Proof { intent: ProofIntent::Perceptual, ..proof.clone() },
            Proof { dest_warning: false, ..proof.clone() },
            Proof { display_warning: true, ..proof.clone() },
            Proof { profile: None, ..proof.clone() },
            Proof {
                monitor: Some(ProofProfile::monitor(std::sync::Arc::new(photocraft_cms::Builtin::DisplayP3.profile().clone()))),
                ..proof.clone()
            },
        ] {
            assert_ne!(key, variant.key());
        }
        assert!(ProofProfile::try_from(std::sync::Arc::new(vec![0; 128])).is_err());
        assert!(serde_json::from_str::<Proof>(r#"{"profile":[0,1,2]}"#).is_err());
    }

    #[test]
    fn j22_proof_incomplete_monitor_returns_error_without_substitution() {
        use li_color::curve::Curve;
        use photocraft_cms::Builtin;
        use photocraft_cms::pipeline::Stage;
        use photocraft_cms::profile::{Lut, LutKind};
        let mut monitor = Builtin::Srgb.profile().clone();
        monitor.matrix = None;
        monitor.trc = None;
        monitor.b2a = [Some(Lut { kind: LutKind::Ab, inputs: 3, outputs: 3, stages: vec![Stage::Curves(vec![Curve::Identity; 3])] }), None, None];
        let monitor = ProofProfile::monitor(std::sync::Arc::new(monitor.with_encoded_bytes()));
        let proof =
            Proof { profile: Some(custom(Builtin::CoatedCmyk)), monitor: Some(monitor), bpc: false, simulate_paper: true, ..Default::default() };
        assert!(proof.try_params(OutputSpace::Srgb).is_ok(), "destination-only monitor can display a proof");
        let invalid = Proof { display_warning: true, ..proof };
        assert!(invalid.try_params(OutputSpace::Srgb).is_err(), "round-trip monitor warning cannot silently substitute a profile");
        let req = crate::RenderRequest { proof: Some(invalid), ..crate::RenderRequest::fit(2, 2) };
        let src = li_raster::RgbFrame::filled(2, 2, [0.18; 3]);
        assert!(crate::render_checked(&src, &Default::default(), &Default::default(), &req).is_err());
        assert!(
            crate::render_cached_checked(
                &std::sync::Arc::new(src.clone()),
                &Default::default(),
                &Default::default(),
                &req,
                &crate::StageCache::default()
            )
            .is_err()
        );
        assert_eq!(crate::render(&src, &Default::default(), &Default::default(), &req).image.len(), 0);
    }

    #[test]
    fn j22_proof_warnings_match_shared_gamut_and_monitor() {
        use photocraft_cms::{Builtin, GamutCheck};
        let proof = Proof { space: OutputSpace::DisplayP3, dest_warning: true, display_warning: true, ..Default::default() };
        let p = proof.params(OutputSpace::Srgb).unwrap();
        let src = photocraft_cms::builtin::linear_rec2020();
        let dest = GamutCheck::new(src, Builtin::DisplayP3.profile(), photocraft_cms::gamut::DEFAULT_THRESHOLD).unwrap();
        let monitor = GamutCheck::new(src, Builtin::Srgb.profile(), photocraft_cms::gamut::DEFAULT_THRESHOLD).unwrap();
        let mut states = [false; 4];
        for r in [0.0, 0.1, 0.5, 1.0] {
            for g in [0.0, 0.1, 0.5, 1.0] {
                for b in [0.0, 0.1, 0.5, 1.0] {
                    let c = [r, g, b];
                    let d = dest.out_of_gamut(&c);
                    let m = monitor.out_of_gamut(&c);
                    states[usize::from(d) * 2 + usize::from(m)] = true;
                    if d || m {
                        assert_eq!(
                            p.apply(c),
                            match (d, m) {
                                (true, true) => PROOF_BOTH_WARNING,
                                (true, false) => PROOF_DEST_WARNING,
                                _ => PROOF_DISPLAY_WARNING,
                            }
                        );
                    }
                }
            }
        }
        assert!(states[0] && states[1] && states[3], "neutral, monitor-only and both warnings tested");
        let p3 = Proof { monitor: Some(ProofProfile::monitor(std::sync::Arc::new(Builtin::DisplayP3.profile().clone()))), ..proof }
            .params(OutputSpace::Srgb)
            .unwrap();
        assert_ne!(p3.apply([0.1, 0.5, 0.1]), p.apply([0.1, 0.5, 0.1]), "active monitor changes warning or device output");
    }

    #[test]
    fn trcs_invert() {
        for o in OutputSpace::ALL {
            let t = o.trc();
            for i in 0..=100 {
                let x = i as f32 / 100.0;
                assert!((t.decode(t.encode(x)) - x).abs() < 1e-5, "{o:?} {x}");
            }
        }
    }

    #[test]
    fn parse_and_serde_names() {
        for o in OutputSpace::ALL {
            let s = serde_json::to_value(o).unwrap();
            assert_eq!(OutputSpace::parse(s.as_str().unwrap()), Some(o));
            assert_eq!(OutputSpace::parse(o.label()), Some(o), "{}", o.label());
        }
    }

    #[test]
    fn white_maps_to_white() {
        for o in OutputSpace::ALL {
            let m = o.from_working();
            for row in m {
                assert!((row.iter().sum::<f32>() - 1.0).abs() < 1e-4, "{o:?}");
            }
            assert!((o.luma().iter().sum::<f32>() - 1.0).abs() < 1e-4);
        }
    }
}
