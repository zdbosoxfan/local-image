//! The Library's MCP tools (moved from `lightcraft-mcp`'s `tools.rs`): helpers for the common
//! loop (import → select → develop → render → export) plus `library_command_list` /
//! `library_command_run`, which reach every Library command. [`crate::PhotocraftMcp`] serves them
//! next to the editor's tools; the CLI's `library` subcommands call the same functions.

use serde_json::{Map, Value, json};

use super::{LibraryHeadless, encode_image};
use crate::budgets::MAX_PREVIEW_SIDE;

/// Prefix of every Library tool name.
pub const TOOL_PREFIX: &str = "library_";

/// Largest render a trusted (CLI) session returns; automation is held to [`MAX_PREVIEW_SIDE`].
pub const MAX_TRUSTED_RENDER: u64 = 16384;

/// What a Library tool returns.
#[derive(Clone, Debug, PartialEq)]
pub enum LibraryReply {
    /// A JSON result.
    Json(Value),
    /// An encoded image (PNG or JPEG) with its metadata (`id`, `width`, `height`, `path`?).
    Image { bytes: Vec<u8>, mime: &'static str, info: Value },
}

impl LibraryReply {
    /// The JSON result, or the image's metadata.
    pub fn json(&self) -> &Value {
        match self {
            LibraryReply::Json(v) => v,
            LibraryReply::Image { info, .. } => info,
        }
    }
}

fn tool(name: &str, title: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": format!("{TOOL_PREFIX}{name}"),
        "title": title,
        "description": description,
        "inputSchema": {"type": "object", "properties": properties, "required": required},
    })
}

fn ids_schema(what: &str) -> Value {
    json!({"type": "array", "items": {"type": "integer"}, "description": what})
}

