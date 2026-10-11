//! Original GPL-3.0-or-later layer projection. Adapts the repeat instance grouping contract
//! in VectorCraft doc/pattern.rs without its Node tree. Consumers apply transforms once.
//! Upstream pin d522c1d7be4035bd4f4a84cd6ebfca44f5155092, file commit
//! 5afc78a1002cc217f743f74cea9dcdf640318b0a (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Retain licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
use super::*;
use photocraft_doc::{BlendIf, BlendMode, Effects, Group, LabelColor, LayerId, Locks, MAX_GROUP_DEPTH, VectorData, vector_design::live::LiveGroup};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub struct Expansion {
    pub layers: Vec<Layer>,
    /// Absolute source-to-document transforms, also on enclosing masks/effects.
    pub transforms: BTreeMap<u64, Affine>,
    /// Generated IDs point to the editable source. IDs are deterministic transient IDs.
    pub sources: BTreeMap<u64, LayerId>,
}

fn bounds(layers: &[Layer], depth: usize) -> Result<Option<Bounds>> {
    if depth > MAX_GROUP_DEPTH {
        return Err(error("repeat group depth limit exceeded"));
    }
    let mut result: Option<Bounds> = None;
    for l in layers {
        let b = if let LayerContent::Group(g) = &l.content {
            let source = bounds(&g.children, depth + 1)?;
            match (&l.vector.live, source) {
                (Some(LiveGroup::Repeat(spec)), Some(b)) => {
                    let mut expanded: Option<Bounds> = None;
                    for a in transforms(spec, b)? {
                        let next = b.transform(a)?;
                        expanded = Some(expanded.map_or(next, |old| old.union(next)));
                    }
                    expanded
                }
                (Some(LiveGroup::Repeat(_)), None) => return Err(error("repeat source has no geometric bounds")),
                _ => source,
            }
        } else {
            plain_bounds(l)?
        };
        if let Some(b) = b {
            result = Some(result.map_or(b, |old| old.union(b)));
        }
    }
    Ok(result)
}
pub fn source_bounds(layers: &[Layer]) -> Result<Bounds> {
    // Bound untrusted trees/knots before asking the analytic path adapter to allocate.
    preflight(layers)?;
    bounds(layers, 0)?.ok_or_else(|| error("repeat source has no geometric bounds"))
}

fn preflight(layers: &[Layer]) -> Result<u64> {
    fn visit(layers: &[Layer], depth: usize, seen: &mut BTreeSet<u64>, knots: &mut usize) -> Result<(usize, usize)> {
        if depth > MAX_GROUP_DEPTH {
            return Err(error("repeat group depth limit exceeded"));
        }
        let mut count = 0usize;
        let mut generated_knots = 0usize;
        for l in layers {
            if !seen.insert(l.id.0) || seen.len() > MAX_LAYERS {
                return Err(error("duplicate repeat source ID or source layer limit exceeded"));
            }
            let mut own_knots = 0usize;
            if let LayerContent::Shape(s) = &l.content {
                own_knots = s.path.subpaths.iter().try_fold(0usize, |sum, p| sum.checked_add(p.knots.len())).ok_or_else(|| error("repeat knot overflow"))?;
                *knots = knots.checked_add(own_knots).ok_or_else(|| error("repeat knot overflow"))?;
                if *knots > MAX_KNOTS {
                    return Err(error("repeat source knot limit exceeded"));
                }
                for p in s.path.subpaths.iter().flat_map(|s| &s.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]) {
                    number(p.x)?;
                    number(p.y)?;
                }
            }
            let (children, child_knots) = if let Some(g) = l.children() { visit(g, depth + 1, seen, knots)? } else { (0, 0) };
            let (extra, copies) = match &l.vector.live {
                Some(LiveGroup::Repeat(spec)) => {
                    if l.children().is_none_or(|c| c.is_empty()) {
                        return Err(error("repeat must own a nonempty group"));
                    }
                    let copies = instance_count(spec)? as usize;
                    (copies, copies)
                }
                _ => (0, 1),
            };
            count = count
                .checked_add(1)
                .and_then(|v| v.checked_add(extra))
                .and_then(|v| children.checked_mul(copies).and_then(|c| v.checked_add(c)))
                .ok_or_else(|| error("repeat layer overflow"))?;
            generated_knots = generated_knots
                .checked_add(own_knots)
                .and_then(|v| child_knots.checked_mul(copies).and_then(|k| v.checked_add(k)))
                .ok_or_else(|| error("repeat knot overflow"))?;
            if count > MAX_LAYERS || generated_knots > MAX_KNOTS {
                return Err(error("repeat generated layer/knot limit exceeded"));
            }
            // A repeat adds a grouping level in the projection.
            if extra > 0 && depth + 2 > MAX_GROUP_DEPTH {
                return Err(error("repeat expansion depth limit exceeded"));
            }
        }
        Ok((count, generated_knots))
    }
    let mut seen = BTreeSet::new();
    visit(layers, 0, &mut seen, &mut 0)?;
    Ok(seen.last().copied().unwrap_or(0))
}

