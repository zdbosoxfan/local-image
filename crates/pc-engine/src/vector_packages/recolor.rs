//! Recolor Artwork, adapted from VectorCraft `color/src/recolor.rs` and
//! `engine/src/cmd/recolor.rs` at d522c1d7be4035bd4f4a84cd6ebfca44f5155092
//! (MIT OR Apache-2.0). Copyright (c) 2026 ArtCraft Team and VectorCraft contributors.
//! Weighted farthest-first k-means uses the existing pc-vecfx Oklab conversion. Tints
//! of one global swatch stay together. Preserve Lightness keeps CIE L* (D50, 0..100)
//! by reducing target chroma when needed to fit sRGB; alpha is always retained.
use super::colours::{self, Access, Capture, Preview, bad};
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_color::convert::{lab_to_srgb, srgb_to_lab};
use photocraft_doc::vector_design::{exchange::VectorWarning, swatches::*};
use photocraft_doc::{Color, Fill, LayerId};
use photocraft_vector::appearance::{NoProjection, StrokeProjection};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

pub const MAX_COLORS: usize = 4096;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    #[default]
    Exact,
    PreserveTints,
    PreserveLightness,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub color: Color,
    pub reference: Option<SwatchRef>,
    pub targets: Vec<PaintTarget>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub from: Vec<usize>,
    pub to: Color,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub capture: Capture,
    pub selection: Vec<LayerId>,
    pub colors: Vec<Entry>,
    pub rows: Vec<Row>,
    pub method: Method,
    pub seed: u64,
    pub save_group: Option<String>,
    pub warnings: Vec<VectorWarning>,
}
fn key(c: Color, r: Option<SwatchRef>) -> String {
    // Model and exact normalized components; alpha is part of colour identity. Signed zero
    // compares like normal Rust floats; this canonicalization keeps the hash coherent.
    let bits = |x: f32| if x == 0. { 0 } else { x.to_bits() };
    format!("{:?}:{:?}:{}:{:?}", c.mode, c.c.map(bits), bits(c.alpha), r.map(|r| (r.swatch.0, bits(r.tint))))
}
fn extract(s: &Session, access: Access, ids: &[LayerId]) -> Result<(Vec<Entry>, Vec<VectorWarning>)> {
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    let book = (access.read)(doc)?;
    book.validate().map_err(bad)?;
    let (used, warnings) = colours::used_colors(doc, &book, Some(ids))?;
    let mut colors = Vec::<Entry>::new();
    let mut map = HashMap::<String, usize>::new();
    for u in used {
        colours::validate_target(doc, &u.target)?;
        let k = key(u.color, u.reference);
        if let Some(i) = map.get(&k).copied() {
            colors[i].targets.push(u.target);
        } else {
            map.insert(k, colors.len());
            colors.push(Entry { color: u.color, reference: u.reference, targets: vec![u.target] });
        }
    }
    if colors.is_empty() || colors.len() > MAX_COLORS {
        return Err(bad("selection needs 1..4096 supported colours"));
    }
    Ok((colors, warnings))
}
pub fn begin(s: &Session, access: Access, ids: &[LayerId]) -> Result<Plan> {
    let (colors, warnings) = extract(s, access, ids)?;
    let rows = colors.iter().enumerate().map(|(i, c)| Row { from: vec![i], to: c.color }).collect();
    Ok(Plan { capture: Capture::begin(s)?, selection: ids.to_vec(), colors, rows, method: Method::Exact, seed: 1, save_group: None, warnings })
}
/// xorshift64* and Fisher-Yates adapted from the pinned VectorCraft randomizer.
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9e3779b97f4a7c15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545f4914f6cdd1d)
    }
    fn unit(&mut self) -> f32 {
        ((self.next() >> 40) as f32) / (1u64 << 24) as f32
    }
}
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum()
}
fn darkest(colors: &[Entry], from: &[usize]) -> usize {
    from.iter()
        .copied()
        .min_by(|a, b| photocraft_vecfx::blend::paint::oklab(colors[*a].color)[0].total_cmp(&photocraft_vecfx::blend::paint::oklab(colors[*b].color)[0]))
        .unwrap_or(0)
}
impl Plan {
    pub fn validate(&self) -> Result<()> {
        if self.colors.is_empty() || self.colors.len() > MAX_COLORS || self.rows.is_empty() || self.rows.len() > self.colors.len() {
            return Err(bad("invalid recolour row budget"));
        }
        for entry in &self.colors {
            validate_color(entry.color).map_err(bad)?;
            if entry.reference.is_some_and(|r| r.swatch.0 == 0 || !r.tint.is_finite() || !(0.0..=100.0).contains(&r.tint)) {
                return Err(bad("invalid captured swatch reference"));
            }
        }
        let mut seen = HashSet::new();
        for row in &self.rows {
            validate_color(row.to).map_err(bad)?;
            if row.from.is_empty() {
                return Err(bad("empty recolour row"));
            }
            for i in &row.from {
                if *i >= self.colors.len() || !seen.insert(*i) {
                    return Err(bad("invalid or overlapping recolour rows"));
                }
            }
        }
        if seen.len() != self.colors.len() {
            return Err(bad("recolour rows must cover every extracted colour"));
        }
        if let Some(name) = &self.save_group {
            validate_name(name).map_err(bad)?;
        }
        Ok(())
    }
    /// Global base/tint units cannot split across rows. Seed chooses the first farthest-first
    /// centre; weighted centroids and input-order tie breaks make replay deterministic.
    pub fn reduce(&mut self, n: usize, seed: u64) -> Result<()> {
        self.validate()?;
        if n == 0 || n > self.colors.len() || n > MAX_COLORS {
            return Err(bad("colour groups outside 1..colour count"));
        }
        let mut units: Vec<Vec<usize>> = Vec::new();
        for (i, entry) in self.colors.iter().enumerate() {
            if let Some(r) = entry.reference
                && let Some(unit) = units.iter_mut().find(|u| self.colors[u[0]].reference.is_some_and(|x| x.swatch == r.swatch))
            {
                unit.push(i);
                continue;
            }
            units.push(vec![i]);
        }
        let n = n.min(units.len());
        let pts: Vec<_> = units
            .iter()
            .map(|u| {
                let mut p = photocraft_vecfx::blend::paint::oklab(self.colors[darkest(&self.colors, u)].color);
                p[0] *= 0.5;
                p
            })
            .collect();
        let weights: Vec<_> = units.iter().map(|u| u.iter().map(|i| self.colors[*i].targets.len() as f64).sum::<f64>()).collect();
        let first = (Rng::new(seed).next() % units.len() as u64) as usize;
        let mut centres = vec![pts[first]];
        let mut seed_indices = HashSet::from([first]);
        while centres.len() < n {
            let i = (0..pts.len())
                .filter(|i| !seed_indices.contains(i))
                .max_by(|a, b| {
                    let d = |i: usize| centres.iter().map(|c| distance(pts[i], *c)).fold(f64::INFINITY, f64::min);
                    d(*a).total_cmp(&d(*b)).then_with(|| b.cmp(a))
                })
                .ok_or_else(|| bad("unable to seed recolour groups"))?;
            centres.push(pts[i]);
            seed_indices.insert(i);
        }
        let nearest =
            |p: [f64; 3], centers: &[[f64; 3]]| centers.iter().enumerate().min_by(|a, b| distance(p, *a.1).total_cmp(&distance(p, *b.1))).map_or(0, |x| x.0);
        let mut assignment: Vec<_> = pts.iter().map(|p| nearest(*p, &centres)).collect();
        for _ in 0..50 {
            // Empty clusters are reseeded from the farthest point in a cluster of size>1.
            let mut counts = vec![0; n];
            for i in &assignment {
                counts[*i] += 1;
            }
            for k in 0..n {
                if counts[k] == 0 {
                    let i = (0..pts.len())
                        .filter(|i| counts[assignment[*i]] > 1)
                        .max_by(|a, b| distance(pts[*a], centres[assignment[*a]]).total_cmp(&distance(pts[*b], centres[assignment[*b]])))
                        .ok_or_else(|| bad("empty recolour cluster"))?;
                    counts[assignment[i]] -= 1;
                    assignment[i] = k;
                    counts[k] += 1;
                }
            }
            let mut sums = vec![([0.; 3], 0.); n];
            for (i, p) in pts.iter().enumerate() {
                let sum = &mut sums[assignment[i]];
                for (s, v) in sum.0.iter_mut().zip(p) {
                    *s += v * weights[i];
                }
                sum.1 += weights[i];
            }
            for (c, (p, w)) in centres.iter_mut().zip(sums) {
                if w > 0. {
                    *c = p.map(|v| v / w);
                }
            }
            let next: Vec<_> = pts.iter().map(|p| nearest(*p, &centres)).collect();
            if next == assignment {
                break;
            }
            // Keep the last nonempty assignment when identical centres cannot be separated.
            if (0..n).any(|k| !next.contains(&k)) {
                break;
            }
            assignment = next;
        }
        let mut groups = vec![Vec::new(); n];
        for (i, group) in assignment.iter().enumerate() {
            groups[*group].extend(&units[i]);
        }
        for g in &mut groups {
            g.sort_unstable();
        }
        groups.sort_by_key(|g| g[0]);
        self.rows = groups.into_iter().map(|from| Row { to: self.colors[darkest(&self.colors, &from)].color, from }).collect();
        self.seed = seed;
        self.validate()
    }
    pub fn randomize_order(&mut self, seed: u64) {
        let mut colors: Vec<_> = self.rows.iter().map(|r| r.to).collect();
        let mut rng = Rng::new(seed);
        for i in (1..colors.len()).rev() {
            let j = (rng.next() % (i as u64 + 1)) as usize;
            colors.swap(i, j);
        }
        for (row, c) in self.rows.iter_mut().zip(colors) {
            row.to = c;
        }
        self.seed = seed;
    }
    pub fn randomize_saturation_brightness(&mut self, seed: u64) {
        let mut rng = Rng::new(seed);
        for row in &mut self.rows {
            let [r, g, b] = row.to.to_rgb();
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            let delta = max - min;
            let h = if delta <= 1e-6 {
                0.
            } else if max == r {
                ((g - b) / delta).rem_euclid(6.)
            } else if max == g {
                (b - r) / delta + 2.
            } else {
                (r - g) / delta + 4.
            };
            let sat = if delta <= 1e-6 { 0. } else { 0.25 + rng.unit() * 0.75 };
            let v = 0.3 + rng.unit() * 0.7;
            let c = v * sat;
            let x = c * (1. - (h.rem_euclid(2.) - 1.).abs());
            let m = v - c;
            let rgb = match h as u32 {
                0 => [c, x, 0.],
                1 => [x, c, 0.],
                2 => [0., c, x],
                3 => [0., x, c],
                4 => [x, 0., c],
                _ => [c, 0., x],
            };
            row.to = Color::rgba(rgb[0] + m, rgb[1] + m, rgb[2] + m, row.to.alpha);
        }
        self.seed = seed;
    }
    /// Drag one source colour onto a row. A singleton source row disappears, and every colour
    /// still has exactly one assignment. Targets and colour identities remain captured.
    pub fn move_color(&mut self, color: usize, destination: usize) -> Result<()> {
        self.validate()?;
        if color >= self.colors.len() || destination >= self.rows.len() {
            return Err(bad("invalid recolour drag"));
        }
        let mut rows = self.rows.clone();
        for row in &mut rows {
            row.from.retain(|i| *i != color);
        }
        rows[destination].from.push(color);
        rows.retain(|r| !r.from.is_empty());
        self.rows = rows;
        self.validate()
    }
}
fn preserve_lightness(source: Color, to: Color) -> Color {
    let l = srgb_to_lab(source.to_rgb())[0];
    let [_, a, b] = srgb_to_lab(to.to_rgb());
    let sample = |k| lab_to_srgb([l, a * k, b * k]);
    let mut low = 0.;
    let mut high = 1.;
    for _ in 0..24 {
        let k = (low + high) * 0.5;
        let rgb = sample(k);
        if (srgb_to_lab(rgb)[0] - l).abs() <= 0.005 {
            low = k;
        } else {
            high = k;
        }
    }
    let [r, g, b] = sample(low);
    Color::rgba(r, g, b, source.alpha)
}
fn map(plan: &Plan, row: &Row, i: usize) -> Result<Color> {
    let entry = &plan.colors[i];
    let mut to = row.to;
    to.alpha = entry.color.alpha;
    match plan.method {
        Method::Exact => Ok(to),
        Method::PreserveLightness => Ok(preserve_lightness(entry.color, to)),
        Method::PreserveTints => {
            if let Some(r) = entry.reference {
                return tint(to, r.tint).map_err(bad);
            }
            let base = plan.colors[darkest(&plan.colors, &row.from)].color.to_rgb();
            let c = entry.color.to_rgb();
            let ink = base.map(|v| 1. - v);
            let actual = c.map(|v| 1. - v);
            let denom: f32 = ink.iter().map(|v| v * v).sum();
            let t = if denom > 1e-9 { actual.iter().zip(ink).map(|(a, b)| a * b).sum::<f32>() / denom } else { 1. };
            if actual.iter().zip(ink).all(|(a, b)| (a - b * t).abs() <= 0.01) { tint(to, t.clamp(0., 1.) * 100.).map_err(bad) } else { Ok(to) }
        }
    }
}
pub fn prepare(s: &Session, access: Access, plan: &Plan) -> Result<Preview> {
    prepare_with_projection(s, access, plan, &NoProjection)
}
pub fn prepare_with_projection(s: &Session, access: Access, plan: &Plan, projection: &dyn StrokeProjection) -> Result<Preview> {
    plan.capture.check(s)?;
    plan.validate()?;
    let (current, _) = extract(s, access, &plan.selection)?;
    if current != plan.colors {
        return Err(bad("recolour extraction changed; start again"));
    }
    let mut doc = (*s.active().ok_or(EngineError::NoDocument)?.doc).clone();
    let mut book = (access.read)(&doc)?;
    let mut changed = Vec::new();
    let mut seen = HashSet::new();
    for entry in &plan.colors {
        for t in &entry.targets {
            if !seen.insert(t) {
                return Err(bad("duplicate captured paint"));
            }
            colours::validate_target(&doc, t)?;
        }
    }
    for row in &plan.rows {
        for i in &row.from {
            let to = map(plan, row, *i)?;
            for t in &plan.colors[*i].targets {
                colours::set_color(&mut doc, t, to)?;
                // Selection-scoped recolour detaches uses; editing the source global swatch would
                // unexpectedly recolour unselected art. Save Group preserves the chosen palette.
                book.links.retain(|l| l.target != *t);
                changed.push(t.clone());
            }
        }
    }
    if let Some(name) = &plan.save_group {
        let id = book.next_id().map_err(bad)?;
        let name = book.free_name(name);
        book.groups.push(SwatchGroup { id, name });
        for (i, row) in plan.rows.iter().enumerate() {
            let swatch_id = SwatchId(book.next_id().map_err(bad)?);
            let name = book.free_name(&format!("Recolor {}", i + 1));
            book.swatches.push(Swatch { id: swatch_id, name, paint: Fill::Solid(row.to), global: false, spot: false, group: Some(id), stops: Vec::new() });
        }
    }
    let mut p = colours::finish(doc, book, &changed, projection)?;
    p.warnings = plan.warnings.clone();
    (access.write)(&mut p.document, p.book.clone())?;
    Ok(p)
}
pub fn commit(s: &mut Session, access: Access, plan: &Plan) -> Result<()> {
    let p = prepare(s, access, plan)?;
    colours::publish(s, plan.capture, p, "Recolor Artwork")
}
fn apply(s: &mut Session, v: &Value) -> Result<Value> {
    let p: Plan = serde_json::from_value(v.clone()).map_err(|e| bad(e.to_string()))?;
    commit(s, colours::access()?, &p)?;
    Ok(json!({"changed":true}))
}
fn colors(s: &mut Session, v: &Value) -> Result<Value> {
    let ids: Vec<LayerId> = serde_json::from_value(v.get("layers").cloned().ok_or_else(|| bad("layers required"))?).map_err(|e| bad(e.to_string()))?;
    serde_json::to_value(begin(s, colours::access()?, &ids)?).map_err(|e| bad(e.to_string()))
}
fn reduce(s: &mut Session, v: &Value) -> Result<Value> {
    let mut plan: Plan = serde_json::from_value(v.get("plan").cloned().ok_or_else(|| bad("plan required"))?).map_err(|e| bad(e.to_string()))?;
    plan.capture.check(s)?;
    let n = v.get("count").and_then(Value::as_u64).ok_or_else(|| bad("count required"))?;
    if n > MAX_COLORS as u64 {
        return Err(bad("colour count budget exceeded"));
    }
    plan.reduce(n as usize, v.get("seed").and_then(Value::as_u64).unwrap_or(1))?;
    serde_json::to_value(plan).map_err(|e| bad(e.to_string()))
}
fn preview(s: &mut Session, v: &Value) -> Result<Value> {
    let plan: Plan = serde_json::from_value(v.clone()).map_err(|e| bad(e.to_string()))?;
    let p = prepare(s, colours::access()?, &plan)?;
    Ok(json!({"detached":true,"warnings":p.warnings}))
}
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "recolor.colors",
            label: "Artwork Colours",
            menu: &[],
            shortcut: None,
            params: "{layers:[id]}",
            enabled: crate::vector_cmds::has_doc,
            run: colors,
            journal: false,
        },
        CommandSpec {
            id: "recolor.reduce",
            label: "Reduce Artwork Colours",
            menu: &[],
            shortcut: None,
            params: "{plan,count,seed}",
            enabled: crate::vector_cmds::has_doc,
            run: reduce,
            journal: false,
        },
        CommandSpec {
            id: "recolor.preview",
            label: "Preview Recolor Artwork",
            menu: &[],
            shortcut: None,
            params: "Plan",
            enabled: crate::vector_cmds::has_doc,
            run: preview,
            journal: false,
        },
        CommandSpec {
            id: "recolor.apply",
            label: "Recolor Artwork",
            menu: &[],
            shortcut: None,
            params: "Plan",
            enabled: crate::vector_cmds::has_doc,
            run: apply,
            journal: true,
        },
    ]
}
#[cfg(test)]
mod tests;
