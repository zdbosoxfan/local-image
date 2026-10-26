//! `filter.*` commands (Photoshop's Filter menu) backed by `photocraft-algo`.
//!
//! Every command acts on the active pixel layer (or the cached pixels of a
//! smart object, recording a smart filter), respects the selection, and is
//! undoable. `filter.lastFilter` re-runs the most recent filter command.

use photocraft_algo::{self as algo, Distribution, FilterParams, PolarMode, Preserve, RadialMethod, RippleSize, SpherizeMode, UndefinedAreas, WaveType};
use photocraft_doc::{LayerContent, SmartFilter};
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

fn f(p: &Value, k: &str, d: f32) -> f32 {
    p.get(k).and_then(Value::as_f64).map_or(d, |v| v as f32)
}
fn i(p: &Value, k: &str, d: i32) -> i32 {
    // Clamped like the neighbouring filter params: an out-of-range value saturates instead of
    // wrapping through `as i32` (`3e9` used to come back as a negative offset).
    p.get(k).and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f.round() as i64))).map_or(d, |v| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
}
fn b(p: &Value, k: &str, d: bool) -> bool {
    p.get(k).and_then(Value::as_bool).unwrap_or(d)
}
fn s<'a>(p: &'a Value, k: &str, d: &'a str) -> &'a str {
    p.get(k).and_then(Value::as_str).unwrap_or(d)
}
fn seed(p: &Value) -> u32 {
    // Stored records keep their tolerant fallback outside the documented unsigned range.
    p.get("seed").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).unwrap_or_else(|| i(p, "seed", 0) as u32)
}
fn undefined(p: &Value) -> UndefinedAreas {
    match s(p, "undefinedAreas", "wrap") {
        "repeat" => UndefinedAreas::Repeat,
        "transparent" => UndefinedAreas::Transparent,
        _ => UndefinedAreas::Wrap,
    }
}

/// Minimum / Maximum › Preserve (Photoshop's default is Squareness).
fn preserve(p: &Value) -> Preserve {
    if s(p, "preserve", "squareness") == "roundness" { Preserve::Roundness } else { Preserve::Squareness }
}

/// One-step presets (no dialog): the history step they record (their own name, not the generic
/// algorithm's) and the fixed parameters, tuned to match the reference app's fixed-strength filters.
fn preset(id: &str) -> Option<(&'static str, FilterParams)> {
    Some(match id {
        "filter.blur.blur" => ("Blur", FilterParams::GaussianBlur { radius: 0.6 }),
        "filter.blur.blurMore" => (
            "Blur More",
            FilterParams::Custom {
                kernel: vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 1.0, 0.0, 0.0, 2.0, 2.0, 2.0, 0.0, 0.0, 1.0, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                scale: 14.0,
                offset: 0.0,
            },
        ),
        "filter.sharpen.sharpen" => ("Sharpen", FilterParams::UnsharpMask { amount: 60.0, radius: 0.5, threshold: 0.0 }),
        "filter.sharpen.sharpenMore" => ("Sharpen More", FilterParams::UnsharpMask { amount: 150.0, radius: 0.6, threshold: 0.0 }),
        "filter.sharpen.sharpenEdges" => ("Sharpen Edges", FilterParams::UnsharpMask { amount: 100.0, radius: 0.8, threshold: 6.0 }),
        "filter.noise.despeckle" => ("Despeckle", FilterParams::SurfaceBlur { radius: 2.0, threshold: 12.0 }),
        _ => return None,
    })
}

