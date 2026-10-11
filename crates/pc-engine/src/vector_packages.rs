//! Compiled registration slots for independent vector packages. Each package owns its
//! module; the coordinator owns this aggregate. Empty slots deliberately advertise no
//! command. See docs/wip/vector-tasks/README.md for ownership and integration.
use crate::commands::CommandSpec;
pub mod affinity;
pub mod anchor_tools;
pub mod appearance;
pub mod blend;
pub mod brushes;
pub mod clip_compound;
pub mod constraints;
pub mod constructive;
pub mod corners;
pub mod drawing;
pub mod envelope;
pub mod layout;
pub mod node;
pub mod path_text;
pub mod pdf_export;
pub mod pdf_import;
pub mod pen;
pub mod pencil;
pub mod repeat;
pub mod snapping;
pub mod svg_export;
pub mod svg_import;
pub mod symbols;
pub mod trace_centerline;
pub mod trace_cleanup;
pub mod trace_cutout;
pub mod trace_output;
pub mod width;

pub(crate) fn specs() -> Vec<CommandSpec> {
    let mut out = Vec::new();
    out.extend(drawing::specs());
    out.extend(node::specs());
    out.extend(pen::specs());
    out.extend(pencil::specs());
    out.extend(corners::specs());
    out.extend(snapping::specs());
    out.extend(appearance::specs());
    out.extend(constructive::specs());
    out.extend(width::specs());
    out.extend(path_text::specs());
    out.extend(symbols::specs());
    out.extend(constraints::specs());
    out.extend(layout::specs());
    out.extend(brushes::specs());
    out.extend(blend::specs());
    out.extend(repeat::specs());
    out.extend(envelope::specs());
    out.extend(svg_import::specs());
    out.extend(svg_export::specs());
    out.extend(pdf_export::specs());
    out.extend(pdf_import::specs());
    out.extend(affinity::specs());
    out.extend(trace_centerline::specs());
    out.extend(trace_cleanup::specs());
    out.extend(trace_cutout::specs());
    out.extend(trace_output::specs());
    out.extend(clip_compound::specs());
    out
}
