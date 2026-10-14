//! J26: the editor's lens profile and red eye through the Develop bridge — parity with Develop
//! and the quality measurements recorded in docs/wip/CLAUDE-REPORT-j26-lens-bridge.md.

use super::*;
use photocraft_cms::Builtin;
use photocraft_color::{ColorMode, PixelFormat, SampleType};

const CAMERA: (&str, &str) = ("Canon", "Canon EOS 5D Mark II");
const LENS_NAME: &str = "Canon EF 24-70mm f/2.8L USM";

/// A camera's EXIF as its JPEG carries it (IFD0 + Exif IFD with the recorded pixel size).
fn camera_exif(width: u32, height: u32, orientation: u16) -> Vec<u8> {
    camera_exif_at(24, width, height, orientation)
}

fn camera_exif_at(focal: u32, width: u32, height: u32, orientation: u16) -> Vec<u8> {
    use lightcraft_tiff::{ByteOrder, IfdBuilder, TiffWriter, Value};
    let mut ex = IfdBuilder::new();
    ex.set(0x829D, Value::Rational(vec![(4, 1)])); // FNumber
    ex.set(0x920A, Value::Rational(vec![(focal, 1)])); // FocalLength
    ex.set(0xA434, Value::Ascii(LENS_NAME.into())); // LensModel
    ex.set(0xA002, Value::Long(vec![width])); // PixelXDimension
    ex.set(0xA003, Value::Long(vec![height])); // PixelYDimension
    let mut ifd0 = IfdBuilder::new();
    ifd0.set(0x010F, Value::Ascii(CAMERA.0.into())); // Make
    ifd0.set(0x0110, Value::Ascii(CAMERA.1.into())); // Model
    ifd0.set(0x0112, Value::Short(vec![orientation])); // Orientation
    ifd0.set_child(0x8769, ex);
    TiffWriter::new(ByteOrder::Little, false).write(&[ifd0]).unwrap()
}

fn enc(v: f32) -> f32 {
    li_color::curve::srgb_trc().eval_inverse(v.clamp(0.0, 1.0))
}

#[test]
fn j26_lens_profile_is_develops_lensfun_query() {
    let exif = camera_exif(6000, 4000, 1);
    let p = LensProfile::from_exif(&exif, 360, 240).expect("the bundled database knows the lens");
    // Develop's `lens_db::for_photo` query for a photo with this camera, lens and shot.
    let develop = lightcraft_engine::lens_db::correction(&lightcraft_engine::lens_db::Query {
        exif_camera: format!("{} {}", CAMERA.0, CAMERA.1),
        exif_lens: LENS_NAME.into(),
        camera: None,
        lens: None,
        focal: 24.0,
        aperture: Some(4.0),
        distance: None,
    });
    assert!(develop.is_some());
    assert_eq!(p.correction(), develop);
    assert!(p.label().contains("24-70"), "{}", p.label());
    // Resized: still the whole frame. Cropped: refused.
    assert!(LensProfile::from_exif(&exif, 1500, 1000).is_some());
    assert!(LensProfile::from_exif(&exif, 300, 240).is_none());
    // Portrait shot shown upright keeps its orientation for the optical centre.
    let turned = LensProfile::from_exif(&camera_exif(6000, 4000, 6), 240, 360).unwrap();
    assert_eq!(turned.orientation, 6);
    // Stored form round-trips.
    assert_eq!(LensProfile::from_json(&p.to_json()).unwrap(), p);
    assert_eq!(LensProfile::from_json(&turned.to_json()).unwrap(), turned);
    assert!(LensProfile::from_json(&serde_json::json!({"focal": 24})).is_err());
    // The map is Develop's map: same source positions and gain.
    let (w, h) = (360.0, 240.0);
    let m = p.map(w, h, true, true, true).unwrap();
    let dm = develop.unwrap().on(w, h);
    for (x, y) in [(0.5, 0.5), (180.0, 120.0), (359.5, 239.5), (40.0, 200.0)] {
        for ch in 0..3 {
            assert_eq!(m.to_source(x, y, ch), dm.to_source(x, y, ch, 1.0, 1.0));
        }
        assert_eq!(m.gain(x, y), dm.gain(x, y, 1.0));
    }
}

