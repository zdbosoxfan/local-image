//! Canonical tool families. Persona selects defaults and shortcuts, never an implementation.
//! Raster Pencil/Gradient keep their historic identities; Vector Pencil/Fill are available
//! in either persona and use the same vector handlers (architecture §5).
use crate::toolsets::{ALL_TOOLS, Persona, VECTOR_TOOLS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handler {
    Move,
    Pen,
    Node,
    PathSelection,
    Shape,
    CustomShape,
    Text,
    PathText,
    VectorPencil,
    RasterPencil,
    VectorPaint,
    RasterGradient,
    Eyedropper,
    Crop,
    VectorCrop,
    Hand,
    Zoom,
    Corner,
    Transform,
    Feature(u8),
    Raster,
}

/// Additional geometric tools exposed to Pixel without changing its frozen keymap or
/// old built-in layouts. The shell can append these to its existing shape/text flyouts.
pub const SHARED_ADDITIONS: &[&str] = &[
    "RoundedRectangle",
    "Star",
    "DoubleStar",
    "SquareStar",
    "Trapezoid",
    "Diamond",
    "Donut",
    "Pie",
    "Segment",
    "Arrow",
    "Callout",
    "Cog",
    "Heart",
    "Tear",
    "Crescent",
    "Cloud",
    "Spiral",
    "Arc",
    "RectGrid",
    "PolarGrid",
    "VectorPencil",
    "Corner",
    "VectorFill",
    "Transparency",
    "ArtText",
    "FrameText",
    "TextOnPath",
    "PointTransform",
    "VectorCrop",
];

pub fn shared_in_pixel(id: &str) -> bool {
    SHARED_ADDITIONS.iter().any(|name| name.eq_ignore_ascii_case(id))
}

/// Allocation-free lookup. All aliases of a family converge here before persona defaults.
pub fn handler(id: &str) -> Option<Handler> {
    let canonical = ALL_TOOLS.iter().copied().chain(VECTOR_TOOLS.iter().map(|t| t.id)).find(|name| name.eq_ignore_ascii_case(id))?;
    Some(match canonical {
        "Move" => Handler::Move,
        "Pen" => Handler::Pen,
        "DirectSelection" => Handler::Node,
        "PathSelection" => Handler::PathSelection,
        "CustomShape" => Handler::CustomShape,
        "Rectangle" | "RoundedRectangle" | "EllipseShape" | "Triangle" | "Polygon" | "Star" | "DoubleStar" | "SquareStar" | "Trapezoid" | "Diamond"
        | "Donut" | "Pie" | "Segment" | "Arrow" | "Callout" | "Cog" | "Heart" | "Tear" | "Crescent" | "Cloud" | "Spiral" | "Arc" | "Line" | "RectGrid"
        | "PolarGrid" => Handler::Shape,
        "Type" | "VerticalType" | "ArtText" | "FrameText" => Handler::Text,
        "TextOnPath" => Handler::PathText,
        "VectorPencil" => Handler::VectorPencil,
        "Pencil" => Handler::RasterPencil,
        "VectorFill" | "Transparency" => Handler::VectorPaint,
        "Gradient" => Handler::RasterGradient,
        "Eyedropper" => Handler::Eyedropper,
        "Crop" => Handler::Crop,
        "VectorCrop" => Handler::VectorCrop,
        "Hand" => Handler::Hand,
        "Zoom" => Handler::Zoom,
        "Corner" => Handler::Corner,
        "PointTransform" => Handler::Transform,
        _ => VECTOR_TOOLS.iter().find(|t| t.id == canonical).and_then(|t| t.package).map_or(Handler::Raster, Handler::Feature),
    })
}

pub fn resolve(id: &str, persona: Persona) -> Option<Handler> {
    crate::toolsets::supports(id, persona).then(|| handler(id)).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared47_every_shared_tool_has_one_handler_and_legacy_settings_load() {
        for id in ALL_TOOLS.iter().copied().filter(|id| crate::toolsets::supports(id, Persona::Vector)).chain(SHARED_ADDITIONS.iter().copied()) {
            assert_eq!(resolve(id, Persona::Pixel), resolve(id, Persona::Vector), "{id}");
            assert!(resolve(id, Persona::Pixel).is_some(), "{id}");
        }
        for id in ALL_TOOLS {
            assert!(resolve(id, Persona::Pixel).is_some(), "{id}");
        }
        for t in VECTOR_TOOLS {
            assert!(resolve(t.id, Persona::Vector).is_some(), "{}", t.id);
        }
        // Architecture explicitly keeps the old pixel-perfect Pencil and destructive
        // Gradient IDs distinct; naming overlap must not silently change old settings.
        assert_ne!(handler("Pencil"), handler("VectorPencil"));
        assert_ne!(handler("Gradient"), handler("VectorFill"));
        let mut s = crate::Session::new();
        s.load_prefs_json(r#"{"toolbar":{"activeSet":"photographer"}}"#).unwrap();
        assert_eq!(s.prefs().toolbar.persona, Persona::Pixel);
        assert_eq!(s.prefs().toolbar.active().id, "photographer");
        s.execute("toolset.save", serde_json::json!({"name":"Shared","tools":SHARED_ADDITIONS})).unwrap();
        let mut restored = crate::Session::new();
        restored.load_prefs_json(&s.prefs_to_json()).unwrap();
        assert_eq!(s.prefs().toolbar, restored.prefs().toolbar);
    }
}

#[cfg(test)]
mod preview_tests {
    use crate::vector_packages::pen::{self, Gesture, Mode, Modifiers, Target};
    use photocraft_doc::{ColorMode, Document, LayerContent, SampleType, Size};
    use photocraft_geom::Point;
    #[test]
    fn shared47_pen_preview_reuses_document_and_layer_and_matches_commit() {
        let mut s = crate::Session::new();
        s.add_document(Document::new("Large", Size::new(12000, 12000), ColorMode::Rgb, SampleType::U8), None);
        let mut g = Gesture::begin(&s, Target::NewShape { above: None }, Mode::Line, Point::new(40., 40.), 6., None, Some(Default::default())).unwrap();
        let mut doc = None;
        let mut layer = None;
        pen::update_preview(&s, g.request(), &mut doc, &mut layer).unwrap();
        let first = layer.unwrap();
        let allocation = doc.as_ref().unwrap().layers.as_ptr();
        for p in [Point::new(100., 70.), Point::new(150., 100.), Point::new(180., 100.)] {
            g.motion(p, Modifiers::default()).unwrap();
            pen::update_preview(&s, g.request(), &mut doc, &mut layer).unwrap();
            assert_eq!(layer, Some(first));
            assert_eq!(allocation, doc.as_ref().unwrap().layers.as_ptr());
            assert_eq!(doc.as_ref().unwrap().layers.len(), 1);
            assert!(s.journal.is_empty());
        }
        let generation = g.generation();
        g.motion(Point::new(180., 100.), Modifiers::default()).unwrap();
        assert_eq!(generation, g.generation());
        let commit = g.finish().unwrap();
        let detached = doc.unwrap();
        let LayerContent::Shape(shown) = &detached.layer(first).unwrap().content else { panic!("shape required") };
        let path = shown.path.clone();
        s.execute("path.penCommit", serde_json::to_value(commit).unwrap()).unwrap();
        let st = s.active().unwrap();
        let LayerContent::Shape(committed) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content else { panic!("shape required") };
        assert_eq!(path, committed.path);
        assert_eq!(s.journal.len(), 1);
        assert!(s.undo());
        assert!(s.active().unwrap().doc.layers.is_empty());
    }
    #[test]
    fn shared47_shape_paint_is_atomic_and_old_default_stays_valid() {
        let mut s = crate::Session::new();
        s.add_document(Document::new("Shape", Size::new(240, 240), ColorMode::Rgb, SampleType::U8), None);
        let design = photocraft_doc::vector_design::drawing::DesignShape {
            rect: [20., 30., 100., 80.],
            angle: 0.,
            parameters: photocraft_doc::vector_design::drawing::ShapeParameters::Ellipse,
        };
        let st = s.active().unwrap();
        let p = serde_json::json!({"document":st.doc.id.0,"revision":st.revision,"design":design});
        s.execute("shape.design.create", p).unwrap();
        let before = s.active().unwrap().doc.clone();
        let base = s.journal.len();
        let st = s.active().unwrap();
        let mut p =
            serde_json::json!({"document":st.doc.id.0,"revision":st.revision,"design":design,"fill":null,"stroke":photocraft_doc::ShapeStroke::default()});
        p["stroke"]["width"] = serde_json::json!(-2.);
        assert!(s.execute("shape.design.create", p).is_err());
        assert_eq!(before, s.active().unwrap().doc);
        assert_eq!(base, s.journal.len());
    }
}

#[cfg(test)]
mod snap_tests {
    use crate::vector_packages::snapping::query::{Angle, Cache, Exclusions, Settings, View};
    use photocraft_doc::{ColorMode, Document, Path, SampleType, Size, Subpath};
    #[test]
    fn shared47_constrained_snap_keeps_real_targets_and_rejects_off_ray_nodes() {
        let mut doc = Document::new("Snap", Size::new(240, 240), ColorMode::Rgb, SampleType::U8);
        doc.work_path = Some(Path::new(vec![Subpath::polyline(&[(99.5, 99.), (130., 130.)])]));
        let cache = Cache::build(&doc, 0, Exclusions::default(), None).unwrap();
        let settings = Settings { grid: true, grid_step: 20., document: false, bounds: false, alignment: false, ..Default::default() };
        let ray = Angle { origin: [40., 40.], step: std::f64::consts::FRAC_PI_4, offset: 0. };
        let snap = cache.point([99., 99.], &settings, View::default(), Some(ray), false).unwrap();
        assert_eq!(snap.point, [100., 100.]);
        assert!(snap.guides.iter().all(|g| g.hit.kind == crate::vector_packages::snapping::query::Kind::Grid));
        let only_angle = Settings { grid: false, nodes: false, ..settings };
        let snap = cache.point([99., 98.], &only_angle, View::default(), Some(ray), false).unwrap();
        assert!((snap.point[0] - 98.5).abs() < 1e-9 && (snap.point[1] - 98.5).abs() < 1e-9);
    }
}
