//! Auto tone and auto white balance (histogram / grey-world statistics on a proxy), and the white
//! balance selector.

use li_color::cct::{temp_tint_to_xy, xy_to_temp_tint};
use li_color::{REC2020, Xy, bradford, luminance_2020};
use li_raster::RgbFrame;
use lightcraft_develop::DevelopSettings;
use lightcraft_geom::Point;
use serde::Serialize;

use crate::SourceInfo;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct AutoTone {
    pub exposure: f64,
    pub contrast: f64,
    pub highlights: f64,
    pub shadows: f64,
    pub whites: f64,
    pub blacks: f64,
    pub vibrance: f64,
    pub saturation: f64,
}

fn percentile(sorted: &[f32], q: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() - 1) as f32 * q.clamp(0.0, 1.0)) as usize]
}

/// Compute auto tone values for `src` under the current white balance (ignores current tone values).
pub fn auto_tone(src: &RgbFrame, info: &SourceInfo, s: &DevelopSettings) -> AutoTone {
    let mut img = src.fit(512, 512, li_raster::resample::Filter::Box);
    let mut base = DevelopSettings { wb: s.wb, negative: s.negative, disabled_sections: s.disabled_sections.clone(), ..DevelopSettings::default() };
    base.light.exposure = 0.0;
    crate::local::scene_linear_pre(&mut img, info, &base);
    let mut ev: Vec<f32> = img.data.iter().map(|c| (luminance_2020(*c).max(1e-6) / 0.18).log2()).collect();
    ev.sort_by(|a, b| a.total_cmp(b));
    let median = percentile(&ev, 0.5);
    let exposure = (-median * 0.85 - 0.1).clamp(-4.0, 4.0);
    let (p01, p05, p95, p995) =
        (percentile(&ev, 0.01) + exposure, percentile(&ev, 0.05) + exposure, percentile(&ev, 0.95) + exposure, percentile(&ev, 0.995) + exposure);
    let highlights = if p995 > 2.2 { -((p995 - 2.2) * 38.0).min(90.0) } else { 0.0 };
    let shadows = if p05 < -4.0 { ((-4.0 - p05) * 22.0).min(70.0) } else { 0.0 };
    let spread = p95 - p05;
    let contrast = ((6.5 - spread) * 6.0).clamp(-20.0, 30.0);
    let whites = if p995 < 1.8 { ((1.8 - p995) * 25.0).min(40.0) } else { -((p995 - 3.5).max(0.0) * 10.0).min(30.0) };
    let blacks = if p01 > -5.0 { -((p01 + 5.0) * 10.0).min(35.0) } else { ((-7.0 - p01).max(0.0) * 8.0).min(20.0) };
    AutoTone {
        exposure: (exposure as f64 * 100.0).round() / 100.0,
        contrast: contrast.round() as f64,
        highlights: highlights.round() as f64,
        shadows: shadows.round() as f64,
        whites: whites.round() as f64,
        blacks: blacks.round() as f64,
        vibrance: 12.0,
        saturation: 3.0,
    }
}

/// An automatic black & white mix (slider values, red … magenta) for `src` under `s`'s white
/// balance and tone: each hue band's colourful pixels are pushed away from the image's mean
/// lightness — bands brighter than average get brighter, darker ones darker — so areas that
/// differ only in colour stay apart in grey. Bands with almost no colourful pixels stay at 0.
pub fn auto_bw_mix(src: &RgbFrame, info: &SourceInfo, s: &DevelopSettings) -> [f64; 8] {
    use li_color::perceptual::{lab_to_lch, oklab_from_2020};
    let mut img = src.fit(512, 512, li_raster::resample::Filter::Box);
    let base = DevelopSettings {
        wb: s.wb,
        light: s.light,
        negative: s.negative,
        disabled_sections: s.disabled_sections.clone(),
        ..DevelopSettings::default()
    };
    crate::local::scene_linear_pre(&mut img, info, &base);
    let gain = 2f32.powf(base.light.exposure as f32);
    let (mut mass, mut sum_l) = ([0f64; 8], [0f64; 8]);
    let (mut all_l, mut n) = (0f64, 0usize);
    for p in &img.data {
        let lch = lab_to_lch(oklab_from_2020(p.map(|v| (v * gain).max(0.0))));
        all_l += lch[0] as f64;
        n += 1;
        let k = (lch[1] / 0.2).min(1.0) as f64; // as in the B&W conversion
        if k < 0.05 {
            continue;
        }
        let w = crate::colorops::band_weights(lch[2]);
        for i in 0..8 {
            mass[i] += w[i] as f64 * k;
            sum_l[i] += w[i] as f64 * k * lch[0] as f64;
        }
    }
    if n == 0 {
        return [0.0; 8];
    }
    let mean = all_l / n as f64;
    let total: f64 = mass.iter().sum();
    std::array::from_fn(|i| {
        if total <= 0.0 || mass[i] / total < 0.01 {
            return 0.0;
        }
        let sep = sum_l[i] / mass[i] - mean;
        (sep * 400.0).clamp(-60.0, 60.0).round()
    })
}

