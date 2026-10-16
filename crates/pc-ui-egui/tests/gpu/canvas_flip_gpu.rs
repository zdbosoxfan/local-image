//! View › Flip Horizontal mirrors the document in the GPU canvas shader (it used to send the view
//! through the CPU compositor and egui textures): the flipped canvas is the unflipped one
//! mirrored about the view centre, and it is still drawn by the GPU path. Skips without a GPU.

use egui_kittest::kittest::Queryable;
use photocraft_ui_egui::PhotocraftApp;
use serde_json::json;

type Harness = egui_kittest::Harness<'static, PhotocraftApp>;

fn harness() -> Option<Harness> {
    let built = std::panic::catch_unwind(|| {
        egui_kittest::Harness::builder().with_size(egui::vec2(900.0, 600.0)).with_pixels_per_point(1.0).with_max_steps(64).wgpu().build_eframe(|cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
            if let Some(rs) = cc.wgpu_render_state.as_ref() {
                app.set_wgpu(rs.clone());
            }
            app
        })
    });
    match built {
        Ok(h) if h.state().gpu_active() => Some(h),
        _ => {
            eprintln!("skipping: no GPU adapter");
            None
        }
    }
}

/// The canvas pixels (RGB) inside its rect, row by row, and the rect's width.
fn canvas(h: &mut Harness) -> (Vec<[u8; 3]>, usize) {
    h.run_steps(6);
    let r = h.state().last_canvas_rect.shrink(8.0);
    let img = h.render().expect("render");
    let (x0, x1) = (r.left().ceil() as u32, (r.right().floor() as u32).min(img.width()));
    let mut px = Vec::new();
    for y in r.top().ceil() as u32..(r.bottom().floor() as u32).min(img.height()) {
        for x in x0..x1 {
            let p = img.get_pixel(x, y);
            px.push([p[0], p[1], p[2]]);
        }
    }
    (px, (x1 - x0) as usize)
}

#[test]
fn flipped_view_is_mirrored_on_the_gpu() {
    let _gpu = crate::gpu_lock();
    let Some(mut h) = harness() else { return };
    {
        let app = h.state_mut();
        app.run("file.new", json!({"width": 400, "height": 300, "background": "white"})).expect("new");
        app.run("select.rect", json!({"x": 0, "y": 0, "width": 120, "height": 300})).expect("select");
        app.run("edit.fill", json!({"color": "#c02010"})).expect("fill red");
        app.run("select.rect", json!({"x": 300, "y": 40, "width": 100, "height": 100})).expect("select");
        app.run("edit.fill", json!({"color": "#1040d0"})).expect("fill blue");
        app.run("select.deselect", json!({})).expect("deselect");
        app.sync_views();
    }
    // Zoom 1, document centred: the mirror axis is the canvas centre.
    let ctx = h.ctx.clone();
    let (req, _) = photocraft_ui_egui::control::ControlRequest::new("ui.set", json!({"zoom": 1.0, "center": [200, 150]}));
    photocraft_ui_egui::control::handle(h.state_mut(), &ctx, &req);
    let (plain, w) = canvas(&mut h);
    assert!(h.state().perf.last_refresh.starts_with("gpu"), "unflipped refresh {}", h.state().perf.last_refresh);
    h.get_by_label("View").click();
    h.run_steps(3);
    h.get_by_label_contains("Flip Horizontal").click();
    h.run_steps(3);
    assert!(h.state().ui.view.flip_horizontal);
    let (flipped, w2) = canvas(&mut h);
    assert_eq!(w, w2);
    assert!(h.state().gpu_active() && h.state().perf.gpu, "flipped view left the GPU canvas");
    // Mirrored rows inside the document (400×300 at zoom 1 around the canvas centre; the
    // pasteboard pattern isn't symmetric). The mirror axis is the view's centre, which may sit a
    // little off the sampled rect's (rulers): search for it.
    let rows = plain.len() / w;
    let (cx, cy) = (w / 2, rows / 2);
    let mut best = u64::MAX;
    for shift in (-48i64..=48).step_by(1) {
        let mut bad = 0u64;
        for y in (cy - 140..cy + 140).step_by(2) {
            for x in cx - 150..cx + 150 {
                let mx = (w as i64 - 1 - x as i64 + shift) as usize;
                let (a, b) = (plain[y * w + x], flipped[y * w + mx]);
                if (0..3).any(|i| a[i].abs_diff(b[i]) > 3) {
                    bad += 1;
                }
            }
        }
        best = best.min(bad);
    }
    assert!(best < 300, "{best} pixels differ from the mirrored canvas");
    // The red band is on the right now.
    let row = &flipped[cy * w..(cy + 1) * w];
    let red = |p: &[u8; 3]| p[0] > 150 && p[1] < 80 && p[2] < 80;
    let first_red = row.iter().position(red).expect("red visible");
    assert!(first_red > cx, "red band starts at {first_red} of {w}");
}
