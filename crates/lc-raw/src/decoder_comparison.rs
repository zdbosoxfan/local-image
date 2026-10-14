//! Permanent ignored head-to-head harness. Only decoder_compare_* tests are run by the driver.
//! Samples, external decoder outputs, timing/robustness ledgers stay under target/raw-comparison.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use crate::{BlackLevel, Cfa, Mode, RawData, RawFormat, RawImage, Rect};
use proptest::prelude::*;
use rawler::{rawimage::RawPhotometricInterpretation, rawsource::RawSource};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};
#[path = "../tests/it/robust.rs"]
mod robust_seeds;
#[path = "../tests/it/vendor_robust.rs"]
mod vendor_seeds;

fn root() -> PathBuf {
    std::env::var_os("RAW_COMPARE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/raw-comparison"))
}
fn threads() -> usize {
    std::env::var("RAW_COMPARE_THREADS").unwrap_or_else(|_| "4".into()).parse().unwrap()
}
fn rows() -> impl Iterator<Item = Vec<&'static str>> {
    let ids = std::env::var("RAW_COMPARE_IDS").unwrap_or_default();
    include_str!("../tests/fixtures/decoder-comparison-samples.tsv")
        .lines()
        .skip(1)
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .filter(move |row| ids.is_empty() || ids.split(',').any(|id| id == row[0]))
}
struct Sensor {
    width: usize,
    height: usize,
    cpp: usize,
    data: RawData,
    black: BlackLevel,
    white: Vec<f32>,
    active: Rect,
    crop: Rect,
    cfa: Option<Cfa>,
    wb: Option<[f32; 3]>,
    orientation: u16,
    // Native black patterns/deltas are active-relative; rawler patterns are sensor-relative.
    anchor: (usize, usize),
    reference_colors: Option<Vec<usize>>,
    reference_repeat: Vec<f32>,
}
impl Sensor {
    fn own(raw: RawImage) -> Self {
        Self {
            width: raw.width,
            height: raw.height,
            cpp: raw.cpp,
            data: raw.data,
            black: raw.black,
            white: raw.white,
            active: raw.active_area,
            crop: Rect::new(raw.active_area.x + raw.crop.x, raw.active_area.y + raw.crop.y, raw.crop.width, raw.crop.height),
            cfa: raw.cfa,
            wb: raw.wb_multipliers.or_else(|| {
                raw.color.as_shot_neutral.map(|n| {
                    let v = n.map(|n| (1.0 / n) as f32);
                    v.map(|x| x / v[1])
                })
            }),
            orientation: raw.orientation.to_exif(),
            anchor: (raw.active_area.x, raw.active_area.y),
            reference_colors: None,
            reference_repeat: Vec::new(),
        }
    }
    fn rawler(raw: rawler::RawImage) -> Self {
        let rect = |r: rawler::imgop::Rect| Rect::new(r.p.x, r.p.y, r.d.w, r.d.h);
        let active = raw.active_area.map(rect).unwrap_or(Rect::new(0, 0, raw.width, raw.height));
        let crop = raw.crop_area.map(rect).unwrap_or(active);
        let cfa = if let RawPhotometricInterpretation::Cfa(c) = &raw.photometric {
            Some(Cfa {
                width: c.cfa.width,
                height: c.cfa.height,
                pattern: (0..c.cfa.height).flat_map(|y| (0..c.cfa.width).map(move |x| c.cfa.color_at(y, x) as u8)).collect(),
            })
        } else {
            None
        };
        let wb = [raw.wb_coeffs[0], raw.wb_coeffs[1], raw.wb_coeffs[2]];
        Self {
            width: raw.width,
            height: raw.height,
            cpp: raw.cpp,
            data: match raw.data {
                rawler::RawImageData::Integer(d) => RawData::U16(d),
                rawler::RawImageData::Float(d) => RawData::F32(d),
            },
            black: BlackLevel {
                repeat_cols: raw.blacklevel.width,
                repeat_rows: raw.blacklevel.height,
                values: raw.blacklevel.as_vec(),
                ..Default::default()
            },
            white: raw.whitelevel.as_vec(),
            active,
            crop,
            cfa,
            wb: wb.iter().all(|v| v.is_finite() && *v > 0.0).then(|| wb.map(|v| v / wb[1])),
            orientation: raw.orientation.to_u16(),
            anchor: (0, 0),
            reference_colors: None,
            reference_repeat: Vec::new(),
        }
    }
    fn black_at(&self, x: usize, y: usize, s: usize) -> f64 {
        if let Some(colors) = &self.reference_colors {
            let c = if self.cpp == 1 { colors[(y % 6) * 6 + x % 6] } else { s };
            let base = self.black.values.get(c).copied().unwrap_or(self.black.values[0]);
            let add = self
                .reference_repeat
                .get((y % self.black.repeat_rows.max(1)) * self.black.repeat_cols.max(1) + x % self.black.repeat_cols.max(1))
                .copied()
                .unwrap_or(0.0);
            return f64::from(base + add);
        }
        let x = x - self.anchor.0;
        let y = y - self.anchor.1;
        f64::from(self.black.at(x, y, s, self.cpp))
    }
    fn value(&self, x: usize, y: usize, s: usize) -> f64 {
        f64::from(self.data.get((y * self.width + x) * self.cpp + s))
    }
    fn white_at(&self, s: usize) -> f64 {
        f64::from(self.white.get(s).or_else(|| self.white.first()).copied().unwrap_or(1.0))
    }
}
fn reference(dir: &Path, id: &str) -> Option<Sensor> {
    let text = fs::read_to_string(dir.join(format!("{id}.libraw.meta"))).ok()?;
    let lines: Vec<_> = text.lines().collect();
    let nums = |line: &str| line.split_whitespace().map(|v| v.parse::<f32>().unwrap()).collect::<Vec<_>>();
    let size = nums(lines[0]);
    let area = nums(lines[1]);
    let repeat = nums(lines[8]);
    let file = fs::read(dir.join(format!("{id}.libraw.f32"))).ok()?;
    let data = file.as_chunks::<4>().0.iter().map(|&v| f32::from_le_bytes(v)).collect::<Vec<_>>();
    let (width, height, cpp) = (size[0] as usize, size[1] as usize, size[2] as usize);
    if data.len() != width * height * cpp {
        return None;
    }
    let raw_wb = nums(lines[4]);
    let wb = [raw_wb[0], raw_wb[1], raw_wb[2]];
    let orientation = match lines[5] {
        "3" => 3,
        "5" => 8,
        "6" => 6,
        _ => 1,
    };
    let active = Rect::new(area[0] as usize, area[1] as usize, area[2] as usize, area[3] as usize).clipped(width, height);
    Some(Sensor {
        width,
        height,
        cpp,
        data: RawData::F32(data),
        black: BlackLevel { values: nums(lines[2]), repeat_rows: repeat[0] as usize, repeat_cols: repeat[1] as usize, ..Default::default() },
        white: nums(lines[3]),
        active,
        crop: active,
        cfa: None,
        wb: wb.iter().all(|v| *v > 0.0).then(|| wb.map(|v| v / wb[1])),
        orientation,
        anchor: (0, 0),
        reference_colors: Some(lines[10].chars().map(|c| c.to_digit(10).unwrap() as usize).collect()),
        reference_repeat: nums(lines[9]),
    })
}
#[derive(Debug, Default)]
struct Delta {
    count: usize,
    exact: usize,
    sum: f64,
    max: f64,
}
impl Delta {
    fn add(&mut self, a: f64, b: f64) {
        let d = (a - b).abs();
        self.count += 1;
        self.exact += usize::from(a == b);
        self.sum += d;
        self.max = self.max.max(d);
    }
    fn fields(&self) -> String {
        format!(
            "{}\t{:.9}\t{:.9}\t{:.6}",
            self.count,
            self.max,
            self.sum / self.count.max(1) as f64,
            100.0 * self.exact as f64 / self.count.max(1) as f64
        )
    }
}
fn common(a: &Sensor, b: &Sensor) -> Option<Rect> {
    if (a.width, a.height, a.cpp) != (b.width, b.height, b.cpp) {
        return None;
    }
    let x = a.active.x.max(b.active.x);
    let y = a.active.y.max(b.active.y);
    let right = (a.active.x + a.active.width).min(b.active.x + b.active.width).min(a.width);
    let bottom = (a.active.y + a.active.height).min(b.active.y + b.active.height).min(a.height);
    (right > x && bottom > y).then(|| Rect::new(x, y, right - x, bottom - y))
}
fn agreement(a: &Sensor, b: &Sensor) -> Option<(Delta, Delta, Delta)> {
    let area = common(a, b)?;
    let (mut raw, mut black, mut normalized) = (Delta::default(), Delta::default(), Delta::default());
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            for s in 0..a.cpp {
                let av = a.value(x, y, s);
                let bv = b.value(x, y, s);
                let ab = a.black_at(x, y, s);
                let bb = b.black_at(x, y, s);
                raw.add(av, bv);
                black.add(av - ab, bv - bb);
                normalized.add((av - ab) / (a.white_at(s) - ab), (bv - bb) / (b.white_at(s) - bb));
            }
        }
    }
    Some((raw, black, normalized))
}
fn mismatches(a: &Sensor, b: &Sensor) -> Vec<&'static str> {
    let mut out = Vec::new();
    if (a.width, a.height, a.cpp) != (b.width, b.height, b.cpp) {
        out.push("sensor_geometry");
    }
    let eq = |x: f32, y: f32| (x - y).abs() <= 1e-5;
    if (0..a.cpp.max(b.cpp)).any(|s| !eq(a.white_at(s) as f32, b.white_at(s) as f32)) {
        out.push("white");
    }
    if let Some(area) = common(a, b)
        && (0..6)
            .any(|y| (0..6).any(|x| (0..a.cpp).any(|s| (a.black_at(area.x + x, area.y + y, s) - b.black_at(area.x + x, area.y + y, s)).abs() > 1e-5)))
    {
        out.push("black");
    }
    if match (a.wb, b.wb) {
        (Some(x), Some(y)) => x.iter().zip(y).any(|(&x, y)| (x - y).abs() > 1e-5),
        (None, None) => false,
        _ => true,
    } {
        out.push("WB");
    }
    if a.active != b.active {
        out.push("active_area");
    }
    if a.crop != b.crop {
        out.push("crop");
    }
    if a.orientation != b.orientation {
        out.push("orientation");
    }
    if let Some(reference) = &b.reference_colors {
        if a.cfa.as_ref().is_some_and(|c| {
            (0..6).any(|y| (0..6).any(|x| c.color_at(x, y) != if reference[y * 6 + x] == 3 { 1 } else { reference[y * 6 + x] as u8 }))
        }) {
            out.push("CFA");
        }
    } else if a.cfa != b.cfa {
        out.push("CFA");
    }
    out
}
fn median(mut f: impl FnMut()) -> f64 {
    let mut times = Vec::new();
    for _ in 0..5 {
        let t = Instant::now();
        f();
        times.push(t.elapsed().as_secs_f64() * 1000.);
    }
    times.sort_by(f64::total_cmp);
    times[2]
}
fn native_sensor_decode(bytes: &[u8], format: RawFormat) -> crate::Result<RawImage> {
    // Saturation lifting is an application processing policy, not decompression; compare codecs before it.
    if format == RawFormat::Dng { crate::dng::decode(bytes, Mode::Full) } else { crate::decode_native(bytes, format, Mode::Full) }
}
fn rawler_decode(bytes: &[u8]) -> rawler::Result<rawler::RawImage> {
    rawler::decode(&RawSource::new_from_slice(bytes), &Default::default())
}
fn own_preview(raw: &RawImage) {
    let step = if raw.cfa.as_ref().is_some_and(|c| c.width == 6 && c.height == 6) { 3 } else { 2 };
    let k = raw.crop.width.max(raw.crop.height).div_ceil(512).max(step).next_multiple_of(step);
    let rgb = raw.develop_binned(k, 0.99).unwrap().unwrap_or_else(|| raw.develop(crate::Method::Bilinear).unwrap());
    std::hint::black_box(lightcraft_raster::resample::fit(&rgb, 512, 512, lightcraft_raster::resample::Filter::Box));
}
fn rawler_preview(raw: &rawler::RawImage) {
    use rawler::imgop::develop::{Intermediate, ProcessingStep, RawDevelop};
    let image =
        RawDevelop::new_with(&[ProcessingStep::Rescale, ProcessingStep::Demosaic, ProcessingStep::CropActiveArea, ProcessingStep::CropDefault])
            .develop_intermediate(raw)
            .unwrap();
    let dim = image.dim();
    let data = match image {
        Intermediate::ThreeColor(c) => c.data,
        Intermediate::FourColor(c) => c.data.into_iter().map(|p| [p[0], (p[1] + p[3]) * 0.5, p[2]]).collect(),
        Intermediate::Monochrome(c) => c.data.into_iter().map(|v| [v; 3]).collect(),
    };
    let rgb = lightcraft_raster::Rgb32f { width: dim.w, height: dim.h, data };
    std::hint::black_box(lightcraft_raster::resample::fit(&rgb, 512, 512, lightcraft_raster::resample::Filter::Box));
}
#[test]
#[ignore = "CC0 comparison: scripts/raw_decoder_comparison.py --fetch --references --run"]
fn decoder_compare_cc0() {
    let dir = root();
    fs::create_dir_all(&dir).unwrap();
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads()).build().unwrap();
    let timing = std::env::var("RAW_COMPARE_TIMINGS").as_deref() == Ok("1");
    let mut summary = String::from(
        "id\tformat\tcompression\tmaker\tmodel\tvariant\tcpp\tours_status\trawler_status\tours_decode_ms\trawler_decode_ms\tours_preview_ms\trawler_downscale_ms\n",
    );
    let mut deltas = String::from("id\tpair\tmetric\tcount\tmax_abs\tmean_abs\tidentical_percent\tmetadata_mismatches\n");
    let mut metadata = String::new();
    for row in rows() {
        let bytes = fs::read(dir.join(row[1])).expect("fetch CC0 files first");
        let fmt = crate::probe(&bytes).unwrap();
        let label = if row[1].to_ascii_uppercase().ends_with(".NRW") {
            "Nrw".into()
        } else if row[1].to_ascii_uppercase().ends_with(".RWL") {
            "Rwl".into()
        } else {
            format!("{fmt:?}")
        };
        let compression = if fmt == RawFormat::Raf {
            row[4].to_string()
        } else if fmt == RawFormat::Rw2 {
            let t = lightcraft_tiff::Tiff::parse(&bytes).unwrap();
            format!("packing-{}", t.ifds[0].u16(0x2d).unwrap_or(0))
        } else {
            lightcraft_tiff::Tiff::parse(&bytes)
                .ok()
                .and_then(|t| t.all_ifds().into_iter().max_by_key(|i| i.u64(256).unwrap_or(0) * i.u64(257).unwrap_or(0)).and_then(|i| i.u16(259)))
                .map_or_else(|| "unknown".into(), |v| v.to_string())
        };
        let own = pool.install(|| native_sensor_decode(&bytes, fmt));
        let theirs = pool.install(|| rawler_decode(&bytes));
        let (mut od, mut rd, mut op, mut rp) = (f64::NAN, f64::NAN, f64::NAN, f64::NAN);
        if timing {
            if own.is_ok() {
                od = median(|| drop(pool.install(|| native_sensor_decode(&bytes, fmt)).unwrap()));
                op = median(|| pool.install(|| own_preview(&native_sensor_decode(&bytes, fmt).unwrap())));
            }
            if theirs.is_ok() {
                rd = median(|| drop(pool.install(|| rawler_decode(&bytes)).unwrap()));
                rp = median(|| pool.install(|| rawler_preview(&rawler_decode(&bytes).unwrap())));
            }
        }
        summary.push_str(&format!(
            "{}\t{label}\t{compression}\t{}\t{}\t{}\t{}\t{}\t{}\t{od:.3}\t{rd:.3}\t{op:.3}\t{rp:.3}\n",
            row[0],
            row[2],
            row[3],
            row[4],
            own.as_ref().map_or(0, |r| r.cpp),
            own.as_ref().map(|_| "ok".into()).unwrap_or_else(|e| e.to_string()),
            theirs.as_ref().map(|_| "ok".into()).unwrap_or_else(|e| e.to_string())
        ));
        let own = own.ok().map(Sensor::own);
        let theirs = theirs.ok().map(Sensor::rawler);
        let reference = reference(&dir, row[0]);
        for (name, a, b) in [
            ("ours-rawler", own.as_ref(), theirs.as_ref()),
            ("ours-libraw", own.as_ref(), reference.as_ref()),
            ("rawler-libraw", theirs.as_ref(), reference.as_ref()),
        ] {
            if let (Some(a), Some(b)) = (a, b) {
                metadata.push_str(&format!("{} {name}: ours/left active={:?} crop={:?} black={:?} white={:?} WB={:?} orientation={} CFA={:?}; right active={:?} crop={:?} black={:?} white={:?} WB={:?} orientation={} CFA={:?}\n",row[0],a.active,a.crop,a.black,a.white,a.wb,a.orientation,a.cfa,b.active,b.crop,b.black,b.white,b.wb,b.orientation,b.cfa));
                if let Some((raw, black, norm)) = agreement(a, b) {
                    for (label, d) in [("stored", raw), ("black_subtracted", black), ("normalized", norm)] {
                        deltas.push_str(&format!("{}\t{name}\t{label}\t{}\t{}\n", row[0], d.fields(), mismatches(a, b).join(",")));
                    }
                } else {
                    deltas.push_str(&format!("{}\t{name}\tunmatched_geometry\t0\tNaN\tNaN\t0\t{}\n", row[0], mismatches(a, b).join(",")));
                }
            }
        }
        eprintln!("comparison {} {fmt:?}", row[0]);
        fs::write(dir.join("comparison.tsv"), &summary).unwrap();
        fs::write(dir.join("agreement.tsv"), &deltas).unwrap();
        fs::write(dir.join("metadata.txt"), &metadata).unwrap();
    }
    fs::write(
        dir.join("measurement-config.txt"),
        format!(
            "threads={}\nlogical_cores={}\nmedian_runs={}\nraw-preview=Rescale+Demosaic+CropActiveArea+CropDefault+Box512\n",
            threads(),
            std::thread::available_parallelism().unwrap(),
            if timing { 5 } else { 0 }
        ),
    )
    .unwrap();
}

