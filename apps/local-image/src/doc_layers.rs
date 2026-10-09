//! The Library's read-only **Layers** list of a layered document (PSD/PSB, layered TIFF,
//! `.pcraft`): the host reads the document with the editor's readers and hands the Library plain
//! data (`lightcraft_ui_egui::panels::doc_layers`), so the Library's crates never depend on the
//! editor's. Develop layers say which Library photo they follow.

use lightcraft_ui_egui::panels::doc_layers::{DocLayer, DocLayers, LayerKind};
use photocraft_doc::{Document, Layer, LayerContent};

/// Layers listed at most (the rest are counted).
const MAX_LAYERS: usize = 200;
/// Thumbnail size (longer side, pixels).
const THUMB: u32 = 48;

/// The layers of the document at `path` (runs on the Library's worker thread).
pub fn load(path: &str) -> Result<DocLayers, String> {
    let bytes = photocraft_format::read_file(std::path::Path::new(path)).map_err(|e| e.to_string())?;
    let doc = if photocraft_format::is_pcraft(&bytes) {
        photocraft_format::load_from_bytes(&bytes).map_err(|e| e.to_string())?
    } else {
        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        photocraft_io::import(&name, &bytes).map_err(|e| e.to_string())?.document
    };
    Ok(summarize(&doc))
}

fn kind(l: &Layer) -> LayerKind {
    match &l.content {
        LayerContent::Raster(_) => LayerKind::Pixel,
        LayerContent::Group(_) => LayerKind::Group,
        LayerContent::Adjustment(_) => LayerKind::Adjustment,
        LayerContent::Fill(_) => LayerKind::Fill,
        LayerContent::Text(_) => LayerKind::Text,
        LayerContent::Shape(_) => LayerKind::Shape,
        LayerContent::Smart(sm) if sm.develop.is_some() => LayerKind::Develop,
        LayerContent::Smart(_) => LayerKind::Smart,
    }
}

