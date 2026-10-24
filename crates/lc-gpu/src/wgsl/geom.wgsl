// Geometry (`lightcraft_pipeline::geometry::Frame::sample`): orientation, and the single cubic
// resample through the crop/straighten/flip affine or the full lens + perspective warp.
// Bindings: src (rgb), dst (rgb), coverage (the warp's tables, see `sample_warp`).

fn src_px(w: u32, h: u32, x: i32, y: i32) -> vec3<f32> {
    let cx = u32(clamp(x, 0, i32(w) - 1));
    let cy = u32(clamp(y, 0, i32(h) - 1));
    let j = 3u * (cy * w + cx);
    return vec3<f32>(src[j], src[j + 1u], src[j + 2u]);
}

// `li_raster::resample::Filter::CatmullRom`.
fn catmull_rom(t: f32) -> f32 {
    let x = abs(t);
    if (x < 1.0) {
        return 1.5 * x * x * x - 2.5 * x * x + 1.0;
    }
    if (x < 2.0) {
        return -0.5 * x * x * x + 2.5 * x * x - 4.0 * x + 2.0;
    }
    return 0.0;
}

// One axis of `cubic` (`li_raster`'s `cubic_axis`): first source index, tap count, normalized
// weights and their `antiring_taper`; at most 17 taps, the kernel widened up to 4×.
struct CubicAxis {
    j0: i32,
    n: u32,
    w: array<f32, 17>,
    taper: array<f32, 17>,
}

fn cubic_axis(x: f32, n: u32, stretch: f32) -> CubicAxis {
    let s = clamp(stretch, 1.0, 4.0);
    let f = clamp(x, -1.0, f32(n) + 1.0) - 0.5;
    let j0 = floor(f - 2.0 * s) + 1.0;
    var a: CubicAxis;
    a.j0 = i32(j0);
    a.n = min(u32(ceil(f + 2.0 * s) - 1.0 - j0) + 1u, 17u);
    var sum = 0.0;
    for (var k = 0u; k < a.n; k++) {
        a.w[k] = catmull_rom((j0 + f32(k) - f) / s);
        sum = sum + a.w[k];
    }
    var w_max = 0.0;
    for (var k = 0u; k < a.n; k++) {
        a.w[k] = a.w[k] / sum;
        w_max = max(w_max, a.w[k]);
    }
    for (var k = 0u; k < a.n; k++) {
        a.taper[k] = antiring_taper(a.w[k], w_max);
    }
    return a;
}

// `RgbFrame::sample_cubic` (pixel centres at +0.5, clamped edges, ringing suppressed).
fn cubic(w: u32, h: u32, x: f32, y: f32, stretch: vec2<f32>, slack: f32) -> vec3<f32> {
    let ax = cubic_axis(x, w, stretch.x);
    let ay = cubic_axis(y, h, stretch.y);
    var acc = vec3<f32>(0.0);
    var lobe = vec3<f32>(0.0);
    var lobe_w = 0.0;
    for (var ky = 0u; ky < ay.n; ky++) {
        var row = vec3<f32>(0.0);
        var lobe_row = vec3<f32>(0.0);
        var lobe_row_w = 0.0;
        for (var kx = 0u; kx < ax.n; kx++) {
            let p = src_px(w, h, ax.j0 + i32(kx), ay.j0 + i32(ky));
            let c = ax.w[kx] * ax.taper[kx];
            row = row + p * ax.w[kx];
            lobe_row = lobe_row + p * c;
            lobe_row_w = lobe_row_w + c;
        }
        let c = ay.w[ky] * ay.taper[ky];
        acc = acc + row * ay.w[ky];
        lobe = lobe + lobe_row * c;
        lobe_w = lobe_w + lobe_row_w * c;
    }
    if (!(lobe_w > 0.0)) {
        return acc;
    }
    let mean = lobe / lobe_w;
    var dlo = vec3<f32>(0.0);
    var dhi = vec3<f32>(0.0);
    for (var ky = 0u; ky < ay.n; ky++) {
        for (var kx = 0u; kx < ax.n; kx++) {
            let t = ax.taper[kx] * ay.taper[ky];
            if (t > 0.0) {
                let d = (src_px(w, h, ax.j0 + i32(kx), ay.j0 + i32(ky)) - mean) * t;
                dlo = min(dlo, d);
                dhi = max(dhi, d);
            }
        }
    }
    let rlo = mean + dlo;
    let rhi = mean + dhi;
    return vec3<f32>(antiring(acc.x, rlo.x, rhi.x, slack), antiring(acc.y, rlo.y, rhi.y, slack), antiring(acc.z, rlo.z, rhi.z, slack));
}

