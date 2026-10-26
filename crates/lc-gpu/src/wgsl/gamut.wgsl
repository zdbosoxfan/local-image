// `lightcraft_pipeline::gamut`: hue-preserving gamut compression along the constant-lightness,
// constant-hue OkLab line through a colour (cube-rooted LMS `l + t·d`, see `Line`).
fn gamut_at(m: array<vec3<f32>, 3>, l: f32, d: vec3<f32>, t: f32) -> vec3<f32> {
    let q = vec3<f32>(l) + t * d;
    return mul3(m, q * q * q);
}

// `Line::depth`: how far inside the gamut chroma scale `t` lies across the zone `z0..z1` (0 to 1;
// a hard step when `z1 <= z0`), and in `.y` 1 when a channel lies within the zone.
fn gamut_depth(m: array<vec3<f32>, 3>, l: f32, d: vec3<f32>, t: f32, upper: bool, z0: f32, z1: f32) -> vec2<f32> {
    let n = l * l * l;
    let v = gamut_at(m, l, d, t);
    var depth = 1.0;
    var within = 0.0;
    for (var i = 0u; i < 3u; i++) {
        for (var f = 0u; f < 2u; f++) {
            if (f == 1u && !upper) { continue; }
            var margin = v[i] / n;
            if (f == 1u) { margin = (1.0 - v[i]) / (1.0 - n); }
            var x = 0.0;
            if (z1 > z0) {
                x = (margin - z0) / (z1 - z0);
            } else if (margin >= z1) {
                x = 1.0;
            }
            if (x > 0.0 && x < 1.0) { within = 1.0; }
            depth = min(depth, x);
        }
    }
    return vec2<f32>(clamp(depth, 0.0, 1.0), within);
}

// `Line::inside`
fn gamut_inside(m: array<vec3<f32>, 3>, l: f32, d: vec3<f32>, t: f32, upper: bool, shift: f32) -> bool {
    return gamut_depth(m, l, d, t, upper, shift, shift).x >= 1.0;
}

// `Line::reach`
fn gamut_reach(l: f32, c: f32, d: vec3<f32>) -> f32 {
    var t = GAMUT_MAX_CHROMA / c;
    for (var i = 0u; i < 3u; i++) {
        if (d[i] < 0.0) { t = min(t, l / -d[i]); }
    }
    return t;
}

fn gamut_horner(c: vec4<f32>, t: f32) -> f32 {
    return ((c.w * t + c.z) * t + c.y) * t + c.x;
}

// `Line::faces`: face `f` of channel `i` (0: its face at 0, 1: at 1) as a cubic that is >= 0
// where the line is `shift` inside it.
fn gamut_face(m: array<vec3<f32>, 3>, l: f32, d: vec3<f32>, i: u32, f: u32, shift: f32) -> vec4<f32> {
    var rows = m;
    let row = rows[i];
    var c = vec4<f32>(0.0);
    for (var j = 0u; j < 3u; j++) {
        let w = row[j];
        let dj = d[j];
        c.x += w * l * l * l;
        c.y += w * 3.0 * l * l * dj;
        c.z += w * 3.0 * l * dj * dj;
        c.w += w * dj * dj * dj;
    }
    let n = l * l * l;
    if (f == 0u) { return vec4<f32>(c.x - shift * n, c.y, c.z, c.w); }
    return vec4<f32>(1.0 - shift * (1.0 - n) - c.x, -c.y, -c.z, -c.w);
}

// `pieces`: the bounds of the cubic's monotone pieces in 0..hi (0, the turning points, then hi).
fn gamut_pieces(c: vec4<f32>, hi: f32) -> vec4<f32> {
    let a = 3.0 * c.w;
    let b = 2.0 * c.z;
    let cc = c.y;
    var cuts = vec4<f32>(0.0, hi, hi, hi);
    var n = 1u;
    let disc = b * b - 4.0 * a * cc;
    if (disc >= 0.0 && (a != 0.0 || b != 0.0)) {
        var sq = sqrt(disc);
        if (b < 0.0) { sq = -sq; }
        let q = -0.5 * (b + sq);
        if (a != 0.0) {
            let r = q / a;
            if (r > 0.0 && r < hi) { cuts[n] = r; n += 1u; }
        }
        if (q != 0.0) {
            let r = cc / q;
            if (r > 0.0 && r < hi) { cuts[n] = r; n += 1u; }
        }
        if (n == 3u && cuts.y > cuts.z) {
            let x = cuts.y;
            cuts.y = cuts.z;
            cuts.z = x;
        }
    }
    cuts[n] = hi;
    return cuts;
}