/// Grey-world white balance weighted towards mid-tone, low-chroma pixels. Returns (temp, tint).
pub fn auto_wb(src: &RgbFrame, info: &SourceInfo) -> (f64, f64) {
    let img = src.fit(256, 256, li_raster::resample::Filter::Box);
    let (mut acc, mut wsum) = ([0.0f64; 3], 0.0f64);
    for c in &img.data {
        let y = luminance_2020(*c);
        if !(0.01..=2.0).contains(&y) {
            continue;
        }
        let mx = c[0].max(c[1]).max(c[2]);
        let mn = c[0].min(c[1]).min(c[2]);
        let chroma = (mx - mn) / (mx + 1e-6);
        let w = (1.0 - chroma).powi(2) as f64 * (1.0 - ((y.log2() + 2.5) / 4.0).abs().min(1.0)) as f64;
        for i in 0..3 {
            acc[i] += c[i] as f64 * w;
        }
        wsum += w;
    }
    if wsum <= 0.0 {
        return (info.as_shot_temp, info.as_shot_tint);
    }
    neutral_wb(acc.map(|v| v / wsum), info)
}

/// What a white-balance pick neutralises.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WbTarget {
    /// The photo's white balance ([`crate::local::wb_matrix_for`]).
    Global,
    /// A layer's white balance, applied on top of the photo's ([`crate::local::wb_change`]).
    Mask,
}

/// Long edge of the render a white-balance pick samples.
const PICK_EDGE: usize = 1024;

/// The white balance (K, tint) under which the patch around normalized image point `p` renders
/// neutral, for `target`. The patch is 5 × 5 pixels of a 1024 px render of the source before any
/// white balance, so lens corrections, Upright and the crop place it where it was clicked. `None`
/// outside the photo or on black.
pub fn wb_pick(src: &RgbFrame, info: &SourceInfo, s: &DevelopSettings, p: Point, target: WbTarget) -> Option<(f64, f64)> {
    let plan = crate::plan(src, info, s, &crate::RenderRequest::fit(PICK_EDGE, PICK_EDGE));
    let img = plan.frame.sample(src, plan.w, plan.h);
    let o = plan.frame.norm_to_out(plan.w, plan.h).apply(p);
    let (cx, cy) = (o.x.floor() as i64, o.y.floor() as i64);
    if cx < 0 || cy < 0 || cx >= plan.w as i64 || cy >= plan.h as i64 {
        return None;
    }
    let (mut acc, mut n) = ([0f64; 3], 0.0);
    for y in (cy - 2).max(0)..=(cy + 2).min(plan.h as i64 - 1) {
        for x in (cx - 2).max(0)..=(cx + 2).min(plan.w as i64 - 1) {
            let c = img.data[y as usize * plan.w + x as usize];
            for i in 0..3 {
                acc[i] += c[i] as f64;
            }
            n += 1.0;
        }
    }
    let avg = acc.map(|v| v / n);
    let y = li_color::luminance_2020_f64(avg);
    if y.is_nan() || y <= 1e-6 {
        return None;
    }
    Some(match target {
        WbTarget::Global => neutral_wb(avg, info),
        WbTarget::Mask => {
            // the layer changes the photo's white to its own (Rec. 2020 Bradford), on the photo's render
            let s = &*plan.settings;
            let g = crate::local::wb_matrix_for(info, s).map_or(avg, |m| std::array::from_fn(|i| (0..3).map(|j| m[i][j] as f64 * avg[j]).sum()));
            let (t, tint) = crate::local::effective_wb(info, s);
            let seen = bradford(REC2020.white, temp_tint_to_xy(t, tint)).apply(REC2020.to_xyz().apply(g));
            clamped(xy_to_temp_tint(Xy::from_xyz(seen)))
        }
    })
}

