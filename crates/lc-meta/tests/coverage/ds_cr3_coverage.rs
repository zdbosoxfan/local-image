use lightcraft_meta::cr3::{self, Cr3, Cr3TrackKind};
use lightcraft_meta::read_exif;
use lightcraft_tiff::{ByteOrder, IfdBuilder, TiffWriter, Value, tags as t};

const CANON_UUID: [u8; 16] = [0x85, 0xc0, 0xb6, 0x87, 0x82, 0x0f, 0x11, 0xe0, 0x81, 0x11, 0xf4, 0xce, 0x46, 0x2b, 0x6a, 0x48];
const XMP_UUID: [u8; 16] = [0xbe, 0x7a, 0xcf, 0xcb, 0x97, 0xa9, 0x42, 0xe8, 0x9c, 0x71, 0x99, 0x94, 0x91, 0xe3, 0xaf, 0xac];
const CRAW_CHILDREN_AT: usize = 82;

fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    v.extend_from_slice(kind);
    v.extend_from_slice(body);
    v
}

fn uuid_box(u: &[u8; 16], body: &[u8]) -> Vec<u8> {
    let mut b = u.to_vec();
    b.extend_from_slice(body);
    bx(b"uuid", &b)
}

fn full(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = vec![0u8; 4];
    b.extend_from_slice(body);
    bx(kind, &b)
}

fn craw(w: u16, h: u16, children: &[u8]) -> Vec<u8> {
    let mut b = vec![0u8; CRAW_CHILDREN_AT];
    b[24..26].copy_from_slice(&w.to_be_bytes());
    b[26..28].copy_from_slice(&h.to_be_bytes());
    b.extend_from_slice(children);
    bx(b"CRAW", &b)
}

fn trak(entry: Vec<u8>, offset: u64, size: u32) -> Vec<u8> {
    let mut stsd = 1u32.to_be_bytes().to_vec();
    stsd.extend(entry);
    let mut stsz = 0u32.to_be_bytes().to_vec();
    stsz.extend(1u32.to_be_bytes());
    stsz.extend(size.to_be_bytes());
    let mut co64 = 1u32.to_be_bytes().to_vec();
    co64.extend(offset.to_be_bytes());
    let stbl = [full(b"stsd", &stsd), full(b"stsz", &stsz), full(b"co64", &co64)].concat();
    bx(b"trak", &bx(b"mdia", &bx(b"minf", &bx(b"stbl", &stbl))))
}

fn tiff_with(tag: u16, v: Value) -> Vec<u8> {
    let b = IfdBuilder::new().with(tag, v);
    TiffWriter::new(ByteOrder::Little, false).write(&[b]).unwrap()
}

fn ftyp() -> Vec<u8> {
    bx(b"ftyp", b"crx \0\0\0\x01crx isom")
}

fn sample_metadata() -> Vec<u8> {
    let cmt1 = tiff_with(t::MODEL, Value::Ascii("Canon EOS Test".into()));
    let cmt2 = tiff_with(t::ISO_SPEED, Value::Short(vec![800]));
    let cmt4 = tiff_with(1, Value::Ascii("N".into()));
    let canon = uuid_box(&CANON_UUID, &[bx(b"CMT1", &cmt1), bx(b"CMT2", &cmt2), bx(b"CMT4", &cmt4), bx(b"THMB", b"thumb")].concat());
    let mut f = ftyp();
    f.extend(bx(b"moov", &canon));
    f
}

fn sample_with_tracks() -> Vec<u8> {
    let payload_at = 4096u64;
    let jpeg_t = trak(craw(64, 48, &bx(b"JPEG", &[0; 4])), payload_at, 16);
    let raw_t = trak(craw(60, 40, &bx(b"CMP1", &[0xff, 0x10, 0, 0x30])), payload_at + 16, 32);
    let mut f = ftyp();
    f.extend(bx(b"moov", &[jpeg_t, raw_t].concat()));
    f.resize(payload_at as usize + 64, 0);
    f
}

#[test]
fn is_cr3_true_for_ftyp_crx() {
    let f = ftyp();
    assert!(cr3::is_cr3(&f));
    assert!(cr3::is_cr3(&f[..12]));
}

