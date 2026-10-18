//! A smart object's filter mask: how the filtered pixels and the unfiltered (placed) ones combine.
//!
//! Photoshop stores a smart object's layer pixels with its filters applied but not its filter
//! mask, and mixes the unfiltered pixels back in through the mask when it composites. Our smart
//! objects keep the mixed result as their pixels, so the engine's re-render and PSD import mix
//! ([`mix`]) and PSD export unmixes ([`unmix`]).

use photocraft_doc::LayerMask;
use photocraft_raster::Surface;

/// Mixes `filtered` over `base` through the filter mask (premultiplied, any channel layout).
pub fn mix(base: &Surface, filtered: &Surface, mask: &LayerMask) -> Surface {
    let fmt = base.format();
    let area = base.content_bounds().union(&filtered.content_bounds());
    let mut out = filtered.clone();
    if area.is_empty() || filtered.format() != fmt {
        return out;
    }
    let n = fmt.channels();
    let (b, f) = (base.read_region(area), filtered.read_region(area));
    let mut k = Vec::new();
    mask.values_into(area, &mut k);
    let mut o = vec![0.0f32; b.len()];
    for (i, ((bp, fp), op)) in b.chunks_exact(n).zip(f.chunks_exact(n)).zip(o.chunks_exact_mut(n)).enumerate() {
        let m = k.get(i).copied().unwrap_or(1.0).clamp(0.0, 1.0);
        if !fmt.alpha {
            for c in 0..n {
                op[c] = bp[c] + (fp[c] - bp[c]) * m;
            }
            continue;
        }
        let a = n - 1;
        let oa = bp[a] + (fp[a] - bp[a]) * m;
        op[a] = oa;
        for c in 0..a {
            let pm = bp[c] * bp[a] + (fp[c] * fp[a] - bp[c] * bp[a]) * m;
            // Integer surfaces clamp when stored. Float/HDR values must not be
            // clipped merely because the user blends through a smart-filter mask.
            op[c] = if oa > 0.0 { pm / oa } else { 0.0 };
        }
    }
    out.write_region(area, &o);
    out.prune();
    out
}

/// Recover filtered pixels from the stored mixed result; hidden pixels use the base.
pub fn unmix(base: &Surface, mixed: &Surface, mask: &LayerMask) -> Surface {
    let fmt = base.format();
    let area = base.content_bounds().union(&mixed.content_bounds());
    let mut out = mixed.clone();
    if area.is_empty() || mixed.format() != fmt {
        return out;
    }
    let n = fmt.channels();
    let (b, x) = (base.read_region(area), mixed.read_region(area));
    let mut coverage = Vec::new();
    mask.values_into(area, &mut coverage);
    let mut result = vec![0.; b.len()];
    for (i, ((bp, xp), op)) in b.chunks_exact(n).zip(x.chunks_exact(n)).zip(result.chunks_exact_mut(n)).enumerate() {
        let m = coverage.get(i).copied().unwrap_or(1.).clamp(0., 1.);
        if m < 1. / 1024. {
            if !fmt.alpha || bp[n - 1] > 0. {
                op.copy_from_slice(bp);
            }
            continue;
        }
        if !fmt.alpha {
            for c in 0..n {
                op[c] = bp[c] + (xp[c] - bp[c]) / m;
            }
            continue;
        }
        let a = n - 1;
        let alpha = (bp[a] + (xp[a] - bp[a]) / m).clamp(0., 1.);
        if alpha <= 0. {
            continue;
        }
        op[a] = alpha;
        for c in 0..a {
            op[c] = (bp[c] * bp[a] + (xp[c] * xp[a] - bp[c] * bp[a]) / m) / alpha;
        }
    }
    out.write_region(area, &result);
    out.prune();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_color::{ColorMode, PixelFormat, SampleType};

    #[test]
    fn mask_mix_and_unmix_preserve_visible_samples_and_empty_pixels() {
        let fmt = PixelFormat::new(ColorMode::Rgb, SampleType::F32, true);
        let mut base = Surface::new(fmt);
        let mut filtered = Surface::new(fmt);
        let mut mask = LayerMask::reveal_all();
        mask.surface = Surface::new(PixelFormat::new(ColorMode::Grayscale, SampleType::F32, false));
        for x in 0..4 {
            base.write_pixel(x, 0, &[0.2, 0.4, 0.6, 0.5]);
            filtered.write_pixel(x, 0, &[2., -0.5, 0.8, 0.75]);
            mask.surface.write_pixel(x, 0, &[x as f32 / 3.]);
        }
        let mixed = mix(&base, &filtered, &mask);
        assert_eq!(mixed.pixel(0, 0), base.pixel(0, 0));
        let restored = unmix(&base, &mixed, &mask);
        for x in 1..4 {
            for (a, b) in restored.pixel(x, 0).iter().zip(filtered.pixel(x, 0)) {
                assert!((a - b).abs() < 1e-5);
            }
        }
        base.write_pixel(0, 0, &[1., 1., 1., 0.]);
        let empty = unmix(&base, &mix(&base, &filtered, &mask), &mask);
        assert_eq!(empty.pixel(0, 0), vec![0.; 4]);
    }
}
