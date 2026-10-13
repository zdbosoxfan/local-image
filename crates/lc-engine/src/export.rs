//! Export: encode a rendered image to JPEG / PNG / TIFF / WebP / AVIF (or copy the original / write a
//! DNG, each with the edits in XMP) in sRGB, Display P3, Adobe RGB
//! (1998) compatible, ProPhoto RGB or Rec. 2020 with an embedded ICC profile we generate from the
//! published primaries and curves, optional output sharpening and a JPEG file-size limit. Pure (bytes in, bytes out) so the desktop
//! app, CLI, MCP and the web build share it; writing the file is the caller's job.

pub use lightcraft_codecs::TiffCompression;
use lightcraft_codecs::{ChromaSubsampling, EncodeImage, EncodeMeta, NamedSpace, Samples, encode, icc};
use lightcraft_meta::{DateTime, Gps, Metadata};
pub use lightcraft_pipeline::{DeepImage, DeepSamples, OutputDepth, OutputSpace};
use lightcraft_raster::Rgba8;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    #[default]
    Jpeg,
    Png,
    Tiff,
    Webp,
    Avif,
    /// The original file, unchanged, with an XMP sidecar holding the edits.
    Original,
    /// A DNG of the raw data (raw photos only), the edits embedded as XMP.
    Dng,
}

impl ExportFormat {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "jpeg" | "jpg" => Self::Jpeg,
            "png" => Self::Png,
            "tiff" | "tif" => Self::Tiff,
            "webp" => Self::Webp,
            "avif" => Self::Avif,
            "original" => Self::Original,
            "dng" => Self::Dng,
            _ => return None,
        })
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Tiff => "tif",
            Self::Webp => "webp",
            Self::Avif => "avif",
            // the original keeps its own extension (see `ExportOptions::file_name_for`)
            Self::Original => "",
            Self::Dng => "dng",
        }
    }

    /// Formats that render pixels (everything but `Original` and `Dng`).
    pub fn is_rendered(self) -> bool {
        !matches!(self, Self::Original | Self::Dng)
    }
}

/// How the output size is chosen (see [`Resize`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResizeMode {
    /// `value` = long edge in pixels.
    #[default]
    LongEdge,
    /// `value` = short edge in pixels.
    ShortEdge,
    /// `value` = width in pixels.
    Width,
    /// `value` = height in pixels.
    Height,
    /// Fit inside `value × height` pixels, whichever way round the photo is (long edge ≤ the larger).
    Dimensions,
    /// Fit inside a `value` (width) × `height` box as given (Lightroom's "Width & Height").
    WidthHeight,
    /// `value` = megapixels.
    Megapixels,
    /// `value` = percent of the full size.
    Percent,
}

/// Output size: a mode, its value(s) and whether smaller photos are left at their own size.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Resize {
    pub mode: ResizeMode,
    pub value: f32,
    /// Second dimension for [`ResizeMode::Dimensions`].
    pub height: u32,
    /// Never upscale past the photo's own (cropped) size.
    pub dont_enlarge: bool,
}

impl Default for Resize {
    fn default() -> Self {
        Self { mode: ResizeMode::LongEdge, value: 2048.0, height: 2048, dont_enlarge: true }
    }
}

impl Resize {
    pub fn long_edge(px: u32) -> Self {
        Self { value: px as f32, ..Default::default() }
    }

    /// Scale factor from a `w × h` (cropped, full-resolution) photo to the output.
    pub fn scale(&self, w: f64, h: f64) -> f64 {
        let (w, h) = (w.max(1.0), h.max(1.0));
        let (long, short) = (w.max(h), w.min(h));
        let v = (self.value as f64).max(0.0);
        let k = match self.mode {
            ResizeMode::LongEdge => v / long,
            ResizeMode::ShortEdge => v / short,
            ResizeMode::Width => v / w,
            ResizeMode::Height => v / h,
            ResizeMode::Dimensions => {
                let (a, b) = (v, self.height as f64);
                (a.max(b) / long).min(a.min(b) / short)
            }
            ResizeMode::WidthHeight => (v / w).min(self.height as f64 / h),
            ResizeMode::Megapixels => (v * 1e6 / (w * h)).sqrt(),
            ResizeMode::Percent => v / 100.0,
        };
        let k = if self.dont_enlarge { k.min(1.0) } else { k };
        // at least one pixel, at most the encoders' 65535 limit
        k.clamp(1.0 / short, 65_535.0 / long)
    }

    /// Output size for a `w × h` (cropped, full-resolution) photo.
    pub fn apply(&self, w: f64, h: f64) -> (usize, usize) {
        let k = self.scale(w, h);
        (((w * k).round() as usize).max(1), ((h * k).round() as usize).max(1))
    }
}

/// What happens when an output file already exists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Conflict {
    /// Add `-2`, `-3`, … to the name.
    #[default]
    Unique,
    Overwrite,
    /// Leave the existing file and skip the photo.
    Skip,
}

/// A named set of `app.export` params.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExportPreset {
    pub name: String,
    pub params: serde_json::Value,
}

/// The built-in export presets (File → Export with Preset).
pub fn builtin_presets() -> Vec<ExportPreset> {
    let p = |name: &str, params: serde_json::Value| ExportPreset { name: name.into(), params };
    vec![
        p("JPEG (Small)", serde_json::json!({"format": "jpeg", "quality": 85, "longEdge": 2048, "colorSpace": "srgb"})),
        p("JPEG (Large)", serde_json::json!({"format": "jpeg", "quality": 92, "longEdge": 0, "colorSpace": "srgb"})),
        p("Original + Settings", serde_json::json!({"format": "original"})),
        p("DNG", serde_json::json!({"format": "dng"})),
    ]
}

/// Restoration method for export enlargement. Absent in old settings: ordinary resizing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnhanceMethod {
    Faithful,
    Rescue,
    Generative,
}
impl EnhanceMethod {
    pub fn key(self) -> &'static str {
        match self {
            Self::Faithful => "faithful",
            Self::Rescue => "rescue",
            Self::Generative => "generative",
        }
    }
}

/// Output sharpening target (applied after resizing, in display-encoded values).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SharpenFor {
    #[default]
    None,
    Screen,
    Matte,
    Glossy,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SharpenAmount {
    Low,
    #[default]
    Standard,
    High,
}

/// Where a watermark sits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    #[default]
    BottomRight,
}

/// A text watermark, sized relative to the image so every export size looks the same.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Watermark {
    pub text: String,
    /// Basic Japanese vertical lettering: upright glyphs in columns from right to left.
    pub vertical: bool,
    /// Text height as a fraction of the image's short edge.
    pub size: f32,
    /// 0..1.
    pub opacity: f32,
    pub anchor: Anchor,
    /// Margin as a fraction of the short edge.
    pub inset: f32,
    /// sRGB colour.
    pub color: [u8; 3],
    /// Soft dark drop shadow for legibility on bright areas.
    pub shadow: bool,
    /// A graphic (PNG with transparency, JPEG, …) drawn instead of the text (empty = text).
    pub image: String,
    /// The graphic's width as a fraction of the photo's width.
    pub image_width: f32,
    /// The colour space the photo is in (set by the encoder; the graphic is converted into it).
    #[serde(skip)]
    pub target: Option<OutputSpace>,
}

impl Default for Watermark {
    fn default() -> Self {
        Self {
            text: String::new(),
            vertical: false,
            size: 0.035,
            opacity: 0.7,
            anchor: Anchor::BottomRight,
            inset: 0.025,
            color: [255; 3],
            shadow: true,
            image: String::new(),
            image_width: 0.2,
            target: None,
        }
    }
}

/// Inter SemiBold (OFL, see assets/ATTRIBUTION.md).
static WATERMARK_FONT: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");

/// The watermark faces: Inter first, then the craft-fonts CJK faces (Mincho first, the watermark's
/// serif look; then any other CJK face — a watermark has no UI language, so every CJK script is
/// offered). Without craft-fonts that is Inter alone, and CJK characters draw as Inter's
/// missing-glyph box.
fn watermark_fonts(craft: &'static [crate::fonts::CraftFont]) -> Vec<ab_glyph::FontRef<'static>> {
    let mut cjk: Vec<_> = crate::fonts::cjk(craft).collect();
    cjk.sort_by_key(|f| !f.is_mincho());
    std::iter::once(WATERMARK_FONT)
        .chain(cjk.into_iter().map(|f| f.bytes))
        .filter_map(|bytes| ab_glyph::FontRef::try_from_slice(bytes).ok())
        .collect()
}

/// Draw `wm` onto `img` (straight alpha blending of the encoded values).
pub fn draw_watermark(img: &mut Rgba8, wm: &Watermark) {
    let width = img.width;
    watermark_coverage(img.width, img.height, wm, crate::fonts::CRAFT_FONTS, |x, y, k, col| {
        let p = &mut img.data[y * width + x];
        for c in 0..3 {
            p[c] = (p[c] as f32 + (col[c] as f32 - p[c] as f32) * k).round() as u8;
        }
    });
}

/// Draw `wm` onto a high-bit-depth image (`wm.color` is in the image's space, 8-bit encoded; float
/// images are linear and blend in linear light).
pub fn draw_watermark_deep(img: &mut DeepImage, wm: &Watermark) {
    let (width, trc) = (img.width, img.space.trc());
    match &mut img.samples {
        DeepSamples::U16(v) => watermark_coverage(img.width, img.height, wm, crate::fonts::CRAFT_FONTS, |x, y, k, col| {
            let i = (y * width + x) * 3;
            for c in 0..3 {
                let p = v[i + c] as f32;
                v[i + c] = (p + (col[c] as f32 * 257.0 - p) * k).round().clamp(0.0, 65535.0) as u16;
            }
        }),
        DeepSamples::F32(v) => watermark_coverage(img.width, img.height, wm, crate::fonts::CRAFT_FONTS, |x, y, k, col| {
            let i = (y * width + x) * 3;
            for c in 0..3 {
                let target = trc.decode(col[c] as f32 / 255.0);
                v[i + c] += (target - v[i + c]) * k;
            }
        }),
    }
}

/// Lay out `wm` on a `width × height` image and call `blend(x, y, coverage × opacity, colour)` for
/// every covered pixel (shadow pass first). `craft` is [`crate::fonts::CRAFT_FONTS`] (a parameter
/// so tests can render without it).
fn watermark_coverage(
    width: usize,
    height: usize,
    wm: &Watermark,
    craft: &'static [crate::fonts::CraftFont],
    mut blend_px: impl FnMut(usize, usize, f32, [u8; 3]),
) {
    use ab_glyph::{Font, PxScale, ScaleFont, point};
    if !wm.image.trim().is_empty() {
        logo_coverage(width, height, wm, blend_px);
        return;
    }
    let text = wm.text.trim();
    if text.is_empty() || width == 0 || height == 0 {
        return;
    }
    let fonts = watermark_fonts(craft);
    let Some(latin) = fonts.first() else { return };
    let short = width.min(height) as f32;
    let px = (wm.size.clamp(0.005, 0.5) * short).max(6.0);
    let mut glyphs = Vec::new();
    let mut x = 0.0f32;
    let mut y = 0.0f32;
    let mut tw = 0.0f32;
    let mut prev = None;
    let columns = text.split('\n').count();
    let mut column = 0usize;
    for original in text.chars() {
        if original == '\n' {
            if wm.vertical {
                column = column.saturating_add(1);
                tw = tw.max(y);
                y = 0.0;
            } else {
                tw = tw.max(x);
                x = 0.0;
                y += px;
            }
            prev = None;
            continue;
        }
        let vertical_form = match original {
            '、' => '︑',
            '。' => '︒',
            '（' => '︵',
            '）' => '︶',
            '「' => '﹁',
            '」' => '﹂',
            '『' => '﹃',
            '』' => '﹄',
            '【' => '︻',
            '】' => '︼',
            '…' => '︙',
            other => other,
        };
        let has = |font: &ab_glyph::FontRef<'_>, c: char| font.glyph_id(c).0 != 0;
        let ch = if wm.vertical && fonts.iter().skip(1).any(|f| has(f, vertical_form)) { vertical_form } else { original };
        // The first face with the glyph; Inter (its missing-glyph box) when none has it.
        let face = fonts.iter().position(|f| has(f, ch)).unwrap_or(0);
        let sf = fonts.get(face).unwrap_or(latin).as_scaled(PxScale::from(px));
        let id = sf.glyph_id(ch);
        if wm.vertical {
            let left = columns.saturating_sub(column.saturating_add(1)) as f32 * px;
            let pen = point(left + (px - sf.h_advance(id)) / 2.0, y + sf.ascent());
            let rotation = matches!(original, 'ー' | '—' | '–').then_some((left + px / 2.0, y + px / 2.0));
            glyphs.push((face, id.with_scale_and_position(px, pen), rotation));
            y += px;
        } else {
            if let Some((previous_face, previous)) = prev
                && previous_face == face
            {
                x += sf.kern(previous, id);
            }
            glyphs.push((face, id.with_scale_and_position(px, point(x, y + sf.ascent())), None));
            x += sf.h_advance(id);
            prev = Some((face, id));
        }
    }
    let (tw, th) = if wm.vertical { (columns as f32 * px, tw.max(y)) } else { (tw.max(x), y + px) };
    let inset = wm.inset.clamp(0.0, 0.4) * short;
    let (w, h) = (width as f32, height as f32);
    use Anchor::*;
    let ox = match wm.anchor {
        TopLeft | Left | BottomLeft => inset,
        Top | Center | Bottom => (w - tw) / 2.0,
        TopRight | Right | BottomRight => w - inset - tw,
    };
    let oy = match wm.anchor {
        TopLeft | Top | TopRight => inset,
        Left | Center | Right => (h - th) / 2.0,
        BottomLeft | Bottom | BottomRight => h - inset - th,
    };
    let alpha = wm.opacity.clamp(0.0, 1.0);
    let mut blend = |gx: i32, gy: i32, cov: f32, col: [u8; 3], a: f32| {
        if gx < 0 || gy < 0 || gx >= width as i32 || gy >= height as i32 {
            return;
        }
        blend_px(gx as usize, gy as usize, (cov * a).clamp(0.0, 1.0), col);
    };
    let passes: &[(f32, [u8; 3], f32)] =
        if wm.shadow { &[((px * 0.05).max(1.0), [0, 0, 0], 0.45), (0.0, wm.color, 1.0)] } else { &[(0.0, wm.color, 1.0)] };
    for &(off, col, a) in passes {
        for (face, g, rotation) in &glyphs {
            if let Some(o) = fonts.get(*face).and_then(|font| font.outline_glyph(g.clone())) {
                let b = o.px_bounds();
                o.draw(|gx, gy, cov| {
                    let (sx, sy) = (b.min.x + gx as f32, b.min.y + gy as f32);
                    let (sx, sy) = match rotation {
                        Some((cx, cy)) => (cx - (sy - cy), cy + (sx - cx)),
                        None => (sx, sy),
                    };
                    blend((ox + off + sx) as i32, (oy + off + sy) as i32, cov, col, a * alpha);
                });
            }
        }
    }
}

/// A watermark graphic, decoded once per file (and again when the file changes).
fn watermark_logo(path: &str) -> Option<std::sync::Arc<Rgba8>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    static CACHE: Mutex<Option<HashMap<String, (std::time::SystemTime, Arc<Rgba8>)>>> = Mutex::new(None);
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let map = c.get_or_insert_with(HashMap::new);
    if let Some((m, img)) = map.get(path)
        && *m == modified
    {
        return Some(img.clone());
    }
    let bytes = std::fs::read(path).ok()?;
    let img = Arc::new(lightcraft_codecs::decode(&bytes, Default::default()).ok()?.to_srgb8());
    map.insert(path.to_string(), (modified, img.clone()));
    Some(img)
}

