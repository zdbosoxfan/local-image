//! Package-owned trace cutout command boundary. See docs/wip/vector-tasks/README.md.
//! Previews are detached; commits use vector_api or Session::edit with journaled specs.
//! No placeholder commands are exposed.
use crate::commands::CommandSpec;
pub(crate) fn specs() -> Vec<CommandSpec> {
    Vec::new()
}
