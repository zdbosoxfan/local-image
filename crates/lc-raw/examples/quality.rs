//! Offline reproducible synthetic quality/preview measurements. No client data is used.
//! `RAYON_NUM_THREADS=8 cargo +1.98.1 run --offline --release -p lightcraft-raw --example quality -- [--timings]`
use lightcraft_raw::demosaic::{mosaic_from_rgb, psnr};
use lightcraft_raw::highlight::{SegmentationOptions, segmentation};
use lightcraft_raw::{BlackLevel, Cfa, ColorData, Method, Normalized, Orientation, RawData, RawFormat, RawImage, Rect, Rgb32f};
use std::time::Instant;

mod quality_support;
use quality_support::{chroma_error, scene};

const METHODS: [Method; 9] =
    [Method::Bilinear, Method::Ppg, Method::Ahd, Method::Rcd, Method::DualRcd, Method::Vng4, Method::DualRcdVng, Method::Amaze, Method::DualAmazeVng];
fn raw(n: Normalized) -> RawImage {
    let (w, h) = (n.width, n.height);
    RawImage {
        format: RawFormat::Dng,
        width: w,
        height: h,
        cpp: 1,
        data: RawData::F32(n.data),
        cfa: n.cfa,
        bits: 32,
        black: BlackLevel::uniform(0.0),
        white: vec![1.0],
        active_area: Rect::new(0, 0, w, h),
        crop: Rect::new(0, 0, w, h),
        orientation: Orientation::Normal,
        color: ColorData::default(),
        wb_multipliers: None,
        linearized: true,
        opcodes: Default::default(),
        metadata: Default::default(),
    }
}
fn highlights(w: usize, h: usize) -> Rgb32f {
    Rgb32f::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        let lum = 0.2 + 1.8 * fx + 0.07 * (fy * 16.0).sin();
        [lum, 0.7 * lum, 0.5 * lum]
    })
}
fn preview() {
    let truth = highlights(768, 512);
    let cfa = Cfa::bayer("RGGB").unwrap();
    let n = mosaic_from_rgb(&truth.map(|p| p.map(|v| v.min(1.0))), &cfa);
    let r = raw(n);
    let full = r
        .develop_with_cfa(Method::Rcd, &Default::default(), |n| {
            segmentation(n, [1.0; 3], 0.99, &SegmentationOptions::default());
        })
        .unwrap();
    println!("highlights,segmentation full vs truth,{:.6},{:.9}", psnr(&truth, &full, 16), chroma_error(&truth, &full, 16));
    for k in [2, 4, 6, 8] {
        let p = r
            .develop_binned_with(k, 0.99, |n, scale| {
                segmentation(n, [1.0; 3], 0.99, &SegmentationOptions::default().at_scale(scale));
            })
            .unwrap()
            .unwrap();
        let down = lightcraft_raster::resample::fit(&full, p.width, p.height, lightcraft_raster::resample::Filter::Box);
        let mut max = 0.0f32;
        for (a, b) in p.data.iter().zip(&down.data) {
            for c in 0..3 {
                max = max.max((a[c] - b[c]).abs());
            }
        }
        println!("preview,k={k},{}x{},{:.6},{:.9},max={max:.9}", p.width, p.height, psnr(&down, &p, 4), chroma_error(&down, &p, 4));
    }
}
fn main() {
    println!("scene,method,PSNR_dB,chroma_RMSE");
    for (kind, name) in ["zone plate", "Siemens star", "colour edges"].into_iter().enumerate() {
        let truth = scene(kind, 256, 256);
        let n = mosaic_from_rgb(&truth, &Cfa::bayer("RGGB").unwrap());
        for method in METHODS {
            let out = lightcraft_raw::demosaic(&n, method);
            println!("{name},{method:?},{:.6},{:.9}", psnr(&truth, &out, 16), chroma_error(&truth, &out, 16));
        }
    }
    preview();
    if std::env::args().any(|a| a == "--timings") {
        let (w, h) = (6000, 4000);
        let cfa = Cfa::bayer("RGGB").unwrap();
        let n = Normalized {
            width: w,
            height: h,
            cpp: 1,
            cfa: Some(cfa.clone()),
            data: (0..w * h)
                .map(|i| {
                    let (x, y) = (i % w, i / w);
                    let v = 0.2 + 0.6 * x as f32 / w as f32 + 0.05 * ((x * 7 + y * 13) % 11) as f32 / 11.0;
                    v * [0.9, 1.0, 0.7][cfa.color_at(x, y) as usize]
                })
                .collect(),
        };
        for method in METHODS {
            let t = Instant::now();
            let out = lightcraft_raw::demosaic(&n, method);
            std::hint::black_box(&out);
            println!("timing,{method:?},{:.3} ms", t.elapsed().as_secs_f64() * 1000.0);
        }
        let r = raw(mosaic_from_rgb(&highlights(w, h).map(|p| p.map(|v| v.min(1.0))), &cfa));
        let mut n = r.normalized().unwrap();
        let t = Instant::now();
        segmentation(&mut n, [1.0; 3], 0.99, &SegmentationOptions::default());
        println!("timing,segmentation24MP,{:.3} ms", t.elapsed().as_secs_f64() * 1000.0);
        for k in [2, 4, 6, 8] {
            let t = Instant::now();
            let p = r
                .develop_binned_with(k, 0.99, |n, scale| {
                    segmentation(n, [1.0; 3], 0.99, &SegmentationOptions::default().at_scale(scale));
                })
                .unwrap();
            std::hint::black_box(p);
            println!("timing,segmentation-binned-k{k},{:.3} ms", t.elapsed().as_secs_f64() * 1000.0);
        }
    }
}
