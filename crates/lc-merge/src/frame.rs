//! Merge inputs: scene-linear frames decoded from raw files (camera RGB, before white balance) or
//! standard images (linear light in the file's primaries), with their exposure from EXIF.

use li_color::Mat3;
use li_raster::RgbFrame;
use li_raster::resample::Filter;
use lightcraft_geom::Orientation;
use lightcraft_meta::Metadata;
use lightcraft_raw::{ColorData, Opcode, RawImage, Rect};

use crate::{MergeError, Result};

/// What the RGB values of a frame mean.
#[derive(Clone, Debug, PartialEq)]
pub enum FrameColor {
    /// Camera RGB of a raw file (not white balanced): the DNG colour tags of the source.
    Camera(Box<ColorData>),
    /// Linear RGB with these primaries: linear RGB → XYZ (D50-adapted, ICC PCS).
    Linear { to_xyz_d50: Mat3 },
}

/// One input photo.
#[derive(Clone, Debug)]
pub struct Frame {
    /// Scene-linear pixels; 1.0 = the sensor/file white level.
    pub image: RgbFrame,
    /// Values at or above this (any channel) are saturated.
    pub clip: f32,
    /// Capture brightness from EXIF, in EV: log2(t · ISO / N²) (higher = brighter frame); `None`
    /// when the file doesn't say.
    pub exposure: Option<f64>,
    pub raw: bool,
    pub color: FrameColor,
    /// How `image` must be rotated/flipped for display (`Normal` once applied).
    pub orientation: Orientation,
    pub metadata: Metadata,
    /// Raw files: the source's `BaselineExposure` (EV), carried into the output.
    pub baseline_exposure: f64,
}

impl Frame {
    /// A frame from linear pixels in the develop working space (linear Rec.2020), e.g. a rendered
    /// procedural scene. `exposure` as in [`Frame::exposure`].
    pub fn from_linear_rec2020(image: RgbFrame, exposure: Option<f64>) -> Frame {
        let to_xyz_d50 = li_color::bradford(li_color::D65, li_color::D50).mul(&li_color::REC2020.to_xyz());
        let clip = detect_clip(&image);
        Frame {
            image,
            clip,
            exposure,
            raw: false,
            color: FrameColor::Linear { to_xyz_d50 },
            orientation: Orientation::Normal,
            metadata: Metadata::default(),
            baseline_exposure: 0.0,
        }
    }

    pub fn width(&self) -> usize {
        self.image.width
    }
    pub fn height(&self) -> usize {
        self.image.height
    }

    /// Apply the display orientation to the pixels.
    pub fn orient(&mut self) {
        if self.orientation != Orientation::Normal {
            self.image = self.image.oriented(self.orientation);
            self.orientation = Orientation::Normal;
        }
    }
}

/// EV of a capture from its EXIF: log2(t · ISO / N²). ISO defaults to 100 and N to 1 when absent
/// (only ratios between frames matter); `None` without an exposure time.
pub fn exif_exposure(m: &Metadata) -> Option<f64> {
    let t = m.exposure_time.filter(|t| *t > 0.0 && t.is_finite())?;
    let iso = m.iso.filter(|i| *i > 0).unwrap_or(100) as f64;
    let n = m.f_number.filter(|n| *n > 0.0 && n.is_finite()).unwrap_or(1.0);
    Some((t * iso / (n * n)).log2())
}

/// Clip levels sit this far below the value saturated samples reach, so demosaic and fusion
/// round-off on a clipped area still counts as clipped.
pub(crate) const CLIP_MARGIN: f32 = 0.995;