struct Projector {
    next: u64,
    out: Expansion,
}
impl Projector {
    fn id(&mut self) -> Result<LayerId> {
        self.next = self.next.checked_add(1).ok_or_else(|| error("repeat transient ID namespace exhausted"))?;
        Ok(LayerId(self.next))
    }
    fn layers(&mut self, layers: &[Layer], inherited: Affine, generated: bool, depth: usize) -> Result<Vec<Layer>> {
        if depth > MAX_GROUP_DEPTH {
            return Err(error("repeat expansion depth limit exceeded"));
        }
        let mut out = Vec::with_capacity(layers.len());
        for source in layers {
            let mut l = source.clone();
            if generated {
                l.id = self.id()?;
                l.fill_cache = None;
                l.psd_id = None;
                l.psd_blocks.clear();
                l.link_group = None;
            }
            self.out.transforms.insert(l.id.0, inherited);
            self.out.sources.insert(l.id.0, source.id);
            if let LayerContent::Group(g) = &source.content {
                let children = if let Some(LiveGroup::Repeat(spec)) = &source.vector.live {
                    let b = bounds(&g.children, depth + 1)?.ok_or_else(|| error("repeat source has no bounds"))?;
                    l.vector.live = None;
                    let mut instances = Vec::new();
                    for (index, a) in transforms(spec, b)?.into_iter().enumerate() {
                        let a = inherited.mul(&a);
                        b.transform(a)?;
                        let id = self.id()?;
                        let children = self.layers(&g.children, a, true, depth + 2)?;
                        self.out.transforms.insert(id.0, Affine::IDENTITY);
                        self.out.sources.insert(id.0, source.id);
                        instances.push(instance_group(id, index, children));
                    }
                    instances
                } else {
                    self.layers(&g.children, inherited, generated, depth + 1)?
                };
                l.content = LayerContent::Group(Group { children, ..g.clone() });
            }
            out.push(l);
        }
        Ok(out)
    }
}
fn instance_group(id: LayerId, index: usize, children: Vec<Layer>) -> Layer {
    // No LayerId::fresh() calls in a read-only projection: output and global ID state are stable.
    Layer {
        id,
        name: format!("Instance {}", index + 1),
        vector: VectorData::default(),
        visible: true,
        locks: Locks::default(),
        blend: BlendMode::PassThrough,
        opacity: 1.,
        fill_opacity: 1.,
        clipped: false,
        mask: None,
        vector_mask: None,
        effects: Effects { enabled: true, ..Default::default() },
        label: LabelColor::None,
        content: LayerContent::Group(Group { children, expanded: false, artboard: None }),
        psd_blocks: Vec::new(),
        psd_id: None,
        fill_cache: None,
        link_group: None,
        excluded_channels: 0,
        blend_if: BlendIf::default(),
        video: None,
        generation: None,
    }
}

/// Evaluates repeat groups recursively into ordinary layers with absolute transforms.
/// The caller's tree, source caches and parameters are untouched. Projected IDs are transient;
/// expansion into an editable document must allocate fresh persistent IDs after preparation.
pub fn expand_layers(layers: &[Layer]) -> Result<Expansion> {
    let next = preflight(layers)?;
    let mut p = Projector { next, out: Expansion { layers: Vec::new(), transforms: BTreeMap::new(), sources: BTreeMap::new() } };
    p.out.layers = p.layers(layers, Affine::IDENTITY, false, 0)?;
    Ok(p.out)
}
/// File adapters share this evaluator through the exchange DTO and depend only on pc-vecfx.
pub fn expand_scene(scene: &photocraft_doc::vector_design::exchange::ImportedScene) -> Result<Expansion> {
    expand_layers(&scene.layers)
}
