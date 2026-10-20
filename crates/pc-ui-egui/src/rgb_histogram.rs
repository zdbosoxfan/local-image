//! Paints the editor's RGB histograms (`li_raster::histogram::RgbHistogram`) with the shared `li_ui_kit` painter.
use crate::theme::Tokens;
use egui::Rect;
use li_raster::histogram::RgbHistogram;

pub(crate) fn paint(painter: &egui::Painter, rect: Rect, h: &RgbHistogram, t: &Tokens) {
    li_ui_kit::histogram::paint_rgb(painter, rect, li_ui_kit::histogram::Histogram::u64(&h.channels, Some(&h.luma)), t);
}

pub(crate) fn paint_sample(painter: &egui::Painter, rect: Rect, rgb: [f32; 3], t: &Tokens) {
    li_ui_kit::histogram::paint_sample(painter, rect, rgb, t);
}

#[cfg(test)]
#[path = "../../li-ui-kit/tests/reference/histogram_editor.rs"]
mod previous_editor;
#[cfg(test)]
#[path = "../../li-ui-kit/tests/reference/histogram_library.rs"]
mod previous_library;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::theme::ThemeKind;
    use egui_kittest::{Harness, kittest::Queryable};

    /// The shared CPU kittest renderer (`li-testkit`): no GPU or window.
    fn cpu_renderer() -> li_testkit::SoftpaintRenderer {
        li_testkit::SoftpaintRenderer::with_clear_from(|ctx: &egui::Context| crate::theme::Tokens::get(ctx).canvas)
    }

    #[test]
    fn histogram_widget_screens_and_clipping_keyboard_parity() {
        let channels: [[u64; 256]; 3] = std::array::from_fn(|c| {
            std::array::from_fn(|i| {
                let center = [85.0, 130.0, 180.0][c];
                let height = (-(i as f32 - center).powi(2) / 1500.0).exp() * 1000.0;
                height as u64 + if i == 0 || i == 255 { 400 } else { 0 }
            })
        });
        let h = RgbHistogram { channels, luma: channels[1], samples: 100_000, ..Default::default() };
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/design-system/histogram");
        std::fs::create_dir_all(&dir).unwrap();
        for kind in [ThemeKind::ProMedium, ThemeKind::StudioLight] {
            for width in [250.0, 400.0] {
                for ppp in [1.0, 1.5] {
                    let histogram = h.clone();
                    let mut harness =
                        Harness::builder().with_size(egui::vec2(width, 300.0)).with_pixels_per_point(ppp).renderer(cpu_renderer()).build_ui_state(
                            move |ui, clipping: &mut bool| {
                                crate::theme::apply(ui.ctx(), kind);
                                li_ui_kit::library::apply_tokens(ui.ctx(), li_ui_kit::library::for_kind(kind));
                                crate::widgets::rgb_histogram(ui, &histogram, 120.0);
                                let view = li_ui_kit::histogram::Histogram::u64(&histogram.channels, Some(&histogram.luma));
                                let response = li_ui_kit::histogram::library(ui, Some(view), (0.01, 0.0), *clipping, "Toggle histogram clipping");
                                if response.clicked() {
                                    *clipping = !*clipping;
                                }
                                assert!(response.rect.width() <= width);
                            },
                            false,
                        );
                    harness.run_steps(3);
                    harness.get_by_label("Toggle histogram clipping").click();
                    harness.run();
                    assert!(*harness.state(), "histogram click toggles clipping");
                    harness.get_by_label("Toggle histogram clipping").focus();
                    harness.run_steps(1);
                    harness.key_press(egui::Key::Space);
                    harness.run();
                    assert!(!*harness.state(), "focused histogram Space toggles clipping");
                    harness.render().unwrap().save(dir.join(format!("{kind:?}-{}-{ppp}.png", width as u32))).unwrap();
                }
            }
        }
    }
    #[test]
    fn histogram_cpu_pixels_match_frozen_profiles() {
        let channels: [[u32; 256]; 3] = std::array::from_fn(|c| std::array::from_fn(|i| ((i * (c + 1) * 73 + c * 37) % 101) as u32));
        let wide = channels.map(|c| c.map(u64::from));
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/design-system/histogram");
        std::fs::create_dir_all(&dir).unwrap();
        let mut checked_pixels = 0usize;
        for kind in [ThemeKind::ProMedium, ThemeKind::StudioLight] {
            for width in [250.0, 400.0] {
                for ppp in [1.0, 1.5] {
                    for library in [false, true] {
                        let mut harness =
                            Harness::builder().with_size(egui::vec2(width, 140.0)).with_pixels_per_point(ppp).renderer(cpu_renderer()).build_ui_state(
                                move |ui, baseline: &mut bool| {
                                    crate::theme::apply(ui.ctx(), kind);
                                    let t = Tokens::get(ui.ctx());
                                    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 120.0), egui::Sense::hover());
                                    ui.painter().rect_filled(rect, t.radius_sm, t.histogram_background());
                                    let plot = rect.shrink(li_ui_kit::histogram::EDITOR_INSET);
                                    let view = li_ui_kit::histogram::Histogram::u64(&wide, Some(&wide[1]));
                                    match (library, *baseline) {
                                        (false, false) => li_ui_kit::histogram::paint_rgb(ui.painter(), plot, view, &t),
                                        (false, true) => previous_editor::paint(ui.painter(), plot, &wide, &t),
                                        (true, false) => li_ui_kit::histogram::paint_library(ui.painter(), plot, view, &t),
                                        (true, true) => previous_library::paint(ui.painter(), plot, &channels, &channels[1]),
                                    }
                                },
                                true,
                            );
                        harness.run_steps(2);
                        let old = harness.render().unwrap();
                        *harness.state_mut() = false;
                        harness.run_steps(1);
                        let new = harness.render().unwrap();
                        assert_eq!(old, new, "frozen pixel parity {kind:?}/{width}/{ppp}/library={library}");
                        checked_pixels += new.width() as usize * new.height() as usize;
                        let profile = if library { "library" } else { "editor" };
                        for (phase, image) in [("before", old), ("after", new)] {
                            image.save(dir.join(format!("{phase}-{profile}-{kind:?}-{}-{ppp}.png", width as u32))).unwrap();
                        }
                    }
                }
            }
        }
        eprintln!("histogram frozen parity: {checked_pixels} RGBA pixels checked; differing pixels=0; maximum channel error=0");
    }
}