/// The graphic watermark: scaled (premultiplied, so transparent edges stay clean) to
/// `image_width` of the photo, placed like the text, blended with its alpha × opacity.
fn logo_coverage(width: usize, height: usize, wm: &Watermark, mut blend_px: impl FnMut(usize, usize, f32, [u8; 3])) {
    let Some(logo) = watermark_logo(wm.image.trim()) else {
        log::warn!("watermark: can't read {}", wm.image);
        return;
    };
    if width == 0 || height == 0 || logo.width == 0 || logo.height == 0 {
        return;
    }
    let lw = ((wm.image_width.clamp(0.01, 1.0) * width as f32).round() as usize).clamp(1, width);
    let lh = ((lw as f32 * logo.height as f32 / logo.width as f32).round() as usize).clamp(1, height);
    let pre: lightcraft_raster::Image<[f32; 4]> = lightcraft_raster::Image::from_fn(logo.width, logo.height, |x, y| {
        let p = logo.get(x, y);
        let a = p[3] as f32 / 255.0;
        [p[0] as f32 * a, p[1] as f32 * a, p[2] as f32 * a, a]
    });
    let scaled = lightcraft_raster::resample::resize(&pre, lw, lh, lightcraft_raster::resample::Filter::Mitchell);
    let inset = wm.inset.clamp(0.0, 0.4) * width.min(height) as f32;
    let (w, h, lwf, lhf) = (width as f32, height as f32, lw as f32, lh as f32);
    use Anchor::*;
    let ox = match wm.anchor {
        TopLeft | Left | BottomLeft => inset,
        Top | Center | Bottom => (w - lwf) / 2.0,
        TopRight | Right | BottomRight => w - inset - lwf,
    }
    .max(0.0) as usize;
    let oy = match wm.anchor {
        TopLeft | Top | TopRight => inset,
        Left | Center | Right => (h - lhf) / 2.0,
        BottomLeft | Bottom | BottomRight => h - inset - lhf,
    }
    .max(0.0) as usize;
    let space = wm.target.unwrap_or(OutputSpace::Srgb);
    let op = wm.opacity.clamp(0.0, 1.0);
    for y in 0..lh {
        for x in 0..lw {
            let [r, g, b, a] = scaled.get(x, y);
            let a = a.clamp(0.0, 1.0);
            let (px, py) = (ox + x, oy + y);
            if a <= 1e-4 || px >= width || py >= height {
                continue;
            }
            let c = [r, g, b].map(|v| (v / a).round().clamp(0.0, 255.0) as u8);
            blend_px(px, py, a * op, srgb8_in(space, c));
        }
    }
}

/// Which metadata is embedded in exported files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetadataPolicy {
    /// Everything we know: camera, capture settings, lens, location, title, caption, keywords, copyright.
    #[default]
    All,
    /// Everything except camera/lens make, model and capture settings.
    AllExceptCamera,
    /// Copyright and creator only.
    Copyright,
    /// Nothing (the sRGB profile is still embedded).
    None,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExportOptions {
    pub format: ExportFormat,
    /// 1–100 (JPEG, AVIF).
    pub quality: u8,
    /// Output size (`None` = full size).
    pub resize: Option<Resize>,
    /// AI method used only when resizing enlarges the developed image (sRGB model input).
    pub enhance: Option<EnhanceMethod>,
    /// Print resolution written into the file (pixels per inch).
    pub ppi: u16,
    /// JPEG only: largest quality whose file fits in this many KB.
    pub limit_kb: Option<u32>,
    pub sharpen: SharpenFor,
    pub sharpen_amount: SharpenAmount,
    /// File name template with the batch-rename tokens ([`crate::rename::expand_tokens`]): `{name}`
    /// (original stem), `{seq}` (zero-padded to 3, counting from `start_number`; `{seq:N}` for N
    /// digits), `{date}` (capture date, YYYYMMDD; `{date:%Y-%m-%d}`…), `{num}`, `{folder}`,
    /// `{camera}`, `{lens}`, `{iso}`, `{rating}`, `{title}`, `{creator}`, `{ext}`; the output
    /// extension is appended.
    pub naming: String,
    /// First `{seq}` value.
    pub start_number: u32,
    /// Created inside the destination folder (empty = none).
    pub subfolder: String,
    /// Export each photo into the folder of its own original (Lightroom's "Same folder as original
    /// photo") instead of the batch's destination folder; `subfolder` still applies. The usual
    /// guard holds: an original is never written over.
    pub same_folder: bool,
    /// When a file of the same name exists in the destination folder.
    pub conflict: Conflict,
    /// TIFF compression.
    pub tiff_compression: TiffCompression,
    /// DNG export: how the raw data is stored.
    pub dng_compression: DngCompression,
    pub metadata: MetadataPolicy,
    /// Strip GPS / location even when the policy would include it.
    pub remove_location: bool,
    /// Explicit opt-in: project named face regions into People keywords on exported copies.
    #[serde(default)]
    pub include_people_names: bool,
    /// Text watermark (none when absent or the text is empty).
    pub watermark: Option<Watermark>,
    /// Output colour space (AVIF is always sRGB: its muxer cannot embed a profile).
    pub color_space: OutputSpace,
    /// Bits per channel: 8, 16 (PNG, TIFF), 32 (TIFF: float, linear) or 10 (AVIF); `None` = the
    /// format's default (TIFF 16, everything else 8). See [`ExportOptions::effective_depth`].
    pub bit_depth: Option<u8>,
    /// JPEG chroma subsampling (`None` = 4:4:4, or 4:2:0 with a file-size limit). Not a user
    /// setting: Save Over Original keeps the original's.
    #[serde(skip)]
    pub jpeg_chroma: Option<ChromaSubsampling>,
    /// Explicit Save Over Original only: encode into the source's matrix/TRC profile.
    /// Internal render state must never become part of a preset.
    #[serde(skip)]
    #[doc(hidden)]
    pub source_encoding: Option<Box<SourceEncoding>>,
    #[serde(skip)]
    #[doc(hidden)]
    pub source_metadata: Option<Box<Metadata>>,
}

#[derive(Clone, Debug, PartialEq)]
#[doc(hidden)]
pub struct SourceEncoding {
    pub space: lightcraft_codecs::SourceSpace,
    pub profile: Vec<u8>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            format: ExportFormat::Jpeg,
            quality: 90,
            resize: None,
            enhance: None,
            ppi: 240,
            limit_kb: None,
            sharpen: SharpenFor::None,
            sharpen_amount: SharpenAmount::Standard,
            naming: "{name}".into(),
            start_number: 1,
            subfolder: String::new(),
            same_folder: false,
            conflict: Conflict::Unique,
            tiff_compression: TiffCompression::Deflate,
            dng_compression: DngCompression::Lossless,
            metadata: MetadataPolicy::All,
            remove_location: false,
            include_people_names: false,
            watermark: None,
            color_space: OutputSpace::Srgb,
            bit_depth: None,
            jpeg_chroma: None,
            source_encoding: None,
            source_metadata: None,
        }
    }
}

impl ExportOptions {
    /// Read options from command params (`format`, `quality`, `limitKb`, `sharpen`, `sharpenAmount`,
    /// `naming`, `metadata`, `removeLocation`, `watermark`, `colorSpace`, `bitDepth`, `ppi`) and the
    /// size: one of `longEdge`, `shortEdge`, `width`, `height` (both = fit inside W × H),
    /// `megapixels`, `percent` (absent or 0 = full size), or a [`Resize`] object as `resize`; plus
    /// `dontEnlarge` (default true).
    pub fn from_json(p: &serde_json::Value) -> Self {
        use serde_json::Value;
        let d = Self::default();
        let s = |k: &str| p.get(k).and_then(Value::as_str);
        let u = |k: &str| p.get(k).and_then(Value::as_u64);
        fn enm<T: serde::de::DeserializeOwned>(p: &Value, k: &str) -> Option<T> {
            p.get(k).cloned().and_then(|v| serde_json::from_value(v).ok())
        }
        Self {
            format: s("format").and_then(ExportFormat::parse).unwrap_or(d.format),
            quality: u("quality").map_or(d.quality, |q| q.clamp(1, 100) as u8),
            resize: Self::resize_from_json(p),
            enhance: enm(p, "enhance"),
            ppi: u("ppi").filter(|v| *v > 0).map_or(d.ppi, |v| v.min(u16::MAX as u64) as u16),
            limit_kb: u("limitKb").filter(|v| *v > 0).map(|v| v.min(u32::MAX as u64) as u32),
            sharpen: enm(p, "sharpen").unwrap_or(d.sharpen),
            sharpen_amount: enm(p, "sharpenAmount").unwrap_or(d.sharpen_amount),
            naming: s("naming").map_or(d.naming, str::to_string),
            start_number: u("startNumber").map_or(d.start_number, |v| v.min(999_999) as u32),
            subfolder: s("subfolder").map_or(d.subfolder, |v| v.trim().replace(['\\', ':', '\0'], "_")),
            same_folder: p.get("sameFolder").and_then(Value::as_bool).unwrap_or(d.same_folder),
            conflict: enm(p, "conflict").unwrap_or(d.conflict),
            tiff_compression: match s("tiffCompression").map(str::to_ascii_lowercase).as_deref() {
                Some("none") => TiffCompression::None,
                Some("lzw") => TiffCompression::Lzw,
                Some("zip" | "deflate") => TiffCompression::Deflate,
                _ => d.tiff_compression,
            },
            dng_compression: enm(p, "dngCompression").unwrap_or(d.dng_compression),
            metadata: enm(p, "metadata").unwrap_or(d.metadata),
            remove_location: p.get("removeLocation").and_then(Value::as_bool).unwrap_or(d.remove_location),
            include_people_names: p.get("includePeopleNames").and_then(Value::as_bool).unwrap_or(false),
            watermark: match p.get("watermark") {
                Some(Value::String(t)) => Some(Watermark { text: t.clone(), ..Default::default() }),
                Some(v @ Value::Object(_)) => serde_json::from_value(v.clone()).ok(),
                _ => None,
            }
            .filter(|w: &Watermark| !w.text.trim().is_empty() || !w.image.trim().is_empty()),
            color_space: s("colorSpace").and_then(OutputSpace::parse).unwrap_or(d.color_space),
            bit_depth: u("bitDepth").filter(|b| matches!(b, 8 | 10 | 16 | 32)).map(|b| b as u8),
            jpeg_chroma: None,
            source_encoding: None,
            source_metadata: None,
        }
    }

    fn resize_from_json(p: &serde_json::Value) -> Option<Resize> {
        use serde_json::Value;
        let f = |k: &str| p.get(k).and_then(Value::as_f64).filter(|v| *v > 0.0);
        let dont_enlarge = p.get("dontEnlarge").and_then(Value::as_bool).unwrap_or(true);
        if let Some(r) = p.get("resize").filter(|v| v.is_object()) {
            let r: Resize = serde_json::from_value(r.clone()).ok()?;
            return (r.value > 0.0).then_some(Resize { dont_enlarge: p.get("dontEnlarge").and_then(Value::as_bool).unwrap_or(r.dont_enlarge), ..r });
        }
        let r = |mode, value: f64| Resize { mode, value: value.min(65_535.0) as f32, height: 0, dont_enlarge };
        Some(match (f("width"), f("height")) {
            (Some(w), Some(h)) => Resize { height: h.min(65_535.0) as u32, ..r(ResizeMode::Dimensions, w) },
            (Some(w), None) => r(ResizeMode::Width, w),
            (None, Some(h)) => r(ResizeMode::Height, h),
            (None, None) => {
                if let Some(v) = f("longEdge") {
                    r(ResizeMode::LongEdge, v)
                } else if let Some(v) = f("shortEdge") {
                    r(ResizeMode::ShortEdge, v)
                } else if let Some(v) = f("megapixels") {
                    r(ResizeMode::Megapixels, v)
                } else {
                    let v = f("percent")?;
                    r(ResizeMode::Percent, v)
                }
            }
        })
    }

    /// These options as `app.export` params ([`ExportOptions::from_json`] reads them back
    /// unchanged; full size is written as `longEdge: 0`).
    pub fn to_json(&self) -> serde_json::Value {
        let mut v = serde_json::to_value(self).unwrap_or_default();
        if let Some(o) = v.as_object_mut() {
            o.remove("resize");
            if let Some(sz) = Self::resize_json(self.resize.as_ref()).as_object() {
                o.extend(sz.clone());
            }
            if self.resize.is_none() {
                o.insert("longEdge".into(), 0.into());
            }
            o.retain(|_, v| !v.is_null());
        }
        v
    }

    /// Whether `p` names an output size (any of the size params, even 0 = full size).
    pub fn has_size_param(p: &serde_json::Value) -> bool {
        ["resize", "longEdge", "shortEdge", "width", "height", "megapixels", "percent"].iter().any(|k| p.get(k).is_some())
    }

    /// The size params of [`ExportOptions::from_json`] for `resize` (`{}` = full size).
    pub fn resize_json(resize: Option<&Resize>) -> serde_json::Value {
        match resize {
            None => serde_json::json!({}),
            Some(r) => serde_json::json!({"resize": r, "dontEnlarge": r.dont_enlarge}),
        }
    }

    /// The sample format the file is written with (unsupported requests fall back to the closest).
    pub fn effective_depth(&self) -> OutputDepth {
        match (self.format, self.bit_depth) {
            (ExportFormat::Jpeg | ExportFormat::Webp | ExportFormat::Original | ExportFormat::Dng, _) => OutputDepth::U8,
            (ExportFormat::Png, Some(16 | 32)) => OutputDepth::U16,
            (ExportFormat::Png, _) => OutputDepth::U8,
            (ExportFormat::Tiff, Some(8)) => OutputDepth::U8,
            (ExportFormat::Tiff, Some(32)) => OutputDepth::F32Linear,
            (ExportFormat::Tiff, _) => OutputDepth::U16,
            (ExportFormat::Avif, Some(10 | 16 | 32)) => OutputDepth::U16,
            (ExportFormat::Avif, _) => OutputDepth::U8,
        }
    }

    /// The bit depths `format` offers (value, label); the first is its default.
    pub fn bit_depths(format: ExportFormat) -> &'static [(u8, &'static str)] {
        match format {
            ExportFormat::Jpeg | ExportFormat::Webp | ExportFormat::Original | ExportFormat::Dng => &[(8, "8-bit")],
            ExportFormat::Png => &[(8, "8-bit"), (16, "16-bit")],
            ExportFormat::Tiff => &[(16, "16-bit"), (8, "8-bit"), (32, "32-bit float")],
            ExportFormat::Avif => &[(8, "8-bit"), (10, "10-bit")],
        }
    }

    /// The colour space the file is actually written in (AVIF: sRGB).
    pub fn effective_space(&self) -> OutputSpace {
        if self.format == ExportFormat::Avif { OutputSpace::Srgb } else { self.color_space }
    }

    /// Output file name for photo `p` at 1-based position `seq` in a batch (the original's
    /// extension is kept for [`ExportFormat::Original`]).
    pub fn file_name_for(&self, p: &lightcraft_catalog::Photo, seq: usize) -> String {
        let base = if self.naming.trim().is_empty() { "{name}" } else { self.naming.as_str() };
        let n = seq + self.start_number.max(1) as usize - 1;
        let name = crate::rename::expand_tokens(base, p, n, 3);
        let mut name: String = name.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '\0') { '_' } else { c }).collect();
        let (stem, ext) = p.file_name.rsplit_once('.').unwrap_or((&p.file_name, ""));
        if name.trim().is_empty() {
            // every token came out empty (no title, no capture time…): keep the original name
            name = stem.to_string();
        }
        if self.format == ExportFormat::Original {
            let base = name.trim_end_matches('.');
            if ext.is_empty() { base.to_string() } else { format!("{base}.{ext}") }
        } else {
            format!("{name}.{}", self.format.extension())
        }
    }
}

