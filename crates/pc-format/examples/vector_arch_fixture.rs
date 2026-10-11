use photocraft_doc::*;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut d = Document::new("Legacy vector fixture", Size::new(32, 24), ColorMode::Rgb, SampleType::U8);
    let p = Path::new(vec![Subpath::polygon(&[(2., 2.), (18., 2.), (18., 16.), (2., 16.)])]);
    let mut l = Layer::new(
        "Rectangle",
        LayerContent::Shape(ShapeLayer {
            path: p.clone(),
            fill: Some(Fill::Solid(Color::BLACK)),
            stroke: Some(ShapeStroke::default()),
            live: Some(LiveShape::Rect { rect: [2., 2., 16., 14.], radii: [0.; 4] }),
            ..Default::default()
        }),
    );
    l.vector_mask = Some(VectorMask::new(p.clone()));
    d.layers.push(l);
    d.layers.push(Layer::new("Legacy text", LayerContent::Text(TextLayer { text: "Old".into(), ..Default::default() })));
    d.paths.push(NamedPath { name: "Saved outline".into(), path: p.clone(), psd_raw: None });
    d.work_path = Some(p);
    d.clipping_path = Some(ClippingPath { name: "Saved outline".into(), flatness: 0. });
    let bytes = photocraft_format::save_to_bytes(&d, &Default::default())?;
    let manifest = photocraft_format::read_manifest(&bytes)?;
    let raw = serde_json::to_string_pretty(&manifest)?;
    std::fs::write("crates/pc-format/tests/fixtures/vector-v1.pcraft", bytes)?;
    std::fs::write("crates/pc-format/tests/fixtures/vector-v1.json", raw)?;
    Ok(())
}
