use super::*;
use serde_json::json;

#[test]
fn bugfix_save_o029_overwrite_never_reuses_a_batch_output_or_sidecar() {
    use lightcraft_catalog::{Op, PhotoId};
    let mut session = crate::Session::with_demo();
    for id in 1..=2 {
        let mut photo = (**session.catalog.photo(PhotoId(id)).unwrap()).clone();
        photo.file_name = "same.png".into();
        photo.format = "PNG".into();
        photo.width = 8;
        photo.height = 6;
        session.catalog.apply(Op::RemovePhoto { id: PhotoId(id) }).unwrap();
        session.catalog.apply(Op::AddPhoto { photo: Box::new(photo) }).unwrap();
    }
    let options = ExportOptions { conflict: Conflict::Overwrite, format: ExportFormat::Png, ..Default::default() };
    let items = prepare_batch(&mut session, &[PhotoId(1), PhotoId(2)], &options).unwrap();
    let mut paths = Vec::new();
    let result = run_batch(
        items,
        &options,
        &Destination { dir: "/exports".into(), exact: None },
        &mut |path, _| {
            paths.push(path.to_string());
            Ok(())
        },
        &|p| p == "/exports/same.png",
        true,
        &mut |_, _| true,
    )
    .unwrap();
    assert_eq!(paths, ["/exports/same.png", "/exports/same-2.png"]);
    assert_eq!(result.len(), 2);

    let guard = std::sync::Arc::new(session.original_guard());
    let items = ["pair.nef", "pair.jpg"]
        .into_iter()
        .enumerate()
        .map(|(i, name)| PreparedExport {
            photo: PhotoId(i as u64 + 1),
            file_name: name.into(),
            subfolder: None,
            source_dir: None,
            guard: guard.clone(),
            work: Work::File {
                path: format!("/input/{name}"),
                read: Some(std::sync::Arc::new(|_| Ok(b"original".to_vec()))),
                packet: "<xmp/>".into(),
                dng: None,
                remove_location: false,
                label: name.into(),
                size: (8, 6),
            },
        })
        .collect();
    let mut paths = Vec::new();
    run_batch(
        items,
        &options,
        &Destination { dir: "/exports".into(), exact: None },
        &mut |path, _| {
            paths.push(path.to_string());
            Ok(())
        },
        &|_| false,
        true,
        &mut |_, _| true,
    )
    .unwrap();
    assert_eq!(paths, ["/exports/pair.nef", "/exports/pair.xmp", "/exports/pair-2.jpg", "/exports/pair-2.xmp"]);
}

// the ordered batch pipeline

fn wide_open() -> lightcraft_sysmem::HeavyJobs {
    lightcraft_sysmem::HeavyJobs::new(4, 64 << 30, || lightcraft_sysmem::MemoryNow { available: 64 << 30, rss: None })
}

fn parallel() -> pipeline::Shape {
    pipeline::Shape { renderers: 3, encoders: 3, window: 8 }
}

/// Demo photos as rendered exports, `n` of them (the demo has a few; repeated as needed).
fn rendered_batch(session: &mut crate::Session, n: usize, o: &ExportOptions) -> Vec<PreparedExport> {
    let ids: Vec<_> = session.catalog.photos().map(|p| p.id).collect();
    let ids: Vec<_> = (0..n).map(|i| ids[i % ids.len()]).collect();
    prepare_batch(session, &ids, o).unwrap()
}

/// `bytes` with the creation time of each embedded ICC profile (the 12 bytes before its `acsp`
/// signature) blanked: the profile writer stamps the current time, so two runs a second apart
/// differ there and nowhere else.
fn without_icc_dates(bytes: &[u8]) -> Vec<u8> {
    let mut b = bytes.to_vec();
    let at: Vec<usize> = b.windows(4).enumerate().filter(|(i, w)| *w == b"acsp" && *i >= 12).map(|(i, _)| i).collect();
    for i in at {
        b[i - 12..i].fill(0);
    }
    b
}

/// `(path, bytes)` in write order and the progress calls of one shaped run (ICC creation times
/// blanked, [`without_icc_dates`]).
#[allow(clippy::type_complexity)]
fn shaped_run(
    items: Vec<PreparedExport>,
    o: &ExportOptions,
    permits: &lightcraft_sysmem::HeavyJobs,
    shape: pipeline::Shape,
) -> (Vec<serde_json::Value>, Vec<(String, Vec<u8>)>, Vec<usize>) {
    let (mut written, mut calls) = (Vec::new(), Vec::new());
    let to = Destination { dir: "/exports".into(), exact: None };
    let files = run_batch_shaped(
        items,
        o,
        &to,
        &mut |p, b| {
            written.push((p.to_string(), without_icc_dates(b)));
            Ok(())
        },
        &|_| false,
        false,
        &mut |done, _| {
            calls.push(done);
            true
        },
        &lightcraft_sysmem::JobCtl::new(),
        permits,
        shape,
    )
    .unwrap();
    (files, written, calls)
}

#[test]
fn pipeline_depth_does_not_change_the_exported_files_or_their_order() {
    let _gpu = crate::tests_gpu::gpu_state(); // no other test switches the GPU off mid-way
    let mut session = crate::Session::with_demo();
    for o in [
        ExportOptions::from_json(&json!({"format": "jpeg", "width": 96, "height": 96, "naming": "{name}"})),
        ExportOptions::from_json(&json!({"format": "tiff", "width": 64, "height": 64, "bitDepth": 16, "naming": "{name}"})),
    ] {
        let permits = wide_open();
        let serial = shaped_run(rendered_batch(&mut session, 7, &o), &o, &permits, pipeline::Shape::serial());
        let deep = shaped_run(rendered_batch(&mut session, 7, &o), &o, &permits, parallel());
        let auto = shaped_run(rendered_batch(&mut session, 7, &o), &o, &permits, pipeline::Shape::auto(7, 4, 8));
        assert_eq!(serial.1.len(), 7, "every photo written (repeated names made unique)");
        assert!(serial.1 == deep.1, "same bytes, same paths, same order at depth 1 and N");
        assert!(serial.1 == auto.1);
        assert_eq!(serial.0, deep.0);
        assert_eq!(serial.2, (0..7).collect::<Vec<_>>(), "progress in input order");
        assert_eq!(deep.2, serial.2);
        assert_eq!(permits.in_flight(), (0, 0), "every permit released");
    }
}

/// File-format items whose reads go through `read` (no disk): `{name}.bin` with bytes `name`.
fn file_items(session: &crate::Session, names: &[&str], read: crate::merge::ByteReader) -> Vec<PreparedExport> {
    let guard = std::sync::Arc::new(session.original_guard());
    names
        .iter()
        .enumerate()
        .map(|(i, name)| PreparedExport {
            photo: lightcraft_catalog::PhotoId(i as u64 + 1),
            file_name: format!("{name}.bin"),
            subfolder: None,
            source_dir: None,
            guard: guard.clone(),
            work: Work::File {
                path: format!("/input/{name}"),
                read: Some(read.clone()),
                packet: "<xmp/>".into(),
                dng: None,
                remove_location: false,
                label: name.to_string(),
                size: (8, 6),
            },
        })
        .collect()
}