fn put(i: u32, v: vec3<f32>) {
    dst[3u * i] = v.x;
    dst[3u * i + 1u] = v.y;
    dst[3u * i + 2u] = v.z;
}

// Orientation: dst (ow × oh) pixel (x, y) = src pixel (P[4]·x + P[5]·y + P[6], P[7]·x + P[8]·y + P[9])
// (integers). P: sw, sh, ow, oh, map[6].
@compute @workgroup_size(16, 16)
fn orient(@builtin(global_invocation_id) g: vec3<u32>) {
    let sw = pu(0u);
    let ow = pu(2u);
    let oh = pu(3u);
    if (g.x >= ow || g.y >= oh) {
        return;
    }
    let x = i32(g.x);
    let y = i32(g.y);
    let m = array<i32, 6>(bitcast<i32>(pu(4u)), bitcast<i32>(pu(5u)), bitcast<i32>(pu(6u)), bitcast<i32>(pu(7u)), bitcast<i32>(pu(8u)), bitcast<i32>(pu(9u)));
    let sx = u32(m[0] * x + m[1] * y + m[2]);
    let sy = u32(m[3] * x + m[4] * y + m[5]);
    let j = 3u * (sy * sw + sx);
    put(g.y * ow + g.x, vec3<f32>(src[j], src[j + 1u], src[j + 2u]));
}


// Cubic resample through an affine (output px → base px). P: bw, bh, w, h, affine[6], stretch[2]
// (`geometry::affine_stretch`), anti-ringing slack.
@compute @workgroup_size(16, 16)
fn sample_affine(@builtin(global_invocation_id) g: vec3<u32>) {
    let w = pu(2u);
    let h = pu(3u);
    if (g.x >= w || g.y >= h) {
        return;
    }
    let px = f32(g.x) + 0.5;
    let py = f32(g.y) + 0.5;
    let x = pf(4u) * px + pf(6u) * py + pf(8u);
    let y = pf(5u) * px + pf(7u) * py + pf(9u);
    put(g.y * w + g.x, cubic(pu(0u), pu(1u), x, y, vec2<f32>(pf(10u), pf(11u)), pf(12u)));
}

// The lens / perspective warp (`lightcraft_pipeline::optics::Warp`), in oriented-source px.
// P layout (see `geom_warp_params`): 0 bw, 1 bh, 2 w, 3 h, 4..10 output → transformed affine,
// 10 sx, 11 sy, 12 W, 13 H, 14 persp?, 15..24 persp_inv, 24 k1, 25..28 ca, 28 lens_dist,
// 29 warp?, 30..48 warp planes (3 × 6), 48 warp centre x, 49 y, 50 warp radius, 51 vig_stops,
// 52 vig_power, 53 lens_vig, 54 vignette?, 55..60 vignette k, 60 vignette centre x, 61 y,
// 62 vignette radius, 63 per-channel?, 64 gain?, 65 database distortion kind (0/1/2/3),
// 66..69 strength-scaled distortion k (poly3/poly5/ptlens), 69 TCA kind (0/1/2),
// 70..73 red TCA [v,c,b], 73..76 blue TCA [v,c,b] (linear v is strength-scaled),
// 76 database vignette?, 77..80 pa k, 80 ns, 81 cx, 82 cy, 83 TCA strength, 84 vig strength.
// 85 radial count, 86 attenuation count, 87..103 radial knots, 103..151 radial scales (3 × 16),
// 151..167 attenuation knots, 167..183 attenuation, 183 radial centre x, 184 y, 185 radius.
// 186 camera basis?, 187..196 working → camera matrix, 196..205 camera → working matrix (row-major).
// 205 DNG fisheye? (the existing warp's first four coefficients), 206 stretch block size,
// 207 anti-ringing slack.
// The coverage binding holds the framing bit mask (rows of ceil(w / 32) words), then the kernel
// widening per square block of P[206] output px (`geometry::warp_stretch`, f32 pairs, row-major).
const WP_N: u32 = 208u;

