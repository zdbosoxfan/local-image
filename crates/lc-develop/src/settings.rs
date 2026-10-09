//! The develop settings schema. Every field has a neutral default; serde uses `#[serde(default)]` so
//! older/newer files load (unknown fields are ignored, missing fields take defaults).

use lightcraft_geom::{CropGeometry, Homography, Orientation, Point};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DevelopSettings {
    pub version: u32,
    pub profile: Profile,
    #[serde(default, skip_serializing_if = "is_legacy_process")]
    pub process: ProcessVersion,
    pub treatment: Treatment,
    pub wb: WhiteBalance,
    pub light: Light,
    pub curve: ToneCurve,
    pub color: ColorAdj,
    pub mixer: Mixer,
    /// Point Color: up to [`MAX_POINT_COLORS`] sampled colours, each with its own adjustment.
    pub point_colors: Vec<PointColor>,
    pub bw_mix: BwMix,
    pub grading: ColorGrading,
    pub effects: Effects,
    pub vignette: Vignette,
    pub grain: Grain,
    pub detail: Detail,
    pub optics: Optics,
    pub geometry: Geometry,
    pub crop: Crop,
    pub orientation: Orientation,
    pub masks: Vec<Mask>,
    pub spots: Vec<Spot>,
    pub red_eye: Vec<RedEye>,
    pub lens_blur: LensBlur,
    pub enhance: Enhance,
    /// Camera calibration: shadows tint and primary hue/saturation (applied before tone mapping).
    pub calibration: Calibration,
    /// Film negative conversion (scanned colour / B&W negatives). Left out of the JSON while it is
    /// at its defaults, so settings (and their hashes, XMP sidecars) written before it existed are
    /// unchanged; [`DevelopSettings::to_json_full`] includes it.
    #[serde(skip_serializing_if = "Negative::is_default")]
    pub negative: Negative,
    /// Raw processing: demosaic, highlight reconstruction, capture sharpening ([`crate::tools`];
    /// like the sections below, left out of the JSON at its defaults).
    #[serde(skip_serializing_if = "crate::tools::RawProcessing::is_default")]
    pub raw: crate::tools::RawProcessing,
    /// Lens profile corrections from the lens database.
    #[serde(skip_serializing_if = "crate::tools::LensDb::is_default")]
    pub lens_db: crate::tools::LensDb,
    /// Tone equalizer.
    #[serde(skip_serializing_if = "crate::tools::ToneEq::is_default")]
    pub tone_eq: crate::tools::ToneEq,
    /// Colour calibration (chromatic adaptation, gamut compression).
    #[serde(skip_serializing_if = "crate::tools::ColorCal::is_default")]
    pub color_cal: crate::tools::ColorCal,
    /// Section on/off toggles (the "eye" buttons on panel headers): section id → enabled.
    pub disabled_sections: Vec<String>,
}

impl Default for DevelopSettings {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            profile: Profile::default(),
            process: ProcessVersion::default(),
            treatment: Treatment::Color,
            wb: WhiteBalance::default(),
            light: Light::default(),
            curve: ToneCurve::default(),
            color: ColorAdj::default(),
            mixer: Mixer::default(),
            point_colors: Vec::new(),
            bw_mix: BwMix::default(),
            grading: ColorGrading::default(),
            effects: Effects::default(),
            vignette: Vignette::default(),
            grain: Grain::default(),
            detail: Detail::default(),
            optics: Optics::default(),
            geometry: Geometry::default(),
            crop: Crop::default(),
            orientation: Orientation::Normal,
            masks: Vec::new(),
            spots: Vec::new(),
            red_eye: Vec::new(),
            lens_blur: LensBlur::default(),
            enhance: Enhance::default(),
            calibration: Calibration::default(),
            negative: Negative::default(),
            raw: Default::default(),
            lens_db: Default::default(),
            tone_eq: Default::default(),
            color_cal: Default::default(),
            disabled_sections: Vec::new(),
        }
    }
}

/// The processing engine version: which algorithms render the primary Develop sliders.
/// [`ProcessVersion::Legacy`] is what settings written before 2026 render with (bit-identical);
/// [`ProcessVersion::V2026`] uses the darktable-grade tools (see `docs/DEVELOP-DESIGN.md` §4.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessVersion {
    #[default]
    Legacy,
    V2026,
}

pub fn is_legacy_process(p: &ProcessVersion) -> bool {
    *p == ProcessVersion::Legacy
}

