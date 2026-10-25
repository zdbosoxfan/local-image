//! Device tests for the hue-preserving gamut knees (`gamut_*` in `gamut.wgsl`) against
//! `lightcraft_pipeline::gamut` on the CPU.
use super::*;
use lightcraft_pipeline::OutputSpace;
use lightcraft_pipeline::finish::GAMUT_KNEE;
use lightcraft_pipeline::gamut;
use lightcraft_pipeline::tone2::{SCENE_GAMUT_KNEE, SCENE_GAMUT_LIMIT};

fn mul3(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// Colours from neutral to far past Rec. 2020 along constant OkLab hue and lightness, and a
/// pseudo-random spread including negative channels.
fn colours() -> Vec<[f32; 3]> {
    let mut out = Vec::new();
    for l in [0.03f32, 0.2, 0.45, 0.6, 0.75, 0.9, 0.97] {
        for hi in 0..72 {
            let h = hi as f32 * std::f32::consts::TAU / 72.0;
            for ci in 0..40 {
                let c = ci as f32 * 0.012;
                out.push(li_color::perceptual::oklab_to_2020([l, c * h.cos(), c * h.sin()]));
            }
        }
    }
    for c in [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [-0.1, 0.5, 0.2], [0.8, -0.02, 0.1]] {
        for exposure in [1e-6, 0.01, 10.0, 1000.0, 1e6, 1e30] {
            out.push(c.map(|v| v * exposure));
        }
    }
    let mut state = 0x9e37_79b9u32;
    for _ in 0..20_000 {
        out.push(std::array::from_fn(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state >> 8) as f32 / (1 << 24) as f32 * 1.3 - 0.15
        }));
    }
    out
}

#[test]
fn gamut_knees_match_the_cpu() {
    let gpu_device = crate::test_device().unwrap_or_else(|| panic!("real GPU required: {:?}", crate::unavailable_reason()));
    let g = &*gpu_device;
    let _scope = crate::ctx::RenderScope::new(g);
    let colours = colours();
    for space in [OutputSpace::Srgb, OutputSpace::DisplayP3, OutputSpace::AdobeRgb, OutputSpace::ProPhoto] {
        let to_out = space.from_working();
        let lms = gamut::lms_to(&to_out);
        let mut p = vec![colours.len() as u32];
        p.extend(to_out.as_flattened().iter().chain(lms.as_flattened()).map(|v| v.to_bits()));
        let mut cx = Cx::new(g);
        let input = g.upload(colours.as_flattened());
        let out = g.buffer(colours.len() * 6);
        cx.run("gamut_probe", &p, &[Some(&input), Some(&out)], groups1(colours.len()));
        let gpu: Vec<f32> = cx.read(&out, colours.len() * 6);
        let (mut worst_out, mut worst_scene) = ((0f32, [0f32; 3]), (0f32, [0f32; 3]));
        for (d, q) in colours.iter().zip(gpu.as_chunks::<6>().0) {
            let cpu_out = gamut::output(*d, mul3(&to_out, *d), &lms, GAMUT_KNEE);
            let cpu_scene = gamut::scene(*d, SCENE_GAMUT_KNEE, SCENE_GAMUT_LIMIT);
            assert!(q.iter().chain(&cpu_out).chain(&cpu_scene).all(|v| v.is_finite()), "{} at {d:?}: {q:?}", space.label());
            let scale = |c: [f32; 3]| c.iter().fold(1f32, |m, v| m.max(v.abs()));
            let e_out = (0..3).map(|k| (cpu_out[k] - q[k]).abs()).fold(0f32, f32::max) / scale(cpu_out);
            let e_scene = (0..3).map(|k| (cpu_scene[k] - q[3 + k]).abs()).fold(0f32, f32::max) / scale(cpu_scene);
            if e_out > worst_out.0 {
                worst_out = (e_out, *d);
            }
            if e_scene > worst_scene.0 {
                worst_scene = (e_scene, *d);
            }
        }
        // the renderers' cube roots differ in the last bits; along ProPhoto's folds past Rec. 2020
        // that moves a colour by a few 1e-4
        let bound = if space == OutputSpace::ProPhoto { 1e-3 } else { 2e-4 };
        assert!(worst_out.0 < bound, "{}: output knee differs by {} at {:?}", space.label(), worst_out.0, worst_out.1);
        assert!(worst_scene.0 < 2e-4, "scene knee differs by {} at {:?}", worst_scene.0, worst_scene.1);
    }
}
