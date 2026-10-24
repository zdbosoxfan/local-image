// Separable resample with precomputed taps (`li_raster::resample::weights`, uploaded):
// table[3i..3i+3] = (first source index, tap count, weight offset), weights stored as f32 bits.
// With P[4] set, each output is passed through `antiring` (`li_raster::resample::resize_antiring`).
// Bindings: src, table (u32), dst.

// Horizontal. P: sw, h, dw, nc, antiring?.
@compute @workgroup_size(16, 16)
fn resize_h(@builtin(global_invocation_id) g: vec3<u32>) {
    let sw = pu(0u);
    let h = pu(1u);
    let dw = pu(2u);
    let nc = pu(3u);
    let ar = pu(4u) != 0u;
    let x = g.x;
    let y = g.y;
    if (x >= dw || y >= h) {
        return;
    }
    let lo = table[3u * x];
    let cnt = table[3u * x + 1u];
    let off = table[3u * x + 2u];
    for (var c = 0u; c < nc; c++) {
        var acc = 0.0;
        var r = ring_empty();
        for (var k = 0u; k < cnt; k++) {
            let s = src[(y * sw + lo + k) * nc + c];
            let wt = bitcast<f32>(table[off + k]);
            acc = acc + s * wt;
            r = ring_add(r, s, wt > 0.0);
        }
        if (ar) {
            acc = antiring(acc, r);
        }
        dst[(y * dw + x) * nc + c] = acc;
    }
}

// Vertical. P: w, sh, dh, nc, antiring?.
@compute @workgroup_size(16, 16)
fn resize_v(@builtin(global_invocation_id) g: vec3<u32>) {
    let w = pu(0u);
    let sh = pu(1u);
    let dh = pu(2u);
    let nc = pu(3u);
    let ar = pu(4u) != 0u;
    let x = g.x;
    let y = g.y;
    if (x >= w || y >= dh) {
        return;
    }
    let lo = table[3u * y];
    let cnt = table[3u * y + 1u];
    let off = table[3u * y + 2u];
    for (var c = 0u; c < nc; c++) {
        var acc = 0.0;
        var r = ring_empty();
        for (var k = 0u; k < cnt; k++) {
            let s = src[((lo + k) * w + x) * nc + c];
            let wt = bitcast<f32>(table[off + k]);
            acc = acc + s * wt;
            r = ring_add(r, s, wt > 0.0);
        }
        if (ar) {
            acc = antiring(acc, r);
        }
        dst[(y * w + x) * nc + c] = acc;
    }
}
