use lightcraft_raster::Rgb32f;
pub fn scene(kind: usize, w: usize, h: usize) -> Rgb32f {
    Rgb32f::from_fn(w, h, |x, y| {
        let mut sum = [0.0; 3];
        for dy in 0..4 {
            for dx in 0..4 {
                let (fx, fy) = (x as f32 + (dx as f32 + 0.5) / 4.0 - w as f32 / 2.0, y as f32 + (dy as f32 + 0.5) / 4.0 - h as f32 / 2.0);
                let v = match kind {
                    0 => {
                        let lum = 0.5 + 0.45 * (std::f32::consts::PI * 0.95 * (fx * fx + fy * fy) / (w as f32)).cos();
                        [lum; 3]
                    }
                    1 => {
                        let lum = if fx * fx + fy * fy < 16.0 { 0.5 } else { 0.5 + 0.45 * (48.0 * fy.atan2(fx)).cos() };
                        [lum; 3]
                    }
                    _ => {
                        let i = ((fx + 0.63 * fy + w as f32) * 0.055).floor() as i32;
                        [[0.8, 0.25, 0.12], [0.15, 0.7, 0.25], [0.12, 0.3, 0.85]][i.rem_euclid(3) as usize]
                    }
                };
                for c in 0..3 {
                    sum[c] += v[c] / 16.0;
                }
            }
        }
        sum
    })
}
pub fn chroma_error(a: &Rgb32f, b: &Rgb32f, border: usize) -> f64 {
    let (mut sum, mut n) = (0.0, 0);
    for y in border..a.height - border {
        for x in border..a.width - border {
            let (p, q) = (a.get(x, y), b.get(x, y));
            for c in [0, 2] {
                let d = (p[c] - p[1]) as f64 - (q[c] - q[1]) as f64;
                sum += d * d;
                n += 1;
            }
        }
    }
    (sum / n as f64).sqrt()
}
