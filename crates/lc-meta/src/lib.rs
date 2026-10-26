//! Photo metadata for LightCraft: EXIF (TIFF/raw files, JPEG APP1, PNG `eXIf`, WebP `EXIF`), XMP packets
//! (read + write, standard namespaces plus our own `lc:` namespace) and basic IPTC-IIM.
//!
//! Entry points:
//! - [`read_exif`] — a TIFF-structured Exif block → [`Metadata`]; [`from_tiff`] for an already parsed stream.
//! - [`embedded`] / [`jpeg_segments`] / [`png_chunks`] / [`webp_chunks`] — locate Exif / XMP / ICC / IPTC blocks.
//! - [`parse_xmp`] / [`write_xmp`] — XMP packets (interchange fields + the opaque `lc:settings` JSON).
//! - [`parse_iptc`] — IPTC-IIM record 2 datasets.
//! - [`parse_gpx`] — GPS track logs (GPX) for geotagging by capture time.
//! - [`extract`] — all of the above for a whole file (JPEG, PNG, WebP, TIFF/DNG/raw, Canon CR3).
//! - [`cr3`] — the Canon CR3 container (metadata blocks, XMP, the JPEG and raw tracks).
#![forbid(unsafe_code)]
#![expect(clippy::map_err_ignore, reason = "baseline 2026-10-25: unreviewed")]
#![cfg_attr(test, expect(clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed"))]

mod container;
mod tiff_replace;
pub use tiff_replace::preserve_tiff_metadata;
pub mod cr3;
mod datetime;
mod dji;
mod sony;
pub use dji::{DJI_NS, DjiMetadata};
pub use sony::{SonyLensCorrection, SonyMetadata, SonyPixelShift};
mod exif;
mod file_info;
mod gpx;
mod iptc;
pub mod tags;
mod xmp;
mod xmp_orientation;
pub use file_info::{copyright_xmp, read_file_info, try_write_file_info};
pub use xmp_orientation::upright_xmp;
mod xmp_merge;

pub use container::{Embedded, embedded, jpeg_segments, png_chunks, webp_chunks};
pub use datetime::DateTime;
pub use exif::{exif_ifd, from_tiff, from_tiff_with_bytes, read_exif, strip_exif_header, try_read_exif, write_exif};
pub use gpx::{GpxError, Match, TrackPoint, Tracklog, parse_gpx};
pub use iptc::parse_iptc;
pub use lightcraft_geom::{Orientation, Rect};
pub use tags::{TagRow, file_tag_rows, tag_rows};
pub use xmp::{CRS_NS, LC_NS, XmpData, XmpError, XmpValue, parse_xmp, write_xmp, write_xmp_lc};
pub use xmp_merge::{MergeRules, merge_xmp, normalize_xmp_geometry, normalize_xmp_geometry_with_extended_guid, xmp_has_extended_guid};

use serde::{Deserialize, Serialize};

/// GPS position (WGS 84). Latitude/longitude in signed decimal degrees (north / east positive).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Gps {
    pub latitude: f64,
    pub longitude: f64,
    /// Metres above sea level (negative below).
    pub altitude: Option<f64>,
}

/// Exif `Flash` value decoded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flash {
    pub fired: bool,
    /// The raw Exif `Flash` bit field.
    pub raw: u16,
}

/// A named region of interest on a photo (MWG Region Guidelines, `mwg-rs:Regions`): most often a face,
/// drawn by Lightroom, digiKam, Picasa or similar tools, or named locally by Smart Sort.
/// Interchange is read-only: LightCraft does not write MWG regions to XMP.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Region {
    /// Normalized to the image's full, oriented frame (`Metadata::width`/`height`), y-down, 0..1 on
    /// both axes — MWG's own convention, and what `AppliedToDimensions` areas are converted to.
    pub rect: Rect,
    pub kind: RegionKind,
    /// `mwg-rs:Name` (e.g. the person's name).
    pub name: Option<String>,
    pub description: Option<String>,
    /// Added by Smart Sort; imported/manual regions remain false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto: bool,
}

