//! One Color/Swatches panel for both personas. User and document palettes share
//! SwatchBook; live edits use the existing captured document transactions.
use crate::{PhotocraftApp, fill_stroke_proxy::ActivePaint};
use egui::{Color32, RichText};
use photocraft_color::{
    Color, ColorMode,
    ase::{self, Palette},
};
use photocraft_doc::{
    Fill,
    vector_design::swatches::{self, PaintSlot, PaintTarget, Swatch, SwatchBook, SwatchGroup, SwatchId},
};
use photocraft_engine::{
    Session,
    toolsets::Persona,
    vector_packages::colours::{Action, Capture, Request},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Scope {
    #[default]
    User,
    Document,
}
#[derive(Clone, Default)]
struct State {
    scope: Scope,
    search: String,
    list: bool,
    secondary: bool,
    selected: Option<SwatchId>,
    group: Option<u64>,
    editor: Option<Editor>,
    recent: Vec<Color>,
    error: Option<String>,
    document: Option<u64>,
    reveal: Option<Option<u64>>,
    last_drop_frame: Option<u64>,
}
#[derive(Clone)]
struct Editor {
    hex: String,
    scope: Scope,
    capture: Option<Capture>,
    swatch: Option<Swatch>,
    group: Option<SwatchGroup>,
}
#[derive(Clone, Debug)]
struct Drag {
    scope: Scope,
    id: SwatchId,
    capture: Option<Capture>,
}
fn valid_drag(drag: &Drag, app: &PhotocraftApp, scope: Scope) -> bool {
    drag.scope == scope && drag.capture.is_none_or(|capture| capture.check(&app.session).is_ok())
}
fn state_id() -> egui::Id {
    egui::Id::new("shared-color-swatches")
}
fn load(app: &PhotocraftApp, ui: &egui::Ui) -> State {
    ui.data(|d| d.get_temp(state_id())).unwrap_or_else(|| State {
        list: app.session.prefs().user_swatches.list_view,
        recent: app.session.prefs().user_swatches.recent.iter().copied().filter(|c| swatches::validate_color(*c).is_ok()).take(16).collect(),
        ..Default::default()
    })
}
fn save(app: &mut PhotocraftApp, ui: &egui::Ui, s: State) {
    if app.session.prefs().user_swatches.recent != s.recent || app.session.prefs().user_swatches.list_view != s.list {
        app.session.edit_prefs(|p| {
            p.user_swatches.recent = s.recent.clone();
            p.user_swatches.list_view = s.list;
        });
    }
    ui.data_mut(|d| d.insert_temp(state_id(), s));
}
fn is_vector(app: &PhotocraftApp) -> bool {
    app.session.prefs().toolbar.persona == Persona::Vector
}
fn rgba(c: Color) -> [f32; 4] {
    let rgb = c.to_rgb();
    [rgb[0], rgb[1], rgb[2], c.alpha]
}
fn tool_color(app: &PhotocraftApp, secondary: bool) -> Color {
    let c = if secondary { app.session.tools.background } else { app.session.tools.foreground };
    Color::rgba(c[0], c[1], c[2], c[3])
}
fn current(app: &PhotocraftApp, secondary: bool) -> Color {
    if is_vector(app)
        && let Some(target) = targets(app, secondary).first()
        && let Some(st) = app.session.active()
        && let Ok(color) = photocraft_engine::vector_packages::colours::color(&st.doc, target)
        && let Ok(color) = display_color(&app.session, color, Scope::Document)
    {
        return color;
    }
    tool_color(app, secondary)
}
fn remember(s: &mut State, c: Color) {
    s.recent.retain(|v| *v != c);
    s.recent.insert(0, c);
    s.recent.truncate(16);
}
fn book(app: &PhotocraftApp, scope: Scope) -> SwatchBook {
    match scope {
        Scope::User => app.session.prefs().user_swatches.palette.clone(),
        Scope::Document => app.session.active().map(|st| st.doc.swatches.clone()).unwrap_or_default(),
    }
}
fn command(app: &mut PhotocraftApp, action: Action) -> Result<(), String> {
    let capture = Capture::begin(&app.session).map_err(|e| e.to_string())?;
    app.session.execute("swatch.document.edit", serde_json::json!(Request { capture, action })).map(|_| ()).map_err(|e| e.to_string())
}
fn apply(app: &mut PhotocraftApp, scope: Scope, action: Action) -> Result<(), String> {
    if scope == Scope::Document {
        return command(app, action);
    }
    let mut b = book(app, scope);
    match action {
        Action::Add { swatch } => b.swatches.push(swatch),
        Action::Edit { swatch } => {
            let id = swatch.id;
            *b.swatches.iter_mut().find(|w| w.id == id).ok_or("missing swatch")? = swatch;
        }
        Action::Delete { ids } => b.swatches.retain(|w| !ids.contains(&w.id)),
        Action::Group { group, ids } => {
            for w in &mut b.swatches {
                if ids.contains(&w.id) {
                    w.group = Some(group.id);
                }
            }
            b.groups.push(group);
        }
        Action::RenameGroup { id, name } => b.groups.iter_mut().find(|g| g.id == id).ok_or("missing group")?.name = name,
        Action::DeleteGroup { id } => {
            b.swatches.retain(|w| w.group != Some(id));
            b.groups.retain(|g| g.id != id);
        }
        Action::Move { id, group, before } => swatches::move_swatch(&mut b, id, group, before)?,
        Action::Import { palette, .. } => {
            let mut remap = std::collections::BTreeMap::new();
            for g in palette.groups {
                let id = b.next_id()?;
                remap.insert(g.id, id);
                b.groups.push(SwatchGroup { id, name: b.free_name(&g.name) });
            }
            for mut w in palette.swatches {
                w.id = SwatchId(b.next_id()?);
                w.name = b.free_name(&w.name);
                w.group = w.group.and_then(|g| remap.get(&g).copied());
                b.swatches.push(w);
            }
        }
        _ => return Err("This action requires a document palette".into()),
    }
    b.validate()?;
    app.session.edit_prefs(|p| p.user_swatches.palette = b);
    Ok(())
}
fn targets(app: &PhotocraftApp, secondary: bool) -> Vec<PaintTarget> {
    use photocraft_doc::{LayerContent, vector_design::appearance::AppearanceItem};
    let Some(st) = app.session.active() else { return vec![] };
    st.selected_layers()
        .into_iter()
        .filter_map(|id| {
            let l = st.doc.layer(id)?;
            if matches!(l.content, photocraft_doc::LayerContent::Text(_)) {
                return (!secondary).then_some(PaintTarget { symbol: None, layer: id, paint: PaintSlot::Fill, stop: None });
            }
            let slot = l
                .vector
                .appearance
                .as_ref()
                .and_then(|a| {
                    let matches = |it: &AppearanceItem| {
                        matches!(it,AppearanceItem::Fill(f) if !secondary && f.visible) || matches!(it,AppearanceItem::Stroke(st) if secondary && st.visible)
                    };
                    if app.shared_tools.paint.options.selected_stop.is_some() && a.items.get(app.shared_tools.paint.options.item).is_some_and(matches) {
                        return Some(PaintSlot::Appearance(app.shared_tools.paint.options.item));
                    }
                    a.items.iter().enumerate().rev().find_map(|(i, it)| match it {
                        AppearanceItem::Fill(f) if !secondary && f.visible => Some(PaintSlot::Appearance(i)),
                        AppearanceItem::Stroke(s) if secondary && s.visible => Some(PaintSlot::Appearance(i)),
                        _ => None,
                    })
                })
                .or_else(|| match &l.content {
                    LayerContent::Shape(s) if !secondary && s.fill.is_some() => Some(PaintSlot::Fill),
                    LayerContent::Shape(s) if secondary && s.stroke.is_some() => Some(PaintSlot::Stroke),
                    LayerContent::Fill(_) | LayerContent::Text(_) if !secondary => Some(PaintSlot::Fill),
                    _ => None,
                })?;
            let stop =
                app.shared_tools.paint.options.selected_stop.filter(|_| matches!(slot,PaintSlot::Appearance(i) if i==app.shared_tools.paint.options.item));
            Some(PaintTarget { symbol: None, layer: id, paint: slot, stop })
        })
        .collect()
}
fn select(app: &mut PhotocraftApp, s: &mut State, w: &Swatch, secondary: bool) -> Result<(), String> {
    s.selected = Some(w.id);
    s.group = w.group;
    s.secondary = secondary;
    let ts = targets(app, secondary);
    if is_vector(app) && s.scope == Scope::Document && !ts.is_empty() {
        command(app, if w.global { Action::Assign { targets: ts, swatch: w.id, tint: 100. } } else { Action::AssignPaint { targets: ts, swatch: w.id } })?;
    } else if is_vector(app) {
        app.ui.fill_stroke_proxy.active = if secondary { ActivePaint::Stroke } else { ActivePaint::Fill };
        if let Fill::Solid(c) = w.paint {
            let native = native_color(&app.session, c, Scope::Document)?;
            set_current_paint(app, secondary, native)?;
            if s.scope == Scope::Document {
                let ts = targets(app, secondary);
                if !ts.is_empty() {
                    command(
                        app,
                        if w.global { Action::Assign { targets: ts, swatch: w.id, tint: 100. } } else { Action::AssignPaint { targets: ts, swatch: w.id } },
                    )?;
                }
            }
        }
    }

    if let Fill::Solid(c) = w.paint {
        let c = display_color(&app.session, c, s.scope)?;
        if secondary {
            app.session.tools.background = rgba(c);
        } else {
            app.session.tools.foreground = rgba(c);
            if !is_vector(app) {
                crate::type_tool::foreground_changed(app);
            }
        }
        remember(s, c);
    }
    Ok(())
}
/// Document paints keep their source model. The picker/tool chips are sRGB.
fn palette_mode(mode: ColorMode) -> ColorMode {
    match mode {
        ColorMode::Cmyk | ColorMode::Lab => mode,
        ColorMode::Bitmap | ColorMode::Grayscale | ColorMode::Duotone => ColorMode::Grayscale,
        _ => ColorMode::Rgb,
    }
}
fn color_source(s: &Session, c: Color, scope: Scope) -> std::sync::Arc<photocraft_cms::Profile> {
    if scope == Scope::Document
        && let Some(st) = s.active().filter(|st| palette_mode(st.doc.mode) == palette_mode(c.mode))
    {
        return photocraft_engine::color_cmds::document_profile(&st.doc);
    }
    match c.mode {
        ColorMode::Rgb => std::sync::Arc::new(photocraft_cms::Builtin::Srgb.profile().clone()),
        ColorMode::Lab => std::sync::Arc::new(photocraft_cms::Builtin::LabD50.profile().clone()),
        m => s.color.working(m),
    }
}
fn display_color(s: &Session, c: Color, scope: Scope) -> Result<Color, String> {
    let source = color_source(s, c, scope);
    if c.mode == ColorMode::Rgb && source.content_hash() == photocraft_cms::Builtin::Srgb.profile().content_hash() {
        return Ok(c);
    }
    let transform = photocraft_cms::Transform::new(&source, photocraft_cms::Builtin::Srgb.profile(), s.color.settings.intent(), s.color.settings.bpc)
        .map_err(|e| e.to_string())?;
    let mut values = c.c;
    transform.apply(&mut values, 4);
    Ok(Color::rgba(values[0].clamp(0., 1.), values[1].clamp(0., 1.), values[2].clamp(0., 1.), c.alpha))
}
fn native_color(s: &Session, c: Color, scope: Scope) -> Result<Color, String> {
    if scope == Scope::User {
        return Ok(c);
    }
    let Some(st) = s.active() else { return Ok(c) };
    let src = color_source(s, c, Scope::User);
    let dst = photocraft_engine::color_cmds::document_profile(&st.doc);
    let mode = palette_mode(st.doc.mode);
    if c.mode == mode && src.content_hash() == dst.content_hash() {
        return Ok(c);
    }
    let transform = photocraft_cms::Transform::new(&src, &dst, s.color.settings.intent(), s.color.settings.bpc).map_err(|e| e.to_string())?;
    let mut values = c.c;
    transform.apply(&mut values, 4);
    for v in &mut values {
        *v = v.clamp(0., 1.);
    }
    for v in &mut values[mode.color_channels()..] {
        *v = 0.;
    }
    Ok(Color { mode, c: values, alpha: c.alpha })
}
fn preview_color(app: &PhotocraftApp, ui: &egui::Ui, scope: Scope, c: Color) -> Color {
    let src = color_source(&app.session, c, scope);
    if c.mode == ColorMode::Rgb && src.content_hash() == photocraft_cms::Builtin::Srgb.profile().content_hash() {
        return c;
    }
    let key = egui::Id::new(("swatches-icc-preview", src.content_hash(), app.session.color.settings.intent(), app.session.color.settings.bpc));
    let cached: Option<std::sync::Arc<photocraft_cms::Transform>> = ui.data(|d| d.get_temp(key));
    let transform = if let Some(transform) = cached {
        transform
    } else {
        let Ok(transform) =
            photocraft_cms::Transform::new(&src, photocraft_cms::Builtin::Srgb.profile(), app.session.color.settings.intent(), app.session.color.settings.bpc)
        else {
            return c;
        };
        let transform = std::sync::Arc::new(transform);
        ui.data_mut(|d| d.insert_temp(key, transform.clone()));
        transform
    };
    let mut values = c.c;
    transform.apply(&mut values, 4);
    Color::rgba(values[0].clamp(0., 1.), values[1].clamp(0., 1.), values[2].clamp(0., 1.), c.alpha)
}
fn set_current_paint(app: &mut PhotocraftApp, secondary: bool, color: Color) -> Result<(), String> {
    let ts = targets(app, secondary);
    if !ts.is_empty() {
        return command(app, Action::SetColor { targets: ts, color });
    }
    let ids = app
        .session
        .active()
        .map(|st| {
            st.selected_layers()
                .into_iter()
                .filter(|id| st.doc.layer(*id).is_some_and(|l| matches!(l.content, photocraft_doc::LayerContent::Shape(_))))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if ids.is_empty() {
        return Ok(());
    }
    app.ui.fill_stroke_proxy.active = if secondary { ActivePaint::Stroke } else { ActivePaint::Fill };
    app.ui.fill_stroke_proxy.apply(&mut app.session, &ids, &crate::fill_stroke_proxy::Action::Colour(color)).map_err(|e| e.to_string())
}
fn targets_picker(app: &mut PhotocraftApp, ui: &mut egui::Ui, s: &mut State) {
    let vector = is_vector(app);
    let saved_tools = vector.then_some((app.session.tools.foreground, app.session.tools.background));
    if vector {
        for secondary in [false, true] {
            if let Some(target) = targets(app, secondary).first()
                && let Some(st) = app.session.active()
                && let Ok(color) = photocraft_engine::vector_packages::colours::color(&st.doc, target)
            {
                let color = rgba(preview_color(app, ui, Scope::Document, color));
                if secondary {
                    app.session.tools.background = color;
                } else {
                    app.session.tools.foreground = color;
                }
            }
        }
    }
    ui.horizontal_wrapped(|ui| {
        for (secondary, label) in [(false, if vector { "Fill" } else { "Foreground" }), (true, if vector { "Stroke" } else { "Background" })] {
            let c = current(app, secondary).to_rgba8();
            let r = ui.add_sized([112., 30.], egui::Button::new(format!("     {label}")).selected(s.secondary == secondary));
            r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
            let rect = egui::Rect::from_min_size(r.rect.left_top() + egui::vec2(5., 6.), egui::vec2(18., 18.));
            ui.painter().rect_filled(rect, 0., Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]));
            if r.clicked() {
                s.secondary = secondary;
                app.ui.fill_stroke_proxy.active = if secondary { ActivePaint::Stroke } else { ActivePaint::Fill };
            }
        }
    });
    let before = tool_color(app, s.secondary);
    if s.secondary {
        std::mem::swap(&mut app.session.tools.foreground, &mut app.session.tools.background);
    }
    // The established HSB controls are shared verbatim between personas.
    super::color_picker_edit(app, ui, !s.secondary && !vector);
    if s.secondary {
        std::mem::swap(&mut app.session.tools.foreground, &mut app.session.tools.background);
    }
    let after = tool_color(app, s.secondary);
    if let Some((foreground, background)) = saved_tools {
        app.session.tools.foreground = foreground;
        app.session.tools.background = background;
    }
    if after != before {
        if s.secondary {
            app.session.tools.background = rgba(after);
        } else {
            app.session.tools.foreground = rgba(after);
        }
        remember(s, after);
        if vector {
            match native_color(&app.session, after, Scope::Document).and_then(|color| set_current_paint(app, s.secondary, color)) {
                Ok(()) => {}
                Err(e) => s.error = Some(e),
            }
        }
    }
}
pub(crate) fn color_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let mut s = load(app, ui);
    targets_picker(app, ui, &mut s);
    save(app, ui, s);
}
fn seed(app: &mut PhotocraftApp) {
    if !app.session.prefs().user_swatches.initialized {
        let mut b = SwatchBook::default();
        b.groups.push(SwatchGroup { id: 1, name: "Starter Colours".into() });
        for (i, rgb) in super::SWATCHES.iter().enumerate() {
            b.swatches.push(Swatch {
                id: SwatchId(i as u64 + 2),
                name: format!("Starter {}", i + 1),
                paint: Fill::Solid(Color::rgb(f32::from(rgb[0]) / 255., f32::from(rgb[1]) / 255., f32::from(rgb[2]) / 255.)),
                global: false,
                spot: false,
                group: Some(1),
                stops: vec![],
            });
        }
        app.session.edit_prefs(|p| {
            if p.user_swatches.palette.is_empty() {
                p.user_swatches.palette = b;
            }
            p.user_swatches.initialized = true;
        });
    }
}
const LIBRARIES: &[(&str, Option<&[u8]>)] = &[
    ("Open Color", Some(include_bytes!("../../../assets/palettes/open-color.gpl"))),
    ("Tailwind CSS", Some(include_bytes!("../../../assets/palettes/tailwind.gpl"))),
    ("CSS Named Colours", Some(include_bytes!("../../../assets/palettes/color-name.gpl"))),
    ("Web-safe (216)", None),
    ("RGB", None),
    ("CMYK", None),
    ("Pastel", None),
    ("Light", None),
    ("Pure", None),
    ("Dark", None),
    ("Darker", None),
];
fn library(name: &str, data: Option<&[u8]>) -> Result<SwatchBook, String> {
    if let Some(data) = data {
        return swatches::from_palette(photocraft_color::gpl::read(data)?.palette);
    }
    let mut g = ase::Group { name: name.into(), swatches: vec![] };
    let mut add = |label: String, c: Color| g.swatches.push(ase::Swatch { name: label, color: c, global: false, spot: false });
    if name == "Web-safe (216)" {
        for r in 0..6 {
            for g in 0..6 {
                for b in 0..6 {
                    add(format!("#{:02X}{:02X}{:02X}", r * 51, g * 51, b * 51), Color::rgb(r as f32 / 5., g as f32 / 5., b as f32 / 5.));
                }
            }
        }
    } else if name == "CMYK" {
        for (label, c) in [("Cyan", [1., 0., 0., 0.]), ("Magenta", [0., 1., 0., 0.]), ("Yellow", [0., 0., 1., 0.]), ("Black", [0., 0., 0., 1.])] {
            add(label.into(), Color { mode: ColorMode::Cmyk, c, alpha: 1. });
        }
    } else {
        let (sat, value) = match name {
            "Pastel" => (0.3, 1.),
            "Light" => (0.5, 1.),
            "Dark" => (0.8, 0.55),
            "Darker" => (0.8, 0.3),
            _ => (1., 1.),
        };
        for i in 0..12 {
            let h = egui::ecolor::Hsva::new(i as f32 / 12., sat, value, 1.).to_srgb();
            add(format!("{name} {}", i + 1), Color::rgb(f32::from(h[0]) / 255., f32::from(h[1]) / 255., f32::from(h[2]) / 255.));
        }
    }
    swatches::from_palette(Palette { swatches: vec![], groups: vec![g] })
}
fn import(app: &mut PhotocraftApp, scope: Scope, name: &str, bytes: &[u8]) -> Result<Vec<String>, String> {
    if bytes.len() > ase::MAX_BYTES {
        return Err("Palette exceeds the 16 MiB limit".into());
    }
    let ext = std::path::Path::new(&name).extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    let mut result = match ext.as_str() {
        "ase" => ase::read(bytes)?,
        "aco" => photocraft_color::aco::read(bytes)?,
        "gpl" => photocraft_color::gpl::read(bytes)?,
        "kpl" => {
            let zip = photocraft_format::zip::ZipReader::new(bytes).map_err(|e| e.to_string())?;
            if zip.entries.len() > ase::MAX_ENTRIES || zip.entries.iter().filter(|e| e.name == "colorset.xml").count() != 1 {
                return Err("Invalid KPL archive entries".into());
            }
            let xml = zip.read_by_name("colorset.xml", ase::MAX_BYTES).map_err(|e| e.to_string())?;
            let mut profiles = std::collections::BTreeMap::new();
            let mut budget = ase::MAX_BYTES.saturating_sub(xml.len());
            if zip.find("profiles.xml").is_some() {
                let profile_xml = zip.read_by_name("profiles.xml", budget).map_err(|e| e.to_string())?;
                budget = budget.saturating_sub(profile_xml.len());
                for (name, file) in photocraft_color::kpl::read_profiles_xml(&profile_xml)? {
                    if zip.entries.iter().filter(|e| e.name == file).count() != 1 {
                        return Err("Invalid KPL profile member".into());
                    }
                    let bytes = zip.read_by_name(&file, budget).map_err(|e| e.to_string())?;
                    budget = budget.saturating_sub(bytes.len());
                    profiles.insert(name, photocraft_cms::Profile::parse(&bytes).map_err(|e| e.to_string())?);
                }
            }
            let mut transforms = std::collections::HashMap::new();
            photocraft_color::kpl::read_xml_with_profiles(&xml, |mut c, name| {
                let Some(profile) = profiles.get(name) else { return Ok(None) };
                let expected = match c.mode {
                    ColorMode::Rgb => photocraft_cms::ColorSpace::Rgb,
                    ColorMode::Cmyk => photocraft_cms::ColorSpace::Cmyk,
                    ColorMode::Lab => photocraft_cms::ColorSpace::Lab,
                    _ => photocraft_cms::ColorSpace::Gray,
                };
                if profile.color_space != expected {
                    return Err("KPL colour/profile model mismatch".into());
                }
                let hash = profile.content_hash();
                if let std::collections::hash_map::Entry::Vacant(entry) = transforms.entry(hash) {
                    entry.insert(
                        photocraft_cms::Transform::new(
                            profile,
                            photocraft_cms::Builtin::Srgb.profile(),
                            app.session.color.settings.intent(),
                            app.session.color.settings.bpc,
                        )
                        .map_err(|e| e.to_string())?,
                    );
                }
                let transform = transforms.get(&hash).ok_or("Missing KPL profile transform")?;
                if transform.inputs() > 4 || transform.outputs() > 4 {
                    return Err("Unsupported KPL profile channel count".into());
                }
                transform.apply(&mut c.c, 4);
                c.mode = ColorMode::Rgb;
                c.c[3] = 0.;
                Ok(Some(c))
            })?
        }
        _ => return Err("Choose an ASE, ACO, GPL or KPL swatch file".into()),
    };
    // Exchange RGB is sRGB; CMYK/Gray without profiles use configured working
    // spaces, Lab is D50. Convert into the document/working space through CMS.
    let destination = if scope == Scope::Document {
        app.session.active().map(|st| photocraft_engine::color_cmds::document_profile(&st.doc)).unwrap_or_else(|| app.session.color.working(ColorMode::Rgb))
    } else {
        std::sync::Arc::new(photocraft_cms::Builtin::Srgb.profile().clone())
    };
    let mode = if scope == Scope::Document { app.session.active().map_or(ColorMode::Rgb, |st| palette_mode(st.doc.mode)) } else { ColorMode::Rgb };
    let mut transforms = std::collections::HashMap::new();
    for w in result.palette.swatches.iter_mut().chain(result.palette.groups.iter_mut().flat_map(|g| &mut g.swatches)) {
        let source = match w.color.mode {
            ColorMode::Rgb => photocraft_cms::Builtin::Srgb.profile().clone(),
            ColorMode::Lab => photocraft_cms::Builtin::LabD50.profile().clone(),
            m => (*app.session.color.working(m)).clone(),
        };
        if let std::collections::hash_map::Entry::Vacant(entry) = transforms.entry(w.color.mode) {
            entry.insert(
                photocraft_cms::Transform::new(&source, &destination, app.session.color.settings.intent(), app.session.color.settings.bpc)
                    .map_err(|e| e.to_string())?,
            );
        }
        transforms.get(&w.color.mode).ok_or("Missing palette transform")?.apply(&mut w.color.c, 4);
        w.color.mode = mode;
        for c in &mut w.color.c[mode.color_channels()..] {
            *c = 0.;
        }
        for c in &mut w.color.c {
            *c = c.clamp(0., 1.);
        }
    }
    if result.palette.groups.is_empty() {
        result.palette.groups.push(ase::Group {
            name: std::path::Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or("Imported Palette").chars().take(100).collect(),
            swatches: std::mem::take(&mut result.palette.swatches),
        });
    }
    let b = swatches::from_palette(result.palette)?;
    apply(app, scope, Action::Import { palette: b, replace: false })?;
    Ok(result.warnings)
}
fn export(app: &PhotocraftApp, scope: Scope, ext: &str) -> Result<(Vec<u8>, Vec<String>), String> {
    let b = book(app, scope);
    let mut result = swatches::to_palette(&b)?;
    if matches!(ext, "aco" | "gpl") {
        result.warnings.push("This format flattens folders and does not store live or spot flags".into());
    }
    if ext == "kpl" && b.swatches.iter().any(|w| w.global && !w.spot) {
        result.warnings.push("KPL preserves spots, but has no ordinary live-colour flag".into());
    }
    if matches!(ext, "gpl" | "kpl") {
        result.warnings.push(if ext == "gpl" {
            "GPL stores 8-bit sRGB colours".into()
        } else {
            "KPL export stores sRGB colours; spot flags and folders are retained".into()
        });
        for w in result.palette.swatches.iter_mut().chain(result.palette.groups.iter_mut().flat_map(|g| &mut g.swatches)) {
            w.color = display_color(&app.session, w.color, scope)?;
        }
    }
    let bytes = match ext {
        "ase" => ase::write(&result.palette)?,
        "aco" => photocraft_color::aco::write(&result.palette)?,
        "gpl" => photocraft_color::gpl::write(&result.palette)?,
        "kpl" => {
            let xml = photocraft_color::kpl::write_xml(&result.palette)?;
            let mut zip = photocraft_format::zip::ZipWriter::new();
            zip.add("mimetype", b"application/x-krita-palette").map_err(|e| e.to_string())?;
            zip.add("colorset.xml", &xml).map_err(|e| e.to_string())?;
            zip.finish().map_err(|e| e.to_string())?
        }
        _ => return Err("unsupported palette export".into()),
    };
    Ok((bytes, result.warnings))
}
fn open_editor(s: &mut State, app: &PhotocraftApp, w: Option<Swatch>, g: Option<SwatchGroup>) {
    let hex = w
        .as_ref()
        .and_then(|w| match w.paint {
            Fill::Solid(c) => display_color(&app.session, c, s.scope).ok(),
            _ => None,
        })
        .map(|c| {
            let [r, g, b, _] = c.to_rgba8();
            format!("#{r:02X}{g:02X}{b:02X}")
        })
        .unwrap_or_default();
    s.editor =
        Some(Editor { hex, scope: s.scope, capture: if s.scope == Scope::Document { Capture::begin(&app.session).ok() } else { None }, swatch: w, group: g });
}
fn editor(app: &mut PhotocraftApp, ui: &mut egui::Ui, s: &mut State) {
    let Some(mut e) = s.editor.take() else { return };
    if e.capture.is_some_and(|c| c.check(&app.session).is_err()) {
        s.error = Some("Document changed; open Swatch Options again".into());
        return;
    }
    let mut close = false;
    let mut action = None;
    egui::Window::new("Swatch Options").id(egui::Id::new("shared-swatch-options")).collapsible(false).resizable(false).show(ui.ctx(), |ui| {
        if let Some(w) = &mut e.swatch {
            ui.label("Name");
            let name = ui.add(egui::TextEdit::singleline(&mut w.name).id_salt("swatch-name").desired_width(190.).char_limit(1024));
            name.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Swatch name"));
            if let Fill::Solid(c) = &mut w.paint {
                let mut bytes = preview_color(app, ui, e.scope, *c).to_rgba8();
                if ui.color_edit_button_srgba_unmultiplied(&mut bytes).changed() {
                    let edited = Color::rgba(f32::from(bytes[0]) / 255., f32::from(bytes[1]) / 255., f32::from(bytes[2]) / 255., f32::from(bytes[3]) / 255.);
                    match native_color(&app.session, edited, e.scope) {
                        Ok(color) => {
                            *c = color;
                            e.hex = format!("#{:02X}{:02X}{:02X}", bytes[0], bytes[1], bytes[2]);
                        }
                        Err(error) => s.error = Some(error),
                    }
                }
            }
            if let Fill::Solid(c) = &mut w.paint {
                let response = ui.add(egui::TextEdit::singleline(&mut e.hex).hint_text("#RRGGBB").desired_width(140.).char_limit(7));
                response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Swatch hex colour"));
                let value = e.hex.trim_start_matches('#');
                if response.changed()
                    && value.len() == 6
                    && let Ok(value) = u32::from_str_radix(value, 16)
                {
                    let rgb = Color::rgba(((value >> 16) & 255) as f32 / 255., ((value >> 8) & 255) as f32 / 255., (value & 255) as f32 / 255., c.alpha);
                    match native_color(&app.session, rgb, e.scope) {
                        Ok(color) => *c = color,
                        Err(error) => s.error = Some(error),
                    }
                }
                if value.len() != 6 || !value.bytes().all(|v| v.is_ascii_hexdigit()) {
                    ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, "Enter six hexadecimal digits");
                }
            }
            ui.add_enabled_ui(e.scope == Scope::Document && matches!(w.paint, Fill::Solid(_)), |ui| {
                ui.checkbox(&mut w.global, "Live colour")
                    .on_hover_text("Updates linked shapes, text and vector layers in either mode. Already-painted pixels do not change.");
                if ui.checkbox(&mut w.spot, "Spot colour").changed() && w.spot {
                    w.global = true;
                }
            })
            .response
            .on_disabled_hover_text("Live colours require a solid document swatch");
            if !w.global {
                w.spot = false;
            }
            if ui
                .add_enabled(
                    swatches::validate_name(&w.name).is_ok()
                        && (!matches!(w.paint, Fill::Solid(_))
                            || (e.hex.trim_start_matches('#').len() == 6 && e.hex.trim_start_matches('#').bytes().all(|v| v.is_ascii_hexdigit()))),
                    egui::Button::new("Save Swatch"),
                )
                .on_disabled_hover_text("Enter a name (1–1024 characters) and a valid six-digit colour")
                .clicked()
            {
                action = Some(Action::Edit { swatch: w.clone() });
            }
        }
        if let Some(g) = &mut e.group {
            ui.label("Folder name");
            ui.text_edit_singleline(&mut g.name);
            if ui
                .add_enabled(swatches::validate_name(&g.name).is_ok(), egui::Button::new("Save Folder"))
                .on_disabled_hover_text("Enter a folder name of 1–1024 characters")
                .clicked()
            {
                action = Some(Action::RenameGroup { id: g.id, name: g.name.clone() });
            }
            if ui.button("Delete Folder and Swatches").clicked() {
                action = Some(Action::DeleteGroup { id: g.id });
            }
        }
        if ui.button("Cancel").clicked() {
            close = true;
        }
    });
    if let Some(action) = action {
        match apply(app, e.scope, action) {
            Ok(()) => close = true,
            Err(error) => s.error = Some(error),
        }
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    if !close {
        s.editor = Some(e);
    }
}
fn chip(ui: &mut egui::Ui, w: &Swatch, list: bool, selected: bool, color: Option<Color>) -> egui::Response {
    let color = color.unwrap_or_else(|| match &w.paint {
        Fill::Solid(c) => *c,
        Fill::Gradient { stops, .. } => stops.first().map_or(Color::BLACK, |s| s.1),
        _ => Color::BLACK,
    });
    let [r, g, b, a] = color.to_rgba8();
    let c = Color32::from_rgba_unmultiplied(r, g, b, a);
    let label = if list { format!("      {}", w.name) } else { String::new() };
    let response = ui.add_sized(
        if list { egui::vec2(ui.available_width(), 26.) } else { egui::vec2(24., 24.) },
        egui::Button::new(label).sense(egui::Sense::click_and_drag()).fill(if list { crate::theme::Tokens::get(ui.ctx()).field } else { c }).selected(selected),
    );
    if list {
        let rect = egui::Rect::from_min_size(response.rect.left_top() + egui::vec2(4., 4.), egui::vec2(18., 18.));
        ui.painter().rect_filled(rect, 2., c);
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &w.name));
    if w.global {
        let p = response.rect.right_bottom();
        ui.painter().add(egui::Shape::convex_polygon(
            vec![p, p - egui::vec2(7., 0.), p - egui::vec2(0., 7.)],
            if w.spot { Color32::BLACK } else { Color32::WHITE },
            egui::Stroke::NONE,
        ));
    }
    response.on_hover_text(format!(
        "{}{}{}\nClick: primary colour · Alt/right-click: secondary colour",
        w.name,
        if w.global { " · Live colour: linked objects update; painted pixels do not" } else { "" },
        if w.spot { " · Spot" } else { "" }
    ))
}
pub(crate) fn panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    seed(app);
    let mut s = load(app, ui);
    let mut action = None;
    let mut load_library = None;
    let mut import_requested = false;
    let mut export_requested = None;
    targets_picker(app, ui, &mut s);
    let old_scope = s.scope;
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut s.scope, Scope::User, "User Swatches");
        ui.add_enabled_ui(app.session.active().is_some(), |ui| ui.selectable_value(&mut s.scope, Scope::Document, "Document Swatches"))
            .response
            .on_disabled_hover_text("Open a document to use its swatches");
        ui.menu_button("Swatches menu", |ui| {
            ui.checkbox(&mut s.list, "List with names");
            ui.menu_button("Swatch Libraries", |ui| {
                for &(name, data) in LIBRARIES {
                    if ui.button(name).clicked() {
                        load_library = Some((name, data));
                        ui.close();
                    }
                }
            });
            if ui.add_enabled(palette_picker_available(app), egui::Button::new("Import Swatches…")).on_disabled_hover_text("File picker unavailable").clicked()
            {
                import_requested = true;
                ui.close();
            }
            ui.menu_button("Export Swatches", |ui| {
                for ext in ["ase", "aco", "gpl", "kpl"] {
                    if ui
                        .add_enabled(
                            (app.services.pick_save.is_some() || (cfg!(not(target_arch = "wasm32")) && app.background_jobs)) && app.services.write.is_some(),
                            egui::Button::new(format!("{}…", ext.to_ascii_uppercase())),
                        )
                        .on_disabled_hover_text("File saving unavailable")
                        .clicked()
                    {
                        export_requested = Some(ext);
                        ui.close();
                    }
                }
            });
        });
    });
    if s.scope == Scope::Document && app.session.active().is_none() {
        s.scope = Scope::User;
    }
    let document = app.session.active().map(|st| st.doc.id.0);
    if old_scope != s.scope || (s.scope == Scope::Document && s.document != document) {
        s.selected = None;
        s.group = None;
        s.editor = None;
        s.reveal = None;
    }
    s.document = document;
    let reveal = s.reveal.take();
    let b = book(app, s.scope);
    s.selected = s.selected.filter(|id| b.swatch(*id).is_some());
    s.group = s.group.filter(|id| b.groups.iter().any(|g| g.id == *id));
    let search =
        ui.add(egui::TextEdit::singleline(&mut s.search).hint_text("Search swatches by name").desired_width(ui.available_width()).id_salt("swatch-search"));
    search.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Search swatches"));
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(b.swatches.len() + b.groups.len() < swatches::MAX_SWATCHES, egui::Button::new("Add Current Colour"))
            .on_disabled_hover_text("Palette is full (4096 entries)")
            .clicked()
        {
            match b.next_id() {
                Ok(id) => {
                    action = Some(Action::Add {
                        swatch: Swatch {
                            id: SwatchId(id),
                            name: b.free_name("Colour"),
                            paint: Fill::Solid(native_color(&app.session, current(app, s.secondary), s.scope).unwrap_or_else(|_| current(app, s.secondary))),
                            global: false,
                            spot: false,
                            group: s.group,
                            stops: vec![],
                        },
                    })
                }
                Err(e) => s.error = Some(e),
            }
        }
        if ui
            .add_enabled(b.swatches.len() + b.groups.len() < swatches::MAX_SWATCHES, egui::Button::new("New Folder"))
            .on_disabled_hover_text("Palette is full (4096 entries)")
            .clicked()
        {
            match b.next_id() {
                Ok(id) => action = Some(Action::Group { group: SwatchGroup { id, name: b.free_name("Colour Group") }, ids: s.selected.into_iter().collect() }),
                Err(e) => s.error = Some(e),
            }
        }
        if ui.add_enabled(s.selected.is_some(), egui::Button::new("Swatch Options…")).on_disabled_hover_text("Select a swatch first").clicked() {
            let w = s.selected.and_then(|id| b.swatch(id)).cloned();
            open_editor(&mut s, app, w, None);
        }
    });
    ui.horizontal_wrapped(|ui| {
        let ts = targets(app, s.secondary);
        if ui
            .add_enabled(s.scope == Scope::Document && s.selected.is_some() && !ts.is_empty(), egui::Button::new("Apply to Selected Objects"))
            .on_disabled_hover_text("Choose a document swatch and select a shape or text layer")
            .clicked()
            && let Some(w) = s.selected.and_then(|id| b.swatch(id))
        {
            action = Some(if w.global { Action::Assign { targets: ts, swatch: w.id, tint: 100. } } else { Action::AssignPaint { targets: ts, swatch: w.id } });
        }
    });
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(s.selected.is_some(), egui::Button::new("Delete Selected")).on_disabled_hover_text("Select a swatch first").clicked() {
            action = Some(Action::Delete { ids: s.selected.into_iter().collect() });
        }
        ui.add_enabled_ui(s.selected.is_some(), |ui| {
            ui.menu_button("Move Selected", |ui| {
                if let Some(id) = s.selected {
                    let folder = b.swatch(id).and_then(|w| w.group);
                    let members: Vec<_> = b.swatches.iter().filter(|w| w.group == folder).map(|w| w.id).collect();
                    if let Some(at) = members.iter().position(|w| *w == id) {
                        if ui.add_enabled(at > 0, egui::Button::new("Move Earlier")).on_disabled_hover_text("Already first in this folder").clicked() {
                            action = Some(Action::Move { id, group: folder, before: Some(members[at - 1]) });
                            ui.close();
                        }
                        if ui
                            .add_enabled(at + 1 < members.len(), egui::Button::new("Move Later"))
                            .on_disabled_hover_text("Already last in this folder")
                            .clicked()
                        {
                            action = Some(Action::Move { id, group: folder, before: members.get(at + 2).copied() });
                            ui.close();
                        }
                    }
                }
                ui.separator();
                for g in std::iter::once(None).chain(b.groups.iter().map(Some)) {
                    if ui.button(g.map_or("Ungrouped", |g| g.name.as_str())).clicked()
                        && let Some(id) = s.selected
                    {
                        action = Some(Action::Move { id, group: g.map(|g| g.id), before: None });
                        ui.close();
                    }
                }
            });
        })
        .response
        .on_disabled_hover_text("Select a swatch first");
        if ui.add_enabled(s.group.is_some(), egui::Button::new("Folder Options…")).on_disabled_hover_text("Select a folder first").clicked() {
            let g = s.group.and_then(|id| b.groups.iter().find(|g| g.id == id)).cloned();
            open_editor(&mut s, app, None, g);
        }
    });
    if !s.recent.is_empty() {
        egui::CollapsingHeader::new("Recent Colours").default_open(true).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, c) in s.recent.clone().iter().enumerate() {
                    let w = Swatch {
                        id: SwatchId(u64::MAX - i as u64),
                        name: format!("Recent colour {}", i + 1),
                        paint: Fill::Solid(*c),
                        global: false,
                        spot: false,
                        group: None,
                        stops: vec![],
                    };
                    let r = chip(ui, &w, false, false, None);
                    if r.clicked() || r.secondary_clicked() {
                        let secondary = r.secondary_clicked() || ui.input(|i| i.modifiers.alt);
                        let scope = s.scope;
                        let selected = s.selected;
                        let group = s.group;
                        s.scope = Scope::User;
                        if let Err(e) = select(app, &mut s, &w, secondary) {
                            s.error = Some(e);
                        }
                        s.scope = scope;
                        s.selected = selected;
                        s.group = group;
                    }
                }
            });
        });
    }
    let filter = s.search.to_lowercase();
    let palette_height = (ui.available_height() - 48.).clamp(80., 340.);
    egui::ScrollArea::vertical().id_salt("swatches-scroll").max_height(palette_height).show(ui, |ui| {
        for group in std::iter::once(None).chain(b.groups.iter().map(Some)) {
            let gid = group.map(|g| g.id);
            let members: Vec<_> = b.swatches.iter().filter(|w| w.group == gid && w.name.to_lowercase().contains(&filter)).collect();
            if !filter.is_empty() && members.is_empty() {
                continue;
            }
            let header = egui::CollapsingHeader::new(group.map_or("Ungrouped", |g| g.name.as_str()))
                .id_salt((s.scope as u8, gid))
                .default_open(group.is_some() || !members.is_empty())
                .open(if !filter.is_empty() || reveal == Some(gid) { Some(true) } else { None })
                .show(ui, |ui| {
                    let draw = |ui: &mut egui::Ui, s: &mut State, action: &mut Option<Action>, app: &mut PhotocraftApp, row: &[&Swatch]| {
                        for w in row {
                            ui.push_id((s.scope as u8, w.id.0), |ui| {
                                let response = chip(
                                    ui,
                                    w,
                                    s.list,
                                    s.selected == Some(w.id),
                                    match w.paint {
                                        Fill::Solid(c) => Some(preview_color(app, ui, s.scope, c)),
                                        _ => None,
                                    },
                                );
                                response.dnd_set_drag_payload(Drag {
                                    scope: s.scope,
                                    id: w.id,
                                    capture: if s.scope == Scope::Document { Capture::begin(&app.session).ok() } else { None },
                                });
                                if let Some(d) = response.dnd_release_payload::<Drag>()
                                    && valid_drag(&d, app, s.scope)
                                {
                                    *action = Some(Action::Move { id: d.id, group: gid, before: Some(w.id) });
                                }
                                if response.clicked() || response.secondary_clicked() {
                                    let secondary = response.secondary_clicked() || ui.input(|i| i.modifiers.alt);
                                    if let Err(e) = select(app, s, w, secondary) {
                                        s.error = Some(e);
                                    }
                                }
                                if response.double_clicked() {
                                    open_editor(s, app, Some((*w).clone()), None);
                                }
                                response.context_menu(|ui| {
                                    if ui.button("Swatch Options…").clicked() {
                                        open_editor(s, app, Some((*w).clone()), None);
                                        ui.close();
                                    }
                                    if ui.button("Delete Swatch").clicked() {
                                        *action = Some(Action::Delete { ids: vec![w.id] });
                                        ui.close();
                                    }
                                    ui.menu_button("Move to Folder", |ui| {
                                        for g in std::iter::once(None).chain(b.groups.iter().map(Some)) {
                                            if ui.button(g.map_or("Ungrouped", |g| g.name.as_str())).clicked() {
                                                *action = Some(Action::Move { id: w.id, group: g.map(|g| g.id), before: None });
                                                ui.close();
                                            }
                                        }
                                    });
                                    if ui.button("Move Earlier").clicked() {
                                        if let Some(i) = members.iter().position(|v| v.id == w.id)
                                            && i > 0
                                        {
                                            *action = Some(Action::Move { id: w.id, group: gid, before: Some(members[i - 1].id) });
                                        }
                                        ui.close();
                                    }
                                    if ui.button("Move Later").clicked() {
                                        if let Some(i) = members.iter().position(|v| v.id == w.id)
                                            && i + 1 < members.len()
                                        {
                                            *action = Some(Action::Move { id: w.id, group: gid, before: members.get(i + 2).map(|w| w.id) });
                                        }
                                        ui.close();
                                    }
                                });
                            });
                        }
                    };
                    if s.list {
                        draw(ui, &mut s, &mut action, app, &members);
                    } else {
                        let cols = ((ui.available_width() + ui.spacing().item_spacing.x) / (24. + ui.spacing().item_spacing.x)).floor().max(1.) as usize;
                        for row in members.chunks(cols) {
                            ui.horizontal(|ui| draw(ui, &mut s, &mut action, app, row));
                        }
                    }
                    if members.is_empty() {
                        ui.label("Drop swatches here");
                    }
                });
            if let Some(response) = &header.body_response
                && let Some(d) = response.dnd_release_payload::<Drag>()
                && valid_drag(&d, app, s.scope)
            {
                action = Some(Action::Move { id: d.id, group: gid, before: None });
            }
            if reveal == Some(gid) {
                header.header_response.scroll_to_me(Some(egui::Align::Center));
            }
            if header.header_response.clicked() {
                s.group = gid;
            }
            if let Some(d) = header.header_response.dnd_release_payload::<Drag>()
                && valid_drag(&d, app, s.scope)
            {
                action = Some(Action::Move { id: d.id, group: gid, before: None });
            }
            if let Some(g) = group {
                header.header_response.context_menu(|ui| {
                    if ui.button("Rename Folder…").clicked() {
                        open_editor(&mut s, app, None, Some(g.clone()));
                        ui.close();
                    }
                    if ui.button("Delete Folder and Swatches").clicked() {
                        action = Some(Action::DeleteGroup { id: g.id });
                        ui.close();
                    }
                });
            }
        }
    });
    ui.label(RichText::new("Click: primary · Alt/right-click: secondary").small());
    ui.label(RichText::new("Live colours update linked objects in both modes.").small())
        .on_hover_text("Linked shapes, text and vector layers update. Already-painted pixels do not change.");
    if let Some(action) = action {
        let reveal = match &action {
            Action::Add { swatch } => Some((swatch.group, Some(swatch.id))),
            Action::Group { group, .. } => Some((Some(group.id), s.selected)),
            _ => None,
        };
        match apply(app, s.scope, action) {
            Ok(()) => {
                s.error = None;
                if let Some((group, selected)) = reveal {
                    s.reveal = Some(group);
                    s.selected = selected;
                    s.group = group;
                }
            }
            Err(e) => s.error = Some(e),
        }
    }
    if let Some((name, data)) = load_library {
        match library(name, data).and_then(|palette| {
            if s.scope == Scope::Document {
                let bytes = swatches::export_ase(&palette)?.bytes;
                import(app, s.scope, &format!("{name}.ase"), &bytes).map(|warnings| {
                    app.ui.status = warnings.join("; ");
                })
            } else {
                apply(app, s.scope, Action::Import { palette, replace: false })
            }
        }) {
            Ok(()) => s.error = None,
            Err(e) => s.error = Some(e),
        }
    }
    if import_requested && let Some((name, bytes)) = pick_palette(app) {
        match bytes.and_then(|bytes| import(app, s.scope, &name, &bytes)) {
            Ok(warnings) => {
                s.error = None;
                app.ui.status = warnings.join("; ");
            }
            Err(e) => s.error = Some(e),
        }
    }
    if let Some(ext) = export_requested {
        match export(app, s.scope, ext) {
            Ok((bytes, warnings)) => {
                if let Some(path) = pick_palette_save(app, ext) {
                    match app.services.write.as_mut().ok_or("File writer unavailable".into()).and_then(|write| write(&path, &bytes)) {
                        Ok(()) => app.ui.status = warnings.join("; "),
                        Err(e) => s.error = Some(e),
                    }
                }
            }
            Err(e) => s.error = Some(e),
        }
    }
    let region = DropRegion { rect: ui.min_rect(), layer: ui.layer_id(), frame: ui.ctx().cumulative_frame_nr(), scope: s.scope };
    ui.data_mut(|d| d.insert_temp(drop_region_id(), region));
    // Standalone panel hosts also use the dispatch seam. The desktop shell must
    // call it before its image-open dispatch (see report's prepared hook).
    let files = ui.input(|i| i.raw.dropped_files.clone());
    if !files.is_empty() {
        save(app, ui, s.clone());
        let at = app.services.cursor_pos.as_mut().and_then(|f| f(ui.ctx())).or_else(|| ui.ctx().pointer_latest_pos());
        let _ = route_drops(app, ui.ctx(), files, at);
        s = load(app, ui);
    }
    editor(app, ui, &mut s);
    if let Some(e) = &s.error {
        ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e);
    }
    save(app, ui, s);
}
#[derive(Clone)]
struct DropRegion {
    rect: egui::Rect,
    layer: egui::LayerId,
    frame: u64,
    scope: Scope,
}
fn drop_region_id() -> egui::Id {
    egui::Id::new("swatches-drop-region")
}
/// Return only files the normal document-open path should receive. No palette
/// is read unless the last visible Swatches panel owns the pointer position.
pub(crate) fn route_drops(
    app: &mut PhotocraftApp,
    ctx: &egui::Context,
    files: Vec<egui::DroppedFileHandle>,
    at: Option<egui::Pos2>,
) -> Vec<egui::DroppedFileHandle> {
    if files.is_empty() {
        return files;
    }
    let region: Option<DropRegion> = ctx.data(|d| d.get_temp(drop_region_id()));
    let Some(region) = region else { return files };
    let frame = ctx.cumulative_frame_nr();
    let Some(at) = at.filter(|at| region.rect.contains(*at) && region.frame.saturating_add(1) >= frame && ctx.layer_id_at(*at) == Some(region.layer)) else {
        return files;
    };
    let _ = at;
    let mut state: State = ctx.data(|d| d.get_temp(state_id()).unwrap_or_default());
    let already = state.last_drop_frame == Some(frame);
    let mut remaining = Vec::new();
    for file in files {
        let name = file.path().to_string_lossy().into_owned();
        let ext = file.path().extension().and_then(|v| v.to_str()).unwrap_or("").to_ascii_lowercase();
        if !["ase", "aco", "gpl", "kpl"].contains(&ext.as_str()) {
            remaining.push(file);
            continue;
        }
        if already {
            continue;
        }
        match read_palette_drop(&file).and_then(|bytes| import(app, region.scope, &name, &bytes)) {
            Ok(warnings) => {
                app.ui.status = warnings.join("; ");
                app.ui.status_error = false;
                state.error = None;
            }
            Err(error) => {
                app.ui.status_error = true;
                app.ui.status = error.clone();
                state.error = Some(error);
            }
        }
    }
    state.last_drop_frame = Some(frame);
    ctx.data_mut(|d| d.insert_temp(state_id(), state));
    remaining
}
#[cfg(not(target_arch = "wasm32"))]
fn read_palette_drop(file: &egui::DroppedFileHandle) -> Result<Vec<u8>, String> {
    if !file.path().is_absolute() {
        let bytes = file.bytes()?;
        if bytes.len() > ase::MAX_BYTES {
            return Err("Palette exceeds 16 MiB".into());
        }
        return Ok(bytes);
    }
    read_palette_path(file.path())
}
#[cfg(not(target_arch = "wasm32"))]
fn read_palette_path(path: &std::path::Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    photocraft_format::read::validate_path(path).map_err(|e| e.to_string())?;
    let input = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let metadata = input.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > ase::MAX_BYTES as u64 {
        return Err("Palette exceeds 16 MiB".into());
    }
    let mut bytes = Vec::new();
    input.take(ase::MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > ase::MAX_BYTES {
        return Err("Palette exceeds 16 MiB".into());
    }
    Ok(bytes)
}
#[cfg(target_arch = "wasm32")]
fn read_palette_drop(_: &egui::DroppedFileHandle) -> Result<Vec<u8>, String> {
    Err("Use Import Swatches on the web".into())
}
fn palette_picker_available(app: &PhotocraftApp) -> bool {
    app.services.pick_open.is_some() || (cfg!(not(target_arch = "wasm32")) && app.background_jobs)
}
fn pick_palette(app: &mut PhotocraftApp) -> Option<(String, Result<Vec<u8>, String>)> {
    // Desktop File > Open filters only image/document files. Use the existing
    // dedicated native-dialog API here; standalone/web hosts retain injection.
    #[cfg(not(target_arch = "wasm32"))]
    if app.background_jobs {
        use crate::dialog_filter::FileDialogExt;
        let path =
            crate::dialog_filter::file_dialog().set_title("Import Swatches").add_filter_nocase("Swatch files", &["ase", "aco", "gpl", "kpl"]).pick_file()?;
        return Some((path.to_string_lossy().into_owned(), read_palette_path(&path)));
    }
    app.services.pick_open.as_mut().and_then(|pick| pick())
}
fn pick_palette_save(app: &mut PhotocraftApp, ext: &str) -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    if app.background_jobs {
        use crate::dialog_filter::FileDialogExt;
        let path = crate::dialog_filter::file_dialog()
            .set_title("Export Swatches")
            .add_filter_nocase("Swatch file", &[ext])
            .set_file_name(format!("Palette.{ext}"))
            .save_file()?;
        return Some(path.to_string_lossy().into_owned());
    }
    app.services.pick_save.as_mut().and_then(|pick| pick(&format!("Palette.{ext}")))
}
#[cfg(test)]
#[path = "swatches_panel_tests.rs"]
mod tests;
