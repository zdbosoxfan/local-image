//! Approved Glossy Duotone base with six reviewed upstream replacements.
use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use quick_xml::{
    Reader, Writer,
    events::{BytesStart, Event},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Manifest {
    approved: Approved,
    sets: BTreeMap<String, SourceSet>,
    sources: BTreeMap<String, Source>,
    tools: Vec<Tool>,
}
#[derive(Deserialize)]
struct Approved {
    commit: String,
    colour: String,
    monochrome: String,
}
#[derive(Deserialize)]
struct SourceSet {
    commit: String,
}
#[derive(Deserialize)]
struct Source {
    file: String,
    sha256: String,
    licence: String,
    set: String,
}
#[derive(Deserialize)]
struct Tool {
    id: String,
    label: String,
    family: String,
    mode: String,
    components: Vec<Component>,
    #[serde(default)]
    outline_width: f32,
    #[serde(default)]
    highlight_width: f32,
}
#[derive(Deserialize)]
struct Component {
    source: String,
}
#[derive(Clone, Copy)]
enum Layer {
    Body,
    Face,
    Highlight,
}

fn manifest(root: &Path) -> Result<Manifest> {
    let manifest: Manifest = serde_json::from_str(&std::fs::read_to_string(root.join("assets/icons/source-manifest.json"))?)?;
    ensure!(manifest.approved.commit.len() == 40, "approved base must have a pinned commit");
    Ok(manifest)
}

fn paint_property(key: &str) -> bool {
    matches!(
        key,
        "fill"
            | "stroke"
            | "fill-opacity"
            | "stroke-opacity"
            | "stroke-width"
            | "stroke-linecap"
            | "stroke-linejoin"
            | "stroke-dasharray"
            | "stroke-dashoffset"
            | "paint-order"
    )
}

/// Keep upstream paths, coordinates, fill rules and transforms; replace only their paint.
/// Crop's open paths become broad material bands; filled Krita shapes keep their filled faces.
fn repaint(source: &str, tool: &Tool, mono: bool, layer: Layer) -> Result<String> {
    let mut reader = Reader::from_str(source);
    let mut writer = Writer::new(Vec::new());
    let mut skip = 0;
    let mut root = true;
    let mut part = 0_usize;
    loop {
        match reader.read_event()? {
            event @ (Event::Start(_) | Event::Empty(_)) if skip == 0 => {
                let (e, empty) = match event {
                    Event::Start(e) => (e, false),
                    Event::Empty(e) => (e, true),
                    _ => unreachable!(),
                };
                let name = std::str::from_utf8(e.name().as_ref())?.to_owned();
                if matches!(name.as_str(), "metadata" | "defs" | "style" | "sodipodi:namedview") {
                    if !empty {
                        skip = 1;
                    }
                    continue;
                }
                let mut element = BytesStart::new(name.clone());
                let shape = matches!(name.as_str(), "path" | "rect" | "circle" | "ellipse" | "polygon" | "polyline" | "line");
                if shape {
                    part += 1;
                }
                for attribute in e.attributes() {
                    let attribute = attribute?;
                    let key = std::str::from_utf8(attribute.key.as_ref())?;
                    if paint_property(key) {
                        continue;
                    }
                    let value = attribute.decode_and_unescape_value(reader.decoder())?;
                    let value = if key == "style" {
                        value
                            .split(';')
                            .filter_map(|entry| {
                                let (key, value) = entry.split_once(':')?;
                                (!paint_property(key.trim())).then(|| format!("{}:{}", key.trim(), value.trim()))
                            })
                            .collect::<Vec<_>>()
                            .join(";")
                    } else {
                        value.into_owned()
                    };
                    element.push_attribute((key, value.as_str()));
                }
                if shape {
                    let metal = matches!((tool.id.as_str(), part), ("brush", 3) | ("eraser", 2) | ("blend", 1));
                    let material = if metal { "url(#mix-metal)" } else { "url(#mix-face)" };
                    let edge = if mono { "#ffffff" } else { "#1b1c20" };
                    let fill = if tool.id == "crop" || matches!(layer, Layer::Highlight) { "none" } else { material };
                    let stroke = match layer {
                        Layer::Body => edge,
                        Layer::Face => material,
                        Layer::Highlight => "url(#mix-shine)",
                    };
                    let width = match layer {
                        Layer::Body => tool.outline_width,
                        Layer::Face => 2.2,
                        Layer::Highlight => tool.highlight_width,
                    };
                    element.push_attribute(("fill", fill));
                    element.push_attribute(("stroke", stroke));
                    element.push_attribute(("stroke-width", width.to_string().as_str()));
                    element.push_attribute(("stroke-linecap", "round"));
                    element.push_attribute(("stroke-linejoin", "round"));
                    element.push_attribute(("paint-order", "stroke fill"));
                }
                writer.write_event(if empty { Event::Empty(element) } else { Event::Start(element) })?;
                if root {
                    root = false;
                    let (top, bottom) = if mono { ("#ffffff", "#ffffff") } else { palette(&tool.family).with_context(|| format!("unknown icon family: {}", tool.family))? };
                    let (metal_top, metal_bottom) = if mono { ("#ffffff", "#ffffff") } else { palette("grey").expect("built-in grey palette") };
                    let shine = if mono { "#ffffff" } else { "#F7F9FC" };
                    writer.get_mut().extend_from_slice(format!("<defs><linearGradient id=\"mix-face\" x2=\".25\" y2=\"1\"><stop stop-color=\"{top}\"/><stop offset=\"1\" stop-color=\"{bottom}\"/></linearGradient><linearGradient id=\"mix-metal\" x2=\".25\" y2=\"1\"><stop stop-color=\"{metal_top}\"/><stop offset=\"1\" stop-color=\"{metal_bottom}\"/></linearGradient><linearGradient id=\"mix-shine\" x2=\"0\" y2=\"1\"><stop stop-color=\"{shine}\" stop-opacity=\".28\"/><stop offset=\"1\" stop-color=\"{shine}\" stop-opacity=\"0\"/></linearGradient></defs>").as_bytes());
                }
            }
            Event::Start(_) => skip += 1,
            Event::End(_) if skip > 0 => skip -= 1,
            Event::Eof => break,
            Event::Decl(_) | Event::Comment(_) => {}
            event if skip == 0 => writer.write_event(event)?,
            _ => {}
        }
    }
    Ok(String::from_utf8(writer.into_inner())?)
}

fn artwork(root: &Path, manifest: &Manifest, tool: &Tool, mono: bool) -> Result<String> {
    if tool.mode == "approved" {
        ensure!(tool.components.is_empty(), "approved icon must not carry unused sources");
        let file = if mono { &manifest.approved.monochrome } else { &manifest.approved.colour };
        let approved: BTreeMap<String, String> = serde_json::from_str(&std::fs::read_to_string(root.join(file))?)?;
        return approved.get(&tool.id).cloned().context("missing approved artwork");
    }
    ensure!(tool.mode == "sourced" && tool.components.len() == 1, "unexpected source selection");
    let component = &tool.components[0];
    let source = manifest.sources.get(&component.source).context("unknown glyph")?;
    let bytes = std::fs::read(root.join(&source.file))?;
    ensure!(hex::encode(Sha256::digest(&bytes)) == source.sha256, "upstream hash changed: {}", source.file);
    ensure!(matches!(source.licence.as_str(), "CC-BY-SA-4.0" | "ISC AND MIT"), "unapproved licence");
    let text = std::str::from_utf8(&bytes)?;
    let mut images = String::new();
    let mut layers = vec![Layer::Body];
    if tool.id == "crop" {
        layers.push(Layer::Face);
    }
    layers.push(Layer::Highlight);
    for layer in layers {
        images.push_str(&format!(
            "<image width=\"24\" height=\"24\" href=\"data:image/svg+xml;base64,{}\"/>",
            STANDARD.encode(repaint(text, tool, mono, layer)?)
        ));
    }
    let commit = &manifest.sets[&source.set].commit;
    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\">\n<!-- {} @ {commit} ({}); filled Glossy Duotone adaptation GPL-3.0-or-later, {} family. -->\n<g transform=\"translate(1.5 1.5) scale(.875)\">{images}</g>\n</svg>\n",
        component.source, source.licence, tool.family
    ))
}