// How many pieces `gamut_pieces` found.
fn gamut_piece_count(p: vec4<f32>, hi: f32) -> u32 {
    if (p.y >= hi) { return 1u; }
    if (p.z >= hi) { return 2u; }
    return 3u;
}

// `Line::stays_inside`
fn gamut_stays_inside(m: array<vec3<f32>, 3>, l: f32, d: vec3<f32>, upper: bool, shift: f32, hi: f32) -> bool {
    for (var i = 0u; i < 3u; i++) {
        for (var f = 0u; f < 2u; f++) {
            if (f == 1u && !upper) { continue; }
            let c = gamut_face(m, l, d, i, f, shift);
            let p = gamut_pieces(c, hi);
            let n = gamut_piece_count(p, hi);
            for (var k = 0u; k <= n; k++) {
                if (gamut_horner(c, p[k]) < 0.0) { return false; }
            }
        }
    }
    return true;
}

// `Line::extent`, with `sign_changes` inline.
fn gamut_extent(m: array<vec3<f32>, 3>, l: f32, d: vec3<f32>, upper: bool, z0: f32, z1: f32, hi: f32) -> f32 {
    var cuts: array<f32, 38>;
    cuts[0] = 0.0;
    cuts[1] = hi;
    var count = 2u;
    for (var z = 0u; z < 2u; z++) {
        var shift = z0;
        if (z == 1u) { shift = z1; }
        for (var i = 0u; i < 3u; i++) {
            for (var f = 0u; f < 2u; f++) {
                if (f == 1u && !upper) { continue; }
                let c = gamut_face(m, l, d, i, f, shift);
                let p = gamut_pieces(c, hi);
                let np = gamut_piece_count(p, hi);
                for (var k = 0u; k < np; k++) {
                    var lo = p[k];
                    var up = p[k + 1u];
                    let below = gamut_horner(c, lo) < 0.0;
                    if (below == (gamut_horner(c, up) < 0.0)) { continue; }
                    for (var s = 0u; s < GAMUT_STEPS; s++) {
                        let mid = 0.5 * (lo + up);
                        if ((gamut_horner(c, mid) < 0.0) == below) { lo = mid; } else { up = mid; }
                    }
                    let flo = gamut_horner(c, lo);
                    let fup = gamut_horner(c, up);
                    var root = 0.5 * (lo + up);
                    if (flo != fup) { root = clamp(lo + (up - lo) * flo / (flo - fup), lo, up); }
                    cuts[count] = root;
                    count += 1u;
                }
            }
        }
    }
    for (var i = 1u; i < count; i++) {
        let x = cuts[i];
        var j = i;
        while (j > 0u && cuts[j - 1u] > x) {
            cuts[j] = cuts[j - 1u];
            j -= 1u;
        }
        cuts[j] = x;
    }
    let gauss = 0.7745967;
    var total = 0.0;
    for (var i = 1u; i < count; i++) {
        let mid = 0.5 * (cuts[i - 1u] + cuts[i]);
        let half = 0.5 * (cuts[i] - cuts[i - 1u]);
        let centre = gamut_depth(m, l, d, mid, upper, z0, z1);
        if (centre.y == 0.0) {
            total += centre.x * 2.0 * half;
        } else {
            let a = gamut_depth(m, l, d, mid - gauss * half, upper, z0, z1).x;
            let b = gamut_depth(m, l, d, mid + gauss * half, upper, z0, z1).x;
            total += half * (5.0 * a + 8.0 * centre.x + 5.0 * b) / 9.0;
        }
    }
    return total;
}

// `gamut::knee_curve`
fn gamut_knee_curve(s: f32, knee: f32, limit: f32) -> f32 {
    if (s <= knee || limit <= 1.0 + 1e-6) { return s; }
    let p = 1.2;
    let scale = (limit - knee) / pow(pow((1.0 - knee) / (limit - knee), -p) - 1.0, 1.0 / p);
    let u = (s - knee) / scale;
    return knee + scale * u / pow(1.0 + pow(u, p), 1.0 / p);
}

