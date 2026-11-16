//! YCbCr TIFFs (PhotometricInterpretation 6), as scanners write them: JPEG-compressed with 2x2
//! chroma subsampling, and uncompressed with ReferenceBlackWhite and YCbCrCoefficients.

use crate::common::tiffgen::*;
use photocraft_codecs::*;

const SUBSAMPLING: u16 = 530;
const YCBCR_COEFFICIENTS: u16 = 529;
const REFERENCE_BLACK_WHITE: u16 = 532;

/// A little-endian RATIONAL value for `tiffgen`.
fn rational(num: u32, den: u32) -> u64 {
    u64::from(num) | (u64::from(den) << 32)
}

/// The image's pixels as RGB bytes.
fn rgb(img: &Image) -> Vec<u8> {
    img.to_rgba8().as_chunks::<4>().0.iter().flat_map(|[r, g, b, _]| [*r, *g, *b]).collect()
}

/// A smooth colour test card, `w`x`h` RGB.
fn card(w: u32, h: u32) -> Vec<u8> {
    let mut v = Vec::new();
    for y in 0..h {
        for x in 0..w {
            v.extend([(x * 255 / (w - 1)) as u8, (y * 255 / (h - 1)) as u8, (255 - x * 255 / (w - 1)) as u8]);
        }
    }
    v
}

#[test]
fn jpeg_compressed_ycbcr_with_subsampled_chroma_reads_as_rgb() {
    let (w, h) = (32, 24);
    let img = Image::from_raw(w, h, ChannelLayout::Rgb, SampleType::U8, card(w, h)).unwrap();
    let stream = encode(&img, Format::Jpeg, &EncodeOptions { jpeg_chroma_subsampling: true, ..Default::default() }).unwrap();
    let sof = stream.windows(2).position(|m| m == [0xFF, 0xC0]).expect("baseline frame header");
    assert_eq!(stream[sof + 11], 0x22, "luma sampling factors");
    let mut dir = strips(w, h, 8, 3, 6, h, &stream);
    dir.entries.retain(|e| e.0 != 259);
    let dir = dir.tag(259, SHORT, &[7]).tag(SUBSAMPLING, SHORT, &[2, 2]);
    let img = decode(&build(true, false, &[dir], &[0]).bytes).unwrap();
    assert_eq!((img.width(), img.height(), img.layout(), img.sample_type()), (w, h, ChannelLayout::Rgb, SampleType::U8));
    // The oracle: the same JPEG stream decoded to RGB by an independent decoder.
    let want = image::load_from_memory_with_format(&stream, image::ImageFormat::Jpeg).unwrap().to_rgb8().into_raw();
    let worst = rgb(&img).iter().zip(&want).map(|(g, o)| g.abs_diff(*o)).max().unwrap();
    assert!(worst <= 2, "largest difference from the oracle: {worst}");
}

#[test]
fn uncompressed_ycbcr_reads_as_rgb_chunky_and_planar() {
    // Black, white, mid gray and JFIF's pure red, without subsampling.
    let px = [0u8, 128, 128, 255, 128, 128, 128, 128, 128, 76, 85, 255];
    let want = [0, 0, 0, 255, 255, 255, 128, 128, 128, 254, 0, 0];
    let chunky = strips(4, 1, 8, 3, 6, 1, &px).tag(SUBSAMPLING, SHORT, &[1, 1]);
    assert_eq!(rgb(&decode(&build(true, false, &[chunky], &[0]).bytes).unwrap()), want);
    for little in [true, false] {
        let planar = Dir { entries: base(4, 1, 8, 3, 6), chunks: planar_strips(&px, 4, 1, 3, 1, 1), tiled: false }
            .tag(278, LONG, &[1])
            .tag(284, SHORT, &[2])
            .tag(SUBSAMPLING, SHORT, &[1, 1]);
        assert_eq!(rgb(&decode(&build(little, false, &[planar], &[0]).bytes).unwrap()), want, "little endian: {little}");
    }
    // 16-bit, big-endian: white, black and red.
    let px = [65535u16, 32768, 32768, 0, 32768, 32768, 19595, 21710, 65535];
    let dir = strips(3, 1, 16, 3, 6, 1, &u16s(&px, false)).tag(SUBSAMPLING, SHORT, &[1, 1]);
    let img = decode(&build(false, false, &[dir], &[0]).bytes).unwrap();
    assert_eq!((img.layout(), img.sample_type()), (ChannelLayout::Rgb, SampleType::U16));
    let n = img.to_normalized();
    let want = [1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    assert!(n.iter().zip(want).all(|(g, w)| (g - w).abs() < 2.0 / 255.0), "{n:?}");
}

#[test]
fn ycbcr_tags_set_the_conversion() {
    let decode_rgb = |px: &[u8], tags: &[(u16, Vec<u64>)]| {
        let mut dir = strips(px.len() as u32 / 3, 1, 8, 3, 6, 1, px).tag(SUBSAMPLING, SHORT, &[1, 1]);
        for (tag, values) in tags {
            dir = dir.tag(*tag, RATIONAL, values);
        }
        rgb(&decode(&build(true, false, &[dir], &[0]).bytes).unwrap())
    };
    // Studio range: Y 16-235, chroma 128 +- 112.
    let studio: Vec<u64> = [16, 235, 128, 240, 128, 240].map(|v| rational(v, 1)).to_vec();
    assert_eq!(decode_rgb(&[16, 128, 128, 235, 128, 128], &[(REFERENCE_BLACK_WHITE, studio.clone())]), [0, 0, 0, 255, 255, 255]);
    // Weights 0.25, 0.5, 0.25 with Cb -64 and Cr +64: R = Y + 1.5 Cr, B = Y + 1.5 Cb, G keeps Y.
    let weights = vec![rational(1, 4), rational(1, 2), rational(1, 4)];
    assert_eq!(decode_rgb(&[128, 64, 192], &[(YCBCR_COEFFICIENTS, weights.clone())]), [224, 128, 32]);
    // Weights that do not sum to one keep grays neutral, as libtiff does.
    let quarter = vec![rational(1, 4); 3];
    assert_eq!(decode_rgb(&[128, 128, 128, 128, 64, 192], &[(YCBCR_COEFFICIENTS, quarter)]), [128, 128, 128, 224, 128, 32]);
    // Tags with the wrong count, or that would divide by zero, use the defaults (BT.601, full range).
    let mut four = weights;
    four.push(rational(1, 4));
    let mut seven = studio;
    seven.push(rational(0, 1));
    assert_eq!(decode_rgb(&[128, 64, 192], &[(YCBCR_COEFFICIENTS, four), (REFERENCE_BLACK_WHITE, seven)]), [218, 104, 15]);
    let zero = vec![rational(0, 1), rational(0, 0), rational(5, 1)];
    assert_eq!(decode_rgb(&[255, 128, 128], &[(YCBCR_COEFFICIENTS, zero), (REFERENCE_BLACK_WHITE, vec![rational(7, 1); 6])]), [255, 255, 255]);
}

#[test]
fn unsupported_ycbcr_files_fail_cleanly() {
    // Four samples (an extra sample next to YCbCr).
    let four = strips(1, 1, 8, 4, 6, 1, &[128, 128, 128, 255]).tag(SUBSAMPLING, SHORT, &[1, 1]);
    assert!(decode(&build(true, false, &[four], &[0]).bytes).is_err());
    // Subsampled chroma without JPEG compression (the default subsampling is 2x2).
    let sub = strips(2, 2, 8, 3, 6, 2, &[128; 12]);
    assert!(decode(&build(true, false, &[sub], &[0]).bytes).is_err());
}
