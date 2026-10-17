//! Reproducible linear noisy re-mosaic curves, rawler second reference and crop evidence.
//! Outputs only under target/demosaic-quality. No dependencies or private photographs.
use lightcraft_raw::demosaic::{mosaic_from_rgb, noise_sigma, psnr};
use lightcraft_raw::{Cfa, Method, Normalized, RawData, RgbFrame};
use std::io::Write;
use std::path::Path;
mod quality_support;
fn gaussian(seed: &mut u32) -> f32 {
    let mut uniform = || {
        *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        ((*seed >> 8) as f32 + 0.5) / 16777216.0
    };
    let a = uniform();
    let b = uniform();
    (-2.0 * a.ln()).sqrt() * (std::f32::consts::TAU * b).cos()
}
fn ppm(path: &Path, img: &RgbFrame) {
    let mut f = std::fs::File::create(path).unwrap();
    write!(f, "P6\n{} {}\n255\n", img.width, img.height).unwrap();
    for p in &img.data {
        f.write_all(&p.map(|v| (v.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8)).unwrap();
    }
}
fn floats(path: &Path, img: &RgbFrame) {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    for p in &img.data {
        for v in p {
            f.write_all(&v.to_le_bytes()).unwrap();
        }
    }
}
fn rawler(n: &Normalized) -> RgbFrame {
    use rawler::imgop::sensor::{Demosaic, xtrans::markesteijn::XTransMarkesteijnDemosaic};
    let c = n.cfa.as_ref().unwrap();
    let pattern: String = (0..6).flat_map(|y| (0..6).map(move |x| "RGB".as_bytes()[c.color_at(x, y) as usize] as char)).collect();
    let pixels = rawler::pixarray::PixF32::new_with(n.data.clone(), n.width, n.height);
    let rect = rawler::imgop::Rect::new(rawler::imgop::Point::new(0, 0), rawler::imgop::Dim2::new(n.width, n.height));
    let out = XTransMarkesteijnDemosaic::new_pass_3().demosaic(&pixels, &rawler::CFA::new(&pattern), &rawler::cfa::PlaneColor::new("RGB"), rect);
    RgbFrame { width: n.width, height: n.height, data: out.data }
}
fn dng(n: Normalized, path: &Path) {
    use lightcraft_raw::{BlackLevel, ColorData, Orientation, RawFormat, RawImage, Rect};
    let (w, h) = (n.width, n.height);
    let color = ColorData { color_matrix: [Some(li_color::SRGB.to_xyz().inverse().unwrap()), None], illuminant: [21, 0], ..Default::default() };
    let raw = RawImage {
        format: RawFormat::Dng,
        width: w,
        height: h,
        cpp: 1,
        data: RawData::U16(n.data.iter().map(|v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16).collect()),
        cfa: n.cfa,
        bits: 16,
        black: BlackLevel::uniform(0.0),
        white: vec![65535.0],
        active_area: Rect::new(0, 0, w, h),
        crop: Rect::new(0, 0, w, h),
        orientation: Orientation::Normal,
        color,
        wb_multipliers: Some([1.0; 3]),
        linearized: true,
        vendor_lens: None,
        opcodes: Default::default(),
        metadata: Default::default(),
    };
    std::fs::write(path, lightcraft_raw::dngwrite::write_dng(&raw, &Default::default()).unwrap()).unwrap();
}
fn natural_curves(paths: &[String]) {
    println!("sensor,scene,sigma,estimated_sigma,method,cpsnr_db,chroma_rmse,ms_per_mp");
    for (kind, path) in paths.iter().enumerate() {
        let bytes = std::fs::read(path).unwrap();
        let values: Vec<f32> = bytes.as_chunks::<4>().0.iter().map(|v| f32::from_le_bytes(*v)).collect();
        assert_eq!(values.len(), 512 * 512 * 3);
        let truth = RgbFrame { width: 512, height: 512, data: values.as_chunks::<3>().0.to_vec() };
        for (sensor, cfa, methods) in [
            (
                "NaturalBayer",
                Cfa::bayer("RGGB").unwrap(),
                vec![Method::Ahd, Method::Rcd, Method::Amaze, Method::DualAmazeVng, Method::DualRcdVng, Method::Lmmse, Method::Dcb],
            ),
            ("NaturalXTrans", Cfa::xtrans(), vec![Method::XtransDirectional, Method::Markesteijn3, Method::DualMarkesteijnVng]),
        ] {
            for sigma in [0.0, 0.002, 0.005, 0.01, 0.02, 0.04, 0.08] {
                let mut n = mosaic_from_rgb(&truth, &cfa);
                let mut seed = 123456;
                for v in &mut n.data {
                    *v += sigma * gaussian(&mut seed);
                }
                let estimated = noise_sigma(&n);
                for &method in &methods {
                    let start = std::time::Instant::now();
                    let out = lightcraft_raw::demosaic(&n, method);
                    println!(
                        "{sensor},{kind},{sigma},{estimated:.8},{method:?},{:.6},{:.9},{:.3}",
                        psnr(&truth, &out, 24),
                        quality_support::chroma_error(&truth, &out, 24),
                        start.elapsed().as_secs_f64() * 1000.0 / (512.0 * 512.0 / 1e6)
                    );
                }
                if sensor == "NaturalXTrans" && sigma == 0.0 {
                    let out = rawler(&n);
                    println!(
                        "{sensor},{kind},0,0,rawler_0.8.0,{:.6},{:.9},0",
                        psnr(&truth, &out, 24),
                        quality_support::chroma_error(&truth, &out, 24)
                    );
                }
            }
        }
    }
}
fn sensor_timing(width: usize, height: usize) {
    for (sensor, cfa, methods) in [
        ("Bayer", Cfa::bayer("RGGB").unwrap(), vec![Method::Auto, Method::Lmmse, Method::Dcb]),
        ("XTrans", Cfa::xtrans(), vec![Method::XtransDirectional, Method::Markesteijn3, Method::DualMarkesteijnVng]),
    ] {
        let data = (0..width * height)
            .map(|i| {
                let (x, y) = (i % width, i / width);
                let c = cfa.color_at(x, y) as usize;
                let lum = 0.2 + 0.5 * x as f32 / width as f32 + 0.05 * (x as f32 * 0.7).sin() * (y as f32 * 0.9).cos();
                lum * [0.8, 1.0, 0.6][c]
            })
            .collect();
        let n = Normalized { width, height, cpp: 1, data, cfa: Some(cfa) };
        for method in methods {
            let start = std::time::Instant::now();
            let out = lightcraft_raw::demosaic(&n, method);
            println!(
                "{sensor},{width}x{height},{method:?},{:.3}ms/MP,check={:.6}",
                start.elapsed().as_secs_f64() * 1000.0 / (width * height) as f64 * 1e6,
                out.data[out.data.len() / 2][1]
            );
        }
    }
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|v| v == "--profiles") {
        profile_curves(&args[1], &args[2..]);
        return;
    }
    if args.first().is_some_and(|v| v == "--reference-raw") {
        raw_references(&args[1..]);
        return;
    }
    if args.first().is_some_and(|v| v == "--timing") {
        sensor_timing(args.get(1).and_then(|v| v.parse().ok()).unwrap_or(9504), args.get(2).and_then(|v| v.parse().ok()).unwrap_or(6336));
        return;
    }
    if args.first().is_some_and(|v| v == "--linear") {
        natural_curves(&args[1..]);
        return;
    }

    let root = Path::new("target/demosaic-quality");
    std::fs::create_dir_all(root).unwrap();
    println!("sensor,scene,sigma,estimated_sigma,method,cpsnr_db,chroma_rmse,ms_per_mp");
    for (sensor, cfa, methods) in [
        (
            "Bayer",
            Cfa::bayer("RGGB").unwrap(),
            vec![Method::Ahd, Method::Rcd, Method::Amaze, Method::DualAmazeVng, Method::DualRcdVng, Method::Lmmse, Method::Dcb],
        ),
        ("XTrans", Cfa::xtrans(), vec![Method::XtransDirectional, Method::Markesteijn3, Method::DualMarkesteijnVng]),
    ] {
        for kind in 0..5 {
            let truth = if kind < 3 {
                quality_support::scene(kind, 384, 384)
            } else {
                RgbFrame::from_fn(384, 384, |x, y| {
                    let fx = x as f32 / 384.0;
                    let fy = y as f32 / 384.0;
                    let l = if kind == 3 {
                        0.4 + 0.2 * fx + 0.1 * fy
                    } else {
                        0.4 + 0.1 * (x as f32 * 0.7).sin() * (y as f32 * 0.9).cos() + if x > 192 { 0.15 } else { 0.0 }
                    };
                    [l * (0.75 + 0.2 * fx), l, l * (0.55 + 0.3 * fy)]
                })
            };
            if kind < 3 {
                ppm(&root.join(format!("{sensor}-{kind}-truth.ppm")), &truth);
            }
            for sigma in [0.0, 0.002, 0.005, 0.01, 0.02, 0.04, 0.08] {
                let mut n = mosaic_from_rgb(&truth, &cfa);
                let mut seed = 123456;
                for v in &mut n.data {
                    *v += sigma * gaussian(&mut seed);
                }
                if sigma == 0.0 && kind == 0 {
                    // Match the exact 16-bit sensor samples used by the CLI reference.
                    // Tiny perturbations can change homogeneity votes on periodic charts.
                    for v in &mut n.data {
                        *v = (*v * 65535.0).round() / 65535.0;
                    }
                }
                let estimate = noise_sigma(&n);
                if sigma == 0.0 && kind == 0 {
                    dng(n.clone(), &root.join(format!("{sensor}.dng")));
                }
                for &method in &methods {
                    let start = std::time::Instant::now();
                    let out = lightcraft_raw::demosaic(&n, method);
                    let ms = start.elapsed().as_secs_f64() * 1000.0 / (384.0 * 384.0 / 1e6);
                    println!(
                        "{sensor},{kind},{sigma},{estimate:.8},{method:?},{:.6},{:.9},{ms:.3}",
                        psnr(&truth, &out, 24),
                        quality_support::chroma_error(&truth, &out, 24)
                    );
                    if sigma == 0.0 && kind < 3 {
                        ppm(&root.join(format!("{sensor}-{kind}-{method:?}.ppm")), &out);
                        if kind == 0 {
                            floats(&root.join(format!("{sensor}-{method:?}.f32")), &out);
                        }
                    }
                }
                if sensor == "XTrans" && sigma == 0.0 {
                    let out = rawler(&n);
                    println!("XTrans,{kind},0,0,rawler_0.8.0,{:.6},{:.9},0", psnr(&truth, &out, 24), quality_support::chroma_error(&truth, &out, 24));
                }
            }
        }
    }
    for path in std::env::args().skip(1) {
        let raw = lightcraft_raw::decode(&std::fs::read(&path).unwrap()).unwrap();
        let n = raw.normalized().unwrap();
        eprintln!("real,{path},{}x{},iso={:?},sigma={:.8}", n.width, n.height, raw.metadata.iso, noise_sigma(&n));
        for method in [Method::Auto, Method::Lmmse, Method::Dcb, Method::Markesteijn3] {
            let start = std::time::Instant::now();
            let out = lightcraft_raw::demosaic(&n, method);
            eprintln!("timing,{path},{method:?},{:.3}ms/MP", start.elapsed().as_secs_f64() * 1000.0 / (n.width * n.height) as f64 * 1e6);
            drop(out);
        }
    }
}

