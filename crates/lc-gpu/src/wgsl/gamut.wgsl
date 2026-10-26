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

// A sign-changing monotone cubic bracket; the scene and output choose their own step counts.
fn gamut_root(c: vec4<f32>, lower: f32, upper: f32, steps: u32) -> f32 {
    var lo = lower;
    var up = upper;
    let below = gamut_horner(c, lo) < 0.0;
    for (var step = 0u; step < steps; step++) {
        let mid = 0.5 * (lo + up);
        if ((gamut_horner(c, mid) < 0.0) == below) { lo = mid; } else { up = mid; }
    }
    let flo = gamut_horner(c, lo);
    let fup = gamut_horner(c, up);
    if (flo != fup) { return clamp(lo + (up - lo) * flo / (flo - fup), lo, up); }
    return 0.5 * (lo + up);
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
                    let lo = p[k];
                    let up = p[k + 1u];
                    let below = gamut_horner(c, lo) < 0.0;
                    if (below == (gamut_horner(c, up) < 0.0)) { continue; }
                    let root = gamut_root(c, lo, up, GAMUT_STEPS);
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
    let normalized = d / l;
    if (gamut_stays_inside(OK_FROM_LMS, 1.0, normalized, false, 0.0, 1.0 / knee)) { return vec4<f32>(0.0); }
    let t = gamut_compressed(gamut_extent(OK_FROM_LMS, 1.0, normalized, false, 0.0, 0.0, gamut_reach(1.0, c / l, normalized)), knee, limit);
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

fn gamut_in_cube(rgb: vec3<f32>, eps: f32) -> bool {
    return all(rgb >= vec3<f32>(-eps)) && all(rgb <= vec3<f32>(1.0 + eps));
}

// `Line::output_chroma`: first exit walking out from gray; negative for invalid gray.
fn gamut_output_chroma(m: array<vec3<f32>, 3>, l: f32, d: vec3<f32>, input: f32) -> f32 {
    if (!gamut_in_cube(gamut_at(m, l, d, 0.0), GAMUT_OUTPUT_EPS)) { return -1.0; }
    var chroma = input;
    for (var i = 0u; i < 3u; i++) {
        for (var f = 0u; f < 2u; f++) {
            var face = gamut_face(m, l, d, i, f, 0.0);
            face.x += GAMUT_OUTPUT_EPS;
            let p = gamut_pieces(face, input);
            let np = gamut_piece_count(p, input);
            for (var k = 0u; k < np; k++) {
                var lo = p[k];
                var up = p[k + 1u];
                if (gamut_horner(face, lo) < 0.0 || gamut_horner(face, up) >= 0.0) { continue; }
                for (var step = 0u; step < GAMUT_OUTPUT_STEPS; step++) {
                    let mid = 0.5 * (lo + up);
                    if (gamut_horner(face, mid) >= 0.0) { lo = mid; } else { up = mid; }
                }
                chroma = min(chroma, lo);
            }
        }
    }
    return chroma;
}

// `Line::output_endpoint`; xyz destination RGB, w endpoint chroma.
fn gamut_output_endpoint(m: array<vec3<f32>, 3>, d: vec3<f32>) -> vec4<f32> {
    var qs: array<f32, 46>;
    qs[0] = 0.0;
    var count = 1u;
    let hi = 2.0 * GAMUT_MAX_CHROMA;
    for (var i = 0u; i < 3u; i++) {
        let face = gamut_face(m, 1.0, d, i, 0u, 0.0);
        let turns = gamut_pieces(face, hi);
        let nt = gamut_piece_count(turns, hi);
        for (var k = 1u; k <= nt; k++) { qs[count] = turns[k]; count += 1u; }
        // Expanded lower faces, then pairwise upper-envelope channel crossings.
        for (var j = 0u; j < 3u + i; j++) {
            var c: vec4<f32>;
            if (j < 3u) {
                c = (1.0 + GAMUT_OUTPUT_EPS) * face + GAMUT_OUTPUT_EPS * gamut_face(m, 1.0, d, j, 0u, 0.0);
            } else {
                c = face - gamut_face(m, 1.0, d, j - 3u, 0u, 0.0);
            }
            let p = gamut_pieces(c, hi);
            let np = gamut_piece_count(p, hi);
            for (var k = 0u; k < np; k++) {
                let lo = p[k];
                let up = p[k + 1u];
                let below = gamut_horner(c, lo) < 0.0;
                if (below == (gamut_horner(c, up) < 0.0)) { continue; }
                qs[count] = gamut_root(c, lo, up, GAMUT_OUTPUT_STEPS);
                count += 1u;
            }
        }
    }
    var minimum = 1e30;
    for (var i = 0u; i < count; i++) {
        let rgb = gamut_at(m, 1.0, d, qs[i]);
        let mx = max(rgb.x, max(rgb.y, rgb.z));
        let lower = -GAMUT_OUTPUT_EPS * mx / (1.0 + GAMUT_OUTPUT_EPS);
        if (all(rgb >= vec3<f32>(lower)) && mx > 0.0) { minimum = min(minimum, mx); }
    }
    var q = 0.0;
    for (var i = 0u; i < count; i++) {
        let rgb = gamut_at(m, 1.0, d, qs[i]);
        let mx = max(rgb.x, max(rgb.y, rgb.z));
        let lower = -GAMUT_OUTPUT_EPS * mx / (1.0 + GAMUT_OUTPUT_EPS);
        if (all(rgb >= vec3<f32>(lower)) && mx > 0.0 && mx <= minimum + GAMUT_ENDPOINT_EPS) { q = max(q, qs[i]); }
    }
    let rgb = gamut_at(m, 1.0, d, q);
    let mx = max(rgb.x, max(rgb.y, rgb.z));
    let scale = (1.0 + GAMUT_OUTPUT_EPS) / mx;
    let l = pow(scale, 1.0 / 3.0);
    return vec4<f32>(rgb * scale, q * l);
}

// `gamut::output`: first constant-L exit blended with three RGB casts near the blue pinches.
fn gamut_output(dd: vec3<f32>, r: vec3<f32>, m: array<vec3<f32>, 3>, band: u32) -> vec3<f32> {
    if (gamut_in_cube(r, GAMUT_OUTPUT_EPS)) { return r; }
    let lab = oklab(dd);
    if (lab.x <= 0.0) { return vec3<f32>(0.0); }
    let c = length(lab.yz);
    if (c <= 1e-7) {
        let l = min(lab.x, pow(1.0 + GAMUT_OUTPUT_EPS, 1.0 / 3.0));
        return clamp(mul3(m, vec3<f32>(l * l * l)), vec3<f32>(-GAMUT_OUTPUT_EPS), vec3<f32>(1.0 + GAMUT_OUTPUT_EPS));
    }
    let d = gamut_dir(vec3<f32>(0.0, lab.yz / c));
    var l = min(lab.x, 2.0);
    let input = min(c, GAMUT_MAX_CHROMA);
    var chroma = gamut_output_chroma(m, l, d, input);
    if (chroma < 0.0) {
        let endpoint = gamut_output_endpoint(m, d);
        if (endpoint.w <= input) { return endpoint.xyz; }
        var lo = 0.0;
        var up = l;
        chroma = 0.0;
        for (var step = 0u; step < GAMUT_OUTPUT_STEPS; step++) {
            l = 0.5 * (lo + up);
            let found = gamut_output_chroma(m, l, d, input);
            if (found >= 0.0) { lo = l; chroma = found; } else { up = l; }
        }
        l = lo;
    }
    let clipped = gamut_at(m, l, d, chroma);
    if (band == 0u) { return clipped; }
    var window = GAMUT_BLUE_SRGB;
    if (band == 2u) { window = GAMUT_BLUE_REC2020; }
    let center = 0.5 * (window.x + window.y);
    let hue = atan2(lab.z, lab.y);
    let angle = hue - center;
    let wrapped = angle - 6.283185307179586 * floor((angle + 3.141592653589793) / 6.283185307179586);
    let distance = abs(wrapped) - 0.5 * (window.y - window.x);
    let t = clamp((distance - 0.01) / 0.09, 0.0, 1.0);
    let weight = 1.0 - t * t * (3.0 - 2.0 * t);
    if (weight == 0.0) { return clipped; }
    let anchor = clamp(l, 1e-4, 1.0 - 1e-4);
    let gray = anchor * anchor * anchor;
    var target = r;
    var boundary = vec3<f32>(gray);
    for (var cast = 0u; cast < 3u; cast++) {
        let delta = target - vec3<f32>(gray);
        var scale = bitcast<f32>(0x7f800000u);
        for (var k = 0u; k < 3u; k++) {
            if (delta[k] > 0.0) { scale = min(scale, (1.0 - gray) / delta[k]); }
            else if (delta[k] < 0.0) { scale = min(scale, -gray / delta[k]); }
        }
        boundary = vec3<f32>(gray);
        if (scale < bitcast<f32>(0x7f800000u)) { boundary += scale * delta; }
        var working = boundary;
        if (band == 1u) { working = mul3(GAMUT_FROM_SRGB, boundary); }
        let cast_lab = oklab(working);
        target = gamut_at(m, l, d, length(cast_lab.yz));
    }
    return clipped + weight * (boundary - clipped);
}