/// Smooth linear-light content (resampler differences stay small) with a little colour.
fn smooth(w: i32, h: i32) -> Surface {
    let mut s = Surface::new(PixelFormat::new(ColorMode::Rgb, SampleType::F32, true));
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
            let a = 0.35 + 0.2 * (u * 9.0).sin() * (v * 7.0).cos();
            data.extend([a, a * 0.8 + 0.05 * v, a * 0.6 + 0.1 * u, 1.0]);
        }
    }
    s.write_region(Rect::new(0, 0, w, h), &data);
    s
}

fn rgb(s: &Surface, area: Rect) -> Vec<[f32; 3]> {
    s.read_region(area).as_chunks::<4>().0.iter().map(|p| [p[0], p[1], p[2]]).collect()
}

/// (mean, max) display-encoded difference over the interior (a 4 % border left out, where the
/// corrected image runs past the frame).
fn diff(a: &[[f32; 3]], b: &[[f32; 3]], w: usize, h: usize) -> (f32, f32) {
    let (mx, my) = (w / 25, h / 25);
    let (mut sum, mut n, mut worst) = (0.0f64, 0usize, 0.0f32);
    for y in my..h - my {
        for x in mx..w - mx {
            for c in 0..3 {
                let d = (enc(a[y * w + x][c]) - enc(b[y * w + x][c])).abs();
                sum += f64::from(d);
                n += 1;
                worst = worst.max(d);
            }
        }
    }
    ((sum / n as f64) as f32, worst)
}

#[test]
fn j26_lens_correction_matches_develops_render() {
    let (w, h) = (360, 240);
    let frame = Rect::new(0, 0, w, h);
    let db = LensProfile::from_exif(&camera_exif(6000, 4000, 1), w as u32, h as u32).unwrap();
    let c = db.correction().unwrap();
    assert!(c.moves_pixels() && c.vignetting.is_some());
    let src = smooth(w, h);
    let profile = linear_rec2020();
    let manual = LensCorrection::default();
    let t0 = std::time::Instant::now();
    let editor = lens_correction_surface(&src, frame, &manual, Some(&db), profile).unwrap();
    let editor_ms = t0.elapsed().as_secs_f64() * 1e3;
    // Develop's own CPU render of the same pixels with the lensfun profile.
    let mut s = identity().clone();
    s.optics.lens_profile = true;
    s.lens_db.enabled = true;
    let info = lightcraft_pipeline::SourceInfo { lens_db: Some(c), ..Default::default() };
    let req = lightcraft_pipeline::RenderRequest {
        space: lightcraft_pipeline::OutputSpace::Rec2020,
        depth: lightcraft_pipeline::OutputDepth::F32Linear,
        ..lightcraft_pipeline::RenderRequest::fit(w as usize, h as usize)
    };
    let raster = lightcraft_raster::Rgb32f { width: w as usize, height: h as usize, data: rgb(&src, frame) };
    let out = lightcraft_pipeline::render(&raster, &info, &s, &req).deep.unwrap();
    let lightcraft_pipeline::DeepSamples::F32(v) = out.samples else { panic!("float output") };
    let develop = v.as_chunks::<3>().0.to_vec();
    let (mean, worst) = diff(&rgb(&editor, frame), &develop, w as usize, h as usize);
    eprintln!("J26 lens: editor vs Develop render: mean {:.3} max {:.3} levels/255; editor {editor_ms:.1} ms", mean * 255.0, worst * 255.0);
    assert!(mean < 0.75 / 255.0 && worst < 3.0 / 255.0, "editor vs Develop: mean {mean} max {worst}");
    // Before J26, "auto" on this EXIF meant the editor's generic 24 mm-equivalent profile.
    let generic = photocraft_algo::lens::LensCorrection { profile: Some(photocraft_algo::lens::generic_profile(24.0)), ..Default::default() };
    let old = photocraft_algo::lens::correct(&src, frame, &generic);
    let (old_mean, old_worst) = diff(&rgb(&old, frame), &develop, w as usize, h as usize);
    eprintln!("J26 lens: old generic 'auto' vs Develop render: mean {:.3} max {:.3} levels/255", old_mean * 255.0, old_worst * 255.0);
    assert!(old_mean > 4.0 * mean);
    // Every part can be switched off.
    let off = LensCorrection { correct_distortion: false, correct_ca: false, correct_vignette: false, ..Default::default() };
    assert_eq!(lens_correction_surface(&src, frame, &off, Some(&db), profile).unwrap().read_region(frame), src.read_region(frame));
    // 24 mm is barrel (the corners come from inside the frame); at 70 mm the lens is pincushion:
    // the corrected corners run past the frame and are transparent (Photoshop's default edge),
    // and Auto Scale enlarges until both stages cover the frame.
    assert!(editor.rgba(0, 0)[3] > 0.99);
    assert_eq!(lens_auto_scale(&manual, frame, Some(&db)), 100.0);
    let tele = LensProfile::from_exif(&camera_exif_at(70, 6000, 4000, 1), w as u32, h as u32).unwrap();
    let out = lens_correction_surface(&src, frame, &manual, Some(&tele), profile).unwrap();
    assert!(out.rgba(0, 0)[3] < 0.5, "{:?}", out.rgba(0, 0));
    let scale = lens_auto_scale(&manual, frame, Some(&tele));
    assert!(scale > 100.0, "{scale}");
    let covered = lens_correction_surface(&src, frame, &LensCorrection { scale, ..manual.clone() }, Some(&tele), profile).unwrap();
    let corners = [covered.rgba(0, 0)[3], covered.rgba(w - 1, 0)[3], covered.rgba(0, h - 1)[3], covered.rgba(w - 1, h - 1)[3]];
    assert!(corners.iter().all(|a| *a > 0.999), "{corners:?} at {scale}");
}

