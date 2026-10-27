//! The document order of dynamic-combo option inputs in a raw `/object_info` response.
//!
//! `/object_info` lists each node's top-level inputs in `input_order`, but the inputs nested in a
//! `COMFY_DYNAMICCOMBO_V3` option (`TextGenerate.sampling_mode` "on": temperature, top_k, top_p,
//! …) have no order field: only the JSON object's key order says it, and the editor fills their
//! widget values in that order. The workspace `serde_json` map sorts keys, so this pass walks the
//! raw bytes with its own visitor, records only those names, and skips everything else.
//!
//! The result is stored in the inventory under [`ORDER_KEY`] as
//! `{class: {input path: {option: {"required": [..], "optional": [..]}}}}`; a nested combo inside
//! an option has the path `outer.inner`.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::{Value, json};

/// Where the nested order lives in an [`super::ObjectInfo`].
pub const ORDER_KEY: &str = "__li_order";

const DYNAMIC_COMBO: &str = "COMFY_DYNAMICCOMBO_V3";

type Names = (Vec<String>, Vec<String>);
type Records = BTreeMap<(String, String, String), Names>;

/// The nested order of every dynamic combo in a raw `/object_info` body.
pub fn nested_order(bytes: &[u8]) -> serde_json::Result<Value> {
    let mut records = Records::new();
    let mut de = serde_json::Deserializer::from_slice(bytes);
    de.deserialize_map(Top(&mut records))?;
    let mut out = json!({});
    for ((class, path, option), (required, optional)) in records {
        out[&class][&path][&option] = json!({ "required": required, "optional": optional });
    }
    Ok(out)
}

struct Top<'a>(&'a mut Records);

impl<'de> Visitor<'de> for Top<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an object of node classes")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(class) = map.next_key::<String>()? {
            map.next_value_seed(Class { class: &class, out: &mut *self.0 })?;
        }
        Ok(())
    }
}

struct Class<'a> {
    class: &'a str,
    out: &'a mut Records,
}

impl<'de> DeserializeSeed<'de> for Class<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Class<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a node class")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(key) = map.next_key::<String>()? {
            if key == "input" {
                map.next_value_seed(Sections { class: self.class, out: &mut *self.out })?;
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(())
    }
    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }
}

/// `{"required": {...}, "optional": {...}, "hidden": {...}}`.
struct Sections<'a> {
    class: &'a str,
    out: &'a mut Records,
}

impl<'de> DeserializeSeed<'de> for Sections<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Sections<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("input sections")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(key) = map.next_key::<String>()? {
            if key == "required" || key == "optional" {
                let mut names = Vec::new();
                map.next_value_seed(Inputs { class: self.class, prefix: "", out: &mut *self.out, names: &mut names })?;
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(())
    }
}

/// `{name: [type, options], ...}`: records the names in order and descends into dynamic combos.
struct Inputs<'a> {
    class: &'a str,
    prefix: &'a str,
    out: &'a mut Records,
    names: &'a mut Vec<String>,
}

impl<'de> DeserializeSeed<'de> for Inputs<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Inputs<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an object of inputs")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(name) = map.next_key::<String>()? {
            let path = if self.prefix.is_empty() { name.clone() } else { format!("{}.{name}", self.prefix) };
            map.next_value_seed(Spec { class: self.class, path: &path, out: &mut *self.out })?;
            self.names.push(name);
        }
        Ok(())
    }
}

/// `[type, options]`: only a dynamic combo's options are read.
struct Spec<'a> {
    class: &'a str,
    path: &'a str,
    out: &'a mut Records,
}

impl<'de> DeserializeSeed<'de> for Spec<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
    }
}

/// The first element of a spec: a type name, or a legacy combo's list of choices.
struct TypeName;

impl<'de> DeserializeSeed<'de> for TypeName {
    type Value = Option<String>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Option<String>, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for TypeName {
    type Value = Option<String>;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a type name or a list of choices")
    }
    fn visit_str<E>(self, v: &str) -> Result<Option<String>, E> {
        Ok(Some(v.to_owned()))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Option<String>, A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(None)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Option<String>, A::Error> {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(None)
    }
    fn visit_unit<E>(self) -> Result<Option<String>, E> {
        Ok(None)
    }
    fn visit_bool<E>(self, _: bool) -> Result<Option<String>, E> {
        Ok(None)
    }
    fn visit_i64<E>(self, _: i64) -> Result<Option<String>, E> {
        Ok(None)
    }
    fn visit_u64<E>(self, _: u64) -> Result<Option<String>, E> {
        Ok(None)
    }
    fn visit_f64<E>(self, _: f64) -> Result<Option<String>, E> {
        Ok(None)
    }
}

impl<'de> Visitor<'de> for Spec<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an input spec")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let ty = seq.next_element_seed(TypeName)?.flatten();
        if ty.as_deref() == Some(DYNAMIC_COMBO) {
            seq.next_element_seed(ComboOptions { class: self.class, path: self.path, out: &mut *self.out })?;
        }
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(())
    }
    fn visit_str<E>(self, _: &str) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }
}

