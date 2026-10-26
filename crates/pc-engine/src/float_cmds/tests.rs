use photocraft_geom::Rect;
use serde_json::json;

use super::*;

/// An 80×60 transparent document whose layer is painted red over (10..30)²; (10..30)² selected.
fn session() -> (Session, LayerId) {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 80, "height": 60, "background": "transparent"})).unwrap();
    let id = s.active().unwrap().active_layer.unwrap();
    s.edit("paint", |doc, _| {
        doc.layer_mut(id).unwrap().surface_mut().unwrap().fill_rect(Rect::new(10, 10, 30, 30), &[1.0, 0.0, 0.0, 1.0]);
        Ok(())
    })
    .unwrap();
    s.execute("select.rect", json!({"x": 10, "y": 10, "width": 20, "height": 20})).unwrap();
    (s, id)
}

fn alpha(d: &Document, id: LayerId, x: i32, y: i32) -> f32 {
    d.layer(id).unwrap().surface().unwrap().rgba(x, y)[3]
}

#[test]
fn bugfix_save_o045_floating_moves_protect_a_clean_document_until_returned_or_dropped() {
    let (mut s, _) = session();
    s.active_mut().unwrap().saved_revision = s.active().unwrap().revision;
    let before = s.active().unwrap().doc.clone();
    s.execute("select.float", json!({"dx": 20})).unwrap();
    assert!(s.active().unwrap().is_dirty());
    s.execute("select.float", json!({"dx": -20})).unwrap();
    assert!(!s.active().unwrap().is_dirty());
    s.execute("select.float", json!({"dx": 20})).unwrap();
    drop_floating(&mut s).unwrap();
    assert!(s.active().unwrap().is_dirty());
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.active().unwrap().is_dirty(), "Undo keeps the engine's existing revision-based dirty state");
}

#[test]
fn move_pixels_cut_and_copy_commit_pixels_and_selection_in_one_step() {
    for copy in [false, true] {
        let (mut s, id) = session();
        let before = s.active().unwrap().doc.clone();
        let steps = s.active().unwrap().history.past_len();
        s.execute("select.movePixels", json!({"dx": 30, "dy": 5, "copy": copy})).unwrap();
        let st = s.active().unwrap();
        assert_eq!(st.history.past_len(), steps + 1);
        assert_eq!(alpha(&st.doc, id, 12, 12), if copy { 1.0 } else { 0.0 });
        assert_eq!(alpha(&st.doc, id, 42, 17), 1.0);
        assert_eq!(st.doc.selection.as_ref().unwrap().content_bounds(), Rect::new(40, 15, 60, 35));
        s.undo();
        assert_eq!(*s.active().unwrap().doc, *before);
        s.redo();
        assert_eq!(alpha(&s.active().unwrap().doc, id, 42, 17), 1.0);
    }
}

#[test]
fn move_pixels_respects_locks_and_zero_delta_is_not_an_edit() {
    let (mut s, id) = session();
    let steps = s.active().unwrap().history.past_len();
    s.execute("select.movePixels", json!({"dx": 0, "dy": 0})).unwrap();
    assert_eq!(s.active().unwrap().history.past_len(), steps);
    s.edit("lock", |d, _| {
        d.layer_mut(id).unwrap().locks.position = true;
        Ok(())
    })
    .unwrap();
    assert!(s.execute("select.movePixels", json!({"dx": 5})).is_err());
    assert_eq!(alpha(&s.active().unwrap().doc, id, 12, 12), 1.0);
}

#[test]
fn float_moves_a_cut_piece_without_touching_the_document_until_dropped() {
    let (mut s, id) = session();
    let steps = s.active().unwrap().history.past_len();
    assert_eq!(s.execute("select.float", json!({"dx": 15, "dy": 0})).unwrap()["offset"], json!([15, 0]));
    // Moving it again doesn't cut anything new.
    s.execute("select.float", json!({"dx": 0, "dy": 10})).unwrap();
    let st = s.active().unwrap();
    assert_eq!(floating(st).unwrap().offset, (15, 10));
    assert_eq!(alpha(&st.doc, id, 12, 12), 1.0, "the document is untouched while floating");
    assert_eq!(st.history.past_len(), steps);
    let shown = displayed(st, (0, 0)).unwrap();
    assert!(alpha(&shown, id, 12, 12) == 0.0 && alpha(&shown, id, 27, 22) == 1.0, "shown cut and moved");
    // Drop: one history step, pixels and selection where they were shown.
    s.execute("select.drop", json!({})).unwrap();
    let st = s.active().unwrap();
    assert!(floating(st).is_none());
    assert_eq!(st.history.past_len(), steps + 1);
    for (x, y) in [(12, 12), (27, 22), (44, 39), (40, 40)] {
        assert_eq!(alpha(&st.doc, id, x, y), alpha(&shown, id, x, y), "dropped as shown at ({x}, {y})");
    }
    assert_eq!(st.doc.selection.as_ref().unwrap().content_bounds(), Rect::new(25, 20, 45, 40));
    // Undo takes the whole move back.
    s.undo();
    assert_eq!(alpha(&s.active().unwrap().doc, id, 12, 12), 1.0);
}

