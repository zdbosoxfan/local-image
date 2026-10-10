//! V3 live shape/corner parameters. This is a data boundary, not a geometry implementation.
//! Coordinates are document pixels, angles radians. Corners address the unexpanded base path;
//! inserting/deleting/reordering knots must remap these indices in the same engine edit.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CornerKind {
    #[default]
    Round,
    Inverted,
    Chamfer,
    Straight,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Corner {
    pub radius: f64,
    pub kind: CornerKind,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct KnotCorner {
    pub subpath: usize,
    pub knot: usize,
    pub corner: Corner,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesignShape {
    pub rect: [f64; 4],
    #[serde(default, skip_serializing_if = "is_zero")]
    pub angle: f64,
    pub parameters: ShapeParameters,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ShapeParameters {
    Rect { radii: [f64; 4], kinds: [CornerKind; 4] },
    Ellipse,
    Polygon { sides: u32, curved: f64, corner: f64 },
    Star { points: u32, inner: f64, corner: f64 },
    DoubleStar { points: u32, inner: f64, secondary: f64, corner: f64 },
    SquareStar { points: u32, inner: f64, corner: f64 },
    Triangle { top: f64 },
    Trapezoid { top_left: f64, top_right: f64 },
    Diamond { top: f64 },
    Donut { hole: f64 },
    Pie { start: f64, end: f64 },
    Segment { start: f64, end: f64, inner: f64 },
    Arrow { from: [f64; 2], to: [f64; 2], head_width: f64, head_length: f64, shaft: f64, double: bool },
    Callout { ellipse: bool, tail: [f64; 2], radius: f64 },
    Cog { teeth: u32, inner: f64, hole: f64, curve: f64 },
    Heart { depth: f64 },
    Tear { bend: f64 },
    Crescent { offset: f64 },
    Cloud { bubbles: u32 },
    Spiral { center: [f64; 2], radius: f64, decay: f64, turns: f64 },
    Arc { start: f64, end: f64 },
    Line { from: [f64; 2], to: [f64; 2] },
    RectGrid { rows: u32, cols: u32 },
    PolarGrid { rings: u32, spokes: u32 },
}
fn is_zero(v: &f64) -> bool {
    *v == 0.0
}