#[test]
fn is_cr3_false_for_wrong_or_short_inputs() {
    assert!(!cr3::is_cr3(&bx(b"ftyp", b"cr0 \0\0\0\x01crx isom")));
    assert!(!cr3::is_cr3(&bx(b"moov", b"crx \0\0\0\x01crx isom")));
    let f = ftyp();
    for n in 0..12 {
        assert!(!cr3::is_cr3(&f[..n]), "short length {n}");
    }
}

#[test]
fn parse_cr3_none_for_non_cr3() {
    assert_eq!(cr3::parse_cr3(b""), None);
    assert_eq!(cr3::parse_cr3(b"hello"), None);
    assert_eq!(cr3::parse_cr3(&bx(b"moov", b"crx \0\0\0\x01crx isom")), None);
}

#[test]
fn parse_cr3_minimal_returns_defaults() {
    let f = ftyp();
    let c = cr3::parse_cr3(&f).unwrap();
    assert_eq!(c.cmt, [None, None, None, None]);
    assert_eq!(c.thumbnail, None);
    assert_eq!(c.xmp, None);
    assert!(c.tracks.is_empty());
}

#[test]
fn parse_cr3_finds_canon_cmt_and_thumbnail() {
    let canon = uuid_box(
        &CANON_UUID,
        &[bx(b"CMT1", b"one"), bx(b"CMT2", b"two"), bx(b"CMT3", b"three"), bx(b"CMT4", b"four"), bx(b"THMB", b"thumb")].concat(),
    );
    let mut f = ftyp();
    f.extend(bx(b"moov", &canon));
    let c = cr3::parse_cr3(&f).unwrap();
    assert_eq!(c.cmt[0], Some(&b"one"[..]));
    assert_eq!(c.cmt[1], Some(&b"two"[..]));
    assert_eq!(c.cmt[2], Some(&b"three"[..]));
    assert_eq!(c.cmt[3], Some(&b"four"[..]));
    assert_eq!(c.thumbnail, Some(&b"thumb"[..]));
}

#[test]
fn parse_cr3_finds_xmp_uuid() {
    let mut f = ftyp();
    f.extend(uuid_box(&XMP_UUID, b"<x:xmpmeta/>"));
    let c = cr3::parse_cr3(&f).unwrap();
    assert_eq!(c.xmp, Some(&b"<x:xmpmeta/>"[..]));
}

#[test]
fn parse_cr3_ignores_unknown_uuid() {
    let mut f = ftyp();
    f.extend(uuid_box(&[0xAB; 16], b"junk"));
    let c = cr3::parse_cr3(&f).unwrap();
    assert_eq!(c.xmp, None);
}

#[test]
fn parse_cr3_track_jpeg_data_roundtrip() {
    let payload_at = 4096usize;
    let size = 16usize;
    let f = sample_with_tracks();
    let c = cr3::parse_cr3(&f).unwrap();
    assert_eq!(c.tracks.len(), 2);
    assert_eq!(c.tracks[0].kind, Cr3TrackKind::Jpeg);
    assert_eq!(c.tracks[0].data, Some((payload_at, size)));
}

#[test]
fn parse_cr3_track_raw_dimensions_and_cmp1() {
    let f = sample_with_tracks();
    let c = cr3::parse_cr3(&f).unwrap();
    let raw = match &c.tracks[1].kind {
        Cr3TrackKind::Raw { width, height, cmp1: Some((at, len)) } => (*width, *height, *at, *len),
        other => panic!("expected raw with cmp1, got {other:?}"),
    };
    assert_eq!((raw.0, raw.1, raw.3), (60, 40, 4));
    assert_eq!(&f[raw.2..raw.2 + raw.3], &[0xff, 0x10, 0, 0x30]);
    assert_eq!(c.tracks[1].data, Some((4112, 32)));
}

#[test]
fn parse_cr3_track_raw_without_cmp1() {
    let t = trak(craw(60, 40, &[]), 4096, 0);
    let mut f = ftyp();
    f.extend(bx(b"moov", &t));
    let c = cr3::parse_cr3(&f).unwrap();
    match &c.tracks[0].kind {
        Cr3TrackKind::Raw { width: 60, height: 40, cmp1: None } => {}
        other => panic!("expected raw without cmp1, got {other:?}"),
    }
    assert_eq!(c.tracks[0].data, None);
}

