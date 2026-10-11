//! Correspondence adapted from VectorCraft doc/blend.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Local Image addition: match joined contours by containment depth and normalized position,
//! preserving ordered PSD components rather than accidentally pairing an outer with a hole.
use super::{MAX_KNOTS, Result, error, validate_path};
use photocraft_doc::{Knot, Path, PathOp, Subpath};
use photocraft_pathops::geom::{self, ParamCurve as _, Shape as _};

#[derive(Clone, Debug)]
pub struct PathPair {
    original_a: Path,
    original_b: Path,
    pairs: Vec<(geom::SubPath, geom::SubPath, PathOp)>,
}
fn bounds(s: &geom::SubPath) -> geom::Rect {
    geom::PathData::single(s.clone()).bounds().unwrap_or(geom::Rect::ZERO)
}
fn depth(subs: &[geom::SubPath], i: usize) -> usize {
    let Some(p) = subs[i].anchors.first().map(|a| a.p) else { return 0 };
    let area = subs[i].signed_area().abs();
    subs.iter()
        .enumerate()
        .filter(|(j, other)| {
            *j != i && other.closed && other.signed_area().abs() > area + 1e-9 && geom::PathData::single((*other).clone()).to_bezpath().winding(p) != 0
        })
        .count()
}
fn normal(s: &geom::SubPath) -> Vec<geom::Vec2> {
    let b = bounds(s);
    let c = b.center();
    let scale = b.width().max(b.height()).max(1e-9);
    s.anchors.iter().map(|a| (a.p - c) / scale).collect()
}
fn best_rotation(a: &geom::SubPath, b: &geom::SubPath) -> usize {
    let (a, b) = (normal(a), normal(b));
    let n = a.len().min(b.len());
    if n < 2 {
        return 0;
    }
    let cost = |r: usize| (0..n).map(|i| (a[i] - b[(i + r) % n]).hypot2()).sum::<f64>();
    let stride = n.div_ceil(512);
    let best = (0..n).step_by(stride).min_by(|x, y| cost(*x).total_cmp(&cost(*y))).unwrap_or(0);
    (best + n - stride..=best + n + stride).map(|r| r % n).min_by(|x, y| cost(*x).total_cmp(&cost(*y))).unwrap_or(best)
}
fn grow(s: &mut geom::SubPath, n: usize) {
    while s.anchors.len() < n {
        if s.segment_count() == 0 {
            let p = s.anchors.last().map_or(geom::Point::ZERO, |a| a.p);
            s.anchors.push(geom::Anchor::corner(p));
        } else {
            let size = |i| {
                let c = s.segment(i);
                (c.p1 - c.p0).hypot() + (c.p2 - c.p1).hypot() + (c.p3 - c.p2).hypot()
            };
            let i = (0..s.segment_count()).max_by(|i, j| size(*i).total_cmp(&size(*j))).unwrap_or(0);
            s.insert_anchor(i, 0.5);
        }
    }
}
pub(super) fn section(s: &geom::SubPath, i: usize, from: f64, to: f64) -> geom::SubPath {
    let c = s.segment(i).subsegment(from..to);
    geom::SubPath::new(
        vec![
            geom::Anchor { p: c.p0, h_in: c.p0, h_out: c.p1, kind: geom::AnchorKind::Smooth },
            geom::Anchor { p: c.p3, h_in: c.p2, h_out: c.p3, kind: geom::AnchorKind::Smooth },
        ],
        false,
    )
}
fn lengths(s: &geom::SubPath) -> Vec<f64> {
    let mut out = vec![0.];
    for i in 0..s.segment_count() {
        let length = geom::PathData::single(section(s, i, 0., 1.)).length();
        out.push(out.last().copied().unwrap_or(0.) + length);
    }
    out
}
fn resample(s: &geom::SubPath, lengths: &[f64], positions: &[f64]) -> geom::SubPath {
    let total = lengths.last().copied().unwrap_or(0.);
    let mut anchors = Vec::<geom::Anchor>::new();
    for w in positions.windows(2) {
        let start = w[0] * total;
        let end = w[1] * total;
        let i = lengths.partition_point(|v| *v <= start + 1e-9).saturating_sub(1).min(s.segment_count() - 1);
        let parameter = |distance: f64| {
            if distance <= lengths[i] + 1e-9 {
                return 0.;
            }
            if distance >= lengths[i + 1] - 1e-9 {
                return 1.;
            }
            let (mut lo, mut hi) = (0., 1.);
            for _ in 0..32 {
                let t = (lo + hi) * 0.5;
                let length = geom::PathData::single(section(s, i, 0., t)).length();
                if length < distance - lengths[i] {
                    lo = t;
                } else {
                    hi = t;
                }
            }
            (lo + hi) * 0.5
        };
        let part = section(s, i, parameter(start), parameter(end));
        if let Some(last) = anchors.last_mut() {
            last.h_out = part.anchors[0].h_out;
        } else {
            anchors.push(part.anchors[0]);
        }
        anchors.push(part.anchors[1]);
    }
    if s.closed && anchors.len() > 1 {
        let last = anchors.pop();
        if let (Some(last), Some(first)) = (last, anchors.first_mut()) {
            first.h_in = last.h_in;
        }
    }
    geom::SubPath::new(anchors, s.closed)
}
fn common_samples(a: &mut geom::SubPath, b: &mut geom::SubPath) -> Result<()> {
    let (la, lb) = (lengths(a), lengths(b));
    let (ta, tb) = (la.last().copied().unwrap_or(0.), lb.last().copied().unwrap_or(0.));
    if ta < 1e-9 || tb < 1e-9 {
        let n = a.anchors.len().max(b.anchors.len());
        grow(a, n);
        grow(b, n);
        return Ok(());
    }
    let mut positions: Vec<_> = la.iter().map(|v| v / ta).chain(lb.iter().map(|v| v / tb)).collect();
    positions.sort_by(f64::total_cmp);
    positions.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    if positions.len() > MAX_KNOTS {
        return Err(error("blend resampling exceeds knot budget"));
    }
    *a = resample(a, &la, &positions);
    *b = resample(b, &lb, &positions);
    Ok(())
}
fn start(s: &mut geom::SubPath, index: Option<usize>) -> Result<()> {
    if let Some(i) = index {
        if i >= s.anchors.len() {
            return Err(error("blend start anchor is out of range"));
        }
        if s.closed {
            s.anchors.rotate_left(i);
        } else if i + 1 == s.anchors.len() {
            s.reverse();
        } else if i != 0 {
            return Err(error("open blend starts must be endpoints"));
        }
    }
    Ok(())
}
fn prepare(mut a: geom::SubPath, mut b: geom::SubPath, starts: (Option<usize>, Option<usize>)) -> Result<(geom::SubPath, geom::SubPath)> {
    start(&mut a, starts.0)?;
    start(&mut b, starts.1)?;
    if a.closed && b.closed && a.signed_area() * b.signed_area() < 0. {
        b.reverse();
        b.anchors.rotate_right(1);
    }
    if a.closed && b.closed && starts == (None, None) {
        let r = if a.anchors.len() == b.anchors.len() {
            best_rotation(&a, &b)
        } else {
            let (qa, qb) = (normal(&a), normal(&b));
            qb.iter().enumerate().min_by(|(_, x), (_, y)| (**x - qa[0]).hypot2().total_cmp(&(**y - qa[0]).hypot2())).map_or(0, |(i, _)| i)
        };
        b.anchors.rotate_left(r);
    } else if !a.closed && !b.closed && starts == (None, None) && a.anchors.len() > 1 && b.anchors.len() > 1 {
        let (qa, qb) = (normal(&a), normal(&b));
        let forward = (qa[0] - qb[0]).hypot2() + (qa[qa.len() - 1] - qb[qb.len() - 1]).hypot2();
        let reverse = (qa[0] - qb[qb.len() - 1]).hypot2() + (qa[qa.len() - 1] - qb[0]).hypot2();
        if reverse < forward {
            b.reverse();
        }
    }
    if a.anchors.len() != b.anchors.len() {
        common_samples(&mut a, &mut b)?;
    }
    Ok((a, b))
}
fn collapse(like: &geom::SubPath, center: geom::Point) -> geom::SubPath {
    geom::SubPath::new(vec![geom::Anchor::corner(center); like.anchors.len().max(1)], like.closed)
}
impl PathPair {
    pub fn new(a: &Path, b: &Path) -> Result<Self> {
        Self::with_starts(a, b, (None, None))
    }
    /// Explicit first-contour start anchors win over the automatic least-twist match.
    /// The shared persisted BlendSpec has no clicked-start field yet (00's schema lane).
    pub fn with_starts(a: &Path, b: &Path, starts: (Option<usize>, Option<usize>)) -> Result<Self> {
        validate_path(a)?;
        validate_path(b)?;
        if a.inverted != b.inverted {
            return Err(error("blend keys must share fill inversion"));
        }
        let (ga, gb) = (photocraft_pathops::to_geometry(a), photocraft_pathops::to_geometry(b));
        let (ca, cb) = (a.components(), b.components());
        let (center_a, center_b) = (ga.bounds().map_or(geom::Point::ZERO, |b| b.center()), gb.bounds().map_or(geom::Point::ZERO, |b| b.center()));
        let mut pairs = Vec::new();
        let mut knots = 0;
        for component in 0..ca.len().max(cb.len()) {
            let ra = ca.get(component).cloned().unwrap_or(0..0);
            let rb = cb.get(component).cloned().unwrap_or(0..0);
            let op_a = (!ra.is_empty()).then(|| a.effective_op(ra.start));
            let op_b = (!rb.is_empty()).then(|| b.effective_op(rb.start));
            if op_a.is_some() && op_b.is_some() && op_a != op_b {
                return Err(error("blend compound components must have matching ordered operations"));
            }
            let op = op_a.or(op_b).unwrap_or_default();
            let (sa, sb) = (&ga.subpaths[ra.clone()], &gb.subpaths[rb.clone()]);
            let mut used = vec![false; sb.len()];
            let mut matched = Vec::new();
            for (i, x) in sa.iter().enumerate() {
                let role = depth(sa, i);
                // Spatial contour correspondence in each key's normalized component frame.
                let (ba, bb) = (ga.bounds().unwrap_or(geom::Rect::ZERO), gb.bounds().unwrap_or(geom::Rect::ZERO));
                let norm_center = |s: &geom::SubPath, frame: geom::Rect| (bounds(s).center() - frame.center()) / frame.width().max(frame.height()).max(1e-9);
                let j = sb
                    .iter()
                    .enumerate()
                    .filter(|(j, y)| !used[*j] && x.closed == y.closed && role == depth(sb, *j))
                    .min_by(|(_, y), (_, z)| {
                        let cost = |y: &geom::SubPath| {
                            (norm_center(x, ba) - norm_center(y, bb)).hypot2()
                                + (bounds(x).width() / ba.width().max(1e-9) - bounds(y).width() / bb.width().max(1e-9)).powi(2)
                        };
                        cost(y).total_cmp(&cost(z))
                    })
                    .map(|(j, _)| j);
                let y = if let Some(j) = j {
                    used[j] = true;
                    sb[j].clone()
                } else {
                    collapse(x, center_b)
                };
                matched.push((x.clone(), y));
            }
            for (j, y) in sb.iter().enumerate().filter(|(j, _)| !used[*j]) {
                let _ = j;
                matched.push((collapse(y, center_a), y.clone()));
            }
            for (index, (x, y)) in matched.into_iter().enumerate() {
                let explicit = component == 0 && index == 0;
                let (x, y) = prepare(x, y, if explicit { starts } else { (None, None) })?;
                knots += x.anchors.len();
                if knots > MAX_KNOTS {
                    return Err(error("blend correspondence exceeds knot budget"));
                }
                pairs.push((x, y, if index == 0 { op } else { PathOp::Join }));
            }
        }
        Ok(Self { original_a: a.clone(), original_b: b.clone(), pairs })
    }
    pub fn at(&self, t: f64) -> Result<Path> {
        if !t.is_finite() || !(0. ..=1.).contains(&t) {
            return Err(error("blend interpolation fraction must be finite and between 0 and 1"));
        }
        if t == 0. {
            return Ok(self.original_a.clone());
        }
        if t == 1. {
            return Ok(self.original_b.clone());
        }
        let pt = |a: geom::Point, b: geom::Point| {
            let p = a.lerp(b, t);
            photocraft_geom::Point::new(p.x, p.y)
        };
        Ok(Path {
            subpaths: self
                .pairs
                .iter()
                .map(|(a, b, op)| Subpath {
                    closed: if t < 0.5 { a.closed } else { b.closed },
                    op: *op,
                    knots: a
                        .anchors
                        .iter()
                        .zip(&b.anchors)
                        .map(|(a, b)| Knot {
                            anchor: pt(a.p, b.p),
                            in_ctrl: pt(a.h_in, b.h_in),
                            out_ctrl: pt(a.h_out, b.h_out),
                            smooth: if t < 0.5 { a.kind } else { b.kind } == geom::AnchorKind::Smooth,
                        })
                        .collect(),
                })
                .collect(),
            fill_rule: if t < 0.5 { self.original_a.fill_rule } else { self.original_b.fill_rule },
            inverted: self.original_a.inverted,
        })
    }
}
