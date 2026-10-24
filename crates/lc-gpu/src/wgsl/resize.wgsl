// Separable resample with precomputed taps (`li_raster::resample::weights`, uploaded):
// table[3i..3i+3] = (first source index, tap count, weight offset), weights stored as f32 bits.
// With P[4] set, ringing is suppressed as in `li_raster::resample::resize_antiring`, with the slack
// in P[5]: with a slack the horizontal pass also writes each output clamped without it to `exact`,
// where the vertical pass measures its range, so the two passes' slack does not compound.
// Bindings: src, table (u32), dst, exact.

fn tap(i: u32) -> f32 {
    return bitcast<f32>(table[i]);
}

fn max_tap(off: u32, cnt: u32) -> f32 {
    var w_max = 0.0;
    for (var k = 0u; k < cnt; k++) {
        w_max = max(w_max, tap(off + k));
    }
    return w_max;
}

// Horizontal. P: sw, h, dw, nc, antiring?, slack.
@compute @workgroup_size(16, 16)
fn resize_h(@builtin(global_invocation_id) g: vec3<u32>) {
    let sw = pu(0u);
    let h = pu(1u);
    let dw = pu(2u);
    let nc = pu(3u);
    let ar = pu(4u) != 0u;
    let slack = pf(5u);
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
        for (var k = 0u; k < cnt; k++) {
            acc = acc + src[(y * sw + lo + k) * nc + c] * tap(off + k);
        }
        let i = (y * dw + x) * nc + c;
        if (!ar) {
            dst[i] = acc;
            continue;
        }
        let w_max = max_tap(off, cnt);
        var lobe = 0.0;
        var lobe_w = 0.0;
        for (var k = 0u; k < cnt; k++) {
            let wt = tap(off + k);
            let cw = wt * antiring_taper(wt, w_max);
            lobe = lobe + src[(y * sw + lo + k) * nc + c] * cw;
            lobe_w = lobe_w + cw;
        }
        var v = acc;
        var clamped = acc;
        if (lobe_w > 0.0) {
            let mean = lobe / lobe_w;
            var dlo = 0.0;
            var dhi = 0.0;
            for (var k = 0u; k < cnt; k++) {
                let d = (src[(y * sw + lo + k) * nc + c] - mean) * antiring_taper(tap(off + k), w_max);
                dlo = min(dlo, d);
                dhi = max(dhi, d);
            }
            v = antiring(acc, mean + dlo, mean + dhi, slack);
            clamped = antiring(acc, mean + dlo, mean + dhi, 0.0);
        }
        dst[i] = v;
        if (slack != 0.0) {
            exact[i] = clamped;
        }
    }
}

// The value the vertical pass measures its anti-ringing range on (see the header).
fn range_v(i: u32, split: bool) -> f32 {
    if (split) {
        return exact[i];
    }
    return src[i];
}

// Vertical. P: w, sh, dh, nc, antiring?, slack.
@compute @workgroup_size(16, 16)
fn resize_v(@builtin(global_invocation_id) g: vec3<u32>) {
    let w = pu(0u);
    let sh = pu(1u);
    let dh = pu(2u);
    let nc = pu(3u);
    let ar = pu(4u) != 0u;
    let slack = pf(5u);
    let split = slack != 0.0;
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
        for (var k = 0u; k < cnt; k++) {
            acc = acc + src[((lo + k) * w + x) * nc + c] * tap(off + k);
        }
        let i = (y * w + x) * nc + c;
        if (!ar) {
            dst[i] = acc;
            continue;
        }
        let w_max = max_tap(off, cnt);
        var lobe = 0.0;
        var lobe_w = 0.0;
        for (var k = 0u; k < cnt; k++) {
            let wt = tap(off + k);
            let cw = wt * antiring_taper(wt, w_max);
            lobe = lobe + range_v(((lo + k) * w + x) * nc + c, split) * cw;
            lobe_w = lobe_w + cw;
        }
        if (lobe_w > 0.0) {
            let mean = lobe / lobe_w;
            var dlo = 0.0;
            var dhi = 0.0;
            for (var k = 0u; k < cnt; k++) {
                let d = (range_v(((lo + k) * w + x) * nc + c, split) - mean) * antiring_taper(tap(off + k), w_max);
                dlo = min(dlo, d);
                dhi = max(dhi, d);
            }
            acc = antiring(acc, mean + dlo, mean + dhi, slack);
        }
        dst[i] = acc;
    }
}
