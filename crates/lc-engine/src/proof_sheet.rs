//! Paginated, color-managed contact sheets. Uses the ordinary full-quality export renderer,
//! krilla's lossless PNG embedding and vector metadata captions shaped by the existing pc-text engine.
use crate::{Session, media::RenderJob};
use krilla::{
    Document,
    geom::{Size, Transform},
    image::Image,
    page::PageSettings,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageSize {
    #[default]
    A4,
    Letter,
    A3,
}
impl PageSize {
    pub fn points(self) -> (f32, f32) {
        match self {
            Self::A4 => (595.276, 841.89),
            Self::Letter => (612., 792.),
            Self::A3 => (841.89, 1190.551),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Caption {
    #[default]
    Filename,
    Title,
    Caption,
    FilenameTitle,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProofOptions {
    pub page: PageSize,
    pub landscape: bool,
    pub columns: usize,
    pub rows: usize,
    pub caption: Caption,
    pub watermark: String,
}
impl Default for ProofOptions {
    fn default() -> Self {
        Self { page: PageSize::A4, landscape: false, columns: 3, rows: 4, caption: Caption::Filename, watermark: String::new() }
    }
}
const MARGIN: f32 = 28.346; // 10 mm, PDF points, not UI geometry
const GAP: f32 = 10.;
const CAPTION_HEIGHT: f32 = 32.;
const CAPTION_GAP: f32 = 4.;
const DPI: f32 = 300.;
impl ProofOptions {
    pub fn dimensions(&self) -> (f32, f32) {
        let (w, h) = self.page.points();
        if self.landscape { (h, w) } else { (w, h) }
    }
    /// Pixel bounds for the full-quality, 300 dpi image area of one cell.
    pub fn render_bounds(&self) -> Result<(usize, usize), String> {
        let (cw, ch) = self.cell()?;
        Ok(((cw * DPI / 72.).ceil() as usize, ((ch - CAPTION_HEIGHT - CAPTION_GAP) * DPI / 72.).ceil() as usize))
    }
    fn cell(&self) -> Result<(f32, f32), String> {
        if !(1..=12).contains(&self.columns) || !(1..=12).contains(&self.rows) {
            return Err("grid rows and columns must be 1..12".into());
        }
        let (w, h) = self.dimensions();
        let cw = (w - MARGIN * 2. - GAP * (self.columns - 1) as f32) / self.columns as f32;
        let ch = (h - MARGIN * 2. - GAP * (self.rows - 1) as f32) / self.rows as f32;
        if cw < 24. || ch <= CAPTION_HEIGHT + CAPTION_GAP + 12. {
            return Err("grid is too dense for this page size".into());
        }
        Ok((cw, ch))
    }
}
pub struct ProofItem {
    render: RenderJob,
    cost: u64,
    caption: String,
}
pub struct ProofJob {
    opts: ProofOptions,
    items: Vec<ProofItem>,
}
impl Session {
    /// Detach jobs on the UI thread; decoding, rendering and PDF serialization happen elsewhere.
    pub fn prepare_proof_sheet(&mut self, ids: &[lightcraft_catalog::PhotoId], opts: ProofOptions) -> Result<ProofJob, String> {
        let (max_w, max_h) = opts.render_bounds()?;
        if ids.is_empty() {
            return Err("select photos for the proof sheet".into());
        }
        let mut items = Vec::with_capacity(ids.len());
        for id in ids {
            let p = self.catalog.photo(*id).ok_or("no such photo")?;
            let caption = match opts.caption {
                Caption::Filename => p.file_name.clone(),
                Caption::Title => {
                    if p.meta.title.is_empty() {
                        p.file_name.clone()
                    } else {
                        p.meta.title.clone()
                    }
                }
                Caption::Caption => {
                    if p.meta.caption.is_empty() {
                        p.file_name.clone()
                    } else {
                        p.meta.caption.clone()
                    }
                }
                Caption::FilenameTitle => {
                    if p.meta.title.is_empty() {
                        p.file_name.clone()
                    } else {
                        format!("{} - {}", p.file_name, p.meta.title)
                    }
                }
            };
            let cost = crate::export::render_cost((p.width as usize, p.height as usize), (max_w, max_h), lightcraft_pipeline::OutputDepth::U8);
            let render = self.export_job(*id, max_w, max_h, lightcraft_pipeline::OutputSpace::Srgb, lightcraft_pipeline::OutputDepth::U8)?;
            items.push(ProofItem { render, cost, caption });
        }
        Ok(ProofJob { opts, items })
    }
}
/// Existing paragraph layout handles Unicode, kerning, bidi and installed-font fallback.
/// Ellipsize to three lines, preserving a Unicode character boundary.
fn caption_layout(engine: &mut photocraft_text::TextEngine, text: &str, width: f32) -> photocraft_text::TextLayout {
    let mut layer = photocraft_doc::TextLayer {
        text: text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(4096).collect(),
        font_family: "Inter".into(),
        size_pt: 9.,
        shape: photocraft_doc::text::TextShape::Box { x: 0., y: 0., width, height: CAPTION_HEIGHT },
        ..Default::default()
    };
    layer.paragraphs = vec![photocraft_doc::text::ParagraphRun {
        len: layer.text.len(),
        style: photocraft_doc::text::ParagraphStyle { align: photocraft_doc::text::TextAlign::Center, ..Default::default() },
    }];
    let mut layout = engine.layout(&layer, 72.);
    if layout.lines.len() > 3 {
        layer.text.truncate(layout.lines[2].range.end);
        layer.text.push('…');
        layout = engine.layout(&layer, 72.);
        while layout.lines.len() > 3 {
            layer.text.pop();
            layer.text.pop();
            layer.text.push('…');
            layout = engine.layout(&layer, 72.);
        }
    }
    layout
}
/// Reuse pc-text's authoritative outlines (including complex scripts) at PDF resolution.
/// Outlines avoid embedding installed fonts with licence restrictions.
fn draw_caption(surface: &mut krilla::surface::Surface<'_>, engine: &mut photocraft_text::TextEngine, text: &str, width: f32, x: f32, y: f32) {
    let layout = caption_layout(engine, text, width);
    let outlines = photocraft_text::render::outlines(&layout, &photocraft_geom::Affine::IDENTITY, None);
    surface.set_fill(Some(krilla::paint::Fill { paint: krilla::color::rgb::Color::new(0, 0, 0).into(), ..Default::default() }));
    surface.push_transform(&Transform::from_translate(x, y));
    for outline in outlines {
        let mut path = krilla::geom::PathBuilder::new();
        for el in outline {
            use photocraft_text::render::PathEl;
            match el {
                PathEl::MoveTo(p) => path.move_to(p[0] as f32, p[1] as f32),
                PathEl::LineTo(p) => path.line_to(p[0] as f32, p[1] as f32),
                PathEl::QuadTo(c, p) => path.quad_to(c[0] as f32, c[1] as f32, p[0] as f32, p[1] as f32),
                PathEl::CurveTo(a, b, p) => path.cubic_to(a[0] as f32, a[1] as f32, b[0] as f32, b[1] as f32, p[0] as f32, p[1] as f32),
                PathEl::Close => path.close(),
            }
        }
        if let Some(path) = path.finish() {
            surface.draw_path(&path);
        }
    }
    surface.pop();
}
impl ProofJob {
    pub fn run(self, path: &Path, cancelled: &(dyn Fn() -> bool + Sync), progress: &mut dyn FnMut(usize)) -> Result<usize, String> {
        let (cw, ch) = self.opts.cell()?;
        let (pw, ph) = self.opts.dimensions();
        let per_page = self.opts.rows * self.opts.columns;
        let pages = self.items.len().div_ceil(per_page);
        let mut text_engine = photocraft_text::shared().lock().unwrap_or_else(std::sync::PoisonError::into_inner).fork();
        let mut pdf = Document::new();
        pdf.set_metadata(krilla::metadata::Metadata::new().title("Proof Sheet".into()).creator("Local Image".into()));
        let mut items = self.items.into_iter();
        let mut retained_bytes = 0u64;
        for page_index in 0..pages {
            if cancelled() {
                return Err("Proof sheet cancelled".into());
            }
            let mut page = pdf.start_page_with(PageSettings::from_wh(pw, ph).ok_or("invalid page size")?);
            let mut surface = page.surface();
            for cell in 0..per_page {
                let Some(item) = items.next() else { break };
                if cancelled() {
                    return Err("Proof sheet cancelled".into());
                }
                let _permit = lightcraft_sysmem::heavy_jobs().admit(item.cost, cancelled).ok_or("Proof sheet cancelled")?;
                let mut rendered = crate::export::pipeline::run_render_job(item.render).rendered?;
                if !self.opts.watermark.is_empty() {
                    crate::export::draw_watermark(
                        &mut rendered.image,
                        &crate::export::Watermark { text: self.opts.watermark.clone(), ..Default::default() },
                    );
                }
                let img = &rendered.image;
                let scale = (cw / img.width as f32).min((ch - CAPTION_HEIGHT - CAPTION_GAP) / img.height as f32);
                let (w, h) = (img.width as f32 * scale, img.height as f32 * scale);
                let x = MARGIN + (cell % self.opts.columns) as f32 * (cw + GAP);
                let y = MARGIN + (cell / self.opts.columns) as f32 * (ch + GAP);
                let png =
                    lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(img), &Default::default()).map_err(|e| e.to_string())?;
                retained_bytes = retained_bytes.saturating_add(png.len() as u64);
                // krilla retains image data until serialization, which also needs output and
                // compression buffers. Preserve print quality and fail cleanly rather than OOM.
                if retained_bytes.saturating_mul(3) > lightcraft_sysmem::heavy_jobs().headroom() {
                    return Err("Proof sheet exceeds available memory; export fewer photos at a time".into());
                }
                let image = Image::from_png(png.into(), true)?;
                surface.push_transform(&Transform::from_translate(x + (cw - w) / 2., y + (ch - CAPTION_HEIGHT - CAPTION_GAP - h) / 2.));
                surface.draw_image(image, Size::from_wh(w, h).ok_or("invalid image size")?);
                surface.pop();
                draw_caption(&mut surface, &mut text_engine, &item.caption, cw, x, y + ch - CAPTION_HEIGHT);
                progress(page_index * per_page + cell + 1);
            }
            draw_caption(&mut surface, &mut text_engine, &format!("{} / {pages}", page_index + 1), pw - MARGIN * 2., MARGIN, ph - 16.);
            surface.finish();
            page.finish();
        }
        if cancelled() {
            return Err("Proof sheet cancelled".into());
        }
        let bytes = pdf.finish().map_err(|e| format!("PDF serialization: {e:?}"))?;
        if cancelled() {
            return Err("Proof sheet cancelled".into());
        }
        // Re-creatable output: atomic, never overwrites any existing file or an original.
        li_fs::write_new(path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(pages)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proof_sheet_pagination_metadata_and_atomic_failure() {
        let root = std::env::temp_dir().join(format!("li-proof-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let mut s = Session::with_demo();
        let ids = s.visible_cloned();
        let o = ProofOptions { rows: 2, columns: 2, watermark: "PROOF".into(), ..Default::default() };
        let path = root.join("proof.pdf");
        let mut count = 0;
        let pages = s.prepare_proof_sheet(&ids[..5], o.clone()).unwrap().run(&path, &|| false, &mut |n| count = n).unwrap();
        assert_eq!((pages, count), (2, 5));
        let bytes = std::fs::read(&path).unwrap();
        let evidence = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/library-safety");
        std::fs::create_dir_all(&evidence).unwrap();
        std::fs::write(evidence.join("proof-layout.pdf"), &bytes).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(s.prepare_proof_sheet(&ids[..1], o.clone()).unwrap().run(&path, &|| false, &mut |_| {}).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let fault = li_fs::fail_writes_after(20);
        let broken = root.join("broken.pdf");
        assert!(s.prepare_proof_sheet(&ids[..1], o).unwrap().run(&broken, &|| false, &mut |_| {}).is_err());
        drop(fault);
        assert!(!broken.exists());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn proof_sheet_caption_metrics_grid_validation_and_cancel() {
        let mut engine = photocraft_text::TextEngine::new();
        let layout = caption_layout(&mut engine, "a short caption", 100.);
        assert_eq!(layout.lines.len(), 1);
        let layout = caption_layout(&mut engine, &"a very long caption ".repeat(30), 80.);
        assert_eq!(layout.lines.len(), 3);
        assert!(layout.lines.iter().all(|l| l.x1 - l.x0 <= 80.1));
        let mut s = Session::with_demo();
        let ids = s.visible_cloned();
        assert!(s.prepare_proof_sheet(&ids, ProofOptions { rows: 0, ..Default::default() }).is_err());
        assert!(s.prepare_proof_sheet(&[], Default::default()).is_err());
        let output = std::env::temp_dir().join(format!("li-proof-cancel-{}.pdf", uuid::Uuid::new_v4()));
        assert!(s.prepare_proof_sheet(&ids[..1], Default::default()).unwrap().run(&output, &|| true, &mut |_| {}).is_err());
        assert!(!output.exists());
    }
}