fn centre() -> vec2<f32> {
    return vec2<f32>(pf(12u) / 2.0, pf(13u) / 2.0);
}

fn to_corrected(p: vec2<f32>) -> vec2<f32> {
    if (pu(14u) == 0u) {
        return p;
    }
    let c = centre();
    let l = max(pf(12u), pf(13u)) / 2.0;
    let q = (p - c) / l;
    var wq = pf(21u) * q.x + pf(22u) * q.y + pf(23u);
    if (abs(wq) < 1e-30) {
        wq = 1e-30;
    }
    let x = (pf(15u) * q.x + pf(16u) * q.y + pf(17u)) / wq;
    let y = (pf(18u) * q.x + pf(19u) * q.y + pf(20u)) / wq;
    return c + vec2<f32>(x, y) * l;
}

fn embedded_warp(p: vec2<f32>, ch: u32) -> vec2<f32> {
    let w = pf(12u);
    let h = pf(13u);
    let cc = vec2<f32>(pf(48u) * w, pf(49u) * h);
    let m = max(pf(50u) * max(w, h), 1e-9);
    let o = 30u + 6u * min(ch, 2u);
    let d = (p - cc) / m;
    let r2 = d.x * d.x + d.y * d.y;
    if (pu(205u) != 0u) {
        // DNG WarpFisheye uses theta = atan(r), with the kr0 limit at
        // the optical centre. It has no tangential coefficients.
        let r = sqrt(r2);
        if (r == 0.0) {
            return cc + m * d * pf(o);
        }
        let theta = atan(r);
        let t2 = theta * theta;
        let f = theta / r * (pf(o) + t2 * (pf(o + 1u) + t2 * (pf(o + 2u) + t2 * pf(o + 3u))));
        return cc + m * d * f;
    }
    let f = pf(o) + r2 * (pf(o + 1u) + r2 * (pf(o + 2u) + r2 * pf(o + 3u)));
    let kt0 = pf(o + 4u);
    let kt1 = pf(o + 5u);
    let sx = d.x * f + kt0 * 2.0 * d.x * d.y + kt1 * (r2 + 2.0 * d.x * d.x);
    let sy = d.y * f + kt1 * 2.0 * d.x * d.y + kt0 * (r2 + 2.0 * d.y * d.y);
    return cc + m * vec2<f32>(sx, sy);
}

// Vendor tables use darktable's clamp-ended, piecewise linear interpolation.
fn embedded_radial_value(r: f32, count: u32, knots: u32, values: u32) -> f32 {
    if (count == 0u) {
        return 1.0;
    }
    if (r <= pf(knots)) {
        return pf(values);
    }
    for (var k = 1u; k < count; k++) {
        if (r <= pf(knots + k)) {
            let t = (r - pf(knots + k - 1u)) / (pf(knots + k) - pf(knots + k - 1u));
            return mix(pf(values + k - 1u), pf(values + k), t);
        }
    }
    return pf(values + count - 1u);
}

fn embedded_radial_centre() -> vec2<f32> {
    return vec2<f32>(pf(183u) * pf(12u), pf(184u) * pf(13u));
}

fn embedded_radial_radius(p: vec2<f32>) -> f32 {
    let m = max(pf(185u) * max(pf(12u), pf(13u)), 1e-9);
    return length(p - embedded_radial_centre()) / m;
}

