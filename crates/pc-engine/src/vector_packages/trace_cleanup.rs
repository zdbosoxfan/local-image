//! Detached cleanup evaluator and guarded publication helper for the trace dialog.
//! Public layer.vectorize command IDs and dialog routing live in the trace dialog.
mod transaction;
use crate::commands::CommandSpec;
pub use pc_trace::cleanup::{Options, Output, Stats, Upscale};
pub use transaction::{Capture, Prepared, capture, commit, prepare};

pub fn evaluate(
    image: pc_trace::Image<'_>,
    params: &pc_trace::Params,
    options: &Options,
    cancel: impl Fn() -> bool,
) -> std::result::Result<Output, pc_trace::Error> {
    pc_trace::cleanup::trace_with_cancel(image, params, options, cancel)
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    Vec::new()
}
