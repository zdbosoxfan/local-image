use super::*;

fn varint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
}

fn field_varint(f: u32, v: u64, out: &mut Vec<u8>) {
    varint(u64::from(f) << 3, out);
    varint(v, out);
}

fn field_bytes(f: u32, b: &[u8], out: &mut Vec<u8>) {
    varint(u64::from(f) << 3 | 2, out);
    varint(b.len() as u64, out);
    out.extend_from_slice(b);
}

fn bytes_of(f: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut v = Vec::new();
    f(&mut v);
    v
}

/// An ONNX `ValueInfoProto` (float tensor); `dims: None` leaves the shape unspecified, `None`
/// entries are symbolic.
fn value_info(name: &str, dims: Option<&[Option<u64>]>) -> Vec<u8> {
    bytes_of(|o| {
        field_bytes(1, name.as_bytes(), o);
        let tensor = bytes_of(|t| {
            field_varint(1, 1, t);
            if let Some(dims) = dims {
                let shape = bytes_of(|s| {
                    for (i, d) in dims.iter().enumerate() {
                        let dim = bytes_of(|x| match d {
                            Some(v) => field_varint(1, *v, x),
                            None => field_bytes(2, format!("d{i}").as_bytes(), x),
                        });
                        field_bytes(1, &dim, s);
                    }
                });
                field_bytes(2, &shape, t);
            }
        });
        let typ = bytes_of(|t| field_bytes(1, &tensor, t));
        field_bytes(2, &typ, o);
    })
}

fn node(op: &str, inputs: &[&str], output: &str, attrs: &[Vec<u8>]) -> Vec<u8> {
    bytes_of(|o| {
        for i in inputs {
            field_bytes(1, i.as_bytes(), o);
        }
        field_bytes(2, output.as_bytes(), o);
        field_bytes(4, op.as_bytes(), o);
        for a in attrs {
            field_bytes(5, a, o);
        }
    })
}

fn attr_int(name: &str, v: u64) -> Vec<u8> {
    bytes_of(|o| {
        field_bytes(1, name.as_bytes(), o);
        field_varint(3, v, o);
        field_varint(20, 2, o);
    })
}

fn attr_ints(name: &str, vs: &[u64]) -> Vec<u8> {
    bytes_of(|o| {
        field_bytes(1, name.as_bytes(), o);
        for v in vs {
            field_varint(8, *v, o);
        }
        field_varint(20, 7, o);
    })
}

/// What the tiny test model outputs.
#[derive(Clone, Copy)]
enum Out {
    /// `[1, 1, h, w]`: the sigmoid of the channel mean.
    Map,
    /// `[1, h, w]`.
    Flat,
    /// `[1, n, h, w]`: the channel mean repeated n times.
    Classes(usize),
    /// `[1, 3, h, w]`: the input itself (not a single map).
    Echo,
}

/// A tiny ONNX model (opset 13) with the given input size (`None`: flexible) and output.
fn tiny_onnx(input: Option<(u64, u64)>, out: Out) -> Vec<u8> {
    let dims: Vec<Option<u64>> = match input {
        Some((h, w)) => vec![Some(1), Some(3), Some(h), Some(w)],
        None => vec![Some(1), Some(3), None, None],
    };
    let mean = |keep: u64| node("ReduceMean", &["x"], "m", &[attr_ints("axes", &[1]), attr_int("keepdims", keep)]);
    let nodes = match out {
        Out::Map => vec![mean(1), node("Sigmoid", &["m"], "y", &[])],
        Out::Flat => vec![mean(0), node("Identity", &["m"], "y", &[])],
        Out::Classes(n) => vec![mean(1), node("Concat", &vec!["m"; n], "y", &[attr_int("axis", 1)])],
        Out::Echo => vec![node("Identity", &["x"], "y", &[])],
    };
    let graph = bytes_of(|g| {
        for n in &nodes {
            field_bytes(1, n, g);
        }
        field_bytes(2, b"tiny", g);
        field_bytes(11, &value_info("x", Some(&dims)), g);
        field_bytes(12, &value_info("y", None), g);
    });
    bytes_of(|m| {
        field_varint(1, 7, m);
        field_bytes(7, &graph, m);
        field_bytes(8, &bytes_of(|o| field_varint(2, 13, o)), m);
    })
}

