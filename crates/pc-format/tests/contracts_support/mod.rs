//! Shared, deterministic contract inputs; no rendering or GUI is needed.
#![allow(dead_code)]
use photocraft_doc::vector_design::{appearance::*, drawing::*, live::*, structure::*, text::*};
use photocraft_doc::*;

pub fn document() -> Document {
    Document::new("Contracts", Size::new(32, 24), ColorMode::Rgb, SampleType::U8)
}
pub fn pixels(format: PixelFormat) -> Surface {
    let mut s = Surface::new(format);
    s.fill_rect(Rect::new(2, 3, 8, 9), &photocraft_raster::from_rgba(&format, [0.2, 0.4, 0.8, 1.]));
    s.write_pixel(4, 5, &photocraft_raster::from_rgba(&format, [0.6, 0.2, 0.4, 0.8]));
    s
}
pub fn shape() -> Layer {
    Layer::new(
        "Shape",
        LayerContent::Shape(ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(2., 2.), (18., 2.), (18., 16.), (2., 16.)])]),
            fill: Some(Fill::Solid(Color::BLACK)),
            stroke: Some(ShapeStroke { width: 3., ..Default::default() }),
            live: Some(LiveShape::Rect { rect: [2., 2., 16., 14.], radii: [0.; 4] }),
            ..Default::default()
        }),
    )
}
pub fn vector_data(path: Path) -> VectorData {
    VectorData {
        corners: vec![KnotCorner { subpath: 0, knot: 1, corner: Corner { radius: 2., kind: CornerKind::Chamfer } }],
        live_shape: Some(DesignShape { rect: [2., 2., 16., 14.], angle: 0.25, parameters: ShapeParameters::Star { points: 5, inner: 0.5, corner: 2. } }),
        appearance: Some(Appearance {
            items: vec![
                AppearanceItem::Fill(FillItem { opacity: 0.625, blend: BlendMode::Multiply, visible: false, ..FillItem::new(Fill::Solid(Color::WHITE)) }),
                AppearanceItem::Stroke(StrokeItem {
                    profile: Some(WidthProfile { points: vec![WidthPoint { t: 0., left: 1., right: 2. }, WidthPoint { t: 0.75, left: 2., right: 3. }] }),
                    brush: Some(BrushRef { id: "contract-brush".into() }),
                    start_arrow: Some(Arrowhead::Triangle),
                    end_arrow: Some(Arrowhead::CircleOpen),
                    arrow_scale: [0.5, 2.],
                    dash_align: true,
                    scale_with_object: true,
                    gradient_mode: StrokeGradientMode::Along,
                    ..Default::default()
                }),
            ],
        }),
        constraints: Some(Constraints { right: true, fixed_width: true, ..Default::default() }),
        clip: Some(path),
        ..Default::default()
    }
}
/// Expected model for the frozen v3 fixture, independently constructed with fixed IDs.
pub fn golden_document() -> Document {
    let mut d = Document::new("Contract reference v3", Size::new(32, 24), ColorMode::Rgb, SampleType::U8);
    d.id = DocId(1);
    d.design_units = DesignUnits::Millimeters;
    let mut rectangle = shape();
    rectangle.id = LayerId(2);
    rectangle.name = "Rectangle".into();
    let LayerContent::Shape(s) = &rectangle.content else { unreachable!() };
    let p = s.path.clone();
    rectangle.vector_mask = Some(VectorMask::new(p.clone()));
    let mut text = Layer::new("Legacy text", LayerContent::Text(TextLayer { text: "Old".into(), ..Default::default() }));
    text.id = LayerId(3);
    d.layers = vec![rectangle, text];
    d.paths = vec![NamedPath { name: "Saved outline".into(), path: p.clone(), psd_raw: None }];
    d.work_path = Some(p.clone());
    d.clipping_path = Some(ClippingPath { name: "Saved outline".into(), flatness: 0. });
    d.layers[0].vector = vector_data(p.clone());
    d.layers[1].vector.on_path = Some(PathText { path: p, start: 0.1, end: 0.9, flip: true, align: PathTextAlign::Center });
    let mut master = d.layers[0].clone();
    master.id = LayerId(4);
    master.name = "Master shape".into();
    d.symbols.push(SymbolDef { id: SymbolId(1), name: "Contract symbol".into(), layers: vec![master] });
    let mut instance = Layer::new("Instance", LayerContent::Symbol(SymbolInstance { symbol: SymbolId(1), transform: Affine::translate(3., 4.), cache: None }));
    instance.id = LayerId(5);
    d.layers.push(instance);
    let mut child = shape();
    child.id = LayerId(7);
    let mut group = Layer::group("Live blend", vec![child]);
    group.id = LayerId(6);
    group.vector.live =
        Some(LiveGroup::Blend(BlendSpec { spacing: BlendSpacing::Steps(7), orientation: BlendOrientation::AlignToPath, spine: Some(d.paths[0].path.clone()) }));
    d.layers.push(group);
    d
}
