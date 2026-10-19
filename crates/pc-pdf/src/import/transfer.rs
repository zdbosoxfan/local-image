//! Full-precision soft-mask transfer functions. hayro exposes a mask's /TR only as an
//! 8-bit lookup table, so the importer finds the original function and evaluates it exactly.
use super::{Result, cancelled};
use hayro_interpret::{Function, MaskType, SoftMask};
use hayro_syntax::{
    Pdf,
    object::{Dict, MaybeRef, Name, Object, ObjectIdentifier},
};
use std::{collections::HashMap, sync::atomic::AtomicBool};

/// Transfer functions of every soft-mask dictionary, keyed by its /G group and alpha type.
pub(super) struct Transfers(HashMap<(ObjectIdentifier, bool), Vec<Function>>);

impl Transfers {
    /// Visits each object and its directly nested dictionaries and arrays once.
    pub fn scan(pdf: &Pdf, cancel: &AtomicBool) -> Result<Self> {
        let mut map: HashMap<_, Vec<Function>> = HashMap::new();
        let mut stack = vec![];
        for object in pdf.objects() {
            cancelled(cancel)?;
            stack.push(object);
            while let Some(object) = stack.pop() {
                let dict = match object {
                    Object::Dict(d) => d,
                    Object::Stream(s) => s.dict().clone(),
                    Object::Array(a) => {
                        stack.extend(a.raw_iter().filter_map(direct));
                        continue;
                    }
                    _ => continue,
                };
                if let Some(key) = mask_key(&dict)
                    && let Some(function) = dict.get::<Object<'_>>(b"TR").and_then(|o| Function::new(&o))
                {
                    map.entry(key).or_default().push(function);
                }
                stack.extend(dict.entries().filter_map(|(_, v)| direct(v)));
            }
        }
        Ok(Self(map))
    }
    /// The candidate whose samples equal hayro's table. Candidates sharing a group and
    /// table differ by less than one 8-bit step; the first one is used.
    pub fn find(&self, mask: &SoftMask<'_>, table: &[u8; 256]) -> Option<&Function> {
        self.0.get(&(mask.id(), mask.mask_type() == MaskType::Alpha))?.iter().find(|f| &samples(f) == table)
    }
}

/// Evaluates a transfer function as PDF 1.7 §11.6.5.2 specifies, clamped to [0, 1].
pub(super) fn apply(f: &Function, v: f32) -> f32 {
    f.eval([v].as_slice().into()).and_then(|o| o.first().copied()).unwrap_or(0.0).clamp(0.0, 1.0)
}

/// Matches hayro's own 8-bit sampling of a transfer function.
fn samples(f: &Function) -> [u8; 256] {
    std::array::from_fn(|i| f.eval([i as f32 / 255.0].as_slice().into()).and_then(|o| o.first().copied()).map_or(i as u8, |o| (o * 255.0 + 0.5) as u8))
}

fn mask_key(dict: &Dict<'_>) -> Option<(ObjectIdentifier, bool)> {
    let alpha = match dict.get::<Name<'_>>(b"S")?.as_ref() {
        b"Alpha" => true,
        b"Luminosity" => false,
        _ => return None,
    };
    Some((dict.get_ref(b"G")?.into(), alpha))
}

fn direct(v: MaybeRef<Object<'_>>) -> Option<Object<'_>> {
    if let MaybeRef::NotRef(o) = v { Some(o) } else { None }
}
