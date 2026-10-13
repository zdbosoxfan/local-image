//! Multichannel PSDs: the document's resources and global blocks survive an import -> export ->
//! import round trip (upstream 6547ca0b, without its alpha-channel half).

use photocraft_doc::{Knot, NamedPath, Path, PathOp, Subpath};
use photocraft_io::*;
use photocraft_psd::resources::ImageResource;
use photocraft_psd::{ColorMode, Compression, PsdFile, TaggedBlock, Version, testgen};

/// DisplayInfo (1077) record: RGB colour, opacity (0..100) and kind (0/1 alpha, 2 spot).
fn display_record(out: &mut Vec<u8>, rgb: [u16; 3], opacity: u16, kind: u8) {
    out.extend_from_slice(&0u16.to_be_bytes());
    for v in [rgb[0], rgb[1], rgb[2], 0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out.extend_from_slice(&opacity.to_be_bytes());
    out.push(kind);
}

/// A two-channel Multichannel file: a cyan spot ink and an alpha channel (masked areas, red 50 %).
fn file(depth: u16) -> PsdFile {
    let mut f = testgen::merged_only(Version::Psd, ColorMode::Multichannel, depth, Compression::Rle, 9, 5);
    let mut info = 1u32.to_be_bytes().to_vec();
    display_record(&mut info, [0, 65535, 65535], 0, 2);
    display_record(&mut info, [65535, 0, 0], 50, 1);
    f.resources.push(ImageResource::new(1077, info));
    f
}

fn roundtrip(d: &photocraft_doc::Document) -> (PsdFile, photocraft_doc::Document) {
    let out = export(d, "x.psd", &ExportOptions::default()).unwrap();
    let back = PsdFile::from_bytes(&out.bytes).unwrap();
    let (doc, _) = psd_to_document(&back);
    (back, doc)
}

#[test]
fn resources_and_global_blocks_survive_a_save() {
    let mut f = file(8);
    let iptc = vec![0x1c, 2, 0, 0, 2, 0, 4];
    let print_flags = vec![0, 0, 0, 0, 0, 0, 0, 1, 0];
    let seed = 7u32.to_be_bytes().to_vec();
    let alpha_ids = [0u32, 1].iter().flat_map(|v| v.to_be_bytes()).collect::<Vec<u8>>();
    f.resources.push(ImageResource::new(1028, iptc.clone()));
    f.resources.push(ImageResource::new(1011, print_flags.clone()));
    f.resources.push(ImageResource::new(1044, seed.clone()));
    f.resources.push(ImageResource::new(1053, alpha_ids.clone()));
    // A target-layer index describes layers, which a Multichannel document doesn't have.
    f.resources.push(ImageResource::new(1024, vec![0, 0]));
    let fmsk = vec![0, 0, 0, 0, 0, 50];
    f.global_blocks.push(TaggedBlock::new(*b"FMsk", fmsk.clone()));
    let (mut d, _) = psd_to_document(&f);
    d.guides.horizontal.push(2.0);
    d.guides.vertical.push(3.0);
    d.global_light.angle = 60.0;
    let path =
        Path::new(vec![Subpath { closed: true, op: PathOp::Combine, knots: vec![Knot::corner(1.0, 1.0), Knot::corner(8.0, 1.0), Knot::corner(4.0, 4.0)] }]);
    d.paths.push(NamedPath { name: "Outline".into(), path, psd_raw: None });

    let (back, d2) = roundtrip(&d);
    let res = |id: u16| back.resource(id).map(|r| r.data.clone());
    assert_eq!(res(1028), Some(iptc));
    assert_eq!(res(1011), Some(print_flags));
    assert_eq!(res(1044), Some(seed));
    assert_eq!(res(1053), Some(alpha_ids));
    assert_eq!(res(1024), None);
    let blocks: Vec<_> = back.global_blocks.iter().filter(|b| &b.key == b"FMsk").collect();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].data, fmsk);
    assert_eq!(d2.guides.horizontal, vec![2.0]);
    assert_eq!(d2.guides.vertical, vec![3.0]);
    assert_eq!(d2.global_light.angle, 60.0);
    // The file's own saved path and the new one.
    let names: Vec<&str> = d2.paths.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Path 1", "Outline"]);
    let anchors = |p: &Path| p.subpaths.iter().flat_map(|s| s.knots.iter().map(|k| (k.anchor.x, k.anchor.y))).collect::<Vec<_>>();
    let (got, want) = (anchors(&d2.paths[1].path), anchors(&d.paths[1].path));
    assert_eq!(got.len(), want.len());
    assert!(got.iter().zip(&want).all(|(a, b)| (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3), "{got:?}");
    assert_eq!(back.decode_merged().unwrap(), f.decode_merged().unwrap());

    // Alpha identifiers that no longer match the channels are not written.
    d.channels.pop();
    let (back, _) = roundtrip(&d);
    assert!(back.resource(1053).is_none());
}