/// `mwg-rs:Type`. Unrecognised values are kept verbatim rather than dropped.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegionKind {
    Face,
    Pet,
    Focus,
    BarCode,
    /// `mwg-rs:Type` was missing, empty, or a value we don't recognise.
    Other(String),
}

/// Everything LightCraft shows or searches about a photo. All fields are optional; unknown = `None`/empty.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    // camera
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    pub software: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sony: Option<SonyMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dji: Option<DjiMetadata>,
    // lens
    /// The pixels already contain the source's DNG lens corrections (e.g. a merge).
    /// Persisted in lc:lensCorrectionsBaked so an automatic profile is not applied twice.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lens_corrections_baked: bool,
    /// Per-channel geometric DNG correction already baked into the pixels.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub chromatic_aberration_baked: bool,
    pub lens_make: Option<String>,
    pub lens_model: Option<String>,
    pub lens_serial_number: Option<String>,
    /// Min/max focal length (mm) and min/max f-number at those, as in Exif `LensSpecification`.
    pub lens_spec: Option<[f64; 4]>,
    // capture
    pub capture_time: Option<DateTime>,
    /// Seconds.
    pub exposure_time: Option<f64>,
    pub f_number: Option<f64>,
    pub iso: Option<u32>,
    /// Millimetres.
    pub focal_length: Option<f64>,
    pub focal_length_35mm: Option<f64>,
    pub flash: Option<Flash>,
    /// EV.
    pub exposure_bias: Option<f64>,
    /// Exif `ExposureProgram` code.
    pub exposure_program: Option<u16>,
    /// Exif `MeteringMode` code.
    pub metering_mode: Option<u16>,
    /// Exif `WhiteBalance` (0 auto, 1 manual).
    pub white_balance: Option<u16>,
    pub orientation: Option<Orientation>,
    pub gps: Option<Gps>,
    /// Pixel dimensions as recorded (not oriented).
    pub width: Option<u32>,
    pub height: Option<u32>,
    // descriptive / user
    pub artist: Option<String>,
    /// Original multi-creator sequence behind the semicolon-joined display field.
    /// Used only while `artist` still equals this sequence's joined display. A scalar
    /// artist edit is one literal creator, never a string to split on semicolons.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub creators: Vec<String>,
    /// Creator job title (`photoshop:AuthorsPosition`), shared with editor File Info.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_title: Option<String>,
    pub copyright: Option<String>,
    /// Copyright status (`xmpRights:Marked`): `true` = copyrighted, `false` = public domain, `None` = unknown.
    pub copyright_marked: Option<bool>,
    /// Rights usage terms (`xmpRights:UsageTerms`).
    pub usage_terms: Option<String>,
    /// Copyright info URL (`xmpRights:WebStatement`).
    pub copyright_url: Option<String>,
    pub title: Option<String>,
    pub caption: Option<String>,
    /// Accessibility text (`Iptc4xmpCore:AltTextAccessibility`).
    pub alt_text: Option<String>,
    /// Long accessibility description (`Iptc4xmpCore:ExtDescrAccessibility`).
    pub extended_description: Option<String>,
    /// Place within the city (`Iptc4xmpCore:Location`).
    pub sublocation: Option<String>,
    /// `photoshop:City`, `photoshop:State`, `photoshop:Country`.
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
    pub keywords: Vec<String>,
    /// Hierarchical keywords, `|`-separated paths (`lr:hierarchicalSubject` convention).
    pub hierarchical_keywords: Vec<String>,
    /// -1 = rejected, 0 = unrated, 1..=5 stars.
    pub rating: Option<i8>,
    /// Colour label name (e.g. "Red").
    pub label: Option<String>,
    /// Face/pet/focus/barcode regions (`mwg-rs:Regions`), read from XMP only.
    pub regions: Vec<Region>,
}

