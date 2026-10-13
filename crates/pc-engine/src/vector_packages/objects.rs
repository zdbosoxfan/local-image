//! Document-wide vector visibility/lock recovery, one normal undo transaction.
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::Locks;
use serde_json::{Value, json};

fn eligible(s: &Session, unlock: bool) -> std::result::Result<(), String> {
    let doc = &s.active().ok_or("Open a document first")?.doc;
    if doc.walk().iter().any(|(_, _, layer)| if unlock { layer.locks != Locks::default() } else { !layer.visible }) {
        Ok(())
    } else {
        Err(if unlock { "No locked objects" } else { "No hidden objects" }.into())
    }
}
fn run(s: &mut Session, unlock: bool) -> Result<Value> {
    eligible(s, unlock).map_err(EngineError::Other)?;
    let ids: Vec<_> = s
        .active()
        .ok_or(EngineError::NoDocument)?
        .doc
        .walk()
        .iter()
        .filter(|(_, _, layer)| if unlock { layer.locks != Locks::default() } else { !layer.visible })
        .map(|(_, _, layer)| layer.id)
        .collect();
    let count = ids.len();
    s.edit(if unlock { "Unlock All" } else { "Show All" }, move |doc, _| {
        for id in ids {
            let layer = doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?;
            if unlock {
                layer.locks = Locks::default();
            } else {
                layer.visible = true;
            }
        }
        Ok(())
    })?;
    Ok(json!({"changed":count}))
}
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "vector.object.unlockAll",
            label: "Unlock All",
            menu: &[],
            shortcut: None,
            params: "{}",
            enabled: |s| eligible(s, true),
            run: |s, _| run(s, true),
            journal: true,
        },
        CommandSpec {
            id: "vector.object.showAll",
            label: "Show All",
            menu: &[],
            shortcut: None,
            params: "{}",
            enabled: |s| eligible(s, false),
            run: |s, _| run(s, false),
            journal: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use photocraft_doc::{ColorMode, Document, Layer, LayerContent, SampleType, ShapeLayer, Size};
    #[test]
    fn vectorws_unlock_show_all_nested_objects_preserve_selection_and_undo() {
        let mut s = Session::new();
        let mut doc = Document::new("Objects", Size::new(32, 32), ColorMode::Rgb, SampleType::U8);
        let mut shape = Layer::new("Shape", LayerContent::Shape(ShapeLayer::default()));
        shape.locks.position = true;
        shape.visible = false;
        let id = shape.id;
        let mut group = Layer::group("Group", vec![shape]);
        group.locks.all = true;
        group.visible = false;
        doc.layers.push(group);
        s.add_document(doc, None);
        s.select_layer(id).unwrap();
        let before = s.active().unwrap().doc.clone();
        assert!(s.is_enabled("vector.object.unlockAll"));
        assert_eq!(s.execute("vector.object.unlockAll", json!({})).unwrap()["changed"], 2);
        let unlocked = s.active().unwrap().doc.clone();
        assert!(!s.is_enabled("vector.object.unlockAll"));
        assert!(s.is_enabled("vector.object.showAll"));
        assert_eq!(s.execute("vector.object.showAll", json!({})).unwrap()["changed"], 2);
        assert!(s.active().unwrap().doc.walk().iter().all(|(_, _, layer)| layer.visible));
        assert_eq!(s.active().unwrap().active_layer, Some(id));
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc, unlocked);
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc, before);
    }
}
