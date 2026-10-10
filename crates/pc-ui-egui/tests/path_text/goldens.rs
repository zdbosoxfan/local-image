//! Independent resvg 0.45.1 oracle on externally converted SVG textPath goldens.
//! Text support is not enabled in our locked usvg graph; an absent converted
//! golden is a hard failure when this explicit corpus gate is invoked.
use super::*;
#[test]
#[ignore = "requires coordinator Inkscape textPath-to-outlines golden; see SOURCES.md"]
fn path_text_resvg_ofl_textpath_golden_iou() {
    let directory = std::env::var("LOCAL_IMAGE_PATH_TEXT_GOLDENS").expect("licensed textPath golden directory required");
    let bytes = std::fs::read(std::path::Path::new(&directory).join("abc-circle-outlines.svg")).expect("coordinator-generated golden required");
    let svg = std::str::from_utf8(&bytes).unwrap();
    assert!(!svg.contains("<text"), "golden must contain Inkscape-generated outlines, not unrendered text");
    let golden = egui_extras::image::load_svg_bytes(&bytes, &Default::default()).expect("resvg reference renderer");
    assert_eq!(golden.size, [256, 256]);
    assert!(golden.pixels.iter().any(|p| p.a() > 127));
    let mut h = harness();
    create(&mut h);
    let ours = photocraft_compose::flatten(&h.state().session.active().unwrap().doc);
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
    assert!(score > 0.97, "textPath IoU={score}");
}
