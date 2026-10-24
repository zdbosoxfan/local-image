// Separable resample with precomputed taps (`li_raster::resample::weights`, uploaded):
// table[3i..3i+3] = (first source index, tap count, weight offset), weights stored as f32 bits.
// With P[4] set, each output is the anti-ringing sum of `li_raster::resample::antiring_taps`
// with the slack in P[5].
// Bindings: src, table (u32), dst.

fn tap(i: u32) -> f32 {
    return bitcast<f32>(table[i]);
}

// Horizontal. P: sw, h, dw, nc, antiring?, slack.
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
    var w_max = 0.0;
    for (var k = 0u; k < cnt; k++) {
        w_max = max(w_max, tap(off + k));
    }
    for (var c = 0u; c < nc; c++) {
        var acc = 0.0;
        var lobe = 0.0;
        var lobe_w = 0.0;
        for (var k = 0u; k < cnt; k++) {
            let s = src[(y * sw + lo + k) * nc + c];
            let wt = tap(off + k);
            let cw = wt * antiring_taper(wt, w_max);
            acc = acc + s * wt;
            lobe = lobe + s * cw;
            lobe_w = lobe_w + cw;
        }
        if (ar && lobe_w > 0.0) {
            let mean = lobe / lobe_w;
            var dlo = 0.0;
            var dhi = 0.0;
            for (var k = 0u; k < cnt; k++) {
                let d = (src[(y * sw + lo + k) * nc + c] - mean) * antiring_taper(tap(off + k), w_max);
                dlo = min(dlo, d);
                dhi = max(dhi, d);
            }
            acc = antiring(acc, mean + dlo, mean + dhi, pf(5u));
        }
        dst[(y * dw + x) * nc + c] = acc;
    }
}

// Vertical. P: w, sh, dh, nc, antiring?, slack.
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
    var w_max = 0.0;
    for (var k = 0u; k < cnt; k++) {
        w_max = max(w_max, tap(off + k));
    }
    for (var c = 0u; c < nc; c++) {
        var acc = 0.0;
        var lobe = 0.0;
        var lobe_w = 0.0;
        for (var k = 0u; k < cnt; k++) {
            let s = src[((lo + k) * w + x) * nc + c];
            let wt = tap(off + k);
            let cw = wt * antiring_taper(wt, w_max);
            acc = acc + s * wt;
            lobe = lobe + s * cw;
            lobe_w = lobe_w + cw;
        }
        if (ar && lobe_w > 0.0) {
            let mean = lobe / lobe_w;
            var dlo = 0.0;
            var dhi = 0.0;
            for (var k = 0u; k < cnt; k++) {
                let d = (src[((lo + k) * w + x) * nc + c] - mean) * antiring_taper(tap(off + k), w_max);
                dlo = min(dlo, d);
                dhi = max(dhi, d);
            }
            acc = antiring(acc, mean + dlo, mean + dhi, pf(5u));
        }
        dst[(y * w + x) * nc + c] = acc;
    }
}
