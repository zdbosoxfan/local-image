//! Compatibility API for the shared Euclidean distance transform.
pub use li_matte::distance::edt;

#[cfg(test)]
const EMPTY_DISTANCE: f32 = 1e10;
#[cfg(test)]
mod tests;
