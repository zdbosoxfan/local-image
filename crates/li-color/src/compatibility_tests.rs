use crate::cms_math as math;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use crate::{curve, space::Trc};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn hash32(h: &mut u64, v: f32) {
    hash64(h, v.to_bits() as u64);
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn hash64(h: &mut u64, v: u64) {
    *h = (*h ^ v).wrapping_mul(0x100000001b3);
}
// IEEE arithmetic snapshots were captured from the pre-unification source.
// powf/cbrt delegate to the platform math library; these exact-bit fixtures are
// scoped to the measured Linux x86_64 baseline, while portable formula,
// roundtrip and f32-vs-f64 accuracy tests run on all targets.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn cms_and_curve_legacy_reference_bits() {
    let mut h = [0xcbf29ce484222325; 6];
    let m = [[0.4, 0.3, 0.2], [0.2, 0.7, 0.1], [0.0, 0.1, 0.8]];
    let inv = math::invert(&m).unwrap();
    for row in math::mul(&m, &inv) {
        for v in row {
            hash64(&mut h[0], v.to_bits());
        }
    }
    for xyz in [[0.0, 0.0, 0.0], [0.2, 0.3, 0.1], [-0.02, 0.1, 1.5], math::D50] {
        for v in math::xyz_to_lab(xyz, math::D50) {
            hash64(&mut h[1], v.to_bits());
        }
        for v in math::lab_to_xyz(math::xyz_to_lab(xyz, math::D50), math::D50) {
            hash64(&mut h[1], v.to_bits());
        }
        for v in math::xyz_to_lab_f32(xyz.map(|v| v as f32)) {
            hash32(&mut h[2], v);
        }
        for v in math::lab_to_xyz_f32(math::xyz_to_lab_f32(xyz.map(|v| v as f32))) {
            hash32(&mut h[2], v);
        }
    }
    for row in math::bradford(math::xy_to_xyz(math::D65_XY), math::D50) {
        for v in row {
            hash64(&mut h[3], v.to_bits());
        }
    }
    for prim in [[[0.64, 0.33], [0.3, 0.6], [0.15, 0.06]], [[0.7347, 0.2653], [0.1596, 0.8404], [0.0366, 0.0001]]] {
        for row in math::rgb_to_xyz_matrix(prim[0], prim[1], prim[2], math::D50).unwrap() {
            for v in row {
                hash64(&mut h[4], v.to_bits());
            }
        }
    }
    let curves = [
        curve::Curve::Identity,
        curve::Curve::Gamma(1.8),
        curve::srgb_trc(),
        curve::Curve::Parametric { kind: 0, p: [2.2, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0] },
        curve::Curve::Parametric { kind: 1, p: [2.0, 0.9, 0.1, 0.0, 0.0, 0.0, 0.0] },
        curve::Curve::Parametric { kind: 2, p: [2.0, 0.9, 0.1, 0.05, 0.0, 0.0, 0.0] },
        curve::Curve::Parametric { kind: 4, p: [2.4, 0.9, 0.05, 0.08, 0.04, 0.01, 0.002] },
        curve::Curve::Table(vec![0.0, 0.1, 0.8, 1.0]),
        curve::Curve::Table(vec![1.0, 0.5, 0.0]),
    ];
    for c in curves {
        for i in -16..=160 {
            let x = i as f64 / 128.0;
            hash64(&mut h[5], c.eval64(x).to_bits());
            hash64(&mut h[5], c.eval_inverse64(x).to_bits());
            hash32(&mut h[5], c.eval(x as f32));
            hash32(&mut h[5], c.eval_inverse(x as f32));
        }
    }
    assert_eq!(h, [0xd9f474318a940a25, 0xfd6c3cda35eed4e8, 0x2ed13de450e65442, 0xf3b582ec5ea0126d, 0x12dcecbeccd7b7bc, 0x4b5f046f19d2a7b0]);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn serialized_trc_legacy_reference_bits() {
    let mut h = 0xcbf29ce484222325;
    let curves = [
        Trc::Linear,
        Trc::Srgb,
        Trc::Gamma(1.8),
        Trc::Rec709,
        Trc::Parametric(vec![2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045]),
        Trc::Table(vec![0, 1000, 30000, 65535]),
    ];
    for c in curves {
        for i in -16..=160 {
            let x = i as f32 / 128.0;
            hash32(&mut h, c.to_linear(x));
            hash32(&mut h, c.from_linear(x));
        }
    }
    assert_eq!(h, 0xa325b0d28b0a37fd);
}

#[test]
fn native_f32_lab_reference_accuracy() {
    let mut max_error = 0.0f64;
    for r in 0..=16 {
        for g in 0..=16 {
            for b in 0..=16 {
                let xyz = [r as f64 / 16.0, g as f64 / 16.0, b as f64 / 16.0];
                let reference = math::xyz_to_lab(xyz, math::D50);
                let hot = math::xyz_to_lab_f32(xyz.map(|x| x as f32));
                for i in 0..3 {
                    max_error = max_error.max((reference[i] - hot[i] as f64).abs());
                }
            }
        }
    }
    assert!(max_error < 4e-5, "native f32 Lab max error {max_error}");
}
