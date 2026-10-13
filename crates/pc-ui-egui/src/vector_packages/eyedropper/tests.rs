//! Offscreen egui pointer fixtures, original GPL-3.0-or-later artwork. GPU test is deferred.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui::{Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, FillItem, StrokeItem};
use photocraft_doc::{Color, ColorMode, Fill, Layer, LayerContent, Path, SampleType, ShapeLayer, ShapeStroke, Size, Subpath};
use std::sync::Arc;
pub(super) struct Host {
    pub(super) session: Session,
    tool: State,
    rect: Rect,
    pub(super) ids: Vec<LayerId>,
    error: Option<String>,
    gpu: Option<Gpu>,
    texture: Option<egui::TextureHandle>,
}
struct Gpu {
    device: eframe::wgpu::Device,
    queue: eframe::wgpu::Queue,
    compositor: photocraft_gpu::Compositor,
    previews: usize,
    releases: usize,
}
pub(super) fn host() -> Host {
    let mut doc = Document::new("Eyedropper clicks", Size::new(300, 100), ColorMode::Rgb, SampleType::U8);
    let mut ids = Vec::new();
    for x in [10., 80., 150.] {
        let sh = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(x, 20.), (x + 30., 20.), (x + 30., 70.), (x, 70.)])]),
            fill: Some(Fill::Solid(Color::WHITE)),
            ..Default::default()
        };
        let mut l = Layer::new("Object", LayerContent::Shape(sh));
        if x == 10. {
            l.vector.appearance = Some(Appearance {
                items: vec![
                    AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgb(1., 0., 0.)))),
                    AppearanceItem::Fill(FillItem {
                        paint: Fill::Solid(Color::rgb(0., 1., 0.)),
                        opacity: 0.5,
                        blend: photocraft_doc::BlendMode::Normal,
                        visible: true,
                    }),
                    AppearanceItem::Stroke(StrokeItem {
                        base: ShapeStroke { width: 2., paint: Fill::Solid(Color::BLACK), dashes: vec![3., 2.], ..Default::default() },
                        ..Default::default()
                    }),
                ],
            });
        }
        if let LayerContent::Shape(sh) = &mut l.content {
            sh.cache =
                Some(photocraft_vector::appearance::render_shape(sh, l.vector.appearance.as_ref(), &doc.patterns, doc.pixel_format(), doc.bounds()).unwrap());
        }
        ids.push(l.id);
        doc.layers.push(l);
    }
    let mut l = Layer::raster("raster", doc.pixel_format());
    if let LayerContent::Raster(s) = &mut l.content {
        s.fill_rect(photocraft_geom::Rect::new(220, 20, 250, 70), &[0.2, 0.4, 0.8, 1.]);
    }
    ids.push(l.id);
    doc.layers.push(l);
    let mut session = Session::new();
    session.add_document(doc, None);
    session.select_layer(ids[1]).unwrap();
    Host { session, tool: State::default(), rect: Rect::NOTHING, ids, error: None, gpu: None, texture: None }
}
fn hit(doc: &Document, p: Point) -> Result<Option<Hit>> {
    for l in doc.layers.iter().rev() {
        let bounds = match &l.content {
            LayerContent::Shape(sh) => sh.path.control_bounds().map(|(a, b, c, d)| [a, b, c, d]),
            _ => l.surface().map(|s| {
                let b = s.content_bounds();
                [f64::from(b.x0), f64::from(b.y0), f64::from(b.x1), f64::from(b.y1)]
            }),
        };
        if bounds.is_some_and(|[a, b, c, d]| p.x >= a && p.x <= c && p.y >= b && p.y <= d) {
            return Ok(Some(Hit { layer: l.id, byte: None, stroke: false }));
        }
    }
    Ok(None)
}
fn marquee(doc: &Document, a: Point, b: Point) -> Result<Vec<LayerId>> {
    Ok(doc
        .layers
        .iter()
        .filter(|l| match &l.content {
            LayerContent::Shape(sh) => {
                sh.path.control_bounds().is_some_and(|(x0, y0, x1, y1)| x0 >= a.x.min(b.x) && y0 >= a.y.min(b.y) && x1 <= a.x.max(b.x) && y1 <= a.y.max(b.y))
            }
            _ => false,
        })
        .map(|l| l.id)
        .collect())
}
fn frame(ui: &mut Ui, h: &mut Host) {
    context(ui, &mut h.tool);
    let (rect, _) = ui.allocate_exact_size(vec2(300., 100.), egui::Sense::click_and_drag());
    h.rect = rect;
    h.error = input(&mut h.tool, &mut h.session, ui.ctx(), rect, |p| Point::new(f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y)), hit, marquee, true)
        .err()
        .map(|e| e.to_string());
    if let Some(gpu) = &mut h.gpu {
        let doc = h.tool.preview.as_ref().unwrap_or(&h.session.active().unwrap().doc);
        let pixels = photocraft_gpu::render_to_vec(&mut gpu.compositor, &gpu.device, &gpu.queue, doc, doc.bounds()).unwrap();
        let cpu = photocraft_compose::flatten(doc);
        assert_eq!(cpu.px.len(), pixels.len());
        let max = cpu
            .px
            .iter()
            .zip(&pixels)
            .flat_map(|(a, b)| (0..4).map(move |i| if i == 3 { (a[i] - b[i]).abs() } else { (a[i] * a[3] - b[i] * b[3]).abs() }))
            .fold(0_f32, f32::max);
        assert!(max <= 1. / 255., "GPU premultiplied {max}");
        if h.tool.preview.is_some() {
            gpu.previews += 1;
        } else if h.session.active().unwrap().history.past_len() > 0 {
            gpu.releases += 1;
        }
        let pixels = pixels.into_iter().flat_map(|p| p.map(|v| (v.clamp(0., 1.) * 255.).round() as u8)).collect::<Vec<_>>();
        let image = egui::ColorImage::from_rgba_unmultiplied([300, 100], &pixels);
        if let Some(t) = &mut h.texture {
            t.set(image, egui::TextureOptions::NEAREST);
        } else {
            h.texture = Some(ui.ctx().load_texture("styled document", image, egui::TextureOptions::NEAREST));
        }
        let t = h.texture.as_ref().unwrap();
        ui.painter().image(t.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)), egui::Color32::WHITE);
    }
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(vec2(1000., 330.)).build_ui_state(frame, host())
}
fn event(h: &mut Harness<'_, Host>, p: (f32, f32), pressed: Option<bool>, mods: Modifiers) {
    let pos = h.state().rect.min + vec2(p.0, p.1);
    h.event(if let Some(pressed) = pressed {
        Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: mods }
    } else {
        Event::PointerMoved(pos)
    });
    h.run_steps(1);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
}
fn click(h: &mut Harness<'_, Host>, p: (f32, f32), mods: Modifiers) {
    event(h, p, Some(true), mods);
    event(h, p, Some(false), mods);
}
fn attrs(h: &Host, index: usize) -> Sample {
    engine::sample(&h.session.active().unwrap().doc, h.ids[index], None).unwrap()
}
#[test]
fn styles38_ui_eyedropper_i_click_alt_shift_append_preview_release() {
    let mut h = harness();
    h.state_mut().tool.options.tool = Tool::StylePicker;
    h.event(Event::Key { key: Key::I, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
    assert_eq!(h.state().tool.options.tool, Tool::Eyedropper);
    h.event(Event::Key { key: Key::I, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(1); // context-bar layout follows the activated tool before locating the canvas
    let before = h.state().session.active().unwrap().doc.clone();
    event(&mut h, (25., 40.), Some(true), Modifiers::NONE);
    assert!(h.state().tool.preview.is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    event(&mut h, (25., 40.), Some(false), Modifiers::NONE);
    assert_eq!(attrs(h.state(), 0), attrs(h.state(), 1));
    assert_eq!(h.state().session.active().unwrap().history.past_len(), 1);
    click(&mut h, (165., 40.), Modifiers::ALT);
    assert_eq!(attrs(h.state(), 0), attrs(h.state(), 2));
    let stroke = attrs(h.state(), 1).appearance.unwrap().items[2].clone();
    click(&mut h, (230., 40.), Modifiers::SHIFT);
    let a = attrs(h.state(), 1).appearance.unwrap();
    assert_eq!(a.items.len(), 3);
    assert_eq!(a.items[2], stroke);
    let AppearanceItem::Fill(f) = &a.items[1] else { panic!() };
    let Fill::Solid(c) = f.paint else { panic!() };
    for (v, e) in c.to_rgb().into_iter().zip([0.2, 0.4, 0.8]) {
        assert!((v - e).abs() <= 1. / 255.);
    }
    click(&mut h, (25., 40.), Modifiers { alt: true, shift: true, ..Modifiers::NONE });
    assert_eq!(attrs(h.state(), 1).appearance.unwrap().items.len(), 6);
}
#[test]
fn styles38_ui_style_picker_chosen_fill_marquee_and_unload() {
    let mut h = harness();
    h.get_by_label("Style Picker").click();
    h.run_steps(1);
    h.get_by_label("None").click();
    h.run_steps(1);
    h.get_by_label("Fill").click();
    h.run_steps(1);
    click(&mut h, (25., 40.), Modifiers::NONE);
    assert!(h.state().tool.loaded.is_some());
    assert_eq!(attrs(h.state(), 1).appearance.unwrap().items.len(), 2);
    event(&mut h, (65., 10.), Some(true), Modifiers::NONE);
    event(&mut h, (205., 80.), None, Modifiers::NONE);
    assert!(h.state().tool.preview.is_some());
    event(&mut h, (205., 80.), Some(false), Modifiers::NONE);
    assert_eq!(attrs(h.state(), 2).appearance.unwrap().items.len(), 2);
    assert_eq!(attrs(h.state(), 2).opacity, 1.);
    h.get_by_label("Unload").click();
    h.run_steps(1);
    assert!(h.state().tool.loaded.is_none());
}
#[test]
fn styles38_ui_options_exclusions_escape_stale_selection_and_focus_cancel() {
    let mut h = harness();
    h.get_by_label("Eyedropper Options").click();
    h.run_steps(1);
    // Pick and Apply lists share labels: select the first (Pick) Stroke checkbox.
    h.get_all_by_label("Stroke").next().unwrap().click();
    h.run_steps(1);
    click(&mut h, (25., 40.), Modifiers::NONE);
    assert_eq!(attrs(h.state(), 1).appearance.unwrap().items.len(), 2);
    let before = h.state().session.active().unwrap().doc.clone();
    event(&mut h, (25., 40.), Some(true), Modifiers::NONE);
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
    event(&mut h, (25., 40.), Some(false), Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    event(&mut h, (25., 40.), Some(true), Modifiers::NONE);
    let id = h.state().ids[2];
    h.state_mut().session.select_layer(id).unwrap();
    event(&mut h, (25., 40.), Some(false), Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    event(&mut h, (25., 40.), Some(true), Modifiers::NONE);
    h.event(Event::WindowFocused(false));
    h.run_steps(1);
    assert!(!h.state().tool.captured());
    assert_eq!(serde_json::from_str::<Options>("{}").unwrap(), Options::default());
}
#[test]
#[ignore = "owner speed rules: real GPU and offscreen release gate runs at integration"]
fn styles38_gpu_ui_rtx5090_offscreen_pointer_preview_release() {
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = if cfg!(target_os = "macos") { eframe::wgpu::Backends::METAL } else { eframe::wgpu::Backends::VULKAN };
    setup.native_adapter_selector = Some(Arc::new(|adapters, _| {
        adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                if cfg!(target_os = "macos") {
                    i.backend == eframe::wgpu::Backend::Metal && i.device_type != eframe::wgpu::DeviceType::Cpu
                } else {
                    i.backend == eframe::wgpu::Backend::Vulkan && i.device_type == eframe::wgpu::DeviceType::DiscreteGpu && i.name.contains("5090")
                }
            })
            .cloned()
            .ok_or_else(|| (if cfg!(target_os = "macos") { "hardware Metal non-CPU adapter required" } else { "RTX 5090 Vulkan required" }).into())
    }));
    let rs = egui_kittest::wgpu::create_render_state(eframe::egui_wgpu::WgpuSetup::CreateNew(setup), eframe::egui_wgpu::RendererOptions::PREDICTABLE);
    let info = rs.adapter.get_info();
    assert_eq!(info.backend, if cfg!(target_os = "macos") { eframe::wgpu::Backend::Metal } else { eframe::wgpu::Backend::Vulkan });
    #[cfg(not(target_os = "macos"))]
    assert_eq!(info.device_type, eframe::wgpu::DeviceType::DiscreteGpu);
    #[cfg(target_os = "macos")]
    assert_ne!(info.device_type, eframe::wgpu::DeviceType::Cpu, "hardware Metal adapter required");
    #[cfg(not(target_os = "macos"))]
    assert!(info.name.contains("5090"));
    assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&rs.adapter), eframe::wgpu::TextureFormat::Rgba32Float);
    eprintln!("styles38 UI {info:?}");
    let compositor = photocraft_gpu::Compositor::try_new_with_format(&rs.device, eframe::wgpu::TextureFormat::Rgba32Float).unwrap();
    let mut host = host();
    host.gpu = Some(Gpu { device: rs.device.clone(), queue: rs.queue.clone(), compositor, previews: 0, releases: 0 });
    let renderer = egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs);
    let mut h = Harness::builder().with_size(vec2(1000., 330.)).renderer(renderer).build_ui_state(frame, host);
    event(&mut h, (25., 40.), Some(true), Modifiers::NONE);
    assert!(h.state().tool.preview.is_some());
    h.render().unwrap();
    event(&mut h, (25., 40.), Some(false), Modifiers::NONE);
    h.render().unwrap();
    assert_eq!(attrs(h.state(), 0), attrs(h.state(), 1));
    let gpu = h.state().gpu.as_ref().unwrap();
    assert!(gpu.previews > 0 && gpu.releases > 0);
}
