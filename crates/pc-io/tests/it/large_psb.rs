//! PSD/PSB required-pixel admission: malformed channels fail before a document can be saved.

use photocraft_io::psd_to_document;
use photocraft_psd::testgen;
use photocraft_psd::{ColorMode, Compression, LayerSpec, PixelData, PsdBuilder, Version};

/// A damaged required channel must not produce a writable partial document.
#[test]
fn undecodable_channel_refuses_the_document() {
    for version in [Version::Psd, Version::Psb] {
        let mut b = PsdBuilder::new(4, 4).version(version).compression(Compression::Rle);
        b.push_layer(LayerSpec::new("Damaged", 0, 0, 4, 4, PixelData::Rgba8(vec![200; 64])));
        let mut file = b.build().unwrap();
        let red = file.layers_mut()[0].channels.iter_mut().find(|c| c.id == 0).unwrap();
        red.data.truncate(1);

        let error = psd_to_document(&file).unwrap_err().to_string();
        assert!(error.contains("Damaged") && error.contains("could not be decoded"), "{error}");
        assert!(photocraft_io::import("damaged.psd", &file.to_bytes().unwrap()).is_err());
    }
}

/// A flattened 29000² RGB PSB: its merged image decodes to 2.52 GB, past the old 2 GiB cap, which
/// opened it as a document with no layers. Needs about 12 GB of memory, so it is opt-in:
/// `cargo test --release -p photocraft-io --test large_psb -- --ignored`.
#[test]
#[ignore = "allocates ~12 GB; run in release"]
fn flattened_psb_with_a_merged_image_over_2_gib_opens() {
    const SIDE: u32 = 29_000;
    let file = testgen::merged_only(Version::Psb, ColorMode::Rgb, 8, Compression::Rle, SIDE, SIDE);
    assert!(!file.image_data.data.is_empty(), "the generator could not encode the merged image");

    let (doc, warnings) = psd_to_document(&file).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(doc.layers.len(), 1);
    let s = doc.layers[0].surface().unwrap();
    // testgen::pattern_plane at 8 bits, channel c seeded with c.
    let want = |x: usize, y: usize, c: usize| if (x / 4 + y).is_multiple_of(3) { (c * 17) as u8 } else { (x * 3 + y * 7 + c) as u8 };
    let side = SIDE as usize;
    for (x, y) in [(0, 0), (side - 1, 0), (side / 2, side / 2), (side - 2, side - 1)] {
        let got = s.pixel(x as i32, y as i32);
        for (c, v) in got.iter().take(3).enumerate() {
            assert_eq!((v * 255.0).round() as u8, want(x, y, c), "({x}, {y}) channel {c}");
        }
        assert_eq!(got[3], 1.0);
    }
}