/// The photo white balance that renders source colour `avg` (linear Rec. 2020 at the as-shot white)
/// neutral. A raw with a camera model is solved in camera space, as [`crate::local::wb_matrix_for`]
/// re-derives the camera matrix at the chosen white (the DNG SDK's neutral-to-xy iteration); other
/// sources adapt from the as-shot white in Rec. 2020, as their white balance is applied.
fn neutral_wb(avg: [f64; 3], info: &SourceInfo) -> (f64, f64) {
    if let Some(camera) = info.camera_color {
        let neutral = camera.undo.apply(avg);
        if neutral.iter().all(|v| v.is_finite() && *v > 0.0) {
            return clamped(xy_to_temp_tint(li_color::camera::neutral_to_xy(&camera.color, neutral)));
        }
    }
    let xyz = REC2020.to_xyz().apply(avg);
    let shot = temp_tint_to_xy(info.as_shot_temp, info.as_shot_tint);
    let seen = bradford(REC2020.white, shot).apply(xyz);
    clamped(xy_to_temp_tint(Xy::from_xyz(seen)))
}

/// Temperature and tint in the sliders' ranges and steps.
fn clamped((t, tint): (f64, f64)) -> (f64, f64) {
    (t.clamp(2000.0, 50000.0).round(), tint.clamp(-150.0, 150.0).round())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_image_gets_positive_exposure() {
        let img = RgbFrame::from_fn(64, 64, |x, _| [0.01 + x as f32 * 0.0003; 3]);
        let a = auto_tone(&img, &SourceInfo::default(), &DevelopSettings::default());
        assert!(a.exposure > 1.5, "{a:?}");
        let bright = RgbFrame::from_fn(64, 64, |x, _| [0.8 + x as f32 * 0.01; 3]);
        let b = auto_tone(&bright, &SourceInfo::default(), &DevelopSettings::default());
        assert!(b.exposure < -1.0, "{b:?}");
    }

    #[test]
    fn neutral_image_keeps_as_shot_wb() {
        let img = RgbFrame::from_fn(32, 32, |x, y| [0.05 + (x + y) as f32 * 0.004; 3]);
        let (t, tint) = auto_wb(&img, &SourceInfo::default());
        assert!((t - 6500.0).abs() < 150.0, "{t}");
        assert!(tint.abs() < 6.0, "{tint}");
    }

    #[test]
    fn blue_cast_is_corrected_by_higher_temp() {
        // A bluish cast should be neutralised by telling the pipeline the light was bluer (higher K).
        let img = RgbFrame::from_fn(32, 32, |_, _| [0.16, 0.18, 0.24]);
        let (t, _) = auto_wb(&img, &SourceInfo::default());
        assert!(t > 7000.0, "{t}");
        let mut s = DevelopSettings::default();
        s.wb.mode = lightcraft_develop::WbMode::Custom;
        s.wb.temp = t;
        let (t2, tint2) = auto_wb(&img, &SourceInfo::default());
        let _ = (t2, tint2);
        let mut out = img.clone();
        let (tt, ti) = auto_wb(&img, &SourceInfo::default());
        s.wb.temp = tt;
        s.wb.tint = ti;
        crate::local::scene_linear_pre(&mut out, &SourceInfo::default(), &s);
        let c = out.get(0, 0);
        assert!((c[0] - c[2]).abs() < 0.02, "{c:?}");
    }
}

#[cfg(test)]
mod pick_tests {
    use super::*;
    use li_color::Mat3;
    use li_color::camera::{CameraColor, CameraWhite, camera_neutral, to_working};
    use lightcraft_develop::{WbMode, WhiteBalance};