/// The saturation level of a frame: 1.0, or the (lower) level at which values pile up when the
/// file's white level is optimistic. Returned slightly below the level.
pub fn detect_clip(img: &RgbFrame) -> f32 {
    let maxv = img.data.iter().map(|p| p[0].max(p[1]).max(p[2])).fold(0.0f32, f32::max);
    if maxv.is_nan() || maxv <= 0.0 {
        return 1.0;
    }
    let level = maxv.min(1.0);
    // a pile-up: the top 0.5 % of the range is ≥ 4× denser than the 2 % just below it
    let (mut near, mut below) = (0usize, 0usize);
    for p in &img.data {
        let m = p[0].max(p[1]).max(p[2]);
        if m >= level * CLIP_MARGIN {
            near += 1;
        } else if m >= level * 0.97 && m < level * 0.99 {
            below += 1;
        }
    }
    let piled = near as f64 > img.data.len() as f64 * 1e-4 && near >= 4 && near as f64 / 0.005 > 4.0 * below as f64 / 0.02;
    if maxv >= 1.0 || piled { level * CLIP_MARGIN } else { 1.0 }
}

/// Apply a correction that scales or moves samples unevenly (lens gains, distortion) to camera RGB
/// whose samples at or above `clip` are saturated, and return the clip level afterwards. Such
/// corrections spread clipped samples over a range of values and can lift measured corners above
/// them, so no single level would separate clipped from measured values; the clipped samples are
/// therefore moved to one level, clear above every measured value. `correct` runs on the image
/// and, only when something is clipped, on a full-size clip mask.
pub fn correct_with_clip(image: &mut RgbFrame, clip: f32, correct: impl Fn(&mut RgbFrame)) -> f32 {
    let clipped = image.data.iter().flatten().any(|v| *v >= clip).then(|| image.map(|p| p.map(|v| if v >= clip { 1.0f32 } else { 0.0 })));
    correct(image);
    let peak = image.data.iter().flatten().copied().filter(|v| v.is_finite()).fold(0.0f32, f32::max);
    let level = clip.max(peak / crate::output::UNCLIPPED_PEAK);
    if let Some(mut clipped) = clipped {
        // Gains keep the moved mask well above one half.
        correct(&mut clipped);
        for (p, m) in image.data.iter_mut().zip(&clipped.data) {
            for c in 0..3 {
                if m[c] >= 0.5 {
                    p[c] = level;
                }
            }
        }
    }
    level
}

/// [`correct_with_clip`] for `OpcodeList3` (DJI vignette and gain maps, lens warps).
pub(crate) fn apply_lens_opcodes(list3: &[Opcode], image: &mut RgbFrame, clip: f32) -> f32 {
    if list3.is_empty() {
        return clip;
    }
    correct_with_clip(image, clip, |img| lightcraft_raw::opcodes::apply_list3(list3, img))
}

/// Develop `raw` like [`RawImage::develop`] (normalize, demosaic the mosaic balanced by its
/// as-shot white, `OpcodeList3`, default crop), returning camera RGB and its clip level
/// ([`Frame::clip`]).
fn develop_frame(raw: &RawImage, method: lightcraft_raw::Method) -> Result<(RgbFrame, f32)> {
    let normalized = raw.normalized().map_err(|e| MergeError::Decode(e.to_string()))?;
    let mut image = lightcraft_raw::demosaic_balanced(normalized, method, &Default::default(), raw.as_shot_wb());
    let sensor_clip = detect_clip(&image);
    let clip = apply_lens_opcodes(&raw.opcodes.list3, &mut image, sensor_clip);
    let crop = raw.crop.clipped(image.width, image.height);
    if crop.width > 0 && crop.height > 0 && crop != Rect::new(0, 0, image.width, image.height) {
        image = image.into_crop(crop.x, crop.y, crop.width, crop.height);
    }
    Ok((image, clip))
}

/// The colour model of `raw`'s developed pixels, which are black/white normalized: the file's
/// colour tags in normalized sensor coordinates. Vendor raws keep their as-shot white balance as
/// multipliers rather than DNG tags, so it becomes the equivalent `AsShotNeutral`; without it a
/// merged DNG would open at D65.
pub(crate) fn camera_color(raw: &RawImage) -> FrameColor {
    let mut color = lightcraft_raw::color::normalized_color(raw);
    if color.as_shot_neutral.is_none()
        && color.as_shot_white_xy.is_none()
        && let Some(wb) = raw.wb_multipliers.filter(|w| w.iter().all(|v| v.is_finite() && *v > 0.0))
    {
        let ratios = lightcraft_raw::color::normalization_ratios(raw);
        color.as_shot_neutral = Some(std::array::from_fn(|c| ratios[c] / f64::from(wb[c])));
    }
    FrameColor::Camera(Box::new(color))
}