/// Unsharp mask on an 8-bit image (separable box-blur approximation of a small Gaussian).
pub fn output_sharpen(img: &mut Rgba8, target: SharpenFor, amount: SharpenAmount) {
    let src: Vec<[f32; 3]> = img.data.iter().map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
    if let Some(out) = unsharp(img.width, img.height, &src, target, amount) {
        for (p, v) in img.data.iter_mut().zip(out) {
            for c in 0..3 {
                p[c] = v[c].round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// [`output_sharpen`] on a high-bit-depth image (float images are sharpened in linear light).
pub fn output_sharpen_deep(img: &mut DeepImage, target: SharpenFor, amount: SharpenAmount) {
    let (w, h) = (img.width, img.height);
    match &mut img.samples {
        DeepSamples::U16(v) => {
            let src: Vec<[f32; 3]> = v.as_chunks::<3>().0.iter().map(|c| [c[0] as f32, c[1] as f32, c[2] as f32]).collect();
            if let Some(out) = unsharp(w, h, &src, target, amount) {
                for (d, s) in v.iter_mut().zip(out.as_flattened()) {
                    *d = s.round().clamp(0.0, 65535.0) as u16;
                }
            }
        }
        DeepSamples::F32(v) => {
            let src: Vec<[f32; 3]> = v.as_chunks::<3>().0.iter().map(|c| [c[0], c[1], c[2]]).collect();
            if let Some(out) = unsharp(w, h, &src, target, amount) {
                for (d, s) in v.iter_mut().zip(out.as_flattened()) {
                    *d = s.max(0.0);
                }
            }
        }
    }
}

/// The sharpened values of `src` (`w × h`), or `None` when nothing is to be done.
fn unsharp(w: usize, h: usize, src: &[[f32; 3]], target: SharpenFor, amount: SharpenAmount) -> Option<Vec<[f32; 3]>> {
    let (radius, base) = match target {
        SharpenFor::None => return None,
        SharpenFor::Screen => (1usize, 0.35f32),
        SharpenFor::Matte => (2, 0.6),
        SharpenFor::Glossy => (1, 0.5),
    };
    let k = base
        * match amount {
            SharpenAmount::Low => 0.6,
            SharpenAmount::Standard => 1.0,
            SharpenAmount::High => 1.5,
        };
    if w < 3 || h < 3 {
        return None;
    }
    let r = radius as isize;
    let box_pass = |inp: &[[f32; 3]], horizontal: bool| -> Vec<[f32; 3]> {
        let mut out = vec![[0.0; 3]; inp.len()];
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0.0f32; 3];
                for d in -r..=r {
                    let (sx, sy) = if horizontal {
                        ((x as isize + d).clamp(0, w as isize - 1) as usize, y)
                    } else {
                        (x, (y as isize + d).clamp(0, h as isize - 1) as usize)
                    };
                    let p = inp[sy * w + sx];
                    acc = [acc[0] + p[0], acc[1] + p[1], acc[2] + p[2]];
                }
                let n = (2 * r + 1) as f32;
                out[y * w + x] = acc.map(|v| v / n);
            }
        }
        out
    };
    let blur = box_pass(&box_pass(src, true), false);
    Some(src.iter().zip(&blur).map(|(s, b)| std::array::from_fn(|c| s[c] + (s[c] - b[c]) * k)).collect())
}

/// Encode a rendered display-referred image according to `o` (its pixels must already be in
/// `o.effective_space()`). Resizing to `long_edge` is the
/// caller's job (render at that size); sharpening is applied here.
pub fn encode_image(img: &Rgba8, o: &ExportOptions) -> Result<Vec<u8>, String> {
    encode_with_metadata(img, o, None)
}

/// Like [`encode_image`], embedding `meta` (already filtered by the policy) as EXIF + XMP.
pub fn encode_with_metadata(img: &Rgba8, o: &ExportOptions, meta: Option<&Metadata>) -> Result<Vec<u8>, String> {
    let mut img = img.clone();
    output_sharpen(&mut img, o.sharpen, o.sharpen_amount);
    let space = o.effective_space();
    if let Some(wm) = &o.watermark {
        let wm = Watermark { color: srgb8_in(space, wm.color), target: Some(space), ..wm.clone() };
        draw_watermark(&mut img, &wm);
    }
    let profile = icc::write_named(named_space(space));
    let exif = meta.map(lightcraft_meta::write_exif);
    let xmp = meta.map(|m| lightcraft_meta::write_xmp(m, None));
    let meta = EncodeMeta { icc: Some(&profile), exif: exif.as_deref(), xmp: xmp.as_deref(), ppi: Some(o.ppi) };
    let e = EncodeImage::rgba8(&img);
    let r = match o.format {
        ExportFormat::Jpeg => {
            // 4:4:4 normally; 4:2:0 when targeting a file size (much smaller at equal visual quality).
            let sub = if o.limit_kb.is_some() { ChromaSubsampling::S420 } else { o.jpeg_chroma.unwrap_or(ChromaSubsampling::S444) };
            let jpeg = |q: u8| encode::encode_jpeg(&e, q, sub, &meta);
            match o.limit_kb {
                Some(kb) => {
                    let limit = kb as usize * 1024;
                    // Binary search for the highest quality that fits.
                    let (mut lo, mut hi) = (1u8, o.quality);
                    let mut best = None;
                    while lo <= hi {
                        let mid = lo + (hi - lo) / 2;
                        let b = jpeg(mid).map_err(|e| e.to_string())?;
                        if b.len() <= limit {
                            best = Some(b);
                            lo = mid + 1;
                        } else if mid == 1 {
                            break;
                        } else {
                            hi = mid - 1;
                        }
                    }
                    return best.ok_or_else(|| format!("cannot fit within {kb} KB"));
                }
                None => jpeg(o.quality),
            }
        }
        ExportFormat::Png => encode::encode_png(&e, &meta),
        ExportFormat::Tiff => encode::encode_tiff(&e, o.tiff_compression, &meta),
        ExportFormat::Webp => encode::encode_webp_lossless(&e, &meta),
        ExportFormat::Avif => encode::encode_avif(&e, o.quality, 8, &meta),
        f @ (ExportFormat::Original | ExportFormat::Dng) => return Err(format!("{f:?} export does not encode pixels")),
    };
    r.map_err(|e| e.to_string())
}

/// The codecs' name of an output space (for its ICC profile).
pub fn named_space(s: OutputSpace) -> NamedSpace {
    match s {
        OutputSpace::Srgb => NamedSpace::Srgb,
        OutputSpace::DisplayP3 => NamedSpace::DisplayP3,
        OutputSpace::AdobeRgb => NamedSpace::AdobeRgb,
        OutputSpace::ProPhoto => NamedSpace::ProPhoto,
        OutputSpace::Rec2020 => NamedSpace::Rec2020,
    }
}

/// An 8-bit sRGB colour expressed in `space` (encoded with its curve).
pub fn srgb8_in(space: OutputSpace, c: [u8; 3]) -> [u8; 3] {
    if space == OutputSpace::Srgb {
        return c;
    }
    let lin = c.map(lightcraft_color::transfer::decode_srgb8);
    let m = lightcraft_color::SRGB.to_space(&space.rgb_space()).to_f32();
    let t = space.trc();
    std::array::from_fn(|i| {
        let v = m[i][0] * lin[0] + m[i][1] * lin[1] + m[i][2] * lin[2];
        (t.encode(v) * 255.0 + 0.5) as u8
    })
}

/// Encode a render according to `o`: its high-bit-depth samples when it has them (16-bit PNG/TIFF,
/// 10-bit AVIF, 32-bit float linear TIFF), else its 8-bit image.
pub fn encode_rendered(r: &lightcraft_pipeline::Rendered, o: &ExportOptions, meta: Option<&Metadata>) -> Result<Vec<u8>, String> {
    if let Some(source) = &o.source_encoding {
        return encode_in_source_space(r, o, source, meta);
    }
    match &r.deep {
        Some(d) if o.effective_depth() != OutputDepth::U8 => encode_deep(d, o, meta),
        _ => encode_with_metadata(&r.image, o, meta),
    }
}

/// The render still uses the export pipeline, delivered as linear Rec.2020 floats to avoid
/// an 8-bit/sRGB intermediate. Convert those pixels back to the original profile's primaries
/// and per-channel curves before encoding, including nonstandard RGB matrix/TRC profiles.
fn encode_in_source_space(
    r: &lightcraft_pipeline::Rendered,
    o: &ExportOptions,
    source: &SourceEncoding,
    meta: Option<&Metadata>,
) -> Result<Vec<u8>, String> {
    let deep = r.deep.as_ref().ok_or("source-profile export needs a float render")?;
    let DeepSamples::F32(samples) = &deep.samples else { return Err("source-profile export needs linear floats".into()) };
    let matrix = source.space.to_working().inverse().ok_or("the original's colour matrix is singular")?;
    let curves = source.space.trc.as_ref().ok_or("the original's encoding curves are unknown")?;
    let mut converted = Vec::with_capacity(samples.len());
    let depth = o.effective_depth();
    let float = depth == OutputDepth::F32Linear;
    for pixel in samples.as_chunks::<3>().0 {
        let linear = matrix.apply([pixel[0] as f64, pixel[1] as f64, pixel[2] as f64]);
        for (i, value) in linear.into_iter().enumerate() {
            let encoded = curves[i].from_linear(value as f32);
            converted.push(if float { encoded } else { encoded.clamp(0.0, 1.0) });
        }
    }
    let exif = meta.map(lightcraft_meta::write_exif);
    let xmp = meta.map(|m| lightcraft_meta::write_xmp(m, None));
    let meta = EncodeMeta { icc: Some(&source.profile), exif: exif.as_deref(), xmp: xmp.as_deref(), ppi: Some(o.ppi) };
    let u16s;
    let u8s;
    let samples = if float {
        Samples::F32(&converted)
    } else if depth == OutputDepth::U16 {
        u16s = converted.iter().map(|v| (v * 65535.0 + 0.5) as u16).collect::<Vec<_>>();
        Samples::U16(&u16s)
    } else {
        u8s = converted.iter().map(|v| (v * 255.0 + 0.5) as u8).collect::<Vec<_>>();
        Samples::U8(&u8s)
    };
    let image = EncodeImage::new(deep.width as u32, deep.height as u32, 3, samples);
    match o.format {
        ExportFormat::Jpeg => encode::encode_jpeg(&image, o.quality, o.jpeg_chroma.unwrap_or(ChromaSubsampling::S444), &meta),
        ExportFormat::Png => encode::encode_png(&image, &meta),
        ExportFormat::Tiff => encode::encode_tiff(&image, o.tiff_compression, &meta),
        ExportFormat::Webp => encode::encode_webp_lossless(&image, &meta),
        _ => return Err("this format cannot preserve the original colour profile".into()),
    }
    .map_err(|e| e.to_string())
}

/// Encode a high-bit-depth image (see [`encode_rendered`]).
pub fn encode_deep(img: &DeepImage, o: &ExportOptions, meta: Option<&Metadata>) -> Result<Vec<u8>, String> {
    let mut img = img.clone();
    output_sharpen_deep(&mut img, o.sharpen, o.sharpen_amount);
    if let Some(wm) = &o.watermark {
        let wm = Watermark { color: srgb8_in(img.space, wm.color), target: Some(img.space), ..wm.clone() };
        draw_watermark_deep(&mut img, &wm);
    }
    let profile = match img.samples {
        DeepSamples::F32(_) => icc::write_matrix_trc(&img.space.rgb_space(), &lightcraft_codecs::Trc::Linear),
        DeepSamples::U16(_) => icc::write_named(named_space(img.space)),
    };
    let exif = meta.map(lightcraft_meta::write_exif);
    let xmp = meta.map(|m| lightcraft_meta::write_xmp(m, None));
    let meta = EncodeMeta { icc: Some(&profile), exif: exif.as_deref(), xmp: xmp.as_deref(), ppi: Some(o.ppi) };
    let (w, h) = (img.width as u32, img.height as u32);
    let e = match &img.samples {
        DeepSamples::U16(v) => EncodeImage::new(w, h, 3, Samples::U16(v)),
        DeepSamples::F32(v) => EncodeImage::new(w, h, 3, Samples::F32(v)),
    };
    let r = match o.format {
        ExportFormat::Png => encode::encode_png(&e, &meta),
        ExportFormat::Tiff => encode::encode_tiff(&e, o.tiff_compression, &meta),
        ExportFormat::Avif => encode::encode_avif(&e, o.quality, 8, &meta),
        f => return Err(format!("{f:?} export is 8-bit only")),
    };
    r.map_err(|e| e.to_string())
}

/// Parse a shutter speed such as `1/250`, `0.5` or `2"` into seconds.
fn parse_shutter(s: &str) -> Option<f64> {
    let s = s.trim().trim_end_matches(['s', '"']).trim();
    match s.split_once('/') {
        Some((n, d)) => Some(n.trim().parse::<f64>().ok()? / d.trim().parse::<f64>().ok()?),
        None => s.parse().ok(),
    }
    .filter(|v: &f64| v.is_finite() && *v > 0.0)
}

/// The metadata to embed for `photo` under `o.metadata` / `o.remove_location`. `None` = embed nothing.
pub fn export_metadata(photo: &lightcraft_catalog::Photo, o: &ExportOptions) -> Option<Metadata> {
    let m = &photo.meta;
    let text = |s: &str| (!s.trim().is_empty()).then(|| s.to_string());
    // copyright info (also under "copyright only"): notice, creator, status, usage terms, info URL
    let mut out = Metadata {
        copyright: text(&m.copyright),
        copyright_marked: m.copyright_status.marked(),
        usage_terms: text(&m.usage_terms),
        copyright_url: text(&m.copyright_url),
        artist: text(&m.creator),
        creators: m.creators.clone(),
        software: Some("Local Image".into()),
        ..Default::default()
    };
    match o.metadata {
        MetadataPolicy::None => return None,
        MetadataPolicy::Copyright => return Some(out),
        MetadataPolicy::All | MetadataPolicy::AllExceptCamera => {}
    }
    out.title = text(&m.title);
    out.caption = text(&m.caption);
    out.alt_text = text(&m.alt_text);
    out.extended_description = text(&m.extended_description);
    if !o.remove_location {
        out.sublocation = text(&m.location);
        out.city = text(&m.city);
        out.state = text(&m.state);
        out.country = text(&m.country);
    }
    out.keywords = m.keywords.clone();
    if o.include_people_names {
        add_people_keywords(&mut out.keywords, &m.regions);
    }
    out.capture_time = photo.captured.as_deref().and_then(DateTime::parse_iso);
    out.rating = (photo.rating > 0).then_some(photo.rating as i8);
    // Pixels are exported upright: orientation is baked in.
    out.orientation = Some(lightcraft_meta::Orientation::Normal);
    if !o.remove_location {
        out.gps = m.gps.map(|(latitude, longitude)| Gps { latitude, longitude, altitude: None });
    }
    if o.metadata == MetadataPolicy::All {
        out.model = text(&m.camera);
        out.lens_model = text(&m.lens);
        out.focal_length = m.focal_mm.map(f64::from);
        out.f_number = m.aperture.map(f64::from);
        out.exposure_time = parse_shutter(&m.shutter);
        out.iso = m.iso;
    }
    Some(out)
}

/// Names leave the face store only as explicit export metadata; never include IDs or vectors.
fn add_people_keywords(keywords: &mut Vec<String>, regions: &[lightcraft_meta::Region]) {
    for name in regions
        .iter()
        .filter(|r| r.kind == lightcraft_meta::RegionKind::Face)
        .filter_map(|r| r.name.as_deref())
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        let keyword = format!("People|{name}");
        if !keywords.iter().any(|k| k.eq_ignore_ascii_case(&keyword)) {
            keywords.push(keyword);
        }
    }
}

