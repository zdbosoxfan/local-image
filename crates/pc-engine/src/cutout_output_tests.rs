use super::*;
use photocraft_color::PixelFormat;
use photocraft_geom::Rect;

const AREA: Rect = Rect::new(0, 0, 5, 3);

fn scene() -> (Session, LayerId, Surface) {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 5, "height": 3, "depth": 32})).unwrap();
    let id = s.active().unwrap().active_layer.unwrap();
    s.edit("synthetic pixels", |doc, _| {
        let pixels = doc.layer_mut(id).unwrap().surface_mut().unwrap();
        let data: Vec<_> = (0..15).flat_map(|i| [i as f32 / 15.0, 0.3, 0.8, 0.75]).collect();
        pixels.write_region(AREA, &data);
        Ok(())
    })
    .unwrap();
    s.execute("select.all", json!({})).unwrap();
    let mut mask = Surface::new(PixelFormat::GRAY8);
    mask.write_region(AREA, &[0.0, 1.0, 0.5, 0.25, 0.0].repeat(3));
    (s, id, mask)
}

fn apply(s: &mut Session, id: LayerId, matte: Surface, output: &BackgroundOutput) -> LayerId {
    s.edit("Remove Background", |doc, active| {
        let result = apply_output(doc, id, matte, output, &JobCtx::new())?;
        *active = Some(result);
        Ok(result)
    })
    .unwrap()
}

fn verify(output: BackgroundOutput) {
    let (mut s, id, matte) = scene();
    let before = s.active().unwrap().doc.clone();
    let history = s.active().unwrap().history.past_len();
    let pixels = before.layer(id).unwrap().surface().unwrap().clone();
    let coverage = matte.sample_channel(2, 0, 0);
    let result = apply(&mut s, id, matte, &output);
    let st = s.active().unwrap();
    assert_eq!(st.history.past_len(), history + 1);
    assert_eq!(st.active_layer, Some(result));
    assert!(st.doc.selection.is_none());
    let l = st.doc.layer(result).unwrap();
    match output {
        BackgroundOutput::Transparent => {
            assert!(l.mask.is_none());
            let p = l.surface().unwrap().read_region(AREA);
            assert_eq!(p[3], 0.0);
            assert!((p[7] - 0.75).abs() < 1e-6);
            assert!((p[11] - 0.75 * coverage).abs() < 1e-6);
            assert!((p[8] - 2.0 / 15.0).abs() < 1e-6, "RGB stays straight, without dark fringes");
            assert_eq!(st.doc.layers.len(), 1);
        }
        _ => {
            assert_eq!(l.surface().unwrap(), &pixels);
            assert_eq!(l.mask.as_ref().unwrap().value(0, 0), 0.0);
            assert_eq!(l.mask.as_ref().unwrap().value(1, 0), 1.0);
            assert_eq!(l.mask.as_ref().unwrap().value(2, 0), coverage);
            match output {
                BackgroundOutput::White | BackgroundOutput::Color(_) => {
                    assert_eq!(st.doc.layers.len(), 2);
                    assert_eq!(st.doc.layers[1].id, id);
                    let expected = match output {
                        BackgroundOutput::Color(c) => c,
                        _ => [1.0; 4],
                    };
                    assert_eq!(st.doc.layers[0].content, LayerContent::Fill(Fill::Solid(Color::rgba(expected[0], expected[1], expected[2], expected[3]))));
                    let rendered = photocraft_compose::render(&st.doc, Rect::new(0, 0, 1, 1)).px[0];
                    for (got, want) in rendered.iter().zip(expected) {
                        assert!((*got - want).abs() < 1e-5, "{rendered:?} != {expected:?}");
                    }
                }
                BackgroundOutput::Blur(radius) => {
                    assert_eq!(st.doc.layers.len(), 2);
                    assert_eq!(st.doc.layers[1].id, id);
                    assert!(st.doc.layers[0].mask.is_none());
                    assert_eq!(st.doc.layers[0].surface().unwrap().format(), pixels.format());
                    let expected = photocraft_algo::apply_in(&pixels, &photocraft_algo::FilterParams::GaussianBlur { radius }, AREA, AREA, None, AREA);
                    assert_eq!(st.doc.layers[0].surface().unwrap(), &expected);
                    assert_ne!(expected.read_region(AREA), pixels.read_region(AREA));
                }
                BackgroundOutput::NewLayer => {
                    assert_ne!(result, id);
                    assert_eq!(st.doc.layers.len(), 2);
                    let mut original = before.layer(id).unwrap().clone();
                    original.visible = false;
                    assert_eq!(st.doc.layer(id).unwrap(), &original, "original pixels, mask and properties retained");
                    assert_eq!(st.doc.layers[0].id, id);
                    assert!(st.doc.layers[1].visible);
                }
                BackgroundOutput::Mask | BackgroundOutput::Transparent => {
                    assert_eq!(st.doc.layers.len(), 1);
                    let corner = photocraft_compose::render(&st.doc, Rect::new(0, 0, 1, 1)).px[0];
                    assert_eq!(corner[3], 0.0);
                }
            }
        }
    }
    let after = st.doc.clone();
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before, "the entire output undoes at once");
    assert_eq!(s.active().unwrap().active_layer, Some(id));
    assert!(s.redo());
    assert_eq!(s.active().unwrap().doc, after);
    assert_eq!(s.active().unwrap().active_layer, Some(result));
}

