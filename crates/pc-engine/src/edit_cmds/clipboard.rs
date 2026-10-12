//! Profile-carrying clipboard pixels. Conversion happens before editing history is changed.
use crate::{EngineError, Result, color_cmds};
use photocraft_cms::{Intent, Profile, Transform};
use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_doc::Document;
use photocraft_geom::Rect;
use photocraft_raster::Surface;
use std::sync::Arc;

/// Pixels, their actual colour interpretation, and their original document position.
#[derive(Clone, Debug)]
pub struct Clip {
    pub surface: Surface,
    pub bounds: Rect,
    pub profile: Arc<Profile>,
}

impl Clip {
    /// Untagged pixels use the built-in working space for their colour model.
    /// OS RGBA8 is therefore explicitly interpreted as sRGB.
    pub fn new(surface: Surface, bounds: Rect) -> Self {
        let profile = color_cmds::working_profile(surface.format().mode);
        Self { surface, bounds, profile }
    }

    /// Flatten without reducing precision or losing the document's ICC interpretation.
    /// The compositor's RGB representation is converted back to document space by bands.
    pub fn from_document(doc: &Document) -> Result<Self> {
        Self::from_document_checked(doc, || Ok(()))
    }

    /// Copied files may describe huge sparse canvases. Admit the padded output
    /// allocation before rendering; check cancellation before and between bands.
    pub fn from_document_checked(doc: &Document, mut check: impl FnMut() -> Result<()>) -> Result<Self> {
        check()?;
        let tile = photocraft_geom::TILE_SIZE as u64;
        let format = PixelFormat::new(doc.mode, doc.depth, true);
        let tiles_x = u64::from(doc.size.width).div_ceil(tile);
        let tiles_y = u64::from(doc.size.height).div_ceil(tile);
        let bytes = tiles_x.checked_mul(tiles_y).and_then(|n| n.checked_mul(tile * tile)).and_then(|n| n.checked_mul(format.bytes_per_pixel() as u64));
        if bytes.is_none_or(|n| n > photocraft_format::read::MAX_FILE_BYTES) {
            return Err(EngineError::Other("Clipboard image exceeds the decoded memory limit; resize or export a smaller image first".into()));
        }
        let profile = color_cmds::document_profile(doc);
        let source = color_cmds::composite_profile(doc);
        let target = PixelFormat::new(doc.mode, doc.depth, true);
        let transform = Transform::new(&source, &profile, Intent::RelativeColorimetric, true).map_err(cms_error)?;
        if transform.inputs() != 3 || transform.outputs() != target.mode.color_channels() {
            return Err(EngineError::Other("Clipboard colour mode is unsupported".into()));
        }
        let mut surface = Surface::new(target);
        let mut data = Vec::new();
        photocraft_compose::render_bands(doc, doc.bounds(), 128, |band| -> Result<()> {
            check()?;
            data.resize(band.px.len() * target.channels(), 0.0);
            transform.convert_f32(band.px.as_flattened(), 4, &mut data, target.channels(), true);
            surface.write_region(band.rect, &data);
            check()
        })?;
        surface.prune();
        Ok(Self { surface, bounds: doc.bounds(), profile })
    }

    pub fn for_document(&self, doc: &Document, preserve_numbers: bool, intent: Intent, bpc: bool) -> Result<Surface> {
        let target = PixelFormat::new(doc.mode, doc.depth, true);
        if preserve_numbers {
            if self.surface.format().mode != target.mode {
                return Err(EngineError::Other("Preserving colour numbers requires the same colour mode; choose Convert instead".into()));
            }
            return Ok(if self.surface.format() == target { self.surface.clone() } else { self.surface.convert(target) });
        }
        self.converted(&color_cmds::document_profile(doc), target, intent, bpc)
    }