/// The Library tools as MCP tool definitions (`name`, `title`, `description`, `inputSchema`).
pub fn tool_definitions() -> Vec<Value> {
    let photo_id = json!({"type": "integer", "description": "Photo id (default: the active photo). Makes that photo active first."});
    vec![
        tool(
            "command_list",
            "List Library commands",
            "Every Library command this session can run, with id, label, menu, shortcut, parameter doc and whether it is enabled now. Run one with library_command_run.",
            json!({
                "filter": {"type": "string", "description": "Only commands whose id or label contains this text (case-insensitive)"},
                "enabled_only": {"type": "boolean", "description": "Only commands that can run right now"}
            }),
            &[],
        ),
        tool(
            "command_run",
            "Run Library command",
            "Run any Library command by id with JSON params (see library_command_list for ids and parameter docs), e.g. {command: \"photo.rate\", params: {rating: 4}}. Edits are undoable (edit.undo / edit.redo).",
            json!({"command": {"type": "string"}, "params": {"type": "object", "description": "Command parameters", "additionalProperties": true}}),
            &["command"],
        ),
        tool(
            "import",
            "Import photos",
            "Add image/raw files to the Library. Paths are forward-slash relative paths below the automation read root (local CLI: any path); folders are scanned recursively for photos. The first imported photo becomes active. Returns the new photo ids.",
            json!({
                "paths": {"type": "array", "items": {"type": "string"}, "description": "Files and/or folders"},
                "album": {"type": "integer", "description": "Also add to this album id"},
                "mode": {"type": "string", "enum": ["add", "copy", "move"], "description": "add (default) = reference the files in place. copy / move (local CLI only) = copy them into `destination` (default: the library's Originals/); move then removes each verified original and its XMP sidecars from the source"},
                "destination": {"type": "string", "description": "Copy or move: destination folder"},
                "organize": {"type": "string", "description": "Copy or move: folders inside the destination — date (YYYY/YYYY-MM-DD, default), month, flat, or a template such as {date:%Y}/{date:%Y%m%d}"},
                "rename": {"type": "string", "description": "Copy or move: file-name template, e.g. {date:%Y%m%d_%H%M%S}_{seq:3} (photo.renameTokens lists the tags); the extension is kept"}
            }),
            &["paths"],
        ),
        tool(
            "query_photos",
            "Query photos",
            "List photos (id, file name, size, rating, flag, edited…). Without filter/sort: the current Library view.",
            json!({
                "filter": {"type": "object", "description": "Catalog Filter JSON (e.g. {\"minRating\": 3})", "additionalProperties": true},
                "sort": {"type": "object", "additionalProperties": true},
                "offset": {"type": "integer"},
                "limit": {"type": "integer", "description": "Default 200"}
            }),
            &[],
        ),
        tool(
            "select_photos",
            "Select photos",
            "Set the selection; the active photo (the one develop tools edit) is `active` or the first id.",
            json!({"ids": ids_schema("Photo ids"), "active": {"type": "integer"}, "mode": {"type": "string", "enum": ["replace", "add", "toggle", "range"]}}),
            &["ids"],
        ),
        tool(
            "list_controls",
            "List develop controls",
            "Every develop slider (id like `light.exposure`, label, section, min, max, default, current value) for the active photo.",
            json!({"section": {"type": "string", "description": "Only this section (e.g. light, color, effects, detail)"}}),
            &[],
        ),
        tool(
            "get_develop",
            "Get develop settings",
            "The full develop settings JSON of a photo (white balance, light, color, curves, mixer, grading, effects, crop, masks, spots…).",
            json!({"id": photo_id}),
            &[],
        ),
        tool(
            "set_develop",
            "Set develop settings",
            "Edit a photo (one undo step each for values and settings). `values` sets sliders by control id (see library_list_controls), e.g. {\"light.exposure\": 0.7, \"light.contrast\": 20}. `settings` deep-merges a partial develop-settings JSON (as returned by library_get_develop).",
            json!({
                "id": photo_id,
                "values": {"type": "object", "additionalProperties": {"type": "number"}, "description": "controlId → value"},
                "settings": {"type": "object", "additionalProperties": true, "description": "Partial DevelopSettings JSON to merge"},
                "label": {"type": "string", "description": "History label for `settings`"}
            }),
            &[],
        ),
        tool(
            "apply_preset",
            "Apply preset",
            "Apply a develop preset (ids from library_command_run presets.list) to the selection or `ids`, at `amount` percent (0..200, default 100).",
            json!({"preset": {"type": "string", "description": "Preset id"}, "amount": {"type": "number"}, "ids": ids_schema("Photo ids (default: selection)")}),
            &["preset"],
        ),
        tool(
            "crop",
            "Crop / straighten",
            "Set the crop of a photo: `rect` [x0,y0,x1,y1] in normalized coordinates (0..1, origin top-left) of the straightened frame, `angle` in degrees. `reset: true` clears it. Give at least one of them.",
            json!({"id": photo_id, "rect": {"type": "array", "items": {"type": "number"}, "minItems": 4, "maxItems": 4}, "angle": {"type": "number"}, "reset": {"type": "boolean"}}),
            &[],
        ),
        tool(
            "render_photo",
            "Render photo",
            "Render a photo with its current develop settings (cropped) and return it as an image so you can look at the result.",
            json!({
                "id": photo_id,
                "size": {"type": "integer", "description": format!("Long edge in pixels (default 1024, max {MAX_PREVIEW_SIDE})")},
                "format": {"type": "string", "enum": ["png", "jpeg"], "description": "Image encoding (default png; jpeg is smaller)"},
                "path": {"type": "string", "description": "Also save the rendered file here (below the automation write root); never over an original"}
            }),
            &[],
        ),
        tool(
            "export",
            "Export photos",
            "Render and write photos (below the automation write root; its folders must exist). Give `path` (one photo; format from the extension unless `format` is set) or `dir` (batch, file names from `naming`; default: the write root). Never writes over an original.",
            json!({
                "id": photo_id,
                "ids": {"type": "array", "items": {"type": "integer"}, "description": "Photos to export (default: the selection, else the given/active photo)"},
                "path": {"type": "string"},
                "dir": {"type": "string"},
                "format": {"type": "string", "enum": ["jpeg", "png", "tiff", "webp", "avif", "original", "dng"], "description": "original = the file as is + an XMP sidecar with the edits; dng = raw photos as DNG with the edits embedded"},
                "longEdge": {"type": "integer", "description": "Output long edge in px. When no size param is given: 3000; 0 = full size (cropped, native resolution)"},
                "shortEdge": {"type": "integer", "description": "Output short edge in px"},
                "width": {"type": "integer", "description": "Output width in px (with height: fit inside width × height, either orientation)"},
                "height": {"type": "integer", "description": "Output height in px"},
                "megapixels": {"type": "number", "description": "Output size in megapixels"},
                "percent": {"type": "number", "description": "Output size as a percentage of full size"},
                "dontEnlarge": {"type": "boolean", "description": "Never upscale past the photo's own size (default true)"},
                "ppi": {"type": "integer", "description": "Print resolution written into the file (default 240)"},
                "quality": {"type": "integer", "description": "JPEG/AVIF quality 1..100"},
                "limitKb": {"type": "integer", "description": "JPEG: largest quality that fits this many KB"},
                "sharpen": {"type": "string", "enum": ["none", "screen", "matte", "glossy"]},
                "sharpenAmount": {"type": "string", "enum": ["low", "standard", "high"]},
                "naming": {"type": "string", "description": "File-name template, e.g. {name}-{seq} or {date:%Y-%m-%d}_{title}_{seq:2} (tokens: {name} {num} {seq} {seq:N} {date} {date:%…} {folder} {camera} {lens} {iso} {rating} {title} {creator} {ext})"},
                "startNumber": {"type": "integer", "description": "First {seq} value (default 1)"},
                "metadata": {"type": "string", "enum": ["all", "allExceptCamera", "copyright", "none"]},
                "removeLocation": {"type": "boolean"},
                "colorSpace": {"type": "string", "enum": ["srgb", "displayP3", "adobeRgb", "proPhoto", "rec2020"], "description": "Output colour space (default sRGB; AVIF is always sRGB). adobeRgb = Adobe RGB (1998) compatible; the embedded ICC profile is generated from the published primaries"},
                "bitDepth": {"type": "integer", "enum": [8, 10, 16, 32], "description": "Bits per channel: PNG 8|16 (default 8), TIFF 8|16|32 (default 16; 32 = linear float with a linear profile), AVIF 8|10; JPEG/WebP are 8-bit"},
                "watermark": {"description": "Text, or {text, vertical (boolean; defaults to false), size (fraction of short edge), opacity, anchor (topLeft|top|topRight|left|center|right|bottomLeft|bottom|bottomRight), inset, color [r,g,b], shadow}"}
            }),
            &[],
        ),
    ]
}

