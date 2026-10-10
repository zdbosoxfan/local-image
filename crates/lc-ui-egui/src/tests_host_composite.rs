//! local-image: a photo being edited in Compositing (unsaved) shows its composite in the
//! Library's loupe and grid, marked, with a button back to the document; Develop keeps showing
//! the photo itself.

use std::time::Duration;

use lightcraft_catalog::PhotoId;
use serde_json::json;

use crate::headless::Headless;
use crate::panels::host_composite;
use crate::state::{BeforeAfter, RightPanel, ViewMode};
use crate::{LightcraftApp, Services};

const T: Duration = Duration::from_secs(60);

fn show(h: &mut Headless, id: PhotoId, view: ViewMode, right: RightPanel) {
    let _ = h.app.run("library.select", json!({"ids": [id.0], "active": id.0}));
    h.app.ui.view = view;
    h.app.ui.right = right;
    h.settle(T);
    h.step();
}

#[test]
fn the_library_shows_the_unsaved_composite_of_a_photo_in_compositing() {
    let dir = std::env::temp_dir().join(format!("lc-ui-host-composite-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("photo.png");
    let img = lightcraft_raster::Rgba8 { width: 40, height: 30, data: (0..40 * 30).map(|i| [(i % 40 * 6) as u8, 120, 60, 255]).collect() };
    let png = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&img), &lightcraft_codecs::EncodeMeta::default()).unwrap();
    std::fs::write(&path, png).unwrap();
    let mut s = lightcraft_engine::Session::new().with_fs();
    s.execute("library.import", &json!({"paths": [path.to_string_lossy()]})).unwrap();
    let id = s.catalog.photos().next().map(|p| p.id).unwrap();
    let mut h = Headless::new(LightcraftApp::new(s, Services::default()), [1200.0, 900.0], 1.0);

    // nothing from Compositing: the loupe shows the photo's own render
    show(&mut h, id, ViewMode::Detail, RightPanel::None);
    assert_ne!(h.app.loupe_shown.map(|l| l.1), Some("compositing"));

    // the host hands over the document's composite: the Library loupe shows it, marked
    let ctx = h.view.ctx.clone();
    host_composite::set(&mut h.app, &ctx, id, 7, 2, 2, &[255, 0, 0, 255].repeat(4), "Edited in Compositing · unsaved");
    assert_eq!(host_composite::key(&h.app, id), Some(7));
    show(&mut h, id, ViewMode::Detail, RightPanel::None);
    assert_eq!(h.app.loupe_shown, Some((id, "compositing")));
    assert!(h.app.widgets.iter().any(|(w, _)| w == "hostComposite:open"), "the way back to Compositing");
    let pixel = |h: &Headless, p: egui::Pos2| {
        let img = h.paint();
        img.pixels[p.y as usize * img.size[0] + p.x as usize]
    };
    // Verify what is painted, including every remembered Develop before/after layout.
    for before_after in [BeforeAfter::Original, BeforeAfter::SideBySide, BeforeAfter::TopBottom, BeforeAfter::Split, BeforeAfter::SplitTopBottom] {
        h.app.ui.before_after = before_after;
        h.step();
        assert_eq!(h.app.loupe_shown, Some((id, "compositing")), "{before_after:?}");
        assert_eq!(pixel(&h, h.app.canvas_rect.unwrap().center()), egui::Color32::RED, "{before_after:?}");
    }
    h.app.ui.before_after = BeforeAfter::Off;

    // its button asks the host for the document
    assert_eq!(h.request("ui.clickWidget", json!({"id": "hostComposite:open"}), T)["ok"], true);
    for _ in 0..4 {
        h.step();
    }
    assert_eq!(h.app.host_composite_open, Some(id));

    // Develop edits the photo itself: no stand-in there
    show(&mut h, id, ViewMode::Detail, RightPanel::Edit);
    assert_ne!(h.app.loupe_shown.map(|l| l.1), Some("compositing"));

    // the grid draws it too (and doesn't need the photo's thumbnail for it)
    show(&mut h, id, ViewMode::PhotoGrid, RightPanel::None);
    let thumb = h.app.widgets.iter().find(|(w, _)| w == &format!("thumb:{}", id.0)).unwrap().1;
    assert_eq!(pixel(&h, thumb.center()), egui::Color32::RED);

    // saved or closed in Compositing: the host drops it and the photo shows as itself again
    host_composite::retain(&mut h.app, &[]);
    show(&mut h, id, ViewMode::Detail, RightPanel::None);
    assert_ne!(h.app.loupe_shown.map(|l| l.1), Some("compositing"));
    let _ = std::fs::remove_dir_all(&dir);
}