fn temp_models() -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!("li-seg-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("segmentation")).unwrap();
    dir
}

/// Writes a tiny model into a scratch "downloads" folder and returns its path.
fn picked(dir: &Path, name: &str, input: Option<(u64, u64)>, out: Out) -> PathBuf {
    let p = dir.join("picked").join(name);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, tiny_onnx(input, out)).unwrap();
    p
}

fn keep(_: &Path) -> std::io::Result<()> {
    Ok(())
}

#[test]
fn object_flood_keeps_the_clicked_region_only() {
    let (w, h) = (10, 4);
    let mut p = vec![0.0f32; w * h];
    for y in 0..4 {
        p[y * w + 1] = 1.0;
        p[y * w + 2] = 0.9;
        p[y * w + 7] = 1.0;
    }
    let o = object_at(&p, w, h, 1, 1).unwrap();
    assert!(o[1] > 0.0 && o[2] > 0.0 && o[7] == 0.0);
    assert!(object_at(&p, w, h, 5, 1).is_none());
}

/// One official model per function, each explained in plain language.
#[test]
fn one_official_model_per_function() {
    assert_eq!(MODELS.len(), 3);
    for g in Group::ALL {
        let m = g.official();
        assert_eq!(m.group, Some(g));
        assert_eq!(MODELS.iter().filter(|x| x.group == Some(g)).count(), 1, "{g:?}");
        assert!(m.about.len() > 60 && !g.about().is_empty() && !g.label().is_empty());
        assert!(!m.label.contains("MB"), "{}", m.label);
        assert!(g.in_use(Path::new("/nonexistent")).is_none());
        assert!(!available(g, Path::new("/nonexistent")));
    }
    assert_eq!(Group::Subject.official().id, "isnet");
    assert_eq!(Group::Sky.official().id, "sky-mobileseg");
    assert_eq!(Group::Depth.official().id, "depth-anything-v2-small");
    assert_eq!(Group::Subject.official().task, Task::Subject);
    assert!(matches!(Group::Sky.official().task, Task::Sky { .. }));
    assert_eq!(Group::Depth.official().task, Task::Depth);
    for (file, _) in LEGACY {
        assert!(MODELS.iter().all(|m| m.file != *file), "{file} is still offered");
    }
}

