//! Settings of the toolset upgrades: raw processing (demosaic, highlight reconstruction, capture
//! sharpening), lens profiles from the lens database, the tone equalizer and colour calibration.
//! Every section is off (or at today's behaviour) by default and left out of the JSON while it is
//! at its defaults, so settings written before they existed serialize, hash and render as before.

use serde::{Deserialize, Serialize};

/// Demosaic method for Bayer raws (X-Trans and other patterns always use their own method).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Demosaic {
    /// AHD (bilinear for small thumbnails): what every photo used before.
    #[default]
    Auto,
    Ahd,
    Rcd,
    /// RCD on detail, bilinear in flat areas.
    DualRcd,
    /// RCD on detail, smoothed four-colour VNG-linear in flat areas.
    DualRcdVng,
    Vng4,
    Amaze,
    DualAmazeVng,
    Ppg,
    Bilinear,
}

impl Demosaic {
    /// Menu methods; the saved bilinear dual is shown only when selected.
    pub const ALL: [Demosaic; 9] = [
        Demosaic::Auto,
        Demosaic::Ahd,
        Demosaic::Rcd,
        Demosaic::DualRcdVng,
        Demosaic::Vng4,
        Demosaic::Amaze,
        Demosaic::DualAmazeVng,
        Demosaic::Ppg,
        Demosaic::Bilinear,
    ];
    pub fn is_dual(self) -> bool {
        matches!(self, Self::DualRcd | Self::DualRcdVng | Self::DualAmazeVng)
    }
    pub fn label(self) -> &'static str {
        match self {
            Demosaic::Auto => "Default (AHD)",
            Demosaic::Ahd => "AHD",
            Demosaic::Rcd => "RCD",
            Demosaic::DualRcd => "Dual (RCD + bilinear)",
            Demosaic::DualRcdVng => "Dual (RCD + VNG)",
            Demosaic::Vng4 => "VNG4",
            Demosaic::Amaze => "AMaZE",
            Demosaic::DualAmazeVng => "Dual (AMaZE + VNG)",
            Demosaic::Ppg => "PPG",
            Demosaic::Bilinear => "Bilinear",
        }
    }
}

/// How clipped highlights of a raw are rebuilt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HighlightMode {
    /// Our chromaticity fill (what every photo used before).
    #[default]
    Reconstruct,
    /// Inpaint opposed (darktable).
    Opposed,
    /// CFA segmentation before demosaic (darktable).
    Segmentation,
    /// Clip to neutral white.
    Clip,
}

impl HighlightMode {
    pub const ALL: [HighlightMode; 4] = [HighlightMode::Reconstruct, HighlightMode::Opposed, HighlightMode::Segmentation, HighlightMode::Clip];
    pub fn label(self) -> &'static str {
        match self {
            HighlightMode::Reconstruct => "Reconstruct",
            HighlightMode::Opposed => "Inpaint Opposed",
            HighlightMode::Segmentation => "Segmentation",
            HighlightMode::Clip => "Clip",
        }
    }
}

/// Capture sharpening (deconvolution of the sensor's blur).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureSharpening {
    pub enabled: bool,
    /// Blur radius (Gaussian σ, sensor pixels) 0..1.5; 0 = measured from the raw data.
    pub radius: f64,
    /// Contrast threshold 0..100; 0 = from the ISO.
    pub threshold: f64,
    /// Extra radius towards the corners 0..150 (%).
    pub corner_boost: f64,
    /// Iterations 1..25.
    pub iterations: f64,
}

impl Default for CaptureSharpening {
    fn default() -> Self {
        CaptureSharpening { enabled: false, radius: 0.0, threshold: 0.0, corner_boost: 0.0, iterations: 8.0 }
    }
}

/// Raw processing: demosaic, highlight reconstruction, capture sharpening (raw files only).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RawProcessing {
    pub demosaic: Demosaic,
    /// Dual demosaic's contrast threshold 0..100.
    pub dual_threshold: f64,
    pub highlights: HighlightMode,
    pub capture: CaptureSharpening,
}

impl Default for RawProcessing {
    fn default() -> Self {
        RawProcessing {
            demosaic: Demosaic::Auto,
            dual_threshold: 20.0,
            highlights: HighlightMode::Reconstruct,
            capture: CaptureSharpening::default(),
        }
    }
}

impl RawProcessing {
    pub fn is_default(&self) -> bool {
        *self == RawProcessing::default()
    }
    /// Whether decoding the raw differs from the default (another demosaic or highlight mode,
    /// or capture sharpening asking for the measured radius).
    pub fn changes_decoding(&self) -> bool {
        self.demosaic != Demosaic::Auto || self.highlights != HighlightMode::Reconstruct
    }
}

/// A camera or lens of the lens database (maker and model as the database names them).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LensName {
    pub maker: String,
    pub model: String,
}

/// Lens profile corrections from the lens database (cameras without embedded lens data).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LensDb {
    pub enabled: bool,
    /// The user's picks; `None` = detected from the photo's EXIF.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera: Option<LensName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lens: Option<LensName>,
    /// Strengths 0..200 (%).
    pub distortion: f64,
    pub tca: f64,
    pub vignetting: f64,
}

impl Default for LensDb {
    fn default() -> Self {
        LensDb { enabled: false, camera: None, lens: None, distortion: 100.0, tca: 100.0, vignetting: 100.0 }
    }
}

impl LensDb {
    pub fn is_default(&self) -> bool {
        *self == LensDb::default()
    }
}