/// Reference-only Gaussian approximation to shot + read noise:
/// variance = a[channel]*signal + b[channel] (the Poisson-Gaussian profile model).
/// TSV coefficients are extracted from a whole pinned darktable noiseprofiles.json.
fn profile_curves(table: &str, paths: &[String]) {
    println!("sensor,scene,iso,estimated_sigma,method,cpsnr_db,chroma_rmse");
    let text = std::fs::read_to_string(table).unwrap();
    let profiles: Vec<Vec<f32>> = text.lines().map(|l| l.split_whitespace().map(|v| v.parse().unwrap()).collect()).collect();
    for (kind, path) in paths.iter().enumerate() {
        let bytes = std::fs::read(path).unwrap();
        let values: Vec<f32> = bytes.as_chunks::<4>().0.iter().map(|v| f32::from_le_bytes(*v)).collect();
        assert_eq!(values.len(), 512 * 512 * 3);
        let truth = RgbFrame { width: 512, height: 512, data: values.as_chunks::<3>().0.to_vec() };
        for (sensor, cfa, methods) in [
            (
                "ProfileBayer",
                Cfa::bayer("RGGB").unwrap(),
                vec![Method::Ahd, Method::Rcd, Method::Amaze, Method::DualAmazeVng, Method::DualRcdVng, Method::Lmmse, Method::Dcb],
            ),
            ("ProfileXTrans", Cfa::xtrans(), vec![Method::XtransDirectional, Method::Markesteijn3, Method::DualMarkesteijnVng]),
        ] {
            for profile in &profiles {
                assert_eq!(profile.len(), 7);
                let mut n = mosaic_from_rgb(&truth, &cfa);
                let mut seed = 123456;
                for (i, v) in n.data.iter_mut().enumerate() {
                    let c = cfa.color_at(i % 512, i / 512) as usize;
                    *v += (profile[1 + c] * v.max(0.0) + profile[4 + c]).max(0.0).sqrt() * gaussian(&mut seed);
                }
                let sigma = noise_sigma(&n);
                for &method in &methods {
                    let out = lightcraft_raw::demosaic(&n, method);
                    println!(
                        "{sensor},{kind},{},{sigma:.8},{method:?},{:.6},{:.9}",
                        profile[0] as u32,
                        psnr(&truth, &out, 24),
                        quality_support::chroma_error(&truth, &out, 24)
                    );
                }
            }
        }
    }
}

