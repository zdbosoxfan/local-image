//! ComfyUI API-format graphs (`{"id": {"class_type", "inputs", "_meta"}}`): conversion from the
//! editor's UI format ([`ui`]), canonicalisation of official templates ([`canon`]) and static
//! validation against `/object_info` ([`validate`]). These helpers read and edit the graph as a
//! JSON object; an input is a link when its value is `[node id, output slot]`.

pub mod canon;
pub mod ui;
pub mod validate;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

/// The source of a linked input, `(node, output slot)`.
pub fn link_of(v: &Value) -> Option<(&str, u64)> {
    let a = v.as_array()?;
    if a.len() != 2 {
        return None;
    }
    Some((a[0].as_str()?, a[1].as_u64()?))
}

pub fn link(node: &str, slot: u64) -> Value {
    json!([node, slot])
}

pub fn class_of<'a>(g: &'a Map<String, Value>, id: &str) -> &'a str {
    g.get(id).and_then(|n| n["class_type"].as_str()).unwrap_or("")
}

pub fn inputs(n: &Value) -> Option<&Map<String, Value>> {
    n.get("inputs").and_then(Value::as_object)
}

/// Every consumer of `(node, slot)`: `(consumer, input)`, in id order.
pub fn consumers(g: &Map<String, Value>, node: &str, slot: u64) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (id, n) in g {
        for (k, v) in inputs(n).into_iter().flatten() {
            if link_of(v) == Some((node, slot)) {
                out.push((id.clone(), k.clone()));
            }
        }
    }
    out
}

/// `node` and every node it reads from, transitively.
pub fn ancestors(g: &Map<String, Value>, node: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut todo = vec![node.to_owned()];
    while let Some(id) = todo.pop() {
        if !g.contains_key(&id) || !seen.insert(id.clone()) {
            continue;
        }
        for v in g[&id].get("inputs").and_then(Value::as_object).into_iter().flat_map(|o| o.values()) {
            if let Some((src, _)) = link_of(v) {
                todo.push(src.to_owned());
            }
        }
    }
    seen
}

/// Keeps only `keep` and their ancestors.
pub fn prune_to(g: &mut Map<String, Value>, keep: &[&str]) {
    let mut alive = BTreeSet::new();
    for k in keep {
        alive.extend(ancestors(g, k));
    }
    g.retain(|id, _| alive.contains(id));
}

/// A new node id `li_<purpose>` (or `li_<purpose>_<n>` when taken).
pub fn fresh_id(g: &Map<String, Value>, purpose: &str) -> String {
    let base = format!("li_{purpose}");
    if !g.contains_key(&base) {
        return base;
    }
    (1..).map(|n| format!("{base}_{n}")).find(|id| !g.contains_key(id)).unwrap_or(base)
}

/// Points every input that reads `(node, slot)` at `to` instead (except inputs of `except`).
pub fn rewire(g: &mut Map<String, Value>, from: (&str, u64), to: &Value, except: &[&str]) {
    let from = link(from.0, from.1);
    for (id, n) in g.iter_mut() {
        if except.contains(&id.as_str()) {
            continue;
        }
        if let Some(ins) = n.get_mut("inputs").and_then(Value::as_object_mut) {
            for v in ins.values_mut() {
                if *v == from {
                    *v = to.clone();
                }
            }
        }
    }
}

pub fn add_node(g: &mut Map<String, Value>, purpose: &str, class: &str, inputs: Value) -> String {
    let id = fresh_id(g, purpose);
    g.insert(id.clone(), json!({ "class_type": class, "inputs": inputs }));
    id
}

/// Canonical bytes: keys sorted (the workspace `serde_json` map is ordered), no whitespace.
pub fn canonical_bytes(g: &Value) -> Vec<u8> {
    serde_json::to_vec(g).unwrap_or_default()
}

