use lightcraft_codecs::encode::ChromaSubsampling;
use lightcraft_codecs::jpeg_par;
use lightcraft_codecs::{DecodeOptions, Format, decode};

fn rgb(w: usize, h: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            v.extend_from_slice(&[(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, (x.wrapping_add(y).wrapping_mul(7)) as u8]);
        }
    }
    v
}

fn gray(w: usize, h: usize) -> Vec<u8> {
    (0..w * h).map(|i| (i % 251) as u8).collect()
}

fn decode_jpeg(bytes: &[u8]) -> lightcraft_codecs::Decoded {
    decode(bytes, DecodeOptions::default()).expect("encoded bytes must decode")
}

#[test]
fn encodes_1x1_grayscale_and_decodes() {
    let data = [128u8];
    let bytes = jpeg_par::encode(&data, 1, 1, 1, 92, ChromaSubsampling::S444, &[]);
    assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
    assert_eq!(&bytes[bytes.len() - 2..], &[0xFF, 0xD9]);

    let d = decode_jpeg(&bytes);
    assert_eq!(d.format, Format::Jpeg);
    assert_eq!((d.width, d.height), (1, 1));
    assert!(d.grayscale);
    assert!(!d.has_alpha);
}

#[test]
fn encodes_1x1_rgb_444_and_decodes() {
    let data = [255, 0, 0];
    let bytes = jpeg_par::encode(&data, 1, 1, 3, 90, ChromaSubsampling::S444, &[]);

    let d = decode_jpeg(&bytes);
    assert_eq!(d.format, Format::Jpeg);
    assert_eq!((d.width, d.height), (1, 1));
    assert!(!d.grayscale);
    assert!(!d.has_alpha);
}

#[test]
fn encodes_odd_sizes_rgb_420() {
    let (w, h) = (37, 23);
    let data = rgb(w, h);
    let bytes = jpeg_par::encode(&data, w, h, 3, 90, ChromaSubsampling::S420, &[]);

    let d = decode_jpeg(&bytes);
    assert_eq!(d.format, Format::Jpeg);
    assert_eq!((d.width as usize, d.height as usize), (w, h));
    assert!(d.width > 0 && d.height > 0);
}

#[test]
fn encodes_even_sizes_rgb_420() {
    let (w, h) = (64, 64);
    let data = rgb(w, h);
    let bytes = jpeg_par::encode(&data, w, h, 3, 85, ChromaSubsampling::S420, &[]);

    let d = decode_jpeg(&bytes);
    assert_eq!(d.format, Format::Jpeg);
    assert_eq!((d.width as usize, d.height as usize), (w, h));
}

#[test]
fn encodes_odd_grayscale_sizes() {
    let (w, h) = (13, 17);
    let data = gray(w, h);
    let bytes = jpeg_par::encode(&data, w, h, 1, 88, ChromaSubsampling::S444, &[]);

    let d = decode_jpeg(&bytes);
    assert_eq!(d.format, Format::Jpeg);
    assert_eq!((d.width as usize, d.height as usize), (w, h));
    assert!(d.grayscale);
}

#[test]
fn quality_100_produces_more_bytes_than_low_quality() {
    let (w, h) = (64, 64);
    let data = gray(w, h);
    let lo = jpeg_par::encode(&data, w, h, 1, 10, ChromaSubsampling::S444, &[]);
    let hi = jpeg_par::encode(&data, w, h, 1, 100, ChromaSubsampling::S444, &[]);
    assert!(hi.len() > lo.len());
}

#[test]
fn chroma_420_is_smaller_than_444_for_color() {
    let (w, h) = (64, 64);
    let data = rgb(w, h);
    let b444 = jpeg_par::encode(&data, w, h, 3, 90, ChromaSubsampling::S444, &[]);
    let b420 = jpeg_par::encode(&data, w, h, 3, 90, ChromaSubsampling::S420, &[]);
    assert!(b420.len() < b444.len());
}

#[test]
fn app_segment_payload_is_embedded() {
    let (w, h) = (4, 4);
    let data = rgb(w, h);
    let payload = b"custom-app-payload";
    let bytes = jpeg_par::encode(&data, w, h, 3, 90, ChromaSubsampling::S444, &[(0xE1, payload.to_vec())]);
    assert!(bytes.windows(payload.len()).any(|win| win == payload));
}

