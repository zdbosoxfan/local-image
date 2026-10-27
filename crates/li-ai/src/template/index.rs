//! The official template index (`/templates/index.json` of the connected ComfyUI, or the
//! upstream repository for diagnosis) and the model files each template declares.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::browser::CatalogFile;

/// A ComfyUI version (`0.42.5`, `v0.3.72`, `0.42.5-12-gabc`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('v');
        let core = s.split(['-', '+', ' ']).next()?;
        let mut parts = core.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().map_or(Some(0), |p| p.parse().ok())?;
        let patch = parts.next().map_or(Some(0), |p| p.parse().ok())?;
        Some(Self { major, minor, patch })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Where an index came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexSource {
    /// The connected ComfyUI (its templates match its nodes).
    #[default]
    Server,
    /// The upstream repository (diagnosis only: a newer ComfyUI ships it).
    Upstream,
}

/// One entry of the templates' `index.json`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TemplateEntry {
    pub name: String,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub models: Vec<String>,
    pub date: String,
    pub size: u64,
    /// `minComfyUIVersion`.
    pub min_comfyui: Option<Version>,
    /// `io.inputs[].nodeId` and `io.outputs[].nodeId` (`io` may be null).
    pub io_inputs: Vec<i64>,
    pub io_outputs: Vec<i64>,
    /// `requiresCustomNodes`, when an index carries it.
    pub custom_nodes: Vec<String>,
    pub source: IndexSource,
}

fn io_ids(t: &Value, k: &str) -> Vec<i64> {
    t.get("io")
        .and_then(|io| io.get(k))
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.get("nodeId").and_then(Value::as_i64)).collect())
        .unwrap_or_default()
}

/// The image templates of `index.json` that run locally (open-source, no API nodes).
pub fn parse_template_index(v: &Value) -> Vec<TemplateEntry> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter(|c| c.get("type").and_then(Value::as_str).is_none_or(|t| t == "image"))
        .flat_map(|c| c.get("templates").and_then(Value::as_array).cloned().unwrap_or_default())
        .filter(|t| t.get("openSource").and_then(Value::as_bool) != Some(false))
        .filter(|t| t.get("mediaType").and_then(Value::as_str).is_none_or(|m| m == "image"))
        .filter_map(|t| {
            let strs =
                |k: &str| t.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default();
            Some(TemplateEntry {
                name: t.get("name")?.as_str()?.to_owned(),
                title: t.get("title").and_then(Value::as_str).unwrap_or("").to_owned(),
                description: t.get("description").and_then(Value::as_str).unwrap_or("").to_owned(),
                tags: strs("tags"),
                models: strs("models"),
                date: t.get("date").and_then(Value::as_str).unwrap_or("").to_owned(),
                size: t.get("size").and_then(Value::as_u64).unwrap_or(0),
                min_comfyui: t.get("minComfyUIVersion").and_then(Value::as_str).and_then(Version::parse),
                io_inputs: io_ids(&t, "inputs"),
                io_outputs: io_ids(&t, "outputs"),
                custom_nodes: strs("requiresCustomNodes"),
                source: IndexSource::Server,
            })
        })
        .filter(|t| !t.tags.iter().any(|x| x == "API"))
        .collect()
}

/// Whether a template name is safe as a path component (`^[A-Za-z0-9_-]{1,128}$`).
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

/// A model file a template declares, with the loader node that declares it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredFile {
    pub template: String,
    pub folder: String,
    pub name: String,
    pub url: String,
    pub stem: String,
    pub node_class: String,
    pub node_id: String,
}

impl DeclaredFile {
    /// A main model file (what the template is "for").
    pub fn is_main(&self) -> bool {
        matches!(self.folder.as_str(), "diffusion_models" | "unet" | "unet_gguf" | "checkpoints")
    }
}

