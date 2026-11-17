//! Commands that expand a layer into dense buffers over its content bounds (or the canvas) refuse,
//! before allocating, when those buffers would pass the working budget: a few distant tiles in a
//! small document, or a huge empty canvas, would otherwise ask for terabytes.
#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use photocraft_doc::{LayerId, LayerMask};
use photocraft_geom::Rect;
use serde_json::json;

use crate::Session;

/// A small document whose active layer holds two 10 px blocks two billion pixels apart: two
/// stored tiles, trillions of pixels of content bounds.
fn sparse_session() -> (Session, LayerId) {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 64, "height": 64})).unwrap();
    let id = LayerId(s.execute("layer.new.layer", json!({})).unwrap()["layer"].as_u64().unwrap());
    let doc = Arc::make_mut(&mut s.active_mut().unwrap().doc);
    let surf = doc.layer_mut(id).unwrap().surface_mut().unwrap();
    let px = vec![1.0; surf.channels()];
    surf.fill_rect(Rect::new(0, 0, 10, 10), &px);
    surf.fill_rect(Rect::new(2_000_000_000, 2_000_000_000, 2_000_000_010, 2_000_000_010), &px);
    (s, id)
}

fn refused(s: &mut Session, cmd: &str, p: serde_json::Value) {
    let before = s.active().unwrap().doc.clone();
    let e = s.execute(cmd, p).unwrap_err().to_string();
    assert!(e.contains("working memory"), "{cmd}: {e}");
    assert!(Arc::ptr_eq(&before, &s.active().unwrap().doc), "{cmd} left the document as it was");
}

#[test]
fn apply_layer_mask_over_distant_tiles_is_refused() {
    let (mut s, id) = sparse_session();
    Arc::make_mut(&mut s.active_mut().unwrap().doc).layer_mut(id).unwrap().mask = Some(LayerMask::reveal_all());
    refused(&mut s, "layer.layerMask.apply", json!({"layer": id.0}));
}

#[test]
fn shadows_highlights_over_distant_tiles_is_refused() {
    let (mut s, id) = sparse_session();
    refused(&mut s, "image.adjustments.shadowsHighlights", json!({"layer": id.0}));
}

#[test]
fn multichannel_on_a_huge_empty_canvas_is_refused() {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 40_000, "height": 30_000, "background": "transparent"})).unwrap();
    refused(&mut s, "image.mode.multichannel", json!({}));
}
