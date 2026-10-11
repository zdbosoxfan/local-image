//! Original topology signatures, GPL-3.0-or-later. No coordinate hash or arrangement index
//! is persisted: translation of a source preserves its segment ownership and boundary side.
use super::{Boundary, Edge, MAX_SEGMENTS, Result, Signature, error};
use photocraft_doc::Path;
use photocraft_pathops::{self as ops, geom::ParamCurve};
use std::collections::{BTreeMap, BTreeSet};

impl Signature {
    pub fn validate(&self) -> Result<()> {
        if self.boundary.is_empty()
            || self.boundary.len() > MAX_SEGMENTS
            || self.members.len() > super::MAX_SOURCES
            || self.boundary.iter().any(|b| b.source == 0 || b.segment >= MAX_SEGMENTS || b.subpath >= MAX_SEGMENTS)
            || self.members.windows(2).any(|w| w[0] >= w[1])
            || self.boundary.windows(2).any(|w| w[0] >= w[1])
        {
            return Err(error("invalid region signature"));
        }
        Ok(())
    }
}
fn boundary(path: &Path, inputs: &[ops::Shape], oriented: bool) -> Result<Vec<Boundary>> {
    let geometry = inputs.iter().map(|s| (s.key, ops::to_geometry(&s.path))).collect::<Vec<_>>();
    let mut tokens = BTreeSet::new();
    for segment in ops::to_geometry(path).to_bezpath().segments() {
        for t in [0.2, 0.5, 0.8] {
            let p = segment.eval(t);
            let nearest = geometry.iter().filter_map(|(key, g)| g.nearest(p).map(|n| (*key, g, n))).min_by(|a, b| a.2.4.total_cmp(&b.2.4));
            let Some((key, g, (sp, seg, u, _, d))) = nearest else { return Err(error("region boundary has no source")) };
            if d > 0.01 {
                return Err(error("region boundary lost its source segment"));
            }
            let curve = g.subpaths[sp].segment(seg);
            let a = segment.eval((t - 0.001).max(0.));
            let b = segment.eval((t + 0.001).min(1.));
            let c = curve.eval((u - 0.001).max(0.));
            let e = curve.eval((u + 0.001).min(1.));
            tokens.insert(Boundary { source: key, subpath: sp, segment: seg, forward: oriented && (b - a).dot(e - c) >= 0. });
        }
    }
    let out = tokens.into_iter().collect::<Vec<_>>();
    if out.is_empty() {
        return Err(error("empty region boundary"));
    }
    Ok(out)
}
pub fn face(region: &ops::Region, inputs: &[ops::Shape]) -> Result<Signature> {
    let mut members = region.sources.iter().map(|i| inputs.get(*i).map(|s| s.key).ok_or_else(|| error("invalid region source"))).collect::<Result<Vec<_>>>()?;
    members.sort_unstable();
    members.dedup();
    let out = Signature { members, boundary: boundary(&region.path, inputs, true)?, branch: 0 };
    out.validate()?;
    Ok(out)
}
pub fn edge(path: &Path, source: u64, inputs: &[ops::Shape]) -> Result<Signature> {
    let source_inputs: Vec<_> = inputs.iter().filter(|s| s.key == source).cloned().collect();
    let out = Signature { members: vec![source], boundary: boundary(path, &source_inputs, false)?, branch: 0 };
    out.validate()?;
    Ok(out)
}
/// Different pieces of the same source segment use their order along that source.
pub fn disambiguate_edges(edges: &mut [Edge], inputs: &[ops::Shape]) -> Result<()> {
    let mut groups: BTreeMap<Signature, Vec<(f64, usize)>> = BTreeMap::new();
    for (i, e) in edges.iter().enumerate() {
        let input = inputs.iter().find(|s| Some(&s.key) == e.signature.members.first()).ok_or_else(|| error("edge source missing"))?;
        let g = ops::to_geometry(&input.path);
        let p = ops::to_geometry(&e.path).to_bezpath().segments().next().ok_or_else(|| error("empty edge"))?.eval(0.5);
        let (sp, seg, t, _, _) = g.nearest(p).ok_or_else(|| error("edge source missing"))?;
        groups.entry(e.signature.clone()).or_default().push((sp as f64 * (MAX_SEGMENTS + 1) as f64 + seg as f64 + t, i));
    }
    for (_, mut group) in groups {
        group.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (rank, (_, i)) in group.into_iter().enumerate() {
            edges[i].signature.branch = rank as u32;
        }
    }
    Ok(())
}