/// One exported file.
pub struct Exported {
    pub file_name: String,
    pub bytes: Vec<u8>,
    pub width: usize,
    pub height: usize,
    /// Files written next to it, named like it with another extension: `(extension, bytes)` (the
    /// XMP sidecar of an `Original` export).
    pub sidecars: Vec<(&'static str, Vec<u8>)>,
}

/// Output size of photo `p` under `o` (its cropped full size when `o.resize` is `None`).
pub fn output_size(p: &lightcraft_catalog::Photo, o: &ExportOptions) -> (usize, usize) {
    let (w, h) = lightcraft_pipeline::native_output_size(p.width.max(1) as usize, p.height.max(1) as usize, &p.develop);
    match &o.resize {
        Some(r) => r.apply(w, h),
        None => ((w.round() as usize).max(1), (h.round() as usize).max(1)),
    }
}

/// Render photo `id` at the requested size and encode it (or copy / convert its original for
/// [`ExportFormat::Original`] / [`ExportFormat::Dng`]).
pub fn export_photo(session: &mut crate::Session, id: lightcraft_catalog::PhotoId, o: &ExportOptions, seq: usize) -> Result<Exported, String> {
    prepare_export(session, id, o, seq)?.run()
}

/// One photo's export, set up from the session ([`prepare_export`]). [`PreparedExport::run`] does
/// the heavy part (read, decode, render, encode) without the session, so it can run on another
/// thread.
pub struct PreparedExport {
    pub photo: lightcraft_catalog::PhotoId,
    pub file_name: String,
    /// Relative folder within a batch destination (Smart Sort). Validated before rendering.
    pub subfolder: Option<String>,
    work: Work,
    /// The library's originals, which [`run_batch`] never writes over (shared by a batch).
    guard: std::sync::Arc<crate::originals::OriginalGuard>,
    /// The folder of the photo's original (`None` for a generated demo photo): the destination
    /// with [`ExportOptions::same_folder`].
    source_dir: Option<String>,
}

struct RenderWork {
    job: crate::media::RenderJob,
    upscale: Option<(std::sync::Arc<dyn crate::enhance::AiHost>, (usize, usize))>,
    meta: Option<Metadata>,
    opts: ExportOptions,
    /// Estimated working memory ([`render_cost`]).
    cost: u64,
}

enum Work {
    Render(Box<RenderWork>),
    /// [`ExportFormat::Original`] (the file's bytes + an XMP sidecar) or [`ExportFormat::Dng`] (the
    /// raw data re-encoded as a lossless DNG with the edits in its XMP).
    File {
        path: String,
        read: Option<crate::merge::ByteReader>,
        packet: String,
        dng: Option<DngCompression>,
        /// "Remove location info": the original must not carry a location (upstream LightCraft db7e378).
        remove_location: bool,
        label: String,
        size: (usize, usize),
    },
}

/// Set up the export of photo `id` at 1-based position `seq` of a batch. (For a whole batch use
/// [`prepare_batch`]: it looks at the library's originals once.)
pub fn prepare_export(
    session: &mut crate::Session,
    id: lightcraft_catalog::PhotoId,
    o: &ExportOptions,
    seq: usize,
) -> Result<PreparedExport, String> {
    let guard = std::sync::Arc::new(session.original_guard());
    prepare_guarded(session, id, o, seq, guard)
}

/// [`prepare_export`] for each of `ids` in order (`{seq}` = position, from 1).
pub fn prepare_batch(session: &mut crate::Session, ids: &[lightcraft_catalog::PhotoId], o: &ExportOptions) -> Result<Vec<PreparedExport>, String> {
    let guard = std::sync::Arc::new(session.original_guard());
    ids.iter().enumerate().map(|(i, id)| prepare_guarded(session, *id, o, i + 1, guard.clone())).collect()
}

pub(crate) fn prepare_guarded(
    session: &mut crate::Session,
    id: lightcraft_catalog::PhotoId,
    o: &ExportOptions,
    seq: usize,
    guard: std::sync::Arc<crate::originals::OriginalGuard>,
) -> Result<PreparedExport, String> {
    let p = session.catalog.photo(id).ok_or("no such photo")?;
    let file_name = o.file_name_for(p, seq);
    let source_dir = match &p.source {
        lightcraft_catalog::Source::File { path } => std::path::Path::new(path).parent().map(|d| d.to_string_lossy().to_string()),
        lightcraft_catalog::Source::Demo { .. } => None,
    };
    let work = if o.format.is_rendered() {
        let (w, h) = output_size(p, o);
        let mut meta = export_metadata(p, o);
        if let (Some(meta), Some(original)) = (&mut meta, &o.source_metadata) {
            meta.fill_missing(original);
            if let (Some(gps), Some(old)) = (&mut meta.gps, &original.gps)
                && gps.latitude == old.latitude
                && gps.longitude == old.longitude
            {
                gps.altitude = old.altitude;
            }
            // Orientation and dimensions describe the new, upright rendered pixels.
            meta.orientation = Some(lightcraft_meta::Orientation::Normal);
            meta.width = Some(w as u32);
            meta.height = Some(h as u32);
        }
        let (space, depth) =
            if o.source_encoding.is_some() { (OutputSpace::Rec2020, OutputDepth::F32Linear) } else { (o.effective_space(), o.effective_depth()) };
        let (nw, nh) = lightcraft_pipeline::native_output_size(p.width.max(1) as usize, p.height.max(1) as usize, &p.develop);
        let native = (nw.round().max(1.0) as usize, nh.round().max(1.0) as usize);
        let cost = render_cost((p.width as usize, p.height as usize), (w, h), depth);
        let upscale = if o.enhance.is_some() && w > native.0 && h > native.1 {
            Some((session.enhance.host.clone().ok_or("AI enlargement is not available in this host")?, (w, h)))
        } else {
            None
        };
        let job = if upscale.is_some() {
            session.export_job(id, native.0, native.1, OutputSpace::Srgb, OutputDepth::U8)?
        } else {
            session.export_job(id, w, h, space, depth)?
        };
        Work::Render(Box::new(RenderWork { job, upscale, meta, opts: o.clone(), cost }))
    } else {
        let lightcraft_catalog::Source::File { path } = &p.source else {
            return Err(format!("{} is a generated demo photo: it has no original file to export", p.file_name));
        };
        Work::File {
            path: path.clone(),
            read: session.media.file_bytes.clone(),
            packet: { crate::sidecar::sidecar_packet(&photo_for_sidecar(p, o.include_people_names, o.remove_location), &session.catalog) },
            dng: (o.format == ExportFormat::Dng).then_some(o.dng_compression),
            remove_location: o.remove_location,
            label: p.file_name.clone(),
            size: (p.width as usize, p.height as usize),
        }
    };
    Ok(PreparedExport { photo: id, file_name, subfolder: None, work, guard, source_dir })
}

/// One export after its render stage ([`PreparedExport::develop`]), before encoding.
pub(crate) enum Developed {
    /// Rendered pixels, to encode in the requested format.
    Image(Box<RenderedExport>),
    /// The raw data of a DNG export, to write as a DNG.
    Raw { raw: Box<lightcraft_raw::RawImage>, packet: String, compression: lightcraft_raw::DngCompression },
    /// Nothing left to encode (an `Original` export: the file's own bytes).
    Done(Exported),
}

/// A rendered export waiting for its encoder.
pub(crate) struct RenderedExport {
    r: lightcraft_pipeline::Rendered,
    meta: Option<Metadata>,
    opts: ExportOptions,
}

impl Developed {
    /// Bytes it holds while it waits for the encoder, plus the encoder's working copy (what a
    /// pipeline keeps reserved for it after the render, [`lightcraft_sysmem::Permit::finish_heavy`]).
    pub(crate) fn reserved_bytes(&self) -> u64 {
        let held = match self {
            Developed::Image(img) => {
                let r = &img.r;
                let deep = r.deep.as_ref().map_or(0, |d| match &d.samples {
                    DeepSamples::U16(v) => v.len() * 2,
                    DeepSamples::F32(v) => v.len() * 4,
                });
                r.image.data.len() * 4 + deep
            }
            Developed::Raw { raw, .. } => raw.width * raw.height * raw.cpp.max(1) * 2,
            Developed::Done(e) => e.bytes.len() + e.sidecars.iter().map(|(_, b)| b.len()).sum::<usize>(),
        } as u64;
        match self {
            Developed::Done(_) => held,
            _ => held * 2,
        }
    }
}

/// Pixels assumed for a photo whose size the catalog doesn't know (a 24 MP camera).
const UNKNOWN_PIXELS: u64 = 24_000_000;

fn pixels((w, h): (usize, usize)) -> u64 {
    if w == 0 || h == 0 { UNKNOWN_PIXELS } else { w as u64 * h as u64 }
}

/// Working memory of one rendered export (threading rule 6), from its pixel counts: the decoded
/// source in f32 RGB (12 B/px) plus about two working copies in the develop pipeline (36 B/px of
/// the source), and at the output size the RGBA8 result, the encoder's copy and the encoded file
/// (about 12 B/px) plus the high-bit-depth samples when the format keeps them.
pub(crate) fn render_cost(source: (usize, usize), out: (usize, usize), depth: OutputDepth) -> u64 {
    let deep = match depth {
        OutputDepth::U8 => 0,
        OutputDepth::U16 => 6,
        OutputDepth::F32Linear => 12,
    };
    pixels(source) * 36 + pixels(out) * (12 + deep)
}

/// Working memory of an `Original` (the file's bytes, about 2 B/px for a raw) or `Dng` export
/// (the file's bytes, the decoded 16-bit raw data and the DNG written from it: about 8 B/px).
fn file_cost(size: (usize, usize), dng: bool) -> u64 {
    pixels(size) * if dng { 8 } else { 2 }
}

impl PreparedExport {
    pub fn run(self) -> Result<Exported, String> {
        self.run_with_ctl(&crate::enhance::JobCtl::new())
    }

    pub fn run_with_ctl(self, ctl: &crate::enhance::JobCtl) -> Result<Exported, String> {
        let (file_name, developed) = self.develop(ctl)?;
        encode_developed(file_name, developed, ctl)
    }

    /// The working memory this export is expected to need (for its heavy-job permit).
    pub fn estimated_bytes(&self) -> u64 {
        match &self.work {
            Work::Render(w) => w.cost,
            Work::File { size, dng, .. } => file_cost(*size, dng.is_some()),
        }
    }

    /// The render stage: read and decode the original, develop it (the GPU one batch item at a
    /// time, [`pipeline::run_render_job`]), enlarge it with AI if asked; or read the original /
    /// decode the raw data of an `Original` / `Dng` export. Encoding is [`encode_developed`].
    pub(crate) fn develop(self, ctl: &crate::enhance::JobCtl) -> Result<(String, Developed), String> {
        if ctl.cancelled() {
            return Err(crate::enhance::CANCELLED.to_owned());
        }
        let file_name = self.file_name;
        match self.work {
            Work::Render(w) => {
                let RenderWork { job, upscale, meta, opts, cost: _ } = *w;
                let mut r = pipeline::run_render_job(job).rendered?;
                if let Some((host, target)) = upscale {
                    let request = crate::enhance::UpscaleRequest {
                        width: r.image.width,
                        height: r.image.height,
                        rgba: r.image.data.clone(),
                        target,
                        method: opts.enhance.ok_or("missing enhancement method")?.key().into(),
                    };
                    // AI enlargement (CPU inference or the ComfyUI GPU queue): one at a time
                    let pixels = {
                        let _one = pipeline::AI_GATE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                        host.upscale(&request, ctl)?
                    };
                    if pixels.len() != target.0 * target.1 {
                        return Err("AI enlargement returned an unexpected size".into());
                    }
                    r.image = Rgba8 { width: target.0, height: target.1, data: pixels };
                    r.deep = enhanced_samples(&mut r.image, &opts);
                }
                Ok((file_name, Developed::Image(Box::new(RenderedExport { r, meta, opts }))))
            }
            Work::File { path, read, packet, dng, remove_location, label, size } => {
                let bytes = match &read {
                    Some(r) => r(&path)?,
                    None => std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?,
                };
                let Some(dng) = dng else {
                    // the file is copied byte for byte: refuse rather than leak a location it embeds
                    if remove_location && lightcraft_meta::extract(&bytes).gps.is_some() {
                        return Err(format!(
                            "{label}: can't remove the location from an original file that embeds GPS; export as JPEG, TIFF or DNG instead"
                        ));
                    }
                    let e =
                        Exported { file_name: file_name.clone(), bytes, width: size.0, height: size.1, sidecars: vec![("xmp", packet.into_bytes())] };
                    return Ok((file_name, Developed::Done(e)));
                };
                if lightcraft_raw::probe(&bytes).is_none() {
                    return Err(format!("{label}: DNG export needs a raw photo"));
                }
                let raw = lightcraft_raw::decode(&bytes).map_err(|e| format!("{label}: {e}"))?;
                drop(bytes);
                let compression = match dng {
                    DngCompression::Lossless => lightcraft_raw::DngCompression::Lj92 { tile: 256 },
                    DngCompression::Deflate => lightcraft_raw::DngCompression::Deflate { tile: 256, half: false },
                    DngCompression::Uncompressed => lightcraft_raw::DngCompression::Uncompressed,
                };
                Ok((file_name, Developed::Raw { raw: Box::new(raw), packet, compression }))
            }
        }
    }
}

/// The encode stage of one export: the file's bytes from its [`Developed`] result.
pub(crate) fn encode_developed(file_name: String, developed: Developed, ctl: &crate::enhance::JobCtl) -> Result<Exported, String> {
    match developed {
        Developed::Image(img) => {
            let RenderedExport { r, meta, opts } = *img;
            if ctl.cancelled() {
                return Err(crate::enhance::CANCELLED.to_owned());
            }
            let bytes = encode_rendered(&r, &opts, meta.as_ref())?;
            Ok(Exported { file_name, bytes, width: r.image.width, height: r.image.height, sidecars: Vec::new() })
        }
        Developed::Raw { raw, packet, compression } => {
            let dng = lightcraft_raw::write_dng(&raw, &lightcraft_raw::DngWriteOptions { xmp: Some(packet), compression, ..Default::default() })
                .map_err(|e| e.to_string())?;
            Ok(Exported { file_name, bytes: dng, width: raw.width, height: raw.height, sidecars: Vec::new() })
        }
        Developed::Done(e) => Ok(e),
    }
}

/// How an exported DNG stores its raw data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DngCompression {
    /// Lossless JPEG (the usual DNG compression).
    #[default]
    Lossless,
    /// Deflate (zip) tiles with prediction.
    Deflate,
    Uncompressed,
}

/// The destination of a batch: a folder (with `ExportOptions::subfolder` and the conflict policy
/// applied to each file together with its sidecars), or one exact file path (single photo; an
/// ordinary file already there is replaced). Either way a catalogued original (or its sidecar)
/// is never written over: see [`crate::originals`].
#[derive(Clone, Debug, Default)]
pub struct Destination {
    pub dir: String,
    pub exact: Option<String>,
}

/// Where each of `ids` would be exported to under `o` and `to`, before any conflict policy:
/// the folder ([`ExportOptions::same_folder`]: the photo's own; plus the subfolder) and the file
/// name ([`ExportOptions::file_name_for`]). `None` for a photo with no folder of its own. Used to
/// preview names and to ask about existing files before an export starts.
pub fn planned_paths(
    catalog: &lightcraft_catalog::Catalog,
    ids: &[lightcraft_catalog::PhotoId],
    o: &ExportOptions,
    to: &Destination,
) -> Vec<Option<String>> {
    let join = |a: &str, b: &str| if a.is_empty() { b.to_string() } else { format!("{}/{b}", a.trim_end_matches('/')) };
    let in_sub = |d: &str| if o.subfolder.is_empty() { d.to_string() } else { join(d, &o.subfolder) };
    ids.iter()
        .enumerate()
        .map(|(i, id)| {
            let p = catalog.photo(*id)?;
            if let Some(exact) = to.exact.as_deref().filter(|_| ids.len() == 1) {
                return Some(exact.to_string());
            }
            let dir = match (&p.source, o.same_folder) {
                (_, false) => in_sub(&to.dir),
                (lightcraft_catalog::Source::File { path }, true) => in_sub(&std::path::Path::new(path).parent()?.to_string_lossy()),
                (lightcraft_catalog::Source::Demo { .. }, true) => return None,
            };
            Some(join(&dir, &o.file_name_for(p, i + 1)))
        })
        .collect()
}

/// Export `ids` in order ([`prepare_batch`] + [`run_batch`]), stopping at the first error.
pub fn export_batch(
    session: &mut crate::Session,
    ids: &[lightcraft_catalog::PhotoId],
    o: &ExportOptions,
    to: &Destination,
    write: &mut dyn FnMut(&str, &[u8]) -> Result<(), String>,
    exists: &dyn Fn(&str) -> bool,
) -> Result<Vec<serde_json::Value>, String> {
    let items = prepare_batch(session, ids, o)?;
    run_batch(items, o, to, write, exists, true, &mut |_, _| true)
}

/// Write an exported or rendered file on disk: its folder is created if needed, and the file is
/// replaced atomically ([`lightcraft_catalog::safe_file::write_atomic_nosync`]: a temp file
/// renamed into place), so a failure part-way leaves any previous file intact and no truncated
/// one. Not synced to disk (issue #134): an export can always be made again from the original.
/// The native writer behind exports, renders and screenshots (app, CLI, MCP).
pub fn write_file(path: &str, bytes: &[u8]) -> Result<(), String> {
    write_with(path, bytes, false)
}

/// [`write_file`], synced to disk: for a written file that becomes a library photo (Edit a Copy)
/// and so can't simply be exported again once the catalog points at it.
pub fn write_file_durable(path: &str, bytes: &[u8]) -> Result<(), String> {
    write_with(path, bytes, true)
}

fn write_with(path: &str, bytes: &[u8], durable: bool) -> Result<(), String> {
    use lightcraft_catalog::safe_file::{write_atomic, write_atomic_nosync};
    let p = std::path::Path::new(path);
    if let Some(dir) = p.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    if durable { write_atomic(p, bytes) } else { write_atomic_nosync(p, bytes) }.map_err(|e| format!("{path}: {e}"))
}

/// The model emits sRGB8; preserve the requested output primaries, curve and container depth.
/// Restoration has 8-bit source precision even in 16/32-bit exports.
fn enhanced_samples(image: &mut Rgba8, opts: &ExportOptions) -> Option<DeepImage> {
    let (space, depth) = if opts.source_encoding.is_some() {
        (OutputSpace::Rec2020, OutputDepth::F32Linear)
    } else {
        (opts.effective_space(), opts.effective_depth())
    };
    let matrix = lightcraft_color::SRGB.to_space(&space.rgb_space());
    let trc = space.trc();
    let mut values = Vec::with_capacity(image.data.len() * 3);
    let linear = depth == OutputDepth::F32Linear;
    for pixel in &mut image.data {
        let rgb = matrix.apply([pixel[0], pixel[1], pixel[2]].map(|v| lightcraft_color::transfer::decode_srgb8(v) as f64));
        for c in 0..3 {
            let encoded = trc.encode(rgb[c] as f32).clamp(0.0, 1.0);
            values.push(if linear { rgb[c] as f32 } else { encoded });
            pixel[c] = (encoded * 255.0).round() as u8;
        }
    }
    let samples = match depth {
        OutputDepth::U8 => return None,
        OutputDepth::U16 => DeepSamples::U16(values.into_iter().map(|v| (v * 65535.0).round() as u16).collect()),
        OutputDepth::F32Linear => DeepSamples::F32(values),
    };
    Some(DeepImage { width: image.width, height: image.height, space, samples })
}

/// The path of the sidecar with extension `ext` of the exported file `main`.
fn sidecar_path(main: &str, ext: &str) -> String {
    std::path::Path::new(main).with_extension(ext).to_string_lossy().to_string()
}

