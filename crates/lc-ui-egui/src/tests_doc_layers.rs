//! local-image #31: a layered document's Layers list in Develop and Library (read-only), with
//! Develop layers linking to the photo they follow and "Open in Compositing" handing the file to
//! the host's editor.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use lightcraft_catalog::PhotoId;
use serde_json::json;

use crate::headless::Headless;
use crate::panels::doc_layers::{DocLayer, DocLayers, LayerKind};
use crate::state::{RightPanel, ViewMode};
use crate::{LightcraftApp, Services};

const T: Duration = Duration::from_secs(60);

/// A flat 2 × 2 RGB PSD (no layer records) the Library can import.
fn psd(rgb: [u8; 3]) -> Vec<u8> {
    let mut b = b"8BPS".to_vec();
    b.extend_from_slice(&1u16.to_be_bytes());
    b.extend_from_slice(&[0; 6]);
    b.extend_from_slice(&3u16.to_be_bytes()); // channels
    b.extend_from_slice(&2u32.to_be_bytes()); // height
    b.extend_from_slice(&2u32.to_be_bytes()); // width
    b.extend_from_slice(&8u16.to_be_bytes()); // depth
    b.extend_from_slice(&3u16.to_be_bytes()); // RGB
    for _ in 0..3 {
        b.extend_from_slice(&0u32.to_be_bytes()); // colour mode data, resources, layers
    }
    b.extend_from_slice(&0u16.to_be_bytes()); // raw image data
    for c in rgb {
        b.extend_from_slice(&[c; 4]);
    }
    b
}

fn layer(name: &str, kind: LayerKind, follows: Option<u64>) -> DocLayer {
    let thumb = Some(lightcraft_raster::Rgba8 { width: 4, height: 3, data: vec![[200, 120, 40, 255]; 12] });
    DocLayer { name: name.into(), depth: 0, visible: true, opacity: 1.0, blend: "Normal".into(), kind, follows, thumb }
}

fn has(h: &Headless, id: &str) -> bool {
    h.app.widgets.iter().any(|(w, _)| w == id)
}

fn click(h: &mut Headless, id: &str) {
    h.step();
    h.step();
    assert_eq!(h.request("ui.clickWidget", json!({"id": id}), T)["ok"], true, "{id}");
    for _ in 0..4 {
        h.step();
    }
}

fn show(h: &mut Headless, id: PhotoId, view: ViewMode, right: RightPanel) {
    let _ = h.app.run("library.select", json!({"ids": [id.0], "active": id.0}));
    h.app.ui.view = view;
    h.app.ui.right = right;
    h.settle(T);
    h.step();
}

#[test]
fn layered_documents_list_their_layers_and_open_in_compositing() {
    let dir = std::env::temp_dir().join(format!("lc-ui-doclayers-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("layered.psd"), psd([200, 40, 40])).unwrap();
    std::fs::write(dir.join("flat.psd"), psd([40, 40, 200])).unwrap();
    let mut s = lightcraft_engine::Session::new().with_fs();
    s.execute("library.import", &json!({"paths": [dir.to_string_lossy()]})).unwrap();
    let id = |name: &str| s.catalog.photos().find(|p| p.file_name == name).map(|p| p.id).unwrap();
    let (layered, flat) = (id("layered.psd"), id("flat.psd"));

    let reads = Arc::new(Mutex::new(Vec::<String>::new()));
    let opened = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let (r, o) = (reads.clone(), opened.clone());
    let services = Services {
        png: None,
        doc_layers: Some(Arc::new(move |path: &str| {
            r.lock().unwrap().push(path.to_string());
            let layers = if path.ends_with("layered.psd") {
                let mut retouch = layer("Retouch", LayerKind::Pixel, None);
                retouch.blend = "Multiply".into();
                retouch.opacity = 0.5;
                vec![retouch, layer("Develop", LayerKind::Develop, Some(flat.0))]
            } else {
                vec![layer("Background", LayerKind::Pixel, None)]
            };
            Ok(DocLayers { width: 2, height: 2, layers, more: 0 })
        })),
        open_with: Some(Box::new(move |path: &str, app: &str| {
            o.lock().unwrap().push((path.to_string(), app.to_string()));
            Ok(())
        })),
        ..Default::default()
    };
    let mut h = Headless::new(LightcraftApp::new(s, services), [1400.0, 1000.0], 1.0);

    // Develop on the layered document: the Layers section, opened, lists both layers
    show(&mut h, layered, ViewMode::Detail, RightPanel::Edit);
    assert!(has(&h, "section:docLayers"), "Layers section in Develop");
    click(&mut h, "section:docLayers");
    for w in ["docLayers:row:0", "docLayers:row:1", "docLayers:follow:1", "button:openInCompositing"] {
        assert!(has(&h, w), "{w}");
    }
    assert!(!has(&h, "docLayers:follow:0"), "only Develop layers follow a photo");
    // read once per file version, not every frame
    let n = reads.lock().unwrap().len();
    h.step();
    h.step();
    assert_eq!(reads.lock().unwrap().len(), n);

    // Open in Compositing: the host's own editor (no app named); the Library reloads it later
    click(&mut h, "button:openInCompositing");
    let path = dir.join("layered.psd").to_string_lossy().to_string();
    assert_eq!(opened.lock().unwrap().as_slice(), &[(path, String::new())]);
    assert!(h.app.ui.external_edits.contains(&layered.0));

    // the Develop layer's photo: one click goes to it
    click(&mut h, "docLayers:follow:1");
    assert_eq!(h.app.session.active(), Some(flat));

    // a single-layer document has no list
    show(&mut h, flat, ViewMode::Detail, RightPanel::Edit);
    assert!(!has(&h, "section:docLayers"));

    // the Library's Info panel shows the list too
    show(&mut h, layered, ViewMode::PhotoGrid, RightPanel::Info);
    assert!(has(&h, "docLayers:row:1"), "Layers in Library › Info");

    // no reader (e.g. the web build): nothing
    h.app.services.doc_layers = None;
    show(&mut h, layered, ViewMode::Detail, RightPanel::Edit);
    assert!(!has(&h, "section:docLayers"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn only_documents_that_can_have_layers_are_asked_about() {
    use crate::panels::doc_layers::layered_candidate;
    for p in ["/a/b.psd", "/a/b.PSB", "c.tif", "d.TIFF", "e.pcraft"] {
        assert!(layered_candidate(p), "{p}");
    }
    for p in ["/a/b.jpg", "raw.dng", "noext", "psd"] {
        assert!(!layered_candidate(p), "{p}");
    }
    let one = DocLayers { layers: vec![layer("Background", LayerKind::Pixel, None)], ..Default::default() };
    assert!(!one.is_layered());
    let develop = DocLayers { layers: vec![layer("Develop", LayerKind::Develop, Some(1))], ..Default::default() };
    assert!(develop.is_layered(), "a lone Develop layer is worth showing");
}