/// A dynamic combo's options object (`{"options": [{key, inputs}], ...}`).
struct ComboOptions<'a> {
    class: &'a str,
    path: &'a str,
    out: &'a mut Records,
}

impl<'de> DeserializeSeed<'de> for ComboOptions<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ComboOptions<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("dynamic combo options")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(key) = map.next_key::<String>()? {
            if key == "options" {
                map.next_value_seed(OptionList { class: self.class, path: self.path, out: &mut *self.out })?;
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(())
    }
}

struct OptionList<'a> {
    class: &'a str,
    path: &'a str,
    out: &'a mut Records,
}

impl<'de> DeserializeSeed<'de> for OptionList<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for OptionList<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a list of options")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while seq.next_element_seed(OneOption { class: self.class, path: self.path, out: &mut *self.out })?.is_some() {}
        Ok(())
    }
}

/// `{"key": ..., "inputs": {"required": {...}, "optional": {...}}}` in either key order.
struct OneOption<'a> {
    class: &'a str,
    path: &'a str,
    out: &'a mut Records,
}

impl<'de> DeserializeSeed<'de> for OneOption<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for OneOption<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a dynamic combo option")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut option = None;
        let mut names: Names = (Vec::new(), Vec::new());
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "key" => option = Some(map.next_value::<Value>()?.as_str().map(str::to_owned).unwrap_or_default()),
                "inputs" => map.next_value_seed(OptionInputs { class: self.class, path: self.path, out: &mut *self.out, names: &mut names })?,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        if let Some(option) = option {
            self.out.insert((self.class.to_owned(), self.path.to_owned(), option), names);
        }
        Ok(())
    }
}

struct OptionInputs<'a> {
    class: &'a str,
    path: &'a str,
    out: &'a mut Records,
    names: &'a mut Names,
}

impl<'de> DeserializeSeed<'de> for OptionInputs<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for OptionInputs<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("option inputs")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(section) = map.next_key::<String>()? {
            let names = match section.as_str() {
                "required" => &mut self.names.0,
                "optional" => &mut self.names.1,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                    continue;
                }
            };
            map.next_value_seed(Inputs { class: self.class, prefix: self.path, out: &mut *self.out, names })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_inputs_keep_document_order() {
        let raw = br#"{
            "TextGenerate": {"input": {"required": {"prompt": ["STRING", {}], "sampling_mode": ["COMFY_DYNAMICCOMBO_V3", {"options": [
                {"inputs": {"required": {"temperature": ["FLOAT", {}], "top_k": ["INT", {}], "seed": ["INT", {}]}, "optional": {"presence_penalty": ["FLOAT", {}]}}, "key": "on"},
                {"key": "off", "inputs": {"required": {}}},
                {"key": "nested", "inputs": {"required": {"zeta": ["COMFY_DYNAMICCOMBO_V3", {"options": [{"key": "a", "inputs": {"required": {"b2": ["INT"], "a1": ["INT"]}}}]}]}}}
            ]}]}, "optional": {"mode": [["x", "y"], {}]}}, "output": ["STRING"], "description": {"ignored": [1, 2, {"deep": null}]}},
            "Other": {"input": {"required": {"x": ["INT", {"default": 1}]}}}
        }"#;
        let order = nested_order(raw).unwrap();
        assert_eq!(order["TextGenerate"]["sampling_mode"]["on"]["required"], json!(["temperature", "top_k", "seed"]));
        assert_eq!(order["TextGenerate"]["sampling_mode"]["on"]["optional"], json!(["presence_penalty"]));
        assert_eq!(order["TextGenerate"]["sampling_mode"]["off"]["required"], json!([]));
        assert_eq!(order["TextGenerate"]["sampling_mode.zeta"]["a"]["required"], json!(["b2", "a1"]));
        assert!(order.get("Other").is_none());
    }
}