#[test]
fn any_other_command_drops_it_and_undo_puts_it_back() {
    let (mut s, id) = session();
    let steps = s.active().unwrap().history.past_len();
    s.execute("select.float", json!({"dx": 20, "dy": 0})).unwrap();
    // Undo while floating: the piece goes back; nothing in the document or history changed.
    assert_eq!(s.execute("edit.undo", json!({})).unwrap()["floating"], json!("returned"));
    let st = s.active().unwrap();
    assert!(floating(st).is_none() && alpha(&st.doc, id, 12, 12) == 1.0 && st.history.past_len() == steps);
    // Float again, then deselect: dropped first (where it was shown), then deselected.
    s.execute("select.float", json!({"dx": 20, "dy": 0})).unwrap();
    s.execute("select.deselect", json!({})).unwrap();
    let st = s.active().unwrap();
    assert!(st.doc.selection.is_none() && floating(st).is_none());
    assert!(alpha(&st.doc, id, 12, 12) == 0.0 && alpha(&st.doc, id, 35, 20) == 1.0);
}

#[test]
fn needs_a_selection_and_an_unlocked_pixel_layer() {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 20, "height": 20})).unwrap();
    assert!(!s.is_enabled("select.float"), "no selection");
    s.execute("select.all", json!({})).unwrap();
    assert!(!s.is_enabled("select.float"), "the Background is locked");
    s.execute("layer.new.layer", json!({})).unwrap();
    assert!(s.is_enabled("select.float"));
}

/// Compare small-piece and full-frame Move previews with the same dev-profile harness.
#[test]
#[ignore]
fn float_drag_bench() {
    for (width, height, depth, select_all) in [(6000, 4000, 8, false), (9504, 6336, 16, true)] {
        let mut session = Session::new();
        session.execute("file.new", json!({"width": width, "height": height, "depth": depth, "background": "transparent"})).unwrap();
        session.execute("layer.new.layer", json!({})).unwrap();
        session.edit("paint", |doc, active| {
            doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap()
                .fill_rect(Rect::new(0, 0, width, height), &[0.2, 0.4, 0.6, 1.0]);
            Ok(())
        }).unwrap();
        if select_all {
            session.execute("select.all", json!({})).unwrap();
        } else {
            session.execute("select.rect", json!({"x": 1000, "y": 1000, "width": 500, "height": 400})).unwrap();
        }
        let start = std::time::Instant::now();
        session.execute("select.float", json!({})).unwrap();
        let cut = start.elapsed();
        let start = std::time::Instant::now();
        for i in 0..20 {
            displayed(session.active().unwrap(), (i * 7, i * 3)).unwrap();
        }
        eprintln!("{width}x{height} {depth}-bit select_all={select_all}: cut {cut:?}, then {:?} per pointer move", start.elapsed() / 20);
    }
}

#[test]
fn ds2_move_pixels_does_not_drop_an_existing_floating_piece() {
    let (mut s, _) = session();
    s.execute("select.float", json!({"dx":20,"dy":5})).unwrap();
    let doc = s.active().unwrap().doc.clone();
    let history = s.active().unwrap().history.past_len();
    for dx in [0, 10] {
        assert!(s.execute("select.movePixels", json!({"dx":dx})).is_err());
        assert!(std::sync::Arc::ptr_eq(&s.active().unwrap().doc, &doc));
        assert_eq!(s.active().unwrap().history.past_len(), history);
        assert_eq!(floating(s.active().unwrap()).unwrap().offset, (20, 5));
    }
}