// LensMap::to_source_real: distortion first, then TCA at the distorted radius.
fn database_warp(p: vec2<f32>, ch: u32) -> vec2<f32> {
    let dt = pu(65u);
    let tc = pu(69u);
    if (dt == 0u && (tc == 0u || ch == 1u)) {
        return p;
    }
    let cc = vec2<f32>(pf(81u), pf(82u));
    let ns = pf(80u);
    var d = (p - cc) * ns;
    let r2 = d.x * d.x + d.y * d.y;
    var f = 1.0;
    if (dt == 1u) {
        f = r2 * pf(66u) + 1.0;
    } else if (dt == 2u) {
        f = r2 * pf(66u) + r2 * r2 * pf(67u) + 1.0;
    } else if (dt == 3u) {
        let r = sqrt(max(r2, 0.0));
        f = r2 * r * pf(66u) + r2 * pf(67u) + r * pf(68u) + 1.0;
    }
    d *= f;
    if (tc != 0u && ch != 1u) {
        let o = select(73u, 70u, ch == 0u);
        var scale = pf(o);
        if (tc == 2u) {
            let dr2 = d.x * d.x + d.y * d.y;
            scale = dr2 * pf(o + 2u) + pf(o);
            if (pf(o + 1u) != 0.0) {
                scale = dr2 * pf(o + 2u) + sqrt(max(dr2, 0.0)) * pf(o + 1u) + pf(o);
            }
            scale = (scale - 1.0) * pf(83u) + 1.0;
        }
        d *= scale;
    }
    return d / ns + cc;
}

fn corrected_to_source(p: vec2<f32>, ch: u32) -> vec2<f32> {
    let c = centre();
    let hd = max(sqrt(pf(12u) * pf(12u) + pf(13u) * pf(13u)) / 2.0, 1e-9);
    var q = p;
    let k1 = pf(24u);
    if (k1 != 0.0) {
        let d = q - c;
        let r2 = (d.x * d.x + d.y * d.y) / (hd * hd);
        q = c + d * (1.0 + k1 * r2);
    }
    let ld = pf(28u);
    if (ld != 0.0 && pu(29u) != 0u) {
        let s = embedded_warp(q, ch);
        q = q + (s - q) * ld;
    }
    if (ld != 0.0 && pu(85u) != 0u) {
        let scale = embedded_radial_value(embedded_radial_radius(q), pu(85u), 87u, 103u + 16u * min(ch, 2u));
        let cc = embedded_radial_centre();
        q = cc + (q - cc) * (1.0 + (scale - 1.0) * ld);
    }
    q = database_warp(q, ch);
    let k = pf(25u + ch);
    if (k != 0.0) {
        q = c + (q - c) * (1.0 + k);
    }
    return q;
}

fn warp_gain(s: vec2<f32>) -> f32 {
    var g = 1.0;
    let stops = pf(51u);
    let w = pf(12u);
    let h = pf(13u);
    if (stops != 0.0) {
        let hd = max(sqrt(w * w + h * h) / 2.0, 1e-9);
        let r = min(length(s - centre()) / hd, 1.5);
        g *= exp2(stops * pow(r, pf(52u)));
    }
    let lv = pf(53u);
    if (lv != 0.0 && pu(54u) != 0u) {
        let cc = vec2<f32>(pf(60u) * w, pf(61u) * h);
        let m = max(pf(62u) * max(w, h), 1e-9);
        let d = s - cc;
        let r2 = (d.x * d.x + d.y * d.y) / (m * m);
        var vg = 1.0;
        var rp = r2;
        for (var k = 0u; k < 5u; k++) {
            vg += pf(55u + k) * rp;
            rp *= r2;
        }
        g *= 1.0 + (vg - 1.0) * lv;
    }
    if (lv != 0.0 && pu(86u) != 0u) {
        let attenuation = embedded_radial_value(embedded_radial_radius(s), pu(86u), 151u, 167u);
        g *= 1.0 / max(1.0 - lv * (1.0 - attenuation), 1e-6);
    }
    if (pu(76u) != 0u) {
        // LensMap::gain uses the GREEN source position, after manual CA.
        let d = (s - vec2<f32>(pf(81u), pf(82u))) * pf(80u);
        let r2 = d.x * d.x + d.y * d.y;
        let vg = 1.0 + pf(77u) * r2 + pf(78u) * r2 * r2 + pf(79u) * r2 * r2 * r2;
        if (vg > 1e-6) {
            g *= pow(1.0 / vg, pf(84u));
        }
    }
    return g;
}

