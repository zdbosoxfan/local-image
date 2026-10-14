//! Worker-local evidence for one reusable representative style. No decoded raster,
//! confidence plane, profile table or Undo chain is retained by this cache.
use super::{Input, Look, Options};
use crate::media::DecodedSource;
use lightcraft_catalog::PhotoId;
use lightcraft_develop::DevelopSettings;
use lightcraft_pipeline::{ClipConfidence, SourceInfo};
use lightcraft_preview::{Hash128, Hasher128};
use serde::{Serialize, ser};
use std::{
    collections::BTreeMap,
    fmt,
    sync::{Arc, Weak},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResourceEpochs {
    camera: u64,
    dcp: u64,
}
impl ResourceEpochs {
    fn current() -> Self {
        Self { camera: crate::camera_profiles::cache_key(), dcp: crate::dcp_profiles::cache_key() }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BatchKey {
    pub(super) targets: Vec<PhotoId>,
    digest: Hash128,
    epochs: ResourceEpochs,
}
impl BatchKey {
    pub(super) fn capture(generation: u64, requested: &[PhotoId], inputs: &[Input], looks: &[Look], mut options: Options) -> Option<Self> {
        let mut targets = requested.to_vec();
        targets.sort_unstable();
        targets.dedup();
        let mut sorted = inputs.iter().collect::<Vec<_>>();
        sorted.sort_by_key(|input| input.identity.id);
        if targets.len() != sorted.len() || !targets.iter().zip(&sorted).all(|(id, input)| *id == input.identity.id) {
            return None;
        }
        // This is the exact stage-one recipe. Final normalization and Amount still
        // use the requested options when the shared style is applied to a target.
        options.strength = 100.0;
        options.layer_placement = None;
        let epochs = ResourceEpochs::current();
        let mut digest = Digest::default();
        digest.feed(&("match-look-batch-v1", generation, epochs.camera, epochs.dcp, &targets, looks, options)).ok()?;
        for input in sorted {
            let placement = input.placement.map(|p| (p.index, p.id));
            let previous = input.previous.as_ref().map(|record| (record.options, record.managed_layer));
            digest.feed(&(&input.identity, &input.baseline, placement, &input.faces, input.preview_only, previous)).ok()?;
        }
        Some(Self { targets, digest: digest.finish(), epochs })
    }
    pub(super) fn resources_current(&self) -> bool {
        self.epochs == ResourceEpochs::current()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SharedStyle {
    pub(super) key: BatchKey,
    pub(super) color: Arc<DevelopSettings>,
    pub(super) representative: PhotoId,
    pub(super) sources: BTreeMap<PhotoId, SourceEvidence>,
    /// Only audits actually used by stage one belong here. Any failed audit
    /// prevents constructing this entry, even when the solver retained baseline.
    pub(super) verification: BTreeMap<PhotoId, SourceEvidence>,
}

impl SharedStyle {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn complete(
        key: BatchKey,
        selected_color: &DevelopSettings,
        options: Options,
        representative: PhotoId,
        solved: &[PhotoId],
        audited: &[PhotoId],
        sources: BTreeMap<PhotoId, SourceEvidence>,
        verification: BTreeMap<PhotoId, SourceEvidence>,
    ) -> Option<Self> {
        fn members(ids: &[PhotoId]) -> Vec<PhotoId> {
            let mut ids = ids.to_vec();
            ids.sort_unstable();
            ids.dedup();
            ids
        }
        if members(solved) != key.targets
            || sources.keys().copied().collect::<Vec<_>>() != key.targets
            || !key.targets.contains(&representative)
            || verification.keys().copied().collect::<Vec<_>>() != members(audited)
            || !audited.iter().all(|id| key.targets.contains(id))
        {
            return None;
        }
        let mut color = DevelopSettings::default();
        lightcraft_pipeline::match_look::copy_color_controls(&mut color, selected_color, options);
        Some(Self { key, color: Arc::new(color), representative, sources, verification })
    }
    /// This check intentionally does not validate pixels. The worker must still
    /// match every source/audit witness before publishing any cached-style edits.
    pub(super) fn metadata_current(&self, generation: u64, inputs: &[Input], looks: &[Look], options: Options) -> bool {
        self.key.resources_current() && BatchKey::capture(generation, &self.key.targets, inputs, looks, options).as_ref() == Some(&self.key)
    }
}

#[derive(Clone, Debug)]
pub(super) struct SourceEvidence {
    width: usize,
    height: usize,
    raw_key: u64,
    pixels_digest: Hash128,
    pixels: Weak<lightcraft_raster::Rgb32f>,
    small_facts: Hash128,
    clip: Option<(Hash128, Weak<ClipConfidence>)>,
    profile: Option<(Hash128, Weak<li_color::profile::ProfileLook>)>,
}
impl SourceEvidence {
    pub(super) fn capture(source: &DecodedSource, effective_info: &SourceInfo) -> Option<Self> {
        if source.image.width.checked_mul(source.image.height) != Some(source.image.data.len()) || source.image.data.is_empty() {
            return None;
        }
        let small_facts = small_facts(effective_info)?;
        let clip = effective_info
            .clip_confidence
            .as_ref()
            .map(|clip| {
                if clip.width.checked_mul(clip.height) != Some(clip.data.len()) {
                    return None;
                }
                Some((f32_buffer_digest("clip", clip.width, clip.height, 1, &clip.data)?, Arc::downgrade(clip)))
            })
            .transpose_option()?;
        let profile = effective_info
            .camera_profile
            .as_ref()
            .map(|profile| Some((fingerprint(profile.as_ref())?, Arc::downgrade(profile))))
            .transpose_option()?;
        Some(Self {
            width: source.image.width,
            height: source.image.height,
            raw_key: source.raw_key,
            pixels_digest: f32_buffer_digest("rgb", source.image.width, source.image.height, 3, source.image.data.as_flattened())?,
            pixels: Arc::downgrade(&source.image),
            small_facts,
            clip,
            profile,
        })
    }
    /// Rebind only weak tokens after the complete existing witness has matched.
    /// Digests and source facts remain immutable; no decoded buffer is pinned.
    pub(super) fn validated(&self, source: &DecodedSource, effective_info: &SourceInfo) -> Option<Self> {
        if !self.matches(source, effective_info) {
            return None;
        }
        let mut refreshed = self.clone();
        refreshed.pixels = Arc::downgrade(&source.image);
        if let (Some((_, token)), Some(clip)) = (&mut refreshed.clip, &effective_info.clip_confidence) {
            *token = Arc::downgrade(clip);
        }
        if let (Some((_, token)), Some(profile)) = (&mut refreshed.profile, &effective_info.camera_profile) {
            *token = Arc::downgrade(profile);
        }
        Some(refreshed)
    }
    pub(super) fn matches(&self, source: &DecodedSource, effective_info: &SourceInfo) -> bool {
        if self.width != source.image.width
            || self.height != source.image.height
            || self.raw_key != source.raw_key
            || small_facts(effective_info) != Some(self.small_facts)
        {
            return false;
        }
        let clip_matches = match (&self.clip, &effective_info.clip_confidence) {
            (None, None) => true,
            (Some((digest, token)), Some(clip)) => {
                Weak::ptr_eq(token, &Arc::downgrade(clip)) || f32_buffer_digest("clip", clip.width, clip.height, 1, &clip.data) == Some(*digest)
            }
            _ => false,
        };
        let profile_matches = match (&self.profile, &effective_info.camera_profile) {
            (None, None) => true,
            (Some((digest, token)), Some(profile)) => Weak::ptr_eq(token, &Arc::downgrade(profile)) || fingerprint(profile.as_ref()) == Some(*digest),
            _ => false,
        };
        clip_matches
            && profile_matches
            && (Weak::ptr_eq(&self.pixels, &Arc::downgrade(&source.image))
                || f32_buffer_digest("rgb", source.image.width, source.image.height, 3, source.image.data.as_flattened()) == Some(self.pixels_digest))
    }
}
/// Large typed planes avoid per-scalar serde tags/writes. The stream still includes
/// the scalar type, role, dimensions, channels and length, then every finite f32 bit.
/// A fixed stack chunk bounds temporary memory independently of image dimensions.
fn f32_buffer_digest(role: &str, width: usize, height: usize, channels: usize, values: &[f32]) -> Option<Hash128> {
    if width.checked_mul(height)?.checked_mul(channels)? != values.len() {
        return None;
    }
    let mut digest = Hasher128::new();
    digest.str("match-look-f32-bits-v2").str(role).u64(width as u64).u64(height as u64).u64(channels as u64).u64(values.len() as u64);
    let mut bytes = [0u8; 16384];
    for chunk in values.chunks(bytes.len() / 4) {
        for (value, encoded) in chunk.iter().zip(bytes.chunks_exact_mut(4)) {
            if !value.is_finite() {
                return None;
            }
            encoded.copy_from_slice(&value.to_bits().to_le_bytes());
        }
        digest.update(&bytes[..chunk.len() * 4]);
    }
    Some(digest.finish())
}

trait TransposeOption<T> {
    fn transpose_option(self) -> Option<Option<T>>;
}
impl<T> TransposeOption<T> for Option<Option<T>> {
    fn transpose_option(self) -> Option<Option<T>> {
        match self {
            None => Some(None),
            Some(Some(value)) => Some(Some(value)),
            Some(None) => None,
        }
    }
}

fn small_facts(info: &SourceInfo) -> Option<Hash128> {
    // Exhaustive destructuring makes additions to SourceInfo a compile-time
    // request to update this key. lens_db is deliberately not lost to serde(skip).
    let SourceInfo {
        clip_confidence: _,
        raw_clip_level,
        camera_noise,
        camera_wb_presets,
        lens,
        raw,
        as_shot_temp,
        as_shot_tint,
        relative_wb,
        camera_color,
        camera_profile: _,
        baseline_gain,
        camera_tone,
        lens_db,
        sensor_scale,
        capture_radius,
        capture_threshold,
        look_curve,
        profile_curve,
    } = info;
    let lens_db = lens_db.map(|value| {
        use lightcraft_pipeline::lensdb::{Distortion, LensCorrection, Tca};
        let LensCorrection { diag_norm, center, distortion, tca, vignetting } = value;
        let distortion = match distortion {
            Distortion::None => (0u8, [0.0; 3]),
            Distortion::Poly3 { k1 } => (1, [k1, 0.0, 0.0]),
            Distortion::Poly5 { k1, k2 } => (2, [k1, k2, 0.0]),
            Distortion::Ptlens { a, b, c } => (3, [a, b, c]),
        };
        let tca = match tca {
            Tca::None => (0u8, [0.0; 6]),
            Tca::Linear { kr, kb } => (1, [kr, kb, 0.0, 0.0, 0.0, 0.0]),
            Tca::Poly3 { red, blue } => (2, [red[0], red[1], red[2], blue[0], blue[1], blue[2]]),
        };
        (diag_norm, center, distortion, tca, vignetting)
    });
    fingerprint(&(
        (
            raw_clip_level,
            camera_noise,
            camera_wb_presets,
            lens,
            raw,
            as_shot_temp,
            as_shot_tint,
            relative_wb,
            camera_color,
            baseline_gain,
            camera_tone,
            sensor_scale,
            capture_radius,
            capture_threshold,
            look_curve,
            profile_curve,
        ),
        lens_db,
    ))
}

/// Unlike JSON hashing, this stream preserves signed zero and rejects all
/// nonfinite numbers. Type/length/field tags prevent structural ambiguities.
fn fingerprint<T: Serialize + ?Sized>(value: &T) -> Option<Hash128> {
    let mut digest = Digest::default();
    digest.feed(value).ok()?;
    Some(digest.finish())
}
#[derive(Debug)]
struct Unsupported;
impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unsupported or nonfinite cache evidence")
    }
}
impl std::error::Error for Unsupported {}
impl ser::Error for Unsupported {
    fn custom<T: fmt::Display>(_: T) -> Self {
        Self
    }
}
#[derive(Default)]
struct Digest(Hasher128);
impl Digest {
    fn tag(&mut self, tag: u8) {
        self.0.update(&[tag]);
    }
    fn bytes(&mut self, bytes: &[u8]) {
        self.0.u64(bytes.len() as u64).update(bytes);
    }
    fn name(&mut self, name: &str) {
        self.bytes(name.as_bytes());
    }
    fn len(&mut self, len: Option<usize>) {
        self.0.u64(u64::from(len.is_some())).u64(len.unwrap_or(0) as u64);
    }
    fn feed<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Unsupported> {
        value.serialize(self)
    }
    fn finish(&self) -> Hash128 {
        self.0.finish()
    }
}
struct Compound<'a>(&'a mut Digest);
macro_rules! integer {
    ($method:ident, $ty:ty, $tag:expr) => {
        fn $method(self, value: $ty) -> Result<(), Unsupported> {
            self.tag($tag);
            self.0.update(&value.to_le_bytes());
            Ok(())
        }
    };
}
impl<'a> ser::Serializer for &'a mut Digest {
    type Ok = ();
    type Error = Unsupported;
    type SerializeSeq = Compound<'a>;
    type SerializeTuple = Compound<'a>;
    type SerializeTupleStruct = Compound<'a>;
    type SerializeTupleVariant = Compound<'a>;
    type SerializeMap = Compound<'a>;
    type SerializeStruct = Compound<'a>;
    type SerializeStructVariant = Compound<'a>;
    fn serialize_bool(self, value: bool) -> Result<(), Unsupported> {
        self.tag(1);
        self.tag(u8::from(value));
        Ok(())
    }
    integer!(serialize_i8, i8, 2);
    integer!(serialize_i16, i16, 3);
    integer!(serialize_i32, i32, 4);
    integer!(serialize_i64, i64, 5);
    integer!(serialize_i128, i128, 6);
    integer!(serialize_u8, u8, 7);
    integer!(serialize_u16, u16, 8);
    integer!(serialize_u32, u32, 9);
    integer!(serialize_u64, u64, 10);
    integer!(serialize_u128, u128, 11);
    fn serialize_f32(self, value: f32) -> Result<(), Unsupported> {
        if !value.is_finite() {
            return Err(Unsupported);
        }
        self.tag(12);
        self.0.update(&value.to_bits().to_le_bytes());
        Ok(())
    }
    fn serialize_f64(self, value: f64) -> Result<(), Unsupported> {
        if !value.is_finite() {
            return Err(Unsupported);
        }
        self.tag(13);
        self.0.update(&value.to_bits().to_le_bytes());
        Ok(())
    }
    fn serialize_char(self, value: char) -> Result<(), Unsupported> {
        self.tag(14);
        self.0.update(&(value as u32).to_le_bytes());
        Ok(())
    }
    fn serialize_str(self, value: &str) -> Result<(), Unsupported> {
        self.tag(15);
        self.name(value);
        Ok(())
    }
    fn serialize_bytes(self, value: &[u8]) -> Result<(), Unsupported> {
        self.tag(16);
        self.bytes(value);
        Ok(())
    }
    fn serialize_none(self) -> Result<(), Unsupported> {
        self.tag(17);
        Ok(())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), Unsupported> {
        self.tag(18);
        self.feed(value)
    }
    fn serialize_unit(self) -> Result<(), Unsupported> {
        self.tag(19);
        Ok(())
    }
    fn serialize_unit_struct(self, name: &'static str) -> Result<(), Unsupported> {
        self.tag(20);
        self.name(name);
        Ok(())
    }
    fn serialize_unit_variant(self, name: &'static str, index: u32, variant: &'static str) -> Result<(), Unsupported> {
        self.tag(21);
        self.name(name);
        self.0.u64(u64::from(index));
        self.name(variant);
        Ok(())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, name: &'static str, value: &T) -> Result<(), Unsupported> {
        self.tag(22);
        self.name(name);
        self.feed(value)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<(), Unsupported> {
        self.tag(23);
        self.name(name);
        self.0.u64(u64::from(index));
        self.name(variant);
        self.feed(value)
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Compound<'a>, Unsupported> {
        self.tag(24);
        self.len(len);
        Ok(Compound(self))
    }
    fn serialize_tuple(self, len: usize) -> Result<Compound<'a>, Unsupported> {
        self.tag(25);
        self.len(Some(len));
        Ok(Compound(self))
    }
    fn serialize_tuple_struct(self, name: &'static str, len: usize) -> Result<Compound<'a>, Unsupported> {
        self.tag(26);
        self.name(name);
        self.len(Some(len));
        Ok(Compound(self))
    }
    fn serialize_tuple_variant(self, name: &'static str, index: u32, variant: &'static str, len: usize) -> Result<Compound<'a>, Unsupported> {
        self.tag(27);
        self.name(name);
        self.0.u64(u64::from(index));
        self.name(variant);
        self.len(Some(len));
        Ok(Compound(self))
    }
    fn serialize_map(self, len: Option<usize>) -> Result<Compound<'a>, Unsupported> {
        self.tag(28);
        self.len(len);
        Ok(Compound(self))
    }
    fn serialize_struct(self, name: &'static str, len: usize) -> Result<Compound<'a>, Unsupported> {
        self.tag(29);
        self.name(name);
        self.len(Some(len));
        Ok(Compound(self))
    }
    fn serialize_struct_variant(self, name: &'static str, index: u32, variant: &'static str, len: usize) -> Result<Compound<'a>, Unsupported> {
        self.tag(30);
        self.name(name);
        self.0.u64(u64::from(index));
        self.name(variant);
        self.len(Some(len));
        Ok(Compound(self))
    }
}
macro_rules! elements {
    ($trait:ident, $method:ident) => {
        impl ser::$trait for Compound<'_> {
            type Ok = ();
            type Error = Unsupported;
            fn $method<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Unsupported> {
                self.0.feed(value)
            }
            fn end(self) -> Result<(), Unsupported> {
                self.0.tag(31);
                Ok(())
            }
        }
    };
}
elements!(SerializeSeq, serialize_element);
elements!(SerializeTuple, serialize_element);
elements!(SerializeTupleStruct, serialize_field);
elements!(SerializeTupleVariant, serialize_field);
impl ser::SerializeMap for Compound<'_> {
    type Ok = ();
    type Error = Unsupported;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Unsupported> {
        self.0.tag(32);
        self.0.feed(value)
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Unsupported> {
        self.0.tag(33);
        self.0.feed(value)
    }
    fn end(self) -> Result<(), Unsupported> {
        self.0.tag(31);
        Ok(())
    }
}
macro_rules! fields {
    ($trait:ident) => {
        impl ser::$trait for Compound<'_> {
            type Ok = ();
            type Error = Unsupported;
            fn serialize_field<T: Serialize + ?Sized>(&mut self, name: &'static str, value: &T) -> Result<(), Unsupported> {
                self.0.tag(34);
                self.0.name(name);
                self.0.feed(value)
            }
            fn end(self) -> Result<(), Unsupported> {
                self.0.tag(31);
                Ok(())
            }
        }
    };
}
fields!(SerializeStruct);
fields!(SerializeStructVariant);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Session, media::SourceRef};
    use lightcraft_pipeline::lensdb::{Distortion, LensCorrection};

    fn pixels() -> DecodedSource {
        DecodedSource::new(Arc::new(lightcraft_raster::Rgb32f::from_fn(12, 8, |x, y| [0.03 + x as f32 * 0.02, 0.1 + y as f32 * 0.01, 0.2])), None)
    }
    #[test]
    fn source_witness_matches_identical_reload_but_rejects_changed_pixels_and_facts() {
        let source = pixels();
        let info = SourceInfo::default();
        let evidence = SourceEvidence::capture(&source, &info).unwrap();
        assert!(evidence.matches(&source, &info));
        let reloaded = DecodedSource::new(Arc::new((*source.image).clone()), None);
        assert!(evidence.matches(&reloaded, &info), "eviction/redecode alone must not change the mathematical source");
        let mut changed = (*source.image).clone();
        changed.data[7][1] += 0.001;
        assert!(!evidence.matches(&DecodedSource::new(Arc::new(changed), None), &info));
        let mut changed = info.clone();
        changed.sensor_scale = 2.0;
        assert!(!evidence.matches(&source, &changed));
        let mut changed = info.clone();
        changed.lens_db = Some(LensCorrection { distortion: Distortion::Poly3 { k1: 0.2 }, ..Default::default() });
        assert!(!evidence.matches(&source, &changed), "serde-skipped lens_db must still invalidate evidence");
        let mut changed = source.clone();
        changed.raw_key = 77;
        assert!(!evidence.matches(&changed, &info));
        let mut changed = info.clone();
        changed.as_shot_tint = -0.0;
        assert!(!evidence.matches(&source, &changed), "signed zero is part of exact facts");
        let mut changed = info.clone();
        changed.baseline_gain = f32::INFINITY;
        assert!(SourceEvidence::capture(&source, &changed).is_none());
        let mut changed = (*source.image).clone();
        changed.data[0][0] = f32::NAN;
        assert!(SourceEvidence::capture(&DecodedSource::new(Arc::new(changed), None), &info).is_none());
    }
    #[test]
    fn source_witness_does_not_pin_confidence_profile_or_pixel_buffers() {
        let source = pixels();
        let clip = Arc::new(ClipConfidence { width: 2, height: 2, data: vec![0.0, 0.2, 0.4, 1.0] });
        let profile = Arc::new(li_color::profile::ProfileLook {
            look_table: Some(li_color::profile::HsvTable {
                hue_divisions: 1,
                sat_divisions: 2,
                val_divisions: 1,
                data: vec![[0.0, 1.0, 1.0], [4.0, 0.9, 1.1]],
                srgb_value: false,
            }),
            ..Default::default()
        });
        let info = SourceInfo { clip_confidence: Some(clip.clone()), camera_profile: Some(profile.clone()), ..Default::default() };
        let pixel_count = Arc::strong_count(&source.image);
        let clip_count = Arc::strong_count(&clip);
        let profile_count = Arc::strong_count(&profile);
        let evidence = SourceEvidence::capture(&source, &info).unwrap();
        assert_eq!(Arc::strong_count(&source.image), pixel_count);
        assert_eq!(Arc::strong_count(&clip), clip_count);
        assert_eq!(Arc::strong_count(&profile), profile_count);
        let identical_info =
            SourceInfo { clip_confidence: Some(Arc::new((*clip).clone())), camera_profile: Some(Arc::new((*profile).clone())), ..Default::default() };
        assert!(evidence.matches(&source, &identical_info));
        let mut changed = (*clip).clone();
        changed.data[2] += 0.01;
        let changed_info = SourceInfo { clip_confidence: Some(Arc::new(changed)), ..identical_info.clone() };
        assert!(!evidence.matches(&source, &changed_info));
        let mut changed = (*profile).clone();
        changed.look_table.as_mut().unwrap().data[0][0] = 2.0;
        let changed_info = SourceInfo { camera_profile: Some(Arc::new(changed)), ..identical_info };
        assert!(!evidence.matches(&source, &changed_info));
        let pixel_weak = Arc::downgrade(&source.image);
        let clip_weak = Arc::downgrade(&clip);
        let profile_weak = Arc::downgrade(&profile);
        drop(source);
        drop(info);
        drop(clip);
        drop(profile);
        drop(changed_info);
        assert!(pixel_weak.upgrade().is_none() && clip_weak.upgrade().is_none() && profile_weak.upgrade().is_none());
        assert!(evidence.pixels.upgrade().is_none());
    }
    #[test]
    fn typed_plane_witness_preserves_exact_bits_shapes_and_rebinds_only_after_validation() {
        // Cross the fixed chunk boundary and reject even a one-bit finite change.
        let values = vec![0.0f32; 4097];
        let digest = f32_buffer_digest("clip", 4097, 1, 1, &values).unwrap();
        let mut changed = values.clone();
        changed[4096] = f32::from_bits(1);
        assert_ne!(f32_buffer_digest("clip", 4097, 1, 1, &changed), Some(digest));
        changed[4096] = -0.0;
        assert_ne!(f32_buffer_digest("clip", 4097, 1, 1, &changed), Some(digest));
        assert_ne!(f32_buffer_digest("rgb", 4097, 1, 1, &values), Some(digest));
        assert_ne!(f32_buffer_digest("clip", 1, 4097, 1, &values), Some(digest));
        assert_ne!(f32_buffer_digest("clip", 1, 1, 4097, &values), Some(digest));
        assert!(f32_buffer_digest("clip", 4096, 1, 1, &values).is_none());
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            changed[4096] = invalid;
            assert!(f32_buffer_digest("clip", 4097, 1, 1, &changed).is_none());
        }

        let source = pixels();
        let info = SourceInfo {
            clip_confidence: Some(Arc::new(ClipConfidence { width: 2, height: 2, data: vec![0.0; 4] })),
            camera_profile: Some(Arc::new(li_color::profile::ProfileLook::default())),
            ..Default::default()
        };
        let evidence = SourceEvidence::capture(&source, &info).unwrap();
        let reloaded = DecodedSource::new(Arc::new((*source.image).clone()), None);
        let mut reloaded_info = info.clone();
        reloaded_info.clip_confidence = info.clip_confidence.as_ref().map(|clip| Arc::new((**clip).clone()));
        reloaded_info.camera_profile = info.camera_profile.as_ref().map(|profile| Arc::new((**profile).clone()));
        let counts = (
            Arc::strong_count(&reloaded.image),
            Arc::strong_count(reloaded_info.clip_confidence.as_ref().unwrap()),
            Arc::strong_count(reloaded_info.camera_profile.as_ref().unwrap()),
        );
        let refreshed = evidence.validated(&reloaded, &reloaded_info).unwrap();
        assert!(Weak::ptr_eq(&refreshed.pixels, &Arc::downgrade(&reloaded.image)));
        assert!(Weak::ptr_eq(&refreshed.clip.as_ref().unwrap().1, &Arc::downgrade(reloaded_info.clip_confidence.as_ref().unwrap())));
        assert!(Weak::ptr_eq(&refreshed.profile.as_ref().unwrap().1, &Arc::downgrade(reloaded_info.camera_profile.as_ref().unwrap())));
        assert_eq!(
            counts,
            (
                Arc::strong_count(&reloaded.image),
                Arc::strong_count(reloaded_info.clip_confidence.as_ref().unwrap()),
                Arc::strong_count(reloaded_info.camera_profile.as_ref().unwrap())
            )
        );
        assert_eq!(refreshed.pixels_digest, evidence.pixels_digest);
        let mut changed_info = reloaded_info.clone();
        let mut changed_clip = (**changed_info.clip_confidence.as_ref().unwrap()).clone();
        changed_clip.data[0] = -0.0;
        changed_info.clip_confidence = Some(Arc::new(changed_clip));
        assert!(evidence.validated(&reloaded, &changed_info).is_none());
        let mut changed_image = (*reloaded.image).clone();
        changed_image.data[0][0] = f32::from_bits(changed_image.data[0][0].to_bits() + 1);
        assert!(evidence.validated(&DecodedSource::new(Arc::new(changed_image), None), &reloaded_info).is_none());
        drop(reloaded);
        drop(reloaded_info);
        drop(changed_info);
        assert!(refreshed.pixels.upgrade().is_none());
        assert!(refreshed.clip.as_ref().unwrap().1.upgrade().is_none());
        assert!(refreshed.profile.as_ref().unwrap().1.upgrade().is_none());
    }
    fn batch() -> (Session, Vec<Input>, Vec<Look>, Options) {
        let mut session = Session::with_demo();
        let ids = session.catalog.photos().take(3).map(|photo| photo.id).collect::<Vec<_>>();
        let inputs = ids.iter().map(|id| super::super::input(&mut session, *id).unwrap()).collect();
        let source = pixels();
        let looks = vec![lightcraft_pipeline::match_look::analyze(&source.image, &SourceInfo::default(), &DevelopSettings::default())];
        (session, inputs, looks, Options { consistent_batch: true, ..Default::default() })
    }
    #[test]
    fn complete_batch_key_normalizes_only_requested_strength_and_is_active_independent() {
        let (session, inputs, looks, options) = batch();
        let ids = inputs.iter().map(|input| input.identity.id).collect::<Vec<_>>();
        let key = BatchKey::capture(session.library_generation, &ids, &inputs, &looks, options).unwrap();
        let mut stale = key.clone();
        stale.epochs.camera ^= 1;
        assert!(!stale.resources_current());
        assert_ne!(stale, key);
        let mut reweighted = looks.clone();
        reweighted[0].importance *= 2.0;
        assert_ne!(BatchKey::capture(session.library_generation, &ids, &inputs, &reweighted, options), Some(key.clone()));
        assert_eq!(BatchKey::capture(session.library_generation, &ids, &inputs, &looks, Options { strength: 42.0, ..options }), Some(key.clone()));
        let mut reordered = inputs.clone();
        reordered.reverse();
        let mut reverse_ids = ids.clone();
        reverse_ids.reverse();
        assert_eq!(BatchKey::capture(session.library_generation, &reverse_ids, &reordered, &looks, options), Some(key.clone()));
        for altered in [
            Options { keep_exposure: true, ..options },
            Options { keep_white_balance: true, ..options },
            Options { new_layer: true, ..options },
            Options { protect_skin: false, ..options },
            Options { appearance_aware: true, ..options },
        ] {
            assert_ne!(BatchKey::capture(session.library_generation, &ids, &inputs, &looks, altered), Some(key.clone()));
        }
        let mut altered = inputs.clone();
        altered[0].faces.push([0.1, 0.2, 0.3, 0.4]);
        assert_ne!(BatchKey::capture(session.library_generation, &ids, &altered, &looks, options), Some(key.clone()));
        let mut altered = inputs.clone();
        let mut baseline = (*altered[0].baseline).clone();
        baseline.light.contrast = 12.0;
        altered[0].baseline = Arc::new(baseline);
        assert_ne!(BatchKey::capture(session.library_generation, &ids, &altered, &looks, options), Some(key.clone()));
        let mut altered = inputs.clone();
        altered[0].placement = Some(lightcraft_pipeline::match_look::LayerPlacement { index: 0, id: 7 });
        assert_ne!(BatchKey::capture(session.library_generation, &ids, &altered, &looks, options), Some(key.clone()));
        assert!(
            BatchKey::capture(session.library_generation, &ids, &inputs[..2], &looks, options).is_none(),
            "preparation failures cannot be omitted from a reusable batch"
        );
        assert_ne!(BatchKey::capture(session.library_generation + 1, &ids, &inputs, &looks, options), Some(key));
    }
    #[test]
    fn cache_entry_rejects_partial_or_failed_audit_and_strips_unrelated_tools() {
        let (session, inputs, looks, options) = batch();
        let ids = inputs.iter().map(|input| input.identity.id).collect::<Vec<_>>();
        let key = BatchKey::capture(session.library_generation, &ids, &inputs, &looks, options).unwrap();
        let source = pixels();
        let evidence = SourceEvidence::capture(&source, &SourceInfo::default()).unwrap();
        let sources = ids.iter().map(|id| (*id, evidence.clone())).collect::<BTreeMap<_, _>>();
        let mut style = DevelopSettings::default();
        style.color.saturation = -20.0;
        style.light.exposure = 3.0;
        style.masks.push(lightcraft_develop::Mask { id: 7, name: "Must not be cached".into(), ..Default::default() });
        style.detail.sharpen_amount = 70.0;
        assert!(SharedStyle::complete(key.clone(), &style, options, ids[0], &ids[..2], &[], sources.clone(), BTreeMap::new()).is_none());
        assert!(SharedStyle::complete(key.clone(), &style, options, ids[0], &ids, &[ids[0]], sources.clone(), BTreeMap::new()).is_none());
        let entry = SharedStyle::complete(key, &style, options, ids[0], &ids, &[], sources, BTreeMap::new()).unwrap();
        assert_eq!(entry.color.color.saturation, -20.0);
        assert!(entry.color.masks.is_empty());
        assert_eq!(entry.color.detail, DevelopSettings::default().detail);
        assert_eq!(entry.color.light, DevelopSettings::default().light);
        assert!(entry.metadata_current(session.library_generation, &inputs, &looks, options));
        let mut state = super::super::State::default();
        state.shared_style = Some(Arc::new(entry));
        let serialized = serde_json::to_string(&state).unwrap();
        assert!(!serialized.contains("shared_style") && !serialized.contains("sharedStyle"));
        let loaded: super::super::State = serde_json::from_str(&serialized).unwrap();
        assert!(loaded.shared_style.is_none());
        // The key never contains source objects; resolving a different loaded
        // source affects the witness, not this structural metadata snapshot.
        assert!(matches!(inputs[0].source, SourceRef::Demo { .. } | SourceRef::Loaded(_)));
    }
}
