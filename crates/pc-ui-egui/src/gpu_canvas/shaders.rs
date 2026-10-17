//! WGSL sources of the canvas, sRGB-encode and mip-chain pipelines.

pub(super) const CANVAS_WGSL: &str = r#"
struct View {
    a: vec4<f32>, // screen_w, screen_h, scale (device px per doc px), pixels_per_point
    b: vec4<f32>, // doc origin x, y (device px), doc w, h (doc px)
    c: vec4<f32>, // filter mode, lod, grid alpha, checker square (device px)
    d: vec4<f32>, // checker anchor x, y (device px), display (0 none, 1 LUT, 2 LUT + gamut), output linear
    e: vec4<f32>, // checker light rgb, gamut warning opacity
    f: vec4<f32>, // checker dark rgb, 32-bit preview gain (2^exposure; 0 = off)
    g: vec4<f32>, // gamut warning rgb, 32-bit preview 1 / gamma
    h: vec4<f32>, // x direction (1, or -1 for View › Flip Horizontal), raster scale, rotation sine, cosine
    i: vec4<f32>, // output quantisation levels (0 disables dither), padding
};
struct Tile { r: vec4<f32>, s: vec4<f32> }; // core and sampling rect: x, y, w, h

@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var samp: sampler;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var<uniform> tile: Tile;
@group(2) @binding(0) var lut: texture_3d<f32>;

struct VOut { @builtin(position) pos: vec4<f32> };

fn corner(i: u32) -> vec2<f32> {
    var c = array<vec2<f32>, 6>(vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0));
    return c[i];
}

fn px_to_clip(p: vec2<f32>) -> vec4<f32> {
    let n = p / view.a.xy * 2.0 - 1.0;
    return vec4(n.x, -n.y, 0.0, 1.0);
}

// Same reflected, scaled rotation basis as view_camera::Camera (and its inverse).
fn doc_to_px(d: vec2<f32>) -> vec2<f32> {
    let q = vec2(d.x * view.h.x, d.y) * view.a.z;
    return view.b.xy + vec2(view.h.w * q.x - view.h.z * q.y, view.h.z * q.x + view.h.w * q.y);
}
fn px_to_doc(p: vec2<f32>) -> vec2<f32> {
    let q = (p - view.b.xy) / view.a.z;
    return vec2((view.h.w * q.x + view.h.z * q.y) * view.h.x, -view.h.z * q.x + view.h.w * q.y);
}
fn doc_min() -> vec2<f32> {
    return min(min(doc_to_px(vec2(0.0)), doc_to_px(view.b.zw)), min(doc_to_px(vec2(view.b.z, 0.0)), doc_to_px(vec2(0.0, view.b.w))));
}
fn doc_max() -> vec2<f32> {
    return max(max(doc_to_px(vec2(0.0)), doc_to_px(view.b.zw)), max(doc_to_px(vec2(view.b.z, 0.0)), doc_to_px(vec2(0.0, view.b.w))));
}

@vertex
fn vs_shadow(@builtin(vertex_index) vi: u32) -> VOut {
    let pad = 64.0 * view.a.w;
    let p = mix(doc_min() - vec2(pad), doc_max() + vec2(pad), corner(vi));
    return VOut(px_to_clip(p));
}

@fragment
fn fs_shadow(in: VOut) -> @location(0) vec4<f32> {
    let p = in.pos.xy;
    let d = px_to_doc(p);
    let u = view.a.w;
    if (view.h.z == 0.0 && view.h.w == 1.0) {
        // Preserve the existing upright shadow's arithmetic and coverage exactly.
        let lo = doc_min();
        let hi = doc_max();
        if (all(p >= lo) && all(p < hi)) { discard; }
        let ambient = box_blur(p - vec2(0.0, 7.0 * u), lo, hi, 16.0 * u) * 0.40;
        let contact = box_blur(p - vec2(0.0, 0.75 * u), lo, hi, 1.25 * u) * 0.30;
        return vec4(0.0, 0.0, 0.0, 1.0 - (1.0 - ambient) * (1.0 - contact));
    }
    if (all(d >= vec2(0.0)) && all(d < view.b.zw)) { discard; }
    // The box rotates with the document; light/shadow offsets remain screen-vertical.
    let ambient = box_blur(px_to_doc(p - vec2(0.0, 7.0 * u)), vec2(0.0), view.b.zw, 16.0 * u / view.a.z) * 0.40;
    let contact = box_blur(px_to_doc(p - vec2(0.0, 0.75 * u)), vec2(0.0), view.b.zw, 1.25 * u / view.a.z) * 0.30;
    let a = 1.0 - (1.0 - ambient) * (1.0 - contact);
    return vec4(0.0, 0.0, 0.0, a);
}