/// The orientation tag most of a capture's raw files carry; ties go to `raws[preferred]`. All the
/// files hold one sensor geometry whatever the tag says, and cameras pointed straight up or down
/// can change the tag mid-burst.
pub(crate) fn common_orientation(raws: &[RawImage], preferred: usize) -> Orientation {
    let votes = |o: Orientation| raws.iter().filter(|r| r.orientation == o).count();
    let preferred = raws.get(preferred).map_or(Orientation::Normal, |r| r.orientation);
    raws.iter().map(|r| r.orientation).fold(preferred, |best, o| if votes(o) > votes(best) { o } else { best })
}

/// Metadata for pixels whose DNG lens operations have already been evaluated.
/// Keep the camera/lens identification, but persist the fact that automatic lens
/// corrections must not be inferred and applied again when the output is opened.
pub(crate) fn developed_metadata(raw: &lightcraft_raw::RawImage) -> Metadata {
    let mut metadata = raw.metadata.clone();
    metadata.lens_corrections_baked |= raw.opcodes.list3.iter().any(|op| {
        matches!(
            op,
            lightcraft_raw::Opcode::WarpRectilinear { .. }
                | lightcraft_raw::Opcode::WarpFisheye { .. }
                | lightcraft_raw::Opcode::FixVignetteRadial { .. }
        )
    });
    metadata.chromatic_aberration_baked |= raw.opcodes.list3.iter().any(|op| match op {
        lightcraft_raw::Opcode::WarpRectilinear { planes, .. } => planes.windows(2).any(|p| p[0] != p[1]),
        lightcraft_raw::Opcode::WarpFisheye { planes, .. } => planes.windows(2).any(|p| p[0] != p[1]),
        _ => false,
    });
    metadata
}

