//! Transient kurbo geometry used by the adapted algorithms. Documents use pc-doc types.
pub mod arc;
pub mod bez;
pub mod hit;
pub mod path;
pub mod shapes;
pub use kurbo::{Affine, BezPath, ParamCurve, PathEl, Point, Rect, Shape, Vec2};
pub use path::{Anchor, AnchorKind, FillRule, PathData, SubPath};
pub const EPS: f64 = 1e-9;
pub mod corners;
