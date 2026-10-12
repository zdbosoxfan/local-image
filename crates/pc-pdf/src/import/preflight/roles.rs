//! Bounded stream-role discovery. Inherited role flags avoid rescanning nested
//! role dictionaries; references enter ordered sets only after storage admission.
use super::{Guard, ImportError, Result, Value, cancelled, invalid};
use std::{collections::BTreeSet, sync::atomic::AtomicBool};
type Id = (u32, u16);
#[derive(Default)]
struct Seeds {
    content: BTreeSet<Id>,
    opaque: BTreeSet<Id>,
    fonts: BTreeSet<Id>,
}
pub(super) struct Roles {
    pub content: BTreeSet<Id>,
    pub opaque: BTreeSet<Id>,
    pub fonts: BTreeSet<Id>,
}
struct Budget<'a> {
    cancel: &'a AtomicBool,
    work: usize,
    work_limit: usize,
    objects: usize,
    storage: u64,
    room: u64,
}
impl Budget<'_> {
    fn visit(&mut self) -> Result<()> {
        cancelled(self.cancel)?;
        self.work = self.work.saturating_add(1);
        if self.work > self.work_limit {
            return Err(ImportError::Limit("stream role work"));
        }
        Ok(())
    }
    fn insert(&mut self, set: &mut BTreeSet<Id>, id: Id) -> Result<()> {
        if set.contains(&id) {
            return Ok(());
        }
        if set.len() >= self.objects {
            return Err(ImportError::Limit("stream role reference"));
        }
        // Conservative per-entry BTree node/pointer allowance, including sets
        // whose nodes are later popped. Charge before allocation, never per alias.
        let storage = self.storage.checked_add(64).ok_or(ImportError::Limit("stream role storage"))?;
        if storage > self.room {
            return Err(ImportError::Limit("stream role storage"));
        }
        self.storage = storage;
        set.insert(id);
        Ok(())
    }
}
const CONTENT: u8 = 1;
const OPAQUE: u8 = 2;
const FONT: u8 = 4;
fn discover(v: &Value, inherited: u8, seeds: &mut Seeds, budget: &mut Budget<'_>) -> Result<()> {
    budget.visit()?;
    match v {
        Value::Ref(a, b) => {
            let id = (*a, *b);
            if inherited & CONTENT != 0 {
                budget.insert(&mut seeds.content, id)?;
            }
            if inherited & OPAQUE != 0 {
                budget.insert(&mut seeds.opaque, id)?;
            }
            if inherited & FONT != 0 {
                budget.insert(&mut seeds.fonts, id)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                discover(value, inherited, seeds, budget)?;
            }
        }
        Value::Dict(dict) => {
            let page = v.get(b"Type").and_then(Value::name) == Some(b"Page");
            let font = v.get(b"Type").and_then(Value::name) == Some(b"Font");
            let type3 = font && v.get(b"Subtype").and_then(Value::name) == Some(b"Type3");
            for (key, value) in dict {
                let mut role = inherited;
                if page && key == b"Contents" || type3 && key == b"CharProcs" {
                    role |= CONTENT;
                }
                if matches!(key.as_slice(), b"Metadata" | b"ToUnicode" | b"UseCMap") || font && key == b"Encoding" {
                    role |= OPAQUE;
                }
                if matches!(key.as_slice(), b"FontFile" | b"FontFile2" | b"FontFile3") {
                    role |= FONT;
                }
                discover(value, role, seeds, budget)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn references(v: &Value, pending: &mut BTreeSet<Id>, seen: &BTreeSet<Id>, budget: &mut Budget<'_>) -> Result<()> {
    budget.visit()?;
    match v {
        Value::Ref(a, b) => {
            let id = (*a, *b);
            if !seen.contains(&id) {
                budget.insert(pending, id)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                references(value, pending, seen, budget)?;
            }
        }
        Value::Dict(dict) => {
            for value in dict.values() {
                references(value, pending, seen, budget)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn resolve(mut pending: BTreeSet<Id>, g: &Guard<'_>, streams: &BTreeSet<Id>, budget: &mut Budget<'_>) -> Result<BTreeSet<Id>> {
    let mut seen = BTreeSet::new();
    let mut result = BTreeSet::new();
    while let Some(id) = pending.pop_first() {
        budget.visit()?;
        if seen.contains(&id) {
            continue;
        }
        budget.insert(&mut seen, id)?;
        let record = g.objects.get(&id).ok_or_else(|| invalid("unresolved PDF stream role reference"))?;
        if streams.contains(&id) {
            budget.insert(&mut result, id)?;
        } else {
            references(&record.value, &mut pending, &seen, budget)?;
        }
    }
    Ok(result)
}
pub(super) fn classify(g: &mut Guard<'_>, streams: &BTreeSet<Id>) -> Result<Roles> {
    let mut budget = Budget {
        cancel: g.cancel,
        work: 0,
        work_limit: g.options.limits.segments.saturating_mul(16).min(16_000_000),
        objects: g.options.limits.objects,
        storage: 0,
        room: g.options.limits.decompressed_bytes.saturating_sub(g.bytes),
    };
    let mut seeds = Seeds::default();
    for (&id, r) in &g.objects {
        discover(&r.value, 0, &mut seeds, &mut budget)?;
        if streams.contains(&id) {
            if r.value.get(b"Subtype").and_then(Value::name) == Some(b"Form")
                || r.value.get(b"Type").and_then(Value::name) == Some(b"Pattern") && r.value.get(b"PatternType").and_then(Value::number) == Some(1)
            {
                budget.insert(&mut seeds.content, id)?;
            }
            if matches!(r.value.get(b"Type").and_then(Value::name), Some(b"Metadata" | b"CMap")) {
                budget.insert(&mut seeds.opaque, id)?;
            }
        }
    }
    let content = resolve(seeds.content, g, streams, &mut budget)?;
    let opaque = resolve(seeds.opaque, g, streams, &mut budget)?;
    let fonts = resolve(seeds.fonts, g, streams, &mut budget)?;
    g.charge(budget.storage)?;
    Ok(Roles { content, opaque, fonts })
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[test]
    fn inline_role_nested_repeated_references_are_deduplicated_and_bounded() {
        let mut value = Value::Array(vec![Value::Ref(9, 0); 256]);
        for _ in 0..24 {
            value = Value::Dict(BTreeMap::from([(b"Metadata".to_vec(), value)]));
        }
        let cancel = AtomicBool::new(false);
        let mut budget = Budget { cancel: &cancel, work: 0, work_limit: 1000, objects: 1, storage: 0, room: 64 };
        let mut seeds = Seeds::default();
        discover(&value, 0, &mut seeds, &mut budget).unwrap();
        assert_eq!(seeds.opaque, BTreeSet::from([(9, 0)]));
        assert_eq!(budget.storage, 64);
        assert_eq!(budget.work, 281, "each nested container and repeated reference is visited only once");
        budget.work = 0;
        budget.storage = 0;
        budget.work_limit = 100;
        assert!(matches!(discover(&value, 0, &mut Seeds::default(), &mut budget), Err(ImportError::Limit("stream role work"))));
        budget.work = 0;
        budget.work_limit = 1000;
        budget.storage = 0;
        budget.room = 63;
        assert!(matches!(discover(&value, 0, &mut Seeds::default(), &mut budget), Err(ImportError::Limit("stream role storage"))));
        let cancelled = AtomicBool::new(true);
        budget.cancel = &cancelled;
        assert!(matches!(discover(&value, 0, &mut Seeds::default(), &mut budget), Err(ImportError::Cancelled)));
    }
}