#[test]
fn parse_cr3_other_sample_entry_kind() {
    let entry = bx(b"CTMD", &[]);
    let t = trak(entry, 1_000_000, 0);
    let mut f = ftyp();
    f.extend(bx(b"moov", &t));
    let c = cr3::parse_cr3(&f).unwrap();
    match &c.tracks[0].kind {
        Cr3TrackKind::Other(k) => assert_eq!(k, b"CTMD"),
        other => panic!("expected other kind, got {other:?}"),
    }
    assert_eq!(c.tracks[0].data, None);
}

#[test]
fn parse_cr3_track_data_none_when_offset_out_of_bounds() {
    let t = trak(craw(64, 48, &bx(b"JPEG", &[0; 4])), 1_000_000, 32);
    let mut f = ftyp();
    f.extend(bx(b"moov", &t));
    let c = cr3::parse_cr3(&f).unwrap();
    assert_eq!(c.tracks[0].data, None);
}

#[test]
fn parse_cr3_truncations_never_panic() {
    let f = sample_metadata();
    for n in 0..=f.len() {
        let _ = cr3::parse_cr3(&f[..n]).map(|c| cr3::merged_exif(&c));
    }
}

#[test]
fn parse_cr3_corruptions_never_panic() {
    let f = sample_metadata();
    for i in 0..f.len().min(64) {
        for v in [0u8, 0xff] {
            let mut g = f.clone();
            g[i] = v;
            let _ = cr3::parse_cr3(&g).map(|c| cr3::merged_exif(&c));
        }
    }
}

#[test]
fn parse_cr3_deterministic() {
    let f = sample_metadata();
    let a = cr3::parse_cr3(&f).unwrap();
    let b = cr3::parse_cr3(&f).unwrap();
    assert_eq!(a, b);
    assert_eq!(cr3::merged_exif(&a), cr3::merged_exif(&b));
}

#[test]
fn merged_exif_none_without_cmt1() {
    assert_eq!(cr3::merged_exif(&Cr3::default()), None);
    let mut c = Cr3::default();
    c.cmt[0] = Some(b"not a tiff");
    assert_eq!(cr3::merged_exif(&c), None);
    c.cmt[0] = Some(b"");
    assert_eq!(cr3::merged_exif(&c), None);
}

#[test]
fn merged_exif_combines_ifd0_and_exif_and_gps() {
    let cmt1 = tiff_with(t::MODEL, Value::Ascii("Canon EOS Test".into()));
    let cmt2 = tiff_with(t::ISO_SPEED, Value::Short(vec![800]));
    let cmt4 = tiff_with(1, Value::Ascii("N".into()));
    let canon = uuid_box(&CANON_UUID, &[bx(b"CMT1", &cmt1), bx(b"CMT2", &cmt2), bx(b"CMT4", &cmt4)].concat());
    let mut f = ftyp();
    f.extend(bx(b"moov", &canon));
    let c = cr3::parse_cr3(&f).unwrap();
    let merged = cr3::merged_exif(&c).unwrap();
    let m = read_exif(&merged);
    assert_eq!(m.model.as_deref(), Some("Canon EOS Test"));
    assert_eq!(m.iso, Some(800));
}

#[test]
fn merged_exif_ignores_invalid_optional_cmt2() {
    let cmt1 = tiff_with(t::MODEL, Value::Ascii("Canon EOS Test".into()));
    let canon = uuid_box(&CANON_UUID, &[bx(b"CMT1", &cmt1), bx(b"CMT2", b"not a tiff")].concat());
    let mut f = ftyp();
    f.extend(bx(b"moov", &canon));
    let c = cr3::parse_cr3(&f).unwrap();
    let merged = cr3::merged_exif(&c).unwrap();
    let m = read_exif(&merged);
    assert_eq!(m.model.as_deref(), Some("Canon EOS Test"));
    assert_eq!(m.iso, None);
}
