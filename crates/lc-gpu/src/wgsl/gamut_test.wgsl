// Compiled only in unit-test devices: `gamut_output` (sRGB-sized parameters in P) and
// `gamut_scene` on a list of linear Rec. 2020 colours.
@compute @workgroup_size(256)
fn gamut_probe(@builtin(global_invocation_id) g: vec3<u32>, @builtin(num_workgroups) ng: vec3<u32>) {
    let i = lin_index(g, ng);
    if (i >= pu(0u)) { return; }
    let d = vec3<f32>(a[3u * i], a[3u * i + 1u], a[3u * i + 2u]);
    let om = array<vec3<f32>, 3>(vec3<f32>(pf(1u), pf(2u), pf(3u)), vec3<f32>(pf(4u), pf(5u), pf(6u)), vec3<f32>(pf(7u), pf(8u), pf(9u)));
    let gm = array<vec3<f32>, 3>(vec3<f32>(pf(10u), pf(11u), pf(12u)), vec3<f32>(pf(13u), pf(14u), pf(15u)), vec3<f32>(pf(16u), pf(17u), pf(18u)));
    let o = gamut_output(d, mul3(om, d), gm, GAMUT_KNEE);
    let s = gamut_scene(d, SCENE_GAMUT_KNEE, SCENE_GAMUT_LIMIT);
    dst[6u * i] = o.x;
    dst[6u * i + 1u] = o.y;
    dst[6u * i + 2u] = o.z;
    dst[6u * i + 3u] = s.x;
    dst[6u * i + 4u] = s.y;
    dst[6u * i + 5u] = s.z;
}