// --- document tiles ------------------------------------------------------------------------

@vertex
fn vs_tile(@builtin(vertex_index) vi: u32) -> VOut {
    let p = doc_to_px((tile.r.xy + tile.r.zw * corner(vi)) / view.h.y);
    return VOut(px_to_clip(p));
}

fn checker(p: vec2<f32>) -> vec3<f32> {
    if (view.c.w <= 0.0) {
        return vec3(1.0);
    }
    let c = floor((p - view.d.xy) / view.c.w);
    let odd = fract((c.x + c.y) * 0.5) > 0.25;
    return select(view.e.xyz, view.f.xyz, odd);
}

fn sample_tile(p: vec2<f32>) -> vec4<f32> {
    let scale = view.a.z / view.h.y;
    let d = px_to_doc(p) * view.h.y;                  // document pixel coordinates
    let size = tile.s.zw;
    let t = d - tile.s.xy;                 // texel coordinates within this tile
    let mode = view.c.x;
    var col: vec4<f32>;
    if (mode < 0.5) {
        // Zoomed out: trilinear through the mip chain.
        col = textureSampleLevel(tex, samp, t / size, view.c.y);
    } else if (mode < 1.5) {
        if (view.h.y > 1.0) {
            // Scaled geometry needs float interpolation: the hardware sampler's finite
            // subtexel weights can add another byte of error along fractional-zoom edges.
            let q = t - 0.5;
            let i = vec2<i32>(floor(q));
            let f = fract(q);
            let hi = vec2<i32>(textureDimensions(tex, 0)) - vec2<i32>(1);
            let a = textureLoad(tex, clamp(i, vec2<i32>(0), hi), 0);
            let b = textureLoad(tex, clamp(i + vec2<i32>(1, 0), vec2<i32>(0), hi), 0);
            let c = textureLoad(tex, clamp(i + vec2<i32>(0, 1), vec2<i32>(0), hi), 0);
            let d = textureLoad(tex, clamp(i + vec2<i32>(1, 1), vec2<i32>(0), hi), 0);
            col = mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
        } else {
            col = textureSampleLevel(tex, samp, t / size, 0.0);
        }
    } else if (mode > 2.5) {
        let hi = vec2<i32>(textureDimensions(tex, 0)) - vec2<i32>(1);
        col = textureLoad(tex, clamp(vec2<i32>(floor(t)), vec2<i32>(0), hi), 0);
    } else {
        // Crisp texels with a one-device-pixel anti-aliased edge ("sharp bilinear").
        let tt = t - 0.5;
        let i = floor(tt);
        let f = clamp((tt - i - 0.5) * scale + 0.5, vec2(0.0), vec2(1.0));
        col = textureSampleLevel(tex, samp, (i + 0.5 + f) / size, 0.0);
    }
    return col;
}

// CPU counterpart: li_color::lut::sample, tetrahedral branch. GPU uses f32 because
// portable WGSL has no f64; interpolate four exact texels rather than hardware trilinear.
fn display_lut_tetra(c: vec3<f32>) -> vec4<f32> {
    let n = i32(textureDimensions(lut).x);
    let pos = clamp(c, vec3(0.0), vec3(1.0)) * f32(n - 1);
    let lo = min(vec3<i32>(floor(pos)), vec3(n - 2));
    let f = pos - vec3<f32>(lo);
    var order = vec3(0, 1, 2);
    if (f.x >= f.y) {
        if (f.y >= f.z) { order = vec3(0, 1, 2); }
        else if (f.x >= f.z) { order = vec3(0, 2, 1); }
        else { order = vec3(2, 0, 1); }
    } else {
        if (f.x >= f.z) { order = vec3(1, 0, 2); }
        else if (f.y >= f.z) { order = vec3(1, 2, 0); }
        else { order = vec3(2, 1, 0); }
    }
    var d1 = vec3(0);
    d1[order.x] = 1;
    var d2 = d1;
    d2[order.y] = 1;
    let a = textureLoad(lut, lo, 0);
    let b = textureLoad(lut, lo + d1, 0);
    let c1 = textureLoad(lut, lo + d2, 0);
    let d = textureLoad(lut, lo + vec3(1), 0);
    let x = f[order.x];
    let y = f[order.y];
    let z = f[order.z];
    return a * (1.0 - x) + b * (x - y) + c1 * (y - z) + d * z;
}