/// Run prepared exports in order: pick each one's path, and hand the bytes (and sidecars) to
/// `write`. `exists` tells whether a path is taken. The conflict policy applies to a file and its
/// sidecars as one: with Unique both get the same free name, with Skip the photo is skipped when
/// either is taken. A path that is a catalogued original (or its sidecar) is refused whatever the
/// policy. `progress(done, next file)` is called before each photo; returning false cancels the
/// rest. Returns one JSON object per photo: `{path, photo, width, height, bytes, sidecars}`,
/// `{skipped: path}` or (unless `stop_on_error`) `{photo, file, error}`.
pub fn run_batch(
    items: Vec<PreparedExport>,
    o: &ExportOptions,
    to: &Destination,
    write: &mut dyn FnMut(&str, &[u8]) -> Result<(), String>,
    exists: &dyn Fn(&str) -> bool,
    stop_on_error: bool,
    progress: &mut dyn FnMut(usize, &str) -> bool,
) -> Result<Vec<serde_json::Value>, String> {
    run_batch_with_ctl(items, o, to, write, exists, stop_on_error, progress, &crate::enhance::JobCtl::new())
}

/// The background export uses this control for cancellation within a restoration tile/job.
///
/// The batch runs as an ordered pipeline (threading rules 3, 5, 6, 10): a feeder admits photos in
/// order, each with a heavy-job permit sized from its pixels ([`lightcraft_sysmem::heavy_jobs`]),
/// render threads develop them (the GPU one at a time), encode threads encode them, and this
/// thread names, checks and writes them strictly in input order (`progress`, conflict handling,
/// the originals guard and `write` stay here). Stage threads are plain OS threads in the Utility
/// class joined by bounded channels; rayon is only used inside a stage. How many photos are in
/// flight comes from the compute profile and the free memory, never a constant.
#[allow(clippy::too_many_arguments)]
pub fn run_batch_with_ctl(
    items: Vec<PreparedExport>,
    o: &ExportOptions,
    to: &Destination,
    write: &mut dyn FnMut(&str, &[u8]) -> Result<(), String>,
    exists: &dyn Fn(&str) -> bool,
    stop_on_error: bool,
    progress: &mut dyn FnMut(usize, &str) -> bool,
    ctl: &crate::enhance::JobCtl,
) -> Result<Vec<serde_json::Value>, String> {
    let permits = lightcraft_sysmem::heavy_jobs();
    let shape = pipeline::Shape::auto(items.len(), permits.max_jobs(), lightcraft_sysmem::profile().threads);
    run_batch_shaped(items, o, to, write, exists, stop_on_error, progress, ctl, permits, shape)
}

/// What the write stage does at one position of a batch.
enum Slot {
    /// Rendered by the pipeline, written into `dir`.
    Run { dir: String, has_subfolder: bool },
    /// Reported as this photo's error (or ends the batch with `stop_on_error`).
    Fail(String),
    /// Ends the batch with this error when reached.
    Abort(String),
}

/// [`run_batch_with_ctl`] with explicit permits and pipeline shape (tests compare shapes).
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_batch_shaped(
    items: Vec<PreparedExport>,
    o: &ExportOptions,
    to: &Destination,
    write: &mut dyn FnMut(&str, &[u8]) -> Result<(), String>,
    exists: &dyn Fn(&str) -> bool,
    stop_on_error: bool,
    progress: &mut dyn FnMut(usize, &str) -> bool,
    ctl: &crate::enhance::JobCtl,
    permits: &lightcraft_sysmem::HeavyJobs,
    shape: pipeline::Shape,
) -> Result<Vec<serde_json::Value>, String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    let join = |a: &str, b: &str| if a.is_empty() { b.to_string() } else { format!("{}/{b}", a.trim_end_matches('/')) };
    let in_sub = |d: &str| if o.subfolder.is_empty() { d.to_string() } else { join(d, &o.subfolder) };
    let batch_dir = in_sub(&to.dir);
    let single = items.len() == 1;

    // Each photo's folder first (pure, in order); an error is reported when the batch reaches it,
    // as before, and nothing after a batch-ending error is rendered.
    let mut slots = Vec::with_capacity(items.len());
    let mut heads = Vec::with_capacity(items.len());
    let mut work = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        let name = item.file_name.clone();
        let has_subfolder = item.subfolder.is_some();
        let dir = match (o.same_folder && !has_subfolder, &item.source_dir) {
            (false, _) => Ok(batch_dir.clone()),
            (true, Some(d)) => Ok(in_sub(d)),
            (true, None) => Err(format!("{name}: a generated demo photo has no folder of its own; choose a specific folder")),
        };
        let slot = match (dir, &item.subfolder) {
            (Err(e), _) => Slot::Fail(e),
            (Ok(dir), None) => Slot::Run { dir, has_subfolder },
            (Ok(dir), Some(sub)) => match crate::smart_sort::plan::sanitize(sub) {
                Ok(sub) => Slot::Run { dir: join(&dir, &sub), has_subfolder },
                Err(e) => Slot::Abort(e),
            },
        };
        heads.push((item.photo, name, item.guard.clone()));
        let abort = matches!(slot, Slot::Abort(_));
        if matches!(slot, Slot::Run { .. }) {
            work.push((i, item));
        }
        slots.push(slot);
        if abort {
            break; // the batch ends here: later photos are never rendered
        }
    }

    let flow = pipeline::Flow::default();
    if cfg!(target_arch = "wasm32") {
        // no threads in the browser: each photo is rendered and encoded when the writer reaches it
        let mut work: std::collections::HashMap<usize, PreparedExport> = work.into_iter().collect();
        let mut next = |i: usize| work.remove(&i).map(|item| item.run_with_ctl(ctl));
        return write_stage(&mut next, &slots, &heads, o, to, single, write, exists, stop_on_error, progress, ctl, &flow, &join);
    }
    let stop = AtomicBool::new(false);
    let stopped = || stop.load(Ordering::Relaxed) || ctl.cancelled();
    std::thread::scope(|sc| {
        let outputs = pipeline::spawn(sc, work, shape, permits, &flow, &stopped, ctl);
        let r = {
            // this photo's result, keeping those that finish before it (with their permits) until
            // their turn; `None` once the pipeline ended without it
            let mut early = std::collections::BTreeMap::new();
            let mut next = move |i: usize| loop {
                if let Some((r, _permit)) = early.remove(&i) {
                    return Some(r);
                }
                match outputs.recv() {
                    Ok((seq, r, permit)) => {
                        early.insert(seq, (r, permit));
                    }
                    Err(_) if stopped() => return None,
                    Err(_) => return Some(Err("the export stopped unexpectedly: its worker threads could not run".to_string())),
                }
            };
            write_stage(&mut next, &slots, &heads, o, to, single, write, exists, stop_on_error, progress, ctl, &flow, &join)
            // `next` goes here with the receiver and the results held back: an encoder waiting
            // to hand over a photo sees the channel closed
        };
        // whatever ended the batch (done, cancelled, an error): the other stages stop now
        stop.store(true, Ordering::Relaxed);
        flow.wake();
        r
    })
}

type Head = (lightcraft_catalog::PhotoId, String, std::sync::Arc<crate::originals::OriginalGuard>);

/// The write stage of [`run_batch_shaped`], on the caller's thread: takes each photo's result in
/// input order (`next(position)`) and names, checks and writes it.
#[allow(clippy::too_many_arguments)]
fn write_stage(
    next: &mut dyn FnMut(usize) -> Option<Result<Exported, String>>,
    slots: &[Slot],
    heads: &[Head],
    o: &ExportOptions,
    to: &Destination,
    single: bool,
    write: &mut dyn FnMut(&str, &[u8]) -> Result<(), String>,
    exists: &dyn Fn(&str) -> bool,
    stop_on_error: bool,
    progress: &mut dyn FnMut(usize, &str) -> bool,
    ctl: &crate::enhance::JobCtl,
    flow: &pipeline::Flow,
    join: &dyn Fn(&str, &str) -> String,
) -> Result<Vec<serde_json::Value>, String> {
    use serde_json::json;
    let mut taken = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (i, (slot, (photo, name, guard))) in slots.iter().zip(heads).enumerate() {
        if ctl.cancelled() || !progress(i, name) {
            break;
        }
        let photo = *photo;
        let (dir, has_subfolder) = match slot {
            Slot::Run { dir, has_subfolder } => (dir, *has_subfolder),
            Slot::Fail(err) => {
                if stop_on_error {
                    return Err(err.clone());
                }
                out.push(json!({"photo": photo.0, "file": name, "error": err}));
                flow.written(i + 1);
                continue;
            }
            Slot::Abort(err) => return Err(err.clone()),
        };
        // the pipeline ended without it: cancelled
        let Some(result) = next(i) else { break };
        let e = match result {
            Ok(e) => e,
            Err(err) if stop_on_error => return Err(err),
            Err(err) => {
                out.push(json!({"photo": photo.0, "file": name, "error": err}));
                flow.written(i + 1);
                continue;
            }
        };
        // the exported file and its sidecars
        let group = |main: &str| std::iter::once(main.to_string()).chain(e.sidecars.iter().map(|(x, _)| sidecar_path(main, x))).collect::<Vec<_>>();
        let path = match to.exact.as_deref().filter(|_| single && !has_subfolder) {
            Some(p) => Ok(p.to_string()),
            None => {
                let path = join(dir, &e.file_name);
                let busy = |main: &str| group(main).iter().any(|p| taken.contains(p) || exists(p));
                if !busy(&path) {
                    Ok(path)
                } else {
                    match o.conflict {
                        Conflict::Overwrite if !group(&path).iter().any(|p| taken.contains(p)) => Ok(path),
                        Conflict::Skip => {
                            out.push(json!({"skipped": path}));
                            flow.written(i + 1);
                            continue;
                        }
                        Conflict::Unique | Conflict::Overwrite => {
                            let (stem, ext) = e.file_name.rsplit_once('.').map_or((e.file_name.as_str(), None), |(a, b)| (a, Some(b)));
                            let name = |n: usize| join(dir, &ext.map_or(format!("{stem}-{n}"), |x| format!("{stem}-{n}.{x}")));
                            (2..1_000_000).map(name).find(|p| !busy(p)).ok_or_else(|| format!("{path}: no free file name"))
                        }
                    }
                }
            }
        };
        if ctl.cancelled() {
            break;
        }
        let file = path.clone().unwrap_or_else(|_| name.clone());
        let written = path.and_then(|path| {
            let files = group(&path);
            // never over an original, whatever the conflict policy or the exact path said
            for f in &files {
                guard.check(std::path::Path::new(f))?;
            }
            write(&path, &e.bytes)?;
            let mut sidecars = Vec::new();
            for ((_, bytes), sc) in e.sidecars.iter().zip(files.iter().skip(1)) {
                write(sc, bytes)?;
                sidecars.push(sc.clone());
            }
            Ok((path, files, sidecars))
        });
        match written {
            Ok((path, files, sidecars)) => {
                taken.extend(files);
                out.push(json!({"path": path, "photo": photo.0, "width": e.width, "height": e.height, "bytes": e.bytes.len(), "sidecars": sidecars}));
            }
            Err(err) if stop_on_error => return Err(err),
            Err(err) => out.push(json!({"photo": photo.0, "file": file, "error": err})),
        }
        flow.written(i + 1);
    }
    Ok(out)
}

/// The bounded, ordered batch pipeline behind exports (and the GPU gate Build Previews shares).
pub mod pipeline {
    use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
    use std::sync::{Arc, Condvar, Mutex, PoisonError};
    use std::time::Duration;

    use lightcraft_sysmem::{HeavyJobs, Permit, ThreadClass};

    use super::{Exported, PreparedExport};

    /// Batch renders that would use the GPU take this one at a time: concurrent full-size GPU
    /// renders could exhaust video memory, and a failed GPU render turns the GPU off for the
    /// process. Their decoding runs in parallel outside it. Interactive renders (loupe, grid)
    /// never take it.
    pub static GPU_GATE: Mutex<()> = Mutex::new(());
    /// AI enlargement (CPU inference or the ComfyUI GPU queue) in an export: one at a time.
    pub static AI_GATE: Mutex<()> = Mutex::new(());