fn exec(lib: &mut LibraryHeadless, command: &str, params: Value) -> Result<Value, String> {
    lib.call("engine.execute", json!({"command": command, "params": params}))
}

/// Make `args.id` (if any) the active photo.
fn activate(lib: &mut LibraryHeadless, args: &Value) -> Result<(), String> {
    if let Some(id) = args.get("id").and_then(Value::as_u64) {
        exec(lib, "library.select", json!({"ids": [id], "active": id}))?;
    }
    Ok(())
}

fn obj(args: &Value, keys: &[&str]) -> Value {
    let mut m = Map::new();
    for k in keys {
        if let Some(v) = args.get(*k).filter(|v| !v.is_null()) {
            m.insert((*k).to_string(), v.clone());
        }
    }
    Value::Object(m)
}

const EXPORT_KEYS: &[&str] = &[
    "ids",
    "path",
    "dir",
    "format",
    "longEdge",
    "shortEdge",
    "width",
    "height",
    "megapixels",
    "percent",
    "dontEnlarge",
    "ppi",
    "quality",
    "limitKb",
    "sharpen",
    "sharpenAmount",
    "naming",
    "startNumber",
    "metadata",
    "removeLocation",
    "watermark",
    "colorSpace",
    "bitDepth",
];

fn render_photo(lib: &mut LibraryHeadless, args: &Value) -> Result<LibraryReply, String> {
    let save_to = args.get("path").and_then(Value::as_str);
    if let Some(p) = save_to {
        // Refuse a bad target before rendering (the write checks again).
        exec(lib, "export.checkTarget", json!({"path": p}))?;
    }
    let max = if lib.is_trusted() { MAX_TRUSTED_RENDER } else { u64::from(MAX_PREVIEW_SIDE) };
    let size = args.get("size").and_then(Value::as_u64).unwrap_or(1024).clamp(16, max);
    let id = match args.get("id").and_then(Value::as_u64) {
        Some(id) => Some(id),
        None => {
            let s = exec(lib, "library.state", json!({}))?;
            s["selection"]["active"].as_u64().or(s["selection"]["ids"][0].as_u64())
        }
    };
    let id = id.ok_or("no active photo: import or select one, or pass `id`")?;
    let img = lib.render(&json!({"id": id, "size": size}), size, max)?;
    let jpeg = matches!(args.get("format").and_then(Value::as_str), Some("jpeg" | "jpg"));
    let bytes = if jpeg { encode_image("jpg", &img, 88)? } else { encode_image("png", &img, 92)? };
    let mut info = json!({"id": id, "width": img.width, "height": img.height});
    if let Some(p) = save_to {
        lib.write_file(p, &bytes)?;
        info["path"] = json!(p);
    }
    Ok(LibraryReply::Image { bytes, mime: if jpeg { "image/jpeg" } else { "image/png" }, info })
}