/// Builds the algorithm parameters for a filter command id from JSON params
/// (Photoshop dialog units).
pub fn params_for(id: &str, p: &Value) -> Option<FilterParams> {
    if let Some((_, fp)) = preset(id) {
        return Some(fp);
    }
    Some(match id {
        "filter.blur.gaussianBlur" => FilterParams::GaussianBlur { radius: f(p, "radius", 1.0).clamp(0.1, 1000.0) },
        "filter.blur.boxBlur" => FilterParams::BoxBlur { radius: f(p, "radius", 1.0).clamp(1.0, 2000.0) },
        "filter.blur.motionBlur" => FilterParams::MotionBlur { angle: f(p, "angle", 0.0), distance: f(p, "distance", 10.0).clamp(1.0, 2000.0) },
        "filter.blur.radialBlur" => FilterParams::RadialBlur {
            amount: f(p, "amount", 10.0).clamp(1.0, 100.0),
            method: if s(p, "method", "spin") == "zoom" { RadialMethod::Zoom } else { RadialMethod::Spin },
            center_x: f(p, "centerX", 0.5),
            center_y: f(p, "centerY", 0.5),
        },
        "filter.blur.surfaceBlur" => {
            FilterParams::SurfaceBlur { radius: f(p, "radius", 5.0).clamp(1.0, 100.0), threshold: f(p, "threshold", 15.0).clamp(2.0, 255.0) }
        }
        "filter.sharpen.unsharpMask" => FilterParams::UnsharpMask {
            amount: f(p, "amount", 50.0).clamp(1.0, 500.0),
            radius: f(p, "radius", 1.0).clamp(0.1, 1000.0),
            threshold: f(p, "threshold", 0.0).clamp(0.0, 255.0),
        },
        "filter.sharpen.smartSharpen" => FilterParams::SmartSharpen {
            amount: f(p, "amount", 100.0).clamp(1.0, 500.0),
            radius: f(p, "radius", 1.0).clamp(0.1, 64.0),
            reduce_noise: f(p, "reduceNoise", 10.0).clamp(0.0, 100.0),
        },
        "filter.other.highPass" => FilterParams::HighPass { radius: f(p, "radius", 10.0).clamp(0.1, 1000.0) },
        "filter.noise.addNoise" => FilterParams::AddNoise {
            amount: f(p, "amount", 12.5).clamp(0.1, 400.0),
            distribution: if s(p, "distribution", "uniform") == "gaussian" { Distribution::Gaussian } else { Distribution::Uniform },
            monochromatic: b(p, "monochromatic", false),
            seed: seed(p),
        },
        "filter.noise.median" => FilterParams::Median { radius: f(p, "radius", 1.0).clamp(1.0, 500.0) },
        "filter.noise.dustAndScratches" => {
            FilterParams::DustAndScratches { radius: f(p, "radius", 1.0).clamp(1.0, 500.0), threshold: f(p, "threshold", 0.0).clamp(0.0, 255.0) }
        }
        "filter.other.minimum" => FilterParams::Minimum { radius: f(p, "radius", 1.0).clamp(0.2, 500.0), preserve: preserve(p) },
        "filter.other.maximum" => FilterParams::Maximum { radius: f(p, "radius", 1.0).clamp(0.2, 500.0), preserve: preserve(p) },
        "filter.other.offset" => FilterParams::Offset { horizontal: i(p, "horizontal", 0), vertical: i(p, "vertical", 0), undefined: undefined(p) },
        "filter.pixelate.mosaic" => FilterParams::Mosaic { cell_size: f(p, "cellSize", 10.0).clamp(2.0, 200.0) },
        "filter.stylize.emboss" => {
            FilterParams::Emboss { angle: f(p, "angle", 135.0), height: f(p, "height", 3.0).clamp(1.0, 100.0), amount: f(p, "amount", 100.0).clamp(1.0, 500.0) }
        }
        "filter.stylize.findEdges" => FilterParams::FindEdges,
        "filter.stylize.solarize" => FilterParams::Solarize,
        "filter.distort.twirl" => FilterParams::Twirl { angle: f(p, "angle", 50.0).clamp(-999.0, 999.0) },
        "filter.distort.pinch" => FilterParams::Pinch { amount: f(p, "amount", 50.0).clamp(-100.0, 100.0) },
        "filter.distort.spherize" => FilterParams::Spherize {
            amount: f(p, "amount", 100.0).clamp(-100.0, 100.0),
            mode: match s(p, "mode", "normal") {
                "horizontalOnly" => SpherizeMode::HorizontalOnly,
                "verticalOnly" => SpherizeMode::VerticalOnly,
                _ => SpherizeMode::Normal,
            },
        },
        "filter.distort.wave" => FilterParams::Wave {
            generators: i(p, "generators", 5).clamp(1, 999) as u32,
            wavelength_min: f(p, "wavelengthMin", 10.0).clamp(1.0, 998.0),
            wavelength_max: f(p, "wavelengthMax", 120.0).clamp(2.0, 999.0),
            amplitude_min: f(p, "amplitudeMin", 5.0).clamp(1.0, 998.0),
            amplitude_max: f(p, "amplitudeMax", 35.0).clamp(1.0, 999.0),
            wave_type: match s(p, "type", "sine") {
                "triangle" => WaveType::Triangle,
                "square" => WaveType::Square,
                _ => WaveType::Sine,
            },
            undefined: undefined(p),
            seed: seed(p),
        },
        "filter.distort.ripple" => FilterParams::Ripple {
            amount: f(p, "amount", 100.0).clamp(-999.0, 999.0),
            size: match s(p, "size", "medium") {
                "small" => RippleSize::Small,
                "large" => RippleSize::Large,
                _ => RippleSize::Medium,
            },
        },
        "filter.distort.polarCoordinates" => FilterParams::PolarCoordinates {
            mode: if s(p, "mode", "rectangularToPolar") == "polarToRectangular" { PolarMode::PolarToRectangular } else { PolarMode::RectangularToPolar },
        },
        _ => return crate::filters_ext::params_for(id, p),
    })
}

/// Runs filter command `id` with JSON `params` on `surf` (the pixel function behind every
/// `filter.*` command; smart filters re-run it on re-render). `bounds` frames distortions;
/// `selection` limits the result. Like layer filters, neighbourhood filters repeat the edge pixels
/// of `canvas` ∪ the surface's content instead of fading in transparency, and the output is
/// clipped to it. `Ok(None)` for ids that aren't pixel filters. `profile` is the document's RGB
/// profile (`color_cmds::composite_profile`), which the Camera Raw Filter (`filter.develop`)
/// converts through; `None` = sRGB. `Err` when the requesting session's gate denies the stored
/// command or params (an ambient path the session may not read): callers surface it rather than
/// render unfiltered pixels.
pub fn apply_filter_to_surface_in(
    id: &str,
    params: &Value,
    surf: &photocraft_raster::Surface,
    bounds: photocraft_geom::Rect,
    selection: Option<&photocraft_raster::Surface>,
    canvas: photocraft_geom::Rect,
    profile: Option<&photocraft_cms::Profile>,
) -> Result<Option<photocraft_raster::Surface>> {
    // Re-renders bypass command dispatch; stored params must still obey the session's gate
    // before any resolver below reads a file they name.
    crate::jobs::authorize_render_command(id, params)?;
    if id == crate::develop_filter_cmds::DEVELOP {
        return Ok(crate::develop_filter_cmds::apply_to_surface(id, params, surf, canvas, profile));
    }
    // Liquify / Puppet Warp / Perspective Warp smart filters (distort_cmds).
    if let Some(out) = crate::distort_cmds::apply_to_surface(id, params, surf, canvas) {
        return Ok(Some(out));
    }
    // Lens Correction / Adaptive Wide Angle / Camera Raw smart filters (lens_cmds).
    if let Some(out) = crate::lens_cmds::apply_to_surface_in(id, params, surf, canvas, profile) {
        return Ok(Some(out));
    }
    // Image › Adjustments recorded as smart filters (Levels, Curves, Shadows/Highlights… from PSD).
    if let Some(kind) = id.strip_prefix("image.adjustments.") {
        return Ok(crate::adjust_cmds::adjust_as_filter(kind, params, surf));
    }
    // WebAssembly plug-in smart filters (plugin_cmds).
    if id == crate::plugin_cmds::RUN {
        return Ok(crate::plugin_cmds::apply_to_surface(id, params, surf, selection, canvas));
    }
    let Some(fp) = params_for(id, params) else { return Ok(None) };
    if matches!(fp, FilterParams::Displace { map: None, .. }) && params.get("__displacePsd").is_some() {
        return Err(EngineError::Other("Displace's embedded map is unreadable; choose another map".into()));
    }
    let sel_bounds = selection.map(photocraft_raster::Surface::content_bounds);
    let content = surf.content_bounds();
    let area = algo::output_area(&fp, content, bounds, sel_bounds);
    Ok(Some(algo::apply_in(surf, &fp, area, bounds, selection, canvas.union(&content))))
}