// `Line::compressed`; negative when the colour stays where it is.
fn gamut_compressed(extent: f32, knee: f32, limit: f32) -> f32 {
    if (!(extent > 0.0)) { return 0.0; }
    let s = 1.0 / extent;
    let mapped = min(gamut_knee_curve(s, knee, limit), 1.0);
    if (mapped == s) { return -1.0; }
    return mapped * extent;
}

// `Line::new`: (l, chroma) of the line, l = 0 for no line; `gamut_dir` gives its d.
fn gamut_line(lab: vec3<f32>) -> vec2<f32> {
    let c = length(lab.yz);
    if (!(lab.x > 0.0 && c > 1e-7)) { return vec2<f32>(0.0); }
    return vec2<f32>(lab.x, c);
}
fn gamut_dir(lab: vec3<f32>) -> vec3<f32> {
    let a = lab.y;
    let b = lab.z;
    return vec3<f32>(OK_FROM_LAB[0].y * a + OK_FROM_LAB[0].z * b, OK_FROM_LAB[1].y * a + OK_FROM_LAB[1].z * b, OK_FROM_LAB[2].y * a + OK_FROM_LAB[2].z * b);
}

// `Line::scene`; `.w` is 0 inside the knee.
fn gamut_scene_line(l: f32, c: f32, d: vec3<f32>, knee: f32, limit: f32) -> vec4<f32> {
    if (gamut_stays_inside(OK_FROM_LMS, l, d, false, 0.0, 1.0 / knee)) { return vec4<f32>(0.0); }
    let t = gamut_compressed(gamut_extent(OK_FROM_LMS, l, d, false, 0.0, 0.0, gamut_reach(l, c, d)), knee, limit);
    if (t < 0.0) { return vec4<f32>(0.0); }
    return vec4<f32>(gamut_at(OK_FROM_LMS, l, d, t), 1.0);
}

// `gamut::scene`
fn gamut_scene(c: vec3<f32>, knee: f32, limit: f32) -> vec3<f32> {
    let lab = oklab(c);
    let line = gamut_line(lab);
    if (line.x <= 0.0) { return c; }
    let r = gamut_scene_line(line.x, line.y, gamut_dir(lab), knee, limit);
    if (r.w == 0.0) { return c; }
    return r.xyz;
}

// `gamut::scene_lab`
fn gamut_scene_lab(lab: vec3<f32>, knee: f32, limit: f32) -> vec3<f32> {
    let line = gamut_line(lab);
    if (line.x <= 0.0) { return oklab_inv(lab); }
    let d = gamut_dir(lab);
    let r = gamut_scene_line(line.x, line.y, d, knee, limit);
    if (r.w == 0.0) { return gamut_at(OK_FROM_LMS, line.x, d, 1.0); }
    return r.xyz;
}

// `gamut::output`: `dd` linear Rec. 2020, `r` the same colour through the output matrix, `m` the
// output's `gamut::lms_to` matrix.
fn gamut_output(dd: vec3<f32>, r: vec3<f32>, m: array<vec3<f32>, 3>, knee: f32) -> vec3<f32> {
    let lab = oklab(dd);
    let line = gamut_line(lab);
    let l = line.x;
    if (l <= 0.0 || l >= 1.0) { return r; }
    let d = gamut_dir(lab);
    if (gamut_stays_inside(m, l, d, true, GAMUT_ZONE_IN, 1.0 / knee) || !gamut_inside(m, l, d, 0.0, true, 0.0)) { return r; }
    let hi = gamut_reach(l, line.y, d);
    let o = gamut_extent(m, l, d, true, GAMUT_ZONE_OUT, GAMUT_ZONE_IN, hi);
    let w = gamut_extent(OK_FROM_LMS, l, d, true, GAMUT_ZONE_OUT, GAMUT_ZONE_IN, hi);
    let t = gamut_compressed(o, knee, w / o);
    if (t < 0.0) { return r; }
    return gamut_at(m, l, d, t);
}