    /// Sony ILCE-1 ColorMatrix1/2 (StdA, D65), as the decoder fills them.
    fn ilce1() -> CameraColor {
        CameraColor {
            illuminant: [17, 21],
            color_matrix: [
                Some(Mat3([[0.8933, -0.4409, 0.02], [-0.4637, 1.2364, 0.2554], [-0.0304, 0.0803, 0.7616]])),
                Some(Mat3([[0.8161, -0.2947, -0.0739], [-0.4811, 1.2668, 0.2389], [-0.0437, 0.1229, 0.6524]])),
            ],
            ..Default::default()
        }
    }

    /// A raw decoded at as-shot `shot` (K, tint) and a grey card lit by `light`, as the decoder
    /// renders it (camera RGB through the as-shot matrix and multipliers).
    fn raw_scene(color: CameraColor, shot: (f64, f64), light: (f64, f64)) -> (SourceInfo, [f32; 3]) {
        let total = to_working(&color, temp_tint_to_xy(shot.0, shot.1));
        let camera = CameraWhite { color, undo: total.inverse().expect("invertible") };
        let info = SourceInfo { raw: true, as_shot_temp: shot.0, as_shot_tint: shot.1, camera_color: Some(camera), ..SourceInfo::default() };
        let card = total.apply(camera_neutral(&color, temp_tint_to_xy(light.0, light.1)));
        let k = 0.18 / li_color::luminance_2020_f64(card);
        (info, card.map(|v| (v * k) as f32))
    }

    fn assert_neutral(c: [f32; 3], what: &str) {
        let (r, b) = (c[0] / c[1], c[2] / c[1]);
        assert!((r - 1.0).abs() < 0.005 && (b - 1.0).abs() < 0.005, "{what}: R/G {r}, B/G {b}");
    }

    fn custom(wb: (f64, f64)) -> DevelopSettings {
        DevelopSettings { wb: WhiteBalance { mode: WbMode::Custom, temp: wb.0, tint: wb.1 }, ..DevelopSettings::default() }
    }

    #[test]
    fn raw_pick_renders_the_clicked_grey_neutral() {
        for (shot, light) in [((5500.0, 0.0), (2850.0, 0.0)), ((3000.0, 0.0), (6500.0, 15.0)), ((5500.0, 0.0), (4000.0, 15.0))] {
            let (info, card) = raw_scene(ilce1(), shot, light);
            let src = RgbFrame::filled(64, 48, card);
            let wb = wb_pick(&src, &info, &DevelopSettings::default(), Point::new(0.5, 0.5), WbTarget::Global).expect("a grey card");
            let mut img = RgbFrame::filled(2, 2, card);
            crate::local::white_balance(&mut img, &info, &custom(wb));
            assert_neutral(img.get(0, 0), &format!("shot {shot:?}, light {light:?} -> {wb:?}"));
            // Auto WB solves its grey-world average the same way
            assert_eq!(auto_wb(&src, &info), wb);
        }
    }

    #[test]
    fn raw_pick_of_an_as_shot_neutral_keeps_as_shot() {
        let (info, card) = raw_scene(ilce1(), (5200.0, 4.0), (5200.0, 4.0));
        let src = RgbFrame::filled(32, 32, card);
        let wb = wb_pick(&src, &info, &DevelopSettings::default(), Point::new(0.3, 0.6), WbTarget::Global);
        assert_eq!(wb, Some((5200.0, 4.0)));
        assert!(crate::local::wb_matrix_for(&info, &custom((5200.0, 4.0))).is_none());
    }

    #[test]
    fn mask_pick_neutralises_on_top_of_the_photo_white_balance() {
        let (info, card) = raw_scene(ilce1(), (5500.0, 0.0), (2850.0, 0.0));
        let src = RgbFrame::filled(64, 48, card);
        for global in [DevelopSettings::default(), custom((4000.0, 5.0))] {
            let wb = wb_pick(&src, &info, &global, Point::new(0.5, 0.5), WbTarget::Mask).expect("a grey card");
            let mut img = RgbFrame::filled(2, 2, card);
            crate::local::white_balance(&mut img, &info, &global);
            let m = crate::local::wb_change(crate::local::effective_wb(&info, &global), wb).expect("the layer changes the white");
            let c = img.get(0, 0);
            let c = std::array::from_fn(|i| (0..3).map(|j| m[i][j] * c[j]).sum());
            assert_neutral(c, &format!("global {:?} -> layer {wb:?}", global.wb));
        }
    }