/// Run one Library tool (`library_<name>`, or the bare name).
pub fn call_tool(lib: &mut LibraryHeadless, name: &str, args: &Value) -> Result<LibraryReply, String> {
    let args = if args.is_null() { &json!({}) } else { args };
    if !args.is_object() {
        return Err("tool arguments must be an object".into());
    }
    let s = |k: &str| args.get(k).and_then(Value::as_str);
    let name = name.strip_prefix(TOOL_PREFIX).unwrap_or(name);
    let json = |r: Result<Value, String>| r.map(LibraryReply::Json);
    match name {
        "command_list" => {
            let filter = s("filter").unwrap_or("").to_lowercase();
            let enabled_only = args.get("enabled_only").and_then(Value::as_bool).unwrap_or(false);
            let Value::Array(all) = lib.call("engine.commands", json!({}))? else { return Err("command list is not an array".into()) };
            let items = all
                .into_iter()
                .filter(|c| {
                    let hay = format!("{} {}", c["id"].as_str().unwrap_or(""), c["label"].as_str().unwrap_or("")).to_lowercase();
                    (filter.is_empty() || hay.contains(&filter)) && (!enabled_only || c["enabled"].as_bool().unwrap_or(true))
                })
                .collect();
            Ok(LibraryReply::Json(Value::Array(items)))
        }
        "command_run" => match s("command") {
            Some(c) => json(exec(lib, c, args.get("params").cloned().filter(|p| !p.is_null()).unwrap_or(json!({})))),
            None => Err("missing `command`".into()),
        },
        "import" => {
            let r = lib.import(args)?;
            Ok(LibraryReply::Json(lib.display(r)))
        }
        "query_photos" => json(exec(lib, "catalog.query", obj(args, &["filter", "sort", "offset", "limit"]))),
        "select_photos" => {
            let mut p = obj(args, &["ids", "active", "mode"]);
            if p.get("active").is_none()
                && let Some(first) = args["ids"].get(0)
            {
                p["active"] = first.clone();
            }
            json(exec(lib, "library.select", p))
        }
        "list_controls" => json(exec(lib, "develop.controls", obj(args, &["section"]))),
        "get_develop" => json(exec(lib, "develop.get", obj(args, &["id"]))),
        "set_develop" => {
            activate(lib, args)?;
            let values = args.get("values").filter(|v| v.as_object().is_some_and(|o| !o.is_empty()));
            let settings = args.get("settings").filter(|v| v.as_object().is_some_and(|o| !o.is_empty()));
            if values.is_none() && settings.is_none() {
                return Err("give `values` ({controlId: number}) and/or `settings` (partial develop JSON)".to_string());
            }
            if let Some(v) = values {
                exec(lib, "develop.set", json!({"values": v}))?;
            }
            if let Some(st) = settings {
                let mut p = json!({"settings": st});
                if let Some(l) = args.get("label") {
                    p["label"] = l.clone();
                }
                exec(lib, "develop.merge", p)?;
            }
            // Echo the resulting values of the touched sliders.
            let controls = exec(lib, "develop.controls", json!({}))?;
            let touched: Vec<Value> = match (values, controls) {
                (Some(Value::Object(v)), Value::Array(all)) => all
                    .into_iter()
                    .filter(|c| c["id"].as_str().is_some_and(|id| v.contains_key(id)))
                    .map(|c| json!({"id": c["id"], "value": c["value"]}))
                    .collect(),
                _ => Vec::new(),
            };
            Ok(LibraryReply::Json(json!({"ok": true, "controls": touched})))
        }
        "apply_preset" => match s("preset").or(s("id")) {
            Some(p) => {
                let mut params = obj(args, &["amount", "ids"]);
                params["id"] = json!(p);
                json(exec(lib, "preset.apply", params))
            }
            None => Err("missing `preset`".into()),
        },
        "crop" if args.get("rect").is_none() && args.get("angle").is_none() && args.get("reset").and_then(Value::as_bool) != Some(true) => {
            Err("give `rect`, `angle` or `reset: true`".into())
        }
        "crop" => {
            activate(lib, args)?;
            if args.get("reset").and_then(Value::as_bool) == Some(true) {
                json(exec(lib, "crop.reset", json!({})))
            } else {
                json(exec(lib, "crop.set", obj(args, &["rect", "angle"])))
            }
        }
        "render_photo" => render_photo(lib, args),
        "export" => {
            activate(lib, args)?;
            json(lib.call("app.export", obj(args, EXPORT_KEYS)))
        }
        other => Err(format!("unknown Library tool `{TOOL_PREFIX}{other}` (see tools/list)")),
    }
}
