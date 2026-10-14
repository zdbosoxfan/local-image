//! Shared command metadata and menu model for Library, editor and native menus.
//! Engines keep their own command implementations and bind the generic session/error types.

pub mod command;
pub mod menu;
pub use command::{CommandInfo, CommandSpec, Enabled, Run};

#[cfg(feature = "egui")]
pub mod widgets;

/// UI command declaration shared by both shells: id, label, default shortcut, menu path.
pub type UiCommand = (&'static str, &'static str, Option<&'static str>, &'static str);
