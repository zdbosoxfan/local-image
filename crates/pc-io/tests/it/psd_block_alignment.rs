//! Every layer-level tagged block of an exported PSD/PSB is a multiple of 4 bytes long.
//!
//! Photoshop pads layer-level blocks to 4 inside the length field; all of its own files do (567
//! fixtures checked), and a regenerated block that stopped at an even but unaligned size left the
//! layer record's extra data unaligned: a Pattern Fill's `PtFl` came out 2 bytes short of the
//! boundary for half of all pattern names.

use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_doc::{Document, Fill, Layer, LayerContent, Pattern};
use photocraft_geom::{Rect, Size};
use photocraft_io::*;
use photocraft_psd::PsdFile;
use photocraft_raster::Surface;

fn pattern(name: &str) -> Pattern {
    let mut s = Surface::new(PixelFormat::RGBA8);
    s.fill_rect(Rect::new(0, 0, 8, 8), &[0.8, 0.8, 0.8, 1.0]);
    s.fill_rect(Rect::new(0, 0, 4, 4), &[0.2, 0.4, 0.9, 1.0]);
    Pattern { id: "059289fb-0000-4000-8000-000000000001".into(), name: name.into(), width: 8, height: 8, surface: s }
}

/// One Pattern Fill layer per name; the name's length moves the descriptor's size by two bytes
/// per character, so consecutive lengths cover every residue modulo 4.
fn doc(depth: SampleType, names: &[&str]) -> Document {
    let mut d = Document::new("p", Size::new(40, 24), ColorMode::Rgb, depth);
    for name in names {
        let p = pattern(name);
        let fill = Fill::Pattern { name: (*name).into(), scale: 1.0, id: p.id.clone(), angle: 0.0, link: true, phase: (0.0, 0.0) };
        d.patterns.push(p);
        d.layers.push(Layer::new(format!("Fill {name}"), LayerContent::Fill(fill)));
    }
    d
}

#[test]
fn layer_blocks_are_four_byte_aligned() {
    let names = ["A", "Ab", "Abc", "Abcd", "Dots", "Checker", "Wood grain"];
    for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
        for path in ["x.psd", "x.psb"] {
            let bytes = export(&doc(depth, &names), path, &ExportOptions::default()).expect("export").bytes;
            let file = PsdFile::from_bytes(&bytes).expect("reparse");
            let mut fills = 0;
            for layer in file.layers() {
                for b in &layer.blocks {
                    let key = String::from_utf8_lossy(&b.key).into_owned();
                    assert_eq!(b.data.len() % 4, 0, "{depth:?} {path}: layer {:?} block {key} is {} bytes", layer.name(), b.data.len());
                }
                fills += usize::from(layer.block(b"PtFl").is_some());
            }
            assert_eq!(fills, names.len(), "{depth:?} {path}: every pattern fill keeps its PtFl block");
        }
    }
}

fn be(b: &[u8], at: usize, long: bool) -> usize {
    if long { u64::from_be_bytes(b[at..at + 8].try_into().unwrap()) as usize } else { u32::from_be_bytes(b[at..at + 4].try_into().unwrap()) as usize }
}

/// Walks the raw file to the first global block and returns (key, declared length, the bytes
/// after the data up to the next block or the end of the section).
fn first_global_block(bytes: &[u8]) -> ([u8; 4], usize, Vec<u8>) {
    let psb = be(bytes, 4, false) >> 16 == 2;
    let mut p = 26;
    p += 4 + be(bytes, p, false); // color mode data
    p += 4 + be(bytes, p, false); // image resources
    let lw = if psb { 8 } else { 4 };
    let end = p + lw + be(bytes, p, psb);
    p += lw;
    p += lw + be(bytes, p, psb); // classic layer info
    p += 4 + be(bytes, p, false); // global layer mask
    let key: [u8; 4] = bytes[p + 4..p + 8].try_into().unwrap();
    let len = be(bytes, p + 8, psb);
    let data_end = p + 8 + lw + len;
    let next = (data_end..end).find(|&q| bytes.get(q..q + 4).is_some_and(|s| s == b"8BIM" || s == b"8B64")).unwrap_or(end);
    (key, len, bytes[data_end..next].to_vec())
}

/// The global `Lr16`/`Lr32` block of a 16/32-bit export declares the exact length of the layer
/// data, and its padding to 4 follows outside that length, as Photoshop writes it. Readers that
/// check the length exactly (Unity's PSD Importer) reject a length that counts the padding.
#[test]
fn lr16_lr32_length_excludes_padding() {
    use crate::common::{self, Features};
    let mut unaligned = 0;
    for (depth, key) in [(SampleType::U16, *b"Lr16"), (SampleType::F32, *b"Lr32")] {
        for mode in [ColorMode::Rgb, ColorMode::Grayscale] {
            for features in [Features::PIXELS, Features::ALL] {
                let doc = common::gen_doc(mode, depth, features);
                for path in ["x.psd", "x.psb"] {
                    let name = format!("{depth:?} {mode:?} {path}");
                    let bytes = export(&doc, path, &ExportOptions::default()).expect("export").bytes;
                    let file = PsdFile::from_bytes(&bytes).expect("reparse");
                    let body = file.layer_info.as_ref().expect("layers").unpadded_len(file.header.version).unwrap() as usize;
                    let (k, len, after) = first_global_block(&bytes);
                    assert_eq!(k, key, "{name}: first global block");
                    assert_eq!(len, body, "{name}: declared length vs layer data");
                    assert_eq!(after, vec![0; (4 - body % 4) % 4], "{name}: padding after the block");
                    assert_eq!(common::strict_block_errors(&bytes), Vec::<String>::new(), "{name}");
                    unaligned += usize::from(!body.is_multiple_of(4));
                }
            }
        }
    }
    // Only a body that is not already a multiple of 4 tells padding inside the length apart from
    // padding after it.
    assert!(unaligned > 0, "no case exercises the padding");
}
