#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("image dimensions must be nonzero")]
    EmptyImage,
    #[error("RGBA buffer length does not match dimensions")]
    InvalidBuffer,
    #[error("image exceeds the {0} pixel tracing budget")]
    PixelBudget(usize),
    #[error("invalid trace parameter: {0}")]
    InvalidParams(&'static str),
    #[error("no unused transparency key colour found")]
    NoKeyColor,
    #[error("trace cancelled")]
    Cancelled,
    #[error("tracing complexity exceeds the boundary budget")]
    BoundaryBudget,
}