macro_rules! fill {
    ($self:ident, $o:ident, $($f:ident),*) => { $( if $self.$f.is_none() { $self.$f = $o.$f.clone(); } )* };
}
macro_rules! overlay {
    ($self:ident, $o:ident, $($f:ident),*) => { $( if $o.$f.is_some() { $self.$f = $o.$f.clone(); } )* };
}

impl Metadata {
    /// DJI coordinates fill an absent standard GPS position. DJI altitude uses a camera-specific
    /// datum (often ellipsoidal); it stays in DjiMetadata instead of becoming EXIF mean-sea-level.
    pub(crate) fn fill_dji_gps(&mut self) {
        if self.gps.is_none()
            && let Some(dji) = &self.dji
            && let (Some(latitude), Some(longitude)) = (dji.latitude, dji.longitude)
        {
            self.gps = Some(Gps { latitude, longitude, altitude: None });
        }
    }

    /// Intentionally replace the creator field with one literal name, or clear it.
    /// Unlike retaining the imported display value, this discards the source list.
    pub fn set_artist(&mut self, artist: Option<String>) {
        self.artist = artist;
        self.creators.clear();
    }

    /// Fill every empty field of `self` from `other`.
    pub fn fill_missing(&mut self, other: &Metadata) {
        self.lens_corrections_baked |= other.lens_corrections_baked;
        self.chromatic_aberration_baked |= other.chromatic_aberration_baked;
        if self.artist.is_none() {
            self.artist = other.artist.clone();
            self.creators = other.creators.clone();
        }
        fill!(
            self,
            other,
            make,
            model,
            serial_number,
            software,
            sony,
            dji,
            lens_make,
            lens_model,
            lens_serial_number,
            lens_spec,
            capture_time,
            exposure_time,
            f_number,
            iso,
            focal_length,
            focal_length_35mm,
            flash,
            exposure_bias,
            exposure_program,
            metering_mode,
            white_balance,
            orientation,
            gps,
            width,
            height,
            author_title,
            copyright,
            copyright_marked,
            usage_terms,
            copyright_url,
            title,
            caption,
            alt_text,
            extended_description,
            sublocation,
            city,
            state,
            country,
            rating,
            label
        );
        if self.keywords.is_empty() {
            self.keywords = other.keywords.clone();
        }
        if self.hierarchical_keywords.is_empty() {
            self.hierarchical_keywords = other.hierarchical_keywords.clone();
        }
        if self.regions.is_empty() {
            self.regions = other.regions.clone();
        }
    }

    /// Replace the user-editable fields (rating, label, title, caption, artist, copyright, keywords, GPS) with
    /// `other`'s where `other` has them — the XMP-over-EXIF precedence rule.
    pub fn overlay_user_fields(&mut self, other: &Metadata) {
        if other.artist.is_some() {
            self.artist = other.artist.clone();
            self.creators = other.creators.clone();
        }
        overlay!(self, other, rating, label, title, caption, author_title, copyright, copyright_marked, usage_terms, copyright_url, gps);
        if !other.keywords.is_empty() {
            self.keywords = other.keywords.clone();
        }
        if !other.hierarchical_keywords.is_empty() {
            self.hierarchical_keywords = other.hierarchical_keywords.clone();
        }
        if !other.regions.is_empty() {
            self.regions = other.regions.clone();
        }
    }

    /// Exposure time formatted like cameras show it: `1/250`, `0.5`, `2″`.
    pub fn exposure_display(&self) -> Option<String> {
        let t = self.exposure_time?;
        if t.is_nan() || t <= 0.0 {
            return None;
        }
        Some(if t < 0.3 {
            format!("1/{}", (1.0 / t).round())
        } else if t < 1.0 {
            format!("{}", (t * 10.0).round() / 10.0)
        } else {
            format!("{}″", (t * 10.0).round() / 10.0)
        })
    }

