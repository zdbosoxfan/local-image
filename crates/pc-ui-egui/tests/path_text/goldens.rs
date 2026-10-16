//! Independent resvg 0.45.1 oracle on externally converted SVG textPath goldens.
//! Text support is not enabled in our locked usvg graph; an absent converted
//! golden is a hard failure when this explicit corpus gate is invoked.
use super::*;
#[test]
#[ignore = "requires LOCAL_IMAGE_PATH_TEXT_GOLDENS with the Inkscape textPath-to-outlines golden; see SOURCES.md"]
fn path_text_resvg_ofl_textpath_golden_iou() {
    let directory = std::env::var("LOCAL_IMAGE_PATH_TEXT_GOLDENS").expect("licensed textPath golden directory required");
    let bytes = std::fs::read(std::path::Path::new(&directory).join("abc-circle-outlines.svg")).expect("coordinator-generated golden required");
    let svg = std::str::from_utf8(&bytes).unwrap();
    assert!(!svg.contains("<text"), "golden must contain Inkscape-generated outlines, not unrendered text");
    // Resvg's 4x4 edge quantization at 20 px disagrees with exact area coverage
    // around the 50% mask cutoff. Compare both geometries at 8x, retaining IoU > .97.
    const SCALE: u32 = 8;
    let side = 256 * SCALE;
    let golden = egui_extras::image::load_svg_bytes_with_size(&bytes, egui::SizeHint::Width(side), &Default::default()).expect("resvg reference renderer");
    assert_eq!(golden.size, [side as usize, side as usize]);
    assert!(golden.pixels.iter().any(|p| p.a() > 127));
    let mut h = harness();
    create(&mut h);
    let mut doc = (*h.state().session.active().unwrap().doc).clone();
    doc.size = Size::new(side, side);
    let id = active(&h);
    let layer = doc.layer(id).unwrap();
    let path = layer.vector.on_path.clone().unwrap();
    let LayerContent::Text(mut text) = layer.content.clone() else { panic!() };
    text.transform = photocraft_geom::Affine::scale(SCALE as f64).mul(&text.transform);
    photocraft_engine::vector_packages::path_text::refresh(&doc, &mut text, &path).unwrap();
    doc.layer_mut(id).unwrap().content = LayerContent::Text(text);
    let ours = photocraft_compose::flatten(&doc);
    let (mut intersection, mut union) = (0, 0);
    for (a, b) in ours.px.iter().zip(&golden.pixels) {
        if a[3] > 0.5 && b.a() > 127 {
            intersection += 1;
        }
        if a[3] > 0.5 || b.a() > 127 {
            union += 1;
        }
    }
    let score = intersection as f64 / union as f64;
    eprintln!("independent Inkscape/resvg textPath 8x IoU={score}");
    assert!(score > 0.97, "textPath IoU={score}");
}