#[test]
fn float_feathered_selection_preserves_partial_alpha_across_depths() {
    for depth in [8, 16, 32] {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 120, "height": 80, "background": "transparent", "depth": depth})).unwrap();
        let id = s.active().unwrap().active_layer.unwrap();
        s.edit("paint", |doc, _| {
            doc.layer_mut(id).unwrap().surface_mut().unwrap().fill_rect(Rect::new(0, 0, 55, 80), &[1.0, 0.0, 0.0, 1.0]);
            Ok(())
        })
        .unwrap();
        s.execute("select.rect", json!({"x": 20, "y": 20, "width": 30, "height": 30, "feather": 4.0})).unwrap();
        s.execute("select.float", json!({"dx": 40, "dy": 0})).unwrap();
        s.execute("select.drop", json!({})).unwrap();
        let surf = s.active().unwrap().doc.layer(id).unwrap().surface().unwrap();
        // Center of moved piece is opaque red; center of hole is transparent; feathered edge has partial alpha.
        assert!(surf.rgba(75, 35)[3] > 0.95, "{depth}-bit moved center");
        assert!(surf.rgba(35, 35)[3] < 0.05, "{depth}-bit hole center");
        let edge_a = surf.rgba(60, 35)[3];
        assert!(edge_a > 0.1 && edge_a < 0.9, "{depth}-bit feathered edge alpha={edge_a}");
    }
}

#[test]
fn selected_interior_tiles_share_storage_on_cut_and_place() {
    let mut session = Session::new();
    session.execute("file.new", json!({"width": 1024, "height": 1024, "background": "transparent"})).unwrap();
    let id = session.active().unwrap().active_layer.unwrap();
    session.edit("paint", |doc, _| {
        let surface = doc.layer_mut(id).unwrap().surface_mut().unwrap();
        for y in 0..4 {
            for x in 0..4 {
                surface.fill_rect(Rect::new(x * 256, y * 256, (x + 1) * 256, (y + 1) * 256), &[x as f32 / 4.0, y as f32 / 4.0, 0.5, 1.0]);
            }
        }
        Ok(())
    }).unwrap();
    session.execute("select.rect", json!({"x": 100, "y": 100, "width": 800, "height": 800})).unwrap();
    session.execute("select.float", json!({})).unwrap();
    let state = session.active().unwrap();
    let original = state.doc.layer(id).unwrap().surface().unwrap();
    let parts = &floating(state).unwrap().parts;
    let coordinate = photocraft_geom::TileCoord { tx: 1, ty: 1 };
    assert!(Arc::ptr_eq(original.tile(coordinate).unwrap(), parts.piece.tile(coordinate).unwrap()));
    assert!(parts.rest.tile(coordinate).is_none());
    let shown = displayed(state, (256, 0)).unwrap();
    let moved_coordinate = photocraft_geom::TileCoord { tx: 2, ty: 1 };
    assert!(Arc::ptr_eq(original.tile(coordinate).unwrap(), shown.layer(id).unwrap().surface().unwrap().tile(moved_coordinate).unwrap()));
}

#[test]
fn tile_clear_matches_dense_clear_with_soft_and_sparse_selections() {
    use photocraft_color::{ColorMode, SampleType};
    for mode in [ColorMode::Rgb, ColorMode::Cmyk, ColorMode::Lab, ColorMode::Grayscale] {
        for sample in [SampleType::U8, SampleType::U16, SampleType::F32] {
            let format = PixelFormat::new(mode, sample, true);
            let pixel = photocraft_raster::from_rgba(&format, [0.3, 0.7, 0.2, 0.8]);
            for default in [vec![0.0; format.channels()], pixel.clone()] {
                let mut actual = Surface::with_default(format, &default);
                let bounds = Rect::new(-10, -5, 270, 260);
                actual.fill_rect(bounds, &pixel);
                let mut selection = Surface::with_default(PixelFormat::GRAY8, &[0.5]);
                selection.fill_rect(Rect::new(0, 0, 256, 256), &[1.0]);
                selection.fill_rect(Rect::new(-5, -5, 0, 0), &[0.0]);
                let mut expected = actual.clone();
                crate::pixels::clear_surface(&mut expected, bounds, Some(&selection)).unwrap();
                clear_selected_pixels(&mut actual, bounds, &selection).unwrap();
                assert_eq!(actual.read_region(bounds), expected.read_region(bounds), "{mode:?}/{sample:?}");
            }
        }
    }
}
