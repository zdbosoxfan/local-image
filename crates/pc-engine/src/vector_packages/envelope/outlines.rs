//! Native path conversion using pc-text outlines. Quadratic elevation follows the
//! existing type_extra_cmds adapter; contours retain their glyph's winding.
use crate::vector_packages::path_text::{curves::glyph_subpaths, on_path::PaintedOutline};
use photocraft_doc::{Color, Path};
pub(super) fn paths(paints: &[PaintedOutline]) -> Vec<(Color, Path)> {
    let mut out: Vec<(Color, Path)> = Vec::new();
    for paint in paints {
        let index = out.iter().position(|(c, _)| *c == paint.color).unwrap_or_else(|| {
            out.push((paint.color, Path::default()));
            out.len() - 1
        });
        out[index].1.subpaths.extend(glyph_subpaths(&paint.elements));
    }
    out
}
