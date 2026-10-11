//! Original bounded gap bridges (GPL-3.0-or-later). Source paths are never moved or closed.
//! Presets are document pixels. A nearest bridge connects dangling endpoints to endpoints
//! or another curve; a bridge participates in region construction but has no visible stroke.
use super::{Gap, MAX_SEGMENTS, Result, error};
use photocraft_doc::{Path, Subpath};
use photocraft_pathops as ops;
use std::collections::BTreeSet;

pub fn close(input: &[ops::Shape], gap: Gap) -> Result<Vec<ops::Shape>> {
    if gap == Gap::None {
        return Ok(input.to_vec());
    }
    let mut ends = Vec::new();
    for (i, shape) in input.iter().enumerate() {
        for (sp, subpath) in shape.path.subpaths.iter().enumerate().filter(|(_, s)| !s.closed && s.knots.len() >= 2) {
            for k in [0, subpath.knots.len() - 1] {
                ends.push((i, sp, k, subpath.knots[k].anchor));
            }
        }
    }
    if ends.len() > MAX_SEGMENTS {
        return Err(error("gap endpoint budget exceeded"));
    }
    let mut candidates = Vec::new();
    for (a, &(i, source_subpath, _, p)) in ends.iter().enumerate() {
        for (b, &(_, _, _, q)) in ends.iter().enumerate().skip(a + 1) {
            let d = (p.x - q.x).hypot(p.y - q.y);
            if d > 1e-7 && d <= gap.distance() {
                candidates.push((d, a, Some(b), q));
            }
        }
        for (j, shape) in input.iter().enumerate() {
            let other_subpaths;
            let candidate = if i == j {
                other_subpaths = Path {
                    subpaths: shape.path.subpaths.iter().enumerate().filter(|(index, _)| *index != source_subpath).map(|(_, s)| s.clone()).collect(),
                    ..shape.path.clone()
                };
                &other_subpaths
            } else {
                &shape.path
            };
            if let Some((_, _, _, q, d)) = ops::nearest(candidate, p).map_err(error)? {
                if d <= 1e-7 {
                    // This endpoint already meets a curve, and is not dangling.
                    candidates.push((0., a, None, p));
                } else if d <= gap.distance() {
                    let endpoint = ends
                        .iter()
                        .enumerate()
                        .find(|(_, (shape, _, _, endpoint))| *shape == j && (endpoint.x - q.x).hypot(endpoint.y - q.y) <= 1e-7)
                        .map(|(b, _)| b);
                    candidates.push((d, a, endpoint, q));
                }
            }
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut out = input.to_vec();
    let mut used = BTreeSet::new();
    // Reserve identity by endpoint index rather than distance-sort order.
    let base = input.iter().map(|s| s.key).max().unwrap_or(0).checked_add(1).ok_or_else(|| error("gap ID overflow"))?;
    for (d, a, b, q) in candidates {
        if used.contains(&a) || b.is_some_and(|b| used.contains(&b)) {
            continue;
        }
        used.insert(a);
        if let Some(b) = b {
            used.insert(b);
        }
        if d == 0. {
            continue;
        }
        let p = ends[a].3;
        let key = base.checked_add(a as u64).ok_or_else(|| error("gap ID overflow"))?;
        out.push(ops::Shape::new(Path::new(vec![Subpath::polyline(&[(p.x, p.y), (q.x, q.y)])]), key));
    }
    let segments = out.iter().flat_map(|s| &s.path.subpaths).map(|s| s.knots.len()).sum::<usize>();
    if segments > MAX_SEGMENTS {
        return Err(error("gap closing exceeds geometry budget"));
    }
    Ok(out)
}

#[cfg(test)]
mod review_tests {
    use super::*;
    #[test]
    fn bugfix_vector_gap_endpoint_curve_ties_reserve_both_ends() {
        for reversed in [false, true] {
            let mut input = vec![
                ops::Shape::new(Path::new(vec![Subpath::polyline(&[(-20., 0.), (0., 0.)])]), 1),
                ops::Shape::new(Path::new(vec![Subpath::polyline(&[(2., 0.), (22., 0.)])]), 2),
            ];
            if reversed {
                input.reverse();
            }
            let out = close(&input, Gap::Small).unwrap();
            assert_eq!(out.len(), 3, "one bridge per gap");
            let broken = photocraft_doc::Layer::new("Broken rectangle", photocraft_doc::LayerContent::Shape(photocraft_doc::ShapeLayer {
                path: Path::new(vec![Subpath::polyline(&[(0.,0.),(0.,20.),(20.,20.),(20.,0.),(2.,0.)])]),
                ..Default::default()
            }));
            let graph = crate::live_paint::Graph::new(&[broken], Gap::Small).unwrap();
            assert_eq!(graph.inputs.len(), 2);
            assert_eq!(graph.faces.len(), 1);
            assert!(graph.face_at(photocraft_geom::Point::new(10.,10.)).unwrap().is_some());
        }
    }
}
