use super::{Program, lut_rows, sample_stops4, to_row};
use photocraft_compose::adjust::{self, Transfer};
use photocraft_doc::Adjustment;
use photocraft_doc::adjust::ToneSpace;

/// A tone transfer as the shader's `t_decode`/`t_encode` exponent (0: the sRGB curve).
fn transfer_exponent(t: Transfer) -> f32 {
    match t {
        Transfer::Srgb => 0.0,
        Transfer::Gamma(g) => g,
    }
}

/// Adjustment → (kernel kind, parameters, LUT rows). Kinds are the `switch` in `adjust()`.
pub fn adjustment_program(adj: &Adjustment, transfer: Transfer, depth: photocraft_color::SampleType) -> Program {
    let mut p = [[0.0f32; 4]; 4];
    match adj {
        Adjustment::Invert => (1, p, None),
        Adjustment::Threshold { level } => {
            let t = (level * 255.0).round();
            p[0] = match depth {
                photocraft_color::SampleType::U8 => [t, 1.0, 0.0, 0.0],
                photocraft_color::SampleType::U16 => [t, 2.0, (t * 32768.0 / 255.0).round(), 0.0],
                photocraft_color::SampleType::F32 => [t, 0.0, 0.0, 0.0],
            };
            (2, p, None)
        }
        Adjustment::Posterize { levels } => {
            p[0][0] = (*levels).clamp(2, 255) as f32;
            p[0][1] = if depth == photocraft_color::SampleType::U16 { 1.0 } else { 0.0 };
            (3, p, None)
        }
        Adjustment::BrightnessContrast { brightness, contrast, legacy: true } => {
            let c = contrast.clamp(-100.0, 99.0);
            let k = if c >= 0.0 { 1.0 / (1.0 - c / 100.0) } else { 1.0 + c / 100.0 };
            p[0] = [brightness / 255.0, k, 0.0, 0.0];
            (4, p, None)
        }
        Adjustment::BrightnessContrast { brightness, contrast, .. } => {
            // Modern B/C: the CPU's lookup table (compose::adjust::modern_lut) on every channel,
            // through the Levels / Curves kernel.
            let row = to_row(&adjust::modern_lut(*brightness, *contrast));
            (7, p, Some(vec![row; 3]))
        }
        Adjustment::Exposure { exposure, offset, gamma } => {
            p[0] = [2f32.powf(*exposure), *offset, gamma.max(0.01), transfer_exponent(transfer.for_exposure())];
            (6, p, None)
        }
        // The CPU's channel/master composition and depth rounding stay in the tone rows.
        Adjustment::Levels { space, .. } | Adjustment::Curves { space, .. } => {
            let mut rows: Vec<_> = adjust::tone_luts_depth(adj, Some(depth)).iter().map(|t| to_row(t)).collect();
            let kind = match space {
                ToneSpace::Rgb => 7,
                ToneSpace::Lab => 17,
                ToneSpace::Cmyk => 18,
            };
            if *space == ToneSpace::Cmyk {
                photocraft_color::convert::with_cmyk_grids(|from, to| {
                    if from.clut.inputs != 3
                        || from.clut.outputs != 4
                        || to.clut.inputs != 4
                        || to.clut.outputs != 3
                        || from.clut.grid.iter().any(|n| *n != from.clut.grid[0])
                        || to.clut.grid.iter().any(|n| *n != to.clut.grid[0])
                    {
                        return;
                    }
                    let mut append = |data: &[f32]| {
                        let base = rows.len() * 4096;
                        rows.extend(data.chunks(4096).map(to_row));
                        base as f32
                    };
                    p[0] = [from.clut.grid[0] as f32, to.clut.grid[0] as f32, append(&from.clut.data), append(&to.clut.data)];
                    for (i, grid) in [from, to].into_iter().enumerate() {
                        if let Some(post) = grid.post {
                            p[1][i] = (rows.len() * 4096) as f32;
                            rows.extend(post.iter().map(|t| to_row(t)));
                        }
                    }
                });
            }
            (kind, p, Some(rows))
        }
        Adjustment::HueSaturation { hue, saturation, lightness, colorize, ranges } => {
            p[0] = [*hue, saturation / 100.0, lightness / 100.0, if *colorize { 1.0 } else { 0.0 }];
            p[1][1] = if depth == photocraft_color::SampleType::F32 { 1.0 } else { 0.0 };
            if !*colorize && ranges.iter().any(|r| !r.is_neutral()) {
                // Range edits: hue-indexed rows (shift, saturation, lightness), as on the CPU.
                p[1][0] = 1.0;
                (8, p, Some(adjust::hue_range_tables(ranges).iter().map(|t| to_row(t)).collect()))
            } else {
                (8, p, None)
            }
        }
        Adjustment::Vibrance { vibrance, saturation } => {
            p[0] = [vibrance / 100.0, saturation / 100.0, 0.0, 0.0];
            (9, p, None)
        }
        Adjustment::ChannelMixer { matrix, monochrome } => {
            p[0] = matrix[0];
            p[1] = matrix[1];
            p[2] = matrix[2];
            p[3][0] = if *monochrome { 1.0 } else { 0.0 };
            (10, p, None)
        }
        Adjustment::PhotoFilter { color, density, preserve_luminosity } => {
            // compose::adjust: the linear-light matrix, then SetLum on the encoded values (8/16-bit).
            let linear_doc = depth == photocraft_color::SampleType::F32;
            let m = adjust::photo_filter_matrix(*color, *density, *preserve_luminosity && linear_doc);
            for (row, m) in p.iter_mut().zip(m) {
                *row = [m[0], m[1], m[2], 0.0];
            }
            p[3] = [if *preserve_luminosity && !linear_doc { 1.0 } else { 0.0 }, transfer_exponent(transfer), 0.0, 0.0];
            (11, p, None)
        }
        Adjustment::BlackWhite { weights, tint } => {
            p[0] = [weights[0], weights[1], weights[2], weights[3]];
            p[1] = [weights[4], weights[5], if tint.is_some() { 1.0 } else { 0.0 }, 0.0];
            if let Some(t) = tint {
                p[2] = [t[0], t[1], t[2], 0.0];
            }
            (12, p, None)
        }
        Adjustment::GradientMap { stops, reverse, dither } => {
            p[0][0] = if *reverse { 1.0 } else { 0.0 };
            p[0][1] = if *dither { 1.0 } else { 0.0 };
            let s4: Vec<(f32, [f32; 4])> = stops.iter().map(|(t, c)| (*t, [c[0], c[1], c[2], 1.0])).collect();
            let rows = lut_rows(|r, t| if s4.is_empty() { t } else { sample_stops4(&s4, t)[r] }, 3);
            (13, p, Some(rows))
        }
        Adjustment::ColorBalance { shadows, midtones, highlights, preserve_luminosity } => {
            // The CPU's per-channel curves (compose::adjust::color_balance_channel) as tone rows.
            let tones = [*shadows, *midtones, *highlights];
            let rows = lut_rows(|ch, value| adjust::color_balance_channel(tones, *preserve_luminosity, ch, value), 3);
            (7, p, Some(rows))
        }
        Adjustment::SelectiveColor { relative, adjustments } => {
            // 9 ranges × CMYK don't fit the 16 params: they go in LUT row 0 (read texel-exact).
            p[0][0] = if *relative { 1.0 } else { 0.0 };
            let mut row = [0.0f32; 4096];
            for (i, v) in adjustments.iter().flatten().enumerate() {
                row[i] = *v;
            }
            (15, p, Some(vec![row]))
        }
        // Upstream 5049a1d0: a stored size is untrusted; `lut3d_len` is the checked n³ × 3.
        Adjustment::ColorLookup { lut: Some(table), size, tetrahedral, dither, domain, .. }
            if adjust::lut3d_len(*size).is_some_and(|len| table.len() >= len) =>
        {
            // The flattened table (n³ RGB triplets) spans as many 4096-wide rows as it needs.
            let n = *size as usize;
            let rows = adjust::lut3d_len(*size)
                .and_then(|len| table.get(..len))
                .unwrap_or_default()
                .chunks(4096)
                .map(|c| {
                    let mut row = [0.0f32; 4096];
                    row[..c.len()].copy_from_slice(c);
                    row
                })
                .collect();
            p[0] = [n as f32, if *tetrahedral { 1.0 } else { 0.0 }, if *dither { 1.0 } else { 0.0 }, 0.0];
            let (min, max) = domain.filter(|d| d.valid()).map_or(([0.0; 3], [1.0; 3]), |d| (d.min, d.max));
            p[1] = [min[0], min[1], min[2], 0.0];
            p[2] = std::array::from_fn(|i| if i < 3 { max[i] - min[i] } else { 0.0 });
            (16, p, Some(rows))
        }
        // Identity on the CPU too (not evaluated there).
        Adjustment::ColorLookup { .. } | Adjustment::Unsupported { .. } => (0, p, None),
    }
}