/// The node ids in a stable order: numeric scope parts compared as numbers (`2:10` after `2:9`).
pub fn sorted_ids(g: &Map<String, Value>) -> Vec<String> {
    let mut ids: Vec<String> = g.keys().cloned().collect();
    ids.sort_by_key(|id| id.split(':').map(|p| p.parse::<u64>().map_or((1, 0, p.to_owned()), |n| (0, n, String::new()))).collect::<Vec<_>>());
    ids
}

/// Replaces node ids by structural signatures so two graphs that differ only in ids compare
/// equal: each node becomes `class(inputs with links replaced by the source's signature)`.
pub fn normalised(g: &Map<String, Value>) -> BTreeMap<String, Value> {
    fn sig(g: &Map<String, Value>, id: &str, memo: &mut BTreeMap<String, String>, depth: u32) -> String {
        if let Some(s) = memo.get(id) {
            return s.clone();
        }
        if depth > 256 {
            return format!("<cycle {id}>");
        }
        let Some(n) = g.get(id) else { return format!("<missing {id}>") };
        let mut parts = Vec::new();
        for (k, v) in inputs(n).into_iter().flatten() {
            let val = match link_of(v) {
                Some((src, slot)) => format!("{}#{slot}", sig(g, src, memo, depth + 1)),
                None => v.to_string(),
            };
            parts.push(format!("{k}={val}"));
        }
        let s = format!("{}({})", class_of(g, id), parts.join(","));
        memo.insert(id.to_owned(), s.clone());
        s
    }
    let mut memo = BTreeMap::new();
    let mut out = BTreeMap::new();
    for id in g.keys() {
        let s = sig(g, id, &mut memo, 0);
        let n = &g[id];
        let mut node = json!({ "class_type": n["class_type"] });
        let mut ins = Map::new();
        for (k, v) in inputs(n).into_iter().flatten() {
            ins.insert(
                k.clone(),
                match link_of(v) {
                    Some((src, slot)) => json!([sig(g, src, &mut memo, 0), slot]),
                    None => v.clone(),
                },
            );
        }
        node["inputs"] = Value::Object(ins);
        let mut key = s;
        while out.contains_key(&key) {
            key.push('\'');
        }
        out.insert(key, node);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_helpers_follow_links() {
        let mut g = json!({
            "1": {"class_type": "A", "inputs": {"x": 1}},
            "2": {"class_type": "B", "inputs": {"a": ["1", 0]}},
            "3": {"class_type": "C", "inputs": {"b": ["2", 0], "a": ["1", 0]}},
            "9": {"class_type": "Orphan", "inputs": {}}
        })
        .as_object()
        .unwrap()
        .clone();
        assert_eq!(consumers(&g, "1", 0), vec![("2".to_owned(), "a".to_owned()), ("3".to_owned(), "a".to_owned())]);
        assert_eq!(ancestors(&g, "3").into_iter().collect::<Vec<_>>(), ["1", "2", "3"]);
        let id = add_node(&mut g, "x", "D", json!({"in": ["1", 0]}));
        assert_eq!(id, "li_x");
        assert_eq!(fresh_id(&g, "x"), "li_x_1");
        rewire(&mut g, ("1", 0), &link(&id, 0), &[&id]);
        assert_eq!(g["2"]["inputs"]["a"], json!(["li_x", 0]));
        assert_eq!(g["li_x"]["inputs"]["in"], json!(["1", 0]));
        prune_to(&mut g, &["3"]);
        assert!(!g.contains_key("9") && g.contains_key("li_x"));
        assert_eq!(sorted_ids(&json!({"2:10": 0, "2:9": 0, "10": 0, "li_a": 0}).as_object().unwrap().clone()), ["2:9", "2:10", "10", "li_a"]);
        let renamed = json!({"a": {"class_type": "A", "inputs": {"x": 1}}, "b": {"class_type": "B", "inputs": {"a": ["a", 0]}}});
        let original = json!({"1": {"class_type": "A", "inputs": {"x": 1}}, "2": {"class_type": "B", "inputs": {"a": ["1", 0]}}});
        assert_eq!(normalised(renamed.as_object().unwrap()), normalised(original.as_object().unwrap()));
    }
}
