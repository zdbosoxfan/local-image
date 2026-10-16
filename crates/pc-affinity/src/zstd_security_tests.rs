use crate::{Archive, Limits, synth};

fn archive_with_rle_block(size: u32) -> Vec<u8> {
    let mut frame = vec![0x28, 0xb5, 0x2f, 0xfd, 0, 0]; // A 1KiB window.
    frame.extend_from_slice(&((size << 3) | 3).to_le_bytes()[..3]);
    frame.push(b'X');
    let mut archive = synth::container(&[("doc.dat", &frame, synth::Method::Stored)], None);
    let fat = u64::from_le_bytes(archive[16..24].try_into().unwrap()) as usize;
    let record = fat + 59; // One fixed-layout #FT4 header, then its first entry.
    archive[record + 13..record + 21].copy_from_slice(&(size as u64).to_le_bytes());
    archive[record + 29..record + 33].copy_from_slice(&crc32fast::hash(&vec![b'X'; size as usize]).to_le_bytes());
    archive[record + 33] = 2; // Zstandard; stored length stays at ten bytes.
    archive
}

#[test]
fn affinity_decoder_enforces_block_bound_despite_accepted_container_sizes() {
    let valid = archive_with_rle_block(128);
    assert_eq!(Archive::open(&valid, Limits::default()).unwrap().read("doc.dat").unwrap(), vec![b'X'; 128]);
    // The declared 2KiB entry fits every app budget, but exceeds this frame's1KiB window.
    // The old decoder accepted this tiny fixture; the patched decoder rejects the block.
    let invalid = archive_with_rle_block(2048);
    let error = Archive::open(&invalid, Limits::default()).unwrap().read("doc.dat").unwrap_err();
    assert!(error.to_string().contains("compressed"), "{error}");
}