#[test]
fn remove_deletes_only_that_model_and_reports_the_size() {
    let dir = temp_models();
    let (isnet, sky) = (spec("isnet").unwrap(), spec("sky-mobileseg").unwrap());
    std::fs::write(model_path(&dir, sky), vec![1u8; 1234]).unwrap();
    std::fs::write(model_path(&dir, isnet), vec![2u8; 99]).unwrap();
    assert_eq!(Group::Subject.in_use(&dir), Some(InUse::Official(isnet)));
    assert_eq!(installed_bytes(&dir, sky), Some(1234));
    assert_eq!(remove(&dir, isnet).unwrap(), 99);
    assert!(!model_path(&dir, isnet).exists());
    assert!(model_path(&dir, sky).exists(), "other models stay");
    assert!(Group::Subject.in_use(&dir).is_none());
    // removing a model that isn't installed is a no-op
    assert_eq!(remove(&dir, isnet).unwrap(), 0);
    // a custom disposer (the Trash) gets the file instead
    let moved = dir.join("trashed.onnx");
    let freed = remove_with(&dir, sky, &|p| std::fs::rename(p, &moved)).unwrap();
    assert_eq!(freed, 1234);
    assert!(moved.exists() && !available(Group::Sky, &dir));
    // a disposer that fails leaves the model installed and says so
    std::fs::write(model_path(&dir, sky), b"x").unwrap();
    assert!(remove_with(&dir, sky, &|_| Err(std::io::Error::other("nope"))).is_err());
    assert!(model_path(&dir, sky).exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_custom_subject_model_is_validated_copied_and_used() {
    let dir = temp_models();
    let src = picked(&dir, "my model.onnx", Some((64, 64)), Out::Map);
    let c = add_custom(&dir, Group::Subject, &src, CustomOptions { norm: Norm::ImageNet, sky_class: 2 }, &keep).unwrap();
    assert_eq!((c.size, c.norm, c.name.as_str()), (64, Norm::ImageNet, "my model.onnx"));
    assert!(c.path(&dir).starts_with(custom_dir(&dir)) && c.path(&dir).is_file(), "copied into the custom folder");
    assert!(src.is_file(), "the picked file stays where it was");
    // remembered, and used instead of the official one (which isn't even installed)
    assert_eq!(custom(&dir, Group::Subject), Some(c.clone()));
    assert_eq!(Group::Subject.in_use(&dir), Some(InUse::Custom(c.clone())));
    assert!(custom(&dir, Group::Sky).is_none());
    let seg = shared(&dir).expect("the custom model loads");
    assert!(seg.id().starts_with("custom:subject-"));
    let (w, h) = (80, 50);
    seg.predict(&synthetic(w, h), w, h).unwrap();
    // reverting puts the copy away and forgets the setting
    let moved = dir.join("trash.onnx");
    let freed = clear_custom(&dir, Group::Subject, &|p| std::fs::rename(p, &moved)).unwrap();
    assert!(freed > 0 && moved.is_file());
    assert!(custom(&dir, Group::Subject).is_none() && shared(&dir).is_none() && !custom_list(&dir).exists());
    assert_eq!(clear_custom(&dir, Group::Subject, &keep).unwrap(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_custom_model_beats_an_installed_official_one() {
    let dir = temp_models();
    std::fs::write(model_path(&dir, Group::Depth.official()), b"not a real model").unwrap();
    assert!(matches!(Group::Depth.in_use(&dir), Some(InUse::Official(_))));
    let src = picked(&dir, "d.onnx", Some((32, 32)), Out::Flat);
    add_custom(&dir, Group::Depth, &src, CustomOptions::default(), &keep).unwrap();
    assert!(matches!(Group::Depth.in_use(&dir), Some(InUse::Custom(_))));
    let seg = shared_depth(&dir).unwrap();
    let (w, h) = (40, 30);
    let d = seg.predict_depth(&synthetic(w, h), w, h).unwrap();
    assert_eq!(d.len(), w * h);
    // back to the official one
    clear_custom(&dir, Group::Depth, &|p| std::fs::remove_file(p)).unwrap();
    assert!(matches!(Group::Depth.in_use(&dir), Some(InUse::Official(_))));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_flexible_input_falls_back_to_the_official_size() {
    let dir = temp_models();
    let src = picked(&dir, "flex.onnx", None, Out::Map);
    assert_eq!(input_size(&src).unwrap(), None);
    let c = add_custom(&dir, Group::Depth, &src, CustomOptions::default(), &keep).unwrap();
    assert_eq!(c.size, Group::Depth.official().size);
    let fixed = picked(&dir, "fixed.onnx", Some((48, 48)), Out::Map);
    assert_eq!(input_size(&fixed).unwrap(), Some(48));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn models_that_do_not_fit_are_refused_with_a_reason_and_change_nothing() {
    let dir = temp_models();
    let echo = picked(&dir, "echo.onnx", Some((32, 32)), Out::Echo);
    let e = format!("{:#}", add_custom(&dir, Group::Subject, &echo, CustomOptions::default(), &keep).unwrap_err());
    assert!(e.contains("Not a subject model"), "{e}");
    let e = format!("{:#}", add_custom(&dir, Group::Depth, &echo, CustomOptions::default(), &keep).unwrap_err());
    assert!(e.contains("Not a depth model"), "{e}");
    // a sky model needs one map or one per class: this is 3 channels (RGB echoed), so class 5 is out of range
    let e = format!("{:#}", add_custom(&dir, Group::Sky, &echo, CustomOptions { sky_class: 5, ..Default::default() }, &keep).unwrap_err());
    assert!(e.contains("out of range"), "{e}");
    // a sky model must have a channel axis
    let flat = picked(&dir, "flat.onnx", Some((32, 32)), Out::Flat);
    let e = format!("{:#}", add_custom(&dir, Group::Sky, &flat, CustomOptions::default(), &keep).unwrap_err());
    assert!(e.contains("Not a sky model"), "{e}");
    // not square, not an image input, not ONNX at all
    let wide = picked(&dir, "wide.onnx", Some((64, 32)), Out::Map);
    let e = format!("{:#}", add_custom(&dir, Group::Subject, &wide, CustomOptions::default(), &keep).unwrap_err());
    assert!(e.contains("square"), "{e}");
    let junk = dir.join("picked/junk.onnx");
    std::fs::write(&junk, b"this is not a model").unwrap();
    assert!(add_custom(&dir, Group::Subject, &junk, CustomOptions::default(), &keep).is_err());
    let txt = dir.join("picked/readme.txt");
    std::fs::write(&txt, b"hi").unwrap();
    let e = format!("{:#}", add_custom(&dir, Group::Subject, &txt, CustomOptions::default(), &keep).unwrap_err());
    assert!(e.contains(".onnx"), "{e}");
    assert!(add_custom(&dir, Group::Subject, &dir.join("missing.onnx"), CustomOptions::default(), &keep).is_err());
    for g in Group::ALL {
        assert!(custom(&dir, g).is_none());
    }
    assert!(!custom_dir(&dir).exists() || std::fs::read_dir(custom_dir(&dir)).unwrap().next().is_none(), "nothing was copied");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sky_models_are_one_sigmoid_map_or_class_logits() {
    let dir = temp_models();
    let one = picked(&dir, "one.onnx", Some((32, 32)), Out::Map);
    let c = add_custom(&dir, Group::Sky, &one, CustomOptions { sky_class: 7, ..Default::default() }, &keep).unwrap();
    assert_eq!((c.classes, c.class), (1, 0), "a single map ignores the class index");
    let seg = shared_sky(&dir).unwrap();
    let (w, h) = (30, 20);
    let p = seg.predict_sky(&synthetic(w, h), w, h).unwrap();
    assert_eq!(p.len(), w * h);
    // ADE20K-style class logits: the sky class is the one the user names
    let many = picked(&dir, "many.onnx", Some((32, 32)), Out::Classes(4));
    let c = add_custom(&dir, Group::Sky, &many, CustomOptions { sky_class: 2, ..Default::default() }, &keep).unwrap();
    assert_eq!((c.classes, c.class), (4, 2));
    assert_eq!(custom(&dir, Group::Sky).unwrap().classes, 4, "replaced, not added");
    assert!(shared_sky(&dir).unwrap().predict_sky(&synthetic(w, h), w, h).is_ok());
    let e = format!("{:#}", add_custom(&dir, Group::Sky, &many, CustomOptions { sky_class: 4, ..Default::default() }, &keep).unwrap_err());
    assert!(e.contains("0 to 3"), "{e}");
    assert_eq!(custom(&dir, Group::Sky).unwrap().class, 2, "a refused model leaves the setting alone");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn replacing_a_custom_model_puts_the_old_copy_away() {
    let dir = temp_models();
    let a = picked(&dir, "a.onnx", Some((32, 32)), Out::Map);
    let b = picked(&dir, "b.onnx", Some((32, 32)), Out::Map);
    let first = add_custom(&dir, Group::Subject, &a, CustomOptions::default(), &keep).unwrap();
    let put_away = std::sync::Mutex::new(Vec::new());
    let second = add_custom(&dir, Group::Subject, &b, CustomOptions::default(), &|p| {
        put_away.lock().unwrap().push(p.to_path_buf());
        Ok(())
    })
    .unwrap();
    assert_eq!(*put_away.lock().unwrap(), vec![first.path(&dir)]);
    assert_eq!(custom(&dir, Group::Subject), Some(second));
    // the same function twice with the same file is fine and puts nothing away
    let again = add_custom(&dir, Group::Subject, &b, CustomOptions { norm: Norm::ImageNet, ..Default::default() }, &|_| panic!("nothing to put away")).unwrap();
    assert_eq!(again.norm, Norm::ImageNet);
    // functions are independent
    let d = picked(&dir, "d.onnx", Some((32, 32)), Out::Flat);
    add_custom(&dir, Group::Depth, &d, CustomOptions::default(), &keep).unwrap();
    assert!(custom(&dir, Group::Subject).is_some() && custom(&dir, Group::Depth).is_some());
    clear_custom(&dir, Group::Depth, &keep).unwrap();
    assert!(custom(&dir, Group::Subject).is_some() && custom(&dir, Group::Depth).is_none());
    // a custom model whose file has gone is ignored
    std::fs::remove_file(custom(&dir, Group::Subject).unwrap().path(&dir)).unwrap();
    assert!(custom(&dir, Group::Subject).is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn old_models_are_listed_never_used_and_can_be_removed() {
    let dir = temp_models();
    assert!(legacy_installed(&dir).is_empty());
    let seg = dir.join("segmentation");
    std::fs::write(seg.join("u2netp.onnx"), vec![0u8; 100]).unwrap();
    std::fs::write(seg.join("midas_v21_small_256.onnx"), vec![0u8; 50]).unwrap();
    std::fs::write(seg.join("unrelated.onnx"), vec![0u8; 7]).unwrap();
    assert_eq!(legacy_installed(&dir).iter().map(|(_, b)| b).sum::<u64>(), 150);
    for g in Group::ALL {
        assert!(!available(g, &dir), "old files don't serve {g:?}");
    }
    let trash = dir.join("trash");
    std::fs::create_dir_all(&trash).unwrap();
    let freed = remove_legacy_with(&dir, &|p| std::fs::rename(p, trash.join(p.file_name().unwrap()))).unwrap();
    assert_eq!(freed, 150);
    assert!(legacy_installed(&dir).is_empty() && seg.join("unrelated.onnx").is_file());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_custom_list_survives_odd_names_and_bad_lines() {
    let dir = temp_models();
    let src = picked(&dir, "wé ird:na/me?.onnx", Some((32, 32)), Out::Map);
    let c = add_custom(&dir, Group::Subject, &src, CustomOptions::default(), &keep).unwrap();
    assert!(!c.file.contains(['/', ':', '?']), "{}", c.file);
    assert_eq!(custom(&dir, Group::Subject).unwrap().file, c.file);
    let mut text = std::fs::read_to_string(custom_list(&dir)).unwrap();
    text.push_str("garbage line\nsky\tx\n");
    std::fs::write(custom_list(&dir), text).unwrap();
    assert_eq!(custom(&dir, Group::Subject), Some(c));
    let _ = std::fs::remove_dir_all(dir);
}

/// Runs the official subject model on a synthetic subject when it's available (downloaded by the
/// developer to `LI_SEG_TEST_MODELS`).
#[test]
fn isnet_finds_a_centred_subject() {
    let Some(dir) = std::env::var_os("LI_SEG_TEST_MODELS") else { return };
    let spec = spec("isnet").unwrap();
    let path = Path::new(&dir).join(spec.file);
    if !path.is_file() {
        return;
    }
    let seg = Segmenter::load(spec, &path).unwrap();
    let (w, h) = (256, 192);
    let mut img = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let inside = ((x as f32 - 128.0) / 60.0).powi(2) + ((y as f32 - 96.0) / 70.0).powi(2) < 1.0;
            let c = if inside { [220, 60, 40] } else { [70 + (x % 9) as u8, 120, 160] };
            img[i..i + 3].copy_from_slice(&c);
            img[i + 3] = 255;
        }
    }
    let p = seg.predict(&img, w, h).unwrap().expect("a subject");
    assert!(p[96 * w + 128] > 0.5, "centre {}", p[96 * w + 128]);
    assert!(p[5 * w + 5] < 0.5, "corner {}", p[5 * w + 5]);
}

/// The sky model on a synthetic landscape when available (`LI_SEG_TEST_MODELS` holding the
/// PP-MobileSeg file).
#[test]
fn sky_model_finds_the_sky() {
    let Some(dir) = std::env::var_os("LI_SEG_TEST_MODELS") else { return };
    let spec = spec("sky-mobileseg").unwrap();
    let path = Path::new(&dir).join(spec.file);
    if !path.is_file() {
        return;
    }
    let seg = Segmenter::load(spec, &path).unwrap();
    let (w, h) = (320, 240);
    let mut img = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let c = if y < 110 { [90 + (y / 3) as u8, 150 + (y / 4) as u8, 230] } else { [70 + (x % 13) as u8, 110 + (y % 7) as u8, 50] };
            img[i..i + 3].copy_from_slice(&c);
            img[i + 3] = 255;
        }
    }
    let p = seg.predict_sky(&img, w, h).unwrap();
    assert!(p[30 * w + 160] > 0.5, "sky {}", p[30 * w + 160]);
    assert!(p[200 * w + 160] < 0.5, "ground {}", p[200 * w + 160]);
}

/// A photo-like scene with depth cues: a checkered ground plane receding to the horizon
/// (in perspective) under a plain sky, and a box standing on the near ground.
fn ground_scene(w: usize, h: usize) -> Vec<u8> {
    let horizon = h as f32 * 0.4;
    let mut img = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let c = if (y as f32) < horizon {
                [150 + (y * 60 / h) as u8, 185, 235]
            } else {
                // ground point at distance z ∝ 1 / (y − horizon)
                let dy = (y as f32 - horizon + 0.5).max(0.5);
                let z = 40.0 / dy;
                let gx = (x as f32 - w as f32 / 2.0) * z / 40.0;
                let check = ((gx.floor() as i64 + (z * 2.0).floor() as i64) & 1) == 0;
                let fog = (z / 4.0).min(1.0);
                let base = if check { [120.0, 100.0, 70.0] } else { [70.0, 60.0, 45.0] };
                std::array::from_fn(|k| (base[k] * (1.0 - fog) + [150.0, 175.0, 210.0][k] * fog) as u8)
            };
            let in_box = x > w * 2 / 5 && x < w * 3 / 5 && y > h * 3 / 5 && y < h * 9 / 10;
            let c = if in_box { [200, 40, 40] } else { c };
            img[i..i + 3].copy_from_slice(&c);
            img[i + 3] = 255;
        }
    }
    img
}

/// The depth model runs under tract and sees the near ground as nearer than the horizon
/// (`LI_SEG_TEST_MODELS` holding the Depth Anything V2 Small file).
#[test]
fn depth_model_finds_the_near_ground() {
    let Some(dir) = std::env::var_os("LI_SEG_TEST_MODELS") else { return };
    let spec = spec("depth-anything-v2-small").unwrap();
    let path = Path::new(&dir).join(spec.file);
    if !path.is_file() {
        return;
    }
    let seg = Segmenter::load(spec, &path).unwrap();
    let (w, h) = (384, 288);
    let img = ground_scene(w, h);
    let d = seg.predict_depth(&img, w, h).unwrap();
    assert_eq!(d.len(), w * h);
    assert!(d.iter().all(|v| (0.0..=1.0).contains(v)));
    let at = |x: usize, y: usize| d[y * w + x];
    let (near, far) = (at(w / 8, h - 6), at(w / 8, (h as f32 * 0.42) as usize));
    assert!(near > far + 0.2, "near ground {near}, horizon {far}");
    assert!(at(w / 2, h * 3 / 4) > far, "the box is nearer than the horizon");
}
