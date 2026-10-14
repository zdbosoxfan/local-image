//! `cargo xtask icons`: verify the vendored tool icon set against `assets/icons/source-manifest.json`.
//!
//! Every tool icon is a pinned upstream file (Lucide 0.468.0, PhotoCraft, VectorCraft, Tabler) or a
//! Local Image original drawn to Lucide's rules; nothing is generated (the Glossy Duotone generator
//! was retired with that artwork on 2026-10-13). This check keeps the files, hashes, licences, line
//! style and registries in step. How to add an icon: `docs/DESIGN-SYSTEM.md`, section 4.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use quick_xml::{Reader, events::Event};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Manifest {
    schema: u32,
    sets: BTreeMap<String, Set>,
    tools: Vec<Tool>,
}

#[derive(Deserialize)]
struct Set {
    licence_files: Vec<String>,
}

#[derive(Deserialize)]
struct Tool {
    id: String,
    family: String,
    set: String,
    source: String,
    sha256: String,
    #[serde(default)]
    modification: Option<String>,
    #[serde(default)]
    upstream_sha256: Option<String>,
}

/// Colour-mode families (`li-ui-kit/src/color_icon_data.rs`).
const FAMILIES: [&str; 9] = ["blue", "green", "orange", "cyan", "purple", "measure", "yellow", "liquify", "grey"];

