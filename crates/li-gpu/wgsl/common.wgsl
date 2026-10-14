// Shared display helpers for Develop, canvas and compositor. No bindings or entry points.
// sRGB transfer: IEC 61966-2-1, https://www.w3.org/Graphics/Color/srgb . Keep the
// existing CPU convention for negative values (linear segment) and unclipped HDR values.
fn srgb_to_linear(v: f32) -> f32 {
    if (v == 1.0) { return 1.0; }
    if (v <= 0.04045) { return v / 12.92; }
    return pow((v + 0.055) / 1.055, 2.4);
}

fn linear_to_srgb(v: f32) -> f32 {
    // Preserve the exact IEC endpoint through half-float stores even with directed rounding.
    if (v == 1.0) { return 1.0; }
    if (v <= 0.0031308) { return v * 12.92; }
    return 1.055 * pow(v, 1.0 / 2.4) - 0.055;
}

// Scalar branches avoid evaluating pow with a negative base in a vector select.
fn srgb_to_linear3(c: vec3<f32>) -> vec3<f32> {
    return vec3(srgb_to_linear(c.x), srgb_to_linear(c.y), srgb_to_linear(c.z));
}

fn linear_to_srgb3(c: vec3<f32>) -> vec3<f32> {
    return vec3(linear_to_srgb(c.x), linear_to_srgb(c.y), linear_to_srgb(c.z));
}

// Exactly the position hash and 16-bit fractions of lightcraft_pipeline::finish::dither8.
// All RGB channels use the same sample, preserving neutral gradients. Tool-specific grain,
// Dissolve and gradient-fill hashes remain separate, as their CPU twins define their looks.
fn li_dither_tpdf(xy: vec2<u32>) -> f32 {
    var h = xy.x * 0x9e3779b9u ^ xy.y * 0x7feb352du;
    h ^= h >> 16u;
    h *= 0x7feb352du;
    h ^= h >> 15u;
    h *= 0x846ca68bu;
    h ^= h >> 16u;
    return f32(h & 65535u) / 65536.0 - f32(h >> 16u) / 65536.0;
}

// Dithered code value before rounding, for 8/10/16-bit output. Fade over the final
// code at either end so black and white stay exact. levels must be positive.
fn li_dither_code(v: f32, xy: vec2<u32>, levels: f32) -> f32 {
    let q = clamp(v, 0.0, 1.0) * levels;
    let fade = clamp(min(q, levels - q), 0.0, 1.0);
    return clamp(q + li_dither_tpdf(xy) * fade, 0.0, levels);
}

// Select the logical output code before the render target's implementation-defined
// conversion. This matches the CPU preview's nearest-code quantisation on 8/10-bit panels.
fn li_dither_display(v: f32, xy: vec2<u32>, levels: f32) -> f32 {
    let code = floor(li_dither_code(v, xy, levels) + 0.5);
    // Division may use an approximate reciprocal; exact endpoints must survive every driver.
    if (code == levels) { return 1.0; }
    return code / levels;
}

fn li_dither8(v: f32, xy: vec2<u32>) -> u32 {
    return u32(clamp(li_dither_code(v, xy, 255.0) + 0.5, 0.0, 255.0));
}

// Existing analytic canvas-shadow Gaussian integral; this is not an image blur kernel.
// Preserve the approximation and operation order to retain the existing shadow appearance.
fn erf2(x: vec2<f32>) -> vec2<f32> {
    let s = sign(x);
    let a = abs(x);
    var t = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
    t = t * t;
    return s - s / (t * t);
}

// Coverage of [lo, hi] blurred by Gaussian standard deviation sigma (positive).
fn box_blur(p: vec2<f32>, lo: vec2<f32>, hi: vec2<f32>, sigma: f32) -> f32 {
    let k = 0.70710678 / sigma;
    let q = (erf2((p - lo) * k) - erf2((p - hi) * k)) * 0.5;
    return q.x * q.y;
}
