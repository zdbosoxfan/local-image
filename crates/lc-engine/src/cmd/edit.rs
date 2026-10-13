use serde_json::json;

use super::{CommandSpec, can_redo, can_undo, cmd};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("edit.undo", "Undo", ["Edit"], Some("Cmd+Z"), "{}", can_undo, |s, _| {
            s.end_interaction()?;
            let undone = s.undo_step()?;
            crate::cmd::match_look::reconcile_applied(s);
            Ok(json!({"undone": undone}))
        }),
        cmd!("edit.redo", "Redo", ["Edit"], Some("Cmd+Shift+Z"), "{}", can_redo, |s, _| {
            s.end_interaction()?;
            let redone = s.redo_step()?;
            crate::cmd::match_look::restore_applied(s);
            Ok(json!({"redone": redone}))
        }),
    ]
}