pub(crate) fn has_filterable_layer(s: &Session) -> std::result::Result<(), String> {
    // A targeted alpha channel or Quick Mask is filtered instead of the layer (channel_cmds).
    if crate::channel_cmds::edits_channel(s) {
        return Ok(());
    }
    let d = s.active().ok_or("no document open")?;
    let id = d.active_layer.ok_or("no active layer")?;
    let l = d.doc.layer(id).ok_or("no active layer")?;
    match &l.content {
        LayerContent::Raster(_) => Ok(()),
        LayerContent::Smart(sm) if sm.cache.is_some() => Ok(()),
        other => Err(format!("filters need a pixel layer (active layer is a {} layer)", other.kind_name())),
    }
}

/// A strictly validated filter parameter: its dialog-unit contract.
#[derive(Clone, Copy)]
enum Rule {
    Number(f64, f64),
    Choice(&'static str),
    Bool,
}

impl Rule {
    fn accepts(self, v: &Value) -> bool {
        match self {
            Rule::Number(min, max) => v.as_f64().is_some_and(|n| n.is_finite() && (min..=max).contains(&n)),
            Rule::Choice(choices) => v.as_str().is_some_and(|s| choices.split('|').any(|c| c == s)),
            Rule::Bool => v.is_boolean(),
        }
    }

    fn expected(self) -> String {
        match self {
            Rule::Number(min, max) => format!("a number in {min}..{max}"),
            Rule::Choice(choices) => format!("one of {choices}"),
            Rule::Bool => "a boolean".into(),
        }
    }
}

/// The filters whose new invocations are checked against their documented ranges and choices
/// instead of clamped. Stored smart-filter records still decode tolerantly through `params_for`.
fn strict_fields(id: &str) -> Option<&'static [(&'static str, Rule)]> {
    use Rule::{Bool, Choice, Number};
    Some(match id {
        "filter.blur.smartBlur" => &[
            ("radius", Number(0.1, 100.0)),
            ("threshold", Number(0.1, 100.0)),
            ("quality", Choice("high|medium|low")),
            ("mode", Choice("normal|edgeOnly|overlayEdge")),
        ],
        "filter.blur.shapeBlur" => &[("radius", Number(5.0, 1000.0)), ("shape", Choice("circle|ring|square|diamond|triangle|hexagon|star|heart|cross"))],
        "filter.noise.reduceNoise" => &[
            ("strength", Number(0.0, 10.0)),
            ("preserveDetails", Number(0.0, 100.0)),
            ("reduceColorNoise", Number(0.0, 100.0)),
            ("sharpenDetails", Number(0.0, 100.0)),
            ("removeJpegArtifact", Bool),
        ],
        _ => return None,
    })
}

/// Rejects unknown keys and out-of-range values for the filters in [`strict_fields`], before any
/// side effect (it runs at dispatch, ahead of committing a floating selection, and again in
/// [`run_filter`] for Last Filter). Other commands pass unchecked.
pub(crate) fn validate_params(id: &str, p: &Value) -> Result<()> {
    let Some(fields) = strict_fields(id) else { return Ok(()) };
    let bad = |msg: String| EngineError::BadParams { cmd: id.to_string(), msg };
    if p.is_null() {
        return Ok(());
    }
    let p = p.as_object().ok_or_else(|| bad("params must be a JSON object".into()))?;
    for (key, value) in p {
        let (valid, expected) = match fields.iter().find(|(name, _)| name == key) {
            Some((_, rule)) => (rule.accepts(value), rule.expected()),
            None => match key.as_str() {
                "layer" => (value.as_u64().is_some(), "an unsigned layer id".into()),
                "coalesce" => (value.is_string(), "a string".into()),
                "target" => (
                    value.as_str().is_some_and(|v| matches!(v, "pixels" | "mask" | "quickMask"))
                        || value.as_object().is_some_and(|o| o.len() == 1 && o.get("channel").is_some_and(|c| c.as_u64().is_some())),
                    "pixels, mask, quickMask or {\"channel\": index}".into(),
                ),
                _ => return Err(bad(format!("unknown parameter `{key}`"))),
            },
        };
        if !valid {
            return Err(bad(format!("`{key}` must be {expected}")));
        }
    }
    Ok(())
}

