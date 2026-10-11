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
    for (a, &(i, _, _, p)) in ends.iter().enumerate() {
        for (b, &(_, _, _, q)) in ends.iter().enumerate().skip(a + 1) {
            let d = (p.x - q.x).hypot(p.y - q.y);
            if d > 1e-7 && d <= gap.distance() {
                candidates.push((d, a, Some(b), q));
            }
        }
        for (j, shape) in input.iter().enumerate() {
            if i == j {
                continue;
            }
            if let Some((_, _, _, q, d)) = ops::nearest(&shape.path, p).map_err(error)? {
                if d <= 1e-7 {
                    // This endpoint already meets a curve, and is not dangling.
                    candidates.push((0., a, None, p));
                } else if d <= gap.distance() {
                    candidates.push((d, a, None, q));
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