    /// Convert colour channels, retain alpha, then quantize to the destination depth.
    pub fn converted(&self, profile: &Profile, target: PixelFormat, intent: Intent, bpc: bool) -> Result<Surface> {
        let source = self.surface.format();
        if self.profile.color_space != color_cmds::mode_space(source.mode) || profile.color_space != color_cmds::mode_space(target.mode) || !target.alpha {
            return Err(EngineError::Other("Clipboard profile does not match its pixel format".into()));
        }
        if source.mode == target.mode && self.profile.same_colors(profile) {
            return Ok(if self.surface.format() == target { self.surface.clone() } else { self.surface.convert(target) });
        }
        let transform = Transform::new(&self.profile, profile, intent, bpc).map_err(cms_error)?;
        if transform.inputs() != source.mode.color_channels() || transform.outputs() != target.mode.color_channels() {
            return Err(EngineError::Other("Clipboard colour conversion is unsupported".into()));
        }
        // Transform a tile at a time in float and quantize only at the destination.
        // Avoid full-image float intermediates when converting large 8/16-bit copies.
        let alpha_format = PixelFormat::new(source.mode, source.sample, true);
        let source_surface = if source.alpha { self.surface.clone() } else { self.surface.convert(alpha_format) };
        let mut pixel = vec![0.0; target.channels()];
        transform.convert_f32(&source_surface.default_pixel(), alpha_format.channels(), &mut pixel, target.channels(), true);
        let mut output = Surface::with_default(target, &pixel);
        let mut converted = Vec::new();
        for (coord, _) in source_surface.tiles() {
            let rect = coord.rect();
            let input = source_surface.read_region(rect);
            converted.resize(input.len() / alpha_format.channels() * target.channels(), 0.0);
            transform.convert_f32(&input, alpha_format.channels(), &mut converted, target.channels(), true);
            output.write_region(rect, &converted);
        }
        output.prune();
        Ok(output)
    }

    pub fn srgb(&self) -> Result<Surface> {
        self.converted(photocraft_cms::Builtin::Srgb.profile(), PixelFormat::new(ColorMode::Rgb, SampleType::U8, true), Intent::RelativeColorimetric, true)
    }
}