/// Lucide's style rules on the root element; Local Image originals must also be 2 px with no fills.
fn check_style(id: &str, svg: &str, original: bool) -> Result<()> {
    let mut reader = Reader::from_str(svg);
    let mut root = true;
    loop {
        match reader.read_event().with_context(|| format!("{id}: invalid SVG"))? {
            Event::Start(e) | Event::Empty(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let mut attrs = BTreeMap::new();
                for a in e.attributes() {
                    let a = a?;
                    attrs.insert(String::from_utf8_lossy(a.key.as_ref()).into_owned(), a.decode_and_unescape_value(reader.decoder())?.into_owned());
                }
                let attr = |k: &str| attrs.get(k).map(String::as_str);
                if root {
                    ensure!(name == "svg", "{id}: root element is {name}");
                    ensure!(attr("viewBox") == Some("0 0 24 24"), "{id}: viewBox must be 0 0 24 24");
                    ensure!(attr("stroke") == Some("currentColor"), "{id}: stroke must be currentColor");
                    ensure!(attr("stroke-linecap") == Some("round") && attr("stroke-linejoin") == Some("round"), "{id}: round caps and joins");
                    ensure!(attr("fill") == Some("none"), "{id}: the root fill must be none");
                    if original {
                        ensure!(attr("stroke-width") == Some("2"), "{id}: originals use Lucide's 2 px stroke");
                    }
                    root = false;
                } else if original {
                    ensure!(matches!(attr("fill"), None | Some("none")), "{id}: originals have no fills");
                    ensure!(!matches!(name.as_str(), "image" | "linearGradient" | "radialGradient" | "text" | "style"), "{id}: <{name}> is not a line drawing");
                    ensure!(attr("stroke-width").is_none() && attr("transform").is_none(), "{id}: originals keep one stroke width on the 24 px grid");
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if original {
        ensure!(svg.contains("Local Image original, GPL-3.0-or-later"), "{id}: originals record their licence in a comment");
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// The tool icon names embedded by a registry source file (`assets/icons/tools/<name>.svg`).
fn registered(source: &str) -> BTreeSet<String> {
    source.split("assets/icons/tools/").skip(1).filter_map(|rest| rest.split_once(".svg").map(|(name, _)| name.to_owned())).collect()
}

/// Returns (sourced, drawn) counts.
pub fn check(root: &Path) -> Result<(usize, usize)> {
    let path = root.join("assets/icons/source-manifest.json");
    let manifest: Manifest = serde_json::from_str(&std::fs::read_to_string(&path).with_context(|| path.display().to_string())?)?;
    ensure!(manifest.schema == 2, "unknown icon manifest schema {}", manifest.schema);
    let sources = std::fs::read_to_string(root.join("assets/icons/SOURCES.md"))?;
    let dir = root.join("assets/icons/tools");
    let mut ids = BTreeSet::new();
    let mut drawn = 0;
    for tool in &manifest.tools {
        ensure!(ids.insert(tool.id.clone()), "{}: listed twice", tool.id);
        let set = manifest.sets.get(&tool.set).with_context(|| format!("{}: unknown set {}", tool.id, tool.set))?;
        for licence in &set.licence_files {
            ensure!(root.join(licence).is_file(), "{}: missing licence file {licence}", tool.set);
        }
        ensure!(FAMILIES.contains(&tool.family.as_str()), "{}: unknown family {}", tool.id, tool.family);
        let bytes = std::fs::read(dir.join(format!("{}.svg", tool.id))).with_context(|| format!("{}: missing assets/icons/tools/{}.svg", tool.id, tool.id))?;
        ensure!(sha256(&bytes) == tool.sha256, "{}: file changed without a manifest update (sha256)", tool.id);
        ensure!(tool.modification.is_some() == tool.upstream_sha256.is_some(), "{}: a modified file records its upstream hash", tool.id);
        ensure!(!tool.source.is_empty(), "{}: no source", tool.id);
        let original = tool.set == "local-image";
        drawn += usize::from(original);
        check_style(&tool.id, std::str::from_utf8(&bytes)?, original)?;
        ensure!(sources.contains(&format!("`{}`", tool.id)), "{}: no row in assets/icons/SOURCES.md", tool.id);
    }
    let disk: BTreeSet<_> =
        std::fs::read_dir(&dir)?.map(|e| Ok(e?.path().file_stem().context("file name")?.to_string_lossy().into_owned())).collect::<Result<_>>()?;
    if disk != ids {
        bail!(
            "assets/icons/tools and the manifest differ: unlisted {:?}, missing {:?}",
            disk.difference(&ids).collect::<Vec<_>>(),
            ids.difference(&disk).collect::<Vec<_>>()
        );
    }
    let mut used = BTreeSet::new();
    for registry in ["crates/li-ui-kit/src/tool_icon_data.rs", "crates/li-ui-kit/src/vector_icon_data.rs"] {
        used.extend(registered(&std::fs::read_to_string(root.join(registry)).with_context(|| registry.to_string())?));
    }
    ensure!(
        used == ids,
        "registries and the manifest differ: unregistered {:?}, unknown {:?}",
        ids.difference(&used).collect::<Vec<_>>(),
        used.difference(&ids).collect::<Vec<_>>()
    );
    let families = std::fs::read_to_string(root.join("crates/li-ui-kit/src/color_icon_data.rs"))?;
    for tool in &manifest.tools {
        ensure!(families.contains(&format!("(\"{}\", \"{}\")", tool.id, tool.family)), "{}: colour family row differs from the manifest", tool.id);
    }
    Ok((ids.len() - drawn, drawn))
}

pub fn run(root: &Path) -> Result<()> {
    let (sourced, drawn) = check(root)?;
    println!("icons: {} tool icons ({sourced} sourced, {drawn} Local Image originals); hashes, licences, style and registries agree", sourced + drawn);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendored_tool_icons_match_the_manifest() {
        let (sourced, drawn) = check(&crate::root()).unwrap();
        assert_eq!((sourced, drawn), (144, 24));
    }

    #[test]
    fn j17_shared_icon_registries_remain_audited_through_compatibility_facades() {
        let root = crate::root();
        for facade in [
            "crates/pc-ui-egui/src/tool_icon_data.rs",
            "crates/pc-ui-egui/src/color_icon_data.rs",
            "crates/pc-ui-egui/src/vector_packages/fill_stroke_proxy/icons.rs",
        ] {
            let source = std::fs::read_to_string(root.join(facade)).unwrap();
            assert!(source.contains("pub use li_ui_kit::"), "{facade}");
        }
        assert_eq!(check(&root).unwrap(), (144, 24), "the asset audit must still cover every moved registry row");
    }

    #[test]
    fn style_rules_reject_fills_gradients_and_other_strokes_in_originals() {
        let head = |extra: &str| {
            format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><!-- Local Image original, GPL-3.0-or-later: x -->{extra}</svg>"
            )
        };
        check_style("ok", &head("<path d=\"M4 4h16\"/>"), true).unwrap();
        assert!(check_style("fill", &head("<circle cx=\"12\" cy=\"12\" r=\"4\" fill=\"currentColor\"/>"), true).is_err());
        assert!(check_style("width", &head("<path d=\"M4 4h16\" stroke-width=\"3\"/>"), true).is_err());
        assert!(check_style("gradient", &head("<linearGradient id=\"g\"/>"), true).is_err());
        // Upstream files keep their own marks (PhotoCraft's filled layer-mask dot, for example).
        check_style("upstream", &head("<circle cx=\"12\" cy=\"12\" r=\"4\" fill=\"currentColor\"/>"), false).unwrap();
        assert!(check_style("viewbox", &head("").replace("0 0 24 24", "0 0 16 16"), false).is_err());
    }
}