#[test]
fn decoder_compare_metric_contract() {
    let raw = robust_seeds::sample(crate::DngCompression::Uncompressed);
    let a = Sensor::own(crate::decode_native(&raw, RawFormat::Dng, Mode::Full).unwrap());
    let mut b = Sensor::own(crate::decode_native(&raw, RawFormat::Dng, Mode::Full).unwrap());
    let (_, delta, norm) = agreement(&a, &b).unwrap();
    assert_eq!(delta.max, 0.0);
    assert_eq!(delta.exact, delta.count);
    assert_eq!(norm.max, 0.0);
    let RawData::U16(data) = &mut b.data else { panic!("integer fixture") };
    data[0] += 2;
    b.white[0] += 10.0;
    let (stored, _, normalized) = agreement(&a, &b).unwrap();
    assert_eq!(stored.max, 2.0);
    assert_eq!(stored.exact + 1, stored.count);
    assert!((stored.sum / stored.count as f64 - 2.0 / stored.count as f64).abs() < 1e-12);
    assert!(normalized.max > 0.0);
    assert!(mismatches(&a, &b).contains(&"white"));
}
#[test]
#[ignore = "build the existing malformed corpus for isolated decoder comparison"]
fn decoder_compare_build_fuzz_corpus() {
    use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
    let dir = root().join("fuzz");
    fs::create_dir_all(&dir).unwrap();
    let index = std::cell::Cell::new(0);
    let put = |category: &str, b: &[u8]| {
        let i = index.get();
        fs::write(dir.join(format!("{i:05}-{category}.bin")), b).unwrap();
        index.set(i + 1);
    };
    let vendor = vendor_seeds::samples();
    for s in &vendor {
        put("vendor", s);
        for end in (0..s.len()).step_by(3) {
            put("vendor", &s[..end]);
        }
    }
    for compression in [crate::DngCompression::Uncompressed, crate::DngCompression::Lj92 { tile: 16 }] {
        let sample = robust_seeds::sample(compression);
        for end in (0..sample.len()).step_by(7) {
            put("dng", &sample[..end]);
        }
    }
    let mut runner = TestRunner::new_with_rng(
        Config { cases: 2000, failure_persistence: None, ..Default::default() },
        TestRng::from_seed(RngAlgorithm::ChaCha, &[17; 32]),
    );
    runner
        .run(&(0usize..17, proptest::collection::vec((any::<usize>(), any::<u8>()), 1..16), any::<usize>()), |(kind, flips, cut)| {
            let mut b = vendor[kind].clone();
            let n = b.len();
            for (i, v) in flips {
                b[i % n] = v;
            }
            let len = if cut % 3 == 0 { cut % n } else { n };
            put("vendor", &b[..len]);
            Ok(())
        })
        .unwrap();
    let mut runner = TestRunner::new_with_rng(
        Config { cases: 400, failure_persistence: None, ..Default::default() },
        TestRng::from_seed(RngAlgorithm::ChaCha, &[23; 32]),
    );
    runner
        .run(&(proptest::collection::vec((any::<usize>(), any::<u8>()), 1..24), any::<usize>(), any::<bool>()), |(flips, cut, lj)| {
            let mut b = robust_seeds::sample(if lj { crate::DngCompression::Lj92 { tile: 16 } } else { crate::DngCompression::Uncompressed });
            let n = b.len();
            for (i, v) in flips {
                b[i % n] = v;
            }
            let len = if cut % 3 == 0 { cut % n } else { n };
            put("dng", &b[..len]);
            Ok(())
        })
        .unwrap();
    // Saved robust.proptest-regressions minimized case, not just fresh randomized seeds.
    let mut b = robust_seeds::sample(crate::DngCompression::Lj92 { tile: 16 });
    let n = b.len();
    for (i, v) in [(16380191992317618766u64, 16), (5757657205609716137, 1)] {
        b[i as usize % n] = v;
    }
    put("dng", &b);
    runner
        .run(&(proptest::collection::vec(any::<u8>(), 8..700), 0usize..4), |(mut b, kind)| {
            let headers: [&[u8]; 4] = [b"II*\0\x08\0\0\0", b"MM\0*\0\0\0\x08", b"II*\0\x10\0\0\0CR\x02\0", b"II+\0\x08\0\0\0"];
            let h = headers[kind];
            let n = h.len().min(b.len());
            b[..n].copy_from_slice(&h[..n]);
            put("tiff", &b);
            Ok(())
        })
        .unwrap();
    runner
        .run(&proptest::collection::vec(any::<u8>(), 0..400), |data| {
            let mut s = vec![0xff, 0xd8, 0xff, 0xc3, 0, 11, 12, 0, 8, 0, 8, 1, 1, 0x11, 0];
            s.extend(data);
            put("ljpeg", &s);
            Ok(())
        })
        .unwrap();
    eprintln!("comparison malformed cases={}", index.get());
}
#[test]
#[ignore = "isolated 768MiB process: scripts/raw_decoder_comparison.py --robustness"]
fn decoder_compare_fuzz_child() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let backend = std::env::var("RAW_COMPARE_FUZZ_BACKEND").expect("driver backend");
    let panics = Arc::new(AtomicUsize::new(0));
    let seen = panics.clone();
    let old = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
    }));
    let mut paths: Vec<_> = fs::read_dir(root().join("fuzz")).unwrap().map(|e| e.unwrap().path()).collect();
    paths.sort();
    let (mut ok, mut errors, mut escaped) = (0, 0, 0);
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads()).build().unwrap();
    let skip = std::env::var("RAW_COMPARE_FUZZ_SKIP").unwrap_or_else(|_| "0".into()).parse::<usize>().unwrap();
    for (index, path) in paths.into_iter().enumerate().skip(skip) {
        fs::write(root().join(format!("fuzz-{backend}-progress.txt")), path.to_string_lossy().as_bytes()).unwrap();
        let progress = format!("{}\t{ok}\t{errors}\t{escaped}\t{}\n", index, panics.load(Ordering::SeqCst));
        fs::write(root().join(format!("fuzz-{backend}-checkpoint.tsv")), progress).unwrap();
        let bytes = fs::read(&path).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pool.install(|| {
                if path.file_name().unwrap().to_string_lossy().contains("ljpeg") {
                    if backend == "ours" || backend == "adapter" {
                        return crate::ljpeg::decode(&bytes, 1 << 20).is_ok();
                    }
                    return rawler::decompressors::ljpeg::LjpegDecompressor::new(&bytes)
                        .and_then(|d| {
                            let width = d.width() * d.components();
                            let height = d.height();
                            if width * height > 1 << 20 {
                                return Err("limit".into());
                            }
                            let mut out = vec![0; width * height];
                            d.decode(&mut out, 0, width, width, height, false)
                        })
                        .is_ok();
                }
                match backend.as_str() {
                    "ours" => crate::probe(&bytes).is_some_and(|fmt| native_sensor_decode(&bytes, fmt).is_ok()),
                    "adapter" => crate::decode(&bytes).is_ok(),
                    _ => rawler_decode(&bytes).is_ok(),
                }
            })
        }));
        match result {
            Ok(true) => ok += 1,
            Ok(false) => errors += 1,
            Err(_) => escaped += 1,
        }
    }
    std::panic::set_hook(old);
    let rss = fs::read_to_string("/proc/self/status").unwrap();
    let peak = rss.lines().find(|l| l.starts_with("VmHWM:")).unwrap_or("VmHWM unavailable");
    let text = format!(
        "backend\tok\terrors\tescaped_panics\tinternal_panics\tpeak_memory\n{backend}\t{ok}\t{errors}\t{escaped}\t{}\t{peak}\n",
        panics.load(Ordering::SeqCst)
    );
    fs::write(root().join(format!("robustness-{backend}.tsv")), text).unwrap();
    assert_eq!(escaped, 0, "escaped decoder panics");
}