#[test]
fn default_jfif_header_present_without_app_segments() {
    let (w, h) = (4, 4);
    let data = gray(w, h);
    let bytes = jpeg_par::encode(&data, w, h, 1, 90, ChromaSubsampling::S444, &[]);
    assert!(bytes.windows(5).any(|win| win == b"JFIF\0"));
}

#[test]
fn custom_e0_segment_replaces_default_jfif() {
    let (w, h) = (4, 4);
    let data = rgb(w, h);
    let payload = b"Apple";
    let bytes = jpeg_par::encode(&data, w, h, 3, 90, ChromaSubsampling::S444, &[(0xE0, payload.to_vec())]);
    assert!(bytes.windows(payload.len()).any(|win| win == payload));
    assert!(!bytes.windows(5).any(|win| win == b"JFIF\0"));
}

#[test]
fn encode_is_deterministic() {
    let (w, h) = (20, 20);
    let data = rgb(w, h);
    let a = jpeg_par::encode(&data, w, h, 3, 90, ChromaSubsampling::S420, &[(0xE1, b"meta".to_vec())]);
    let b = jpeg_par::encode(&data, w, h, 3, 90, ChromaSubsampling::S420, &[(0xE1, b"meta".to_vec())]);
    assert_eq!(a, b);
}

#[test]
fn alpha_channel_is_dropped() {
    let (w, h) = (3, 3);
    let mut data = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            data.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, 128, 255]);
        }
    }
    let bytes = jpeg_par::encode(&data, w, h, 4, 90, ChromaSubsampling::S444, &[]);

    let d = decode_jpeg(&bytes);
    assert_eq!((d.width as usize, d.height as usize), (w, h));
    assert!(!d.has_alpha);
    assert!(!d.grayscale);
}

#[test]
fn zero_height_does_not_panic() {
    let bytes = jpeg_par::encode(&[], 1, 0, 1, 90, ChromaSubsampling::S444, &[]);
    assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
    assert_eq!(&bytes[bytes.len() - 2..], &[0xFF, 0xD9]);
}

// BUG: encode panics on width=0 because chunks_mut(0) is called internally.
#[test]
#[ignore = "BUG: encode panics on width=0 due to chunks_mut(0)"]
fn zero_width_panics_bug() {
    // This test documents the known panic and is ignored until fixed.
    let bytes = jpeg_par::encode(&[], 0, 0, 1, 90, ChromaSubsampling::S444, &[]);
    assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
    assert_eq!(&bytes[bytes.len() - 2..], &[0xFF, 0xD9]);
}

#[test]
fn large_image_uses_restart_markers() {
    // Height 520 with 8x8 MCU rows -> my = 65 -> more than one band -> DRI + RST markers.
    let (w, h) = (8, 520);
    let data = gray(w, h);
    let bytes = jpeg_par::encode(&data, w, h, 1, 90, ChromaSubsampling::S444, &[]);

    assert!(bytes.windows(2).any(|win| win[0] == 0xFF && win[1] == 0xDD));
    assert!(bytes.windows(2).any(|win| win[0] == 0xFF && (0xD0..=0xD7).contains(&win[1])));

    // The restart markers must not confuse the crate's own decoder.
    let d = decode_jpeg(&bytes);
    assert_eq!((d.width as usize, d.height as usize), (w, h));
}

#[test]
fn decode_roundtrip_various_dimensions() {
    let cases = [
        (1, 1, 1, ChromaSubsampling::S444),
        (2, 2, 1, ChromaSubsampling::S444),
        (16, 16, 3, ChromaSubsampling::S420),
        (31, 17, 3, ChromaSubsampling::S444),
        (33, 65, 3, ChromaSubsampling::S420),
        (64, 32, 1, ChromaSubsampling::S444),
    ];

    for (w, h, ch, sub) in cases {
        let data = if ch == 1 { gray(w, h) } else { rgb(w, h) };
        let bytes = jpeg_par::encode(&data, w, h, ch, 92, sub, &[]);
        let d = decode_jpeg(&bytes);
        assert_eq!(d.format, Format::Jpeg);
        assert_eq!((d.width as usize, d.height as usize), (w, h), "dimensions mismatch for {w}x{h} ch={ch} sub={sub:?}");
        assert_eq!(d.grayscale, ch == 1, "grayscale flag mismatch for {w}x{h} ch={ch}");
    }
}