pub(crate) fn run_filter(s: &mut Session, id: &str, p: &Value) -> Result<Value> {
    validate_params(id, p)?;
    // Session state some filters read (colours) becomes explicit params; external inputs resolve here.
    let mut prepared = crate::filters_ext::prepare(s, id, p);
    if id == "filter.distort.displace" {
        prepared = crate::filters_ext::displace_request(&prepared);
    }
    let p = &prepared;
    if let Some(gate) = s.authorize {
        gate(id, p)?;
    }
    let mut fp = params_for(id, p).ok_or_else(|| EngineError::Other(format!("unknown filter {id}")))?;
    crate::filters_ext::resolve(s, &mut fp, p)?;
    let layer = match p.get("layer").and_then(Value::as_u64) {
        Some(l) => photocraft_doc::LayerId(l),
        None => s.active().and_then(|d| d.active_layer).ok_or(EngineError::Other("no active layer".into()))?,
    };
    let label = preset(id).map_or(fp.label(), |(name, _)| name).to_string();
    let msg = label.clone();
    let mut params = p.clone();
    if let Value::Object(m) = &mut params {
        m.remove("__kind");
    }
    let id = id.to_string();
    let p = p.clone();
    let layer_id = layer.0;
    // A background job when started with `Session::start` (#210): the filter runs on a worker,
    // checking for cancellation before every tile.
    crate::jobs::edit_job(
        s,
        &label,
        move |doc, _, ctx| {
            let filter = |surf: &photocraft_raster::Surface, fp: &FilterParams, area, bounds, sel: Option<&photocraft_raster::Surface>, extent| {
                ctx.stage(0.0, 1.0, &msg, |ctl| algo::apply_in_with(surf, fp, area, bounds, sel, extent, ctl)).ok_or(EngineError::Cancelled)
            };
            let selection = doc.selection.clone();
            let sel_bounds = selection.as_ref().map(photocraft_raster::Surface::content_bounds);
            // Distortions centre on the selection when there is one, else the canvas.
            let doc_bounds = doc.bounds();
            let bounds = sel_bounds.filter(|b| !b.is_empty()).unwrap_or(doc_bounds);
            // Neighbourhood filters repeat edge pixels at the canvas edge (or past it, where the
            // layer has off-canvas pixels) instead of fading in transparency, as Photoshop does.
            if let Some(surf) = crate::channel_cmds::channel_surface_for_filter(doc, Some(layer), &p)? {
                let content = surf.content_bounds();
                let area = algo::output_area(&fp, content, bounds, sel_bounds);
                *surf = filter(surf, &fp, area, bounds, selection.as_ref(), doc_bounds.union(&content))?;
                return Ok(fp.clone());
            }
            let l = doc.layer_mut(layer).ok_or(EngineError::NoLayer(layer))?;
            let mut fp = fp.clone();
            crate::filters_ext::resolve_in_layer(&mut fp, l, bounds);
            let surf = match &mut l.content {
                LayerContent::Raster(surf) => surf,
                LayerContent::Smart(_) => {
                    let mut params = match &fp {
                        FilterParams::Displace { map: Some(map), .. } if photocraft_io::smart_map::displace_embedded_map(&params).is_none() => {
                            photocraft_io::smart_map::displace_embed_map(&params, map).ok_or_else(|| {
                                EngineError::Other("Displace: choose a smaller map (at most 4096 x 4096 pixels), or rasterize the layer first".into())
                            })?
                        }
                        _ => params.clone(),
                    };
                    if matches!(fp, FilterParams::Displace { .. }) {
                        crate::filters_ext::displace_settle(&mut params)?;
                    }
                    // Non-destructive: record the filter and re-render the smart object from its source.
                    let sf = SmartFilter { command: id.clone(), params, blend: photocraft_color::BlendMode::Normal, opacity: 1.0, visible: true };
                    return crate::smart_cmds::add_smart_filter(doc, layer, sf, selection.as_ref()).map(|()| fp);
                }
                _ => return Err(EngineError::Other("not a pixel layer".into())),
            };
            let content = surf.content_bounds();
            let area = algo::output_area(&fp, content, bounds, sel_bounds);
            *surf = filter(surf, &fp, area, bounds, selection.as_ref(), doc_bounds.union(&content))?;
            Ok(fp)
        },
        move |fp| json!({ "layer": layer_id, "filter": serde_json::to_value(&fp).unwrap_or(Value::Null) }),
    )
}

macro_rules! filter_cmd {
    ($id:literal, $label:literal, [$($m:literal),*], $params:literal) => {
        CommandSpec {
            id: $id,
            label: $label,
            menu: &[$($m),*],
            shortcut: None,
            params: $params,
            enabled: has_filterable_layer,
            run: |s, p| run_filter(s, $id, p),
            journal: true,
        }
    };
}

