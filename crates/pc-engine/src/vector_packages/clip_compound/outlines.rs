//! Reuse pc-text's native outline adapter; do not add another text shaper.
use super::super::path_text::on_path;
use photocraft_doc::Path;
#[path = "../path_text/curves.rs"]
#[expect(
    clippy::duplicate_mod,
    reason = "Package 12's outline adapter is private; reuse its single source until 00 exposes it, without editing another package"
)]
mod native;

pub fn path(paints: &[on_path::PaintedOutline]) -> Path {
    let mut path = Path::default();
    for (_, part) in native::paths(paints) {
        path.subpaths.extend(part.subpaths);
    }
    path
}