    /// Human name of the metering mode.
    pub fn metering_name(&self) -> Option<&'static str> {
        Some(match self.metering_mode? {
            1 => "Average",
            2 => "Center-weighted average",
            3 => "Spot",
            4 => "Multi-spot",
            5 => "Pattern",
            6 => "Partial",
            255 => "Other",
            _ => "Unknown",
        })
    }

    /// Human name of the exposure program.
    pub fn exposure_program_name(&self) -> Option<&'static str> {
        Some(match self.exposure_program? {
            1 => "Manual",
            2 => "Program AE",
            3 => "Aperture-priority AE",
            4 => "Shutter speed priority AE",
            5 => "Creative (slow speed)",
            6 => "Action (high speed)",
            7 => "Portrait",
            8 => "Landscape",
            9 => "Bulb",
            _ => "Not defined",
        })
    }
}

/// Read everything we understand from a whole file (JPEG, PNG, WebP, TIFF-based incl. DNG/raw).
/// Precedence: EXIF, then IPTC fills gaps, then XMP overrides user fields and fills the rest.
pub fn extract(bytes: &[u8]) -> Metadata {
    extract_with_xmp(bytes).0
}

/// [`extract`] and the file's XMP packet ([`Embedded::xmp`]) from one pass over the container.
pub fn extract_with_xmp(bytes: &[u8]) -> (Metadata, Option<String>) {
    let (e, tiff) = container::blocks(bytes);
    let mut m = match &tiff {
        // a TIFF-based file is its own Exif block: read it in place, from the parse already made
        Some(t) => from_tiff_with_bytes(bytes, t),
        None => e.exif.as_deref().map(read_exif).unwrap_or_default(),
    };
    if let Some(i) = e.iptc.as_deref() {
        m.fill_missing(&parse_iptc(i));
    }
    if let Some(x) = e.xmp.as_deref().and_then(|x| parse_xmp(x).ok()) {
        m.overlay_user_fields(&x.metadata);
        m.fill_missing(&x.metadata);
    }
    m.fill_dji_gps();
    (m, e.xmp)
}

