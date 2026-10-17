//! One sourced registry, SVG rasterizer and bounded texture cache for both UI modules.
//! Interface strokes stay at 1.75 px and tool strokes at 2 px: these are deliberate profiles.
use egui::load::{SizeHint, SizedTexture, TextureLoadResult, TextureLoader, TexturePoll};
use egui::{Color32, ColorImage, Context, Image, TextureHandle, TextureOptions, Vec2};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum StrokeStyle {
    Interface,
    Tool,
}
struct Asset {
    bytes: &'static [u8],
    white: [OnceLock<Arc<[u8]>>; 2],
}
struct Registry {
    assets: Vec<Asset>,
    source: HashMap<(usize, usize), usize>,
    by_content: HashMap<&'static [u8], usize>,
    generic: HashMap<&'static str, usize>,
    tools: HashMap<&'static str, usize>,
}
fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut r = Registry { assets: Vec::new(), source: HashMap::new(), generic: HashMap::new(), tools: HashMap::new(), by_content: HashMap::new() };
        let mut by_content = HashMap::new();
        let mut add = |bytes: &'static [u8]| -> usize {
            let id = *by_content.entry(bytes).or_insert_with(|| {
                let id = r.assets.len();
                r.assets.push(Asset { bytes, white: Default::default() });
                id
            });
            r.source.insert(source_key(bytes), id);
            id
        };
        for &(name, bytes) in crate::icon_data::ICONS {
            r.generic.insert(name, add(bytes));
        }
        // Keep the old tool table's first-match priority for overlapping vector IDs.
        for (name, bytes) in
            crate::tool_icon_data::TOOL_ICONS.iter().copied().chain(crate::vector_icon_data::ICONS.iter().map(|(_, name, bytes)| (*name, *bytes)))
        {
            r.tools.entry(name).or_insert_with(|| add(bytes));
        }
        for &icon in crate::library_icons::ALL {
            add(crate::library_icons::svg(icon));
        }
        r.by_content = by_content;
        r
    })
}
fn source_key(bytes: &'static [u8]) -> (usize, usize) {
    (bytes.as_ptr() as usize, bytes.len())
}
// Promoted include_bytes slices (notably the legacy vector const table) may have different
// addresses across codegen units. Content equality remains the authority for those aliases.
fn source_id(bytes: &'static [u8]) -> usize {
    let registry = registry();
    registry.source.get(&source_key(bytes)).copied().unwrap_or_else(|| registry.by_content[bytes])
}
fn index(style: StrokeStyle) -> usize {
    match style {
        StrokeStyle::Interface => 0,
        StrokeStyle::Tool => 1,
    }
}
fn white(id: usize, style: StrokeStyle) -> Arc<[u8]> {
    let asset = &registry().assets[id];
    asset.white[index(style)]
        .get_or_init(|| {
            let svg = String::from_utf8_lossy(asset.bytes).replace("currentColor", "#ffffff");
            let svg = if style == StrokeStyle::Interface { svg.replace("stroke-width=\"2\"", "stroke-width=\"1.75\"") } else { svg };
            Arc::from(svg.into_bytes())
        })
        .clone()
}
pub fn prepared(bytes: &'static [u8], style: StrokeStyle) -> Arc<[u8]> {
    white(source_id(bytes), style)
}
pub fn tool_svg(name: &str) -> Option<&'static [u8]> {
    Some(registry().assets[*registry().tools.get(name)?].bytes)
}
pub fn exists(name: &str) -> bool {
    registry().generic.contains_key(name) || registry().tools.contains_key(name)
}
fn uri(id: usize, style: StrokeStyle) -> String {
    format!("bytes://li-ui-kit/{id}/{}.svg", index(style))
}
fn asset_image(id: usize, style: StrokeStyle, size: f32, tint: Color32) -> Image<'static> {
    Image::from_bytes(uri(id, style), egui::load::Bytes::Shared(white(id, style))).fit_to_exact_size(Vec2::splat(size)).tint(tint)
}
pub fn source_image(bytes: &'static [u8], style: StrokeStyle, size: f32, tint: Color32) -> Image<'static> {
    asset_image(source_id(bytes), style, size, tint)
}
pub fn image(name: &str, size: f32, tint: Color32) -> Image<'static> {
    let r = registry();
    if let Some(&id) = r.generic.get(name) {
        return asset_image(id, StrokeStyle::Interface, size, tint);
    }
    if let Some(&id) = r.tools.get(name) {
        return asset_image(id, StrokeStyle::Tool, size, tint);
    }
    // All missing IDs share one fallback URI, preventing aliases from multiplying cache entries.
    asset_image(r.generic["square"], StrokeStyle::Interface, size, tint)
}
/// Existing resvg rasterization, including the colour-mode disabled transformation.
pub fn raster(bytes: &'static [u8], style: StrokeStyle, pixels: u32, colour: Color32, disabled: bool) -> Result<ColorImage, String> {
    let white = prepared(bytes, style);
    let hex = format!("#{:02x}{:02x}{:02x}", colour.r(), colour.g(), colour.b());
    let mut tinted;
    let svg = if colour == Color32::WHITE {
        white.as_ref()
    } else {
        tinted = String::from_utf8_lossy(bytes).replace("currentColor", &hex);
        if style == StrokeStyle::Interface {
            tinted = tinted.replace("stroke-width=\"2\"", "stroke-width=\"1.75\"");
        }
        tinted.as_bytes()
    };
    let mut image = egui_extras::image::load_svg_bytes_with_size(svg, SizeHint::Width(pixels), &Default::default())?;
    if disabled {
        for p in &mut image.pixels {
            let [r, g, b, a] = p.to_srgba_unmultiplied();
            let grey = ((u32::from(r) * 54 + u32::from(g) * 183 + u32::from(b) * 19) / 256) as u8;
            *p = Color32::from_rgba_unmultiplied(grey, grey, grey, (u16::from(a) * 45 / 100) as u8);
        }
    }
    Ok(image)
}
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Key {
    asset: usize,
    style: StrokeStyle,
    pixels: u32,
    colour: Color32,
    disabled: bool,
    options: TextureOptions,
}
struct Entry {
    handle: TextureHandle,
    last_pass: u64,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<Key, Entry>,
    bytes: usize,
}
const CACHE_LIMIT: usize = 1024;
const CACHE_BYTES: usize = 8 * 1024 * 1024;
type SharedCache = Arc<Mutex<Cache>>;
fn cache(ctx: &Context) -> SharedCache {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<SharedCache>(egui::Id::new("li-ui-kit/textures")).clone())
}
fn cached_texture(ctx: &Context, key: Key) -> Result<TextureHandle, String> {
    let state = cache(ctx);
    let mut cache = state.lock().unwrap_or_else(|p| p.into_inner());
    let pass = ctx.cumulative_pass_nr();
    if let Some(entry) = cache.entries.get_mut(&key) {
        entry.last_pass = pass;
        return Ok(entry.handle.clone());
    }
    let pixels = raster(registry().assets[key.asset].bytes, key.style, key.pixels, key.colour, key.disabled)?;
    // Evict the least recently used entry, retaining textures referenced by this frame's shapes.
    // A frame drawing >1024 distinct icons may temporarily exceed the cap; subsequent frames trim it.
    let new_bytes = pixels.pixels.len() * std::mem::size_of::<Color32>();
    while cache.entries.len() >= CACHE_LIMIT || cache.bytes + new_bytes > CACHE_BYTES {
        let oldest = cache.entries.iter().filter(|(_, e)| e.last_pass < pass).min_by_key(|(_, e)| e.last_pass).map(|(k, _)| *k);
        let Some(oldest) = oldest else { break };
        if let Some(entry) = cache.entries.remove(&oldest) {
            cache.bytes -= entry.handle.byte_size();
        }
    }
    let handle = ctx.load_texture(format!("li-ui-kit/{key:?}"), pixels, key.options);
    cache.bytes += handle.byte_size();
    cache.entries.insert(key, Entry { handle: handle.clone(), last_pass: pass });
    Ok(handle)
}
pub fn texture(ctx: &Context, bytes: &'static [u8], style: StrokeStyle, pixels: u32, colour: Color32, disabled: bool) -> Option<egui::TextureId> {
    let key = Key { asset: source_id(bytes), style, pixels: pixels.clamp(1, 2048), colour, disabled, options: TextureOptions::LINEAR };
    cached_texture(ctx, key).ok().map(|handle| handle.id())
}
/// Use egui's loader API so image widgets, custom painters and cursors share physical-size textures.
/// Other application images continue through the existing image loaders.
pub fn install(ctx: &Context) {
    if !ctx.is_loader_installed(LOADER_ID) {
        ctx.add_texture_loader(Arc::new(IconLoader { cache: cache(ctx) }));
    }
}
const LOADER_ID: &str = "li-ui-kit/icons";
struct IconLoader {
    cache: SharedCache,
}
impl TextureLoader for IconLoader {
    fn id(&self) -> &str {
        LOADER_ID
    }
    fn load(&self, ctx: &Context, uri: &str, options: TextureOptions, hint: SizeHint) -> TextureLoadResult {
        let Some(rest) = uri.strip_prefix("bytes://li-ui-kit/") else {
            return Err(egui::load::LoadError::NotSupported);
        };
        let parse = || {
            let (id, style) = rest.split_once('/')?;
            let id = id.parse::<usize>().ok()?;
            let style = match style {
                "0.svg" => StrokeStyle::Interface,
                "1.svg" => StrokeStyle::Tool,
                _ => return None,
            };
            (id < registry().assets.len()).then_some((id, style))
        };
        let (asset, style) = parse().ok_or_else(|| egui::load::LoadError::Loading("invalid UI icon URI".into()))?;
        let pixels = match hint {
            SizeHint::Width(p) | SizeHint::Height(p) => p,
            SizeHint::Size { width, height, .. } => width.min(height),
            SizeHint::Scale(s) => (24.0 * s.0).round() as u32,
        }
        .clamp(1, 2048);
        let key = Key { asset, style, pixels, colour: Color32::WHITE, disabled: false, options };
        let handle = cached_texture(ctx, key).map_err(egui::load::LoadError::Loading)?;
        Ok(TexturePoll::Ready { texture: SizedTexture::new(handle.id(), Vec2::splat(24.0)) })
    }
    fn forget(&self, uri: &str) {
        let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        cache.entries.retain(|key, _| uri != self::uri(key.asset, key.style));
        cache.bytes = cache.entries.values().map(|entry| entry.handle.byte_size()).sum();
    }
    fn forget_all(&self) {
        *self.cache.lock().unwrap_or_else(|p| p.into_inner()) = Cache::default();
    }
    fn byte_size(&self) -> usize {
        self.cache.lock().unwrap_or_else(|p| p.into_inner()).bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn j17_icon_textures_share_cache_without_alias_or_context_collisions() {
        let ctx = Context::default();
        install(&ctx);
        let crop = crate::library_icons::svg(crate::library_icons::Icon::Crop);
        let direct = texture(&ctx, crop, StrokeStyle::Interface, 20, Color32::WHITE, false).unwrap();
        let crop_image = image("crop", 20.0, Color32::WHITE);
        let loaded =
            crop_image.source(&ctx).load(&ctx, TextureOptions::LINEAR, SizeHint::Size { width: 20, height: 20, maintain_aspect_ratio: false }).unwrap();
        let TexturePoll::Ready { texture: loaded } = loaded else { panic!("icon loader must complete synchronously") };
        assert_eq!(direct, loaded.id, "Library painter and Compositing widget share one texture");
        let handle = |style, pixels, colour, disabled| texture(&ctx, crop, style, pixels, colour, disabled).unwrap();
        assert_eq!(direct, handle(StrokeStyle::Interface, 20, Color32::WHITE, false));
        assert_ne!(direct, handle(StrokeStyle::Tool, 20, Color32::WHITE, false));
        assert_ne!(direct, handle(StrokeStyle::Interface, 40, Color32::WHITE, false));
        assert_ne!(direct, handle(StrokeStyle::Interface, 20, Color32::from_gray(52), false));
        assert_ne!(direct, handle(StrokeStyle::Interface, 20, Color32::WHITE, true));
        assert_eq!(image("unknown-a", 20.0, Color32::WHITE).uri(), image("unknown-b", 20.0, Color32::WHITE).uri());
        let another = Context::default();
        install(&another);
        texture(&another, crop, StrokeStyle::Interface, 20, Color32::WHITE, false).unwrap();
        assert_eq!(cache(&another).lock().unwrap().entries.len(), 1);
        assert_eq!(cache(&ctx).lock().unwrap().entries.len(), 5);
        assert!(Arc::ptr_eq(
            &prepared(crop, StrokeStyle::Interface),
            &prepared(crate::icon_data::ICONS.iter().find(|(n, _)| *n == "crop").unwrap().1, StrokeStyle::Interface)
        ));
        // Forget/reload honours egui's loader lifecycle and doesn't retain a stale handle.
        ctx.forget_image(crop_image.uri().unwrap());
        assert_ne!(direct, handle(StrokeStyle::Interface, 20, Color32::WHITE, false));
    }
    #[test]
    fn j17_icon_cache_retains_current_frame_and_evicts_old_sizes() {
        let ctx = Context::default();
        let crop = crate::library_icons::svg(crate::library_icons::Icon::Crop);
        let first = texture(&ctx, crop, StrokeStyle::Interface, 1, Color32::WHITE, false).unwrap();
        // Cheap 1px fixtures exercise the real limit, without a GPU or large raster allocations.
        for i in 0..CACHE_LIMIT {
            texture(&ctx, crop, StrokeStyle::Interface, 1, Color32::from_rgb((i % 256) as u8, (i / 256) as u8, 42), false).unwrap();
        }
        assert_eq!(cache(&ctx).lock().unwrap().entries.len(), CACHE_LIMIT + 1, "in-flight shapes must remain valid");
        assert_eq!(texture(&ctx, crop, StrokeStyle::Interface, 1, Color32::WHITE, false), Some(first));
        ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
        texture(&ctx, crop, StrokeStyle::Interface, 1, Color32::BLACK, false).unwrap();
        assert_eq!(cache(&ctx).lock().unwrap().entries.len(), CACHE_LIMIT);
    }
}

#[cfg(test)]
#[test]
fn j17_icon_cache_obeys_byte_budget_for_large_dpi_glyphs() {
    let ctx = Context::default();
    let crop = crate::library_icons::svg(crate::library_icons::Icon::Crop);
    for i in 0..33 {
        texture(&ctx, crop, StrokeStyle::Interface, 256, Color32::from_rgb(i, 42, 72), false).unwrap();
    }
    assert_eq!(cache(&ctx).lock().unwrap().entries.len(), 33, "current-frame textures stay alive");
    ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
    texture(&ctx, crop, StrokeStyle::Interface, 256, Color32::WHITE, false).unwrap();
    let shared = cache(&ctx);
    let state = shared.lock().unwrap();
    assert!(state.bytes <= CACHE_BYTES);
    assert_eq!(state.entries.len(), 32);
    assert_eq!(state.bytes, state.entries.values().map(|entry| entry.handle.byte_size()).sum::<usize>());
}

#[cfg(test)]
#[test]
fn j17_small_icon_frame_sample_removes_duplicate_rasters_and_uploads() {
    // Small CPU egui frame sample of the earlier image/painter paths versus the
    // shared paths. Timings are observations, not a hardware-specific acceptance threshold.
    fn sample(shared: bool) -> (usize, Vec<u128>) {
        let ctx = Context::default();
        egui_extras::install_image_loaders(&ctx);
        if shared {
            install(&ctx);
        }
        let source = crate::library_icons::svg(crate::library_icons::Icon::Crop);
        let old_bytes: Arc<[u8]> =
            Arc::from(String::from_utf8_lossy(source).replace("currentColor", "#ffffff").replace("stroke-width=\"2\"", "stroke-width=\"1.75\"").into_bytes());
        let old_image = Image::from_bytes("bytes://j17-baseline/crop.svg", egui::load::Bytes::Shared(old_bytes.clone())).fit_to_exact_size(Vec2::splat(20.0));
        let mut old_painter = None;
        let mut uploads = 0;
        let mut times = Vec::new();
        for frame in 0..13 {
            let started = std::time::Instant::now();
            let mut output = ctx.run_ui(Default::default(), |ui| {
                let painter_id = if shared {
                    texture(ui.ctx(), source, StrokeStyle::Interface, 20, Color32::WHITE, false).unwrap()
                } else {
                    old_painter
                        .get_or_insert_with(|| {
                            let pixels = egui_extras::image::load_svg_bytes_with_size(&old_bytes, SizeHint::Width(20), &Default::default()).unwrap();
                            ui.ctx().load_texture("j17-baseline/library/crop", pixels, TextureOptions::LINEAR)
                        })
                        .id()
                };
                ui.painter().image(
                    painter_id,
                    egui::Rect::from_min_size(egui::pos2(10., 10.), Vec2::splat(20.)),
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                    Color32::WHITE,
                );
                if shared {
                    image("crop", 20., Color32::WHITE).paint_at(ui, egui::Rect::from_min_size(egui::pos2(40., 10.), Vec2::splat(20.)));
                } else {
                    old_image.paint_at(ui, egui::Rect::from_min_size(egui::pos2(40., 10.), Vec2::splat(20.)));
                }
            });
            times.push(started.elapsed().as_micros());
            uploads += output.textures_delta.set.values().flatten().filter(|delta| delta.image.size() == [20, 20]).count();
            if frame > 0 {
                assert!(output.textures_delta.set.values().flatten().all(|delta| delta.image.size() != [20, 20]), "warm frames must not rerasterize/upload");
            }
            output.textures_delta.clear();
        }
        (uploads, times)
    }
    let (before, old_times) = sample(false);
    let (after, new_times) = sample(true);
    assert_eq!((before, after), (2, 1));
    let median = |times: &[u128]| {
        let mut sorted = times[1..].to_vec();
        sorted.sort_unstable();
        sorted[sorted.len() / 2]
    };
    println!(
        "J17 small egui icon-path sample (13 CPU frames each): old/shared cold {} / {} us, warm median {} / {} us; raster uploads {} / {}; RGBA texture bytes {} / {}",
        old_times[0],
        new_times[0],
        median(&old_times),
        median(&new_times),
        before,
        after,
        before * 20 * 20 * 4,
        after * 20 * 20 * 4
    );
}