#[test]
fn one_failed_photo_does_not_stop_the_others_and_is_reported_in_place() {
    let session = crate::Session::with_demo();
    let read: crate::merge::ByteReader =
        std::sync::Arc::new(|p: &str| if p.ends_with("bad") { Err(format!("{p}: unreadable")) } else { Ok(p.as_bytes().to_vec()) });
    let o = ExportOptions { format: ExportFormat::Original, ..Default::default() };
    let names = ["a", "b", "bad", "c", "d"];
    for shape in [pipeline::Shape::serial(), parallel()] {
        let (files, written, _) = shaped_run(file_items(&session, &names, read.clone()), &o, &wide_open(), shape);
        assert_eq!(files.len(), 5);
        assert_eq!(files[2]["error"], "/input/bad: unreadable");
        assert_eq!(files[2]["photo"], 3);
        let paths: Vec<_> = written.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            paths,
            [
                "/exports/a.bin",
                "/exports/a.xmp",
                "/exports/b.bin",
                "/exports/b.xmp",
                "/exports/c.bin",
                "/exports/c.xmp",
                "/exports/d.bin",
                "/exports/d.xmp"
            ]
        );
        assert_eq!(written[4].1, b"/input/c");
    }
    // stop_on_error still ends the batch at the failed photo, after the ones before it
    let mut written = Vec::new();
    let err = run_batch_shaped(
        file_items(&session, &names, read),
        &o,
        &Destination { dir: "/exports".into(), exact: None },
        &mut |p, _| {
            written.push(p.to_string());
            Ok(())
        },
        &|_| false,
        true,
        &mut |_, _| true,
        &lightcraft_sysmem::JobCtl::new(),
        &wide_open(),
        parallel(),
    )
    .unwrap_err();
    assert_eq!(err, "/input/bad: unreadable");
    assert_eq!(written, ["/exports/a.bin", "/exports/a.xmp", "/exports/b.bin", "/exports/b.xmp"]);
}