fn template_nodes(template: &Value) -> impl Iterator<Item = (&Value, String)> {
    let top = template["nodes"].as_array().into_iter().flatten().map(|n| (n, String::new()));
    let inner = template
        .get("definitions")
        .and_then(|d| d.get("subgraphs"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|d| d["nodes"].as_array().into_iter().flatten().map(move |n| (n, d["id"].as_str().unwrap_or("").to_owned())));
    top.chain(inner)
}

/// Every model file a template declares (`properties.models` of its nodes, subgraphs included).
pub fn declared_files(name: &str, template: &Value) -> Vec<DeclaredFile> {
    let mut out: Vec<DeclaredFile> = Vec::new();
    for (n, _) in template_nodes(template) {
        for m in n.get("properties").and_then(|p| p.get("models")).and_then(Value::as_array).into_iter().flatten() {
            let (Some(file), Some(dir)) = (m.get("name").and_then(Value::as_str), m.get("directory").and_then(Value::as_str)) else { continue };
            if out.iter().any(|f| f.name == file && f.folder == dir) {
                continue;
            }
            out.push(DeclaredFile {
                template: name.to_owned(),
                folder: dir.to_owned(),
                name: file.to_owned(),
                url: m.get("url").and_then(Value::as_str).unwrap_or("").to_owned(),
                stem: crate::stem::stem_key(file).key,
                node_class: n.get("type").and_then(Value::as_str).unwrap_or("").to_owned(),
                node_id: n.get("id").map(|v| v.to_string()).unwrap_or_default(),
            });
        }
    }
    out
}

/// The model files a template declares as catalogue files (with a hash or size when declared).
pub fn template_files(template: &Value) -> Vec<CatalogFile> {
    let mut out: Vec<CatalogFile> = Vec::new();
    for (n, _) in template_nodes(template) {
        for m in n.get("properties").and_then(|p| p.get("models")).and_then(Value::as_array).into_iter().flatten() {
            let (Some(name), Some(url), Some(dir)) =
                (m.get("name").and_then(Value::as_str), m.get("url").and_then(Value::as_str), m.get("directory").and_then(Value::as_str))
            else {
                continue;
            };
            if out.iter().any(|f| f.name == name && f.folder == dir) {
                continue;
            }
            let sha =
                m.get("hash").and_then(Value::as_str).filter(|_| m.get("hash_type").and_then(Value::as_str).is_none_or(|t| t.eq_ignore_ascii_case("sha256")));
            out.push(CatalogFile {
                name: name.to_owned(),
                folder: dir.to_owned(),
                url: url.to_owned(),
                sha256: sha.map(str::to_ascii_lowercase),
                bytes: m.get("size").or_else(|| m.get("bytes")).and_then(Value::as_u64),
                ..Default::default()
            });
        }
    }
    out
}

/// Size caps for a template body: at most 16 MiB, 5,000 nodes and subgraph depth 8 (the
/// converter checks the depth).
pub fn check_size(bytes: &[u8], v: &Value) -> Result<(), String> {
    if bytes.len() > 16 << 20 {
        return Err("template larger than 16 MiB".into());
    }
    if template_nodes(v).count() > 5000 {
        return Err("template has more than 5,000 nodes".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn versions_and_index_entries_parse() {
        assert_eq!(Version::parse("0.42.5"), Some(Version { major: 0, minor: 42, patch: 5 }));
        assert_eq!(Version::parse("v0.3.72"), Some(Version { major: 0, minor: 3, patch: 72 }));
        assert_eq!(Version::parse("0.42.5-12-gabc"), Some(Version { major: 0, minor: 42, patch: 5 }));
        assert_eq!(Version::parse("mock"), None);
        assert!(Version::parse("0.41.0") < Version::parse("0.42.5"));
        let idx = json!([{"type": "image", "templates": [
            {"name": "a", "tags": ["Text to Image"], "minComfyUIVersion": "0.41.3", "io": null, "openSource": true},
            {"name": "b", "tags": ["API"], "io": {"outputs": [{"nodeId": 9}]}},
            {"name": "c", "io": {"outputs": [{"nodeId": 9}], "inputs": [{"nodeId": 4}]}}
        ]}]);
        let e = parse_template_index(&idx);
        assert_eq!(e.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["a", "c"]);
        assert_eq!(e[0].min_comfyui, Version::parse("0.41.3"));
        assert!(e[0].io_outputs.is_empty());
        assert_eq!((e[1].io_inputs.as_slice(), e[1].io_outputs.as_slice()), (&[4][..], &[9][..]));
        assert!(valid_name("image_flux3_klein_t2i") && !valid_name("../x") && !valid_name(""));
    }
}
