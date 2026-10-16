//! Tiny malformed frames exercise the upstream block bound, never a decompression bomb.
use photocraft_color::{Color, ColorMode, SampleType};
use photocraft_doc::{Document, Size};
use photocraft_format::{SaveOptions, load_from_bytes, save_to_bytes, zip};
use std::io::Read;

fn rle_frame(size: u32) -> Vec<u8> {
    let mut frame = vec![0x28, 0xb5, 0x2f, 0xfd, 0, 0]; // 1KiB window; no content-size field.
    let block = ((size << 3) | 3).to_le_bytes(); // Last block, RLE; one payload byte.
    frame.extend_from_slice(&block[..3]);
    frame.push(b'X');
    frame
}

#[test]
fn zstd_rejects_a_small_block_larger_than_its_window_before_outer_read_limit() {
    let bad = rle_frame(2048);
    assert_eq!(bad.len(), 10);
    let decoder = ruzstd::decoding::StreamingDecoder::new(bad.as_slice()).unwrap();
    let mut output = Vec::new();
    assert!(decoder.take(16).read_to_end(&mut output).is_err());
    assert!(output.is_empty());
    let good = rle_frame(128);
    let mut decoder = ruzstd::decoding::StreamingDecoder::new(good.as_slice()).unwrap();
    let mut output = Vec::new();
    decoder.read_to_end(&mut output).unwrap();
    assert_eq!(output, vec![b'X'; 128]);
}

#[test]
fn native_loader_rejects_the_invalid_block_inside_a_tile() {
    let document = Document::with_background("bounded fixture", Size::new(2, 2), ColorMode::Rgb, SampleType::U8, Color::WHITE);
    let original = save_to_bytes(&document, &SaveOptions::default()).unwrap();
    assert!(load_from_bytes(&original).is_ok());
    let reader = zip::ZipReader::new(&original).unwrap();
    let mut writer = zip::ZipWriter::new();
    let mut changed = false;
    for entry in &reader.entries {
        let data = if entry.name.starts_with("tiles/") {
            changed = true;
            rle_frame(2048)
        } else {
            reader.read(entry, usize::MAX).unwrap()
        };
        writer.add(&entry.name, &data).unwrap();
    }
    assert!(changed);
    let error = load_from_bytes(&writer.finish().unwrap()).unwrap_err().to_string();
    assert!(error.contains("zstd") && error.contains("block"), "{error}");
}
