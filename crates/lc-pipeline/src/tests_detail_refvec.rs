//! Independent numeric fixtures from pinned upstream C/C++, extracted outside the build.
use crate::detail::{haze, nr};
use lightcraft_raster::Rgb32f;

fn noise(x: usize, y: usize, k: u32) -> f32 {
    let mut v = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ k.wrapping_mul(0xcb1a_b31f);
    v ^= v >> 13;
    v = v.wrapping_mul(0x5bd1_e995);
    v ^= v >> 15;
    (v & 65535) as f32 / 32768.0 - 1.0
}
fn close(actual: f32, want: f32, tol: f32, worst: &mut f32) {
    let error = (actual - want).abs();
    *worst = worst.max(error);
    assert!(error <= tol, "{actual} vs upstream {want}: {error} > {tol}");
}
#[test]
fn upstream_vst_matrices_and_fast_exp() {
    let (to, from) = nr::conversion_matrices([1.0; 3]);
    let mut worst = 0.0;
    let vst = nr::Vst { a: 0.00027, b: 0.00001, p: 1.23, bias: -0.7, wb: 1.0, to, from };
    for line in include_str!("../tests/fixtures/detail-nr.csv").lines() {
        let v: Vec<_> = line.split(',').collect();
        let f = |i: usize| v[i].parse::<f32>().unwrap();
        let u = |i: usize| v[i].parse::<usize>().unwrap();
        match v[0] {
            "matrix" => {
                close(to[u(1)][u(2)], f(3), 1e-6, &mut worst);
                close(from[u(1)][u(2)], f(4), 1e-6, &mut worst);
            }
            "vst" => {
                let rgb = [f(2), f(3), f(4)];
                let forward = vst.forward(rgb);
                let back = vst.backward(forward);
                for k in 0..3 {
                    close(forward[k], f(5 + k), 2e-4, &mut worst);
                    close(back[k], f(8 + k), 2e-6, &mut worst);
                }
            }
            "exp" => assert_eq!(nr::fast_mexp2f(f(2)).to_bits(), f(3).to_bits()),
            _ => {}
        }
    }
    eprintln!("upstream matrix/VST worst absolute difference {worst:.8}");
}
#[test]
fn upstream_eaw_bayes_shrink_and_synthesis() {
    let mut img = Rgb32f::from_fn(37, 29, |x, y| {
        std::array::from_fn(|c| 12.0 + 0.05 * x as f32 + 0.03 * y as f32 + if x >= 37 / 2 { 20.0 } else { 0.0 } + 2.0 * noise(x, y, c as u32 + 1))
    });
    let mut acc = Rgb32f::new(37, 29);
    let mut worst = 0.0;
    let mut worst_stats = 0.0;
    for level in 0..3 {
        let (co, det, sum) = nr::eaw_decompose(&img, level);
        let thr = nr::thresholds(level, img.data.len(), sum, [0.43, 0.67, 0.67]);
        for (a, d) in acc.data.iter_mut().zip(&det.data) {
            for k in 0..3 {
                a[k] += nr::soft_threshold(d[k], thr[k]);
            }
        }
        for line in include_str!("../tests/fixtures/detail-nr.csv").lines() {
            let v: Vec<_> = line.split(',').collect();
            if v[0] != "eaw" && v[0] != "band" {
                continue;
            }
            if v[1].parse::<usize>().unwrap() != level {
                continue;
            }
            let f = |i: usize| v[i].parse::<f32>().unwrap();
            if v[0] == "band" {
                for k in 0..3 {
                    close(sum[k], f(2 + k), 1e-4 * f(2 + k).abs().max(1.0), &mut worst_stats);
                    close(thr[k], f(5 + k), 1e-4 * f(5 + k).abs().max(1.0), &mut worst_stats);
                }
            } else {
                let i = v[2].parse::<usize>().unwrap();
                for k in 0..3 {
                    close(co.data[i][k], f(3 + k), 2e-5, &mut worst);
                    close(det.data[i][k], f(6 + k), 2e-5, &mut worst);
                    close(acc.data[i][k], f(9 + k), 3e-5, &mut worst);
                }
            }
        }
        img = co;
    }
    eprintln!("upstream EAW/synthesis worst {worst:.8}; squared-sum/threshold worst {worst_stats:.8}");
}
#[test]
fn upstream_ambient_transmission_guidance_and_boxes() {
    let img = Rgb32f::from_fn(37, 29, |x, y| {
        std::array::from_fn(|c| {
            0.15 + 0.012 * x as f32 + 0.004 * y as f32 + 0.03 * c as f32 + if x > 37 / 2 { 0.12 } else { 0.0 } + 0.008 * noise(x, y, c as u32 + 1)
        })
    });
    let hz = haze::haze_plane(&img, 1.0);
    let mut worst = 0.0;
    let data: Vec<_> = (0..37 * 29).map(|i| 0.03 * (i % 17) as f32 - 0.2).collect();
    let mean = haze::box_mean(&data, 37, 29, 1, 5);
    for line in include_str!("../tests/fixtures/detail-haze.csv").lines() {
        let v: Vec<_> = line.split(',').collect();
        let f = |i: usize| v[i].parse::<f32>().unwrap();
        match v[0] {
            "air" => {
                for k in 0..3 {
                    close(hz.air[k], f(1 + k), 2e-6, &mut worst);
                }
                close(hz.distance, f(4), 2e-6, &mut worst);
            }
            "box" => close(mean[v[1].parse::<usize>().unwrap()], f(2), 2e-7, &mut worst),
            "haze" => {
                let i = v[2].parse::<usize>().unwrap();
                let s = f(1) * 0.7;
                let d = hz.at(i, s);
                close(1.0 - s * d, f(4), 3e-5, &mut worst);
                let rgb = haze::dehaze_px(img.data[i], s, d, hz.air, hz.distance);
                for k in 0..3 {
                    close(rgb[k], f(5 + k), 3e-5, &mut worst);
                }
            }
            _ => panic!("unknown fixture row"),
        }
    }
    eprintln!("upstream haze / guided RGB / Kahan box worst absolute difference {worst:.8}");
}