/// Tone equalizer: exposure (±2 EV) by luminance zone, −8 EV (`ev8`) … 0 EV (`ev0`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToneEq {
    pub enabled: bool,
    pub ev8: f64,
    pub ev7: f64,
    pub ev6: f64,
    pub ev5: f64,
    pub ev4: f64,
    pub ev3: f64,
    pub ev2: f64,
    pub ev1: f64,
    pub ev0: f64,
    /// Curve smoothing −2..2 (darktable's slider; 0 = σ √2 EV).
    pub smoothing: f64,
    /// Mask smoothing diameter, % of the long edge (0.1..50).
    pub size: f64,
    /// Mask edge refinement 0..100 (higher follows edges more closely).
    pub refine: f64,
    /// Mask exposure / contrast compensation (EV).
    pub mask_exposure: f64,
    pub mask_contrast: f64,
}

impl Default for ToneEq {
    fn default() -> Self {
        ToneEq {
            enabled: false,
            ev8: 0.0,
            ev7: 0.0,
            ev6: 0.0,
            ev5: 0.0,
            ev4: 0.0,
            ev3: 0.0,
            ev2: 0.0,
            ev1: 0.0,
            ev0: 0.0,
            smoothing: 0.0,
            size: 5.0,
            refine: 50.0,
            mask_exposure: 0.0,
            mask_contrast: 0.0,
        }
    }
}

impl ToneEq {
    pub fn is_default(&self) -> bool {
        *self == ToneEq::default()
    }
    /// The nine zones, −8 EV first.
    pub fn zones(&self) -> [f64; 9] {
        [self.ev8, self.ev7, self.ev6, self.ev5, self.ev4, self.ev3, self.ev2, self.ev1, self.ev0]
    }
}

/// Chromatic adaptation space of [`ColorCal`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Adaptation {
    #[default]
    Cat16,
    Bradford,
    FullBradford,
    Xyz,
}

impl Adaptation {
    pub const ALL: [Adaptation; 4] = [Adaptation::Cat16, Adaptation::Bradford, Adaptation::FullBradford, Adaptation::Xyz];
    pub fn label(self) -> &'static str {
        match self {
            Adaptation::Cat16 => "CAT16",
            Adaptation::Bradford => "Bradford (linear)",
            Adaptation::FullBradford => "Bradford (non-linear)",
            Adaptation::Xyz => "XYZ",
        }
    }
}

/// The scene illuminant [`ColorCal`] adapts from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Illuminant {
    /// The White Balance section's temperature and tint.
    #[default]
    WhiteBalance,
    A,
    D50,
    D55,
    D65,
    D75,
    F2,
    F7,
    F11,
    /// `x`, `y` of [`ColorCal`].
    Custom,
}

impl Illuminant {
    pub const ALL: [Illuminant; 10] = [
        Illuminant::WhiteBalance,
        Illuminant::A,
        Illuminant::D50,
        Illuminant::D55,
        Illuminant::D65,
        Illuminant::D75,
        Illuminant::F2,
        Illuminant::F7,
        Illuminant::F11,
        Illuminant::Custom,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Illuminant::WhiteBalance => "As White Balance",
            Illuminant::A => "A (incandescent)",
            Illuminant::D50 => "D50",
            Illuminant::D55 => "D55",
            Illuminant::D65 => "D65",
            Illuminant::D75 => "D75",
            Illuminant::F2 => "F2 (cool white fluorescent)",
            Illuminant::F7 => "F7 (daylight fluorescent)",
            Illuminant::F11 => "F11 (tri-band fluorescent)",
            Illuminant::Custom => "Custom",
        }
    }
}

/// Colour calibration: chromatic adaptation and gamut compression (darktable's colour
/// calibration).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorCal {
    pub enabled: bool,
    pub adaptation: Adaptation,
    pub illuminant: Illuminant,
    /// Custom illuminant chromaticity (CIE 1931 xy).
    pub x: f64,
    pub y: f64,
    /// Gamut compression 0..5 (0 = off).
    pub gamut: f64,
    /// Clip negative RGB.
    pub clip: bool,
}

impl Default for ColorCal {
    fn default() -> Self {
        ColorCal { enabled: false, adaptation: Adaptation::Cat16, illuminant: Illuminant::WhiteBalance, x: 0.3127, y: 0.329, gamut: 1.0, clip: true }
    }
}

impl ColorCal {
    pub fn is_default(&self) -> bool {
        *self == ColorCal::default()
    }
}

#[cfg(test)]
mod raw_quality_tests {
    use super::*;
    #[test]
    fn option_serialization_and_menu_compatibility() {
        assert!(!Demosaic::ALL.contains(&Demosaic::DualRcd));
        assert_eq!(serde_json::from_str::<Demosaic>("\"dualRcd\"").unwrap(), Demosaic::DualRcd);
        for (value, key) in
            [(Demosaic::DualRcdVng, "dualRcdVng"), (Demosaic::Vng4, "vng4"), (Demosaic::Amaze, "amaze"), (Demosaic::DualAmazeVng, "dualAmazeVng")]
        {
            assert!(Demosaic::ALL.contains(&value));
            assert_eq!(serde_json::to_value(value).unwrap(), serde_json::json!(key));
        }
        assert_eq!(serde_json::to_value(HighlightMode::Segmentation).unwrap(), serde_json::json!("segmentation"));
        assert!(HighlightMode::ALL.contains(&HighlightMode::Segmentation));
    }
}