impl DevelopSettings {
    pub fn v2026(&self) -> bool {
        self.process == ProcessVersion::V2026
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Treatment {
    #[default]
    Color,
    Bw,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    /// Profile id from the profile registry (e.g. `lc.color`, `lc.vivid`, `lc.mono`).
    pub id: String,
    /// Creative profile amount 0..200 (%).
    pub amount: f64,
}

impl Default for Profile {
    fn default() -> Self {
        Self { id: "lc.color".into(), amount: 100.0 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WbMode {
    #[default]
    AsShot,
    Auto,
    Daylight,
    Cloudy,
    Shade,
    Tungsten,
    Fluorescent,
    Flash,
    Custom,
}

impl WbMode {
    pub const ALL: [WbMode; 9] = [
        WbMode::AsShot,
        WbMode::Auto,
        WbMode::Daylight,
        WbMode::Cloudy,
        WbMode::Shade,
        WbMode::Tungsten,
        WbMode::Fluorescent,
        WbMode::Flash,
        WbMode::Custom,
    ];
    pub fn label(self) -> &'static str {
        match self {
            WbMode::AsShot => "As Shot",
            WbMode::Auto => "Auto",
            WbMode::Daylight => "Daylight",
            WbMode::Cloudy => "Cloudy",
            WbMode::Shade => "Shade",
            WbMode::Tungsten => "Tungsten",
            WbMode::Fluorescent => "Fluorescent",
            WbMode::Flash => "Flash",
            WbMode::Custom => "Custom",
        }
    }
    /// Preset temperature/tint for raw files (K, tint).
    pub fn preset(self) -> Option<(f64, f64)> {
        match self {
            WbMode::Daylight => Some((5500.0, 10.0)),
            WbMode::Cloudy => Some((6500.0, 10.0)),
            WbMode::Shade => Some((7500.0, 10.0)),
            WbMode::Tungsten => Some((2850.0, 0.0)),
            WbMode::Fluorescent => Some((3800.0, 21.0)),
            WbMode::Flash => Some((5500.0, 0.0)),
            _ => None,
        }
    }
}

/// White balance. For raw files `temp` is the scene illuminant in Kelvin; for rendered files
/// (JPEG etc.) the image is assumed D65-balanced and `temp`/`tint` are the same model with an
/// "as shot" of 6500 K / 0 (the UI shows a relative −100..100 scale for those, like Lightroom).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WhiteBalance {
    pub mode: WbMode,
    pub temp: f64,
    pub tint: f64,
}

impl Default for WhiteBalance {
    fn default() -> Self {
        Self { mode: WbMode::AsShot, temp: 6500.0, tint: 0.0 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Light {
    pub exposure: f64,
    pub contrast: f64,
    pub highlights: f64,
    pub shadows: f64,
    pub whites: f64,
    pub blacks: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToneCurve {
    pub highlights: f64,
    pub lights: f64,
    pub darks: f64,
    pub shadows: f64,
    /// Region split points (percent), Lightroom defaults 25/50/75.
    pub split_shadows: f64,
    pub split_mid: f64,
    pub split_highlights: f64,
    /// Point curves in 0..1 (identity = empty or [(0,0),(1,1)]).
    pub master: Vec<Point>,
    pub red: Vec<Point>,
    pub green: Vec<Point>,
    pub blue: Vec<Point>,
    /// Refine Saturation 0..100: 100 keeps the saturation a curve produces, lower values pull it
    /// back towards the saturation before the curve (strong contrast curves oversaturate).
    pub refine_saturation: f64,
}

impl Default for ToneCurve {
    fn default() -> Self {
        Self {
            highlights: 0.0,
            lights: 0.0,
            darks: 0.0,
            shadows: 0.0,
            split_shadows: 25.0,
            split_mid: 50.0,
            split_highlights: 75.0,
            master: Vec::new(),
            red: Vec::new(),
            green: Vec::new(),
            blue: Vec::new(),
            refine_saturation: 100.0,
        }
    }
}

impl ToneCurve {
    pub fn point_curve_is_identity(pts: &[Point]) -> bool {
        pts.is_empty() || pts.iter().all(|p| (p.x - p.y).abs() < 1e-9)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorAdj {
    pub vibrance: f64,
    pub saturation: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hsl {
    pub hue: f64,
    pub sat: f64,
    pub lum: f64,
}

/// The 8 colour-mixer bands in Lightroom order.
pub const MIXER_BANDS: [&str; 8] = ["red", "orange", "yellow", "green", "aqua", "blue", "purple", "magenta"];
/// Centre hue of each band in degrees (sRGB-ish HSV hue).
pub const MIXER_HUES: [f64; 8] = [0.0, 30.0, 60.0, 120.0, 180.0, 225.0, 270.0, 315.0];

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mixer {
    pub red: Hsl,
    pub orange: Hsl,
    pub yellow: Hsl,
    pub green: Hsl,
    pub aqua: Hsl,
    pub blue: Hsl,
    pub purple: Hsl,
    pub magenta: Hsl,
}

impl Mixer {
    pub fn bands(&self) -> [Hsl; 8] {
        [self.red, self.orange, self.yellow, self.green, self.aqua, self.blue, self.purple, self.magenta]
    }
    pub fn is_neutral(&self) -> bool {
        self.bands().iter().all(|b| *b == Hsl::default())
    }
}

/// Most Point Color samples per photo (Lightroom's limit).
pub const MAX_POINT_COLORS: usize = 8;

/// One Point Color sample: a colour picked on the image (OkLCh, at the stage where Point Color
/// applies: after the colour mixer) and the adjustment of the colours around it. The range is a
/// box in OkLCh around the sample, widened by `range` (overall) and the per-axis widths, with a
/// soft edge.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PointColor {
    /// Sampled OkLab lightness (0..1).
    pub lum: f64,
    /// Sampled OkLCh chroma (0..~0.37).
    pub chroma: f64,
    /// Sampled OkLCh hue, degrees.
    pub hue: f64,
    /// Hue shift −100..100 (±~29° at the sample).
    pub hue_shift: f64,
    /// Saturation shift −100..100 (chroma scale).
    pub sat_shift: f64,
    /// Luminance shift −100..100.
    pub lum_shift: f64,
    /// −100 compresses the colour spread within the range towards the sample (evens out e.g.
    /// skin), +100 expands it.
    pub variance: f64,
    /// Overall range 0..100 (scales all three widths).
    pub range: f64,
    pub hue_range: f64,
    pub sat_range: f64,
    pub lum_range: f64,
}

impl Default for PointColor {
    fn default() -> Self {
        Self {
            lum: 0.5,
            chroma: 0.1,
            hue: 0.0,
            hue_shift: 0.0,
            sat_shift: 0.0,
            lum_shift: 0.0,
            variance: 0.0,
            range: 50.0,
            hue_range: 50.0,
            sat_range: 50.0,
            lum_range: 50.0,
        }
    }
}

impl PointColor {
    /// True when the sample changes nothing.
    pub fn is_neutral(&self) -> bool {
        self.hue_shift == 0.0 && self.sat_shift == 0.0 && self.lum_shift == 0.0 && self.variance == 0.0
    }
}

/// B&W mix (used when treatment = Bw): per-band luminance contribution −100..100.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BwMix {
    pub red: f64,
    pub orange: f64,
    pub yellow: f64,
    pub green: f64,
    pub aqua: f64,
    pub blue: f64,
    pub purple: f64,
    pub magenta: f64,
}

impl BwMix {
    pub fn bands(&self) -> [f64; 8] {
        [self.red, self.orange, self.yellow, self.green, self.aqua, self.blue, self.purple, self.magenta]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Wheel {
    /// Hue in degrees 0..360.
    pub hue: f64,
    /// Saturation 0..100.
    pub sat: f64,
    /// Luminance −100..100.
    pub lum: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorGrading {
    pub shadows: Wheel,
    pub midtones: Wheel,
    pub highlights: Wheel,
    pub global: Wheel,
    /// 0..100, default 50.
    pub blending: f64,
    /// −100..100.
    pub balance: f64,
}

impl Default for ColorGrading {
    fn default() -> Self {
        Self {
            shadows: Wheel::default(),
            midtones: Wheel::default(),
            highlights: Wheel::default(),
            global: Wheel::default(),
            blending: 50.0,
            balance: 0.0,
        }
    }
}

impl ColorGrading {
    pub fn is_neutral(&self) -> bool {
        [self.shadows, self.midtones, self.highlights, self.global].iter().all(|w| w.sat == 0.0 && w.lum == 0.0)
    }
}

/// Camera calibration (Lightroom Classic's Calibration panel): a green/magenta tint of the shadows,
/// and a hue rotation / saturation scale of each working-space primary (−100..100), i.e. a
/// white-preserving 3×3 matrix applied in scene-linear light before tone mapping.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Calibration {
    pub shadows_tint: f64,
    pub red_hue: f64,
    pub red_sat: f64,
    pub green_hue: f64,
    pub green_sat: f64,
    pub blue_hue: f64,
    pub blue_sat: f64,
}

impl Calibration {
    /// (hue, saturation) of the red, green and blue primaries.
    pub fn primaries(&self) -> [(f64, f64); 3] {
        [(self.red_hue, self.red_sat), (self.green_hue, self.green_sat), (self.blue_hue, self.blue_sat)]
    }
    pub fn is_neutral(&self) -> bool {
        *self == Calibration::default()
    }
}

/// What kind of film a scan shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilmStock {
    /// Colour negative (orange mask): per-channel film base.
    #[default]
    Color,
    /// Black & white negative: one film base density for all channels.
    Bw,
    /// Slide (positive transparency): nothing to invert, the scan passes through unchanged.
    Slide,
}

impl FilmStock {
    pub const ALL: [FilmStock; 3] = [FilmStock::Color, FilmStock::Bw, FilmStock::Slide];
    pub fn label(self) -> &'static str {
        match self {
            FilmStock::Color => "Color",
            FilmStock::Bw => "B&W",
            FilmStock::Slide => "Slide",
        }
    }
}

/// An RGB triple of film parameters (linear Rec.2020 working space).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilmRgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl FilmRgb {
    pub const ONE: FilmRgb = FilmRgb { r: 1.0, g: 1.0, b: 1.0 };
    pub const fn new(r: f64, g: f64, b: f64) -> FilmRgb {
        FilmRgb { r, g, b }
    }
    pub fn to_array(self) -> [f64; 3] {
        [self.r, self.g, self.b]
    }
    pub fn from_array(a: [f64; 3]) -> FilmRgb {
        FilmRgb { r: a[0], g: a[1], b: a[2] }
    }
}

impl Default for FilmRgb {
    fn default() -> Self {
        FilmRgb::ONE
    }
}

/// Film negative conversion: inverts a scanned negative and simulates printing it on paper (a port
/// of darktable's *negadoctor*; units, ranges and defaults are negadoctor's). Applied to the
/// white-balanced scene-linear scan, before every tone and colour operation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Negative {
    /// Off by default: photos are untouched until a negative is switched on.
    pub enabled: bool,
    pub film: FilmStock,
    /// D-min: the colour of the unexposed film base (the scan's transmittance there), 0.00001..1.5
    /// per channel. B&W film uses the red component for all channels.
    pub dmin: FilmRgb,
    /// D-max: the film's dynamic range (maximum density above the base), 0.1..6.
    pub d_max: f64,
    /// Scan exposure bias: a density offset correcting the scanner exposure, −1..1.
    pub offset: f64,
    /// Paper black: the print's black density correction, −0.5..0.5.
    pub black: f64,
    /// Paper grade (gamma), 1..8.
    pub gamma: f64,
    /// Paper gloss: highlights above this print value roll off softly, 0.0001..1.
    pub soft_clip: f64,
    /// Print exposure: a gain on the print, 0.5..2.
    pub exposure: f64,
    /// Highlights white balance (illuminant gain per channel), 0.25..2.
    pub wb_high: FilmRgb,
    /// Shadows colour cast (offset per channel), 0.25..2.
    pub wb_low: FilmRgb,
}

impl Default for Negative {
    fn default() -> Self {
        Self {
            enabled: false,
            film: FilmStock::Color,
            dmin: FilmRgb::new(1.0, 0.45, 0.25),
            d_max: 2.046,
            offset: -0.05,
            black: 0.0755,
            gamma: 4.0,
            soft_clip: 0.75,
            exposure: 0.9245,
            wb_high: FilmRgb::ONE,
            wb_low: FilmRgb::ONE,
        }
    }
}

impl Negative {
    pub fn is_default(&self) -> bool {
        *self == Negative::default()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Effects {
    pub texture: f64,
    pub clarity: f64,
    pub dehaze: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VignetteStyle {
    #[default]
    HighlightPriority,
    ColorPriority,
    PaintOverlay,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Vignette {
    pub amount: f64,
    pub midpoint: f64,
    pub roundness: f64,
    pub feather: f64,
    pub highlights: f64,
    pub style: VignetteStyle,
}

impl Default for Vignette {
    fn default() -> Self {
        Self { amount: 0.0, midpoint: 50.0, roundness: 0.0, feather: 50.0, highlights: 0.0, style: VignetteStyle::HighlightPriority }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grain {
    pub amount: f64,
    pub size: f64,
    pub roughness: f64,
    /// Seed for the procedural grain (stable per photo).
    pub seed: u32,
}

impl Default for Grain {
    fn default() -> Self {
        Self { amount: 0.0, size: 25.0, roughness: 50.0, seed: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Detail {
    pub sharpen_amount: f64,
    pub sharpen_radius: f64,
    pub sharpen_detail: f64,
    pub sharpen_masking: f64,
    pub nr_luminance: f64,
    pub nr_detail: f64,
    pub nr_contrast: f64,
    pub nr_color: f64,
    pub nr_color_detail: f64,
    pub nr_color_smoothness: f64,
}

impl Default for Detail {
    /// Lightroom's raw defaults are sharpening 40/1.0/25/0 and colour NR 25/50/50; we use neutral
    /// values for rendered files and apply raw defaults at import time (`DevelopSettings::for_raw`).
    fn default() -> Self {
        Self {
            sharpen_amount: 0.0,
            sharpen_radius: 1.0,
            sharpen_detail: 25.0,
            sharpen_masking: 0.0,
            nr_luminance: 0.0,
            nr_detail: 50.0,
            nr_contrast: 0.0,
            nr_color: 0.0,
            nr_color_detail: 50.0,
            nr_color_smoothness: 50.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Optics {
    pub remove_ca: bool,
    pub lens_profile: bool,
    pub profile_distortion: f64,
    pub profile_vignetting: f64,
    pub defringe_purple_amount: f64,
    pub defringe_purple_hue_lo: f64,
    pub defringe_purple_hue_hi: f64,
    pub defringe_green_amount: f64,
    pub defringe_green_hue_lo: f64,
    pub defringe_green_hue_hi: f64,
    pub distortion: f64,
    pub vignetting: f64,
    pub vignetting_midpoint: f64,
    /// Manual lateral chromatic aberration (red/cyan, blue/yellow fringes), −100..100, added to the
    /// automatic estimate when `remove_ca` is on.
    pub ca_red: f64,
    pub ca_blue: f64,
}

impl Default for Optics {
    fn default() -> Self {
        Self {
            remove_ca: false,
            lens_profile: false,
            profile_distortion: 100.0,
            profile_vignetting: 100.0,
            defringe_purple_amount: 0.0,
            defringe_purple_hue_lo: 30.0,
            defringe_purple_hue_hi: 70.0,
            defringe_green_amount: 0.0,
            defringe_green_hue_lo: 40.0,
            defringe_green_hue_hi: 60.0,
            distortion: 0.0,
            vignetting: 0.0,
            vignetting_midpoint: 50.0,
            ca_red: 0.0,
            ca_blue: 0.0,
        }
    }
}

/// Lens corrections embedded in a DNG file (`OpcodeList3`: `WarpRectilinear`, `FixVignetteRadial`), converted
/// to the oriented, default-cropped image. This is camera/file data (stored on the photo record, not in the develop
/// settings); "Enable Profile Corrections" applies it, scaled by the profile distortion/vignetting amounts.
/// LightCraft never uses Adobe LCP lens profiles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbeddedLens {
    pub warp: Option<EmbeddedWarp>,
    pub vignette: Option<EmbeddedVignette>,
}

/// DNG `WarpRectilinear`: per plane (R, G, B) `[kr0, kr1, kr2, kr3, kt0, kt1]`. For an output (corrected) point at
/// offset `d = (p − center) / radius`, the source point is `center + radius · (d·f(r²) + tangential(d))`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EmbeddedWarp {
    pub planes: [[f64; 6]; 3],
    /// Optical centre, normalized to the oriented image (0..1).
    pub center: Point,
    /// Normalisation radius as a fraction of the oriented image's long edge.
    pub radius: f64,
}

/// DNG `FixVignetteRadial`: gain `1 + k0 r² + k1 r⁴ + … + k4 r¹⁰` with `r = |p − center| / radius`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EmbeddedVignette {
    pub k: [f64; 5],
    pub center: Point,
    pub radius: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Upright {
    #[default]
    Off,
    Auto,
    Guided,
    Level,
    Vertical,
    Full,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Geometry {
    pub upright: Upright,
    /// Guided Upright guides, up to 4 line segments, in normalized coordinates of the lens-corrected
    /// (pre-perspective) oriented image.
    pub guides: Vec<(Point, Point)>,
    /// The automatic Upright correction found by analysis for `upright` (Auto/Level/Vertical/Full), as a
    /// homography lens-corrected → transformed in centred coordinates (`(p − centre) / (long edge / 2)`).
    /// Stored so previews and exports use the identical transform; recomputed by `geometry.upright`.
    pub upright_transform: Option<Homography>,
    pub vertical: f64,
    pub horizontal: f64,
    pub rotate: f64,
    pub aspect: f64,
    pub scale: f64,
    pub offset_x: f64,
    pub offset_y: f64,
    pub constrain_crop: bool,
}

impl Default for Geometry {
    fn default() -> Self {
        Self {
            upright: Upright::Off,
            guides: Vec::new(),
            upright_transform: None,
            vertical: 0.0,
            horizontal: 0.0,
            rotate: 0.0,
            aspect: 0.0,
            scale: 100.0,
            offset_x: 0.0,
            offset_y: 0.0,
            constrain_crop: false,
        }
    }
}

impl Geometry {
    pub fn is_identity(&self) -> bool {
        self.upright == Upright::Off
            && self.vertical == 0.0
            && self.horizontal == 0.0
            && self.rotate == 0.0
            && self.aspect == 0.0
            && self.scale == 100.0
            && self.offset_x == 0.0
            && self.offset_y == 0.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Crop {
    pub geometry: CropGeometry,
    /// Locked aspect (w, h); None = free / original.
    pub aspect: Option<(u32, u32)>,
    pub flip_h: bool,
    pub flip_v: bool,
}

// ---------------------------------------------------------------------------------------------
// Masks and local adjustments

/// The sliders available inside a mask (Lightroom's local adjustments).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalAdjustments {
    pub temp: f64,
    pub tint: f64,
    pub exposure: f64,
    pub contrast: f64,
    pub highlights: f64,
    pub shadows: f64,
    pub whites: f64,
    pub blacks: f64,
    pub texture: f64,
    pub clarity: f64,
    pub dehaze: f64,
    pub hue: f64,
    pub saturation: f64,
    pub sharpness: f64,
    pub noise: f64,
    pub moire: f64,
    pub defringe: f64,
    /// Colour overlay (hue degrees, saturation 0..100); sat 0 = none.
    pub color_hue: f64,
    pub color_sat: f64,
    /// Local vignette/grain are not in Lightroom; we keep the struct extensible.
    pub amount: f64,
}

impl Default for LocalAdjustments {
    fn default() -> Self {
        Self {
            temp: 0.0,
            tint: 0.0,
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            texture: 0.0,
            clarity: 0.0,
            dehaze: 0.0,
            hue: 0.0,
            saturation: 0.0,
            sharpness: 0.0,
            noise: 0.0,
            moire: 0.0,
            defringe: 0.0,
            color_hue: 0.0,
            color_sat: 0.0,
            amount: 100.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaskOp {
    #[default]
    Add,
    Subtract,
    Intersect,
}

/// One brush dab sequence. Coordinates normalized to the uncropped oriented image.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrushStroke {
    pub points: Vec<Point>,
    /// Radius as a fraction of the image's long edge.
    pub size: f64,
    /// 0..100.
    pub feather: f64,
    pub flow: f64,
    pub density: f64,
    pub erase: bool,
    pub auto_mask: bool,
}

impl Default for BrushStroke {
    fn default() -> Self {
        Self { points: Vec::new(), size: 0.03, feather: 50.0, flow: 100.0, density: 100.0, erase: false, auto_mask: false }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MaskShape {
    Brush {
        strokes: Vec<BrushStroke>,
    },
    /// Linear gradient: full effect at `start`, fading to none at `end` (normalized coords).
    Linear {
        start: Point,
        end: Point,
    },
    /// Radial gradient: ellipse centre, radii (normalized to the long edge), rotation (deg), feather 0..100.
    Radial {
        center: Point,
        rx: f64,
        ry: f64,
        angle: f64,
        feather: f64,
        invert: bool,
    },
    /// Colour range: sampled OkLab colours with a refine amount 0..100.
    ColorRange {
        samples: Vec<[f64; 3]>,
        refine: f64,
    },
    /// Luminance range (0..1 bounds with feathered falloff 0..1).
    LuminanceRange {
        lo: f64,
        hi: f64,
        lo_feather: f64,
        hi_feather: f64,
    },
    /// Distance range (0 = nearest … 1 = farthest in the photo), with a feathered edge. `seg`
    /// holds the depth model's distance map (local-image `li-seg` depth; the logit of the
    /// distance); without it the range selects nothing.
    DepthRange {
        lo: f64,
        hi: f64,
        feather: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seg: Option<crate::SegMask>,
    },
    /// AI / automatic selections. `seg` holds the quick segmentation model's result (local-image:
    /// `li-seg`, computed when the mask is added); without it a classical estimate is used.
    Subject {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seg: Option<crate::SegMask>,
    },
    Sky {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seg: Option<crate::SegMask>,
    },
    Background {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seg: Option<crate::SegMask>,
    },
    /// One object picked by clicks (SAM 3 point prompts): `hint` holds the clicks that include,
    /// `exclude` the ones that exclude; `seg` the segmentation computed from them.
    Object {
        hint: Vec<Point>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        exclude: Vec<Point>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seg: Option<crate::SegMask>,
        /// Zoomed-in passes over parts of the image (higher resolution than `seg`), used
        /// inside their rectangles.
        #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::segmask::de_detail")]
        detail: Vec<crate::SegMask>,
        /// Edge −100..100: below 0 harder (a steeper transition), above 0 softer (feathered).
        #[serde(default, skip_serializing_if = "is_zero")]
        edge: f64,
    },
    /// Everything a description names ("sky", "the red car": SAM 3 concept prompts); `seg` is
    /// the segmentation computed for it.
    Prompt {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seg: Option<crate::SegMask>,
        #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::segmask::de_detail")]
        detail: Vec<crate::SegMask>,
        /// Edge −100..100, as for `Object`.
        #[serde(default, skip_serializing_if = "is_zero")]
        edge: f64,
    },
    People {
        person: u32,
        parts: Vec<String>,
    },
    Landscape {
        class: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaskComponent {
    /// A name given in the Masking panel (Rename); `None` shows the shape's kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub op: MaskOp,
    #[serde(default)]
    pub invert: bool,
    pub shape: MaskShape,
}

/// A mask is a develop *layer* (Capture One-style): its components select where, and it holds
/// the quick local sliders ([`LocalAdjustments`], Lightroom's mask sliders) and any develop tool
/// that isn't image-level ([`LayerTools`]), faded by `opacity`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mask {
    pub id: u32,
    pub name: String,
    pub visible: bool,
    pub invert: bool,
    pub components: Vec<MaskComponent>,
    pub adjust: LocalAdjustments,
    /// Refine Edges 0..100: the mask's edges snap to the photo's (guided filter).
    #[serde(skip_serializing_if = "is_zero")]
    pub refine: f64,
    /// Layer opacity 0..100: scales everything the layer does (with `adjust.amount`). Left out of
    /// the JSON at 100, so settings written before layers existed serialize (and hash) as before.
    #[serde(skip_serializing_if = "is_hundred")]
    pub opacity: f64,
    /// The develop tools this layer sets (sparse; empty = none, left out of the JSON).
    #[serde(skip_serializing_if = "LayerTools::is_empty")]
    pub tools: LayerTools,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

fn is_hundred(v: &f64) -> bool {
    *v == 100.0
}

impl Default for Mask {
    fn default() -> Self {
        Self {
            id: 0,
            name: "Mask 1".into(),
            visible: true,
            invert: false,
            components: Vec::new(),
            adjust: LocalAdjustments::default(),
            refine: 0.0,
            opacity: 100.0,
            tools: LayerTools::default(),
        }
    }
}

/// The develop tools a layer ([`Mask`]) applies where it is selected: any section of
/// [`DevelopSettings`] except the image-level ones (profile, calibration, optics, geometry, crop,
/// orientation, negative, enhance). Each section is `None` until the layer changes it; the JSON
/// keys are [`DevelopSettings`]'s, so `tools` is a partial settings object.
///
/// Values are relative to the photo's own settings, every section neutral at its defaults (a
/// layer's exposure +1 is one stop more than the photo's; its curve is applied after the photo's),
/// except white balance: a layer's temperature/tint is the white it renders with (as in Capture
/// One), changed from the photo's white balance.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerTools {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wb: Option<WhiteBalance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light: Option<Light>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curve: Option<ToneCurve>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorAdj>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mixer: Option<Mixer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub point_colors: Option<Vec<PointColor>>,
    /// `Bw`: the layer turns its area black & white (with `bw_mix`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub treatment: Option<Treatment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bw_mix: Option<BwMix>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grading: Option<ColorGrading>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects: Option<Effects>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vignette: Option<Vignette>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grain: Option<Grain>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<Detail>,
}

/// The [`DevelopSettings`] keys a layer can hold (see [`LayerTools`]).
pub const LAYER_KEYS: [&str; 13] =
    ["wb", "light", "curve", "color", "mixer", "point_colors", "treatment", "bw_mix", "grading", "effects", "vignette", "grain", "detail"];

impl LayerTools {
    pub fn is_empty(&self) -> bool {
        *self == LayerTools::default()
    }

    /// Whether the layer holds section `key` (one of [`LAYER_KEYS`]).
    pub fn is_set(&self, key: &str) -> bool {
        self.used_keys().contains(&key)
    }

    /// The sections the layer holds, in [`LAYER_KEYS`] order.
    pub fn used_keys(&self) -> Vec<&'static str> {
        let set = [
            self.wb.is_some(),
            self.light.is_some(),
            self.curve.is_some(),
            self.color.is_some(),
            self.mixer.is_some(),
            self.point_colors.is_some(),
            self.treatment.is_some(),
            self.bw_mix.is_some(),
            self.grading.is_some(),
            self.effects.is_some(),
            self.vignette.is_some(),
            self.grain.is_some(),
            self.detail.is_some(),
        ];
        LAYER_KEYS.iter().zip(set).filter(|(_, s)| *s).map(|(k, _)| *k).collect()
    }

    /// Settings showing this layer's tools: the defaults (every tool neutral) with the layer's
    /// sections laid over them; white balance, until the layer sets it, is `global_wb` (the
    /// photo's white balance in effect: temperature, tint). Panels read and write layer controls
    /// through this view.
    pub fn view(&self, global_wb: (f64, f64)) -> DevelopSettings {
        let d = DevelopSettings::default();
        DevelopSettings {
            wb: self.wb.unwrap_or(WhiteBalance { mode: WbMode::Custom, temp: global_wb.0, tint: global_wb.1 }),
            light: self.light.unwrap_or(d.light),
            curve: self.curve.clone().unwrap_or(d.curve),
            color: self.color.unwrap_or(d.color),
            mixer: self.mixer.unwrap_or(d.mixer),
            point_colors: self.point_colors.clone().unwrap_or_default(),
            treatment: self.treatment.unwrap_or(d.treatment),
            bw_mix: self.bw_mix.unwrap_or(d.bw_mix),
            grading: self.grading.unwrap_or(d.grading),
            effects: self.effects.unwrap_or(d.effects),
            vignette: self.vignette.unwrap_or(d.vignette),
            grain: self.grain.unwrap_or(d.grain),
            detail: self.detail.unwrap_or(d.detail),
            ..d
        }
    }

    /// Section `key` of `view` taken into the layer (`None` for an unknown key).
    fn take(&mut self, key: &str, view: &DevelopSettings) -> Option<()> {
        match key {
            "wb" => self.wb = Some(WhiteBalance { mode: WbMode::Custom, ..view.wb }),
            "light" => self.light = Some(view.light),
            "curve" => self.curve = Some(view.curve.clone()),
            "color" => self.color = Some(view.color),
            "mixer" => self.mixer = Some(view.mixer),
            "point_colors" => self.point_colors = Some(view.point_colors.iter().take(MAX_POINT_COLORS).copied().collect()),
            "treatment" => self.treatment = Some(view.treatment),
            "bw_mix" => self.bw_mix = Some(view.bw_mix),
            "grading" => self.grading = Some(view.grading),
            "effects" => self.effects = Some(view.effects),
            "vignette" => self.vignette = Some(view.vignette),
            "grain" => self.grain = Some(view.grain),
            "detail" => self.detail = Some(view.detail),
            _ => return None,
        }
        Some(())
    }

    /// Drop section `key` (the layer no longer changes it). False for an unknown key.
    pub fn clear(&mut self, key: &str) -> bool {
        match key {
            "wb" => self.wb = None,
            "light" => self.light = None,
            "curve" => self.curve = None,
            "color" => self.color = None,
            "mixer" => self.mixer = None,
            "point_colors" => self.point_colors = None,
            "treatment" => self.treatment = None,
            "bw_mix" => self.bw_mix = None,
            "grading" => self.grading = None,
            "effects" => self.effects = None,
            "vignette" => self.vignette = None,
            "grain" => self.grain = None,
            "detail" => self.detail = None,
            _ => return false,
        }
        true
    }

    /// Merge a partial settings object into the layer, like a preset (objects merge, everything
    /// else replaces; values are clamped through the control specs). Only [`LAYER_KEYS`] are
    /// allowed; a key set to `null` drops that section. `global_wb` as in [`Self::view`].
    pub fn merged(&self, global_wb: (f64, f64), partial: &serde_json::Value) -> Result<LayerTools, String> {
        let Some(obj) = partial.as_object() else { return Err("layer tools must be an object".into()) };
        if let Some(k) = obj.keys().find(|k| !LAYER_KEYS.contains(&k.as_str())) {
            return Err(format!("`{k}` is not a layer tool (layers take {})", LAYER_KEYS.join(", ")));
        }
        let mut out = self.clone();
        let patch: serde_json::Map<String, serde_json::Value> =
            obj.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), v.clone())).collect();
        let view = crate::presets::apply_partial_strict(&self.view(global_wb), &serde_json::Value::Object(patch.clone()))?;
        for k in patch.keys() {
            out.take(k, &view);
        }
        for (k, _) in obj.iter().filter(|(_, v)| v.is_null()) {
            out.clear(k);
        }
        Ok(out)
    }

    /// Set controls by id (`light.exposure`, `pointColor.0.hueShift`, …; clamped) on the layer.
    /// Errors name a control that is unknown or not a layer tool.
    pub fn set_controls(&self, global_wb: (f64, f64), values: &[(String, f64)]) -> Result<LayerTools, String> {
        let mut view = self.view(global_wb);
        let mut out = self.clone();
        for (id, v) in values {
            let key = layer_key_of_control(id).ok_or_else(|| format!("`{id}` is not a layer control"))?;
            if !crate::controls::set(&mut view, id, *v) {
                return Err(format!("unknown control `{id}`"));
            }
            out.take(key, &view);
        }
        Ok(out)
    }
}

/// The [`LayerTools`] section a control belongs to (`light.exposure` → `light`), or `None` for
/// image-level controls (optics, geometry, calibration, profile, negative, crop).
pub fn layer_key_of_control(id: &str) -> Option<&'static str> {
    let fam = id.split('.').next()?;
    Some(match fam {
        "wb" => "wb",
        "light" => "light",
        "curve" => "curve",
        "color" => "color",
        "mixer" => "mixer",
        "pointColor" => "point_colors",
        "bw" => "bw_mix",
        "grading" => "grading",
        "effects" => "effects",
        "vignette" => "vignette",
        "grain" => "grain",
        "detail" => "detail",
        _ => return None,
    })
}

// ---------------------------------------------------------------------------------------------
// Heal / remove / red eye

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpotMode {
    #[default]
    Remove,
    Heal,
    Clone,
    /// local-image: generative AI removal (the repaired pixels are a stored patch, [`AiPatch`]).
    Ai,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spot {
    pub mode: SpotMode,
    /// Target: a single point (circle spot) or a brushed path. Normalized coords.
    pub points: Vec<Point>,
    /// Radius as a fraction of the long edge.
    pub size: f64,
    pub feather: f64,
    pub opacity: f64,
    /// Source offset (normalized) from the target; None = auto (content-aware remove).
    pub source_offset: Option<Point>,
    /// local-image: a lasso outline (normalized, closed) instead of brushed `points` (AI spots).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub polygon: Vec<Point>,
    /// local-image: the develop layer (mask id) whose area an AI spot removed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mask: Option<u32>,
    /// local-image: an AI spot's generated pixels (`None` on other spots). These three fields are
    /// left out of the JSON when empty, so older settings serialize and hash as before.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<AiPatch>,
}

impl Default for Spot {
    fn default() -> Self {
        Self {
            mode: SpotMode::Remove,
            points: Vec::new(),
            size: 0.02,
            feather: 50.0,
            opacity: 100.0,
            source_offset: None,
            polygon: Vec::new(),
            mask: None,
            patch: None,
        }
    }
}

impl Spot {
    /// An AI removal (generated pixels), as opposed to Remove / Heal / Clone.
    pub fn is_ai(&self) -> bool {
        self.mode == SpotMode::Ai
    }
}

/// The stored pixels of an AI removal: a patch in the library's AI store (local-image
/// `lightcraft_engine::enhance`), composited by the pipeline at the retouching stage.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPatch {
    /// Store key (content address of the inputs).
    pub key: String,
    /// Content hash of the photo it was made from: a patch never applies to another photo.
    pub source: String,
    /// Where the patch goes: `[x0, y0, x1, y1]`, normalized coordinates (as spots).
    pub rect: [f64; 4],
    /// The AI engine (`klein`, `qwen-int8`, …) and seed that made it.
    pub engine: String,
    pub seed: u64,
    /// The photo's geometry (orientation, lens, perspective) when it was made; another one means
    /// the patch may not line up (Develop offers Regenerate).
    pub geometry: String,
}

/// A 128-bit content key, written as 32 hex digits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AiKey(pub u128);

impl std::fmt::Display for AiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

impl AiKey {
    pub fn parse(s: &str) -> Option<AiKey> {
        (s.len() == 32).then(|| u128::from_str_radix(s, 16).ok().map(AiKey)).flatten()
    }
}

impl Serialize for AiKey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for AiKey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        AiKey::parse(&s).ok_or_else(|| serde::de::Error::custom("expected 32 hex digits"))
    }
}

/// A photo's AI Denoise result in the library's AI store (local-image).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DenoiseRef {
    /// Store key of the result.
    pub key: AiKey,
    /// Content hash of the photo it was made from (never applied to another photo).
    pub source: AiKey,
}

/// A red eye / pet eye correction: the user's ellipse (centre normalized, radii as fractions of
/// the long edge); the pupil inside it is found automatically.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RedEye {
    pub center: Point,
    pub rx: f64,
    pub ry: f64,
    /// 0..100 (50 = the detected pupil).
    pub pupil_size: f64,
    /// 0..100.
    pub darken: f64,
    pub pet: bool,
    /// Pet eye catchlight: offset of its centre from the pupil centre, in pupil radii.
    #[serde(default)]
    pub catchlight: Option<Point>,
}

impl Default for RedEye {
    fn default() -> Self {
        Self { center: Point::new(0.5, 0.5), rx: 0.02, ry: 0.015, pupil_size: 50.0, darken: 50.0, pet: false, catchlight: None }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LensBlur {
    pub enabled: bool,
    pub amount: f64,
    pub focal_distance: f64,
    pub focal_range: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Enhance {
    /// AI Denoise amount 0..100 (how much of the denoised result is used).
    pub denoise: f64,
    pub raw_details: bool,
    pub super_resolution: bool,
    /// local-image: the AI Denoise result (`None` until Denoise ran; left out of the JSON then).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai: Option<DenoiseRef>,
}
