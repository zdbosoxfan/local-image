//! Owned pass templates, rebound to current surfaces without rerunning the planner or FX check.
//! The structure key includes exact content bounds and all planner inputs; pixel changes inside
//! those bounds only replace borrowed texture sources. Derived masks/shapes invalidate on edits.
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::sync::Arc;

use photocraft_doc::{Document, Layer, LayerContent, LayerId};
use photocraft_geom::Rect;
use photocraft_raster::Surface;

use crate::plan::{FxLayer, MaskUse, Pass, Plan, Role, SurfaceRef, TexUse, Unsupported};

#[derive(Default)]
pub struct PlanCache {
    key: String,
    cached: Option<Result<Template, Unsupported>>,
    bounds: HashMap<u64, Rect>,
    _profile: Option<Arc<Vec<u8>>>,
    pub builds: u64,
}

struct Template {
    passes: Vec<Pass<'static>>,
    direct: Vec<(bool, bool)>,
    patterns: Vec<Option<usize>>,
    fx: Vec<(LayerId, Rect, Rect)>,
    slots: u32,
    root: u32,
    layers: Arc<HashSet<LayerId>>,
    paths: HashMap<LayerId, Vec<usize>>,
}

impl PlanCache {
    pub fn get<'a>(&mut self, doc: &'a Document, check: impl FnOnce(&Plan<'_>) -> Result<(), Unsupported>) -> Result<Plan<'a>, Unsupported> {
        let key = self.structure(doc);
        if self.cached.is_none() || self.key != key {
            self.builds += 1;
            self.cached = Some(crate::plan(doc).and_then(|p| {
                check(&p)?;
                Ok(Template::new(doc, &p))
            }));
            self.key = key;
        }
        match self.cached.as_ref() {
            Some(Ok(t)) => t.bind(doc),
            Some(Err(e)) => Err(e.clone()),
            None => Err(Unsupported("missing cached plan".into())),
        }
    }

    pub fn layer_ids(&self) -> Option<Arc<HashSet<LayerId>>> {
        self.cached.as_ref().and_then(|t| t.as_ref().ok()).map(|t| t.layers.clone())
    }

    fn structure(&mut self, doc: &Document) -> String {
        // Bound storage is bounded even during a long stroke (surface revisions are unique).
        if self.bounds.len() > 4096 {
            self.bounds.clear();
        }
        let mut key = format!("{:?}{:?}{:?}{:?}{:?}{:?}", doc.id, doc.size, doc.mode, doc.depth, doc.global_light, photocraft_compose::psblend::text_gamma());
        // Profile bytes are Arc-shared by document snapshots; keep only identity plus the
        // conversion ID (the compositor also invalidates resident conversions by that ID).
        let _ = write!(key, "{:?}", doc.icc_profile.as_ref().map(Arc::as_ptr));
        self._profile = doc.icc_profile.clone();
        for p in &doc.patterns {
            let _ = write!(key, "{:?}", (&p.id, &p.name, p.width, p.height, p.surface.revision()));
        }
        self.layers(&doc.layers, &mut key);
        key
    }

    fn surface(&mut self, s: &Surface, key: &mut String, derived: bool) {
        let bounds = *self.bounds.entry(s.revision()).or_insert_with(|| photocraft_compose::bounds::content_bounds(s));
        let _ = write!(key, "{:?}", (s.format(), s.default_bytes(), bounds, s.tile_count() > 0, derived.then(|| s.revision())));
    }

    fn layers(&mut self, layers: &[Layer], key: &mut String) {
        key.push('[');
        for l in layers {
            let _ = write!(
                key,
                "{:?}{:?}",
                (l.id, &l.name, l.visible, l.blend, l.opacity, l.fill_opacity, l.clipped, l.excluded_channels),
                (&l.blend_if, &l.effects, &l.vector_mask)
            );
            if let Some(m) = &l.mask {
                let _ = write!(key, "{:?}", (m.enabled, m.density, m.feather));
                self.surface(&m.surface, key, l.vector_mask.as_ref().is_some_and(|v| v.enabled) || photocraft_compose::masks::has_feather(l));
            } else {
                key.push('-');
            }
            match &l.content {
                LayerContent::Raster(s) => {
                    key.push('r');
                    self.surface(s, key, false);
                }
                LayerContent::Group(g) => {
                    let _ = write!(key, "g{:?}", g.artboard);
                    self.layers(&g.children, key);
                }
                LayerContent::Adjustment(a) => {
                    let _ = write!(key, "a{a:?}");
                }
                LayerContent::Fill(f) => {
                    let _ = write!(key, "f{f:?}");
                }
                LayerContent::Shape(s) => {
                    let _ = write!(key, "s{:?}", (&s.path, &s.fill, &s.stroke));
                    if let Some(s) = &s.cache {
                        self.surface(s, key, true);
                    } else {
                        key.push('-');
                    }
                }
                LayerContent::Text(t) => {
                    key.push('t');
                    if let Some(s) = &t.cache {
                        self.surface(s, key, false);
                    } else {
                        key.push('-');
                    }
                }
                LayerContent::Smart(s) => {
                    key.push('o');
                    if let Some(s) = &s.cache {
                        self.surface(s, key, false);
                    } else {
                        key.push('-');
                    }
                }
            }
            if let Some(c) = &l.fill_cache {
                let _ = write!(key, "{:?}", c.fill);
                self.surface(&c.surface, key, false);
            } else {
                key.push('-');
            }
        }
        key.push(']');
    }
}

impl Template {
    fn new(doc: &Document, p: &Plan<'_>) -> Self {
        let own = |s: &SurfaceRef<'_>| match s {
            SurfaceRef::Derived(s) => SurfaceRef::Derived(s.clone()),
            // Direct sources are rebound before use, so don't pin any document tiles.
            SurfaceRef::Doc(s) => SurfaceRef::Derived(Arc::new(Surface::new(s.format()))),
        };
        let passes = p
            .passes
            .iter()
            .map(|p| Pass {
                kernel: p.kernel,
                dst: p.dst,
                a: p.a,
                b: p.b,
                c: p.c,
                d: p.d,
                mode: p.mode,
                opacity: p.opacity,
                color: p.color,
                tex: p.tex.as_ref().map(|t| TexUse { layer: t.layer, role: t.role, surface: own(&t.surface) }),
                mask: p.mask.as_ref().map(|m| MaskUse { layer: m.layer, density: m.density, default: m.default, surface: own(&m.surface) }),
                adjust_kind: p.adjust_kind,
                params: p.params,
                extra: p.extra,
                flags: p.flags,
                lut: p.lut.clone(),
                gradient: p.gradient,
                map: p.map,
                pattern: None,
                clip: p.clip,
            })
            .collect();
        fn collect(layers: &[Layer], path: &mut Vec<usize>, paths: &mut HashMap<LayerId, Vec<usize>>) {
            for (i, l) in layers.iter().enumerate() {
                path.push(i);
                paths.insert(l.id, path.clone());
                if let Some(c) = l.children() {
                    collect(c, path, paths);
                }
                path.pop();
            }
        }
        let mut paths = HashMap::new();
        collect(&doc.layers, &mut Vec::new(), &mut paths);
        let layers = paths.keys().copied().collect();
        Self {
            passes,
            layers: Arc::new(layers),
            paths,
            direct: p
                .passes
                .iter()
                .map(|p| {
                    (
                        p.tex.as_ref().is_some_and(|t| matches!(t.surface, SurfaceRef::Doc(_))),
                        p.mask.as_ref().is_some_and(|m| matches!(m.surface, SurfaceRef::Doc(_))),
                    )
                })
                .collect(),
            patterns: p.passes.iter().map(|p| p.pattern.and_then(|pat| doc.patterns.iter().position(|x| std::ptr::eq(x, pat)))).collect(),
            fx: p.fx.iter().map(|f| (f.layer.id, f.region, f.bounds)).collect(),
            slots: p.slots,
            root: p.root,
        }
    }