    /// How many stage threads a batch uses and how far ahead of the writer it may run.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Shape {
        /// Render threads (each develops one photo at a time).
        pub renderers: usize,
        /// Encode threads.
        pub encoders: usize,
        /// Photos admitted but not yet written, at most.
        pub window: usize,
    }

    impl Shape {
        /// One photo at a time, render → encode → write: the old serial loop (for tests and the
        /// "concurrent exports: 1" choice).
        pub fn serial() -> Shape {
            Shape { renderers: 1, encoders: 1, window: 1 }
        }

        /// From the heavy-job slots (`max_jobs`, memory still limits admission) and the compute
        /// threads `C`: one render thread per slot, one encode thread more (encoders are mostly
        /// single-threaded per image) within `C`, never more threads than photos; the window lets
        /// every stage hold a photo plus one waiting between stages.
        pub fn auto(items: usize, max_jobs: usize, threads: usize) -> Shape {
            let items = items.max(1);
            let renderers = max_jobs.clamp(1, items);
            let encoders = (max_jobs + 1).min(threads).clamp(1, items);
            Shape { renderers, encoders, window: renderers + encoders + 2 }
        }
    }

    /// One photo out of the pipeline: its position, its file (or error) and its permit (released
    /// once written).
    pub(crate) type Output<'p> = (usize, Result<Exported, String>, Permit<'p>);

    /// How far the writer got, so the feeder never runs more than [`Shape::window`] photos ahead.
    #[derive(Default)]
    pub(crate) struct Flow {
        written: Mutex<usize>,
        cv: Condvar,
    }

    impl Flow {
        /// Positions before `n` are done.
        pub(crate) fn written(&self, n: usize) {
            *self.written.lock().unwrap_or_else(PoisonError::into_inner) = n;
            self.cv.notify_all();
        }

        /// Wake the feeder (the batch is ending).
        pub(crate) fn wake(&self) {
            self.cv.notify_all();
        }

        /// Wait until position `seq` is within `window` of the writer; false once `stopped`.
        fn admit(&self, seq: usize, window: usize, stopped: &dyn Fn() -> bool) -> bool {
            let mut w = self.written.lock().unwrap_or_else(PoisonError::into_inner);
            loop {
                if stopped() {
                    return false;
                }
                if seq < *w + window.max(1) {
                    return true;
                }
                w = self.cv.wait_timeout(w, Duration::from_millis(50)).unwrap_or_else(PoisonError::into_inner).0;
            }
        }
    }

    /// Run a batch's render job: decode first, then develop. When the develop may use the GPU it
    /// takes [`GPU_GATE`] (the decode before it does not), so batch items share the GPU one at a
    /// time while their decoding and the CPU work around it overlap.
    pub fn run_render_job(mut job: crate::media::RenderJob) -> crate::media::RenderResult {
        // the same request-level conditions under which `lightcraft_gpu::render` declines; with
        // no usable GPU (or GPU off) batch renders run side by side on the CPU
        let gpu = job.cache.is_none()
            && job.request.proof.is_none()
            && job.request.depth != lightcraft_pipeline::OutputDepth::F32Linear
            && lightcraft_gpu::available();
        if !gpu {
            return job.run();
        }
        if !matches!(job.source, crate::media::SourceRef::Loaded(_)) {
            match crate::guard::catch("Preview render", || job.source.load_source()) {
                Ok(Ok(src)) => job.source = crate::media::SourceRef::Loaded(Box::new(src)),
                // the job reports the failure as it always has
                _ => return job.run(),
            }
        }
        let _gpu = GPU_GATE.lock().unwrap_or_else(PoisonError::into_inner);
        job.run()
    }

    /// A receiver shared by a stage's threads (dropped with the last of them, so the stage before
    /// sees the channel close).
    type Shared<T> = Arc<Mutex<Receiver<T>>>;

    fn next<T>(rx: &Shared<T>) -> Option<T> {
        rx.lock().unwrap_or_else(PoisonError::into_inner).recv().ok()
    }

    fn named<'s, F: FnOnce() + Send + 's>(sc: &'s std::thread::Scope<'s, '_>, name: String, f: F) {
        let started = std::thread::Builder::new().name(name).spawn_scoped(sc, move || {
            lightcraft_sysmem::set_current_thread_class(ThreadClass::Utility);
            f()
        });
        if let Err(e) = started {
            log::warn!("export: could not start a pipeline thread ({e})");
        }
    }

    /// Start the feeder, render and encode threads for `work` (position, photo) in scope `sc`;
    /// returns the channel the finished photos arrive on (in any order, each with its position).
    /// Every stage stops at its next item once `stopped` says so; dropping the receiver also ends
    /// them.
    pub(crate) fn spawn<'s, 'e: 's>(
        sc: &'s std::thread::Scope<'s, 'e>,
        work: Vec<(usize, PreparedExport)>,
        shape: Shape,
        permits: &'e HeavyJobs,
        flow: &'e Flow,
        stopped: &'e (dyn Fn() -> bool + Sync),
        ctl: &'e crate::enhance::JobCtl,
    ) -> Receiver<Output<'e>> {
        type Admitted<'p> = (usize, PreparedExport, Permit<'p>);
        type Developed<'p> = (usize, Result<(String, super::Developed), String>, Permit<'p>);
        let (adm_tx, adm_rx): (SyncSender<Admitted<'e>>, _) = sync_channel(1);
        let (dev_tx, dev_rx): (SyncSender<Developed<'e>>, _) = sync_channel(1);
        let (out_tx, out_rx) = sync_channel::<Output<'e>>(1);

        // feeder: admit in order, within the window, each with a permit sized from its pixels
        named(sc, "export-feed".into(), move || {
            for (seq, item) in work {
                if !flow.admit(seq, shape.window, stopped) {
                    break;
                }
                let Some(permit) = permits.admit(item.estimated_bytes(), stopped) else { break };
                if adm_tx.send((seq, item, permit)).is_err() {
                    break;
                }
            }
        });
        let adm_rx: Shared<Admitted<'e>> = Arc::new(Mutex::new(adm_rx));
        for n in 0..shape.renderers.max(1) {
            let (rx, tx) = (adm_rx.clone(), dev_tx.clone());
            named(sc, format!("export-render-{n}"), move || {
                while let Some((seq, item, mut permit)) = next(&rx) {
                    if stopped() {
                        continue; // dropped: its permit is released
                    }
                    let r = crate::guard::catch("Export", || item.develop(ctl)).and_then(|r| r);
                    // the render's working memory is free now; the result waits for an encoder
                    permit.finish_heavy(r.as_ref().map_or(0, |(_, d)| d.reserved_bytes()));
                    if tx.send((seq, r, permit)).is_err() {
                        break;
                    }
                }
            });
        }
        drop((adm_rx, dev_tx));
        let dev_rx: Shared<Developed<'e>> = Arc::new(Mutex::new(dev_rx));
        for n in 0..shape.encoders.max(1) {
            let (rx, tx) = (dev_rx.clone(), out_tx.clone());
            named(sc, format!("export-encode-{n}"), move || {
                while let Some((seq, r, mut permit)) = next(&rx) {
                    if stopped() {
                        continue;
                    }
                    let r = r.and_then(|(name, d)| crate::guard::catch("Export", || super::encode_developed(name, d, ctl)).and_then(|r| r));
                    permit.shrink(r.as_ref().map_or(0, |e| (e.bytes.len() + e.sidecars.iter().map(|(_, b)| b.len()).sum::<usize>()) as u64));
                    if tx.send((seq, r, permit)).is_err() {
                        break;
                    }
                }
            });
        }
        out_rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bugfix_save_o029_overwrite_never_reuses_a_batch_output_or_sidecar() {
        use lightcraft_catalog::{Op, PhotoId};
        let mut session = crate::Session::with_demo();
        for id in 1..=2 {
            let mut photo = (**session.catalog.photo(PhotoId(id)).unwrap()).clone();
            photo.file_name = "same.png".into();
            photo.format = "PNG".into();
            photo.width = 8;
            photo.height = 6;
            session.catalog.apply(Op::RemovePhoto { id: PhotoId(id) }).unwrap();
            session.catalog.apply(Op::AddPhoto { photo: Box::new(photo) }).unwrap();
        }
        let options = ExportOptions { conflict: Conflict::Overwrite, format: ExportFormat::Png, ..Default::default() };
        let items = prepare_batch(&mut session, &[PhotoId(1), PhotoId(2)], &options).unwrap();
        let mut paths = Vec::new();
        let result = run_batch(
            items,
            &options,
            &Destination { dir: "/exports".into(), exact: None },
            &mut |path, _| {
                paths.push(path.to_string());
                Ok(())
            },
            &|p| p == "/exports/same.png",
            true,
            &mut |_, _| true,
        )
        .unwrap();
        assert_eq!(paths, ["/exports/same.png", "/exports/same-2.png"]);
        assert_eq!(result.len(), 2);

        let guard = std::sync::Arc::new(session.original_guard());
        let items = ["pair.nef", "pair.jpg"]
            .into_iter()
            .enumerate()
            .map(|(i, name)| PreparedExport {
                photo: PhotoId(i as u64 + 1),
                file_name: name.into(),
                subfolder: None,
                source_dir: None,
                guard: guard.clone(),
                work: Work::File {
                    path: format!("/input/{name}"),
                    read: Some(std::sync::Arc::new(|_| Ok(b"original".to_vec()))),
                    packet: "<xmp/>".into(),
                    dng: None,
                    remove_location: false,
                    label: name.into(),
                    size: (8, 6),
                },
            })
            .collect();
        let mut paths = Vec::new();
        run_batch(
            items,
            &options,
            &Destination { dir: "/exports".into(), exact: None },
            &mut |path, _| {
                paths.push(path.to_string());
                Ok(())
            },
            &|_| false,
            true,
            &mut |_, _| true,
        )
        .unwrap();
        assert_eq!(paths, ["/exports/pair.nef", "/exports/pair.xmp", "/exports/pair-2.jpg", "/exports/pair-2.xmp"]);
    }

    // ---- the ordered batch pipeline (threading rules 6, 9, 10, 11)

    fn wide_open() -> lightcraft_sysmem::HeavyJobs {
        lightcraft_sysmem::HeavyJobs::new(4, 64 << 30, || lightcraft_sysmem::MemoryNow { available: 64 << 30, rss: None })
    }

    fn parallel() -> pipeline::Shape {
        pipeline::Shape { renderers: 3, encoders: 3, window: 8 }
    }

    /// Demo photos as rendered exports, `n` of them (the demo has a few; repeated as needed).
    fn rendered_batch(session: &mut crate::Session, n: usize, o: &ExportOptions) -> Vec<PreparedExport> {
        let ids: Vec<_> = session.catalog.photos().map(|p| p.id).collect();
        let ids: Vec<_> = (0..n).map(|i| ids[i % ids.len()]).collect();
        prepare_batch(session, &ids, o).unwrap()
    }

    /// `bytes` with the creation time of each embedded ICC profile (the 12 bytes before its `acsp`
    /// signature) blanked: the profile writer stamps the current time, so two runs a second apart
    /// differ there and nowhere else.
    fn without_icc_dates(bytes: &[u8]) -> Vec<u8> {
        let mut b = bytes.to_vec();
        let at: Vec<usize> = b.windows(4).enumerate().filter(|(i, w)| *w == b"acsp" && *i >= 12).map(|(i, _)| i).collect();
        for i in at {
            b[i - 12..i].fill(0);
        }
        b
    }

    /// `(path, bytes)` in write order and the progress calls of one shaped run (ICC creation times
    /// blanked, [`without_icc_dates`]).
    #[allow(clippy::type_complexity)]
    fn shaped_run(
        items: Vec<PreparedExport>,
        o: &ExportOptions,
        permits: &lightcraft_sysmem::HeavyJobs,
        shape: pipeline::Shape,
    ) -> (Vec<serde_json::Value>, Vec<(String, Vec<u8>)>, Vec<usize>) {
        let (mut written, mut calls) = (Vec::new(), Vec::new());
        let to = Destination { dir: "/exports".into(), exact: None };
        let files = run_batch_shaped(
            items,
            o,
            &to,
            &mut |p, b| {
                written.push((p.to_string(), without_icc_dates(b)));
                Ok(())
            },
            &|_| false,
            false,
            &mut |done, _| {
                calls.push(done);
                true
            },
            &crate::enhance::JobCtl::new(),
            permits,
            shape,
        )
        .unwrap();
        (files, written, calls)
    }

    #[test]
    fn pipeline_depth_does_not_change_the_exported_files_or_their_order() {
        let _gpu = crate::tests_gpu::gpu_state(); // no other test switches the GPU off mid-way
        let mut session = crate::Session::with_demo();
        for o in [
            ExportOptions::from_json(&json!({"format": "jpeg", "width": 96, "height": 96, "naming": "{name}"})),
            ExportOptions::from_json(&json!({"format": "tiff", "width": 64, "height": 64, "bitDepth": 16, "naming": "{name}"})),
        ] {
            let permits = wide_open();
            let serial = shaped_run(rendered_batch(&mut session, 7, &o), &o, &permits, pipeline::Shape::serial());
            let deep = shaped_run(rendered_batch(&mut session, 7, &o), &o, &permits, parallel());
            let auto = shaped_run(rendered_batch(&mut session, 7, &o), &o, &permits, pipeline::Shape::auto(7, 4, 8));
            assert_eq!(serial.1.len(), 7, "every photo written (repeated names made unique)");
            assert!(serial.1 == deep.1, "same bytes, same paths, same order at depth 1 and N");
            assert!(serial.1 == auto.1);
            assert_eq!(serial.0, deep.0);
            assert_eq!(serial.2, (0..7).collect::<Vec<_>>(), "progress in input order");
            assert_eq!(deep.2, serial.2);
            assert_eq!(permits.in_flight(), (0, 0), "every permit released");
        }
    }

    /// File-format items whose reads go through `read` (no disk): `{name}.bin` with bytes `name`.
    fn file_items(session: &crate::Session, names: &[&str], read: crate::merge::ByteReader) -> Vec<PreparedExport> {
        let guard = std::sync::Arc::new(session.original_guard());
        names
            .iter()
            .enumerate()
            .map(|(i, name)| PreparedExport {
                photo: lightcraft_catalog::PhotoId(i as u64 + 1),
                file_name: format!("{name}.bin"),
                subfolder: None,
                source_dir: None,
                guard: guard.clone(),
                work: Work::File {
                    path: format!("/input/{name}"),
                    read: Some(read.clone()),
                    packet: "<xmp/>".into(),
                    dng: None,
                    remove_location: false,
                    label: name.to_string(),
                    size: (8, 6),
                },
            })
            .collect()
    }

    #[test]
    fn one_failed_photo_does_not_stop_the_others_and_is_reported_in_place() {
        let session = crate::Session::with_demo();
        let read: crate::merge::ByteReader =
            std::sync::Arc::new(|p: &str| if p.ends_with("bad") { Err(format!("{p}: unreadable")) } else { Ok(p.as_bytes().to_vec()) });
        let o = ExportOptions { format: ExportFormat::Original, ..Default::default() };
        let names = ["a", "b", "bad", "c", "d"];
        for shape in [pipeline::Shape::serial(), parallel()] {
            let (files, written, _) = shaped_run(file_items(&session, &names, read.clone()), &o, &wide_open(), shape);
            assert_eq!(files.len(), 5);
            assert_eq!(files[2]["error"], "/input/bad: unreadable");
            assert_eq!(files[2]["photo"], 3);
            let paths: Vec<_> = written.iter().map(|(p, _)| p.as_str()).collect();
            assert_eq!(
                paths,
                [
                    "/exports/a.bin",
                    "/exports/a.xmp",
                    "/exports/b.bin",
                    "/exports/b.xmp",
                    "/exports/c.bin",
                    "/exports/c.xmp",
                    "/exports/d.bin",
                    "/exports/d.xmp"
                ]
            );
            assert_eq!(written[4].1, b"/input/c");
        }
        // stop_on_error still ends the batch at the failed photo, after the ones before it
        let mut written = Vec::new();
        let err = run_batch_shaped(
            file_items(&session, &names, read),
            &o,
            &Destination { dir: "/exports".into(), exact: None },
            &mut |p, _| {
                written.push(p.to_string());
                Ok(())
            },
            &|_| false,
            true,
            &mut |_, _| true,
            &crate::enhance::JobCtl::new(),
            &wide_open(),
            parallel(),
        )
        .unwrap_err();
        assert_eq!(err, "/input/bad: unreadable");
        assert_eq!(written, ["/exports/a.bin", "/exports/a.xmp", "/exports/b.bin", "/exports/b.xmp"]);
    }

    #[test]
    fn cancelling_mid_batch_leaves_only_whole_files() {
        let _gpu = crate::tests_gpu::gpu_state();
        let mut session = crate::Session::with_demo();
        let dir = std::env::temp_dir().join(format!("lc-export-cancel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let o = ExportOptions::from_json(&json!({"format": "jpeg", "width": 120, "height": 120}));
        let reference = shaped_run(rendered_batch(&mut session, 8, &o), &o, &wide_open(), pipeline::Shape::serial()).1;
        for cancel_by_ctl in [false, true] {
            let _ = std::fs::remove_dir_all(&dir);
            let ctl = crate::enhance::JobCtl::new();
            let mut count = 0;
            let files = run_batch_shaped(
                rendered_batch(&mut session, 8, &o),
                &o,
                &Destination { dir: dir.to_string_lossy().to_string(), exact: None },
                &mut |p, b| {
                    write_file(p, b)?;
                    count += 1;
                    if count == 3 && cancel_by_ctl {
                        ctl.cancel(); // the export dialog's Cancel, while later photos are in flight
                    }
                    Ok(())
                },
                &|p| std::path::Path::new(p).exists(),
                false,
                &mut |done, _| cancel_by_ctl || done < 3,
                &ctl,
                &wide_open(),
                parallel(),
            )
            .unwrap();
            assert_eq!(files.len(), 3, "{files:?}");
            let mut on_disk: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
            on_disk.sort();
            let expected: Vec<_> = reference[..3].iter().map(|(p, _)| p.rsplit('/').next().unwrap().to_string()).collect();
            let mut sorted = expected.clone();
            sorted.sort();
            assert_eq!(on_disk, sorted, "no partial or temporary files");
            for (name, (_, bytes)) in expected.iter().zip(&reference) {
                assert!(without_icc_dates(&std::fs::read(dir.join(name)).unwrap()) == *bytes, "{name} is whole");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn memory_admission_bounds_the_photos_in_flight() {
        let _gpu = crate::tests_gpu::gpu_state();
        let mut session = crate::Session::with_demo();
        let o = ExportOptions::from_json(&json!({"format": "jpeg", "width": 64, "height": 64}));
        // a machine with no headroom at all: one photo at a time, and the batch still completes
        let tight = lightcraft_sysmem::HeavyJobs::new(4, 0, || lightcraft_sysmem::MemoryNow { available: 0, rss: None });
        let peak = std::sync::atomic::AtomicUsize::new(0);
        let to = Destination { dir: "/exports".into(), exact: None };
        let files = run_batch_shaped(
            rendered_batch(&mut session, 6, &o),
            &o,
            &to,
            &mut |_, _| {
                peak.fetch_max(tight.in_flight().0, std::sync::atomic::Ordering::Relaxed);
                Ok(())
            },
            &|_| false,
            false,
            &mut |_, _| true,
            &crate::enhance::JobCtl::new(),
            &tight,
            parallel(),
        )
        .unwrap();
        assert_eq!(files.iter().filter(|f| f.get("path").is_some()).count(), 6);
        assert!(peak.load(std::sync::atomic::Ordering::Relaxed) <= 1, "never two renders at once");
        assert_eq!(tight.in_flight(), (0, 0));
        // estimates grow with the pixels, and a 16-bit output costs more than an 8-bit one
        assert!(render_cost((6000, 4000), (6000, 4000), OutputDepth::U8) > 1 << 30);
        assert!(render_cost((6000, 4000), (2048, 1365), OutputDepth::U8) < render_cost((6000, 4000), (6000, 4000), OutputDepth::U8));
        assert!(render_cost((6000, 4000), (6000, 4000), OutputDepth::U16) > render_cost((6000, 4000), (6000, 4000), OutputDepth::U8));
        assert_eq!(render_cost((0, 0), (0, 0), OutputDepth::U8), UNKNOWN_PIXELS * 48);
    }

    #[test]
    fn pipeline_shape_follows_the_permits_and_cores() {
        use pipeline::Shape;
        assert_eq!(Shape::auto(20, 1, 2), Shape { renderers: 1, encoders: 2, window: 5 }, "2-core laptop: render overlaps encode");
        assert_eq!(Shape::auto(20, 2, 7), Shape { renderers: 2, encoders: 3, window: 7 });
        assert_eq!(Shape::auto(20, 4, 15), Shape { renderers: 4, encoders: 5, window: 11 });
        assert_eq!(Shape::auto(1, 4, 15), Shape { renderers: 1, encoders: 1, window: 4 }, "never more threads than photos");
        assert_eq!(Shape::auto(0, 0, 0), Shape { renderers: 1, encoders: 1, window: 4 });
    }

    fn test_image() -> Rgba8 {
        let mut img = Rgba8::new(96, 64);
        for (i, p) in img.data.iter_mut().enumerate() {
            let (x, y) = (i % 96, i / 96);
            *p = [(x * 2) as u8, (y * 3) as u8, ((x * y) % 255) as u8, 255];
        }
        img
    }

    #[test]
    fn formats_have_magic() {
        let img = test_image();
        for (f, magic) in
            [(ExportFormat::Jpeg, &b"\xFF\xD8"[..]), (ExportFormat::Png, b"\x89PNG"), (ExportFormat::Tiff, b"II"), (ExportFormat::Webp, b"RIFF")]
        {
            let b = encode_image(&img, &ExportOptions { format: f, ..Default::default() }).unwrap();
            assert!(b.starts_with(magic), "{f:?}");
        }
    }

    #[test]
    fn size_limit_respected() {
        let img = test_image();
        let full = encode_image(&img, &ExportOptions { quality: 100, ..Default::default() }).unwrap();
        let kb = (full.len() / 1024 / 2).max(2) as u32;
        let b = encode_image(&img, &ExportOptions { quality: 100, limit_kb: Some(kb), ..Default::default() }).unwrap();
        assert!(b.len() <= kb as usize * 1024);
    }

    #[test]
    fn sharpen_increases_edge_contrast() {
        let mut img = Rgba8::new(8, 8);
        for (i, p) in img.data.iter_mut().enumerate() {
            let v = if i % 8 < 4 { 80 } else { 170 };
            *p = [v, v, v, 255];
        }
        output_sharpen(&mut img, SharpenFor::Matte, SharpenAmount::High);
        assert!(img.data[3][0] < 80 && img.data[4][0] > 170);
    }

    #[test]
    fn metadata_policies() {
        use lightcraft_catalog::{Photo, PhotoId, Source};
        let mut p = Photo::new(PhotoId(1), Source::Demo { scene: 0 }, "a.jpg", "jpeg", 10, 10, "2026-09-30T00:00:00");
        p.meta.camera = "Synthetic X2".into();
        p.meta.copyright = "(c) Me".into();
        p.meta.copyright_status = lightcraft_catalog::CopyrightStatus::Copyrighted;
        p.meta.usage_terms = "Editorial use only".into();
        p.meta.copyright_url = "https://example.com/rights".into();
        p.meta.shutter = "1/250".into();
        p.meta.gps = Some((43.0, -110.0));
        let all = export_metadata(&p, &ExportOptions::default()).unwrap();
        assert_eq!(all.model.as_deref(), Some("Synthetic X2"));
        assert!((all.exposure_time.unwrap() - 0.004).abs() < 1e-9);
        assert!(all.gps.is_some());
        let o = ExportOptions { metadata: MetadataPolicy::AllExceptCamera, remove_location: true, ..Default::default() };
        let m = export_metadata(&p, &o).unwrap();
        assert!(m.model.is_none() && m.gps.is_none() && m.copyright.is_some());
        let c = export_metadata(&p, &ExportOptions { metadata: MetadataPolicy::Copyright, ..Default::default() }).unwrap();
        assert!(c.model.is_none() && c.gps.is_none() && c.copyright.as_deref() == Some("(c) Me"));
        // "copyright only" keeps all the copyright info: status, usage terms, info URL
        assert_eq!(
            (c.copyright_marked, c.usage_terms.as_deref(), c.copyright_url.as_deref()),
            (Some(true), Some("Editorial use only"), Some("https://example.com/rights"))
        );
        assert!(export_metadata(&p, &ExportOptions { metadata: MetadataPolicy::None, ..Default::default() }).is_none());
        // embedded and readable back from the JPEG
        let jpg = encode_with_metadata(&test_image(), &ExportOptions::default(), Some(&all)).unwrap();
        let back = lightcraft_meta::extract(&jpg);
        assert_eq!(back.model.as_deref(), Some("Synthetic X2"));
        assert_eq!(back.copyright.as_deref(), Some("(c) Me"));
        assert_eq!((back.copyright_marked, back.usage_terms.as_deref()), (Some(true), Some("Editorial use only")));
        assert_eq!(back.copyright_url.as_deref(), Some("https://example.com/rights"));
        assert!(back.gps.is_some());
    }

    #[test]
    fn watermark_draws_in_the_anchored_corner_only() {
        let mut img = Rgba8::new(400, 300);
        for p in img.data.iter_mut() {
            *p = [0, 0, 0, 255];
        }
        let o = ExportOptions::from_json(&serde_json::json!({"watermark": {"text": "LightCraft", "size": 0.08, "opacity": 1.0, "shadow": false}}));
        let wm = o.watermark.clone().unwrap();
        draw_watermark(&mut img, &wm);
        let lit = |x0: usize, x1: usize, y0: usize, y1: usize| {
            (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter(|&(x, y)| img.get(x, y)[0] > 128).count()
        };
        assert!(lit(200, 400, 225, 300) > 100, "bottom-right has text");
        assert!(lit(0, 200, 0, 150) == 0, "top-left untouched");
        // string shorthand
        assert_eq!(ExportOptions::from_json(&serde_json::json!({"watermark": "© Me"})).watermark.unwrap().text, "© Me");
        assert!(ExportOptions::from_json(&serde_json::json!({"watermark": ""})).watermark.is_none());
    }

    fn vertical_japanese() -> Watermark {
        let options =
            ExportOptions::from_json(&json!({"watermark": {"text": "日本語", "vertical": true, "size": 0.1, "shadow": false, "opacity": 1.0}}));
        options.watermark.unwrap()
    }

    #[test]
    fn japanese_watermark_options_and_legacy_defaults() {
        let old = ExportOptions::from_json(&json!({"watermark": {"text": "日本語"}}));
        assert!(!old.watermark.unwrap().vertical);
        let wm = vertical_japanese();
        assert!(wm.vertical);
        let round: Watermark = serde_json::from_value(serde_json::to_value(&wm).unwrap()).unwrap();
        assert!(round.vertical);
    }

    /// Built without craft-fonts, a Japanese watermark still lays out and draws (Inter's
    /// missing-glyph boxes), and Latin text is unaffected.
    #[test]
    fn watermarks_work_without_craft_fonts() {
        assert_eq!(watermark_fonts(&[]).len(), 1, "Inter only");
        for wm in [vertical_japanese(), Watermark { text: "LightCraft 日本語".into(), ..Watermark::default() }] {
            let mut covered = 0usize;
            watermark_coverage(400, 300, &wm, &[], |_, _, k, _| covered += usize::from(k > 0.0));
            assert!(covered > 0, "{:?} draws something", wm.text);
        }
    }

    #[test]
    fn japanese_watermarks_support_vertical_columns() {
        use ab_glyph::Font;
        let fonts = watermark_fonts(crate::fonts::CRAFT_FONTS);
        if fonts.len() < 2 {
            eprintln!("skipped: built without CRAFT_FONTS_DIR, so there is no Japanese watermark face");
            return;
        }
        for c in "日本語の文字".chars() {
            assert!(fonts[1..].iter().any(|f| f.glyph_id(c).0 != 0), "a craft-fonts face has {c}");
        }
        let wm = vertical_japanese();
        let mut img = Rgba8::new(400, 300);
        img.data.fill([0, 0, 0, 255]);
        draw_watermark(&mut img, &wm);
        let lit: Vec<_> = (0..300).flat_map(|y| (0..400).map(move |x| (x, y))).filter(|&(x, y)| img.get(x, y)[0] > 64).collect();
        assert!(lit.len() > 100);
        let w = lit.iter().map(|p| p.0).max().unwrap() - lit.iter().map(|p| p.0).min().unwrap();
        let h = lit.iter().map(|p| p.1).max().unwrap() - lit.iter().map(|p| p.1).min().unwrap();
        assert!(h > w * 2, "vertical Japanese must extend down the column");
        let two = Watermark { text: "日\n本".into(), anchor: Anchor::TopLeft, inset: 0.0, ..wm };
        img.data.fill([0, 0, 0, 255]);
        draw_watermark(&mut img, &two);
        for (text, dx) in [("本", 0), ("日", 30)] {
            let mut single = Rgba8::new(400, 300);
            single.data.fill([0, 0, 0, 255]);
            draw_watermark(&mut single, &Watermark { text: text.into(), ..two.clone() });
            for y in 0..30 {
                for x in 0..30 {
                    assert_eq!(img.get(x + dx, y), single.get(x, y), "newlines move columns left");
                }
            }
        }
    }

    #[test]
    fn graphic_watermark_is_placed_scaled_and_blended() {
        let dir = std::env::temp_dir().join(format!("lc-wm-logo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let logo_path = dir.join("logo.png");
        // 40×20: left half opaque red, right half fully transparent
        let logo = Rgba8::from_fn(40, 20, |x, _| if x < 20 { [255, 0, 0, 255] } else { [0, 255, 0, 0] });
        let png = lightcraft_codecs::encode_png(&EncodeImage::rgba8(&logo), &EncodeMeta::default()).unwrap();
        std::fs::write(&logo_path, png).unwrap();
        let mut img = Rgba8::new(400, 300);
        img.data.iter_mut().for_each(|p| *p = [0, 0, 0, 255]);
        let wm = Watermark { image: logo_path.to_string_lossy().into(), image_width: 0.25, opacity: 1.0, inset: 0.0, ..Default::default() };
        draw_watermark(&mut img, &wm);
        // 100 px wide × 50 px high in the bottom-right corner
        assert!(img.get(310, 280)[0] > 200, "opaque half: {:?}", img.get(310, 280));
        assert_eq!(img.get(390, 280), [0, 0, 0, 255], "transparent half leaves the photo");
        assert_eq!(img.get(310, 240), [0, 0, 0, 255], "above the logo");
        assert_eq!(img.get(10, 10), [0, 0, 0, 255]);
        // an image-only watermark survives the params (no text needed)
        let o = ExportOptions::from_json(&json!({"watermark": {"image": logo_path.to_string_lossy(), "imageWidth": 0.1}}));
        assert_eq!(o.watermark.as_ref().map(|w| w.image_width), Some(0.1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn naming_and_params() {
        let o = ExportOptions::from_json(&serde_json::json!({"format": "jpg", "quality": 150, "longEdge": 2048, "naming": "{name}-{seq}"}));
        assert_eq!(o.format, ExportFormat::Jpeg);
        assert_eq!(o.quality, 100);
        assert_eq!(o.resize, Some(Resize { dont_enlarge: true, height: 0, ..Resize::long_edge(2048) }));
        assert_eq!(o.file_name_for(&named("IMG/1.png"), 7), "IMG_1-007.jpg");
        assert_eq!(o.ppi, 240);
        let orig = ExportOptions { format: ExportFormat::Original, naming: "{name}-{seq}".into(), ..Default::default() };
        assert_eq!(orig.file_name_for(&named("DSC_1.NEF"), 2), "DSC_1-002.NEF");
        assert_eq!(orig.file_name_for(&named("noext"), 1), "noext-001");
        assert_eq!(ExportOptions { format: ExportFormat::Dng, ..Default::default() }.file_name_for(&named("a.cr2"), 1), "a.dng");
    }

    #[test]
    fn enhance_samples_keep_source_profile_float_encoding() {
        let profile = icc::write_matrix_trc(&lightcraft_color::DISPLAY_P3, &lightcraft_codecs::Trc::Gamma(2.0));
        let Some(icc::IccInfo { kind: icc::IccKind::MatrixTrc { to_xyz_d50, trc }, .. }) = icc::parse(&profile) else { panic!("matrix profile") };
        let source = lightcraft_codecs::SourceSpace { named: None, to_xyz_d50, trc: Some(trc), origin: lightcraft_codecs::SpaceOrigin::IccMatrixTrc };
        let options = ExportOptions { source_encoding: Some(Box::new(SourceEncoding { space: source, profile })), ..Default::default() };
        let mut image = Rgba8::from_fn(1, 1, |_, _| [50, 120, 200, 255]);
        let deep = enhanced_samples(&mut image, &options).unwrap();
        assert_eq!(deep.space, OutputSpace::Rec2020);
        let DeepSamples::F32(values) = deep.samples else { panic!("source encoding requires linear floats") };
        let expected = lightcraft_color::SRGB
            .to_space(&lightcraft_color::REC2020)
            .apply([50, 120, 200].map(|v| lightcraft_color::transfer::decode_srgb8(v) as f64));
        for (actual, expected) in values.iter().zip(expected) {
            assert!((*actual as f64 - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn source_profile_encoding_converts_pixels_and_stays_out_of_presets() {
        let profile = icc::write_matrix_trc(&lightcraft_color::DISPLAY_P3, &lightcraft_codecs::Trc::Gamma(2.0));
        let Some(icc::IccInfo { kind: icc::IccKind::MatrixTrc { to_xyz_d50, trc }, .. }) = icc::parse(&profile) else { panic!("matrix profile") };
        let source = lightcraft_codecs::SourceSpace { named: None, to_xyz_d50, trc: Some(trc), origin: lightcraft_codecs::SpaceOrigin::IccMatrixTrc };
        let linear = [0.2, 0.4, 0.1];
        let working = source.to_working().apply(linear).map(|v| v as f32).to_vec();
        let image = Rgba8::new(1, 1);
        let render = lightcraft_pipeline::Rendered {
            histogram: lightcraft_raster::Histogram::of_srgb8(&image),
            image,
            deep: Some(DeepImage { width: 1, height: 1, space: OutputSpace::Rec2020, samples: DeepSamples::F32(working) }),
        };
        let options = ExportOptions {
            format: ExportFormat::Png,
            bit_depth: Some(16),
            source_encoding: Some(Box::new(SourceEncoding { space: source, profile: profile.clone() })),
            ..Default::default()
        };
        let bytes = encode_rendered(&render, &options, None).unwrap();
        let decoded = lightcraft_codecs::decode(&bytes, Default::default()).unwrap();
        assert_eq!(decoded.icc.as_deref(), Some(profile.as_slice()));
        for (value, expected) in decoded.image.data[0].iter().zip(linear) {
            assert!((*value as f64 - expected).abs() < 5e-5, "{value} != {expected}");
        }
        let params = serde_json::to_value(&options).unwrap();
        assert!(params.get("sourceEncoding").is_none());
        assert!(params.get("sourceMetadata").is_none());
        let loaded: ExportOptions = serde_json::from_value(params).unwrap();
        assert!(loaded.source_encoding.is_none());
        let old: ExportOptions = serde_json::from_value(json!({"format": "jpeg", "quality": 80})).unwrap();
        assert!(!old.same_folder);
        for (format, bit_depth, expected_depth) in
            [(ExportFormat::Tiff, None, 16), (ExportFormat::Png, Some(32), 16), (ExportFormat::Jpeg, Some(16), 8), (ExportFormat::Webp, Some(32), 8)]
        {
            let options = ExportOptions { format, bit_depth, ..options.clone() };
            let bytes = encode_rendered(&render, &options, None).unwrap();
            let decoded = lightcraft_codecs::decode(&bytes, Default::default()).unwrap();
            assert_eq!(decoded.bit_depth, expected_depth, "{format:?} {bit_depth:?}");
        }
    }

    fn named(file_name: &str) -> lightcraft_catalog::Photo {
        use lightcraft_catalog::{Photo, PhotoId, Source};
        Photo::new(PhotoId(1), Source::Demo { scene: 0 }, file_name, "", 1, 1, "2026-01-01T00:00:00")
    }

    /// Export naming takes the same tokens as Rename Photos and import renaming.
    #[test]
    fn naming_uses_the_rename_tokens() {
        let mut p = named("DSC_0815.NEF");
        p.captured = Some("2026-09-30T14:05:09".into());
        p.meta.camera = "Model X/2".into();
        p.meta.title = "Harbour".into();
        p.rating = 5;
        let o = |naming: &str| ExportOptions { naming: naming.into(), start_number: 9, ..Default::default() };
        assert_eq!(o("{date:%Y-%m-%d}_{title}_{seq:2}").file_name_for(&p, 1), "2026-09-30_Harbour_09.jpg");
        assert_eq!(o("{camera}-{num}-{rating}").file_name_for(&p, 1), "Model X_2-0815-5.jpg", "path separators become _");
        assert_eq!(o("{name}-{seq}").file_name_for(&p, 3), "DSC_0815-011.jpg", "a bare {{seq}} keeps its 3 digits");
        assert_eq!(o("{ext}_{name}").file_name_for(&p, 1), "NEF_DSC_0815.jpg");
        // nothing left after expanding: the original name
        p.meta.title.clear();
        assert_eq!(o("{title}").file_name_for(&p, 1), "DSC_0815.jpg");
    }

    #[test]
    fn params_round_trip() {
        let o = ExportOptions {
            format: ExportFormat::Tiff,
            quality: 70,
            resize: Some(Resize { mode: ResizeMode::Megapixels, value: 12.5, height: 0, dont_enlarge: false }),
            ppi: 300,
            limit_kb: Some(500),
            sharpen: SharpenFor::Glossy,
            naming: "{date}-{name}-{seq}".into(),
            start_number: 42,
            subfolder: "Client".into(),
            conflict: Conflict::Skip,
            tiff_compression: TiffCompression::Lzw,
            metadata: MetadataPolicy::Copyright,
            remove_location: true,
            watermark: Some(Watermark { text: "© LC".into(), ..Default::default() }),
            color_space: OutputSpace::ProPhoto,
            bit_depth: Some(16),
            ..Default::default()
        };
        assert_eq!(ExportOptions::from_json(&o.to_json()), o);
        let full = ExportOptions::default();
        assert_eq!(ExportOptions::from_json(&full.to_json()), full);
        assert!(ExportOptions::has_size_param(&full.to_json()), "full size is explicit");
        let mut p = named("IMG.png");
        p.captured = Some("2026-09-30T10:00:00".into());
        assert_eq!(o.file_name_for(&p, 2), "20260930-IMG-043.tif");
    }

    #[test]
    fn resize_modes() {
        let size = |p: serde_json::Value, w: f64, h: f64| ExportOptions::from_json(&p).resize.map(|r| r.apply(w, h));
        // a 6000 × 4000 landscape and a 4000 × 6000 portrait
        assert_eq!(size(json!({"longEdge": 1500}), 6000.0, 4000.0), Some((1500, 1000)));
        assert_eq!(size(json!({"longEdge": 1500}), 4000.0, 6000.0), Some((1000, 1500)));
        assert_eq!(size(json!({"shortEdge": 1080}), 6000.0, 4000.0), Some((1620, 1080)));
        assert_eq!(size(json!({"width": 3000}), 4000.0, 6000.0), Some((3000, 4500)));
        assert_eq!(size(json!({"height": 1000}), 6000.0, 4000.0), Some((1500, 1000)));
        // W × H fits either orientation: the long edge gets the larger number
        assert_eq!(size(json!({"width": 1000, "height": 800}), 6000.0, 4000.0), Some((1000, 667)));
        assert_eq!(size(json!({"width": 1000, "height": 800}), 4000.0, 6000.0), Some((667, 1000)));
        assert_eq!(size(json!({"width": 1000, "height": 500}), 6000.0, 4000.0), Some((750, 500)));
        let (w, h) = size(json!({"megapixels": 6}), 6000.0, 4000.0).unwrap();
        assert!(((w * h) as f64 - 6e6).abs() < 6e6 * 0.002, "{w}×{h}");
        assert_eq!(size(json!({"percent": 25}), 6000.0, 4000.0), Some((1500, 1000)));
        // don't enlarge (default) vs enlarge
        assert_eq!(size(json!({"longEdge": 8000}), 6000.0, 4000.0), Some((6000, 4000)));
        assert_eq!(size(json!({"longEdge": 9000, "dontEnlarge": false}), 6000.0, 4000.0), Some((9000, 6000)));
        // full size / absent
        assert_eq!(size(json!({"longEdge": 0}), 6000.0, 4000.0), None);
        assert_eq!(size(json!({}), 6000.0, 4000.0), None);
        assert!(ExportOptions::has_size_param(&json!({"longEdge": 0})) && !ExportOptions::has_size_param(&json!({"quality": 3})));
        // the `resize` object form round-trips (what the dialog and prefs store)
        let r = Resize { mode: ResizeMode::ShortEdge, value: 720.0, height: 0, dont_enlarge: false };
        let back = ExportOptions::from_json(&ExportOptions::resize_json(Some(&r))).resize;
        assert_eq!(back, Some(r));
        assert_eq!(ExportOptions::from_json(&ExportOptions::resize_json(None)).resize, None);
        assert_eq!(ExportOptions::from_json(&json!({"ppi": 300})).ppi, 300);
        // never larger than the encoders' limit, never empty
        assert_eq!(size(json!({"percent": 10000, "dontEnlarge": false}), 6000.0, 4000.0), Some((65535, 43690)));
        assert_eq!(size(json!({"width": 1}), 6000.0, 40.0).map(|s| s.1), Some(1));
    }

    /// A flat field of a saturated green inside Display P3 but outside sRGB, rendered into `space`.
    fn p3_green(space: OutputSpace) -> Rgba8 {
        use lightcraft_color::{DISPLAY_P3, REC2020};
        let c = DISPLAY_P3.to_space(&REC2020).apply_f32([0.04, 0.45, 0.04]);
        let src = lightcraft_raster::Rgb32f::filled(16, 16, c);
        let req = lightcraft_pipeline::RenderRequest { space, ..lightcraft_pipeline::RenderRequest::fit(16, 16) };
        lightcraft_pipeline::render(&src, &Default::default(), &Default::default(), &req).image
    }

    /// Decode `bytes` and return the centre pixel in linear sRGB primaries (unclamped), plus the
    /// recognised space of the embedded profile.
    fn decoded_in_srgb(bytes: &[u8]) -> ([f32; 3], Option<NamedSpace>) {
        let d = lightcraft_codecs::decode(bytes, Default::default()).expect("decodes");
        let m = d.space.to_space(&lightcraft_color::SRGB);
        (m.apply_f32(d.image.get(8, 8)), d.space.named)
    }

    #[test]
    fn p3_colour_survives_a_p3_export_and_is_clipped_in_srgb() {
        let o = |space| ExportOptions { format: ExportFormat::Png, color_space: space, ..Default::default() };
        let (p3, named) = decoded_in_srgb(&encode_image(&p3_green(OutputSpace::DisplayP3), &o(OutputSpace::DisplayP3)).unwrap());
        assert_eq!(named, Some(NamedSpace::DisplayP3));
        assert!(p3[0] < -0.03 || p3[2] < -0.03, "outside sRGB after a P3 round trip: {p3:?}");
        let (s, named) = decoded_in_srgb(&encode_image(&p3_green(OutputSpace::Srgb), &o(OutputSpace::Srgb)).unwrap());
        assert_eq!(named, Some(NamedSpace::Srgb));
        assert!(s.iter().all(|v| *v > -0.002), "{s:?}");
        // the green itself is about the same brightness either way
        assert!((p3[1] - s[1]).abs() < 0.15, "{p3:?} {s:?}");
    }

    #[test]
    fn every_space_embeds_its_own_profile_and_round_trips() {
        let grey = {
            let src = lightcraft_raster::Rgb32f::filled(16, 16, [0.18; 3]);
            let base = lightcraft_pipeline::RenderRequest::fit(16, 16);
            move |space| {
                lightcraft_pipeline::render(&src, &Default::default(), &Default::default(), &lightcraft_pipeline::RenderRequest { space, ..base })
                    .image
            }
        };
        let reference =
            decoded_in_srgb(&encode_image(&grey(OutputSpace::Srgb), &ExportOptions { format: ExportFormat::Tiff, ..Default::default() }).unwrap()).0;
        for space in OutputSpace::ALL {
            for format in [ExportFormat::Jpeg, ExportFormat::Png, ExportFormat::Tiff, ExportFormat::Webp] {
                let o = ExportOptions { format, color_space: space, ..Default::default() };
                let bytes = encode_image(&grey(space), &o).unwrap();
                let (px, named) = decoded_in_srgb(&bytes);
                assert_eq!(named, Some(named_space(space)), "{space:?} {format:?}");
                // a neutral grey decodes to the same linear value whatever the output space
                for c in 0..3 {
                    assert!((px[c] - reference[c]).abs() < 0.01, "{space:?} {format:?}: {px:?} vs {reference:?}");
                }
            }
            let icc = icc::write_named(named_space(space));
            let info = icc::parse(&icc).expect("our profile parses");
            assert_eq!(info.named, Some(named_space(space)));
            assert!(info.description.as_deref().is_some_and(|d| !d.is_empty()));
        }
        let o = ExportOptions { format: ExportFormat::Avif, color_space: OutputSpace::ProPhoto, ..Default::default() };
        assert_eq!(o.effective_space(), OutputSpace::Srgb);
        let o = ExportOptions::from_json(&serde_json::json!({"colorSpace": "displayP3"}));
        assert_eq!(o.color_space, OutputSpace::DisplayP3);
        assert_eq!(ExportOptions::from_json(&serde_json::json!({"colorSpace": "Adobe RGB (1998) compatible"})).color_space, OutputSpace::AdobeRgb);
    }

    #[test]
    fn srgb_watermark_colour_is_converted() {
        assert_eq!(srgb8_in(OutputSpace::Srgb, [10, 200, 30]), [10, 200, 30]);
        for s in OutputSpace::ALL {
            assert_eq!(srgb8_in(s, [255, 255, 255]), [255, 255, 255], "{s:?}");
            assert_eq!(srgb8_in(s, [0, 0, 0]), [0, 0, 0], "{s:?}");
        }
        let p3 = srgb8_in(OutputSpace::DisplayP3, [255, 0, 0]);
        assert!(p3[0] < 255 && p3[1] > 0, "{p3:?}");
    }

    /// A shallow horizontal grey ramp (few 8-bit levels), rendered at `depth`.
    fn ramp(depth: OutputDepth, space: OutputSpace) -> lightcraft_pipeline::Rendered {
        let src = lightcraft_raster::Rgb32f::from_fn(1024, 4, |x, _| [0.10 + 0.03 * x as f32 / 1023.0; 3]);
        let req = lightcraft_pipeline::RenderRequest { depth, space, ..lightcraft_pipeline::RenderRequest::fit(1024, 4) };
        lightcraft_pipeline::render(&src, &Default::default(), &Default::default(), &req)
    }

    fn distinct_levels(bytes: &[u8]) -> (usize, lightcraft_codecs::Decoded) {
        let d = lightcraft_codecs::decode(bytes, Default::default()).expect("decodes");
        let mut v: Vec<u32> = (0..d.image.width).map(|x| d.image.get(x, 1)[1].to_bits()).collect();
        // Dither alternates adjacent levels: count distinct values, not transitions.
        v.sort_unstable();
        v.dedup();
        (v.len(), d)
    }

    #[test]
    fn sixteen_bit_tiff_and_png_have_no_banding() {
        let tiff = |bit_depth| ExportOptions { format: ExportFormat::Tiff, bit_depth, ..Default::default() };
        assert_eq!(tiff(None).effective_depth(), OutputDepth::U16, "TIFF defaults to 16-bit");
        let r16 = ramp(OutputDepth::U16, OutputSpace::Srgb);
        assert!(matches!(r16.deep.as_ref().unwrap().samples, DeepSamples::U16(_)));
        let (levels16, d16) = distinct_levels(&encode_rendered(&r16, &tiff(None), None).unwrap());
        let r8 = ramp(OutputDepth::U8, OutputSpace::Srgb);
        let (levels8, _) = distinct_levels(&encode_rendered(&r8, &tiff(Some(8)), None).unwrap());
        eprintln!("gradient levels: 16-bit TIFF {levels16}, 8-bit TIFF {levels8}");
        assert!(levels8 < 40, "{levels8}");
        assert!(levels16 > 10 * levels8, "16-bit: {levels16} levels vs 8-bit: {levels8}");
        assert_eq!(d16.space.named, Some(NamedSpace::Srgb));
        // the 8-bit preview of a deep render matches the 8-bit render
        for (a, b) in r16.image.data.iter().zip(&r8.image.data) {
            assert!(a[1].abs_diff(b[1]) <= 1, "{a:?} {b:?}");
        }
        let png = ExportOptions { format: ExportFormat::Png, bit_depth: Some(16), color_space: OutputSpace::DisplayP3, ..Default::default() };
        let rp = ramp(OutputDepth::U16, OutputSpace::DisplayP3);
        let bytes = encode_rendered(&rp, &png, None).unwrap();
        assert_eq!(bytes[24], 16, "PNG IHDR bit depth");
        let (levels, d) = distinct_levels(&bytes);
        assert!(levels > 10 * levels8, "{levels}");
        assert_eq!(d.space.named, Some(NamedSpace::DisplayP3));
        // sharpening and watermarking work on deep images too
        let o = ExportOptions {
            sharpen: SharpenFor::Matte,
            watermark: Some(Watermark { text: "LC".into(), size: 0.5, ..Default::default() }),
            ..tiff(None)
        };
        assert!(encode_rendered(&r16, &o, None).is_ok());
    }

    #[test]
    fn float_tiff_is_linear_with_a_linear_profile() {
        let o = ExportOptions { format: ExportFormat::Tiff, bit_depth: Some(32), color_space: OutputSpace::ProPhoto, ..Default::default() };
        assert_eq!(o.effective_depth(), OutputDepth::F32Linear);
        let rf = ramp(OutputDepth::F32Linear, OutputSpace::ProPhoto);
        let (levels, df) = distinct_levels(&encode_rendered(&rf, &o, None).unwrap());
        assert!(levels > 500, "{levels}");
        assert_eq!(df.space.named, Some(NamedSpace::ProPhoto));
        assert!(df.space.trc.as_ref().is_some_and(|t| t[0].is_linear()), "{:?}", df.space.trc);
        // decodes to the same light as the 16-bit gamma-encoded export
        let o16 = ExportOptions { bit_depth: Some(16), ..o.clone() };
        let (_, d16) = distinct_levels(&encode_rendered(&ramp(OutputDepth::U16, OutputSpace::ProPhoto), &o16, None).unwrap());
        for x in [0, 300, 700, 1023] {
            let (a, b) = (df.image.get(x, 1)[1], d16.image.get(x, 1)[1]);
            assert!((a - b).abs() < 2e-4, "{x}: {a} vs {b}");
        }
    }

    #[test]
    fn bit_depth_options_per_format() {
        let d = |format, bit_depth| ExportOptions { format, bit_depth, ..Default::default() }.effective_depth();
        assert_eq!(d(ExportFormat::Jpeg, Some(16)), OutputDepth::U8);
        assert_eq!(d(ExportFormat::Webp, None), OutputDepth::U8);
        assert_eq!(d(ExportFormat::Png, None), OutputDepth::U8);
        assert_eq!(d(ExportFormat::Png, Some(16)), OutputDepth::U16);
        assert_eq!(d(ExportFormat::Avif, Some(10)), OutputDepth::U16);
        assert_eq!(ExportOptions::from_json(&serde_json::json!({"bitDepth": 16})).bit_depth, Some(16));
        assert_eq!(ExportOptions::from_json(&serde_json::json!({"bitDepth": 12})).bit_depth, None);
        for f in [ExportFormat::Jpeg, ExportFormat::Png, ExportFormat::Tiff, ExportFormat::Webp, ExportFormat::Avif] {
            let first = ExportOptions::bit_depths(f)[0].0;
            let def = d(f, None);
            assert_eq!(d(f, Some(first)), def, "{f:?}: the first choice is the default");
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn avif_ten_bit() {
        let o = ExportOptions { format: ExportFormat::Avif, bit_depth: Some(10), quality: 60, ..Default::default() };
        let r = ramp(OutputDepth::U16, OutputSpace::Srgb);
        match encode_rendered(&r, &o, None) {
            Ok(b) => assert_eq!(&b[4..8], b"ftyp"),
            Err(e) => assert!(e.contains("not available"), "{e}"),
        }
    }
}

/// The photo whose sidecar packet an Original/DNG export writes: with people names as keywords when
/// asked, and without any location (GPS, sublocation, city, state, country) under "Remove location
/// info" (upstream LightCraft db7e378). Borrowed when nothing changes.
fn photo_for_sidecar(
    photo: &lightcraft_catalog::Photo,
    include_people_names: bool,
    remove_location: bool,
) -> std::borrow::Cow<'_, lightcraft_catalog::Photo> {
    let mut photo = std::borrow::Cow::Borrowed(photo);
    if include_people_names {
        let copy = photo.to_mut();
        add_people_keywords(&mut copy.meta.keywords, &copy.meta.regions);
    }
    let m = &photo.meta;
    if remove_location && (m.gps.is_some() || [&m.location, &m.city, &m.state, &m.country].iter().any(|s| !s.trim().is_empty())) {
        let m = &mut photo.to_mut().meta;
        m.gps = None;
        m.location.clear();
        m.city.clear();
        m.state.clear();
        m.country.clear();
    }
    photo
}

#[cfg(test)]
#[test]
fn ds2_original_export_borrows_photo_unless_people_keywords_are_requested() {
    let photo = lightcraft_catalog::Photo::new(
        lightcraft_catalog::PhotoId(1),
        lightcraft_catalog::Source::Demo { scene: 1 },
        "fixture.jpg",
        "JPEG",
        8,
        8,
        "2026-10-10",
    );
    let borrowed = photo_for_sidecar(&photo, false, false);
    assert!(matches!(borrowed, std::borrow::Cow::Borrowed(_)));
    assert!(std::ptr::eq(borrowed.as_ref(), &photo));
    let augmented = photo_for_sidecar(&photo, true, false);
    assert!(matches!(augmented, std::borrow::Cow::Owned(_)));
    assert_eq!(augmented.meta.keywords, photo.meta.keywords);
}