#[test]
fn background_output_mask() {
    verify(BackgroundOutput::Mask);
}
#[test]
fn background_output_transparent() {
    verify(BackgroundOutput::Transparent);
}
#[test]
fn background_output_white() {
    verify(BackgroundOutput::White);
}
#[test]
fn background_output_color() {
    verify(BackgroundOutput::Color([0.2, 0.4, 0.6, 1.0]));
}
#[test]
fn background_output_blur() {
    verify(BackgroundOutput::Blur(1.5));
}
#[test]
fn background_output_new_layer() {
    verify(BackgroundOutput::NewLayer);
}

#[test]
fn background_output_validates_before_either_engine_runs() {
    assert_eq!(BackgroundOutput::from_params(&json!({}), CMD).unwrap(), BackgroundOutput::Mask);
    assert_eq!(BackgroundOutput::from_params(&json!({"output":"blur"}), CMD).unwrap(), BackgroundOutput::Blur(12.0));
    assert_eq!(BackgroundOutput::from_params(&json!({"output":"color", "color":"#336699"}), CMD).unwrap(), BackgroundOutput::Color([0.2, 0.4, 0.6, 1.0]));
    for cmd in [CMD, "ai.removeBackground"] {
        for p in [
            json!({"output":null}),
            json!({"output":"unknown"}),
            json!({"output":"color"}),
            json!({"output":"color", "color":[0.0,"bad",1.0]}),
            json!({"output":"blur","amount":-1}),
            json!({"output":"blur","amount":1001}),
        ] {
            let (mut s, _, _) = scene();
            let before = s.active().unwrap().doc.clone();
            assert!(matches!(s.execute(cmd, p), Err(EngineError::BadParams { .. })), "{cmd}");
            assert_eq!(s.active().unwrap().doc, before);
        }
    }
}

#[test]
fn background_output_transparent_adds_alpha_to_rgb_surfaces() {
    let (mut s, id, matte) = scene();
    s.edit("RGB only", |doc, _| {
        let l = doc.layer_mut(id).unwrap();
        let mut fmt = l.surface().unwrap().format();
        fmt.alpha = false;
        l.content = LayerContent::Raster(l.surface().unwrap().convert(fmt));
        Ok(())
    })
    .unwrap();
    apply(&mut s, id, matte, &BackgroundOutput::Transparent);
    let pixels = s.active().unwrap().doc.layer(id).unwrap().surface().unwrap();
    assert!(pixels.format().alpha);
    assert_eq!(pixels.sample_channel(0, 0, 3), 0.0);
    assert_eq!(pixels.sample_channel(1, 0, 3), 1.0);
    assert_eq!(pixels.rgba(1000, 1000)[3], 0.0, "the mask hides unallocated RGB pixels too");
}

#[test]
fn background_output_stays_in_the_source_group() {
    for output in [BackgroundOutput::White, BackgroundOutput::Blur(0.0), BackgroundOutput::NewLayer] {
        let (mut s, id, matte) = scene();
        let group = s
            .edit("group", |doc, _| {
                let layer = doc.remove(id).unwrap();
                Ok(doc.insert_above(None, Layer::group("Photo", vec![layer])))
            })
            .unwrap();
        let before = s.active().unwrap().doc.clone();
        apply(&mut s, id, matte, &output);
        let doc = &s.active().unwrap().doc;
        assert_eq!(doc.layers.len(), 1);
        let siblings = doc.layer(group).unwrap().children().unwrap();
        assert_eq!(siblings.len(), 2);
        assert_eq!(siblings[if output == BackgroundOutput::NewLayer { 0 } else { 1 }].id, id);
        s.undo();
        assert_eq!(s.active().unwrap().doc, before);
    }
}

#[test]
fn background_output_blur_reveals_the_background_after_a_previous_mask() {
    let (mut s, id, matte) = scene();
    s.edit("previous mask", |doc, _| {
        doc.layer_mut(id).unwrap().mask = Some(LayerMask::hide_all());
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().doc.clone();
    apply(&mut s, id, matte, &BackgroundOutput::Blur(1.5));
    let doc = &s.active().unwrap().doc;
    assert!(doc.layers[0].mask.is_none());
    assert!(photocraft_compose::render(doc, Rect::new(0, 0, 1, 1)).px[0][3] > 0.5);
    assert_eq!(doc.layer(id).unwrap().surface(), before.layer(id).unwrap().surface());
    s.undo();
    assert_eq!(s.active().unwrap().doc, before);
}