#[test]
fn decoder_compare_routing_follows_reference_winners_and_keeps_fallback() {
    for format in [RawFormat::Dng, RawFormat::Cr2, RawFormat::Nef, RawFormat::Arw, RawFormat::Pef, RawFormat::Orf] {
        assert!(crate::rawler_adapter::native_first(format));
    }
    for format in [RawFormat::Rw2, RawFormat::Raf, RawFormat::Nrw, RawFormat::Cr3, RawFormat::Srw, RawFormat::OtherTiff] {
        assert!(!crate::rawler_adapter::native_first(format));
    }
    let bytes = robust_seeds::sample(crate::DngCompression::Uncompressed);
    assert_eq!(crate::decode(&bytes).unwrap().data, crate::decode_native(&bytes, RawFormat::Dng, Mode::Full).unwrap().data);
}
#[test]
#[ignore = "CC0 files downloaded by permanent comparison driver"]
fn decoder_compare_routing_cc0_native_and_fallback_samples() {
    for row in rows() {
        let bytes = fs::read(root().join(row[1])).unwrap();
        let format = crate::probe(&bytes).unwrap();
        if crate::rawler_adapter::native_first_for(&bytes, format) {
            if let Ok(native) = crate::decode_native(&bytes, format, Mode::Full) {
                assert_eq!(crate::decode(&bytes).unwrap(), native, "{}", row[0]);
            }
        } else {
            let expected = crate::rawler_adapter::decode(&bytes, format, Mode::Full).unwrap();
            assert_eq!(crate::decode(&bytes).unwrap(), expected, "{}", row[0]);
        }
    }
    // A compressed RAF requires fallback; its asymmetric CFA must retain rawler's row/column order.
    let bytes = fs::read(root().parent().unwrap().join("rawdecode/6122.RAF")).unwrap();
    assert!(matches!(crate::decode_native(&bytes, RawFormat::Raf, Mode::Full), Err(crate::RawError::Unsupported(_))));
    let public = crate::decode(&bytes).unwrap();
    let fallback = crate::rawler_adapter::decode(&bytes, RawFormat::Raf, Mode::Full).unwrap();
    assert_eq!(public, fallback);
    let direct = Sensor::rawler(rawler_decode(&bytes).unwrap());
    assert_eq!(public.cfa, direct.cfa);
}