/// Lenient number parsing used by XMP/IPTC: `"28/10"`, `"2.8"`, `"+0.7"`.
pub(crate) fn parse_number(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some((n, d)) = s.split_once('/') {
        let (n, d) = (n.trim().parse::<f64>().ok()?, d.trim().parse::<f64>().ok()?);
        return (d != 0.0).then_some(n / d).filter(|v| v.is_finite());
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A TIFF-based file is read in place (not copied into an Exif block): EXIF facts, then XMP
    /// over the user fields, exactly as reading the copied block did; the packet comes back too.
    #[test]
    fn tiff_metadata_and_xmp_come_from_one_pass() {
        use lightcraft_tiff::{ByteOrder, IfdBuilder, TiffWriter, Value, tags as t};
        let xmp = write_xmp(&Metadata { caption: Some("Keynote".into()), rating: Some(4), ..Default::default() }, None);
        for order in [ByteOrder::Little, ByteOrder::Big] {
            let mut root = IfdBuilder::new();
            root.set(t::MAKE, Value::Ascii("Sony".into()));
            root.set(t::ORIENTATION, Value::Short(vec![6]));
            root.set(t::IMAGE_DESCRIPTION, Value::Ascii("EXIF caption".into()));
            root.set(t::RATING, Value::Short(vec![1]));
            root.set(t::XMP, Value::Byte(xmp.as_bytes().to_vec()));
            let mut bytes = TiffWriter { order, bigtiff: false }.write(&[root]).unwrap();
            bytes.resize(1 << 20, 0);
            let (m, packet) = extract_with_xmp(&bytes);
            let mut copied = read_exif(embedded(&bytes).exif.as_deref().unwrap());
            let x = parse_xmp(&xmp).unwrap().metadata;
            copied.overlay_user_fields(&x);
            copied.fill_missing(&x);
            copied.fill_dji_gps();
            assert_eq!(m, copied);
            assert_eq!(
                (m.make.as_deref(), m.orientation, m.caption.as_deref(), m.rating),
                (Some("Sony"), Some(Orientation::Rotate90), Some("Keynote"), Some(4))
            );
            assert_eq!(packet.as_deref(), Some(xmp.as_str()));
            assert_eq!(extract(&bytes), m);
        }
        assert_eq!(extract_with_xmp(b"not an image"), (Metadata::default(), None));
    }

    #[test]
    fn numbers() {
        assert_eq!(parse_number("28/10"), Some(2.8));
        assert_eq!(parse_number(" +0.7 "), Some(0.7));
        assert_eq!(parse_number("1/0"), None);
        assert_eq!(parse_number("abc"), None);
        assert_eq!(parse_number("inf"), None);
    }

    #[test]
    fn exposure_display() {
        let mut m = Metadata { exposure_time: Some(1.0 / 250.0), ..Default::default() };
        assert_eq!(m.exposure_display().as_deref(), Some("1/250"));
        m.exposure_time = Some(0.5);
        assert_eq!(m.exposure_display().as_deref(), Some("0.5"));
        m.exposure_time = Some(2.0);
        assert_eq!(m.exposure_display().as_deref(), Some("2″"));
        m.exposure_time = Some(0.0);
        assert_eq!(m.exposure_display(), None);
    }

    #[test]
    fn merge_rules() {
        let mut a = Metadata { make: Some("A".into()), rating: Some(1), ..Default::default() };
        let b = Metadata { make: Some("B".into()), model: Some("M".into()), rating: Some(4), keywords: vec!["k".into()], ..Default::default() };
        a.fill_missing(&b);
        assert_eq!(a.make.as_deref(), Some("A"));
        assert_eq!(a.model.as_deref(), Some("M"));
        assert_eq!(a.rating, Some(1));
        a.overlay_user_fields(&b);
        assert_eq!(a.rating, Some(4));
        assert_eq!(a.make.as_deref(), Some("A"));
        assert_eq!(a.keywords, vec!["k".to_string()]);
    }

    #[test]
    fn names() {
        let m = Metadata { metering_mode: Some(5), exposure_program: Some(3), ..Default::default() };
        assert_eq!(m.metering_name(), Some("Pattern"));
        assert_eq!(m.exposure_program_name(), Some("Aperture-priority AE"));
    }
}

#[cfg(test)]
mod region_auto_tests {
    use super::*;
    #[test]
    fn old_regions_default_to_manual_and_auto_flag_roundtrips() {
        let old = serde_json::json!({"rect":{"x0":0.1,"y0":0.2,"x1":0.3,"y1":0.4},"kind":"Face","name":"Jane","description":null});
        let mut region: Region = serde_json::from_value(old).unwrap();
        assert!(!region.auto);
        assert!(serde_json::to_value(&region).unwrap().get("auto").is_none());
        region.auto = true;
        let back: Region = serde_json::from_value(serde_json::to_value(&region).unwrap()).unwrap();
        assert!(back.auto);
        // The local provenance flag and region names are not emitted to interchange XMP.
        let metadata = Metadata { regions: vec![region], ..Default::default() };
        let xmp = write_xmp(&metadata, None);
        assert!(!xmp.contains("Jane"));
        assert!(!xmp.contains("auto"));
    }
}

#[cfg(test)]
mod sony_dji_owner_tests {
    #[test]
    fn sony_dji_owner_metadata_matches_exiftool_reference_and_export() {
        let Some(root) = std::env::var_os("LOCAL_IMAGE_OWNER_SHOTS") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        // Owner-derived reference values stay in ignored target/, never in repository fixtures.
        let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/sony-dji-reference/owner-metadata-reference.json");
        let reference: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(cache).unwrap()).unwrap();
        assert_eq!(reference.len(), 2, "the local ExifTool cache must contain one Sony and one DJI reference");
        for r in reference {
            let original = std::path::Path::new(r["SourceFile"].as_str().unwrap());
            let bytes = std::fs::read(root.join("Capture").join(original.file_name().unwrap())).unwrap();
            let metadata = super::extract(&bytes);
            if let Some(sony) = &metadata.sony {
                assert_eq!(metadata.model.as_deref(), r["Model"].as_str());
                assert_eq!(metadata.lens_model.as_deref(), r["LensModel"].as_str());
                let count =
                    r["ShutterCount"].as_str().and_then(|s| s.trim().parse::<u32>().ok()).or_else(|| r["ShutterCount"].as_u64().map(|v| v as u32));
                assert_eq!(sony.shutter_count, count);
                assert_eq!(sony.internal_serial_number.as_deref(), r["InternalSerialNumber"].as_str());
                assert_eq!(sony.focus_mode.as_deref(), r["FocusMode"].as_str());
                assert_eq!(sony.focus_area.as_deref(), r["AFAreaModeSetting"].as_str());
                assert_eq!(sony.creative_style.as_deref(), r["CreativeStyle"].as_str());
                assert_eq!(sony.picture_profile.as_deref(), r["PictureProfile"].as_str().map(|s| s.split(" (PP").next().unwrap()));
            } else {
                let d = metadata.dji.as_ref().unwrap();
                assert_eq!(d.gimbal_yaw, r["GimbalYawDegree"].as_f64());
                assert_eq!(
                    d.relative_altitude,
                    r["RelativeAltitude"].as_str().and_then(super::parse_number).or_else(|| r["RelativeAltitude"].as_f64())
                );
                assert_eq!(super::parse_xmp(&super::write_xmp(&metadata, None)).unwrap().metadata.dji, metadata.dji);
            }
        }
    }
}