fn display_color(input: vec4<f32>, p: vec2<f32>) -> vec4<f32> {
    var col = input;
    let d = px_to_doc(p);
    let scale = view.a.z;
    if (view.f.w > 0.0 && col.a > 0.0) {
        // View › 32-bit Preview Options: exposure and gamma in linear light on the sRGB-encoded
        // texture value, before the display LUT. A float canvas keeps values above 1.0, so they
        // are brought into range by the exposure instead of clipped.
        let lin = srgb_to_linear3(max(col.rgb / col.a, vec3(0.0))) * view.f.w;
        let v = linear_to_srgb3(min(pow(max(lin, vec3(1e-12)), vec3(view.g.w)), vec3(1.0)));
        col = vec4(v * col.a, col.a);
    }
    if (view.d.z > 0.5 && col.a > 0.0) {
        // Colour management, Proof Colors, Gamut Warning: the document's display LUT (alpha flags
        // out-of-gamut colours).
        let c = clamp(col.rgb / col.a, vec3(0.0), vec3(1.0));
        let l = display_lut_tetra(c);
        let shown = select(l.rgb, mix(l.rgb, view.g.xyz, view.e.w), view.d.z > 1.5 && l.a > 0.5);
        col = vec4(shown * col.a, col.a);
    }
    var rgb = col.rgb + checker(p) * (1.0 - col.a);
    let grid = view.c.z;
    if (grid > 0.0) {
        let e = fract(d) * scale;
        if (e.x < 1.0 || e.y < 1.0) {
            let l = dot(rgb, vec3(0.299, 0.587, 0.114));
            rgb = mix(rgb, select(vec3(1.0), vec3(0.0), l > 0.55), grid);
        }
    }
    if (view.i.x > 0.0) {
        let xy = vec2<u32>(p);
        rgb = vec3(
            li_dither_display(rgb.x, xy, view.i.x),
            li_dither_display(rgb.y, xy, view.i.x),
            li_dither_display(rgb.z, xy, view.i.x)
        );
    }
    if (view.d.w > 0.5) {
        rgb = srgb_to_linear3(rgb);
    }
    return vec4(rgb, 1.0);
}

@fragment
fn fs_tile(in: VOut) -> @location(0) vec4<f32> {
    return display_color(sample_tile(in.pos.xy), in.pos.xy);
}

// Blend the cached planes in document colour space before applying the display transform.
@fragment
fn fs_plane(in: VOut) -> @location(0) vec4<f32> {
    return sample_tile(in.pos.xy);
}

@vertex
fn vs_move(@builtin(vertex_index) vi: u32) -> VOut {
    return VOut(px_to_clip(doc_to_px(view.b.zw * corner(vi))));
}

@fragment
fn fs_move(in: VOut) -> @location(0) vec4<f32> {
    return display_color(textureLoad(tex, vec2<i32>(in.pos.xy), 0), in.pos.xy);
}
"#;

pub(super) const ENCODE_WGSL: &str = r#"
@group(0) @binding(0) var acc: texture_2d<f32>;
@group(0) @binding(1) var<uniform> offset: vec4<i32>; // x, y, encode sRGB, keep values above 1.0

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let c = textureLoad(acc, vec2<i32>(floor(pos.xy)) - offset.xy, 0);
    let a = clamp(c.a, 0.0, 1.0);
    var rgb = max(c.rgb, vec3(0.0));
    if (offset.w == 0) {
        rgb = min(rgb, vec3(1.0));
    }
    if (offset.z != 0) {
        // Linear composite: store its sRGB encoding (8-bit precision where it matters).
        rgb = linear_to_srgb3(rgb);
    }
    return vec4(rgb * a, a);
}
"#;

pub(super) const MIP_WGSL: &str = r#"
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct V { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> V {
    let uv = vec2(f32((vi << 1u) & 2u), f32(vi & 2u));
    return V(vec4(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0), uv);
}

@fragment
fn fs(in: V) -> @location(0) vec4<f32> {
    // A bilinear tap at the destination texel centre averages the 2×2 source block.
    return textureSampleLevel(src, samp, in.uv, 0.0);
}
"#;
