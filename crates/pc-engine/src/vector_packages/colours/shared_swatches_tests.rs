#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
#[test]
fn shared_swatches_native_live_text_and_shape_in_pixel_mode_and_undo() {
    use photocraft_doc::{
        ColorMode, SampleType, ShapeLayer, Size, TextLayer,
        text::{CharStyle, TextRun},
    };
    let mut doc = Document::new("Pixel", Size::new(96, 64), ColorMode::Rgb, SampleType::U8);
    let text = Layer::new(
        "Type",
        LayerContent::Text(TextLayer {
            text: "Text".into(),
            size_pt: 20.,
            runs: vec![TextRun { len: 4, style: CharStyle { size_pt: 20., ..Default::default() } }],
            ..Default::default()
        }),
    );
    let shape = Layer::new(
        "Shape",
        LayerContent::Shape(ShapeLayer {
            path: photocraft_vector::shapes::rect(0., 0., 20., 20.),
            fill: Some(Fill::Solid(photocraft_doc::Color::BLACK)),
            ..Default::default()
        }),
    );
    let ids = [text.id, shape.id];
    doc.layers = vec![text, shape];
    let mut book = SwatchBook::default();
    book.groups.push(SwatchGroup { id: 1, name: "Brand".into() });
    let w = Swatch {
        id: SwatchId(2),
        name: "Live Ink".into(),
        paint: Fill::Solid(photocraft_doc::Color::BLACK),
        global: true,
        spot: false,
        group: Some(1),
        stops: vec![],
    };
    book.swatches.push(w.clone());
    let targets: Vec<_> = ids.iter().map(|id| PaintTarget { symbol: None, layer: *id, paint: PaintSlot::Fill, stop: None }).collect();
    book.links = targets.iter().map(|target| PaintLink { target: target.clone(), reference: SwatchRef { swatch: w.id, tint: 100. } }).collect();
    doc.swatches = book;
    let access = Access {
        read: |d| Ok(d.swatches.clone()),
        write: |d, b| {
            d.swatches = b;
            Ok(())
        },
    };
    let mut s = Session::new();
    s.add_document(doc, None);
    assert_eq!(s.prefs().toolbar.persona, crate::toolsets::Persona::Pixel);
    let to = photocraft_doc::Color::rgb(0.1, 0.8, 0.3);
    let mut edited = w.clone();
    edited.paint = Fill::Solid(to);
    let capture = Capture::begin(&s).unwrap();
    commit(&mut s, access, &Request { capture, action: Action::Edit { swatch: edited } }).unwrap();
    for t in &targets {
        assert_eq!(color(&s.active().unwrap().doc, t).unwrap(), to);
    }
    let LayerContent::Text(t) = &s.active().unwrap().doc.layer(ids[0]).unwrap().content else { panic!("text") };
    assert_eq!(t.runs[0].style.color, to);
    assert!(t.cache.is_some());
    let LayerContent::Shape(sh) = &s.active().unwrap().doc.layer(ids[1]).unwrap().content else { panic!("shape") };
    assert!(sh.cache.is_some());
    let local = photocraft_doc::Color::rgb(0., 0., 1.);
    let capture = Capture::begin(&s).unwrap();
    commit(&mut s, access, &Request { capture, action: Action::SetColor { targets: vec![targets[1].clone()], color: local } }).unwrap();
    assert_eq!(s.active().unwrap().doc.swatches.links.len(), 1, "local colour detaches that object's live link");
    assert_eq!(color(&s.active().unwrap().doc, &targets[1]).unwrap(), local);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc.swatches.links.len(), 2);
    assert!(s.undo());
    for t in &targets {
        assert_eq!(color(&s.active().unwrap().doc, t).unwrap(), photocraft_doc::Color::BLACK);
    }
}
#[test]
fn shared_swatches_document_move_and_group_rename_are_undoable() {
    let (mut s, b) = super::tests::fixture();
    let access = super::tests::test_access();
    let capture = Capture::begin(&s).unwrap();
    let group = SwatchGroup { id: 2, name: "Folder".into() };
    commit(&mut s, access, &Request { capture, action: Action::Group { group, ids: vec![] } }).unwrap();
    let capture = Capture::begin(&s).unwrap();
    commit(&mut s, access, &Request { capture, action: Action::Move { id: b.swatches[0].id, group: Some(2), before: None } }).unwrap();
    assert_eq!((access.read)(&s.active().unwrap().doc).unwrap().swatches[0].group, Some(2));
    let capture = Capture::begin(&s).unwrap();
    commit(&mut s, access, &Request { capture, action: Action::RenameGroup { id: 2, name: "Renamed".into() } }).unwrap();
    assert_eq!((access.read)(&s.active().unwrap().doc).unwrap().groups[0].name, "Renamed");
    assert!(s.undo());
    assert_eq!((access.read)(&s.active().unwrap().doc).unwrap().groups[0].name, "Folder");
}
#[test]
fn shared_swatches_preferences_restart_old_defaults() {
    let mut s = Session::new();
    let mut book = SwatchBook::default();
    book.groups.push(SwatchGroup { id: 1, name: "My Colours".into() });
    book.swatches.push(Swatch {
        id: SwatchId(2),
        name: "Blue".into(),
        paint: Fill::Solid(photocraft_doc::Color::rgb(0., 0., 1.)),
        global: false,
        spot: false,
        group: Some(1),
        stops: vec![],
    });
    s.edit_prefs(|p| p.user_swatches.palette = book.clone());
    s.execute("prefs.set", json!({"path":"userSwatches.listView", "value":true})).unwrap();
    let json = s.prefs_to_json();
    let mut fresh = Session::new();
    fresh.load_prefs_json(&json).unwrap();
    assert_eq!(fresh.prefs().user_swatches.palette, book);
    fresh.load_prefs_json("{}").unwrap();
    assert!(fresh.prefs().user_swatches.palette.is_empty());
}
