//! Private Form markers bridge the pinned Device API without guessing callback order.
use super::{Guard, ImportError, Lexer, Result, Token, Value, cancelled, invalid};
use std::collections::{BTreeMap, BTreeSet};
pub(in crate::import) const PREFIX: &[u8] = b"PCPDFGroup";
pub(in crate::import) type Table = BTreeMap<Vec<u8>, bool>;
pub(super) fn tag(id: (u32, u16)) -> String {
    format!("PCPDFGroup{}g{}", id.0, id.1)
}
fn visit(g: &Guard<'_>, work: &mut usize) -> Result<()> {
    cancelled(g.cancel)?;
    *work = work.saturating_add(1);
    if *work > g.options.limits.segments.saturating_mul(16).min(16_000_000) {
        return Err(ImportError::Limit("group metadata work"));
    }
    Ok(())
}
fn resolve<'a>(mut value: &'a Value, g: &'a Guard<'_>, work: &mut usize) -> Result<&'a Value> {
    for _ in 0..g.options.limits.depth.min(64) {
        visit(g, work)?;
        match value {
            Value::Ref(a, b) => value = &g.objects.get(&(*a, *b)).ok_or_else(|| invalid("unresolved group reference"))?.value,
            _ => return Ok(value),
        }
    }
    Err(ImportError::Limit("group reference depth"))
}
fn flag(value: Option<&Value>, g: &Guard<'_>, work: &mut usize) -> Result<bool> {
    match value.map(|v| resolve(v, g, work)).transpose()? {
        None | Some(Value::Bool(false)) => Ok(false),
        Some(Value::Bool(true)) => Ok(true),
        _ => Err(invalid("transparency group flag")),
    }
}
fn callback_wrappers(value: &Value, flags: &mut (bool, bool), g: &Guard<'_>, work: &mut usize) -> Result<()> {
    visit(g, work)?;
    match value {
        Value::Dict(dict) => {
            flags.0 |= value.get(b"Subtype").and_then(Value::name) == Some(b"Type3");
            flags.1 |= value.get(b"PatternType").and_then(Value::number) == Some(1) && value.get(b"PaintType").and_then(Value::number) != Some(1);
            for value in dict.values() {
                callback_wrappers(value, flags, g, work)?;
            }
        }
        Value::Array(array) => {
            for value in array {
                callback_wrappers(value, flags, g, work)?;
            }
        }
        _ => {}
    }
    Ok(())
}
// A deliberately conservative whole-document proof handles inherited and default
// paints without assuming a particular caller. Every executable stream and every
// image resource must preserve neutral RGB; unsupported legacy cases are warned,
// not reclassified as verified conversion. No final RGB-to-gray shortcut is used.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ResourceRole {
    Other,
    Resources,
    ExtGStates,
    GraphicsState,
    SoftMask,
}
fn neutral_resources(v: &Value, g: &Guard<'_>, work: &mut usize, role: ResourceRole) -> Result<bool> {
    visit(g, work)?;
    match v {
        Value::Dict(dict) => {
            // Role discovery only seeds direct selectors. Do not prove neutral
            // content while an indirect selector can hide an executable Form,
            // page, Type3 glyph, pattern, or colored image from that inventory.
            for key in [b"Type".as_slice(), b"Subtype", b"PatternType", b"PaintType"] {
                if matches!(v.get(key), Some(Value::Ref(_, _))) {
                    return Ok(false);
                }
            }
            for key in [b"Type".as_slice(), b"Subtype"] {
                if v.get(key).is_some_and(|value| value.name().is_none()) {
                    return Ok(false);
                }
            }
            let role = if v.get(b"Type").and_then(Value::name) == Some(b"ExtGState") { ResourceRole::GraphicsState } else { role };
            // TR2 is checked even in a real soft-mask dictionary. A resource
            // named SMask must never evade effective graphics-transfer checks.
            if let Some(transfer) = v.get(b"TR2")
                && !matches!(resolve(transfer, g, work)?.name(), Some(b"Identity" | b"Default"))
            {
                return Ok(false);
            }
            if let Some(transfer) = v.get(b"TR") {
                let transfer = resolve(transfer, g, work)?;
                let scalar_mask =
                    role == ResourceRole::SoftMask && matches!(v.get(b"S").and_then(Value::name), Some(b"Alpha" | b"Luminosity")) && v.get(b"G").is_some();
                if scalar_mask {
                    if !matches!(transfer, Value::Dict(_)) && transfer.name() != Some(b"Identity") {
                        return Ok(false);
                    }
                } else if v.get(b"TR2").is_none() && !matches!(transfer.name(), Some(b"Identity" | b"Default")) {
                    return Ok(false);
                }
            }
            if dict.contains_key(b"DefaultGray".as_slice()) || dict.contains_key(b"DefaultRGB".as_slice()) {
                return Ok(false);
            }
            if v.get(b"Subtype").and_then(Value::name) == Some(b"Type3") {
                return Ok(false);
            }
            if v.get(b"Subtype").and_then(Value::name) == Some(b"Image")
                && !matches!(v.get(b"ImageMask"), Some(Value::Bool(true)))
                && v.get(b"ColorSpace").map(|v| resolve(v, g, work)).transpose()?.and_then(Value::name) != Some(b"DeviceGray")
            {
                return Ok(false);
            }
            if let Some(bc) = v.get(b"BC") {
                let Value::Array(values) = resolve(bc, g, work)? else { return Ok(false) };
                let Some(Value::Number(first)) = values.first() else { return Ok(false) };
                if !first.is_finite() || !(0.0..=1.0).contains(first) || !values.iter().all(|v| matches!(v, Value::Number(n) if n == first)) {
                    return Ok(false);
                }
            }
            if v.get(b"S").and_then(Value::name) == Some(b"Transparency")
                && let Some(cs) = v.get(b"CS")
                && !matches!(resolve(cs, g, work)?.name(), Some(b"DeviceGray" | b"DeviceRGB"))
            {
                return Ok(false);
            }
            let resource_owner = v.get(b"Type").and_then(Value::name) == Some(b"Page")
                || v.get(b"Subtype").and_then(Value::name) == Some(b"Form")
                || v.get(b"PatternType").and_then(Value::number) == Some(1)
                || v.get(b"Group").is_some() && v.get(b"BBox").is_some();
            for (key, value) in dict {
                // Only structural roles grant the scalar-mask exemption;
                // arbitrary keys in resource-name maps cannot grant a role.
                let child_role = match role {
                    ResourceRole::Resources if key == b"ExtGState" => ResourceRole::ExtGStates,
                    ResourceRole::ExtGStates => ResourceRole::GraphicsState,
                    ResourceRole::GraphicsState if key == b"SMask" => ResourceRole::SoftMask,
                    _ if resource_owner && key == b"Resources" => ResourceRole::Resources,
                    _ => ResourceRole::Other,
                };
                if !neutral_resources(value, g, work, child_role)? {
                    return Ok(false);
                }
            }
        }
        Value::Array(array) => {
            for value in array {
                if !neutral_resources(value, g, work, ResourceRole::Other)? {
                    return Ok(false);
                }
            }
        }
        _ => {}
    }
    Ok(true)
}
fn neutral_document(g: &Guard<'_>, content: &BTreeSet<(u32, u16)>, work: &mut usize) -> Result<bool> {
    // No positive result is cached across roles. Standalone/aliased referenced
    // dictionaries are additionally inspected as Other, conservatively losing
    // scalar exemptions rather than borrowing an exemption from another use.
    for record in g.objects.values() {
        if !neutral_resources(&record.value, g, work, ResourceRole::Other)? {
            return Ok(false);
        }
    }
    for (id, data) in &g.streams {
        if !content.contains(id) {
            continue;
        }
        let mut lexer = Lexer::new(data, g.options, g.cancel);
        while let Some(token) = lexer.next()? {
            visit(g, work)?;
            if let Token::Word(word) = token {
                if std::str::from_utf8(&word).ok().and_then(|w| w.parse::<f64>().ok()).is_some() {
                    continue;
                }
                // No color-space selection, RGB/CMYK paints, patterns, shadings,
                // inline-image bytes or unknown operators enter the proof.
                if !matches!(
                    word.as_slice(),
                    b"g" | b"G"
                        | b"q"
                        | b"Q"
                        | b"cm"
                        | b"w"
                        | b"J"
                        | b"j"
                        | b"M"
                        | b"d"
                        | b"ri"
                        | b"i"
                        | b"gs"
                        | b"m"
                        | b"l"
                        | b"c"
                        | b"v"
                        | b"y"
                        | b"h"
                        | b"re"
                        | b"S"
                        | b"s"
                        | b"f"
                        | b"F"
                        | b"f*"
                        | b"B"
                        | b"B*"
                        | b"b"
                        | b"b*"
                        | b"n"
                        | b"W"
                        | b"W*"
                        | b"Do"
                        | b"BMC"
                        | b"BDC"
                        | b"EMC"
                        | b"MP"
                        | b"DP"
                        | b"BX"
                        | b"EX"
                        | b"true"
                        | b"false"
                        | b"null"
                        | b"BT"
                        | b"ET"
                        | b"Tc"
                        | b"Tw"
                        | b"Tz"
                        | b"TL"
                        | b"Tf"
                        | b"Tr"
                        | b"Ts"
                        | b"Td"
                        | b"TD"
                        | b"Tm"
                        | b"T*"
                        | b"Tj"
                        | b"TJ"
                        | b"'"
                        | b"\""
                ) {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}
pub(super) fn collect(g: &mut Guard<'_>, streams: &BTreeSet<(u32, u16)>, content: &BTreeSet<(u32, u16)>) -> Result<(Table, bool)> {
    let mut table = Table::new();
    let mut work = 0;
    let mut neutral = None;
    let mut unverified = false;
    let mut non_rgb_space = false;
    let mut nonisolated = false;
    let mut wrappers = (false, false);
    // Object and reference depth are already bounded. Table storage is charged
    // before allocation; cancellation is checked between every resource.
    for (&id, record) in &g.objects {
        cancelled(g.cancel)?;
        callback_wrappers(&record.value, &mut wrappers, g, &mut work)?;
        let Some(group) = record.value.get(b"Group") else { continue };
        let group = resolve(group, g, &mut work)?;
        if group.get(b"S").and_then(Value::name) != Some(b"Transparency") {
            return Err(ImportError::Unsupported("unknown Form group type"));
        }
        let isolated = flag(group.get(b"I"), g, &mut work)?;
        if flag(group.get(b"K"), g, &mut work)? {
            return Err(ImportError::Unsupported("knockout transparency groups"));
        }
        if let Some(cs) = group.get(b"CS") {
            let space = resolve(cs, g, &mut work)?.name();
            if space != Some(b"DeviceRGB") {
                non_rgb_space = true;
                if !isolated && streams.contains(&id) {
                    return Err(ImportError::Unsupported("nonisolated group blending colour space"));
                }
                // Existing isolated/page handling remains available. Only the
                // bounded all-neutral proof earns verified DeviceGray status.
                let proven = if space == Some(b"DeviceGray") {
                    match neutral {
                        Some(result) => result,
                        None => {
                            let result = neutral_document(g, content, &mut work)?;
                            neutral = Some(result);
                            result
                        }
                    }
                } else {
                    false
                };
                if !isolated && !proven {
                    return Err(ImportError::Unsupported("unverified default page group blending colour space"));
                }
                unverified |= !proven;
            }
        }
        if !streams.contains(&id) {
            continue;
        } // The page is already the initial compositing group.
        if !matches!(record.value.get(b"Subtype").and_then(Value::name), None | Some(b"Form")) || record.value.get(b"BBox").is_none() {
            return Err(ImportError::Unsupported("group on non-Form stream"));
        }
        nonisolated |= !isolated;
        let storage = (table.len() as u64 + 1).saturating_mul(128);
        if storage > g.options.limits.decompressed_bytes.saturating_sub(g.bytes) {
            return Err(ImportError::Limit("group metadata storage"));
        }
        table.insert(tag(id).into_bytes(), isolated);
    }
    // Pinned uncolored Type3 devices drop both group and marker callbacks.
    // Do not admit backdrop-dependent groups until reachability can be proved.
    if nonisolated && wrappers.0 {
        return Err(ImportError::Unsupported("nonisolated groups with Type3 fonts"));
    }
    if nonisolated && wrappers.1 {
        return Err(ImportError::Unsupported("nonisolated groups with stencil patterns"));
    }
    if nonisolated && non_rgb_space {
        return Err(ImportError::Unsupported("nonisolated groups with inherited non-RGB blending space"));
    }
    g.charge((table.len() as u64).saturating_mul(128))?;
    Ok((table, unverified))
}