    fn bind<'a>(&self, doc: &'a Document) -> Result<Plan<'a>, Unsupported> {
        let source = |id, role| {
            let l = doc.layer_at(self.paths.get(&id)?)?;
            match role {
                Role::Mask => l.mask.as_ref().map(|m| &m.surface),
                _ => l.surface().or_else(|| l.fill_cache.as_ref().map(|c| &c.surface)),
            }
        };
        let mut passes: Vec<Pass<'a>> = self.passes.clone();
        for ((p, &(tex, mask)), pattern) in passes.iter_mut().zip(&self.direct).zip(&self.patterns) {
            if let Some(t) = &mut p.tex
                && tex
            {
                t.surface = SurfaceRef::Doc(source(t.layer, t.role).ok_or_else(|| Unsupported("cached layer source missing".into()))?);
            }
            if let Some(m) = &mut p.mask
                && mask
            {
                m.surface = SurfaceRef::Doc(source(m.layer, Role::Mask).ok_or_else(|| Unsupported("cached mask source missing".into()))?);
            }
            p.pattern = pattern.and_then(|i| doc.patterns.get(i));
        }
        let fx = self
            .fx
            .iter()
            .map(|&(id, region, bounds)| {
                self.paths
                    .get(&id)
                    .and_then(|path| doc.layer_at(path))
                    .map(|layer| FxLayer { layer, region, bounds })
                    .ok_or_else(|| Unsupported("cached effect layer missing".into()))
            })
            .collect::<Result<_, _>>()?;
        Ok(Plan { passes, fx, slots: self.slots, root: self.root })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_color::{Color, ColorMode, SampleType};
    use photocraft_doc::{Effect, LayerMask, Size};

    fn doc() -> Document {
        Document::with_background("cache", Size::new(512, 512), ColorMode::Rgb, SampleType::U8, Color::rgb(0.2, 0.3, 0.4))
    }

    #[test]
    fn pixel_edits_rebind_sources_and_structure_edits_rebuild() {
        let mut d = doc();
        let mut c = PlanCache::default();
        c.get(&d, |_| Ok(())).unwrap();
        let r = Rect::new(10, 10, 20, 20);
        d.layers[0].surface_mut().unwrap().fill_rect(r, &[1.0, 0.0, 0.0, 1.0]);
        let p = c.get(&d, |_| panic!("FX checked again for unchanged structure")).unwrap();
        assert_eq!(c.builds, 1);
        let s = p.passes.iter().filter_map(|p| p.tex.as_ref()).next().unwrap().surface.get();
        assert_eq!(s.pixel(10, 10), [1.0, 0.0, 0.0, 1.0]);
        let edits: Vec<Box<dyn Fn(&mut Document)>> = vec![
            Box::new(|d| d.layers[0].opacity = 0.5),
            Box::new(|d| d.layers[0].mask = Some(LayerMask::reveal_all())),
            Box::new(|d| d.layers[0].mask.as_mut().unwrap().density = 0.7),
            Box::new(|d| d.layers[0].effects.items.push(Effect::default_drop_shadow())),
            Box::new(|d| d.layers[0].visible = false),
            Box::new(|d| d.size = Size::new(1024, 512)),
        ];
        for edit in edits {
            let n = c.builds;
            edit(&mut d);
            c.get(&d, |_| Ok(())).unwrap();
            assert_eq!(c.builds, n + 1);
        }
    }

    #[test]
    fn derived_vector_masks_invalidate_on_pixel_mask_edits() {
        use photocraft_doc::{Path, Subpath, VectorMask};
        let mut d = doc();
        d.layers[0].vector_mask = Some(VectorMask::new(Path::new(vec![Subpath::polygon(&[(0.0, 0.0), (50.0, 0.0), (0.0, 50.0)])])));
        d.layers[0].mask = Some(LayerMask::reveal_all());
        let mut c = PlanCache::default();
        c.get(&d, |_| Ok(())).unwrap();
        d.layers[0].mask.as_mut().unwrap().surface.fill_rect(Rect::new(5, 5, 10, 10), &[0.0]);
        let p = c.get(&d, |_| Ok(())).unwrap();
        assert_eq!(c.builds, 2);
        let mask = p.passes.iter().filter_map(|p| p.mask.as_ref()).next().unwrap();
        assert_eq!(mask.surface.get().pixel(6, 6), [0.0]);
        assert!(mask.surface.get().pixel(12, 12)[0] > 0.9);
    }

    #[test]
    fn derived_feathered_masks_invalidate_on_pixel_edits_inside_existing_bounds() {
        let mut d = doc();
        let mut mask = LayerMask::reveal_all();
        mask.feather = 3.0;
        mask.surface.fill_rect(Rect::new(1, 1, 40, 40), &[0.0]);
        d.layers[0].mask = Some(mask);
        let mut c = PlanCache::default();
        c.get(&d, |_| Ok(())).unwrap();
        d.layers[0].mask.as_mut().unwrap().surface.fill_rect(Rect::new(5, 5, 10, 10), &[0.5]);
        let p = c.get(&d, |_| Ok(())).unwrap();
        assert_eq!(c.builds, 2);
        let expected = photocraft_compose::masks::combined_mask(&d.layers[0], d.bounds()).unwrap();
        let mask = p.passes.iter().filter_map(|p| p.mask.as_ref()).next().unwrap();
        assert_eq!(mask.surface.get(), &expected);
    }

    #[test]
    fn bounds_undo_and_failed_plans_invalidate_correctly() {
        let mut d = Document::new("sparse", Size::new(512, 512), ColorMode::Rgb, SampleType::U8);
        d.layers.push(Layer::raster("paint", d.pixel_format()));
        d.layers[0].surface_mut().unwrap().fill_rect(Rect::new(10, 10, 20, 20), &[1.0; 4]);
        let before = d.clone();
        let mut c = PlanCache::default();
        c.get(&d, |_| Ok(())).unwrap();
        d.layers[0].surface_mut().unwrap().fill_rect(Rect::new(100, 100, 120, 120), &[1.0; 4]);
        let p = c.get(&d, |_| Ok(())).unwrap();
        assert_eq!(c.builds, 2);
        assert!(p.passes.iter().filter_map(|p| p.clip).any(|r| r.contains_rect(&Rect::new(100, 100, 120, 120))));
        c.get(&before, |_| Ok(())).unwrap();
        assert_eq!(c.builds, 3);
        d.mode = ColorMode::Multichannel;
        assert!(c.get(&d, |_| Ok(())).is_err());
        assert!(c.get(&d, |_| panic!("cached refusal rebuilt")).is_err());
        assert_eq!(c.builds, 4);
        d.mode = ColorMode::Rgb;
        assert!(c.get(&d, |_| Ok(())).is_ok());
        assert_eq!(c.builds, 5);
    }
}
