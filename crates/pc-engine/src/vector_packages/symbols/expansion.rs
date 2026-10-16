//! Shared, data-only symbol projection using ordinary Layer objects, not a second document tree.
//! Transforms accompany the projected layer IDs; rendering/export applies each exactly once.
use super::symbol_validation::{self as validation, Result};
use photocraft_doc::{Affine, Document, Group, Layer, LayerContent, LayerId, SymbolDef};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct Expansion {
    pub layers: Vec<Layer>,
    pub transforms: BTreeMap<u64, Affine>,
}
fn project(
    layers: &[Layer],
    symbols: &BTreeMap<u64, &SymbolDef>,
    inherited: Affine,
    template: bool,
    transforms: &mut BTreeMap<u64, Affine>,
) -> Result<Vec<Layer>> {
    layers
        .iter()
        .map(|source| {
            let mut layer = source.clone();
            if template {
                layer.id = LayerId::fresh();
                layer.psd_id = None;
                layer.link_group = None;
            }
            validation::affine(inherited)?;
            transforms.insert(layer.id.0, inherited);
            layer.content = match &source.content {
                LayerContent::Symbol(instance) => {
                    let def = symbols.get(&instance.symbol.0).ok_or_else(|| validation::error("missing symbol"))?;
                    let a = inherited.mul(&instance.transform);
                    validation::affine(a)?;
                    LayerContent::Group(Group {
                        children: project(&def.layers, symbols, a, true, transforms)?,
                        expanded: true,
                        artboard: None,
                        isolation: photocraft_doc::GroupIsolation::Auto,
                    })
                }
                LayerContent::Group(g) => LayerContent::Group(Group {
                    children: project(&g.children, symbols, inherited, template, transforms)?,
                    expanded: g.expanded,
                    artboard: g.artboard.clone(),
                    isolation: g.isolation,
                }),
                content => content.clone(),
            };
            Ok(layer)
        })
        .collect()
}
/// Export adapters (13/14/21) and scaled display (09) consume this public projection.
/// The outer instance's blend/masks/effects stay on its isolated ordinary group.
pub fn expand(doc: &Document, layers: &[Layer]) -> Result<Expansion> {
    validation::document(doc)?;
    validation::validate_selection(&doc.symbols, layers)?;
    project_validated(&doc.symbols, layers)
}
/// File adapters use the exchange DTO directly, without an engine/UI dependency.
pub fn expand_scene(scene: &photocraft_doc::vector_design::exchange::ImportedScene) -> Result<Expansion> {
    expand_layers(&scene.symbols, &scene.layers)
}
pub fn expand_layers(symbols: &[SymbolDef], layers: &[Layer]) -> Result<Expansion> {
    validation::validate(symbols, layers)?;
    project_validated(symbols, layers)
}
fn project_validated(symbols: &[SymbolDef], layers: &[Layer]) -> Result<Expansion> {
    let mut transforms = BTreeMap::new();
    let defs = symbols.iter().map(|s| (s.id.0, s)).collect();
    let layers = project(layers, &defs, Affine::IDENTITY, false, &mut transforms)?;
    Ok(Expansion { layers, transforms })
}

/// Transient per-master generations for package 09's scaled projection cache. Equality uses
/// ordinary master data/COW pixels, and generation changes propagate through nested references.
/// Native documents need no new persisted field; session revisions remain the publication key.
#[derive(Clone, Debug, Default)]
pub struct MasterGenerations {
    previous: Vec<SymbolDef>,
    clock: u64,
    keys: BTreeMap<u64, u64>,
}
impl MasterGenerations {
    pub fn generation(&self, id: photocraft_doc::SymbolId) -> Option<u64> {
        self.keys.get(&id.0).copied()
    }
    pub fn sync(&mut self, doc: &Document) -> Result<()> {
        use std::collections::BTreeSet;
        validation::document(doc)?;
        let old: BTreeMap<_, _> = self.previous.iter().map(|d| (d.id.0, d)).collect();
        let new: BTreeMap<_, _> = doc.symbols.iter().map(|d| (d.id.0, d)).collect();
        let mut changed: BTreeSet<u64> = old.iter().filter(|(id, def)| new.get(id).is_none_or(|next| next != *def)).map(|(id, _)| *id).collect();
        changed.extend(new.iter().filter(|(id, def)| old.get(id).is_none_or(|before| before != *def)).map(|(id, _)| *id));
        fn refs(layers: &[Layer], changed: &BTreeSet<u64>) -> bool {
            layers.iter().any(|l| match &l.content {
                LayerContent::Symbol(s) => changed.contains(&s.symbol.0),
                LayerContent::Group(g) => refs(&g.children, changed),
                _ => false,
            })
        }
        loop {
            let count = changed.len();
            for def in &doc.symbols {
                if refs(&def.layers, &changed) {
                    changed.insert(def.id.0);
                }
            }
            if count == changed.len() {
                break;
            }
        }
        if !changed.is_empty() {
            self.clock = self.clock.checked_add(1).ok_or_else(|| validation::error("master generation exhausted"))?;
            for id in changed {
                self.keys.insert(id, self.clock);
            }
            self.keys.retain(|id, _| new.contains_key(id));
            self.previous = doc.symbols.clone();
        }
        Ok(())
    }
}
