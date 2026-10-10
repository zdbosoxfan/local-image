//! Package-owned corners command boundary. See docs/wip/vector-tasks/README.md.
//! Compute previews outside Session; publish via vector_api or Session::edit, with one
//! journaled CommandSpec per committed operation. No placeholder commands are exposed.

use crate::commands::CommandSpec;
pub(crate) fn specs() -> Vec<CommandSpec> {
    Vec::new()
}