/// Decode a photo (raw or standard format) into a frame no larger than `max_edge` (when given).
/// `orient` applies the EXIF orientation to the pixels.
pub fn load_frame(bytes: &[u8], max_edge: Option<usize>, orient: bool) -> Result<Frame> {
    let metadata = lightcraft_meta::extract(bytes);
    let exposure = exif_exposure(&metadata);
    if lightcraft_raw::probe(bytes).is_some() {
        let raw = lightcraft_raw::decode(bytes).map_err(|e| MergeError::Decode(e.to_string()))?;
        // Each panorama input must have its own ordered camera-RGB corrections
        // applied before registration. The final mosaic has no source lens frame.
        let small = max_edge.is_some_and(|m| m <= 1200);
        let method = if small { lightcraft_raw::Method::Bilinear } else { lightcraft_raw::Method::Auto };
        let (mut image, clip) = develop_frame(&raw, method)?;
        for p in image.data.iter_mut() {
            *p = p.map(|v| v.max(0.0));
        }
        if let Some(m) = max_edge
            && image.width.max(image.height) > m
        {
            image = image.fit(m, m, Filter::Box);
        }
        let mut f = Frame {
            image,
            clip,
            exposure,
            raw: true,
            color: camera_color(&raw),
            orientation: raw.orientation,
            metadata: if raw.metadata == Metadata::default() { metadata } else { developed_metadata(&raw) },
            baseline_exposure: raw.color.baseline_exposure,
        };
        let correction_metadata = developed_metadata(&raw);
        f.metadata.lens_corrections_baked |= correction_metadata.lens_corrections_baked;
        f.metadata.chromatic_aberration_baked |= correction_metadata.chromatic_aberration_baked;
        if f.exposure.is_none() {
            f.exposure = exif_exposure(&f.metadata);
        }
        if orient {
            f.orient();
        }
        return Ok(f);
    }
    let opts = match max_edge {
        Some(m) => lightcraft_codecs::DecodeOptions::fit(m as u32, m as u32),
        None => lightcraft_codecs::DecodeOptions::default(),
    };
    let d = lightcraft_codecs::decode(bytes, opts).map_err(|e| MergeError::Decode(e.to_string()))?;
    let mut image = d.image;
    if let Some(m) = max_edge
        && image.width.max(image.height) > m
    {
        image = image.fit(m, m, Filter::Box);
    }
    let clip = detect_clip(&image);
    let mut f = Frame {
        image,
        clip,
        exposure,
        raw: false,
        color: FrameColor::Linear { to_xyz_d50: d.space.to_xyz_d50 },
        orientation: Orientation::from_exif(d.orientation),
        metadata,
        baseline_exposure: 0.0,
    };
    if orient {
        f.orient();
    }
    Ok(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposure_from_exif() {
        let m = Metadata { exposure_time: Some(1.0 / 125.0), f_number: Some(8.0), iso: Some(100), ..Default::default() };
        let e = exif_exposure(&m).unwrap();
        let m2 = Metadata { exposure_time: Some(4.0 / 125.0), ..m.clone() };
        assert!((exif_exposure(&m2).unwrap() - e - 2.0).abs() < 1e-9);
        assert!(exif_exposure(&Metadata::default()).is_none());
    }

    /// Full-size panorama frames are developed as Develop develops the raw (Auto on the mosaic
    /// balanced by the as-shot white), so slanted neutral edges such as mullions and siding get a
    /// single raw's demosaic, not the false colour of an unbalanced one.
    #[test]
    fn full_size_raw_frames_demosaic_like_develop() {
        // (RW2 rows pack ten samples to a block)
        let (w, h) = (100, 64);
        // neutral bars slanted 18 degrees, as camera RGB under the RW2's [2, 1, 1.5] multipliers
        let scene: Vec<[f32; 3]> =
            (0..w * h).map(|i| if ((i % w) as f32 + (i / w) as f32 / 3.0) % 6.0 < 3.0 { [0.3, 0.6, 0.4] } else { [0.04, 0.08, 0.0533] }).collect();
        let bytes = lightcraft_raw::testgen::rw2(w, h, &lightcraft_raw::testgen::mosaic(&scene, w, [0, 1, 1, 2], 128, 4095), 12);
        let raw = lightcraft_raw::decode(&bytes).unwrap();
        assert_ne!(raw.as_shot_wb(), [1.0; 3], "a white balance to demosaic under");
        let expected = raw.develop(lightcraft_raw::Method::Auto).unwrap();
        let frame = load_frame(&bytes, None, false).unwrap();
        assert_eq!((frame.image.width, frame.image.height), (expected.width, expected.height));
        let error = frame.image.data.iter().flatten().zip(expected.data.iter().flatten()).map(|(a, b)| (a - b.max(0.0)).abs()).fold(0f32, f32::max);
        assert!(error < 1e-6, "the frame differs from Develop's demosaic by {error}");
    }

    #[test]
    fn vendor_raw_frames_open_at_the_cameras_as_shot_white() {
        let (w, h) = (40, 30);
        let mosaic = lightcraft_raw::testgen::mosaic(&lightcraft_raw::testgen::scene(w, h), w, [0, 1, 1, 2], 128, 4095);
        let bytes = lightcraft_raw::testgen::rw2(w, h, &mosaic, 12);
        let source = lightcraft_raw::decode(&bytes).unwrap();
        assert_eq!(source.wb_multipliers, Some([2.0, 1.0, 1.5]));
        assert!(source.color.as_shot_neutral.is_none() && source.color.as_shot_white_xy.is_none());
        let frame = load_frame(&bytes, None, true).unwrap();
        let FrameColor::Camera(color) = &frame.color else { panic!("raw frames carry camera colour") };
        let neutral = color.as_shot_neutral.unwrap();
        for (a, b) in neutral.into_iter().zip([0.5, 1.0, 2.0 / 3.0]) {
            assert!((a - b).abs() < 1e-9, "{neutral:?}");
        }
        let dng = crate::write_linear_dng(
            &frame.image,
            &frame.color,
            frame.orientation,
            &frame.metadata,
            frame.baseline_exposure,
            crate::DngSamples::U16,
            Some(frame.clip),
        )
        .unwrap();
        let merged = lightcraft_raw::decode(&dng).unwrap();
        let (want, got) = (lightcraft_raw::color::as_shot_white_xy(&source), lightcraft_raw::color::as_shot_white_xy(&merged));
        assert!((want.x - got.x).abs() < 1e-3 && (want.y - got.y).abs() < 1e-3, "merged white {got:?}, camera white {want:?}");
        assert!((got.x - li_color::D65.x).abs() > 0.01, "merged DNG fell back to D65");
    }

    #[test]
    fn lens_gains_keep_clipped_samples_on_one_level_above_measured_corners() {
        // A sensor clipped in the centre; the corners stay measured but a strong vignette gain
        // (as in DJI DNGs) lifts them above the centre's clipped samples.
        let mut image = RgbFrame::from_fn(64, 48, |x, y| {
            let r2 = ((x as f32 - 31.5) / 40.0).powi(2) + ((y as f32 - 23.5) / 40.0).powi(2);
            if r2 < 0.04 { [1.0, 1.0, 1.0] } else { [0.6, 0.7, 0.5].map(|v| v * (1.0 - 0.2 * r2)) }
        });
        let list3 = [Opcode::FixVignetteRadial { k: [1.5, 0.0, 0.0, 0.0, 0.0], center: [0.5, 0.5] }];
        let mut reference = image.clone();
        lightcraft_raw::opcodes::apply_list3(&list3, &mut reference);
        let clip = apply_lens_opcodes(&list3, &mut image, 0.995);
        let corner_peak = reference.data.iter().flatten().copied().fold(0.0f32, f32::max);
        assert!(corner_peak > 1.0, "the fixture must lift measured corners above the sensor clip");
        assert!(image.get(31, 23).iter().all(|v| *v == clip), "{:?} vs {clip}", image.get(31, 23));
        for (got, want) in image.data.iter().zip(&reference.data) {
            if got[0] < clip {
                assert_eq!(got, want, "measured values are untouched");
                assert!(got.iter().all(|v| *v <= clip * crate::output::UNCLIPPED_PEAK));
            }
        }
        let mut unclipped = RgbFrame::filled(16, 12, [0.4; 3]);
        assert_eq!(apply_lens_opcodes(&[], &mut unclipped, 0.995), 0.995);
        let runs = std::cell::Cell::new(0);
        let level = correct_with_clip(&mut unclipped, 0.995, |img| {
            runs.set(runs.get() + 1);
            lightcraft_raw::opcodes::apply_list3(&list3, img);
        });
        assert_eq!((runs.get(), level), (1, 0.995), "an unclipped frame needs no clip mask");
    }

    #[test]
    fn common_orientation_takes_the_majority_and_breaks_ties_with_the_preferred_frame() {
        let raw = |orientation| RawImage {
            format: lightcraft_raw::RawFormat::Dng,
            width: 2,
            height: 2,
            cpp: 3,
            data: lightcraft_raw::RawData::F32(vec![0.0; 12]),
            cfa: None,
            bits: 32,
            black: lightcraft_raw::BlackLevel::uniform(0.0),
            white: vec![1.0],
            active_area: Rect::new(0, 0, 2, 2),
            crop: Rect::new(0, 0, 2, 2),
            orientation,
            color: ColorData::default(),
            wb_multipliers: None,
            linearized: false,
            vendor_lens: None,
            opcodes: Default::default(),
            metadata: Metadata::default(),
        };
        let raws = [raw(Orientation::Rotate90), raw(Orientation::Normal), raw(Orientation::Rotate90)];
        assert_eq!(common_orientation(&raws, 1), Orientation::Rotate90);
        assert_eq!(common_orientation(&raws[..2], 1), Orientation::Normal);
        assert_eq!(common_orientation(&raws[..2], 0), Orientation::Rotate90);
    }

    #[test]
    fn clip_detection() {
        let mut img = RgbFrame::from_fn(100, 100, |x, _| [x as f32 / 100.0; 3]);
        assert_eq!(detect_clip(&img), 1.0);
        for p in img.data.iter_mut().take(500) {
            *p = [0.9, 0.9, 0.9];
        }
        for p in img.data.iter_mut() {
            *p = p.map(|v| v.min(0.9));
        }
        assert!((detect_clip(&img) - 0.9 * 0.995).abs() < 1e-6);
    }
}

#[cfg(test)]
mod correction_regressions {
    use super::*;
    use lightcraft_raw::{BlackLevel, ColorData, Opcode, OpcodeLists, RawData, RawFormat, RawImage, Rect};

    #[test]
    fn merge_bakes_ordered_dng_corrections_and_marks_the_output() {
        let (w, h) = (32, 24);
        let raw = RawImage {
            format: RawFormat::Dng,
            width: w,
            height: h,
            cpp: 3,
            data: RawData::F32((0..w * h).flat_map(|i| [0.08 + (i % w) as f32 * 0.002, 0.12, 0.09 + (i / w) as f32 * 0.003]).collect()),
            cfa: None,
            bits: 32,
            black: BlackLevel::uniform(0.),
            white: vec![1.],
            active_area: Rect::new(0, 0, w, h),
            crop: Rect::new(0, 0, w, h),
            orientation: Orientation::Normal,
            color: ColorData::default(),
            wb_multipliers: None,
            linearized: false,
            vendor_lens: None,
            opcodes: OpcodeLists {
                list3: vec![
                    Opcode::WarpRectilinear {
                        planes: vec![[1., 0.04, 0., 0., 0., 0.], [1., 0.02, 0., 0., 0., 0.], [1., 0.06, 0., 0., 0., 0.]],
                        center: [0.5, 0.5],
                    },
                    Opcode::FixVignetteRadial { k: [0.5, 0., 0., 0., 0.], center: [0.5, 0.5] },
                    Opcode::WarpRectilinear { planes: vec![[1., -0.015, 0., 0., 0., 0.]], center: [0.4, 0.6] },
                ],
                ..Default::default()
            },
            metadata: Metadata { make: Some("DJI".into()), model: Some("merge fixture".into()), ..Default::default() },
        };
        let expected = raw.develop(lightcraft_raw::Method::Auto).unwrap();
        let merged = crate::merge_raw_hdr(
            &[raw.clone(), raw.clone()],
            &crate::HdrOptions { align: false, deghost: crate::Deghost::None },
            &crate::no_progress,
        )
        .unwrap();
        let close = |a: &RgbFrame, b: &RgbFrame| {
            assert_eq!((a.width, a.height), (b.width, b.height));
            let error = a.data.iter().flatten().zip(b.data.iter().flatten()).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
            assert!(error < 2e-6, "ordered correction error {error}");
        };
        close(&merged.radiance, &expected);
        assert!(merged.metadata.lens_corrections_baked);
        let bytes = crate::write_linear_dng(
            &merged.radiance,
            &merged.color,
            merged.orientation,
            &merged.metadata,
            merged.baseline_exposure,
            crate::DngSamples::Float,
            Some(merged.clip),
        )
        .unwrap();
        let reopened = lightcraft_raw::decode(&bytes).unwrap();
        assert!(reopened.metadata.lens_corrections_baked);
        assert!(reopened.metadata.chromatic_aberration_baked);
        assert!(reopened.opcodes.list3.is_empty(), "corrections already baked, never repeated");
        // The writer's scale travels in BaselineExposure.
        let gain = 2f32.powf((reopened.color.baseline_exposure - merged.baseline_exposure) as f32);
        close(&reopened.develop(lightcraft_raw::Method::Auto).unwrap().map(|p| p.map(|v| v * gain)), &expected);
        let input = lightcraft_raw::write_dng(
            &raw,
            &lightcraft_raw::DngWriteOptions { compression: lightcraft_raw::DngCompression::Deflate { tile: 16, half: false }, ..Default::default() },
        )
        .unwrap();
        let frame = load_frame(&input, None, false).unwrap();
        close(&frame.image, &expected);
        assert!(frame.metadata.lens_corrections_baked);
    }
}