// ---------------------------------------------------------------------------- red eye

#[derive(Clone, Copy)]
struct Scene {
    w: i32,
    h: i32,
    eye: (f32, f32),
    skin: [f32; 3],
    iris: Option<(f32, [f32; 3])>,
    pupil_r: f32,
    pupil: [f32; 3],
    catchlight: bool,
}

fn paint(sc: &Scene) -> Surface {
    let fmt = PixelFormat::new(ColorMode::Rgb, SampleType::U8, true);
    let mut s = Surface::new(fmt);
    let mut data = Vec::new();
    for y in 0..sc.h {
        for x in 0..sc.w {
            let d = (x as f32 + 0.5 - sc.eye.0).hypot(y as f32 + 0.5 - sc.eye.1);
            let dc = (x as f32 + 0.5 - sc.eye.0 + sc.pupil_r * 0.35).hypot(y as f32 + 0.5 - sc.eye.1 + sc.pupil_r * 0.35);
            let c = if sc.catchlight && dc < sc.pupil_r * 0.22 {
                [0.97, 0.97, 0.97]
            } else if d < sc.pupil_r {
                // a little shading towards the rim, as flash pupils have
                let k = 1.0 - 0.25 * (d / sc.pupil_r).powi(2);
                sc.pupil.map(|v| v * k)
            } else if let Some((r, c)) = sc.iris.filter(|(r, _)| d < *r) {
                let _ = r;
                c
            } else {
                sc.skin
            };
            let mut o = vec![0.0; 4];
            from_rgba_into(&fmt, [c[0], c[1], c[2], 1.0], &mut o);
            data.extend(o);
        }
    }
    s.write_region(Rect::new(0, 0, sc.w, sc.h), &data);
    s
}

/// Pupil residual red, pupil brightness, and the mean / max change of everything 2 px past the
/// pupil (iris, catchlight-free skin), display-encoded.
fn score(sc: &Scene, before: &Surface, after: &Surface) -> (f32, f32, f32, f32) {
    let area = Rect::new(0, 0, sc.w, sc.h);
    let (a, b) = (before.read_region(area), after.read_region(area));
    let (mut red, mut lum, mut n) = (0.0f32, 0.0f32, 0usize);
    let (mut col_sum, mut col_n, mut col_max) = (0.0f32, 0usize, 0.0f32);
    for (i, (p, q)) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0).enumerate() {
        let (x, y) = ((i as i32 % sc.w) as f32 + 0.5, (i as i32 / sc.w) as f32 + 0.5);
        let d = (x - sc.eye.0).hypot(y - sc.eye.1);
        let dc = (x - sc.eye.0 + sc.pupil_r * 0.35).hypot(y - sc.eye.1 + sc.pupil_r * 0.35);
        if d < sc.pupil_r * 0.8 && !(sc.catchlight && dc < sc.pupil_r * 0.4) {
            red += (q[0] - q[1].max(q[2])).max(0.0);
            lum += (q[0] + q[1] + q[2]) / 3.0;
            n += 1;
        } else if d > sc.pupil_r + 2.0 {
            let ch = (0..3).map(|c| (p[c] - q[c]).abs()).fold(0.0f32, f32::max);
            col_sum += ch;
            col_n += 1;
            col_max = col_max.max(ch);
        }
    }
    (red / n.max(1) as f32, lum / n.max(1) as f32, col_sum / col_n.max(1) as f32, col_max)
}

