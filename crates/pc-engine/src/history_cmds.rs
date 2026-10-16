//! local-image: the History panel's buttons, as Photoshop has them.
//!
//! - **Create new snapshot** (`history.newSnapshot`): keeps the current state under a name at the
//!   top of the History panel; snapshots outlive the undo limit and are not saved with the file.
//! - **Restore a snapshot** (`history.restoreSnapshot`): brings it back as a new history step, so
//!   the restore itself can be undone.
//! - **Delete current state** (`history.deleteState`): the current state and every state after it
//!   go (an undo whose redo is discarded).
//! - **Create new document from current state** (`history.newDocument`): a layered copy of the
//!   document as it is now, as a new untitled document.

use std::sync::Arc;

use photocraft_doc::Document;
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

/// A named copy of a document state.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub name: String,
    pub doc: Arc<Document>,
}

fn has_doc(s: &Session) -> std::result::Result<(), String> {
    s.active().map(|_| ()).ok_or_else(|| "no document open".into())
}

fn index(p: &Value, s: &Session, cmd: &str) -> Result<usize> {
    let n = s.active().map_or(0, |st| st.snapshots.len());
    let i = p.get("index").and_then(Value::as_u64).ok_or_else(|| EngineError::BadParams { cmd: cmd.into(), msg: "missing `index`".into() })? as usize;
    if i >= n {
        return Err(EngineError::BadParams { cmd: cmd.into(), msg: format!("no snapshot {i} (there are {n})") });
    }
    Ok(i)
}

fn new_snapshot(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.active_mut().ok_or(EngineError::NoDocument)?;
    let name = p
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Snapshot {}", st.snapshots.len() + 1));
    st.snapshots.push(Snapshot { name: name.clone(), doc: st.doc.clone() });
    st.revision += 1;
    Ok(json!({ "name": name, "index": st.snapshots.len() - 1 }))
}

fn restore(s: &mut Session, p: &Value) -> Result<Value> {
    let i = index(p, s, "history.restoreSnapshot")?;
    let snap = s.active().ok_or(EngineError::NoDocument)?.snapshots[i].clone();
    s.edit(&snap.name, |doc, active| {
        *doc = (*snap.doc).clone();
        if active.is_none_or(|id| doc.layer(id).is_none()) {
            *active = doc.top_layer();
        }
        Ok(())
    })?;
    Ok(json!({ "restored": snap.name }))
}

fn delete_snapshot(s: &mut Session, p: &Value) -> Result<Value> {
    let i = index(p, s, "history.deleteSnapshot")?;
    let st = s.active_mut().ok_or(EngineError::NoDocument)?;
    let snap = st.snapshots.remove(i);
    st.revision += 1;
    Ok(json!({ "deleted": snap.name }))
}

fn delete_state(s: &mut Session, _: &Value) -> Result<Value> {
    if !s.undo() {
        return Err(EngineError::Other("there is no earlier state to go back to".into()));
    }
    let st = s.active_mut().ok_or(EngineError::NoDocument)?;
    st.history.clear_redo();
    st.revision += 1;
    Ok(json!({ "states": st.history.past_len() + 1 }))
}

fn new_document(s: &mut Session, _: &Value) -> Result<Value> {
    let name = s.active().map(|st| st.doc.name.clone()).unwrap_or_default();
    let name = format!("{} copy", name.rsplit_once('.').map_or(name.as_str(), |(stem, _)| stem));
    s.execute("image.duplicate", json!({ "name": name }))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "history.newSnapshot",
            label: "New Snapshot",
            menu: &[],
            shortcut: None,
            params: r##"{"name":str?} → {name,index}"##,
            enabled: has_doc,
            run: new_snapshot,
            journal: true,
        },
        CommandSpec {
            id: "history.restoreSnapshot",
            label: "Restore Snapshot",
            menu: &[],
            shortcut: None,
            params: r##"{"index":n}"##,
            enabled: |s| s.active().filter(|st| !st.snapshots.is_empty()).map(|_| ()).ok_or_else(|| "no snapshots".into()),
            run: restore,
            journal: true,
        },
        CommandSpec {
            id: "history.deleteSnapshot",
            label: "Delete Snapshot",
            menu: &[],
            shortcut: None,
            params: r##"{"index":n}"##,
            enabled: |s| s.active().filter(|st| !st.snapshots.is_empty()).map(|_| ()).ok_or_else(|| "no snapshots".into()),
            run: delete_snapshot,
            journal: true,
        },
        CommandSpec {
            id: "history.deleteState",
            label: "Delete Current State",
            menu: &[],
            shortcut: None,
            params: "{}",
            enabled: |s| s.active().filter(|st| st.history.can_undo()).map(|_| ()).ok_or_else(|| "no earlier state".into()),
            run: delete_state,
            journal: true,
        },
        CommandSpec {
            id: "history.newDocument",
            label: "New Document from Current State",
            menu: &[],
            shortcut: None,
            params: "{}",
            enabled: has_doc,
            run: new_document,
            journal: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_restore_as_a_step_and_states_delete() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 40, "height": 30})).unwrap();
        s.execute("layer.new.layer", json!({})).unwrap();
        s.execute("history.newSnapshot", json!({})).unwrap();
        let layers = |s: &Session| s.active().unwrap().doc.walk().len();
        let n0 = layers(&s);
        s.execute("layer.new.layer", json!({})).unwrap();
        s.execute("layer.new.layer", json!({})).unwrap();
        assert_eq!(layers(&s), n0 + 2);
        // Delete current state: one step back, and it can't be redone.
        s.execute("history.deleteState", json!({})).unwrap();
        assert_eq!(layers(&s), n0 + 1);
        assert!(!s.active().unwrap().history.can_redo());
        // Restore the snapshot: a new, undoable step.
        s.execute("history.restoreSnapshot", json!({"index": 0})).unwrap();
        assert_eq!(layers(&s), n0);
        assert!(s.undo());
        assert_eq!(layers(&s), n0 + 1);
        assert!(s.execute("history.restoreSnapshot", json!({"index": 3})).is_err());
        // A new document from the current state.
        let docs = s.documents().len();
        s.execute("history.newDocument", json!({})).unwrap();
        assert_eq!(s.documents().len(), docs + 1);
        s.execute("history.deleteSnapshot", json!({"index": 0})).ok();
    }
}
