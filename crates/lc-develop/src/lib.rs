//! The non-destructive edit model of LightCraft.
//!
//! [`DevelopSettings`] is the complete description of a photo's look: pure, serializable data.
//! The pipeline evaluates it; the catalog stores it; presets, copy/paste, sync, versions and history
//! are operations on it. Numeric sliders are addressed by id through [`controls`].
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod controls;
pub mod presets;
pub mod segmask;
pub mod settings;

pub use controls::{CONTROLS, ControlSpec, Section, Track};
pub use presets::{Preset, SettingsGroup, apply_partial, extract_groups};
pub use segmask::SegMask;
pub use settings::*;

use serde_json::Value;

impl DevelopSettings {
    /// Settings for a freshly imported raw file (Lightroom applies capture sharpening and colour
    /// noise reduction by default to raw files).
    pub fn for_raw(as_shot_temp: f64, as_shot_tint: f64) -> DevelopSettings {
        let mut s = DevelopSettings::default();
        s.wb.temp = as_shot_temp;
        s.wb.tint = as_shot_tint;
        s.detail.sharpen_amount = 40.0;
        s.detail.nr_color = 25.0;
        s
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// [`Self::to_json`] including the sections it leaves out while they are at their defaults
    /// (`negative`), for code that compares or copies settings key by key (copy/paste groups,
    /// preset amounts, Auto Sync deltas).
    pub fn to_json_full(&self) -> Value {
        let mut v = self.to_json();
        if let Some(o) = v.as_object_mut() {
            o.entry("negative").or_insert_with(|| serde_json::to_value(self.negative).unwrap_or(Value::Null));
        }
        v
    }

    pub fn from_json(v: &Value) -> Result<DevelopSettings, serde_json::Error> {
        serde_json::from_value(v.clone())
    }

    /// Merge a partial JSON object into these settings (fields not mentioned are kept).
    pub fn merged(&self, partial: &Value) -> Result<DevelopSettings, serde_json::Error> {
        let mut v = self.to_json();
        presets::deep_merge(&mut v, partial);
        DevelopSettings::from_json(&v)
    }

    /// Stable 64-bit hash of the settings (FNV-1a over canonical JSON), for preview cache keys.
    pub fn hash64(&self) -> u64 {
        let s = serde_json::to_string(self).unwrap_or_default();
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in s.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h
    }

    /// True if nothing differs from a fresh default (ignoring white balance "as shot" values).
    pub fn is_unedited(&self) -> bool {
        let mut a = self.clone();
        let d = DevelopSettings::default();
        a.wb = d.wb;
        a == d
    }

    pub fn section_enabled(&self, section: &str) -> bool {
        !self.disabled_sections.iter().any(|s| s == section)
    }

    pub fn set_section_enabled(&mut self, section: &str, on: bool) {
        self.disabled_sections.retain(|s| s != section);
        if !on {
            self.disabled_sections.push(section.to_string());
        }
    }

    /// Reset every control in `section` to its default.
    pub fn reset_section(&mut self, section: Section) {
        for c in controls::in_section(section) {
            controls::set(self, c.id, c.default);
        }
        match section {
            Section::Curve => {
                let d = ToneCurve::default();
                self.curve = d;
            }
            Section::Color => self.wb.mode = WbMode::AsShot,
            // the film stock too; whether the conversion is on stays as it was
            Section::Negative => self.negative = Negative { enabled: self.negative.enabled, ..Negative::default() },
            _ => {}
        }
    }

    /// Next free mask id.
    pub fn next_mask_id(&self) -> u32 {
        self.masks.iter().map(|m| m.id).max().map_or(1, |m| m + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serde_roundtrip_default_and_edited() {
        let mut s = DevelopSettings::default();
        controls::set(&mut s, "light.exposure", 1.25);
        s.masks.push(Mask {
            id: 1,
            components: vec![MaskComponent {
                name: None,
                op: MaskOp::Add,
                invert: false,
                shape: MaskShape::Radial {
                    center: lightcraft_geom::Point::new(0.5, 0.5),
                    rx: 0.2,
                    ry: 0.1,
                    angle: 10.0,
                    feather: 50.0,
                    invert: false,
                },
            }],
            ..Default::default()
        });
        let v = s.to_json();
        let back = DevelopSettings::from_json(&v).unwrap();
        assert_eq!(back, s);
        assert_ne!(back.hash64(), DevelopSettings::default().hash64());
    }

    #[test]
    fn missing_fields_take_defaults_and_unknown_ignored() {
        let s = DevelopSettings::from_json(&json!({"light": {"exposure": 0.5}, "futureThing": 3})).unwrap();
        assert_eq!(s.light.exposure, 0.5);
        assert_eq!(s.grading.blending, 50.0);
    }

    #[test]
    fn merge_partial() {
        let s = DevelopSettings::default();
        let m = s.merged(&json!({"light": {"contrast": 20}, "effects": {"clarity": 10}})).unwrap();
        assert_eq!(m.light.contrast, 20.0);
        assert_eq!(m.effects.clarity, 10.0);
        assert_eq!(m.light.exposure, 0.0);
    }

    #[test]
    fn unedited_and_reset() {
        let mut s = DevelopSettings::default();
        s.wb.temp = 5000.0;
        assert!(s.is_unedited());
        s.light.exposure = 1.0;
        s.light.shadows = 30.0;
        assert!(!s.is_unedited());
        s.reset_section(Section::Light);
        assert!(s.is_unedited());
    }

    #[test]
    fn negative_defaults_off_and_old_json_reads_as_disabled() {
        // settings written before the negative conversion existed
        let old = json!({"version": 1, "light": {"exposure": 0.3}, "calibration": {"red_hue": 5.0}, "disabled_sections": []});
        let s = DevelopSettings::from_json(&old).unwrap();
        assert!(!s.negative.enabled);
        assert_eq!(s.negative, Negative::default());
        // a default conversion is left out of the JSON: existing settings hash and serialize as before
        assert!(s.to_json().get("negative").is_none());
        assert!(DevelopSettings::default().to_json().get("negative").is_none());
        assert!(s.to_json_full().get("negative").is_some());
        // a partial section fills in the rest from the defaults
        let s = DevelopSettings::from_json(&json!({"negative": {"enabled": true, "film": "bw", "dmin": {"r": 0.8}}})).unwrap();
        assert!(s.negative.enabled);
        assert_eq!(s.negative.film, FilmStock::Bw);
        assert_eq!(s.negative.dmin, FilmRgb::new(0.8, 1.0, 1.0));
        assert_eq!(s.negative.d_max, 2.046);
        assert!(!s.is_unedited());
        // round trip, and the hash sees the section
        let back = DevelopSettings::from_json(&s.to_json()).unwrap();
        assert_eq!(back, s);
        assert_ne!(s.hash64(), DevelopSettings::default().hash64());
        let mut stock = DevelopSettings::default();
        stock.negative.film = FilmStock::Slide;
        assert_eq!(DevelopSettings::from_json(&stock.to_json()).unwrap(), stock);
    }

    #[test]
    fn negative_merges_copies_and_resets() {
        let s = DevelopSettings::default().merged(&json!({"negative": {"enabled": true, "d_max": 1.6}})).unwrap();
        assert!(s.negative.enabled);
        assert_eq!(s.negative.d_max, 1.6);
        assert_eq!(s.negative.gamma, 4.0);
        // copy/paste: the Negative group carries the conversion, also when it is off (and left out
        // of the JSON) on the source photo
        let off = extract_groups(&DevelopSettings::default(), &[SettingsGroup::Negative]);
        assert_eq!(off["negative"]["enabled"], json!(false));
        let pasted = apply_partial(&s, &off, 1.0);
        assert_eq!(pasted.negative, Negative::default());
        let on = extract_groups(&s, &SettingsGroup::default_copy());
        assert_eq!(apply_partial(&DevelopSettings::default(), &on, 1.0).negative, s.negative);
        // a preset at a low amount onto a photo without a conversion still applies
        let half = apply_partial(&DevelopSettings::default(), &json!({"negative": {"d_max": 3.046}}), 0.25);
        assert!((half.negative.d_max - 2.296).abs() < 1e-9, "{}", half.negative.d_max);
        // controls address the section
        let mut c = s.clone();
        assert!(controls::set(&mut c, "negative.dminG", 0.5));
        assert!(controls::set(&mut c, "negative.wbHighB", 9.0));
        assert_eq!(c.negative.dmin.g, 0.5);
        assert_eq!(c.negative.wb_high.b, 2.0, "clamped to negadoctor's range");
        c.negative.film = FilmStock::Bw;
        c.reset_section(Section::Negative);
        assert_eq!(c.negative, Negative { enabled: true, ..Negative::default() });
    }

    #[test]
    fn section_toggles() {
        let mut s = DevelopSettings::default();
        assert!(s.section_enabled("effects"));
        s.set_section_enabled("effects", false);
        s.set_section_enabled("effects", false);
        assert!(!s.section_enabled("effects"));
        assert_eq!(s.disabled_sections.len(), 1);
        s.set_section_enabled("effects", true);
        assert!(s.section_enabled("effects"));
    }
}