/// Isolate the demosaic on a centre crop of a CC0 camera's actual sensor samples.
/// The DNG's neutral WB and identity camera-RGB -> linear Rec.709 transform remove
/// differences in tone, colour profiles, WB, lens correction and orientation.
fn raw_references(paths: &[String]) {
    let root = Path::new("target/demosaic-quality/raw-references");
    std::fs::create_dir_all(root).unwrap();
    for path in paths {
        let raw = lightcraft_raw::decode(&std::fs::read(path).unwrap()).unwrap();
        let n = raw.normalized().unwrap();
        assert!(n.width >= 512 && n.height >= 512 && n.cpp == 1);
        let (x0, y0) = ((n.width - 512) / 2, (n.height - 512) / 2);
        let cfa = n.cfa.as_ref().unwrap().shifted(x0, y0);
        let bayer = cfa.is_bayer();
        let data = (0..512)
            .flat_map(|y| n.data[(y + y0) * n.width + x0..(y + y0) * n.width + x0 + 512].iter())
            .map(|v| (v.clamp(0.0, 1.0) * 65535.0).round() / 65535.0)
            .collect();
        let crop = Normalized { width: 512, height: 512, cpp: 1, data, cfa: Some(cfa) };
        let id = Path::new(path).file_stem().unwrap().to_string_lossy();
        dng(crop.clone(), &root.join(format!("{id}.dng")));
        for method in if bayer {
            vec![Method::Rcd, Method::Lmmse, Method::Dcb]
        } else {
            vec![Method::XtransDirectional, Method::Markesteijn3, Method::DualMarkesteijnVng]
        } {
            let out = lightcraft_raw::demosaic(&crop, method);
            floats(&root.join(format!("{id}-{method:?}.f32")), &out);
            ppm(&root.join(format!("{id}-{method:?}.ppm")), &out);
        }
        println!("{id},{}x{},iso={:?},cfa={},crop={x0}:{y0}:512:512", n.width, n.height, raw.metadata.iso, n.cfa.as_ref().unwrap().name());
    }
}