#[cfg(test)]
mod sony_cc0_metadata_tests {
    #[test]
    fn sony_metadata_cc0_reference_across_body_generations() {
        let Some(root) = std::env::var_os("LOCAL_IMAGE_SONY_METADATA_CC0") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        for (file, model, count, serial, focus) in [
            ("2414.ARW", "ILCE-7M3", Some(10), Some("6eff0000aa09"), "AF-S"),
            ("3480.ARW", "ILCE-7RM4", Some(35), Some("62ff00001a09"), "AF-A"),
            ("1971.ARW", "ILCE-6000", Some(2687), Some("74ff030d"), "AF-A"),
            ("2647.ARW", "DSC-RX1", None, None, "AF-S"),
            ("8847.ARW", "ILCE-7M5", Some(665), Some("2fff0000e708"), "AF-S"),
        ] {
            let m = super::extract(&std::fs::read(root.join(file)).unwrap());
            assert_eq!(m.model.as_deref(), Some(model));
            let sony = m.sony.unwrap();
            assert_eq!(sony.shutter_count, count, "{file}");
            assert_eq!(sony.internal_serial_number.as_deref(), serial, "{file}");
            assert_eq!(sony.focus_mode.as_deref(), Some(focus), "{file}");
            assert_eq!(sony.picture_profile.as_deref(), Some("Gamma Still - Standard/Neutral"), "{file}");
            if file == "2414.ARW" {
                let lens = sony.lens_correction.as_ref().unwrap();
                assert_eq!(sony.lens_id, Some(32831));
                assert_eq!(lens.distortion, [5, 2, 2, 5, 11, 20, 32, 49, 68, 95, 128, 167, 216, 273, 340, 417]);
                assert_eq!(lens.vignetting, [0, 32, 144, 320, 560, 864, 1216, 1632, 2096, 2608, 3152, 3728, 4320, 4928, 5536, 6144]);
                assert_eq!(lens.ca_red, [128, 128, 128, 128, 128, 128, 128, 128, 256, 256, 384, 384, 384, 512, 512, 640]);
                assert_eq!(lens.ca_blue, [512, 384, 384, 256, 128, 128, 128, 128, 0, 0, 0, -128, -128, -128, -256, -256]);
            } else if file == "8847.ARW" {
                assert_eq!(sony.lens_id, Some(50545));
            } else if file == "3480.ARW" {
                assert_eq!(
                    sony.lens_correction.as_ref().unwrap().distortion,
                    [7, 0, -20, -47, -86, -133, -192, -255, -327, -399, -474, -544, -611, -665, -708, -728]
                );
            }
        }
    }
}