fn cms_error(error: photocraft_cms::CmsError) -> EngineError {
    EngineError::Other(format!("Clipboard colour management: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use photocraft_cms::Builtin;
    use photocraft_doc::{Layer, LayerContent};
    use photocraft_geom::Size;
    use serde_json::json;

    fn document(profile: Builtin, depth: SampleType, rgba: [f32; 4]) -> Document {
        let mut doc = Document::new("Clipboard colour", Size::new(2, 2), ColorMode::Rgb, depth);
        doc.icc_profile = Some(profile.profile().to_bytes());
        let mut layer = Layer::raster("Pixels", PixelFormat::new(ColorMode::Rgb, depth, true));
        let mut surface = Surface::new(layer.surface().unwrap().format());
        surface.write_region(doc.bounds(), &rgba.repeat(4));
        layer.content = LayerContent::Raster(surface);
        doc.layers.push(layer);
        doc
    }

    #[test]
    fn clipboard_colour_copy_paste_convert_numbers_undo_and_new_document() {
        for copy in ["edit.copy", "edit.copyMerged", "edit.cut"] {
            let mut session = Session::new();
            session.add_document(document(Builtin::LinearSrgb, SampleType::F32, [0.25, 0.5, 0.1, 0.375]), None);
            session.execute(copy, json!({})).unwrap();
            assert!(session.clipboard.as_ref().unwrap().profile.same_colors(Builtin::LinearSrgb.profile()));
            session.add_document(document(Builtin::Srgb, SampleType::F32, [0.; 4]), None);
            let before = session.active().unwrap().doc.layers.len();
            session.execute("edit.paste", json!({})).unwrap();
            let px = session.active().unwrap().doc.layers.last().unwrap().surface().unwrap().pixel(0, 0);
            assert!((px[0] - 0.537099).abs() < 0.0001, "{copy}: {px:?}");
            assert!((px[1] - 0.735357).abs() < 0.0001);
            assert_eq!(px[3], 0.375);
            assert!(session.undo());
            assert_eq!(session.active().unwrap().doc.layers.len(), before);
            session.execute("edit.paste", json!({"colorPolicy":"numbers"})).unwrap();
            let px = session.active().unwrap().doc.layers.last().unwrap().surface().unwrap().pixel(0, 0);
            assert!((px[0] - 0.25).abs() < 0.00001);
            session.execute("file.newFromClipboard", json!({})).unwrap();
            let new = &session.active().unwrap().doc;
            assert_eq!(new.icc_profile, Some(Builtin::LinearSrgb.profile().to_bytes()));
            assert_eq!(new.depth, SampleType::F32);
        }
    }

    #[test]
    fn clipboard_colour_conversion_keeps_destination_precision() {
        let clip = Clip::from_document(&document(Builtin::LinearSrgb, SampleType::U8, [0.25, 0.5, 0.1, 0.375])).unwrap();
        let target = document(Builtin::Srgb, SampleType::U16, [0.; 4]);
        let surface = clip.for_document(&target, false, Intent::RelativeColorimetric, true).unwrap();
        let input = clip.surface.pixel(0, 0);
        let mut expected = [0.; 3];
        Transform::new(Builtin::LinearSrgb.profile(), Builtin::Srgb.profile(), Intent::RelativeColorimetric, true).unwrap().eval(&input[..3], &mut expected);
        let actual = surface.pixel(0, 0);
        for i in 0..3 {
            assert!((actual[i] - expected[i]).abs() <= 1.1 / 65535., "{actual:?} vs {expected:?}");
        }
        assert_eq!(surface.format().sample, SampleType::U16);
        assert!((actual[3] - input[3]).abs() <= 1. / 65535.);
        assert!((actual[0] * 255. - (actual[0] * 255.).round()).abs() > 0.02, "result was not rounded to source's 8-bit grid");
    }

    #[test]
    fn clipboard_colour_os_export_is_srgb_and_retains_internal_pixels() {
        let clip = Clip::from_document(&document(Builtin::LinearSrgb, SampleType::F32, [0.25, 0.5, 0.1, 0.375])).unwrap();
        let before = clip.surface.pixel(0, 0);
        let exported = clip.srgb().unwrap();
        let mut out = [[0u8; 4]; 1];
        exported.read_rgba8_into(Rect::from_xywh(0, 0, 1, 1), &mut out);
        assert_eq!(out[0], [137, 188, 89, 96]);
        assert_eq!(clip.surface.pixel(0, 0), before);
        assert!(clip.profile.same_colors(Builtin::LinearSrgb.profile()));
    }

    #[test]
    fn clipboard_colour_rejects_invalid_policies_without_editing() {
        let mut session = Session::new();
        session.add_document(document(Builtin::LinearSrgb, SampleType::F32, [0.25, 0.5, 0.1, 0.375]), None);
        session.execute("edit.copy", json!({})).unwrap();
        for policy in [json!("discard"), json!(false)] {
            let before = session.active().unwrap().revision;
            assert!(session.execute("edit.paste", json!({"colorPolicy":policy})).is_err());
            assert_eq!(session.active().unwrap().revision, before);
        }
        session.add_document(Document::new("CMYK", Size::new(2, 2), ColorMode::Cmyk, SampleType::F32), None);
        let before = session.active().unwrap().revision;
        assert!(session.execute("edit.paste", json!({"colorPolicy":"numbers"})).is_err());
        assert_eq!(session.active().unwrap().revision, before);
        session.execute("edit.paste", json!({})).unwrap();
        let layer = session.active().unwrap().doc.layers.last().unwrap();
        assert_eq!(layer.surface().unwrap().format().mode, ColorMode::Cmyk);
        assert_eq!(layer.surface().unwrap().pixel(0, 0)[4], 0.375);
    }

    #[test]
    fn clipboard_colour_merged_custom_cmyk_uses_real_document_profile() {
        use photocraft_cms::synth::{CmykParams, cmyk_profile};
        let profile = cmyk_profile(&CmykParams { tvi: [0.45; 4], grid_a2b: 3, grid_b2a: 3, ..Default::default() });
        let mut doc = Document::new("CMYK", Size::new(2, 2), ColorMode::Cmyk, SampleType::F32);
        doc.icc_profile = Some(profile.to_bytes());
        let format = PixelFormat::new(ColorMode::Cmyk, SampleType::F32, true);
        let mut layer = Layer::raster("Ink", format);
        let surface = layer.surface_mut().unwrap();
        surface.write_region(doc.bounds(), &[0.2, 0.3, 0.1, 0.1, 0.375].repeat(4));
        doc.layers.push(layer);
        let rendered = photocraft_compose::render(&doc, doc.bounds());
        let clip = Clip::from_document(&doc).unwrap();
        let transform = Transform::new(Builtin::Srgb.profile(), &profile, Intent::RelativeColorimetric, true).unwrap();
        let mut expected = [0.; 4];
        transform.eval(&rendered.px[0][..3], &mut expected);
        let actual = clip.surface.pixel(0, 0);
        for i in 0..4 {
            assert!((actual[i] - expected[i]).abs() < 0.0001, "{actual:?} != {expected:?}");
        }
        assert_eq!(actual[4], 0.375);
        assert_eq!(clip.profile.to_bytes(), profile.to_bytes());
    }
    #[test]
    fn clipboard_colour_mixed_gray_layer_keeps_document_rgb_interpretation() {
        let mut doc = document(Builtin::LinearSrgb, SampleType::F32, [0.; 4]);
        let gray = PixelFormat::new(ColorMode::Grayscale, SampleType::F32, true);
        let mut layer = Layer::raster("Gray layer", gray);
        let surface = layer.surface_mut().unwrap();
        surface.write_region(doc.bounds(), &[0.25, 0.375].repeat(4));
        // Raw Copy must not bake the layer opacity or mask into its pixels.
        layer.opacity = 0.5;
        doc.layers = vec![layer];
        let mut session = Session::new();
        session.add_document(doc, None);
        session.execute("edit.copy", json!({})).unwrap();
        assert!(session.clipboard.as_ref().unwrap().profile.same_colors(Builtin::LinearSrgb.profile()));
        session.execute("edit.paste", json!({})).unwrap();
        let pixel = session.active().unwrap().doc.layers.last().unwrap().surface().unwrap().pixel(0, 0);
        assert_eq!(pixel, vec![0.25, 0.25, 0.25, 0.375]);
        session.add_document(document(Builtin::Srgb, SampleType::F32, [0.; 4]), None);
        session.execute("edit.paste", json!({})).unwrap();
        let pixel = session.active().unwrap().doc.layers.last().unwrap().surface().unwrap().pixel(0, 0);
        assert!((pixel[0] - 0.537099).abs() < 0.0001);
        assert_eq!(pixel[3], 0.375);
    }

    #[test]
    fn clipboard_colour_huge_sparse_canvas_refused_before_rendering_and_cancel_is_honoured() {
        let doc = Document::new("Sparse", Size::new(1_000_000, 1_000_000), ColorMode::Rgb, SampleType::U8);
        let mut checks = 0;
        let result = Clip::from_document_checked(&doc, || {
            checks += 1;
            Ok(())
        });
        assert!(result.unwrap_err().to_string().contains("decoded memory limit"));
        assert_eq!(checks, 1, "admission precedes rendering");
        let mut doc = document(Builtin::LinearSrgb, SampleType::F32, [0.25, 0.5, 0.1, 0.375]);
        doc.size = Size::new(2, 300);
        let mut checks = 0;
        let result = Clip::from_document_checked(&doc, || {
            checks += 1;
            if checks >= 3 { Err(EngineError::Cancelled) } else { Ok(()) }
        });
        assert!(matches!(result, Err(EngineError::Cancelled)));
        assert_eq!(checks, 3, "cancel after first band discards partial pixels");
    }
}