fn palette(family: &str) -> Option<(&'static str, &'static str)> {
    Some(match family {
        "blue" => ("#61B5FF", "#3474B5"),
        "yellow" => ("#EEC466", "#A77932"),
        "grey" => ("#CCD3DF", "#66717F"),
        "measure" => ("#99B8D6", "#526D8C"),
        "liquify" => ("#EF91BD", "#A24779"),
        "green" => ("#64D6A0", "#2A8861"),
        "cyan" => ("#55D6D0", "#287E83"),
        "purple" => ("#B293FF", "#7257B8"),
        "orange" => ("#FF9C55", "#B95635"),
        _ => return None,
    })
}

pub fn run(root: &Path) -> Result<()> {
    let manifest = manifest(root)?;
    for tool in &manifest.tools {
        for (dir, mono) in [("color", false), ("tools", true)] {
            std::fs::write(root.join(format!("assets/icons/{dir}/{}.svg", tool.id)), artwork(root, &manifest, tool, mono)?)?;
        }
    }
    let mut embedded = String::from(
        "//! Approved tool glyphs with reviewed source replacements; generated by `cargo xtask redraw-tool-icons`.\n\npub static TOOL_ICONS: &[(&str, &[u8])] = &[\n",
    );
    for tool in &manifest.tools {
        embedded.push_str(&format!("    (\"{}\", include_bytes!(\"../../../assets/icons/tools/{}.svg\")),\n", tool.id, tool.id));
    }
    embedded.push_str("];\n\n#[cfg(test)]\n#[allow(dead_code)]\npub static TOOL_LABELS: &[(&str, &str)] = &[\n");
    for tool in &manifest.tools {
        embedded.push_str(&format!("    (\"{}\", \"{}\"),\n", tool.id, tool.label));
    }
    embedded.push_str("];\n");
    std::fs::write(root.join("crates/pc-ui-egui/src/tool_icon_data.rs"), embedded)?;
    println!(
        "{} approved tools, {} reviewed source replacements generated in both styles",
        manifest.tools.len(),
        manifest.tools.iter().filter(|tool| tool.mode == "sourced").count()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_tool_artwork_is_pinned_fresh_and_preserves_the_approved_base() {
        let root = crate::root();
        let manifest = manifest(&root).unwrap();
        assert_eq!(manifest.approved.commit.len(), 40);
        let approved: BTreeMap<String, String> = serde_json::from_str(&std::fs::read_to_string(root.join(&manifest.approved.colour)).unwrap()).unwrap();
        let mut names = std::collections::BTreeSet::new();
        let mut used = std::collections::BTreeSet::new();
        for tool in &manifest.tools {
            assert!(names.insert(tool.id.clone()));
            for component in &tool.components {
                used.insert(component.source.clone());
            }
            for (dir, mono) in [("color", false), ("tools", true)] {
                assert_eq!(
                    std::fs::read_to_string(root.join(format!("assets/icons/{dir}/{}.svg", tool.id))).unwrap(),
                    artwork(&root, &manifest, tool, mono).unwrap(),
                    "stale {}",
                    tool.id
                );
            }
        }
        assert_eq!(names, approved.keys().cloned().collect());
        assert_eq!(used, manifest.sources.keys().cloned().collect());
        assert_eq!(manifest.tools.iter().filter(|tool| tool.mode == "approved").count(), 88);
        assert_eq!(manifest.tools.iter().filter(|tool| tool.mode == "sourced").count(), 6);
        for tool in manifest.tools.iter().filter(|tool| tool.mode == "sourced") {
            let source = &manifest.sources[&tool.components[0].source];
            assert_eq!(manifest.sets[&source.set].commit.len(), 40);
            let text = std::fs::read_to_string(root.join(&source.file)).unwrap();
            let geometry = |text: &str| {
                let mut reader = Reader::from_str(text);
                let mut result = Vec::new();
                loop {
                    match reader.read_event().unwrap() {
                        Event::Start(element) | Event::Empty(element)
                            if matches!(element.name().as_ref(), b"path" | b"rect" | b"circle" | b"ellipse" | b"polygon" | b"polyline" | b"line" | b"g") =>
                        {
                            for attribute in element.attributes().flatten() {
                                if matches!(
                                    attribute.key.as_ref(),
                                    b"d" | b"points"
                                        | b"transform"
                                        | b"x"
                                        | b"y"
                                        | b"width"
                                        | b"height"
                                        | b"cx"
                                        | b"cy"
                                        | b"r"
                                        | b"rx"
                                        | b"ry"
                                        | b"x1"
                                        | b"y1"
                                        | b"x2"
                                        | b"y2"
                                        | b"fill-rule"
                                ) {
                                    result.push((attribute.key.as_ref().to_vec(), attribute.value.to_vec()));
                                }
                            }
                        }
                        Event::Eof => break,
                        _ => {}
                    }
                }
                result
            };
            for mono in [false, true] {
                assert_eq!(geometry(&text), geometry(&repaint(&text, tool, mono, Layer::Body).unwrap()), "source geometry changed: {}", source.file);
            }
        }
    }
}