#[test]
fn cancelling_mid_batch_leaves_only_whole_files() {
    let _gpu = crate::tests_gpu::gpu_state();
    let mut session = crate::Session::with_demo();
    let dir = std::env::temp_dir().join(format!("lc-export-cancel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let o = ExportOptions::from_json(&json!({"format": "jpeg", "width": 120, "height": 120}));
    let reference = shaped_run(rendered_batch(&mut session, 8, &o), &o, &wide_open(), pipeline::Shape::serial()).1;
    for cancel_by_ctl in [false, true] {
        let _ = std::fs::remove_dir_all(&dir);
        let ctl = lightcraft_sysmem::JobCtl::new();
        let mut count = 0;
        let files = run_batch_shaped(
            rendered_batch(&mut session, 8, &o),
            &o,
            &Destination { dir: dir.to_string_lossy().to_string(), exact: None },
            &mut |p, b| {
                write_file(p, b)?;
                count += 1;
                if count == 3 && cancel_by_ctl {
                    ctl.cancel(); // the export dialog's Cancel, while later photos are in flight
                }
                Ok(())
            },
            &|p| std::path::Path::new(p).exists(),
            false,
            &mut |done, _| cancel_by_ctl || done < 3,
            &ctl,
            &wide_open(),
            parallel(),
        )
        .unwrap();
        assert_eq!(files.len(), 3, "{files:?}");
        let mut on_disk: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        on_disk.sort();
        let expected: Vec<_> = reference[..3].iter().map(|(p, _)| p.rsplit('/').next().unwrap().to_string()).collect();
        let mut sorted = expected.clone();
        sorted.sort();
        assert_eq!(on_disk, sorted, "no partial or temporary files");
        for (name, (_, bytes)) in expected.iter().zip(&reference) {
            assert!(without_icc_dates(&std::fs::read(dir.join(name)).unwrap()) == *bytes, "{name} is whole");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn memory_admission_bounds_the_photos_in_flight() {
    let _gpu = crate::tests_gpu::gpu_state();
    let mut session = crate::Session::with_demo();
    let o = ExportOptions::from_json(&json!({"format": "jpeg", "width": 64, "height": 64}));
    // a machine with no headroom at all: one photo at a time, and the batch still completes
    let tight = lightcraft_sysmem::HeavyJobs::new(4, 0, || lightcraft_sysmem::MemoryNow { available: 0, rss: None });
    let peak = std::sync::atomic::AtomicUsize::new(0);
    let to = Destination { dir: "/exports".into(), exact: None };
    let files = run_batch_shaped(
        rendered_batch(&mut session, 6, &o),
        &o,
        &to,
        &mut |_, _| {
            peak.fetch_max(tight.in_flight().0, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        },
        &|_| false,
        false,
        &mut |_, _| true,
        &lightcraft_sysmem::JobCtl::new(),
        &tight,
        parallel(),
    )
    .unwrap();
    assert_eq!(files.iter().filter(|f| f.get("path").is_some()).count(), 6);
    assert!(peak.load(std::sync::atomic::Ordering::Relaxed) <= 1, "never two renders at once");
    assert_eq!(tight.in_flight(), (0, 0));
    // estimates grow with the pixels, and a 16-bit output costs more than an 8-bit one
    assert!(render_cost((6000, 4000), (6000, 4000), OutputDepth::U8) > 1 << 30);
    assert!(render_cost((6000, 4000), (2048, 1365), OutputDepth::U8) < render_cost((6000, 4000), (6000, 4000), OutputDepth::U8));
    assert!(render_cost((6000, 4000), (6000, 4000), OutputDepth::U16) > render_cost((6000, 4000), (6000, 4000), OutputDepth::U8));
    assert_eq!(render_cost((0, 0), (0, 0), OutputDepth::U8), UNKNOWN_PIXELS * 48);
}

#[test]
fn pipeline_shape_follows_the_permits_and_cores() {
    use pipeline::Shape;
    assert_eq!(Shape::auto(20, 1, 2), Shape { renderers: 1, encoders: 2, window: 5 }, "2-core laptop: render overlaps encode");
    assert_eq!(Shape::auto(20, 2, 7), Shape { renderers: 2, encoders: 3, window: 7 });
    assert_eq!(Shape::auto(20, 4, 15), Shape { renderers: 4, encoders: 5, window: 11 });
    assert_eq!(Shape::auto(1, 4, 15), Shape { renderers: 1, encoders: 1, window: 4 }, "never more threads than photos");
    assert_eq!(Shape::auto(0, 0, 0), Shape { renderers: 1, encoders: 1, window: 4 });
}

fn test_image() -> Rgba8 {
    let mut img = Rgba8::new(96, 64);
    for (i, p) in img.data.iter_mut().enumerate() {
        let (x, y) = (i % 96, i / 96);
        *p = [(x * 2) as u8, (y * 3) as u8, ((x * y) % 255) as u8, 255];
    }
    img
}

#[test]
fn formats_have_magic() {
    let img = test_image();
    for (f, magic) in
        [(ExportFormat::Jpeg, &b"\xFF\xD8"[..]), (ExportFormat::Png, b"\x89PNG"), (ExportFormat::Tiff, b"II"), (ExportFormat::Webp, b"RIFF")]
    {
        let b = encode_image(&img, &ExportOptions { format: f, ..Default::default() }).unwrap();
        assert!(b.starts_with(magic), "{f:?}");
    }
}

/// The tags of a little-endian TIFF's first IFD.
fn tiff_tags(b: &[u8]) -> Vec<u16> {
    let u16_at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let ifd = u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize;
    (0..usize::from(u16_at(ifd))).map(|i| u16_at(ifd + 2 + 12 * i)).collect()
}

#[test]
fn opaque_eight_bit_exports_carry_no_alpha_channel() {
    let img = test_image();
    let encode = |format, img: &Rgba8| encode_image(img, &ExportOptions { format, bit_depth: Some(8), ..Default::default() }).unwrap();
    let tiff = encode(ExportFormat::Tiff, &img);
    assert!(!tiff_tags(&tiff).contains(&338), "no ExtraSamples tag");
    for (format, bytes) in
        [(ExportFormat::Tiff, tiff), (ExportFormat::Png, encode(ExportFormat::Png, &img)), (ExportFormat::Webp, encode(ExportFormat::Webp, &img))]
    {
        let d = lightcraft_codecs::decode(&bytes, Default::default()).unwrap();
        assert!(!d.has_alpha && d.bit_depth == 8, "{format:?}");
        let worst = d.to_srgb8().data.iter().zip(&img.data).flat_map(|(a, b)| (0..4).map(move |c| a[c].abs_diff(b[c]))).max();
        assert!(worst <= Some(1), "{format:?}: the pixels survive packing ({worst:?})");
    }
    let mut clear = test_image();
    clear.data[5][3] = 0;
    for format in [ExportFormat::Tiff, ExportFormat::Png] {
        assert!(lightcraft_codecs::decode(&encode(format, &clear), Default::default()).unwrap().has_alpha, "{format:?} keeps real transparency");
    }
}

#[test]
fn size_limit_respected() {
    let img = test_image();
    let full = encode_image(&img, &ExportOptions { quality: 100, ..Default::default() }).unwrap();
    let kb = (full.len() / 1024 / 2).max(2) as u32;
    let b = encode_image(&img, &ExportOptions { quality: 100, limit_kb: Some(kb), ..Default::default() }).unwrap();
    assert!(b.len() <= kb as usize * 1024);
}

#[test]
fn sharpen_increases_edge_contrast() {
    let mut img = Rgba8::new(8, 8);
    for (i, p) in img.data.iter_mut().enumerate() {
        let v = if i % 8 < 4 { 80 } else { 170 };
        *p = [v, v, v, 255];
    }
    output_sharpen(&mut img, OutputSpace::Srgb, SharpenFor::Matte, SharpenAmount::High);
    assert!(img.data[3][0] < 80 && img.data[4][0] > 170);
}

/// An 8×8 image: `left` in the left half, `right` in the right half.
fn edge(left: [u8; 3], right: [u8; 3]) -> Rgba8 {
    Rgba8::from_fn(8, 8, |x, _| {
        let c = if x < 4 { left } else { right };
        [c[0], c[1], c[2], 255]
    })
}

fn oklch_chroma(p: [u8; 4]) -> f32 {
    let lab = li_color::srgb_perceptual::linear_srgb_to_oklab([0, 1, 2].map(|c| li_color::transfer::decode_srgb8(p[c])));
    lab[1].hypot(lab[2])
}

#[test]
fn output_sharpening_adds_no_coloured_fringes() {
    // brick red against a neutral wall: the wall's halo stays neutral (per-channel sharpening
    // turned it cyan, (175, 215, 218))
    let mut img = edge([200, 40, 30], [180, 180, 180]);
    output_sharpen(&mut img, OutputSpace::Srgb, SharpenFor::Glossy, SharpenAmount::High);
    let halo = img.data[4];
    assert!(halo[0] > 180, "the edge is still sharpened: {halo:?}");
    assert!(halo[0].max(halo[1]).max(halo[2]) - halo[0].min(halo[1]).min(halo[2]) <= 2, "neutral halo: {halo:?}");
    // red against blue: neither side's halo is more colourful than the flat colour (per-channel
    // sharpening added about 20 %)
    let flat = edge([200, 40, 30], [30, 60, 200]);
    let mut img = flat.clone();
    output_sharpen(&mut img, OutputSpace::Srgb, SharpenFor::Glossy, SharpenAmount::High);
    for x in [3, 4] {
        let (before, after) = (oklch_chroma(flat.data[8 + x]), oklch_chroma(img.data[8 + x]));
        assert!(after <= before * 1.03, "x = {x}: chroma {before} -> {after}");
    }
    // a pixel already at the top keeps its hue rather than being clipped per channel
    let mut img = edge([60, 60, 60], [255, 120, 20]);
    output_sharpen(&mut img, OutputSpace::Srgb, SharpenFor::Matte, SharpenAmount::High);
    assert_eq!(img.data[4], [255, 120, 20, 255]);
    assert!(img.data[3][0] < 60 && img.data[3][0] == img.data[3][1] && img.data[3][1] == img.data[3][2], "{:?}", img.data[3]);
}

#[test]
fn deep_output_sharpening_adds_no_coloured_fringes() {
    let deep = |samples: DeepSamples| DeepImage { width: 8, height: 8, space: OutputSpace::AdobeRgb, samples };
    let half = |left: [f32; 3], right: [f32; 3]| (0..64).flat_map(move |i| if i % 8 < 4 { left } else { right });
    let mut img = deep(DeepSamples::U16(half([51400.0, 10280.0, 7710.0], [46260.0; 3]).map(|v| v as u16).collect()));
    output_sharpen_deep(&mut img, SharpenFor::Glossy, SharpenAmount::High);
    let DeepSamples::U16(v) = &img.samples else { unreachable!() };
    let halo = [v[12], v[13], v[14]];
    assert!(halo[0] > 46260 && halo.iter().max().unwrap() - halo.iter().min().unwrap() <= 2, "neutral 16-bit halo: {halo:?}");
    // linear float: the halo keeps the chromaticity of its side, and black stays black
    let mut img = deep(DeepSamples::F32(half([0.5, 0.03, 0.02], [0.4; 3]).collect()));
    output_sharpen_deep(&mut img, SharpenFor::Matte, SharpenAmount::High);
    let DeepSamples::F32(v) = &img.samples else { unreachable!() };
    let (red, grey) = (&v[9..12], &v[12..15]);
    assert!(grey[0] > 0.4 && (grey[0] - grey[1]).abs() < 1e-6 && (grey[1] - grey[2]).abs() < 1e-6, "{grey:?}");
    assert!(red[0] < 0.5 && (red[1] / red[0] - 0.06).abs() < 1e-5 && (red[2] / red[0] - 0.04).abs() < 1e-5, "{red:?}");
    let mut img = deep(DeepSamples::F32(half([0.0; 3], [1.0; 3]).collect()));
    output_sharpen_deep(&mut img, SharpenFor::Matte, SharpenAmount::High);
    let DeepSamples::F32(v) = &img.samples else { unreachable!() };
    assert!(v.iter().all(|x| x.is_finite() && *x >= 0.0) && v[9] == 0.0 && v[12] > 1.0);
}

#[test]
fn sony_dji_metadata_export_preserves_telemetry_and_respects_policies() {
    use lightcraft_catalog::{Photo, PhotoId, Source};
    let mut p = Photo::new(PhotoId(1), Source::Demo { scene: 0 }, "a.jpg", "jpeg", 10, 10, "2026-09-30T00:00:00");
    let packet = r#"<rdf:Description xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:d="http://www.dji.com/drone-dji/1.0/" d:RelativeAltitude="+12.500" d:GimbalYawDegree="-45.25" d:FlightXSpeed="1.2" d:CustomLocation="retained source"/>"#;
    p.meta.dji = lightcraft_meta::parse_xmp(packet).unwrap().metadata.dji;
    p.meta.serial_number = Some("BODY123".into());
    p.meta.sony = Some(lightcraft_meta::SonyMetadata { shutter_count: Some(42), ..Default::default() });
    let all = export_metadata(&p, &ExportOptions::default()).unwrap();
    assert_eq!(all.dji, p.meta.dji);
    assert_eq!(all.sony, p.meta.sony);
    assert_eq!(all.serial_number, p.meta.serial_number);
    let sidecar = crate::sidecar::sidecar_packet(&p, &lightcraft_catalog::Catalog::default());
    assert_eq!(lightcraft_meta::parse_xmp(&sidecar).unwrap().metadata.dji, p.meta.dji);
    let scrubbed_photo = photo_for_sidecar(&p, false, true);
    assert!(scrubbed_photo.meta.dji.is_none());

    let jpeg = encode_with_metadata(&test_image(), &ExportOptions::default(), Some(&all)).unwrap();
    assert_eq!(lightcraft_meta::extract(&jpeg).dji, p.meta.dji);
    for policy in [MetadataPolicy::AllExceptCamera, MetadataPolicy::Copyright] {
        let m = export_metadata(&p, &ExportOptions { metadata: policy, ..Default::default() }).unwrap();
        assert!(m.dji.is_none() && m.sony.is_none() && m.serial_number.is_none());
    }
    let removed = export_metadata(&p, &ExportOptions { remove_location: true, ..Default::default() }).unwrap();
    assert!(removed.dji.is_none());
    assert!(!lightcraft_meta::write_xmp(&removed, None).contains("CustomLocation"));
    let mut old = serde_json::to_value(&p.meta).unwrap();
    for field in ["sony", "dji", "serial_number"] {
        old.as_object_mut().unwrap().remove(field);
    }
    let old: lightcraft_catalog::Meta = serde_json::from_value(old).unwrap();
    assert!(old.sony.is_none() && old.dji.is_none() && old.serial_number.is_none());
}

#[test]
fn metadata_policies() {
    use lightcraft_catalog::{Photo, PhotoId, Source};
    let mut p = Photo::new(PhotoId(1), Source::Demo { scene: 0 }, "a.jpg", "jpeg", 10, 10, "2026-09-30T00:00:00");
    p.meta.camera = "Synthetic X2".into();
    p.meta.copyright = "(c) Me".into();
    p.meta.copyright_status = lightcraft_catalog::CopyrightStatus::Copyrighted;
    p.meta.usage_terms = "Editorial use only".into();
    p.meta.copyright_url = "https://example.com/rights".into();
    p.meta.shutter = "1/250".into();
    p.meta.gps = Some((43.0, -110.0));
    let all = export_metadata(&p, &ExportOptions::default()).unwrap();
    assert_eq!(all.model.as_deref(), Some("Synthetic X2"));
    assert!((all.exposure_time.unwrap() - 0.004).abs() < 1e-9);
    assert!(all.gps.is_some());
    let o = ExportOptions { metadata: MetadataPolicy::AllExceptCamera, remove_location: true, ..Default::default() };
    let m = export_metadata(&p, &o).unwrap();
    assert!(m.model.is_none() && m.gps.is_none() && m.copyright.is_some());
    let c = export_metadata(&p, &ExportOptions { metadata: MetadataPolicy::Copyright, ..Default::default() }).unwrap();
    assert!(c.model.is_none() && c.gps.is_none() && c.copyright.as_deref() == Some("(c) Me"));
    // "copyright only" keeps all the copyright info: status, usage terms, info URL
    assert_eq!(
        (c.copyright_marked, c.usage_terms.as_deref(), c.copyright_url.as_deref()),
        (Some(true), Some("Editorial use only"), Some("https://example.com/rights"))
    );
    assert!(export_metadata(&p, &ExportOptions { metadata: MetadataPolicy::None, ..Default::default() }).is_none());
    // embedded and readable back from the JPEG
    let jpg = encode_with_metadata(&test_image(), &ExportOptions::default(), Some(&all)).unwrap();
    let back = lightcraft_meta::extract(&jpg);
    assert_eq!(back.model.as_deref(), Some("Synthetic X2"));
    assert_eq!(back.copyright.as_deref(), Some("(c) Me"));
    assert_eq!((back.copyright_marked, back.usage_terms.as_deref()), (Some(true), Some("Editorial use only")));
    assert_eq!(back.copyright_url.as_deref(), Some("https://example.com/rights"));
    assert!(back.gps.is_some());
}

#[test]
fn watermark_draws_in_the_anchored_corner_only() {
    let mut img = Rgba8::new(400, 300);
    for p in img.data.iter_mut() {
        *p = [0, 0, 0, 255];
    }
    let o = ExportOptions::from_json(&serde_json::json!({"watermark": {"text": "LightCraft", "size": 0.08, "opacity": 1.0, "shadow": false}}));
    let wm = o.watermark.clone().unwrap();
    draw_watermark(&mut img, &wm);
    let lit = |x0: usize, x1: usize, y0: usize, y1: usize| {
        (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter(|&(x, y)| img.get(x, y)[0] > 128).count()
    };
    assert!(lit(200, 400, 225, 300) > 100, "bottom-right has text");
    assert!(lit(0, 200, 0, 150) == 0, "top-left untouched");
    // string shorthand
    assert_eq!(ExportOptions::from_json(&serde_json::json!({"watermark": "© Me"})).watermark.unwrap().text, "© Me");
    assert!(ExportOptions::from_json(&serde_json::json!({"watermark": ""})).watermark.is_none());
}

fn vertical_japanese() -> Watermark {
    let options = ExportOptions::from_json(&json!({"watermark": {"text": "日本語", "vertical": true, "size": 0.1, "shadow": false, "opacity": 1.0}}));
    options.watermark.unwrap()
}

#[test]
fn japanese_watermark_options_and_legacy_defaults() {
    let old = ExportOptions::from_json(&json!({"watermark": {"text": "日本語"}}));
    assert!(!old.watermark.unwrap().vertical);
    let wm = vertical_japanese();
    assert!(wm.vertical);
    let round: Watermark = serde_json::from_value(serde_json::to_value(&wm).unwrap()).unwrap();
    assert!(round.vertical);
}

/// Built without craft-fonts, a Japanese watermark still lays out and draws (Inter's
/// missing-glyph boxes), and Latin text is unaffected.
#[test]
fn watermarks_work_without_craft_fonts() {
    assert_eq!(watermark_fonts(&[]).len(), 1, "Inter only");
    for wm in [vertical_japanese(), Watermark { text: "LightCraft 日本語".into(), ..Watermark::default() }] {
        let mut covered = 0usize;
        watermark_coverage(400, 300, &wm, &[], |_, _, k, _| covered += usize::from(k > 0.0));
        assert!(covered > 0, "{:?} draws something", wm.text);
    }
}

#[test]
fn japanese_watermarks_support_vertical_columns() {
    use ab_glyph::Font;
    let fonts = watermark_fonts(crate::fonts::CRAFT_FONTS);
    if fonts.len() < 2 {
        eprintln!("skipped: built without CRAFT_FONTS_DIR, so there is no Japanese watermark face");
        return;
    }
    for c in "日本語の文字".chars() {
        assert!(fonts[1..].iter().any(|f| f.glyph_id(c).0 != 0), "a craft-fonts face has {c}");
    }
    let wm = vertical_japanese();
    let mut img = Rgba8::new(400, 300);
    img.data.fill([0, 0, 0, 255]);
    draw_watermark(&mut img, &wm);
    let lit: Vec<_> = (0..300).flat_map(|y| (0..400).map(move |x| (x, y))).filter(|&(x, y)| img.get(x, y)[0] > 64).collect();
    assert!(lit.len() > 100);
    let w = lit.iter().map(|p| p.0).max().unwrap() - lit.iter().map(|p| p.0).min().unwrap();
    let h = lit.iter().map(|p| p.1).max().unwrap() - lit.iter().map(|p| p.1).min().unwrap();
    assert!(h > w * 2, "vertical Japanese must extend down the column");
    let two = Watermark { text: "日\n本".into(), anchor: Anchor::TopLeft, inset: 0.0, ..wm };
    img.data.fill([0, 0, 0, 255]);
    draw_watermark(&mut img, &two);
    for (text, dx) in [("本", 0), ("日", 30)] {
        let mut single = Rgba8::new(400, 300);
        single.data.fill([0, 0, 0, 255]);
        draw_watermark(&mut single, &Watermark { text: text.into(), ..two.clone() });
        for y in 0..30 {
            for x in 0..30 {
                assert_eq!(img.get(x + dx, y), single.get(x, y), "newlines move columns left");
            }
        }
    }
}

#[test]
fn graphic_watermark_is_placed_scaled_and_blended() {
    let dir = std::env::temp_dir().join(format!("lc-wm-logo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let logo_path = dir.join("logo.png");
    // 40×20: left half opaque red, right half fully transparent
    let logo = Rgba8::from_fn(40, 20, |x, _| if x < 20 { [255, 0, 0, 255] } else { [0, 255, 0, 0] });
    let png = lightcraft_codecs::encode_png(&EncodeImage::rgba8(&logo), &EncodeMeta::default()).unwrap();
    std::fs::write(&logo_path, png).unwrap();
    let mut img = Rgba8::new(400, 300);
    img.data.iter_mut().for_each(|p| *p = [0, 0, 0, 255]);
    let wm = Watermark { image: logo_path.to_string_lossy().into(), image_width: 0.25, opacity: 1.0, inset: 0.0, ..Default::default() };
    draw_watermark(&mut img, &wm);
    // 100 px wide × 50 px high in the bottom-right corner
    assert!(img.get(310, 280)[0] > 200, "opaque half: {:?}", img.get(310, 280));
    assert_eq!(img.get(390, 280), [0, 0, 0, 255], "transparent half leaves the photo");
    assert_eq!(img.get(310, 240), [0, 0, 0, 255], "above the logo");
    assert_eq!(img.get(10, 10), [0, 0, 0, 255]);
    // an image-only watermark survives the params (no text needed)
    let o = ExportOptions::from_json(&json!({"watermark": {"image": logo_path.to_string_lossy(), "imageWidth": 0.1}}));
    assert_eq!(o.watermark.as_ref().map(|w| w.image_width), Some(0.1));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn naming_and_params() {
    let o = ExportOptions::from_json(&serde_json::json!({"format": "jpg", "quality": 150, "longEdge": 2048, "naming": "{name}-{seq}"}));
    assert_eq!(o.format, ExportFormat::Jpeg);
    assert_eq!(o.quality, 100);
    assert_eq!(o.resize, Some(Resize { dont_enlarge: true, height: 0, ..Resize::long_edge(2048) }));
    assert_eq!(o.file_name_for(&named("IMG/1.png"), 7), "IMG_1-007.jpg");
    assert_eq!(o.ppi, 240);
    let orig = ExportOptions { format: ExportFormat::Original, naming: "{name}-{seq}".into(), ..Default::default() };
    assert_eq!(orig.file_name_for(&named("DSC_1.NEF"), 2), "DSC_1-002.NEF");
    assert_eq!(orig.file_name_for(&named("noext"), 1), "noext-001");
    assert_eq!(ExportOptions { format: ExportFormat::Dng, ..Default::default() }.file_name_for(&named("a.cr2"), 1), "a.dng");
}

#[test]
fn enhance_samples_keep_source_profile_float_encoding() {
    let profile = icc::write_matrix_trc(&li_color::DISPLAY_P3, &lightcraft_codecs::Trc::Gamma(2.0));
    let Some(icc::IccInfo { kind: icc::IccKind::MatrixTrc { to_xyz_d50, trc }, .. }) = icc::parse(&profile) else { panic!("matrix profile") };
    let source = lightcraft_codecs::SourceSpace { named: None, to_xyz_d50, trc: Some(trc), origin: lightcraft_codecs::SpaceOrigin::IccMatrixTrc };
    let options = ExportOptions { source_encoding: Some(Box::new(SourceEncoding { space: source, profile })), ..Default::default() };
    let mut image = Rgba8::from_fn(1, 1, |_, _| [50, 120, 200, 255]);
    let deep = enhanced_samples(&mut image, &options).unwrap();
    assert_eq!(deep.space, OutputSpace::Rec2020);
    let DeepSamples::F32(values) = deep.samples else { panic!("source encoding requires linear floats") };
    let expected = li_color::SRGB.to_space(&li_color::REC2020).apply([50, 120, 200].map(|v| li_color::transfer::decode_srgb8(v) as f64));
    for (actual, expected) in values.iter().zip(expected) {
        assert!((*actual as f64 - expected).abs() < 1e-6);
    }
}

#[test]
fn source_profile_encoding_converts_pixels_and_stays_out_of_presets() {
    let profile = icc::write_matrix_trc(&li_color::DISPLAY_P3, &lightcraft_codecs::Trc::Gamma(2.0));
    let Some(icc::IccInfo { kind: icc::IccKind::MatrixTrc { to_xyz_d50, trc }, .. }) = icc::parse(&profile) else { panic!("matrix profile") };
    let source = lightcraft_codecs::SourceSpace { named: None, to_xyz_d50, trc: Some(trc), origin: lightcraft_codecs::SpaceOrigin::IccMatrixTrc };
    let linear = [0.2, 0.4, 0.1];
    let working = source.to_working().apply(linear).map(|v| v as f32).to_vec();
    let image = Rgba8::new(1, 1);
    let render = lightcraft_pipeline::Rendered {
        histogram: li_raster::Histogram::of_srgb8(&image),
        image,
        deep: Some(DeepImage { width: 1, height: 1, space: OutputSpace::Rec2020, samples: DeepSamples::F32(working) }),
    };
    let options = ExportOptions {
        format: ExportFormat::Png,
        bit_depth: Some(16),
        source_encoding: Some(Box::new(SourceEncoding { space: source, profile: profile.clone() })),
        ..Default::default()
    };
    let bytes = encode_rendered(&render, &options, None).unwrap();
    let decoded = lightcraft_codecs::decode(&bytes, Default::default()).unwrap();
    assert_eq!(decoded.icc.as_deref(), Some(profile.as_slice()));
    for (value, expected) in decoded.image.data[0].iter().zip(linear) {
        assert!((*value as f64 - expected).abs() < 5e-5, "{value} != {expected}");
    }
    let params = serde_json::to_value(&options).unwrap();
    assert!(params.get("sourceEncoding").is_none());
    assert!(params.get("sourceMetadata").is_none());
    let loaded: ExportOptions = serde_json::from_value(params).unwrap();
    assert!(loaded.source_encoding.is_none());
    let old: ExportOptions = serde_json::from_value(json!({"format": "jpeg", "quality": 80})).unwrap();
    assert!(!old.same_folder);
    for (format, bit_depth, expected_depth) in
        [(ExportFormat::Tiff, None, 16), (ExportFormat::Png, Some(32), 16), (ExportFormat::Jpeg, Some(16), 8), (ExportFormat::Webp, Some(32), 8)]
    {
        let options = ExportOptions { format, bit_depth, ..options.clone() };
        let bytes = encode_rendered(&render, &options, None).unwrap();
        let decoded = lightcraft_codecs::decode(&bytes, Default::default()).unwrap();
        assert_eq!(decoded.bit_depth, expected_depth, "{format:?} {bit_depth:?}");
    }
}

fn named(file_name: &str) -> lightcraft_catalog::Photo {
    use lightcraft_catalog::{Photo, PhotoId, Source};
    Photo::new(PhotoId(1), Source::Demo { scene: 0 }, file_name, "", 1, 1, "2026-01-01T00:00:00")
}

/// Export naming takes the same tokens as Rename Photos and import renaming.
#[test]
fn naming_uses_the_rename_tokens() {
    let mut p = named("DSC_0815.NEF");
    p.captured = Some("2026-09-30T14:05:09".into());
    p.meta.camera = "Model X/2".into();
    p.meta.title = "Harbour".into();
    p.rating = 5;
    let o = |naming: &str| ExportOptions { naming: naming.into(), start_number: 9, ..Default::default() };
    assert_eq!(o("{date:%Y-%m-%d}_{title}_{seq:2}").file_name_for(&p, 1), "2026-09-30_Harbour_09.jpg");
    assert_eq!(o("{camera}-{num}-{rating}").file_name_for(&p, 1), "Model X_2-0815-5.jpg", "path separators become _");
    assert_eq!(o("{name}-{seq}").file_name_for(&p, 3), "DSC_0815-011.jpg", "a bare {{seq}} keeps its 3 digits");
    assert_eq!(o("{ext}_{name}").file_name_for(&p, 1), "NEF_DSC_0815.jpg");
    // nothing left after expanding: the original name
    p.meta.title.clear();
    assert_eq!(o("{title}").file_name_for(&p, 1), "DSC_0815.jpg");
}

#[test]
fn params_round_trip() {
    let o = ExportOptions {
        format: ExportFormat::Tiff,
        quality: 70,
        resize: Some(Resize { mode: ResizeMode::Megapixels, value: 12.5, height: 0, dont_enlarge: false }),
        ppi: 300,
        limit_kb: Some(500),
        sharpen: SharpenFor::Glossy,
        naming: "{date}-{name}-{seq}".into(),
        start_number: 42,
        subfolder: "Client".into(),
        conflict: Conflict::Skip,
        tiff_compression: TiffCompression::Lzw,
        metadata: MetadataPolicy::Copyright,
        remove_location: true,
        watermark: Some(Watermark { text: "© LC".into(), ..Default::default() }),
        color_space: OutputSpace::ProPhoto,
        bit_depth: Some(16),
        ..Default::default()
    };
    assert_eq!(ExportOptions::from_json(&o.to_json()), o);
    let full = ExportOptions::default();
    assert_eq!(ExportOptions::from_json(&full.to_json()), full);
    assert!(ExportOptions::has_size_param(&full.to_json()), "full size is explicit");
    let mut p = named("IMG.png");
    p.captured = Some("2026-09-30T10:00:00".into());
    assert_eq!(o.file_name_for(&p, 2), "20260930-IMG-043.tif");
}

#[test]
fn resize_modes() {
    let size = |p: serde_json::Value, w: f64, h: f64| ExportOptions::from_json(&p).resize.map(|r| r.apply(w, h));
    // a 6000 × 4000 landscape and a 4000 × 6000 portrait
    assert_eq!(size(json!({"longEdge": 1500}), 6000.0, 4000.0), Some((1500, 1000)));
    assert_eq!(size(json!({"longEdge": 1500}), 4000.0, 6000.0), Some((1000, 1500)));
    assert_eq!(size(json!({"shortEdge": 1080}), 6000.0, 4000.0), Some((1620, 1080)));
    assert_eq!(size(json!({"width": 3000}), 4000.0, 6000.0), Some((3000, 4500)));
    assert_eq!(size(json!({"height": 1000}), 6000.0, 4000.0), Some((1500, 1000)));
    // W × H fits either orientation: the long edge gets the larger number
    assert_eq!(size(json!({"width": 1000, "height": 800}), 6000.0, 4000.0), Some((1000, 667)));
    assert_eq!(size(json!({"width": 1000, "height": 800}), 4000.0, 6000.0), Some((667, 1000)));
    assert_eq!(size(json!({"width": 1000, "height": 500}), 6000.0, 4000.0), Some((750, 500)));
    let (w, h) = size(json!({"megapixels": 6}), 6000.0, 4000.0).unwrap();
    assert!(((w * h) as f64 - 6e6).abs() < 6e6 * 0.002, "{w}×{h}");
    assert_eq!(size(json!({"percent": 25}), 6000.0, 4000.0), Some((1500, 1000)));
    // don't enlarge (default) vs enlarge
    assert_eq!(size(json!({"longEdge": 8000}), 6000.0, 4000.0), Some((6000, 4000)));
    assert_eq!(size(json!({"longEdge": 9000, "dontEnlarge": false}), 6000.0, 4000.0), Some((9000, 6000)));
    // full size / absent
    assert_eq!(size(json!({"longEdge": 0}), 6000.0, 4000.0), None);
    assert_eq!(size(json!({}), 6000.0, 4000.0), None);
    assert!(ExportOptions::has_size_param(&json!({"longEdge": 0})) && !ExportOptions::has_size_param(&json!({"quality": 3})));
    // the `resize` object form round-trips (what the dialog and prefs store)
    let r = Resize { mode: ResizeMode::ShortEdge, value: 720.0, height: 0, dont_enlarge: false };
    let back = ExportOptions::from_json(&ExportOptions::resize_json(Some(&r))).resize;
    assert_eq!(back, Some(r));
    assert_eq!(ExportOptions::from_json(&ExportOptions::resize_json(None)).resize, None);
    assert_eq!(ExportOptions::from_json(&json!({"ppi": 300})).ppi, 300);
    // never larger than the encoders' limit, never empty
    assert_eq!(size(json!({"percent": 10000, "dontEnlarge": false}), 6000.0, 4000.0), Some((27713, 18475)), "512 MP at most");
    assert_eq!(size(json!({"width": 70000, "dontEnlarge": false}), 60000.0, 100.0), Some((65535, 109)), "65535 px an edge");
    assert_eq!(size(json!({"width": 1}), 6000.0, 40.0).map(|s| s.1), Some(1));
}

/// A flat field of a saturated green inside Display P3 but outside sRGB, rendered into `space`.
fn p3_green(space: OutputSpace) -> Rgba8 {
    use li_color::{DISPLAY_P3, REC2020};
    let c = DISPLAY_P3.to_space(&REC2020).apply_f32([0.04, 0.45, 0.04]);
    let src = li_raster::RgbFrame::filled(16, 16, c);
    let req = lightcraft_pipeline::RenderRequest { space, ..lightcraft_pipeline::RenderRequest::fit(16, 16) };
    lightcraft_pipeline::render(&src, &Default::default(), &Default::default(), &req).image
}

/// Decode `bytes` and return the centre pixel in linear sRGB primaries (unclamped), plus the
/// recognised space of the embedded profile.
fn decoded_in_srgb(bytes: &[u8]) -> ([f32; 3], Option<NamedSpace>) {
    let d = lightcraft_codecs::decode(bytes, Default::default()).expect("decodes");
    let m = d.space.to_space(&li_color::SRGB);
    (m.apply_f32(d.image.get(8, 8)), d.space.named)
}

#[test]
fn p3_colour_survives_a_p3_export_and_is_clipped_in_srgb() {
    let o = |space| ExportOptions { format: ExportFormat::Png, color_space: space, ..Default::default() };
    let (p3, named) = decoded_in_srgb(&encode_image(&p3_green(OutputSpace::DisplayP3), &o(OutputSpace::DisplayP3)).unwrap());
    assert_eq!(named, Some(NamedSpace::DisplayP3));
    assert!(p3[0] < -0.03 || p3[2] < -0.03, "outside sRGB after a P3 round trip: {p3:?}");
    let (s, named) = decoded_in_srgb(&encode_image(&p3_green(OutputSpace::Srgb), &o(OutputSpace::Srgb)).unwrap());
    assert_eq!(named, Some(NamedSpace::Srgb));
    assert!(s.iter().all(|v| *v > -0.002), "{s:?}");
    // the green itself is about the same brightness either way
    assert!((p3[1] - s[1]).abs() < 0.15, "{p3:?} {s:?}");
}

#[test]
fn every_space_embeds_its_own_profile_and_round_trips() {
    let grey = {
        let src = li_raster::RgbFrame::filled(16, 16, [0.18; 3]);
        let base = lightcraft_pipeline::RenderRequest::fit(16, 16);
        move |space| {
            lightcraft_pipeline::render(&src, &Default::default(), &Default::default(), &lightcraft_pipeline::RenderRequest { space, ..base.clone() })
                .image
        }
    };
    let reference =
        decoded_in_srgb(&encode_image(&grey(OutputSpace::Srgb), &ExportOptions { format: ExportFormat::Tiff, ..Default::default() }).unwrap()).0;
    for space in OutputSpace::ALL {
        for format in [ExportFormat::Jpeg, ExportFormat::Png, ExportFormat::Tiff, ExportFormat::Webp] {
            let o = ExportOptions { format, color_space: space, ..Default::default() };
            let bytes = encode_image(&grey(space), &o).unwrap();
            let (px, named) = decoded_in_srgb(&bytes);
            assert_eq!(named, Some(named_space(space)), "{space:?} {format:?}");
            // a neutral grey decodes to the same linear value whatever the output space
            for c in 0..3 {
                assert!((px[c] - reference[c]).abs() < 0.01, "{space:?} {format:?}: {px:?} vs {reference:?}");
            }
        }
        let icc = icc::write_named(named_space(space));
        let info = icc::parse(&icc).expect("our profile parses");
        assert_eq!(info.named, Some(named_space(space)));
        assert!(info.description.as_deref().is_some_and(|d| !d.is_empty()));
    }
    let o = ExportOptions { format: ExportFormat::Avif, color_space: OutputSpace::ProPhoto, ..Default::default() };
    assert_eq!(o.effective_space(), OutputSpace::Srgb);
    let o = ExportOptions::from_json(&serde_json::json!({"colorSpace": "displayP3"}));
    assert_eq!(o.color_space, OutputSpace::DisplayP3);
    assert_eq!(ExportOptions::from_json(&serde_json::json!({"colorSpace": "Adobe RGB (1998) compatible"})).color_space, OutputSpace::AdobeRgb);
}

#[test]
fn srgb_watermark_colour_is_converted() {
    assert_eq!(srgb8_in(OutputSpace::Srgb, [10, 200, 30]), [10, 200, 30]);
    for s in OutputSpace::ALL {
        assert_eq!(srgb8_in(s, [255, 255, 255]), [255, 255, 255], "{s:?}");
        assert_eq!(srgb8_in(s, [0, 0, 0]), [0, 0, 0], "{s:?}");
    }
    let p3 = srgb8_in(OutputSpace::DisplayP3, [255, 0, 0]);
    assert!(p3[0] < 255 && p3[1] > 0, "{p3:?}");
}

/// A shallow horizontal grey ramp (few 8-bit levels), rendered at `depth`.
fn ramp(depth: OutputDepth, space: OutputSpace) -> lightcraft_pipeline::Rendered {
    let src = li_raster::RgbFrame::from_fn(1024, 4, |x, _| [0.10 + 0.03 * x as f32 / 1023.0; 3]);
    let req = lightcraft_pipeline::RenderRequest { depth, space, ..lightcraft_pipeline::RenderRequest::fit(1024, 4) };
    lightcraft_pipeline::render(&src, &Default::default(), &Default::default(), &req)
}

fn distinct_levels(bytes: &[u8]) -> (usize, lightcraft_codecs::Decoded) {
    let d = lightcraft_codecs::decode(bytes, Default::default()).expect("decodes");
    let mut v: Vec<u32> = (0..d.image.width).map(|x| d.image.get(x, 1)[1].to_bits()).collect();
    // Dither alternates adjacent levels: count distinct values, not transitions.
    v.sort_unstable();
    v.dedup();
    (v.len(), d)
}

#[test]
fn sixteen_bit_tiff_and_png_have_no_banding() {
    let tiff = |bit_depth| ExportOptions { format: ExportFormat::Tiff, bit_depth, ..Default::default() };
    assert_eq!(tiff(None).effective_depth(), OutputDepth::U16, "TIFF defaults to 16-bit");
    let r16 = ramp(OutputDepth::U16, OutputSpace::Srgb);
    assert!(matches!(r16.deep.as_ref().unwrap().samples, DeepSamples::U16(_)));
    let (levels16, d16) = distinct_levels(&encode_rendered(&r16, &tiff(None), None).unwrap());
    let r8 = ramp(OutputDepth::U8, OutputSpace::Srgb);
    let (levels8, _) = distinct_levels(&encode_rendered(&r8, &tiff(Some(8)), None).unwrap());
    eprintln!("gradient levels: 16-bit TIFF {levels16}, 8-bit TIFF {levels8}");
    assert!(levels8 < 40, "{levels8}");
    assert!(levels16 > 10 * levels8, "16-bit: {levels16} levels vs 8-bit: {levels8}");
    assert_eq!(d16.space.named, Some(NamedSpace::Srgb));
    // the 8-bit preview of a deep render matches the 8-bit render
    for (a, b) in r16.image.data.iter().zip(&r8.image.data) {
        assert!(a[1].abs_diff(b[1]) <= 1, "{a:?} {b:?}");
    }
    let png = ExportOptions { format: ExportFormat::Png, bit_depth: Some(16), color_space: OutputSpace::DisplayP3, ..Default::default() };
    let rp = ramp(OutputDepth::U16, OutputSpace::DisplayP3);
    let bytes = encode_rendered(&rp, &png, None).unwrap();
    assert_eq!(bytes[24], 16, "PNG IHDR bit depth");
    let (levels, d) = distinct_levels(&bytes);
    assert!(levels > 10 * levels8, "{levels}");
    assert_eq!(d.space.named, Some(NamedSpace::DisplayP3));
    // sharpening and watermarking work on deep images too
    let o =
        ExportOptions { sharpen: SharpenFor::Matte, watermark: Some(Watermark { text: "LC".into(), size: 0.5, ..Default::default() }), ..tiff(None) };
    assert!(encode_rendered(&r16, &o, None).is_ok());
}

#[test]
fn float_tiff_is_linear_with_a_linear_profile() {
    let o = ExportOptions { format: ExportFormat::Tiff, bit_depth: Some(32), color_space: OutputSpace::ProPhoto, ..Default::default() };
    assert_eq!(o.effective_depth(), OutputDepth::F32Linear);
    let rf = ramp(OutputDepth::F32Linear, OutputSpace::ProPhoto);
    let (levels, df) = distinct_levels(&encode_rendered(&rf, &o, None).unwrap());
    assert!(levels > 500, "{levels}");
    assert_eq!(df.space.named, Some(NamedSpace::ProPhoto));
    assert!(df.space.trc.as_ref().is_some_and(|t| t[0].is_linear()), "{:?}", df.space.trc);
    // decodes to the same light as the 16-bit gamma-encoded export
    let o16 = ExportOptions { bit_depth: Some(16), ..o.clone() };
    let (_, d16) = distinct_levels(&encode_rendered(&ramp(OutputDepth::U16, OutputSpace::ProPhoto), &o16, None).unwrap());
    for x in [0, 300, 700, 1023] {
        let (a, b) = (df.image.get(x, 1)[1], d16.image.get(x, 1)[1]);
        assert!((a - b).abs() < 2e-4, "{x}: {a} vs {b}");
    }
}

#[test]
fn bit_depth_options_per_format() {
    let d = |format, bit_depth| ExportOptions { format, bit_depth, ..Default::default() }.effective_depth();
    assert_eq!(d(ExportFormat::Jpeg, Some(16)), OutputDepth::U8);
    assert_eq!(d(ExportFormat::Webp, None), OutputDepth::U8);
    assert_eq!(d(ExportFormat::Png, None), OutputDepth::U8);
    assert_eq!(d(ExportFormat::Png, Some(16)), OutputDepth::U16);
    assert_eq!(d(ExportFormat::Avif, Some(10)), OutputDepth::U16);
    assert_eq!(ExportOptions::from_json(&serde_json::json!({"bitDepth": 16})).bit_depth, Some(16));
    assert_eq!(ExportOptions::from_json(&serde_json::json!({"bitDepth": 12})).bit_depth, None);
    for f in [ExportFormat::Jpeg, ExportFormat::Png, ExportFormat::Tiff, ExportFormat::Webp, ExportFormat::Avif] {
        let first = ExportOptions::bit_depths(f)[0].0;
        let def = d(f, None);
        assert_eq!(d(f, Some(first)), def, "{f:?}: the first choice is the default");
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn avif_ten_bit() {
    let o = ExportOptions { format: ExportFormat::Avif, bit_depth: Some(10), quality: 60, ..Default::default() };
    let r = ramp(OutputDepth::U16, OutputSpace::Srgb);
    match encode_rendered(&r, &o, None) {
        Ok(b) => assert_eq!(&b[4..8], b"ftyp"),
        Err(e) => assert!(e.contains("not available"), "{e}"),
    }
}

#[test]
fn ds2_original_export_borrows_photo_unless_people_keywords_are_requested() {
    let photo = lightcraft_catalog::Photo::new(
        lightcraft_catalog::PhotoId(1),
        lightcraft_catalog::Source::Demo { scene: 1 },
        "fixture.jpg",
        "JPEG",
        8,
        8,
        "2026-10-10",
    );
    let borrowed = photo_for_sidecar(&photo, false, false);
    assert!(matches!(borrowed, std::borrow::Cow::Borrowed(_)));
    assert!(std::ptr::eq(borrowed.as_ref(), &photo));
    let augmented = photo_for_sidecar(&photo, true, false);
    assert!(matches!(augmented, std::borrow::Cow::Owned(_)));
    assert_eq!(augmented.meta.keywords, photo.meta.keywords);
}

#[test]
fn dsfix_original_export_rejects_embedded_places_without_gps() {
    for field in 0..4 {
        let mut m = lightcraft_meta::Metadata::default();
        [&mut m.sublocation, &mut m.city, &mut m.state, &mut m.country][field].replace("Private place".into());
        let jpg = encode_with_metadata(&Rgba8 { width: 1, height: 1, data: vec![[0, 0, 0, 255]] }, &ExportOptions::default(), Some(&m)).unwrap();
        assert!(original_embeds_location(&jpg), "field {field}");
    }
    assert!(!original_embeds_location(b"original without location"));
}

#[test]
fn a_panic_in_progress_does_not_hang_the_lanes() {
    let mut session = crate::Session::with_demo();
    let o = ExportOptions::from_json(&json!({"format": "png", "width": 40, "height": 40}));
    // more photos than the pipeline window, so the feeder is waiting for the writer to move on
    let items = rendered_batch(&mut session, 3 * parallel().window, &o);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let to = Destination { dir: "/exports".into(), exact: None };
        let mut calls = 0;
        let r = crate::guard::catch("test batch", || {
            run_batch_shaped(
                items,
                &o,
                &to,
                &mut |_, _| Ok(()),
                &|_| false,
                false,
                &mut |_, _| {
                    calls += 1;
                    assert!(calls < 2, "progress panics on its second call");
                    true
                },
                &lightcraft_sysmem::JobCtl::new(),
                &wide_open(),
                parallel(),
            )
        });
        tx.send(r.and_then(|r| r)).ok();
    });
    let r = rx.recv_timeout(std::time::Duration::from_secs(60));
    assert!(matches!(r, Ok(Err(_))), "the batch returns an error instead of hanging: {r:?}");
}

/// Typing 20000 for 2000 (or percent 10000) with Don't Enlarge off asked for gigapixels, whose
/// first plane failed to allocate and aborted the process. An enlargement stops at 512
/// megapixels; full-size and smaller exports of photos larger than that are untouched.
#[test]
fn enlargements_stop_at_512_megapixels() {
    let size = |p: serde_json::Value, w: f64, h: f64| ExportOptions::from_json(&p).resize.map(|r| r.apply(w, h)).unwrap();
    for p in [json!({"longEdge": 60000, "dontEnlarge": false}), json!({"megapixels": 5000}), json!({"percent": 10000, "dontEnlarge": false})] {
        let (w, h) = size(p.clone(), 6000.0, 4000.0);
        assert!((w * h) as f64 <= MAX_ENLARGED_PIXELS * 1.001, "{p}: {w}×{h}");
        assert!((w as f64 / h as f64 - 1.5).abs() < 0.001, "{p}: keeps the shape");
    }
    assert_eq!(size(json!({"longEdge": 12000, "dontEnlarge": false}), 6000.0, 4000.0), (12000, 8000), "below the ceiling as asked");
    assert_eq!(size(json!({"percent": 100, "dontEnlarge": false}), 30000.0, 20000.0), (30000, 20000), "a 600 MP panorama at full size");
    assert_eq!(size(json!({"percent": 50}), 30000.0, 20000.0), (15000, 10000));
    assert_eq!(size(json!({"percent": 200, "dontEnlarge": false}), 30000.0, 20000.0), (30000, 20000), "never enlarged");
}

/// An enlarged export that needs more memory than the computer has is refused with a message
/// before it starts; everything else goes ahead.
#[test]
fn enlarged_exports_must_fit_the_computers_memory() {
    let gb = 1u64 << 30;
    let ram = |g: u64| Some((g * gb) as usize);
    let why = prepared::too_big_for_memory((27713, 18475), 24_000_000, 20 * gb, ram(16)).unwrap();
    assert!(why.contains("27713 × 18475") && why.contains("20 GB") && why.contains("16 GB") && why.contains("smaller size"), "{why}");
    assert_eq!(prepared::too_big_for_memory((27713, 18475), 24_000_000, 20 * gb, ram(32)), None, "fits");
    assert_eq!(prepared::too_big_for_memory((6000, 4000), 24_000_000, 20 * gb, ram(1)), None, "full size is never refused");
    assert_eq!(prepared::too_big_for_memory((3000, 2000), 24_000_000, 20 * gb, ram(1)), None, "nor a smaller export");
    assert_eq!(prepared::too_big_for_memory((27713, 18475), 24_000_000, 20 * gb, None), None, "unknown memory");
}

/// A photo more than 65535 times as long as it is short (a 70000 × 1 strip) made `scale` clamp
/// with its lower bound above its upper one, which panics, in the export and in the Export
/// dialog's preview on the UI thread. The encoders' limit wins, the short edge keeps a pixel.
#[test]
fn resize_survives_photos_too_long_for_the_encoders() {
    let modes = [
        json!({"longEdge": 2000}),
        json!({"shortEdge": 1}),
        json!({"width": 500, "height": 500}),
        json!({"megapixels": 1}),
        json!({"percent": 50}),
        json!({"percent": 1000, "dontEnlarge": false}),
    ];
    for p in modes {
        for (w, h) in [(70_000.0, 1.0), (1.0, 70_000.0), (1e9, 3.0), (65_536.0, 1.0)] {
            let r = ExportOptions::from_json(&p).resize.unwrap();
            let (ow, oh) = r.apply(w, h);
            assert!((1..=65_535).contains(&ow) && (1..=65_535).contains(&oh), "{p} on {w}×{h}: {ow}×{oh}");
            assert!(r.scale(w, h).is_finite());
        }
    }
    assert_eq!(Resize::long_edge(2000).apply(70_000.0, 1.0), (65_535, 1));
    assert_eq!(Resize::long_edge(2000).apply(1.0, 70_000.0), (1, 65_535));
}