fn camera_basis(v: vec3<f32>, offset: u32) -> vec3<f32> {
    return vec3<f32>(
        dot(v, vec3<f32>(pf(offset), pf(offset + 1u), pf(offset + 2u))),
        dot(v, vec3<f32>(pf(offset + 3u), pf(offset + 4u), pf(offset + 5u))),
        dot(v, vec3<f32>(pf(offset + 6u), pf(offset + 7u), pf(offset + 8u)))
    );
}

// `Frame::vignette_gain` divided out of (P[208] set) or applied to an output-size image (around
// noise reduction, `lightcraft_pipeline::denoise_sampled`). P as `sample_warp`; bindings: src (rgb),
// dst (rgb), coverage (the framing bit mask).
@compute @workgroup_size(16, 16)
fn vignette_gain(@builtin(global_invocation_id) g: vec3<u32>) {
    let w = pu(2u);
    let h = pu(3u);
    if (g.x >= w || g.y >= h) {
        return;
    }
    let i = g.y * w + g.x;
    let v = vec3<f32>(src[3u * i], src[3u * i + 1u], src[3u * i + 2u]);
    let word = g.y * ((w + 31u) / 32u) + g.x / 32u;
    var k = 1.0;
    if ((coverage[word] & (1u << (g.x % 32u))) != 0u) {
        let px = f32(g.x) + 0.5;
        let py = f32(g.y) + 0.5;
        let t = vec2<f32>(pf(4u) * px + pf(6u) * py + pf(8u), pf(5u) * px + pf(7u) * py + pf(9u));
        let gain = warp_gain(corrected_to_source(to_corrected(t), 1u));
        if (gain > 0.0 && gain < 3.0e38) {
            k = gain;
        }
    }
    put(i, select(v * k, v / k, pu(208u) != 0u));
}

@compute @workgroup_size(16, 16)
fn sample_warp(@builtin(global_invocation_id) g: vec3<u32>) {
    let w = pu(2u);
    let h = pu(3u);
    if (g.x >= w || g.y >= h) {
        return;
    }
    let bw = pu(0u);
    let bh = pu(1u);
    let px = f32(g.x) + 0.5;
    let py = f32(g.y) + 0.5;
    let t = vec2<f32>(pf(4u) * px + pf(6u) * py + pf(8u), pf(5u) * px + pf(7u) * py + pf(9u));
    let c = to_corrected(t);
    let s = corrected_to_source(c, 1u);
    let sx = pf(10u);
    let sy = pf(11u);
    // CPU reference coverage avoids an f32 rounding discontinuity at the edge.
    let word = g.y * ((w + 31u) / 32u) + g.x / 32u;
    if ((coverage[word] & (1u << (g.x % 32u))) == 0u) {
        put(g.y * w + g.x, vec3<f32>(BLANK_R, BLANK_G, BLANK_B));
        return;
    }
    let blk = pu(206u);
    let bi = h * ((w + 31u) / 32u) + 2u * ((g.y / blk) * ((w + blk - 1u) / blk) + g.x / blk);
    let st = vec2<f32>(bitcast<f32>(coverage[bi]), bitcast<f32>(coverage[bi + 1u]));
    var v = cubic(bw, bh, s.x * sx, s.y * sy, st, pf(207u));
    if (pu(63u) != 0u) {
        let qr = corrected_to_source(c, 0u);
        let qb = corrected_to_source(c, 2u);
        let red = cubic(bw, bh, qr.x * sx, qr.y * sy, st, pf(207u));
        let blue = cubic(bw, bh, qb.x * sx, qb.y * sy, st, pf(207u));
        if (pu(186u) != 0u) {
            // Calibrated CA refers to camera planes. Undo the loader's linear
            // colour conversion before assembling independently sampled planes.
            v = vec3<f32>(camera_basis(red, 187u).x, camera_basis(v, 187u).y, camera_basis(blue, 187u).z);
            v = camera_basis(v, 196u);
        } else {
            v = vec3<f32>(red.x, v.y, blue.z);
        }
    }
    if (pu(64u) != 0u) {
        v = v * warp_gain(s);
    }
    put(g.y * w + g.x, v);
}