/// `doc`'s layers, top first (a group before its layers), with thumbnails.
pub fn summarize(doc: &Document) -> DocLayers {
    fn walk<'a>(layers: &'a [Layer], depth: usize, out: &mut Vec<(usize, &'a Layer)>) {
        for l in layers.iter().rev() {
            out.push((depth, l));
            if let Some(ch) = l.children() {
                walk(ch, depth + 1, out);
            }
        }
    }
    let mut all = Vec::new();
    walk(&doc.layers, 0, &mut all);
    let more = all.len().saturating_sub(MAX_LAYERS);
    // one layer at a time, shown, on an otherwise empty copy of the document
    let mut single = doc.clone();
    single.layers.clear();
    let layers = all
        .into_iter()
        .take(MAX_LAYERS)
        .map(|(depth, l)| {
            let kind = kind(l);
            let thumb = (kind != LayerKind::Adjustment).then(|| {
                let mut shown = l.clone();
                shown.visible = true;
                single.layers = vec![shown];
                let img = photocraft_compose::thumbnail(&single, THUMB);
                lightcraft_raster::Rgba8 {
                    width: img.width as usize,
                    height: img.height as usize,
                    data: img.pixels.as_chunks::<4>().0.to_vec(),
                }
            });
            DocLayer {
                name: l.name.clone(),
                depth,
                visible: l.visible,
                opacity: l.opacity,
                blend: l.blend.label().to_string(),
                kind,
                follows: match &l.content {
                    LayerContent::Smart(sm) => sm.develop.as_ref().and_then(|d| d.photo),
                    _ => None,
                },
                thumb,
            }
        })
        .collect();
    DocLayers { width: doc.size.width, height: doc.size.height, layers, more }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_color::{BlendMode, ColorMode, SampleType};
    use photocraft_doc::{DevelopLink, SmartObject, SmartSource};
    use photocraft_raster::Surface;

    fn raster(name: &str, rgba: [f32; 4]) -> Layer {
        let fmt = photocraft_color::PixelFormat::new(ColorMode::Rgb, SampleType::U8, true);
        let mut s = Surface::new(fmt);
        let r = photocraft_geom::Rect::new(0, 0, 8, 8);
        let px = photocraft_raster::from_rgba(&fmt, rgba);
        s.write_region(r, &px.repeat(64));
        Layer::new(name, LayerContent::Raster(s))
    }

    fn doc() -> Document {
        let mut d = Document::new("t", photocraft_doc::Size::new(8, 8), ColorMode::Rgb, SampleType::U8);
        let mut develop = raster("Develop", [0.2, 0.4, 0.6, 1.0]);
        let img = photocraft_codecs::Image::from_u8(8, 8, photocraft_codecs::ChannelLayout::Rgb, vec![120; 8 * 8 * 3]).unwrap();
        let png = photocraft_codecs::encode(&img, photocraft_codecs::Format::Png, &photocraft_codecs::EncodeOptions::default()).unwrap();
        let source = SmartSource::Embedded { file_name: "a.png".into(), bytes: std::sync::Arc::new(png) };
        let mut sm = SmartObject::new(source, photocraft_geom::Affine::IDENTITY, develop.surface().cloned());
        sm.develop = Some(DevelopLink { settings: serde_json::json!({}), photo: Some(7) });
        develop.content = LayerContent::Smart(sm);
        let mut retouch = raster("Retouch", [1.0, 0.0, 0.0, 1.0]);
        retouch.blend = BlendMode::Multiply;
        retouch.opacity = 0.5;
        let mut group = Layer::group("Group", vec![raster("Inside", [0.0, 1.0, 0.0, 1.0])]);
        group.visible = false;
        d.layers = vec![develop, retouch, group];
        d
    }

    #[test]
    fn layers_are_listed_top_first_with_develop_links_and_thumbnails() {
        let s = summarize(&doc());
        let names: Vec<&str> = s.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Group", "Inside", "Retouch", "Develop"]);
        assert_eq!(s.layers.iter().map(|l| l.depth).collect::<Vec<_>>(), [0, 1, 0, 0]);
        let (group, retouch, develop) = (&s.layers[0], &s.layers[2], &s.layers[3]);
        assert!(!group.visible && group.kind == LayerKind::Group);
        assert_eq!((retouch.blend.as_str(), retouch.opacity, retouch.kind), ("Multiply", 0.5, LayerKind::Pixel));
        assert_eq!((develop.kind, develop.follows), (LayerKind::Develop, Some(7)));
        let thumb = retouch.thumb.as_ref().unwrap();
        assert_eq!((thumb.width, thumb.height), (8, 8));
        assert!(thumb.data[0][0] > 200 && thumb.data[0][1] < 50, "the layer alone, at full opacity: {:?}", thumb.data[0]);
        // a hidden group's thumbnail still shows its contents
        assert!(group.thumb.as_ref().is_some_and(|t| t.data[0][1] > 200));
        assert_eq!(s.more, 0);
        assert!(s.is_layered());
    }

    #[test]
    fn psd_and_pcraft_files_load() {
        let dir = std::env::temp_dir().join(format!("li-doclayers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let d = doc();
        let psd = photocraft_io::export(&d, "psd", &photocraft_io::ExportOptions::default()).unwrap().bytes;
        std::fs::write(dir.join("a.psd"), psd).unwrap();
        std::fs::write(dir.join("a.pcraft"), photocraft_format::save_to_bytes(&d, &Default::default()).unwrap()).unwrap();
        for f in ["a.psd", "a.pcraft"] {
            let s = load(&dir.join(f).to_string_lossy()).unwrap();
            let names: Vec<&str> = s.layers.iter().map(|l| l.name.as_str()).collect();
            assert_eq!(names, ["Group", "Inside", "Retouch", "Develop"], "{f}");
            assert_eq!(s.layers[3].follows, Some(7), "{f}: the Develop layer's photo");
        }
        assert!(load(&dir.join("missing.psd").to_string_lossy()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