/// The filter command specs.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        filter_cmd!("filter.blur.gaussianBlur", "Gaussian Blur…", ["Filter", "Blur"], r##"{"radius":0.1..1000=1}"##),
        filter_cmd!("filter.blur.blur", "Blur", ["Filter", "Blur"], "{}"),
        filter_cmd!("filter.blur.blurMore", "Blur More", ["Filter", "Blur"], "{}"),
        filter_cmd!("filter.sharpen.sharpen", "Sharpen", ["Filter", "Sharpen"], "{}"),
        filter_cmd!("filter.sharpen.sharpenMore", "Sharpen More", ["Filter", "Sharpen"], "{}"),
        filter_cmd!("filter.sharpen.sharpenEdges", "Sharpen Edges", ["Filter", "Sharpen"], "{}"),
        filter_cmd!("filter.noise.despeckle", "Despeckle", ["Filter", "Noise"], "{}"),
        filter_cmd!("filter.blur.boxBlur", "Box Blur…", ["Filter", "Blur"], r##"{"radius":1..2000=1}"##),
        filter_cmd!("filter.blur.motionBlur", "Motion Blur…", ["Filter", "Blur"], r##"{"angle":-360..360=0,"distance":1..2000=10}"##),
        filter_cmd!(
            "filter.blur.radialBlur",
            "Radial Blur…",
            ["Filter", "Blur"],
            r##"{"amount":1..100=10,"method":"spin|zoom","centerX":0..1=0.5,"centerY":0..1=0.5}"##
        ),
        filter_cmd!("filter.blur.surfaceBlur", "Surface Blur…", ["Filter", "Blur"], r##"{"radius":1..100=5,"threshold":2..255=15}"##),
        filter_cmd!(
            "filter.sharpen.unsharpMask",
            "Unsharp Mask…",
            ["Filter", "Sharpen"],
            r##"{"amount":1..500=50,"radius":0.1..1000=1,"threshold":0..255=0}"##
        ),
        filter_cmd!(
            "filter.sharpen.smartSharpen",
            "Smart Sharpen…",
            ["Filter", "Sharpen"],
            r##"{"amount":1..500=100,"radius":0.1..64=1,"reduceNoise":0..100=10}"##
        ),
        filter_cmd!(
            "filter.noise.addNoise",
            "Add Noise…",
            ["Filter", "Noise"],
            r##"{"amount":0.1..400=12.5,"distribution":"uniform|gaussian","monochromatic":bool,"seed":u32=0}"##
        ),
        filter_cmd!("filter.noise.median", "Median…", ["Filter", "Noise"], r##"{"radius":1..500=1}"##),
        filter_cmd!("filter.noise.dustAndScratches", "Dust & Scratches…", ["Filter", "Noise"], r##"{"radius":1..500=1,"threshold":0..255=0}"##),
        filter_cmd!("filter.pixelate.mosaic", "Mosaic…", ["Filter", "Pixelate"], r##"{"cellSize":2..200=10}"##),
        filter_cmd!("filter.stylize.emboss", "Emboss…", ["Filter", "Stylize"], r##"{"angle":-180..180=135,"height":1..100=3,"amount":1..500=100}"##),
        filter_cmd!("filter.stylize.findEdges", "Find Edges", ["Filter", "Stylize"], "{}"),
        filter_cmd!("filter.stylize.solarize", "Solarize", ["Filter", "Stylize"], "{}"),
        filter_cmd!("filter.distort.twirl", "Twirl…", ["Filter", "Distort"], r##"{"angle":-999..999=50}"##),
        filter_cmd!("filter.distort.pinch", "Pinch…", ["Filter", "Distort"], r##"{"amount":-100..100=50}"##),
        filter_cmd!("filter.distort.spherize", "Spherize…", ["Filter", "Distort"], r##"{"amount":-100..100=100,"mode":"normal|horizontalOnly|verticalOnly"}"##),
        filter_cmd!(
            "filter.distort.wave",
            "Wave…",
            ["Filter", "Distort"],
            r##"{"generators":1..999=5,"wavelengthMin":1..998=10,"wavelengthMax":2..999=120,"amplitudeMin":1..998=5,"amplitudeMax":1..999=35,"type":"sine|triangle|square","undefinedAreas":"wrap|repeat","seed":u32=0}"##
        ),
        filter_cmd!("filter.distort.ripple", "Ripple…", ["Filter", "Distort"], r##"{"amount":-999..999=100,"size":"small|medium|large"}"##),
        filter_cmd!("filter.distort.polarCoordinates", "Polar Coordinates…", ["Filter", "Distort"], r##"{"mode":"rectangularToPolar|polarToRectangular"}"##),
        filter_cmd!("filter.other.highPass", "High Pass…", ["Filter", "Other"], r##"{"radius":0.1..1000=10}"##),
        filter_cmd!("filter.other.minimum", "Minimum…", ["Filter", "Other"], r##"{"radius":0.2..500=1,"preserve":"squareness|roundness"}"##),
        filter_cmd!("filter.other.maximum", "Maximum…", ["Filter", "Other"], r##"{"radius":0.2..500=1,"preserve":"squareness|roundness"}"##),
        filter_cmd!(
            "filter.other.offset",
            "Offset…",
            ["Filter", "Other"],
            r##"{"horizontal":px=0,"vertical":px=0,"undefinedAreas":"wrap|repeat|transparent"}"##
        ),
        CommandSpec {
            id: "filter.lastFilter",
            label: "Last Filter",
            menu: &["Filter"],
            shortcut: Some("Cmd+Alt+F"),
            params: "{}",
            enabled: |s| {
                has_filterable_layer(s)?;
                last_filter(s).map(|_| ()).ok_or_else(|| "no filter applied yet".into())
            },
            run: |s, _| {
                let (id, params) = last_filter(s).ok_or_else(|| EngineError::Other("no filter applied yet".into()))?;
                run_filter(s, &id, &params)
            },
            journal: true,
        },
    ]
}

/// The most recent filter command in the session journal.
fn last_filter(s: &Session) -> Option<(String, Value)> {
    s.journal.iter().rev().find(|(id, _)| params_for(id, &Value::Null).is_some() && id.starts_with("filter.")).cloned()
}

#[cfg(test)]
mod tests_displace;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsigned_seeds_preserve_distinct_deterministic_wave_and_noise() {
        for id in ["filter.distort.wave", "filter.noise.addNoise"] {
            let render = |seed| {
                let mut s = session();
                paint_pattern(&mut s);
                s.execute(id, json!({"seed": seed})).unwrap();
                let d = s.active().unwrap();
                d.doc.layer(d.active_layer.unwrap()).unwrap().surface().unwrap().read_region(d.doc.bounds())
            };
            let high = render(4_000_000_000u32);
            assert_ne!(high, render(2_147_483_647), "{id}: distinct seeds");
            assert_eq!(high, render(4_000_000_000), "{id}: deterministic seed");
            for seed in [0, u32::MAX] {
                let fp = params_for(id, &json!({"seed": seed})).unwrap();
                match fp {
                    FilterParams::Wave { seed: actual, .. } | FilterParams::AddNoise { seed: actual, .. } => assert_eq!(actual, seed),
                    _ => panic!("seeded filter"),
                }
            }
        }
    }

    #[test]
    fn strict_extended_filters_keep_defaults_targets_history_and_last_filter() {
        for depth in [8, 16, 32] {
            for id in ["filter.blur.smartBlur", "filter.blur.shapeBlur", "filter.noise.reduceNoise"] {
                let mut s = Session::new();
                s.execute("file.new", json!({"width":16,"height":16,"depth":depth})).unwrap();
                s.execute("layer.new.layer", json!({})).unwrap();
                let layer = s.active().unwrap().active_layer.unwrap();
                for params in [Value::Null, json!({}), json!({"layer":layer.0,"target":"pixels","coalesce":"extended"})] {
                    let before = s.active().unwrap().doc.clone();
                    let history = s.active().unwrap().history.past_len();
                    s.execute(id, params).unwrap();
                    assert_eq!(s.active().unwrap().history.past_len(), history + 1);
                    let after = s.active().unwrap().doc.clone();
                    s.undo();
                    assert_eq!(*s.active().unwrap().doc, *before);
                    s.redo();
                    assert_eq!(*s.active().unwrap().doc, *after);
                }
                s.execute("filter.lastFilter", json!({})).unwrap();
                // The session injects this target when editing an alpha channel.
                s.execute("channel.new", json!({"fill":"white"})).unwrap();
                s.execute(id, json!({})).unwrap();
            }
        }
    }
    #[test]
    fn blur_more_uses_the_measured_three_by_three_kernel() {
        let mut s = session();
        s.edit("impulse", |doc, active| {
            let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
            surf.fill_rect(photocraft_geom::Rect::new(0, 0, 48, 32), &[0.0, 0.0, 0.0, 1.0]);
            surf.write_pixel(20, 16, &[1.0; 4]);
            Ok(())
        })
        .unwrap();
        s.execute("filter.blur.blurMore", json!({})).unwrap();
        let st = s.active().unwrap();
        let surf = st.doc.layer(st.active_layer.unwrap()).unwrap().surface().unwrap();
        for (x, y, want) in [(20, 16, 2.0 / 14.0), (19, 15, 1.0 / 14.0), (20, 15, 2.0 / 14.0), (18, 16, 0.0)] {
            assert!((surf.sample_channel(x, y, 0) - want).abs() <= 1.0 / 255.0);
        }
    }

    #[test]
    fn stored_extended_filter_records_still_decode_tolerantly() {
        use photocraft_algo::{BlurQuality, BlurShape, FilterParams, SmartBlurMode};
        let smart =
            crate::filters::params_for("filter.blur.smartBlur", &json!({"radius":-2,"threshold":900,"quality":"old","mode":false,"extra":true})).unwrap();
        assert!(matches!(smart, FilterParams::SmartBlur { radius: 0.1, threshold: 100.0, quality: BlurQuality::High, mode: SmartBlurMode::Normal }));
        let shape = crate::filters::params_for("filter.blur.shapeBlur", &json!({"radius":2000,"shape":"old"})).unwrap();
        assert!(matches!(shape, FilterParams::ShapeBlur { radius: 1000.0, shape: BlurShape::Circle }));
        let noise = crate::filters::params_for("filter.noise.reduceNoise", &json!({"strength":-1,"preserveDetails":101,"removeJpegArtifact":"yes"})).unwrap();
        assert!(matches!(noise, FilterParams::ReduceNoise { strength: 0.0, preserve_details: 100.0, remove_jpeg_artifact: false, .. }));
    }
    #[test]
    fn bounded_extended_filters_reject_invalid_fields_without_editing() {
        let numbers = [
            ("filter.blur.smartBlur", "radius", 0.1, 100.0),
            ("filter.blur.smartBlur", "threshold", 0.1, 100.0),
            ("filter.blur.shapeBlur", "radius", 5.0, 1000.0),
            ("filter.noise.reduceNoise", "strength", 0.0, 10.0),
            ("filter.noise.reduceNoise", "preserveDetails", 0.0, 100.0),
            ("filter.noise.reduceNoise", "reduceColorNoise", 0.0, 100.0),
            ("filter.noise.reduceNoise", "sharpenDetails", 0.0, 100.0),
        ];
        let mut s = session();
        let before = s.active().unwrap().doc.clone();
        let journal = s.journal.clone();
        for (id, key, lo, hi) in numbers {
            for value in [json!("3"), json!(true), Value::Null, json!([]), json!({}), json!(lo - 0.01), json!(hi + 0.01), json!(1e300)] {
                let err = s.execute(id, json!({key:value})).unwrap_err();
                assert!(matches!(err, EngineError::BadParams { .. }), "{id}.{key}: {err}");
                // The message names the field and its range, so a caller can correct the call.
                let msg = err.to_string();
                assert!(msg.contains(key) && msg.contains(&format!("{lo}..{hi}")), "{msg}");
                assert_eq!(*s.active().unwrap().doc, *before);
                assert_eq!(s.journal, journal);
            }
            for v in [lo, hi] {
                validate_params(id, &json!({key:v})).unwrap();
            }
        }
        for id in ["filter.blur.smartBlur", "filter.blur.shapeBlur", "filter.noise.reduceNoise"] {
            for p in [json!({"radus":3}), json!({"layer":"2"}), json!({"target":"typo"}), json!({"coalesce":false})] {
                assert!(matches!(s.start(id, p.clone()), Err(EngineError::BadParams { .. })));
                assert!(matches!(run_filter(&mut s, id, &p), Err(EngineError::BadParams { .. })));
            }
        }
        for (id, key, allowed) in [
            ("filter.blur.smartBlur", "quality", "high|medium|low"),
            ("filter.blur.smartBlur", "mode", "normal|edgeOnly|overlayEdge"),
            ("filter.blur.shapeBlur", "shape", "circle|ring|square|diamond|triangle|hexagon|star|heart|cross"),
        ] {
            for v in [json!("typo"), json!(false), json!(1), Value::Null] {
                assert!(validate_params(id, &json!({key:v})).is_err());
            }
            for v in allowed.split('|') {
                validate_params(id, &json!({key:v})).unwrap();
            }
        }
        for v in [json!("true"), json!(1), Value::Null, json!([])] {
            assert!(validate_params("filter.noise.reduceNoise", &json!({"removeJpegArtifact":v})).is_err());
        }
        assert_eq!(*s.active().unwrap().doc, *before);
        assert_eq!(s.journal, journal);
        assert!(s.jobs().is_empty());
    }

    #[test]
    fn invalid_extended_filters_preserve_floating_selection() {
        let mut s = session();
        paint_pattern(&mut s);
        s.execute("select.rect", json!({"x":0,"y":0,"width":4,"height":4})).unwrap();
        s.execute("select.float", json!({"dx":8})).unwrap();
        let before = s.active().unwrap().doc.clone();
        for id in ["filter.blur.smartBlur", "filter.blur.shapeBlur", "filter.noise.reduceNoise"] {
            assert!(s.execute(id, json!({"radus":3})).is_err());
            assert!(s.start(id, json!({"radus":3})).is_err());
            assert!(crate::float_cmds::floating(s.active().unwrap()).is_some());
            assert_eq!(*s.active().unwrap().doc, *before);
        }
    }

    fn session() -> Session {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 48, "height": 32})).unwrap();
        s.execute("layer.new.layer", json!({})).unwrap();
        s.execute("edit.fill", json!({"color": "#3366cc"})).ok();
        s
    }

    fn active_pixels(s: &Session) -> Vec<f32> {
        let d = s.active().unwrap();
        let l = d.doc.layer(d.active_layer.unwrap()).unwrap();
        l.surface().unwrap().read_region(photocraft_geom::Rect::new(0, 0, 48, 32))
    }

    fn paint_pattern(s: &mut Session) {
        s.edit("pattern", |doc, active| {
            let l = doc.layer_mut(active.unwrap()).unwrap();
            let surf = l.surface_mut().unwrap();
            for y in 0..32 {
                for x in 0..48 {
                    let v = ((x * 5 + y * 3) % 17) as f32 / 16.0;
                    surf.write_pixel(x, y, &[v, 1.0 - v, (x % 3) as f32 / 2.0, 1.0]);
                }
            }
            Ok(())
        })
        .unwrap();
    }

    const ALL: [&str; 24] = [
        "filter.blur.gaussianBlur",
        "filter.blur.boxBlur",
        "filter.blur.motionBlur",
        "filter.blur.radialBlur",
        "filter.blur.surfaceBlur",
        "filter.sharpen.unsharpMask",
        "filter.sharpen.smartSharpen",
        "filter.noise.addNoise",
        "filter.noise.median",
        "filter.noise.dustAndScratches",
        "filter.pixelate.mosaic",
        "filter.stylize.emboss",
        "filter.stylize.findEdges",
        "filter.stylize.solarize",
        "filter.distort.twirl",
        "filter.distort.pinch",
        "filter.distort.spherize",
        "filter.distort.wave",
        "filter.distort.ripple",
        "filter.distort.polarCoordinates",
        "filter.other.highPass",
        "filter.other.minimum",
        "filter.other.maximum",
        "filter.other.offset",
    ];

    #[test]
    fn every_filter_command_runs_changes_pixels_and_undoes() {
        for id in ALL {
            let mut s = session();
            paint_pattern(&mut s);
            let before = active_pixels(&s);
            let params = match id {
                "filter.other.offset" => json!({"horizontal": 5, "vertical": 2}),
                "filter.noise.dustAndScratches" => json!({"radius": 2, "threshold": 0}),
                "filter.noise.median" => json!({"radius": 2}),
                _ => json!({}),
            };
            let r = s.execute(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(r.get("filter").is_some(), "{id}");
            assert_ne!(active_pixels(&s), before, "{id} changed nothing");
            s.execute("edit.undo", json!({})).unwrap();
            assert_eq!(active_pixels(&s), before, "{id} undo");
        }
    }

    #[test]
    fn preset_filters_record_their_own_name() {
        // #528: one-step presets run a generic algorithm but name the step after themselves;
        // the generic commands keep the algorithm's name.
        let specs = specs();
        for (id, name) in [
            ("filter.blur.blur", "Blur"),
            ("filter.blur.blurMore", "Blur More"),
            ("filter.sharpen.sharpen", "Sharpen"),
            ("filter.sharpen.sharpenMore", "Sharpen More"),
            ("filter.sharpen.sharpenEdges", "Sharpen Edges"),
            ("filter.noise.despeckle", "Despeckle"),
            ("filter.blur.gaussianBlur", "Gaussian Blur"),
            ("filter.sharpen.unsharpMask", "Unsharp Mask"),
            ("filter.blur.surfaceBlur", "Surface Blur"),
        ] {
            let mut s = session();
            let steps = s.active().unwrap().history.past_len();
            s.execute(id, json!({})).unwrap();
            let h = &s.active().unwrap().history;
            assert_eq!((h.past_len(), h.undo_label()), (steps + 1, Some(name)), "{id}");
            let label = specs.iter().find(|c| c.id == id).unwrap().label;
            assert_eq!(label.trim_end_matches('…'), name, "{id}: the step is named like the command");
        }
    }

    #[test]
    fn specs_are_complete_and_documented() {
        let specs = specs();
        for id in ALL {
            let spec = specs.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("missing {id}"));
            assert!(spec.params.starts_with('{'), "{id}");
            assert!(!spec.menu.is_empty(), "{id}");
        }
    }

    #[test]
    fn disabled_without_pixel_layer() {
        let mut s = Session::new();
        assert!(!s.is_enabled("filter.blur.gaussianBlur"));
        s.execute("file.new", json!({"width": 8, "height": 8})).unwrap();
        s.execute("layer.newAdjustmentLayer.invert", json!({})).unwrap();
        assert!(matches!(s.execute("filter.blur.gaussianBlur", json!({})), Err(EngineError::Disabled(..))));
    }

    #[test]
    fn selection_limits_the_filter() {
        let mut s = session();
        paint_pattern(&mut s);
        s.execute("select.rect", json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap();
        let has_sel = s.active().unwrap().doc.selection.is_some();
        assert!(has_sel);
        let before = active_pixels(&s);
        s.execute("filter.blur.gaussianBlur", json!({"radius": 3})).unwrap();
        let after = active_pixels(&s);
        if has_sel {
            // A pixel far from the selection is unchanged.
            let i = (30 * 48 + 40) * 4;
            assert_eq!(after[i..i + 4], before[i..i + 4]);
            let j = (5 * 48 + 5) * 4;
            assert_ne!(after[j..j + 4], before[j..j + 4]);
        }
    }

    #[test]
    fn minimum_signed_offset_matches_its_wrapped_distance() {
        for depth in [8, 16, 32] {
            let make_session = || {
                let mut s = Session::new();
                s.execute("file.new", json!({"width": 48, "height": 32, "depth": depth})).unwrap();
                s.execute("layer.new.layer", json!({})).unwrap();
                paint_pattern(&mut s);
                s
            };
            let mut actual = make_session();
            let before = active_pixels(&actual);
            actual.execute("filter.other.offset", json!({"horizontal": i32::MIN, "vertical": i32::MIN})).unwrap();
            let mut expected = make_session();
            expected.execute("filter.other.offset", json!({"horizontal": i32::MIN.rem_euclid(48), "vertical": i32::MIN.rem_euclid(32)})).unwrap();
            let after = active_pixels(&actual);
            assert_eq!(after, active_pixels(&expected), "depth {depth}");
            assert_ne!(after, before);
            actual.execute("edit.undo", json!({})).unwrap();
            assert_eq!(active_pixels(&actual), before);
            actual.execute("edit.redo", json!({})).unwrap();
            assert_eq!(active_pixels(&actual), after);
        }
    }

    #[test]
    fn huge_offsets_clamp_instead_of_wrapping() {
        // 3e9 wrapped to a negative offset through `as i32`; it saturates now, so it
        // equals the maximal in-range offset instead of shifting the other way.
        let mut a = session();
        a.execute("filter.other.offset", json!({"horizontal": 3_000_000_000_i64, "vertical": 0})).unwrap();
        let mut b = session();
        b.execute("filter.other.offset", json!({"horizontal": 2_147_483_647_i64, "vertical": 0})).unwrap();
        assert_eq!(active_pixels(&a), active_pixels(&b));
    }

    #[test]
    fn last_filter_repeats_with_same_params() {
        let mut s = session();
        paint_pattern(&mut s);
        assert!(!s.is_enabled("filter.lastFilter"));
        s.execute("filter.other.offset", json!({"horizontal": 3})).unwrap();
        let once = active_pixels(&s);
        s.execute("filter.lastFilter", json!({})).unwrap();
        let twice = active_pixels(&s);
        assert_ne!(once, twice);
        // Offsetting 3 twice equals offsetting 6 once.
        let mut s2 = session();
        paint_pattern(&mut s2);
        s2.execute("filter.other.offset", json!({"horizontal": 6})).unwrap();
        assert_eq!(active_pixels(&s2), twice);
    }

    #[test]
    fn params_map_to_algorithm_units() {
        assert_eq!(params_for("filter.blur.gaussianBlur", &json!({"radius": 4.5})), Some(FilterParams::GaussianBlur { radius: 4.5 }));
        assert_eq!(
            params_for("filter.noise.addNoise", &json!({"amount": 10, "distribution": "gaussian", "monochromatic": true})),
            Some(FilterParams::AddNoise { amount: 10.0, distribution: Distribution::Gaussian, monochromatic: true, seed: 0 })
        );
        assert_eq!(params_for("filter.blur.gaussianBlur", &json!({"radius": -3})), Some(FilterParams::GaussianBlur { radius: 0.1 }));
        assert!(params_for("filter.nope", &json!({})).is_none());
    }

    #[test]
    fn minimum_maximum_preserve_roundness() {
        for depth in [8, 16, 32] {
            let mut s = Session::new();
            s.execute("file.new", json!({"width": 48, "height": 32, "depth": depth})).unwrap();
            s.execute("layer.new.layer", json!({})).unwrap();
            // A white dot on black.
            s.edit("dot", |doc, active| {
                let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
                surf.fill_rect(photocraft_geom::Rect::new(0, 0, 48, 32), &[0.0, 0.0, 0.0, 1.0]);
                surf.write_pixel(24, 16, &[1.0, 1.0, 1.0, 1.0]);
                Ok(())
            })
            .unwrap();
            let before = active_pixels(&s);
            let lit = |s: &Session, x: usize, y: usize| active_pixels(s)[(y * 48 + x) * 4] > 0.5;
            s.execute("filter.other.maximum", json!({"radius": 4, "preserve": "roundness"})).unwrap();
            // Grown into a disc of radius 4: (4, 0) and (2, 3) are in, the square's corner isn't.
            assert!(lit(&s, 28, 16) && lit(&s, 26, 19), "depth {depth}");
            assert!(!lit(&s, 28, 20) && !lit(&s, 27, 19), "depth {depth}");
            s.execute("edit.undo", json!({})).unwrap();
            assert_eq!(active_pixels(&s), before);
            // The default is Squareness: the corner is taken.
            s.execute("filter.other.maximum", json!({"radius": 4})).unwrap();
            assert!(lit(&s, 28, 20), "depth {depth}");
            // Minimum with roundness: only the centre of that 9×9 square holds a whole disc.
            s.execute("filter.other.minimum", json!({"radius": 4, "preserve": "roundness"})).unwrap();
            assert!(!lit(&s, 20, 12) && lit(&s, 24, 16), "depth {depth}");
        }
        assert_eq!(
            params_for("filter.other.minimum", &json!({"radius": 2, "preserve": "roundness"})),
            Some(FilterParams::Minimum { radius: 2.0, preserve: Preserve::Roundness })
        );
        assert_eq!(params_for("filter.other.maximum", &json!({})), Some(FilterParams::Maximum { radius: 1.0, preserve: Preserve::Squareness }));
    }

    #[test]
    fn smart_object_records_smart_filter() {
        use photocraft_doc::{Layer, SmartObject, SmartSource};
        let mut s = session();
        s.edit("smart", |doc, active| {
            let mut cache = photocraft_raster::Surface::new(doc.pixel_format());
            cache.fill_rect(photocraft_geom::Rect::new(0, 0, 10, 10), &[1.0, 0.0, 0.0, 1.0]);
            let l = Layer::new(
                "so",
                LayerContent::Smart(SmartObject {
                    source: SmartSource::Linked { path: String::new() },
                    transform: photocraft_geom::Affine::IDENTITY,
                    smart_filters: vec![],
                    cache: Some(cache),
                    psd_raw: None,
                    filters_enabled: true,
                    filter_mask: None,
                    warp: None,
                    stack_mode: None,
                    perspective: None,
                    develop: None,
                }),
            );
            *active = Some(doc.insert_above(*active, l));
            Ok(())
        })
        .unwrap();
        s.execute("filter.blur.gaussianBlur", json!({"radius": 2})).unwrap();
        let d = s.active().unwrap();
        let LayerContent::Smart(sm) = &d.doc.layer(d.active_layer.unwrap()).unwrap().content else { panic!() };
        assert_eq!(sm.smart_filters.len(), 1);
        assert_eq!(sm.smart_filters[0].command, "filter.blur.gaussianBlur");
        assert_eq!(sm.smart_filters[0].params["radius"], json!(2));
        assert!(sm.cache.as_ref().unwrap().pixel(10, 5)[3] > 0.0, "cache was blurred");
    }
}
