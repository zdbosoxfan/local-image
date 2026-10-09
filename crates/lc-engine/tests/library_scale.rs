//! Reproducible CPU-only event-scale benchmark. See docs/wip/CODEX-REPORT-library-scale.md.
//! Run in release mode with --ignored --nocapture --test-threads=1.
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use lightcraft_catalog::{Catalog, Filter, Op, Photo, PhotoId, Sort, SortKey, Source};
use lightcraft_engine::{Session, export, import};
use lightcraft_preview::JobPool;
use lightcraft_raw::{BlackLevel, Cfa, DngCompression, DngWriteOptions, RawData, RawFormat, RawImage, Rect};
use serde_json::json;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn scratch(label: &str) -> Scratch {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.library-scale-artifacts");
    fs::create_dir_all(&base).unwrap();
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    for attempt in 0.. {
        let path = base.join(format!("library-scale-{label}-{}-{nonce}-{attempt}", std::process::id()));
        match fs::create_dir(&path) {
            Ok(()) => return Scratch(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => panic!("cannot create benchmark scratch directory"),
        }
    }
    unreachable!()
}

fn rss_kib() -> usize {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("VmRSS:")).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse().ok()))
        .unwrap_or(0)
}
static PHASE_PEAK: AtomicUsize = AtomicUsize::new(0);
fn phase_peak_rss_kib() -> usize {
    let current = rss_kib();
    PHASE_PEAK.fetch_max(current, Ordering::Relaxed).max(current)
}
struct StopSample<'a>(&'a AtomicBool);
impl Drop for StopSample<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
fn measure<T>(row: &str, n: usize, f: impl FnOnce() -> T) -> T {
    let stop = AtomicBool::new(false);
    let peak = &PHASE_PEAK;
    peak.store(rss_kib(), Ordering::Relaxed);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !stop.load(Ordering::Relaxed) {
                peak.fetch_max(rss_kib(), Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let stop_on_unwind = StopSample(&stop);
        let start = Instant::now();
        let value = f();
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        peak.fetch_max(rss_kib(), Ordering::Relaxed);
        drop(stop_on_unwind);
        eprintln!("library_bench,row={row},n={n},ms={ms:.3},peak_rss_kib={}", peak.load(Ordering::Relaxed));
        value
    })
}
fn count() -> usize {
    std::env::var("LIBRARY_BENCH_N").ok().and_then(|s| s.parse().ok()).unwrap_or(2000).max(1)
}
fn enabled(row: &str) -> bool {
    std::env::var("LIBRARY_BENCH_CASE").map_or(true, |s| s.split(',').any(|x| x == row))
}

