//! Device-space wireframes over the shared feature projection. Original GPL-3.0-or-later
//! code; Illustrator/Affinity wireframe semantics, no upstream renderer or paint copied.
use photocraft_color::PixelFormat;
use photocraft_doc::{Color, Fill, Path, ShapeLayer, ShapeStroke, Subpath};
use photocraft_geom::{Affine, Point, Rect};
use photocraft_raster::Surface;

pub const MAX_KNOTS: usize = 262_144;
pub const MAX_DEVICE_COORDINATE: f64 = 100_000_000.;
/// A compiled one-device-pixel stroke. The input is already feature-projected geometry;
/// original fills, stroke widths, dashes, boolean operations and effects never participate.
pub struct Outline {
    pub path: Path,
    compiled: crate::CompiledShape,
    polylines: Vec<crate::Polyline>,
}
impl Outline {
    pub fn new(path: &Path, document_to_device: Affine, clip: Rect) -> Result<Self, String> {
        if document_to_device.m.iter().any(|v| !v.is_finite()) || document_to_device.inverse().is_none() {
            return Err("invalid outline view transform".into());
        }
        if path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>() > MAX_KNOTS {
            return Err("outline geometry exceeds knot budget".into());
        }
        // Validate BEFORE flattening or integer conversion, including unused handles.
        let mut path = path.transform(&document_to_device);
        if path
            .subpaths
            .iter()
            .flat_map(|s| &s.knots)
            .any(|k| [k.anchor, k.in_ctrl, k.out_ctrl].iter().any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs().max(p.y.abs()) > MAX_DEVICE_COORDINATE))
        {
            return Err("outline geometry must be finite and bounded".into());
        }
        // Bound the existing flattener's allocation before invoking it. The cubic's
        // second-derivative bound controls its uniform subdivision count. Straight native
        // segments (endpoint controls) need one piece regardless of their length.
        let mut pieces = 0usize;
        for sub in &path.subpaths {
            for [a, c, d, b] in sub.segments() {
                let count = if a == c && d == b {
                    1
                } else {
                    let size = (c.x - a.x).hypot(c.y - a.y) + (d.x - c.x).hypot(d.y - c.y) + (b.x - d.x).hypot(b.y - d.y);
                    let curvature = (a.x - 2. * c.x + d.x).hypot(a.y - 2. * c.y + d.y).max((c.x - 2. * d.x + b.x).hypot(c.y - 2. * d.y + b.y));
                    let tol = crate::DEFAULT_TOLERANCE.min(size * 1e-3).max(1e-7);
                    ((0.75 * curvature / tol).sqrt().ceil() as usize).clamp(1, 1 << 16)
                };
                pieces = pieces.saturating_add(count + 1);
                if pieces > 1_048_576 {
                    return Err("outline flattened geometry exceeds point budget".into());
                }
            }
        }
        path.inverted = false;
        for subpath in &mut path.subpaths {
            subpath.op = photocraft_doc::PathOp::Combine;
        }
        let shape = ShapeLayer {
            path: path.clone(),
            fill: None,
            stroke: Some(ShapeStroke { width: 1., paint: Fill::Solid(Color::BLACK), ..Default::default() }),
            ..Default::default()
        };
        let compiled = crate::CompiledShape::new(&shape, crate::DEFAULT_TOLERANCE, clip);
        let polylines = crate::flatten_path(&path, crate::DEFAULT_TOLERANCE);
        Ok(Self { path, compiled, polylines })
    }
    pub fn render(&self, format: PixelFormat, clip: Rect) -> Result<Surface, String> {
        if u64::from(clip.width()) * u64::from(clip.height()) > 16_777_216 {
            return Err("outline viewport exceeds pixel budget".into());
        }
        Ok(self.compiled.render(format, clip))
    }
    pub fn bounds(&self) -> Option<Rect> {
        self.compiled.bounds()
    }
    /// Distance to actual path segments, never fill coverage or an implicit closing edge.
    pub fn hit(&self, point: Point, tolerance: f64) -> Result<bool, String> {
        if !point.x.is_finite() || !point.y.is_finite() || !tolerance.is_finite() || !(0.0..=64.).contains(&tolerance) {
            return Err("invalid outline hit input".into());
        }
        Ok(self.polylines.iter().any(|poly| {
            let n = poly.pts.len();
            if n == 1 {
                return (point.x - poly.pts[0].0).hypot(point.y - poly.pts[0].1) <= tolerance;
            }
            (0..if poly.closed { n } else { n.saturating_sub(1) }).any(|i| {
                let (ax, ay) = poly.pts[i];
                let (bx, by) = poly.pts[(i + 1) % n];
                let (dx, dy) = (bx - ax, by - ay);
                let sq = dx * dx + dy * dy;
                let t = if sq == 0. { 0. } else { ((point.x - ax) * dx + (point.y - ay) * dy) / sq }.clamp(0., 1.);
                (point.x - ax - t * dx).hypot(point.y - ay - t * dy) <= tolerance
            })
        }))
    }
}
/// Raster/smart images are represented by their geometric box and two diagonals.
pub fn raster_box(bounds: Rect) -> Path {
    if bounds.is_empty() {
        return Path::default();
    }
    let (x0, y0, x1, y1) = (f64::from(bounds.x0), f64::from(bounds.y0), f64::from(bounds.x1), f64::from(bounds.y1));
    Path::new(vec![
        Subpath::polygon(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]),
        Subpath::polyline(&[(x0, y0), (x1, y1)]),
        Subpath::polyline(&[(x1, y0), (x0, y1)]),
    ])
}
#[cfg(test)]
mod tests;