    #[test]
    fn rendered_pick_keeps_the_rec2020_model() {
        let img = RgbFrame::filled(32, 32, [0.16, 0.18, 0.24]);
        let info = SourceInfo::default();
        let wb = wb_pick(&img, &info, &DevelopSettings::default(), Point::new(0.5, 0.5), WbTarget::Global).expect("a patch");
        assert_eq!(wb, auto_wb(&img, &info));
        let mut out = RgbFrame::filled(2, 2, [0.16, 0.18, 0.24]);
        crate::local::white_balance(&mut out, &info, &custom(wb));
        let c = out.get(0, 0);
        assert!((c[0] - c[2]).abs() < 0.02 && (c[1] - c[2]).abs() < 0.02, "{c:?}");
    }

    #[test]
    fn pick_follows_the_corrected_image() {
        // grey left of the middle, red right of it; the perspective offset moves the image sideways,
        // so the clicked point and the same coordinates in the uncorrected source show different colours
        let (w, h) = (256, 192);
        let grey = [0.18f32; 3];
        let src = RgbFrame::from_fn(w, h, |x, _| if x < w / 2 { grey } else { [0.3, 0.02, 0.02] });
        let info = SourceInfo::default();
        let mut s = DevelopSettings::default();
        s.geometry.offset_x = 60.0;
        let plan = crate::plan(&src, &info, &s, &crate::RenderRequest::fit(PICK_EDGE, PICK_EDGE));
        let render = plan.frame.sample(&src, plan.w, plan.h);
        let to_out = plan.frame.norm_to_out(plan.w, plan.h);
        let is_grey = |c: [f32; 3]| (c[0] - c[2]).abs() < 1e-3;
        let p = (5..95)
            .map(|i| Point::new(i as f64 / 100.0, 0.5))
            .find(|p| {
                let o = to_out.apply(*p);
                let (ox, oy) = (o.x as isize, o.y as isize);
                let rendered_grey = (-3..=3).all(|d| is_grey(render.get_clamped(ox + d, oy)));
                let source_red = (-3..=3).all(|d| !is_grey(src.get_clamped((p.x * w as f64) as isize + d, (p.y * h as f64) as isize)));
                rendered_grey && source_red
            })
            .expect("the offset separates rendered and source positions");
        assert_eq!(wb_pick(&src, &info, &s, p, WbTarget::Global), Some((6500.0, 0.0)), "{p:?}");
        assert_eq!(wb_pick(&src, &info, &s, Point::new(1.5, 0.5), WbTarget::Global), None);
    }

    #[test]
    fn pick_on_black_is_refused() {
        let src = RgbFrame::filled(16, 16, [0.0; 3]);
        assert_eq!(wb_pick(&src, &SourceInfo::default(), &DevelopSettings::default(), Point::new(0.5, 0.5), WbTarget::Global), None);
    }
}

#[cfg(test)]
mod bw_tests {
    use super::*;

    #[test]
    fn auto_bw_mix_pushes_light_and_dark_hues_apart() {
        // left: a light yellow, right: a dark blue (scene-linear Rec. 2020)
        let src = RgbFrame::from_fn(64, 32, |x, _| if x < 32 { [0.55, 0.5, 0.05] } else { [0.01, 0.02, 0.12] });
        let m = auto_bw_mix(&src, &SourceInfo::default(), &DevelopSettings::default());
        let (yellow, blue) = (m[2], m[5]);
        assert!(yellow > 0.0 && blue < 0.0, "{m:?}");
        // an image without colour leaves the mix alone
        let grey = RgbFrame::from_fn(16, 16, |x, _| [x as f32 / 16.0; 3]);
        assert_eq!(auto_bw_mix(&grey, &SourceInfo::default(), &DevelopSettings::default()), [0.0; 8]);
    }
}
