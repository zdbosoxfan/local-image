//! `files::save_bytes` is the capability-scoped encoder: a linked path inside the document must
//! not be read through ambient filesystem authority and embedded in the returned bytes.

use photocraft_automation::files;
use photocraft_doc::{Affine, ColorMode, Document, Layer, LayerContent, SampleType, Size, SmartObject, SmartSource, Surface};

#[test]
fn a_linked_smart_object_is_not_read_into_the_bytes() {
    let mut doc = Document::new("linked", Size::new(8, 6), ColorMode::Rgb, SampleType::U8);
    let linked = std::env::current_exe().unwrap();
    let source = SmartSource::Linked { path: linked.to_string_lossy().into_owned() };
    let smart = SmartObject::new(source, Affine::IDENTITY, Some(Surface::new(doc.pixel_format())));
    doc.layers.push(Layer::new("Linked", LayerContent::Smart(smart)));
    let (bytes, warnings) = files::save_bytes(&doc, "out.psd", None, &Default::default()).unwrap();
    assert!(warnings.iter().any(|w| w.contains("not read for this export")), "{warnings:?}");
    assert!(bytes.len() < 1 << 20, "the test executable was not embedded");
}