fn fix(sc: &Scene, click: (f32, f32), radius: f64, pet: bool) -> Option<Surface> {
    let surf = paint(sc);
    let eye = EyeFix { center: [f64::from(click.0), f64::from(click.1)], radii: [radius; 2], pupil_size: 50.0, darken: 50.0, pet, catchlight: false };
    red_eye_surface(&surf, Rect::new(0, 0, sc.w, sc.h), &eye, Builtin::Srgb.profile()).unwrap()
}

const SKIN: [f32; 3] = [0.87, 0.66, 0.56];

fn scenes() -> Vec<(&'static str, Scene, (f32, f32), f64)> {
    let classic = Scene { w: 60, h: 60, eye: (30.0, 30.0), skin: [0.85, 0.65, 0.55], iris: None, pupil_r: 6.0, pupil: [0.85, 0.1, 0.12], catchlight: false };
    let eye = Scene {
        w: 240,
        h: 160,
        eye: (120.0, 80.0),
        skin: SKIN,
        iris: Some((16.0, [0.36, 0.24, 0.14])),
        pupil_r: 8.0,
        pupil: [0.78, 0.16, 0.12],
        catchlight: true,
    };
    let big = Scene {
        w: 480,
        h: 480,
        eye: (240.0, 240.0),
        skin: SKIN,
        iris: Some((40.0, [0.36, 0.24, 0.14])),
        pupil_r: 18.0,
        pupil: [0.74, 0.18, 0.15],
        catchlight: true,
    };
    vec![
        // (name, scene, click, search radius as the tool uses it: canvas short side / 12, 12..200)
        ("classic", classic, (31.5, 29.5), 12.0),
        ("eye with iris + catchlight", eye, (123.5, 82.5), 13.0),
        ("large search radius", big, (246.5, 236.5), 200.0),
    ]
}

#[test]
fn j26_red_eye_through_develop_fixes_pupils_and_leaves_the_rest() {
    for (name, sc, click, radius) in scenes() {
        let before = paint(&sc);
        let t0 = std::time::Instant::now();
        let after = fix(&sc, click, radius, false).unwrap_or_else(|| panic!("{name}: pupil not found"));
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        let (red, lum, col_mean, col_max) = score(&sc, &before, &after);
        let (red0, lum0, ..) = score(&sc, &before, &before);
        eprintln!(
            "J26 red eye {name}: pupil red {red0:.3}→{red:.3}, pupil level {lum0:.3}→{lum:.3}, outside change mean {col_mean:.4} max {col_max:.3}, {ms:.1} ms"
        );
        assert!(red < 0.02, "{name}: red left {red}");
        assert!(lum < 0.2, "{name}: pupil not dark {lum}");
        assert!(col_mean < 0.004, "{name}: collateral {col_mean}");
        // alpha and everything far away keep their exact values
        let area = Rect::new(0, 0, sc.w, sc.h);
        let (a, b) = (before.read_region(area), after.read_region(area));
        for (i, (p, q)) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0).enumerate() {
            assert_eq!(p[3].to_bits(), q[3].to_bits());
            let (x, y) = ((i as i32 % sc.w) as f32 + 0.5, (i as i32 / sc.w) as f32 + 0.5);
            if (x - sc.eye.0).hypot(y - sc.eye.1) > sc.pupil_r * 1.6 + 2.0 {
                assert_eq!(p, q, "{name}: pixel {x},{y} changed");
            }
        }
    }
    // A click on plain skin finds nothing (no history step in the editor).
    let (_, eye, ..) = scenes()[1];
    assert!(fix(&eye, (30.5, 30.5), 13.0, false).is_none());
    // A green-glowing pet pupil is no red eye, but Pet Eye darkens it.
    let pet = Scene { pupil: [0.62, 0.8, 0.35], iris: Some((16.0, [0.22, 0.17, 0.1])), catchlight: false, ..eye };
    assert!(fix(&pet, (122.5, 81.5), 13.0, false).is_none());
    let before = paint(&pet);
    let after = fix(&pet, (122.5, 81.5), 13.0, true).expect("pet eye glow found");
    let (_, lum, col_mean, _) = score(&pet, &before, &after);
    let (_, lum0, ..) = score(&pet, &before, &before);
    eprintln!("J26 pet eye: pupil level {lum0:.3}→{lum:.3}, outside change mean {col_mean:.4}");
    assert!(lum < 0.2 && col_mean < 0.004, "pet: {lum} {col_mean}");
}