// Add a standalone JPEG IFD after the DNG made by the production writer. The raw samples
// remain untouched. Classic little-endian TIFF; the original writer has one root IFD.
fn embed(mut dng: Vec<u8>, jpeg: &[u8], w: u32, h: u32) -> Vec<u8> {
    let root = u32::from_le_bytes(dng[4..8].try_into().unwrap()) as usize;
    let entries = u16::from_le_bytes(dng[root..root + 2].try_into().unwrap()) as usize;
    let next = root + 2 + entries * 12;
    let at = dng.len() as u32;
    dng[next..next + 4].copy_from_slice(&at.to_le_bytes());
    let jpeg_at = at + 2 + 6 * 12 + 4;
    dng.extend_from_slice(&6u16.to_le_bytes());
    for (tag, ty, val) in [(254u16, 4u16, 1u32), (256, 4, w), (257, 4, h), (259, 3, 6), (513, 4, jpeg_at), (514, 4, jpeg.len() as u32)] {
        dng.extend_from_slice(&tag.to_le_bytes());
        dng.extend_from_slice(&ty.to_le_bytes());
        dng.extend_from_slice(&1u32.to_le_bytes());
        dng.extend_from_slice(&val.to_le_bytes());
    }
    dng.extend_from_slice(&0u32.to_le_bytes());
    dng.extend_from_slice(jpeg);
    dng
}
fn generate(dir: &Path, n: usize) {
    let edge: usize = std::env::var("LIBRARY_BENCH_RAW_WIDTH").ok().and_then(|s| s.parse().ok()).unwrap_or(6000).clamp(32, 8192);
    let height = edge * 2 / 3;
    let cfa = Cfa::bayer("RGGB").unwrap();
    let raw = RawImage {
        format: RawFormat::Dng,
        width: edge,
        height,
        cpp: 1,
        data: RawData::U16(
            (0..edge * height)
                .map(|i| {
                    let (x, y) = (i % edge, i / edge);
                    let channel = cfa.color_at(x, y) as usize;
                    (256 + [7200, 10000, 5200][channel] * (100 + (x * 7 / edge + y * 11 / height) % 60) / 160) as u16
                })
                .collect(),
        ),
        cfa: Some(cfa),
        bits: 14,
        black: BlackLevel::uniform(256.0),
        white: vec![16383.0],
        active_area: Rect::new(0, 0, edge, height),
        crop: Rect::new(0, 0, edge, height),
        orientation: Default::default(),
        color: Default::default(),
        wb_multipliers: None,
        linearized: false,
        opcodes: Default::default(),
        metadata: lightcraft_raw::Metadata { make: Some("Synthetic".into()), model: Some("Event Camera".into()), ..Default::default() },
    };
    let preview = lightcraft_raster::Rgba8::from_fn(1536, 1024, |x, y| [(x * 255 / 1536) as u8, (y * 255 / 1024) as u8, 100, 255]);
    let jpeg = lightcraft_preview::encode_jpeg(&preview).unwrap();
    let full_jpeg = {
        let image = lightcraft_raster::Rgba8::from_fn(edge, height, |x, y| {
            let seed = (x as u32).wrapping_mul(0x9e3779b9) ^ (y as u32).wrapping_mul(0x85ebca6b);
            let noise = ((seed ^ (seed >> 16)) & 31) as i32 - 16;
            let channel = |base: i32| (base + noise).clamp(0, 255) as u8;
            [channel((x * 255 / edge) as i32), channel((y * 255 / height) as i32), channel(100), 255]
        });
        lightcraft_preview::encode_jpeg(&image).unwrap()
    };

    let bytes = lightcraft_raw::write_dng(&raw, &DngWriteOptions { compression: DngCompression::Uncompressed, ..Default::default() }).unwrap();
    let dng = embed(bytes, &jpeg, 1536, 1024);
    assert_eq!(lightcraft_raw::embedded_preview(&dng).as_deref(), Some(jpeg.as_slice()));
    let template = dir.join("template.dng");
    fs::write(&template, &dng).unwrap();
    for i in 0..n {
        let path = dir.join(format!("synthetic-{i:05}.{}", if i % 5 == 0 { "jpg" } else { "dng" }));
        if i % 5 == 0 {
            fs::write(&path, &full_jpeg).unwrap();
        } else {
            fs::copy(&template, &path).unwrap();
        }
        OpenOptions::new().append(true).open(path).unwrap().write_all(&(i as u64).to_le_bytes()).unwrap();
    }
    fs::remove_file(template).unwrap();
}
fn synthetic_catalog(n: usize) -> Catalog {
    let mut c = Catalog::new();
    for i in 0..n {
        let mut p = Photo::new(
            PhotoId(i as u64 + 1),
            Source::Demo { scene: 0 },
            &format!("Event_{:05}.ARW", (i * 7919) % n),
            "ARW",
            6000,
            4000,
            "2026-01-01T00:00:00",
        );
        p.rating = (i % 6) as u8;
        p.meta.camera = "Sony Event Camera".into();
        p.meta.title = format!("Conference speaker {}", i % 17);
        c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    c
}
fn cache_rewrites() {
    let dir = scratch("rewrite-cache");
    let image = lightcraft_raster::Rgba8::from_fn(64, 48, |x, y| [(x * 3) as u8, (y * 5) as u8, 80, 255]);
    let bytes = lightcraft_preview::encode_jpeg(&image).unwrap().len() as u64;
    let cache = lightcraft_preview::DiskCache::new(&dir.0, bytes * 25);
    for i in 0..20 {
        cache.put(lightcraft_preview::Hasher128::new().u64(i).finish(), &image);
    }
    measure("disk_cache_rewrites", 2000, || {
        for i in 0..2000 {
            cache.put(lightcraft_preview::Hasher128::new().u64(i % 20).finish(), &image);
        }
    });
}
fn queries() {
    for n in [2000, 20000] {
        let c = synthetic_catalog(n);
        for (name, f, s) in [
            ("filter", Filter { rating: 3, ..Default::default() }, Sort::default()),
            ("filter_ids", Filter { only: (1..=n as u64).step_by(2).map(PhotoId).collect(), ..Default::default() }, Sort::default()),
            ("sort", Filter::default(), Sort { key: SortKey::FileName, ..Default::default() }),
            ("search", Filter { text: "conference camera:sony rating:>2".into(), ..Default::default() }, Sort::default()),
        ] {
            measure(name, n, || {
                for _ in 0..20 {
                    std::hint::black_box(c.query(&f, &s));
                }
            });
        }
    }
}
fn thumbnails(s: &mut Session, ids: &[PhotoId], quick: bool) {
    let start = Instant::now();
    let mut first = false;
    let mut next = 0;
    let mut done = 0;
    let visible = &ids[..ids.len().min(24)];
    let mut visible_done = 0;
    let mut pool = JobPool::new(4);
    let mut digests = Vec::with_capacity(ids.len());
    while done < ids.len() {
        while next < ids.len() && next - done < 12 {
            let id = ids[next];
            let job = s.thumb_job(id, 256).unwrap();
            let q = quick.then(|| s.quick_thumb_job(&job)).flatten();
            pool.submit(
                id,
                job.key,
                if next < 24 { 10 } else { 5 },
                Box::new(move || lightcraft_engine::memory::in_background(|| q.map_or_else(|| job.run(), |q| q.run()))),
            );
            next += 1;
        }
        if let Some(r) = pool.try_recv() {
            assert!(r.result.rendered.is_ok(), "thumbnail failed (source paths suppressed)");
            let image = &r.result.rendered.as_ref().unwrap().image;
            digests.push((r.slot, lightcraft_preview::hash_bytes(image.data.as_flattened())));
            done += 1;
            if quick && visible.contains(&r.slot) {
                visible_done += 1;
                if visible_done == visible.len() {
                    eprintln!(
                        "library_bench,row=first_visible_screen,n={},ms={:.3},peak_rss_kib={}",
                        visible.len(),
                        start.elapsed().as_secs_f64() * 1000.0,
                        phase_peak_rss_kib()
                    );
                }
            }
            if !first {
                eprintln!(
                    "library_bench,row=first_{}_thumbnail,n={},ms={:.3},peak_rss_kib={}",
                    if quick { "visible" } else { "developed" },
                    ids.len(),
                    start.elapsed().as_secs_f64() * 1000.0,
                    phase_peak_rss_kib()
                );
                first = true;
            }
        } else {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    digests.sort_by_key(|(id, _)| *id);
    let mut hash = lightcraft_preview::Hasher128::new();
    for (_, digest) in digests {
        hash.str(&digest.to_string());
    }
    eprintln!("library_bench,row=thumbnail_digest,quick={quick},n={},hash={}", ids.len(), hash.finish());
}
#[test]
#[ignore = "release CPU benchmark; creates ~77 GB of temporary 24 MP DNGs by default"]
fn library_scale() {
    assert!(!cfg!(debug_assertions), "run with --release");
    lightcraft_engine::gpu::set_enabled(false);
    if enabled("queries") {
        cache_rewrites();
        queries();
    }
    if std::env::var("LIBRARY_BENCH_CASE").is_ok_and(|s| s == "queries") {
        return;
    }
    let files = scratch("files");
    let library = scratch("catalog");
    let output = scratch("export");
    let root = if let Some(real) = std::env::var_os("LIBRARY_BENCH_DIR") {
        PathBuf::from(real)
    } else if let Some(existing) = std::env::var_os("LIBRARY_BENCH_SYNTHETIC_DIR") {
        PathBuf::from(existing)
    } else {
        generate(&files.0, count());
        files.0.clone()
    };
    // Only the generated scratch directories above are ever removed. Originals are referenced
    // read-only: Add mode, no renaming, copy/move, XMP writes, or output beside the source.
    let paths = import::expand(&[root.to_string_lossy().into_owned()], None);
    let paths: Vec<_> = paths.into_iter().take(count()).collect();
    assert!(!paths.is_empty(), "no supported inputs");
    let mut s = Session::new().with_fs();
    assert!(s.open_library(&library.0, false).is_ok());
    s.xmp.auto_write = false;
    let report = measure("import", paths.len(), || import::import(&mut s, &paths, import::ImportMode::Add));
    assert!(report.is_ok(), "import failed (source paths suppressed)");
    let report = report.unwrap();
    assert!(report.failed.is_empty(), "some inputs failed to import (source paths suppressed)");
    let ids: Vec<_> = report.imported.iter().copied().map(PhotoId).collect();
    assert!(!ids.is_empty());
    if enabled("thumbnails") {
        measure("visible_thumbnails", ids.len(), || thumbnails(&mut s, &ids, true));
        measure("all_developed_thumbnails", ids.len(), || thumbnails(&mut s, &ids, false));
        s.media.clear_sources();
        while s.media.rendered.evict_oldest().is_some() {}
        let hits_before = s.media.rendered.disk().unwrap().hits.load(Ordering::Relaxed);
        measure("disk_thumbnail_reuse", ids.len(), || thumbnails(&mut s, &ids, false));
        assert_eq!(
            s.media.rendered.disk().unwrap().hits.load(Ordering::Relaxed) - hits_before,
            ids.len() as u64,
            "expected disk reuse for every thumbnail"
        );
    }
    if enabled("smart") {
        let r = measure("smart_previews", ids.len(), || s.execute("library.smartPreviews", &json!({"ids":report.imported})));
        assert!(r.as_ref().is_ok_and(|v| v["failed"].as_array().is_some_and(Vec::is_empty)), "smart preview failed (source paths suppressed)");
    }
    if enabled("export") {
        let selected = &ids[..ids.len().min(200)];
        let opts = export::ExportOptions::from_json(&json!({"format":"jpeg","longEdge":2048}));
        let to = export::Destination { dir: output.0.to_string_lossy().into_owned(), exact: None };
        let r = measure("export", selected.len(), || {
            export::export_batch(&mut s, selected, &opts, &to, &mut export::write_file, &|p| Path::new(p).exists())
        });
        assert!(r.as_ref().is_ok_and(|v| v.iter().all(|v| v["path"].is_string())), "export failed (source paths suppressed)");
        let mut hash = lightcraft_preview::Hasher128::new();
        for file in r.unwrap() {
            let mut bytes = fs::read(file["path"].as_str().unwrap()).unwrap();
            if let Some(at) = bytes.windows(13).position(|b| b == b"ICC_PROFILE\0\x01") {
                bytes[at + 14 + 24..at + 14 + 36].fill(0); // volatile profile creation date
            }
            hash.update(&bytes);
        }
        eprintln!("library_bench,row=export_digest,n={},hash={}", selected.len(), hash.finish());
    }
    measure("catalog_close", ids.len(), || s.close_library()).unwrap();
    measure("catalog_open", ids.len(), || s.open_library(&library.0, false).map(|_| ())).unwrap();
    assert_eq!(s.catalog.len(), ids.len());
    s.close_library().unwrap();
    if std::env::var_os("LIBRARY_BENCH_KEEP_SYNTHETIC").is_some()
        && std::env::var_os("LIBRARY_BENCH_DIR").is_none()
        && std::env::var_os("LIBRARY_BENCH_SYNTHETIC_DIR").is_none()
    {
        std::mem::forget(files);
    }
}
