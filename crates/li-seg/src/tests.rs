use super::*;
use crate::custom::{add_custom_with, custom_list, synthetic};

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
    typed_value_info(name, dims, 1)
}

/// [`value_info`] of ONNX element type `elem` (1 = float, 10 = float16).
fn typed_value_info(name: &str, dims: Option<&[Option<u64>]>, elem: u64) -> Vec<u8> {
    bytes_of(|o| {
        field_bytes(1, name.as_bytes(), o);
        let tensor = bytes_of(|t| {
            field_varint(1, elem, t);
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
    assert_eq!(MODELS.iter().filter(|m| m.group.is_some()).count(), 10);
    assert_eq!(MODELS.iter().filter(|m| m.task == Task::Upscale).count(), 4);
    assert!(MODELS.iter().filter(|m| m.task == Task::Upscale).all(|m| m.group.is_none()));
    for g in Group::ALL {
        let m = g.official();
        assert_eq!(m.group, Some(g));
        assert_eq!(MODELS.iter().filter(|x| x.group == Some(g)).count(), if matches!(g, Group::Faces | Group::Subject) { 2 } else { 1 }, "{g:?}");
        assert!(m.about.len() > 60 && !g.about().is_empty() && !g.label().is_empty());
        // Match size units, not the letters inside the RMBG model's name.
        let size_label = m
            .label
            .split_whitespace()
            .any(|word| matches!(word, "MB" | "MiB") || ((word.ends_with("MB") || word.ends_with("MiB")) && word.starts_with(|c: char| c.is_ascii_digit())));
        assert!(!size_label, "{}", m.label);
        assert!(g.in_use(Path::new("/nonexistent")).is_none());
        assert!(!available(g, Path::new("/nonexistent")));
    }
    assert_eq!(Group::Subject.official().id, "rmbg-1.4");
    assert_eq!(Group::Sky.official().id, "sky-mobileseg");
    assert_eq!(Group::Depth.official().id, "depth-anything-v2-small");
    assert_eq!(Group::Subject.official().task, Task::Subject);
    assert!(matches!(Group::Sky.official().task, Task::Sky { .. }));
    assert_eq!(Group::Depth.official().task, Task::Depth);
    assert_eq!(Group::Tagging.official().task, Task::ImageText);
    assert_eq!(Group::Tagging.official().files().count(), 4);
    for group in [Group::Subject, Group::Sky, Group::Depth] {
        assert!(group.supports_custom());
        assert!(group.official().companions.is_empty());
    }
    assert!(!Group::Tagging.supports_custom());
}

#[test]
fn remove_deletes_only_that_model_and_reports_the_size() {
    let dir = temp_models();
    let (isnet, sky) = (spec("rmbg-1.4").unwrap(), spec("sky-mobileseg").unwrap());
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
    let c = add_custom(&dir, Group::Subject, &src, CustomOptions { norm: Norm::ImageNet, sky_class: 2, ..Default::default() }, &keep).unwrap();
    assert_eq!((c.size, c.norm, c.name.as_str()), (64, Norm::ImageNet, "my model.onnx"));
    assert!(c.path(&dir).starts_with(custom_dir(&dir)) && c.path(&dir).is_file(), "copied into the custom folder");
    assert!(src.is_file(), "the picked file stays where it was");
    // remembered, and used instead of the official one (which isn't even installed)
    assert_eq!(custom(&dir, Group::Subject), Some(c.clone()));
    assert_eq!(Group::Subject.in_use(&dir), Some(InUse::Custom(c.clone())));
    assert!(custom(&dir, Group::Sky).is_none());
    let seg = shared(&dir).unwrap().expect("the custom model loads");
    assert!(seg.id().starts_with("custom:subject-"));
    let (w, h) = (80, 50);
    seg.predict(&synthetic(w, h), w, h).unwrap();
    // reverting puts the copy away and forgets the setting
    let moved = dir.join("trash.onnx");
    let freed = clear_custom(&dir, Group::Subject, &|p| std::fs::rename(p, &moved)).unwrap();
    assert!(freed > 0 && moved.is_file());
    assert!(custom(&dir, Group::Subject).is_none() && shared(&dir).unwrap().is_none() && !custom_list(&dir).exists());
    assert_eq!(clear_custom(&dir, Group::Subject, &keep).unwrap(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn review_model_snapshot_distinguishes_equal_length_artifacts_and_roots() {
    for group in [Group::Subject, Group::Sky, Group::Depth] {
        let a = temp_models();
        let b = temp_models();
        let source_a = picked(&a, "same.onnx", Some((32, 32)), Out::Map);
        let mut different = std::fs::read(&source_a).unwrap();
        let at = different.windows(7).position(|s| s == b"Sigmoid").unwrap();
        different[at..at + 7].copy_from_slice(b"Softmax");
        let source_b = b.join("same.onnx");
        std::fs::write(&source_b, &different).unwrap();
        add_custom(&a, group, &source_a, CustomOptions::default(), &keep).unwrap();
        add_custom(&b, group, &source_b, CustomOptions::default(), &keep).unwrap();
        let first = snapshot(group, &a).unwrap();
        let second = snapshot(group, &b).unwrap();
        assert_eq!(first.key, second.key, "same basename/config/length reproduces the previous collision");
        assert_ne!(first, second);
        let old = first.segmenter().unwrap().unwrap();
        let new = second.segmenter().unwrap().unwrap();
        assert!(!Arc::ptr_eq(&old.run, &new.run), "another root must load its own plan");
        if group == Group::Subject {
            let input = vec![255; 32 * 32 * 4];
            assert!(old.predict(&input, 32, 32).unwrap().is_some());
            assert!(new.predict(&input, 32, 32).unwrap().is_none());
        }
        // Captured requests keep their original artifact when the selected root changes.
        assert_eq!(first.segmenter().unwrap().unwrap().id(), old.id());
        different.push(0);
        std::fs::write(&first.path, &different).unwrap();
        assert!(!first.is_current());
        assert!(first.segmenter().unwrap().is_none(), "a changed artifact cannot reuse the old plan");
        std::fs::remove_dir_all(a).unwrap();
        std::fs::remove_dir_all(b).unwrap();
    }
}

#[test]
fn a_model_that_fails_to_load_is_reported_not_taken_for_no_model() {
    let dir = temp_models();
    assert!(shared_sky(&dir).unwrap().is_none(), "nothing installed: no model, no error");
    let src = picked(&dir, "sky.onnx", Some((32, 32)), Out::Map);
    let c = add_custom(&dir, Group::Sky, &src, CustomOptions::default(), &keep).unwrap();
    std::fs::write(c.path(&dir), b"this is no longer a model").unwrap();
    let err = shared_sky(&dir).unwrap_err();
    assert!(format!("{err:#}").contains("custom “sky.onnx”"), "{err:#}");
    assert!(snapshot(Group::Sky, &dir).unwrap().segmenter().is_err());
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
    let seg = shared_depth(&dir).unwrap().unwrap();
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
    let err = add_custom(&dir, Group::Subject, &echo, CustomOptions::default(), &keep).unwrap_err();
    assert!(matches!(err.downcast_ref::<CustomModelError>(), Some(CustomModelError::NotSubjectModel)), "{err:#}");
    let err = add_custom(&dir, Group::Depth, &echo, CustomOptions::default(), &keep).unwrap_err();
    assert!(matches!(err.downcast_ref::<CustomModelError>(), Some(CustomModelError::NotDepthModel)), "{err:#}");
    // a sky model needs one map or one per class: this is 3 channels (RGB echoed), so class 5 is out of range
    let err = add_custom(&dir, Group::Sky, &echo, CustomOptions { sky_class: 5, ..Default::default() }, &keep).unwrap_err();
    assert!(matches!(err.downcast_ref::<CustomModelError>(), Some(CustomModelError::SkyClassOutOfRange { class: 5, classes: 3 })), "{err:#}");
    // a sky model must have a channel axis
    let flat = picked(&dir, "flat.onnx", Some((32, 32)), Out::Flat);
    let err = add_custom(&dir, Group::Sky, &flat, CustomOptions::default(), &keep).unwrap_err();
    assert!(matches!(err.downcast_ref::<CustomModelError>(), Some(CustomModelError::NotSkyModelOutput { .. })), "{err:#}");
    // not square, not an image input, not ONNX at all
    let wide = picked(&dir, "wide.onnx", Some((64, 32)), Out::Map);
    let err = add_custom(&dir, Group::Subject, &wide, CustomOptions::default(), &keep).unwrap_err();
    assert!(matches!(err.downcast_ref::<CustomModelError>(), Some(CustomModelError::NonSquareInput { height: 64, width: 32 })), "{err:#}");
    let junk = dir.join("picked/junk.onnx");
    std::fs::write(&junk, b"this is not a model").unwrap();
    assert!(add_custom(&dir, Group::Subject, &junk, CustomOptions::default(), &keep).is_err());
    let txt = dir.join("picked/readme.txt");
    std::fs::write(&txt, b"hi").unwrap();
    let err = add_custom(&dir, Group::Subject, &txt, CustomOptions::default(), &keep).unwrap_err();
    assert!(matches!(err.downcast_ref::<CustomModelError>(), Some(CustomModelError::NotOnnx)), "{err:#}");
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
    assert_eq!(c.activation, Activation::Probability, "the map ends in a sigmoid");
    let seg = shared_sky(&dir).unwrap().unwrap();
    let (w, h) = (30, 20);
    let p = seg.predict_sky(&synthetic(w, h), w, h).unwrap();
    assert_eq!(p.len(), w * h);
    // the model's own sigmoid of the sky colour's mean (IS-Net input), not a second sigmoid on top
    let mean = (90.0 + 150.0 + 230.0) / 255.0 / 3.0 - 0.5;
    assert!((p[2 * w + 2] - 1.0 / (1.0 + (-mean as f32).exp())).abs() < 0.01, "{}", p[2 * w + 2]);
    // ADE20K-style class logits: the sky class is the one the user names
    let many = picked(&dir, "many.onnx", Some((32, 32)), Out::Classes(4));
    let c = add_custom(&dir, Group::Sky, &many, CustomOptions { sky_class: 2, ..Default::default() }, &keep).unwrap();
    assert_eq!((c.classes, c.class), (4, 2));
    assert_eq!(custom(&dir, Group::Sky).unwrap().classes, 4, "replaced, not added");
    assert!(shared_sky(&dir).unwrap().unwrap().predict_sky(&synthetic(w, h), w, h).is_ok());
    let err = add_custom(&dir, Group::Sky, &many, CustomOptions { sky_class: 4, ..Default::default() }, &keep).unwrap_err();
    assert!(matches!(err.downcast_ref::<CustomModelError>(), Some(CustomModelError::SkyClassOutOfRange { class: 4, classes: 4 })), "{err:#}");
    assert_eq!(custom(&dir, Group::Sky).unwrap().class, 2, "a refused model leaves the setting alone");
    let _ = std::fs::remove_dir_all(dir);
}

/// A model (opset 13) that ignores its `s × s` input's values and outputs the constant
/// `[1, channels, s, s]` map `values`, through `last` (`Sigmoid`, or `Softmax` over the channels)
/// when given.
fn constant_onnx(s: u64, channels: u64, values: &[f32], last: Option<&str>) -> Vec<u8> {
    let mut nodes = vec![
        node("ReduceMean", &["x"], "m", &[attr_ints("axes", &[1]), attr_int("keepdims", 1)]),
        node("Mul", &["m", "zero"], "z", &[]),
        node("Add", &["z", "k"], if last.is_some() { "a" } else { "y" }, &[]),
    ];
    if let Some(op) = last {
        nodes.push(node(op, &["a"], "y", &if op == "Softmax" { vec![attr_int("axis", 1)] } else { vec![] }));
    }
    let graph = bytes_of(|g| {
        for n in &nodes {
            field_bytes(1, n, g);
        }
        field_bytes(2, b"constant", g);
        field_bytes(5, &initializer("zero", &[1], &[0.0]), g);
        field_bytes(5, &initializer("k", &[1, channels, s, s], values), g);
        field_bytes(11, &value_info("x", Some(&[Some(1), Some(3), Some(s), Some(s)])), g);
        field_bytes(12, &value_info("y", None), g);
    });
    bytes_of(|m| {
        field_varint(1, 7, m);
        field_bytes(7, &graph, m);
        field_bytes(8, &bytes_of(|o| field_varint(2, 13, o)), m);
    })
}

fn picked_bytes(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join("picked").join(name);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, bytes).unwrap();
    p
}

#[test]
fn a_subject_model_that_outputs_logits_gets_its_sigmoid() {
    let dir = temp_models();
    let s = 32usize;
    // BiRefNet-style logits: a background ramp from -15 to -5 around a subject at +10
    let subject = |i: usize| (8..24).contains(&(i % s)) && (8..24).contains(&(i / s));
    let logits: Vec<f32> = (0..s * s).map(|i| if subject(i) { 10.0 } else { -15.0 + 10.0 * (i % s) as f32 / (s - 1) as f32 }).collect();
    let path = picked_bytes(&dir, "birefnet.onnx", &constant_onnx(s as u64, 1, &logits, None));
    let c = add_custom(&dir, Group::Subject, &path, CustomOptions::default(), &keep).unwrap();
    assert_eq!(c.activation, Activation::Logits);
    assert_eq!(custom(&dir, Group::Subject).unwrap().activation, Activation::Logits, "remembered");
    let p = shared(&dir).unwrap().unwrap().predict(&synthetic(s, s), s, s).unwrap().expect("a clear subject");
    for (i, v) in p.iter().enumerate() {
        if subject(i) {
            assert!(*v >= 0.99, "subject {v} at {i}");
        } else {
            assert!(*v <= 0.01, "background haze {v} at {i}");
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sky_models_are_read_as_the_probabilities_or_logits_they_output() {
    let dir = temp_models();
    let s = 32usize;
    let sky = |i: usize| i / s < s / 2;
    let (w, h) = (s, s);
    let photo = synthetic(w, h);
    let check = |p: Vec<f32>, sky_at_least: f32, ground_at_most: f32| {
        for (i, v) in p.iter().enumerate() {
            if sky(i) {
                assert!(*v >= sky_at_least, "sky {v} at {i}");
            } else {
                assert!(*v <= ground_at_most, "ground {v} at {i}");
            }
        }
    };
    // one map ending in a sigmoid: 2 % sky on the ground stays 2 %
    let ground = (0.02f32 / 0.98).ln();
    let map: Vec<f32> = (0..s * s).map(|i| if sky(i) { 4.0 } else { ground }).collect();
    let path = picked_bytes(&dir, "sky-sigmoid.onnx", &constant_onnx(s as u64, 1, &map, Some("Sigmoid")));
    let c = add_custom(&dir, Group::Sky, &path, CustomOptions::default(), &keep).unwrap();
    assert_eq!((c.classes, c.activation), (1, Activation::Probability));
    check(shared_sky(&dir).unwrap().unwrap().predict_sky(&photo, w, h).unwrap(), 0.95, 0.03);
    // one map of logits (TinySkyNet style)
    let map: Vec<f32> = (0..s * s).map(|i| if sky(i) { 6.0 } else { -6.0 }).collect();
    let path = picked_bytes(&dir, "sky-logits.onnx", &constant_onnx(s as u64, 1, &map, None));
    let c = add_custom(&dir, Group::Sky, &path, CustomOptions::default(), &keep).unwrap();
    assert_eq!((c.classes, c.activation), (1, Activation::Logits));
    check(shared_sky(&dir).unwrap().unwrap().predict_sky(&photo, w, h).unwrap(), 0.99, 0.01);
    // four classes through a softmax, sky = class 2
    let classes: Vec<f32> = (0..4).flat_map(|c| (0..s * s).map(move |i| if c == if sky(i) { 2 } else { 0 } { 8.0 } else { 0.0 })).collect();
    let path = picked_bytes(&dir, "sky-softmax.onnx", &constant_onnx(s as u64, 4, &classes, Some("Softmax")));
    let c = add_custom(&dir, Group::Sky, &path, CustomOptions { sky_class: 2, ..Default::default() }, &keep).unwrap();
    assert_eq!((c.classes, c.class, c.activation), (4, 2, Activation::Probability));
    check(shared_sky(&dir).unwrap().unwrap().predict_sky(&photo, w, h).unwrap(), 0.9, 0.1);
    let _ = std::fs::remove_dir_all(dir);
}

/// The sigmoid of the channel mean (like [`Out::Map`]) with float16 input and output; with
/// `keep_io_types` the graph takes and returns float32 and casts around its float16 body, as
/// transformers.js `model_fp16.onnx` exports do.
fn half_onnx(s: u64, keep_io_types: bool) -> Vec<u8> {
    let cast = |input: &str, output: &str, to: u64| node("Cast", &[input], output, &[attr_int("to", to)]);
    let mut nodes = Vec::new();
    let (x, y) = if keep_io_types { ("xh", "yh") } else { ("x", "y") };
    if keep_io_types {
        nodes.push(cast("x", "xh", 10));
    }
    nodes.push(node("ReduceMean", &[x], "m", &[attr_ints("axes", &[1]), attr_int("keepdims", 1)]));
    nodes.push(node("Sigmoid", &["m"], y, &[]));
    if keep_io_types {
        nodes.push(cast("yh", "y", 1));
    }
    let io = if keep_io_types { 1 } else { 10 };
    let graph = bytes_of(|g| {
        for n in &nodes {
            field_bytes(1, n, g);
        }
        field_bytes(2, b"half", g);
        field_bytes(11, &typed_value_info("x", Some(&[Some(1), Some(3), Some(s), Some(s)]), io), g);
        field_bytes(12, &typed_value_info("y", None, io), g);
    });
    bytes_of(|m| {
        field_varint(1, 7, m);
        field_bytes(7, &graph, m);
        field_bytes(8, &bytes_of(|o| field_varint(2, 13, o)), m);
    })
}

#[test]
fn half_precision_subject_models_run_in_f32() {
    let dir = temp_models();
    for keep_io_types in [false, true] {
        let path = picked_bytes(&dir, &format!("half-{keep_io_types}.onnx"), &half_onnx(32, keep_io_types));
        add_custom(&dir, Group::Subject, &path, CustomOptions::default(), &keep).unwrap_or_else(|e| panic!("keep_io_types {keep_io_types}: {e:#}"));
        let (w, h) = (40, 30);
        let p = shared(&dir).unwrap().unwrap().predict(&synthetic(w, h), w, h).unwrap().expect("a subject");
        assert!(p.len() == w * h && p.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "keep_io_types {keep_io_types}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// [`constant_onnx`]'s map stored in the external file `location` (ONNX external data).
fn external_onnx(s: u64, values: &[f32], location: &str) -> Vec<u8> {
    let entry = |k: &str, v: &str| {
        bytes_of(|e| {
            field_bytes(1, k.as_bytes(), e);
            field_bytes(2, v.as_bytes(), e);
        })
    };
    let k = bytes_of(|t| {
        for d in [1, 1, s, s] {
            field_varint(1, d, t);
        }
        field_varint(2, 1, t);
        field_bytes(8, b"k", t);
        field_bytes(13, &entry("location", location), t);
        field_bytes(13, &entry("offset", "0"), t);
        field_bytes(13, &entry("length", &(values.len() * 4).to_string()), t);
        field_varint(14, 1, t);
    });
    let nodes = [
        node("ReduceMean", &["x"], "m", &[attr_ints("axes", &[1]), attr_int("keepdims", 1)]),
        node("Mul", &["m", "zero"], "z", &[]),
        node("Add", &["z", "k"], "y", &[]),
    ];
    let graph = bytes_of(|g| {
        for n in &nodes {
            field_bytes(1, n, g);
        }
        field_bytes(2, b"external", g);
        field_bytes(5, &initializer("zero", &[1], &[0.0]), g);
        field_bytes(5, &k, g);
        field_bytes(11, &value_info("x", Some(&[Some(1), Some(3), Some(s), Some(s)])), g);
        field_bytes(12, &value_info("y", None), g);
    });
    bytes_of(|m| {
        field_varint(1, 7, m);
        field_bytes(7, &graph, m);
        field_bytes(8, &bytes_of(|o| field_varint(2, 13, o)), m);
    })
}

#[test]
fn self_contained_imports_keep_the_loaded_model_bytes() {
    let dir = temp_models();
    let bytes = constant_onnx(32, 1, &vec![0.5; 32 * 32], None);
    let src = picked_bytes(&dir, "model.onnx", &bytes);
    let (loaded, _) = external_data::inlined(&src).unwrap();
    let retained = picked_bytes(&dir, "retained.onnx", &loaded);
    let c = add_custom(&dir, Group::Subject, &retained, CustomOptions::default(), &keep).unwrap();
    assert_eq!(c.size, 32);
    assert_eq!(std::fs::read(c.path(&dir)).unwrap(), bytes);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(target_os = "linux")]
fn self_contained_models_in_shared_memory_import() {
    let dir = temp_models();
    let source_dir = Path::new("/dev/shm").join(dir.file_name().unwrap());
    std::fs::create_dir(&source_dir).expect("/dev/shm must exist and allow a temporary model folder");
    let bytes = constant_onnx(32, 1, &vec![0.5; 32 * 32], None);
    let src = picked_bytes(&source_dir, "model.onnx", &bytes);
    let result = add_custom(&dir, Group::Subject, &src, CustomOptions::default(), &keep);
    std::fs::remove_dir_all(source_dir).unwrap();
    let c = result.unwrap();
    assert_eq!(c.size, 32);
    assert_eq!(std::fs::read(c.path(&dir)).unwrap(), bytes);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(target_os = "linux")]
fn models_with_external_weights_in_shared_memory_import() {
    use prost::Message;

    let dir = temp_models();
    let source_dir = Path::new("/dev/shm").join(dir.file_name().unwrap());
    std::fs::create_dir(&source_dir).expect("/dev/shm must exist and allow a temporary model folder");
    let values = vec![0.5f32; 32 * 32];
    let weights: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    let src = picked_bytes(&source_dir, "model.onnx", &external_onnx(32, &values, "model.onnx_data"));
    std::fs::write(source_dir.join("picked/model.onnx_data"), &weights).unwrap();
    let result = add_custom(&dir, Group::Subject, &src, CustomOptions::default(), &keep);
    std::fs::remove_dir_all(source_dir).unwrap();
    let c = result.unwrap();
    assert_eq!(c.size, 32);
    let bytes = std::fs::read(c.path(&dir)).unwrap();
    let proto = tract_onnx::pb::ModelProto::decode(bytes.as_slice()).unwrap();
    let tensor = proto.graph.unwrap().initializer.into_iter().find(|t| t.name == "k").unwrap();
    assert_eq!(tensor.raw_data, weights);
    assert!(tensor.external_data.is_empty());
    assert_eq!(tensor.data_location, None);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_model_with_external_weights_is_copied_as_one_file() {
    let dir = temp_models();
    let s = 32usize;
    let values: Vec<f32> = (0..s * s).map(|i| if i % s < s / 2 { 0.9 } else { 0.1 }).collect();
    let src = picked_bytes(&dir, "model.onnx", &external_onnx(s as u64, &values, "model.onnx_data"));
    std::fs::write(dir.join("picked/model.onnx_data"), values.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
    let c = add_custom(&dir, Group::Subject, &src, CustomOptions::default(), &keep).unwrap();
    // the picked folder can go: the copy has the weights
    std::fs::remove_dir_all(dir.join("picked")).unwrap();
    let seg = snapshot(Group::Subject, &dir).unwrap().segmenter().unwrap().expect("the copied model loads");
    let p = seg.predict(&synthetic(s, s), s, s).unwrap().expect("a subject");
    assert!(p[s / 4] > 0.99 && p[s * 3 / 4] < 0.01, "{} {}", p[s / 4], p[s * 3 / 4]);
    assert!(c.path(&dir).is_file() && std::fs::read_dir(custom_dir(&dir)).unwrap().count() == 2, "one model file and the list");
    // weights outside the model's folder, or missing, are refused and nothing is kept
    for location in ["../secret.bin", "/etc/passwd", "missing.bin"] {
        let src = picked_bytes(&dir, "escape.onnx", &external_onnx(s as u64, &values, location));
        let err = add_custom(&dir, Group::Depth, &src, CustomOptions::default(), &keep).unwrap_err();
        assert!(custom(&dir, Group::Depth).is_none(), "{location}: {err:#}");
    }
    #[cfg(unix)]
    {
        let outside = dir.join("picked-outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(outside.join("weights.bin"), dir.join("picked/weights.bin")).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("picked/linked")).unwrap();
        let weights: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        for bytes in [weights.as_slice(), b"x"] {
            std::fs::write(outside.join("weights.bin"), bytes).unwrap();
            for location in ["weights.bin", "linked/weights.bin"] {
                let src = picked_bytes(&dir, "escape.onnx", &external_onnx(s as u64, &values, location));
                let err = add_custom(&dir, Group::Depth, &src, CustomOptions::default(), &keep).unwrap_err();
                assert!(format!("{err:#}").contains("must be inside the model's folder"), "{location}: {err:#}");
                assert!(custom(&dir, Group::Depth).is_none());
            }
        }
    }
    assert_eq!(std::fs::read_dir(custom_dir(&dir)).unwrap().count(), 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[cfg(unix)]
fn hf_snapshot_weights_are_imported_with_the_model() {
    let dir = temp_models();
    let s = 32usize;
    let values: Vec<f32> = (0..s * s).map(|i| if i % s < s / 2 { 0.9 } else { 0.1 }).collect();
    let weights: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    for subfolder in ["", "onnx"] {
        let repo = dir.join("hub/models--org--model");
        let snapshot = repo.join("snapshots/revision").join(subfolder);
        let blobs = repo.join("blobs");
        std::fs::create_dir_all(&snapshot).unwrap();
        std::fs::create_dir_all(&blobs).unwrap();
        std::fs::write(blobs.join("weights"), &weights).unwrap();
        std::fs::write(blobs.join("model"), external_onnx(s as u64, &values, "model.onnx_data")).unwrap();
        let prefix = if subfolder.is_empty() { "../../blobs" } else { "../../../blobs" };
        std::os::unix::fs::symlink(format!("{prefix}/weights"), snapshot.join("model.onnx_data")).unwrap();
        let src = snapshot.join("model.onnx");
        std::os::unix::fs::symlink(format!("{prefix}/model"), &src).unwrap();
        let c = add_custom(&dir, Group::Subject, &src, CustomOptions::default(), &keep).unwrap();
        assert_eq!(c.size, s);
        std::fs::remove_dir_all(repo).unwrap();
        let seg = crate::snapshot(Group::Subject, &dir).unwrap().segmenter().unwrap().unwrap();
        let p = seg.predict(&synthetic(s, s), s, s).unwrap().unwrap();
        assert!(p[s / 4] > 0.99 && p[s * 3 / 4] < 0.01, "{subfolder}: {} {}", p[s / 4], p[s * 3 / 4]);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(unix)]
fn hf_snapshot_weights_cannot_escape_the_repository_blobs() {
    let dir = temp_models();
    let values = vec![0.5f32; 32 * 32];
    for (name, target, link_blobs) in [
        ("models--org--model", "models--other--model/blobs/weights", Some("models--other--model/blobs")),
        ("models--org--model", "models--org--model/blobs-extra/weights", None),
        ("models--org--model", "models--org--model/refs/weights", None),
        ("unrelated", "unrelated/blobs/weights", None),
        ("models--model", "models--model/blobs/weights", None),
    ] {
        let hub = dir.join("hub");
        let snapshot = hub.join(name).join("snapshots/revision/onnx");
        let target = hub.join(target);
        std::fs::create_dir_all(&snapshot).unwrap();
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"x").unwrap();
        if let Some(blobs) = link_blobs {
            std::os::unix::fs::symlink(hub.join(blobs), hub.join(name).join("blobs")).unwrap();
        }
        std::os::unix::fs::symlink(&target, snapshot.join("weights")).unwrap();
        let src = snapshot.join("model.onnx");
        std::fs::write(&src, external_onnx(32, &values, "weights")).unwrap();
        let error = add_custom(&dir, Group::Subject, &src, CustomOptions::default(), &keep).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("must be inside the model's folder"), "{message}");
        assert!(message.contains(&target.canonicalize().unwrap().display().to_string()), "{message}");
        assert!(custom(&dir, Group::Subject).is_none());
        assert!(!custom_dir(&dir).exists());
        std::fs::remove_dir_all(hub).unwrap();
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(unix)]
fn external_weights_links_inside_the_model_folder_are_imported() {
    let dir = temp_models();
    let values = vec![2.0f32; 32 * 32];
    let src = picked_bytes(&dir, "model.onnx", &external_onnx(32, &values, "linked/weights-link"));
    std::fs::create_dir_all(dir.join("picked/data")).unwrap();
    std::fs::write(dir.join("picked/data/weights"), values.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
    std::os::unix::fs::symlink("weights", dir.join("picked/data/weights-link")).unwrap();
    std::os::unix::fs::symlink("data", dir.join("picked/linked")).unwrap();
    std::os::unix::fs::symlink("picked", dir.join("model-folder")).unwrap();
    let c = add_custom(&dir, Group::Depth, &dir.join("model-folder").join(src.file_name().unwrap()), CustomOptions::default(), &keep).unwrap();
    assert_eq!(c.size, 32);
    std::fs::remove_dir_all(dir.join("picked")).unwrap();
    assert_eq!(input_size(&c.path(&dir)).unwrap(), Some(32));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn custom_depth_models_get_their_normalisation_and_output_direction() {
    let dir = temp_models();
    let s = 32usize;
    // metric depth: the left half 2 m away, the right half 10 m
    let metres: Vec<f32> = (0..s * s).map(|i| if i % s < s / 2 { 2.0 } else { 10.0 }).collect();
    let path = picked_bytes(&dir, "metric.onnx", &constant_onnx(s as u64, 1, &metres, None));
    let c = add_custom(&dir, Group::Depth, &path, CustomOptions { depth_metric: true, ..CustomOptions::for_group(Group::Depth) }, &keep).unwrap();
    assert_eq!((c.norm, c.class), (Norm::ImageNet, 1), "depth models start ImageNet-normalised");
    let seg = shared_depth(&dir).unwrap().unwrap();
    // the model gets ImageNet-normalised input: mid grey is (0.5 - mean) / std
    let grey: Vec<u8> = (0..s * s).flat_map(|_| [128, 128, 128, 255]).collect();
    let input = seg.input(&grey, s, s, false).unwrap();
    assert!((input[[0, 0, 0, 0]] - (128.0 / 255.0 - 0.485) / 0.229).abs() < 1e-4, "{}", input[[0, 0, 0, 0]]);
    let near = seg.predict_depth(&grey, s, s).unwrap();
    assert!(near[s / 4] > 0.99 && near[s * 3 / 4] < 0.01, "the 2 m side is the nearer: {} {}", near[s / 4], near[s * 3 / 4]);
    // the same model read as inverse depth puts the 10 m side nearest; switching needs no new import
    let c = set_custom_options(&dir, Group::Depth, CustomOptions { norm: Norm::Half, ..CustomOptions::default() }).unwrap();
    assert_eq!((c.norm, c.class), (Norm::Half, 0));
    assert_eq!(custom(&dir, Group::Depth), Some(c), "the list keeps the DPT style and the output");
    let near = shared_depth(&dir).unwrap().unwrap().predict_depth(&grey, s, s).unwrap();
    assert!(near[s / 4] < 0.01 && near[s * 3 / 4] > 0.99);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn custom_lines_from_before_outputs_were_checked_keep_their_meaning() {
    let dir = temp_models();
    let src = picked(&dir, "m.onnx", Some((32, 32)), Out::Map);
    let subject = add_custom(&dir, Group::Subject, &src, CustomOptions::default(), &keep).unwrap();
    let sky = add_custom(&dir, Group::Sky, &src, CustomOptions::default(), &keep).unwrap();
    std::fs::write(custom_list(&dir), format!("subject\t{}\tS\t32\tisnet\t1\t0\nsky\t{}\tK\t32\timagenet\t1\t0\n", subject.file, sky.file)).unwrap();
    assert_eq!(custom(&dir, Group::Subject).unwrap().activation, Activation::Probability);
    let sky = custom(&dir, Group::Sky).unwrap();
    assert_eq!((sky.activation, sky.norm), (Activation::Logits, Norm::ImageNet));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn ds_finding_custom_sky_class_is_validated_on_reload() {
    let dir = temp_models();
    let model = picked(&dir, "many.onnx", Some((32, 32)), Out::Classes(4));
    let c = add_custom(&dir, Group::Sky, &model, CustomOptions { sky_class: 2, ..Default::default() }, &keep).unwrap();
    std::fs::write(custom_list(&dir), format!("sky\t{}\tSky\t32\tisnet\t4\t4\n", c.file)).unwrap();
    assert!(custom(&dir, Group::Sky).is_none());
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
    let spec = spec("rmbg-1.4").unwrap();
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
    // landscape and portrait, each seen in its own aspect ratio
    for (w, h) in [(384, 288), (288, 384)] {
        assert_eq!(seg.input_size(w, h), depth_input(518, w, h));
        let img = ground_scene(w, h);
        let d = seg.predict_depth(&img, w, h).unwrap();
        assert_eq!(d.len(), w * h);
        assert!(d.iter().all(|v| (0.0..=1.0).contains(v)));
        let at = |x: usize, y: usize| d[y * w + x];
        let (near, far) = (at(w / 8, h - 6), at(w / 8, (h as f32 * 0.42) as usize));
        assert!(near > far + 0.2, "{w}x{h}: near ground {near}, horizon {far}");
        assert!(at(w / 2, h * 3 / 4) > far, "{w}x{h}: the box is nearer than the horizon");
    }
}

#[test]
fn flexible_depth_models_keep_the_photos_aspect_ratio() {
    assert_eq!(depth_input(518, 1536, 1024), (518, 784));
    assert_eq!(depth_input(518, 1024, 1536), (784, 518));
    assert_eq!(depth_input(518, 4000, 3000), (518, 686));
    assert_eq!(depth_input(518, 800, 800), (518, 518));
    assert_eq!(depth_input(518, 9000, 1000), (518, 1036), "a panorama is capped at 2:1");
    // the official depth model's flexible input: planned for a shape when a photo of it comes,
    // a fixed one stays square
    let dir = temp_models();
    let depth = ModelSpec { size: 28, ..*Group::Depth.official() };
    let flexible = Segmenter::load(&depth, &picked(&dir, "flex.onnx", None, Out::Map)).unwrap();
    let fixed = Segmenter::load(&depth, &picked(&dir, "fixed.onnx", Some((28, 28)), Out::Map)).unwrap();
    let planned = || flexible.flexible.as_ref().unwrap().built.lock().unwrap().iter().map(|(at, _)| *at).collect::<Vec<_>>();
    assert_eq!(planned(), [], "nothing is planned before a photo comes");
    let (w, h) = (90, 60);
    assert_eq!((flexible.input_size(w, h), fixed.input_size(w, h)), ((28, 42), (28, 28)));
    for seg in [&flexible, &fixed] {
        let d = seg.predict_depth(&synthetic(w, h), w, h).unwrap();
        assert_eq!(d.len(), w * h);
    }
    flexible.predict_depth(&synthetic(h, w), h, w).unwrap();
    flexible.predict_depth(&synthetic(w, h), w, h).unwrap();
    assert_eq!(planned(), [(28, 42), (42, 28)], "one plan per shape, the most recently used first");
    let custom = add_custom(&dir, Group::Depth, &picked(&dir, "custom.onnx", None, Out::Map), CustomOptions::for_group(Group::Depth), &keep).unwrap();
    assert_eq!(shared_depth(&dir).unwrap().unwrap().input_size(w, h), (custom.size, custom.size), "a custom model's patch size is unknown: square");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn companions_install_and_remove_as_one_bundle() {
    let dir = temp_models();
    let mut bundle = *spec("clip-b32-laion").unwrap();
    bundle.bytes = 2;
    bundle.sha256 = "";
    bundle.companions = &[Companion { file: "tiny-text", url: "https://huggingface.co/tiny", bytes: 3, sha256: "" }];
    std::fs::write(model_path(&dir, &bundle), b"xx").unwrap();
    assert_eq!(installed_bytes(&dir, &bundle), None);
    std::fs::write(companion_path(&dir, "tiny-text"), b"x").unwrap();
    assert_eq!(installed_bytes(&dir, &bundle), None, "partial companion");
    std::fs::write(companion_path(&dir, "tiny-text"), b"xxx").unwrap();
    assert_eq!(installed_bytes(&dir, &bundle), Some(5));
    assert_eq!(bundle.download_bytes(), 5);
    assert_eq!(remove(&dir, &bundle).unwrap(), 5);
    assert!(!model_path(&dir, &bundle).exists());
    assert!(!companion_path(&dir, "tiny-text").exists());
    std::fs::write(model_path(&dir, &bundle), b"x").unwrap();
    assert_eq!(remove(&dir, &bundle).unwrap(), 1, "remove partial bundle too");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn wrong_bundle_hash_is_not_installed() {
    let dir = temp_models();
    let mut bundle = *spec("clip-b32-laion").unwrap();
    bundle.bytes = 2;
    bundle.sha256 = "incorrect";
    bundle.companions = &[Companion { file: "tiny-vocab", url: "", bytes: 1, sha256: "" }];
    std::fs::write(model_path(&dir, &bundle), b"xx").unwrap();
    std::fs::write(companion_path(&dir, "tiny-vocab"), b"x").unwrap();
    assert_eq!(installed_bytes(&dir, &bundle), None);
    // Invalid bundles can still be removed in full.
    assert_eq!(remove(&dir, &bundle).unwrap(), 3);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn tagging_requires_its_bundle_and_refuses_custom_models() {
    let dir = temp_models();
    let source = picked(&dir, "single.onnx", Some((32, 32)), Out::Map);
    let err = add_custom(&dir, Group::Tagging, &source, CustomOptions::default(), &keep).unwrap_err();
    assert_eq!(err.downcast_ref::<CustomModelError>(), Some(&CustomModelError::UnsupportedFunction));
    assert!(!custom_dir(&dir).exists(), "nothing is copied for tagging");
    std::fs::write(model_path(&dir, Group::Tagging.official()), b"partial bundle").unwrap();
    assert!(Group::Tagging.in_use(&dir).is_none());
    assert!(!available(Group::Tagging, &dir));
    // A hand-edited custom list cannot activate a single file in place of CLIP's bundle.
    std::fs::create_dir_all(custom_dir(&dir)).unwrap();
    std::fs::copy(&source, custom_dir(&dir).join("single.onnx")).unwrap();
    std::fs::write(custom_list(&dir), "tagging\tsingle.onnx\tCustom CLIP\t32\tisnet\t1\t0\n").unwrap();
    assert!(custom(&dir, Group::Tagging).is_none());
    assert!(Group::Tagging.in_use(&dir).is_none());
    assert!(Segmenter::load(Group::Tagging.official(), &source).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn faces_official_bundle_contains_both_pinned_models_and_no_custom_flow() {
    let spec = Group::Faces.official();
    assert_eq!(spec.id, "sface");
    assert_eq!(spec.task, Task::FaceEmbed);
    assert!(!Group::Faces.supports_custom());
    let files: Vec<_> = spec.files().collect();
    assert_eq!(files.len(), 2);
    assert_eq!(spec.download_bytes(), faces::SFACE.bytes + faces::YUNET.bytes);
    for (id, pinned) in [("yunet", faces::YUNET), ("sface", faces::SFACE)] {
        let m = crate::spec(id).unwrap();
        assert_eq!(m.group, Some(Group::Faces));
        assert_eq!(m.file, pinned.file);
        assert_eq!(m.bytes, pinned.bytes);
        assert_eq!(m.sha256, pinned.sha256);
        assert_eq!(m.url, pinned.url);
    }
    let dir = temp_models();
    std::fs::create_dir_all(dir.join("segmentation")).unwrap();
    std::fs::write(model_path(&dir, spec), b"partial").unwrap();
    assert!(installed_bytes(&dir, spec).is_none());
    assert!(Group::Faces.in_use(&dir).is_none());
    std::fs::write(companion_path(&dir, faces::YUNET.file), b"wrong").unwrap();
    assert!(installed_bytes(&dir, spec).is_none());
    let removed = remove_with(&dir, spec, &|p| std::fs::remove_file(p)).unwrap();
    assert_eq!(removed, 12);
    assert!(!model_path(&dir, spec).exists());
    assert!(!companion_path(&dir, faces::YUNET.file).exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ds_finding_failed_custom_publication_keeps_old_registration_and_file() {
    let dir = temp_models();
    let source = picked(&dir, "a.onnx", Some((32, 32)), Out::Map);
    let old = add_custom(&dir, Group::Subject, &source, CustomOptions::default(), &keep).unwrap();
    let bytes = std::fs::read(old.path(&dir)).unwrap();
    let registry = std::fs::read(custom_list(&dir)).unwrap();
    for src in [&source, &picked(&dir, "b.onnx", Some((32, 32)), Out::Map)] {
        assert!(
            add_custom_with(
                &dir,
                Group::Subject,
                src,
                CustomOptions { norm: Norm::ImageNet, ..Default::default() },
                &|_| panic!("old file disposed before publication"),
                &|_, _| Err(anyhow::anyhow!("disk full"))
            )
            .is_err()
        );
        assert_eq!(std::fs::read(old.path(&dir)).unwrap(), bytes);
        assert_eq!(std::fs::read(custom_list(&dir)).unwrap(), registry);
        assert_eq!(custom(&dir, Group::Subject), Some(old.clone()));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ai_denoise_custom_model_and_use_default() {
    let dir = temp_models();
    let path = picked(&dir, "restore.onnx", Some((32, 32)), Out::Echo);
    for group in [Group::RawDenoise, Group::RgbDenoise] {
        let c = add_custom(&dir, group, &path, CustomOptions::default(), &keep).unwrap();
        assert_eq!(c.size, 32);
        assert_eq!(c.classes, 3);
        assert_eq!(c.class, usize::from(group == Group::RawDenoise));
        assert_eq!(group.in_use(&dir), Some(InUse::Custom(c)));
        clear_custom(&dir, group, &|p| std::fs::remove_file(p)).unwrap();
        assert!(custom(&dir, group).is_none());
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// A residual restoration graph (`y = x + x`): tract runs it, the GPU denoiser has no `Add`.
fn residual_onnx(s: u64) -> Vec<u8> {
    let graph = bytes_of(|g| {
        field_bytes(1, &node("Add", &["x", "x"], "y", &[]), g);
        field_bytes(2, b"residual", g);
        field_bytes(11, &value_info("x", Some(&[Some(1), Some(3), Some(s), Some(s)])), g);
        field_bytes(12, &value_info("y", None), g);
    });
    bytes_of(|m| {
        field_varint(1, 7, m);
        field_bytes(7, &graph, m);
        field_bytes(8, &bytes_of(|o| field_varint(2, 13, o)), m);
    })
}

#[test]
fn custom_denoisers_are_checked_against_the_gpu_graph_that_runs_them() {
    let dir = temp_models();
    let residual = picked_bytes(&dir, "residual.onnx", &residual_onnx(32));
    for group in [Group::RawDenoise, Group::RgbDenoise] {
        let err = add_custom(&dir, group, &residual, CustomOptions::default(), &keep).unwrap_err();
        match err.downcast_ref::<CustomModelError>() {
            Some(CustomModelError::UnsupportedRestoration { reason }) => assert!(reason.contains("Add"), "{reason}"),
            _ => panic!("{err:#}"),
        }
        assert!(custom(&dir, group).is_none(), "refused at import, not at Apply");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn bg_tier_tiny_smoke_preserves_probabilities_and_no_subject() {
    let dir = temp_models();
    let path = picked(&dir, "sigmoid.onnx", Some((8, 8)), Out::Map);
    for id in ["rmbg-1.4", "inspyrenet"] {
        let model = ModelSpec { size: 8, ..*spec(id).unwrap() };
        let seg = Segmenter::load(&model, &path).unwrap();
        let bright = vec![255u8; 4 * 4 * 4];
        let p = seg.predict(&bright, 4, 4).unwrap().expect("bright subject");
        assert!(p.iter().all(|v| *v > 0.5 && *v < 1.0), "constant probabilities must not become zero via min–max");
        let dark: Vec<_> = (0..16).flat_map(|_| [0, 0, 0, 255]).collect();
        assert!(seg.predict(&dark, 4, 4).unwrap().is_none());
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn bg_tier_input_uses_255_not_image_peak() {
    let dir = temp_models();
    let path = picked(&dir, "sigmoid.onnx", Some((8, 8)), Out::Map);
    let model = ModelSpec { size: 8, ..*spec("rmbg-1.4").unwrap() };
    let seg = Segmenter::load(&model, &path).unwrap();
    let rgba: Vec<_> = (0..16).flat_map(|_| [64, 64, 64, 255]).collect();
    assert!(seg.predict(&rgba, 4, 4).unwrap().is_none(), "dark image must not be peak-scaled to white");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "opt-in real ONNX compatibility, LI_SEG_TEST_MODELS"]
fn bg_real_model_compatibility() {
    let dir = PathBuf::from(std::env::var_os("LI_SEG_TEST_MODELS").expect("LI_SEG_TEST_MODELS"));
    for (file, size, isnet, task) in [
        ("isnet-general-use.onnx", 1024, true, Task::Subject),
        ("u2netp.onnx", 320, false, Task::Subject),
        ("modnet.onnx", 512, false, Task::Subject),
        ("rmbg-1.4.onnx", 1024, true, Task::Subject),
        ("inspyrenet-swinb-plus-ultra.onnx", 1024, false, Task::Subject),
        ("pp_mobileseg_base_ade20k_512.onnx", 512, false, spec("sky-mobileseg").unwrap().task),
        ("depth_anything_v2_vits_dynamic.onnx", 518, false, Task::Depth),
    ] {
        let model = ModelSpec { id: file, file, size, isnet, task, ..*spec("rmbg-1.4").unwrap() };
        Segmenter::load(&model, &dir.join(file)).unwrap_or_else(|e| panic!("{file}: {e:#}"));
    }
}

#[test]
fn bg_subject_group_uses_chosen_tier_for_all_callers() {
    let dir = temp_models();
    for id in ["rmbg-1.4", "inspyrenet"] {
        let model = spec(id).unwrap();
        std::fs::write(model_path(&dir, model), b"installed").unwrap();
    }
    assert_eq!(Group::Subject.in_use(&dir), Some(InUse::Official(spec("rmbg-1.4").unwrap())));
    set_subject_quality(&dir, true);
    assert_eq!(Group::Subject.in_use(&dir), Some(InUse::Official(spec("inspyrenet").unwrap())));
    set_subject_quality(&dir, false);
    assert_eq!(Group::Subject.in_use(&dir), Some(InUse::Official(spec("rmbg-1.4").unwrap())));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_subject_function_uses_whichever_tier_is_installed() {
    let (fast, better) = (spec("rmbg-1.4").unwrap(), spec("inspyrenet").unwrap());
    for (installed, chosen) in [(better, false), (fast, true)] {
        let dir = temp_models();
        std::fs::write(model_path(&dir, installed), b"installed").unwrap();
        set_subject_quality(&dir, chosen);
        assert_eq!(Group::Subject.in_use(&dir), Some(InUse::Official(installed)), "chosen better: {chosen}");
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn the_subject_tier_choice_is_kept_in_the_models_folder() {
    let dir = temp_models();
    for model in ["rmbg-1.4", "inspyrenet"] {
        std::fs::write(model_path(&dir, spec(model).unwrap()), b"installed").unwrap();
    }
    set_subject_quality(&dir, true);
    assert_eq!(std::fs::read_to_string(dir.join("segmentation/subject-tier.txt")).unwrap(), "better");
    // a later run (nothing remembered in memory) reads it back
    SUBJECT_QUALITY.lock().unwrap().retain(|(d, _)| *d != dir);
    assert!(subject_quality(&dir));
    assert_eq!(Group::Subject.in_use(&dir), Some(InUse::Official(spec("inspyrenet").unwrap())));
    let _ = std::fs::remove_dir_all(dir);
}

// threading: shared pool and cancel

/// An ONNX `TensorProto` (float) initializer.
fn initializer(name: &str, dims: &[u64], data: &[f32]) -> Vec<u8> {
    bytes_of(|t| {
        for d in dims {
            field_varint(1, *d, t);
        }
        field_varint(2, 1, t);
        field_bytes(8, name.as_bytes(), t);
        field_bytes(9, &data.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>(), t);
    })
}

/// A small conv net (opset 13): `layers` 3×3 convolutions of `channels` channels with ReLU, then
/// the sigmoid of the channel mean as a `[1, 1, s, s]` map. Positive weights, so the map is a
/// clear subject (> 0.5). Big enough for tract's matrix kernels to split across threads.
fn conv_onnx(s: u64, channels: u64, layers: usize) -> Vec<u8> {
    let mut nodes = Vec::new();
    let mut inits = Vec::new();
    let mut prev = "x".to_string();
    for l in 0..layers {
        let cin = if l == 0 { 3 } else { channels };
        let n = (channels * cin * 9) as usize;
        let scale = 1.0 / (cin * 9) as f32;
        let w: Vec<f32> = (0..n).map(|i| ((i as u64 * 2_654_435_761 + l as u64) % 997) as f32 / 997.0 * scale).collect();
        inits.push(initializer(&format!("w{l}"), &[channels, cin, 3, 3], &w));
        nodes.push(node("Conv", &[&prev, &format!("w{l}")], &format!("c{l}"), &[attr_ints("kernel_shape", &[3, 3]), attr_ints("pads", &[1, 1, 1, 1])]));
        nodes.push(node("Relu", &[&format!("c{l}")], &format!("r{l}"), &[]));
        prev = format!("r{l}");
    }
    nodes.push(node("ReduceMean", &[&prev], "m", &[attr_ints("axes", &[1]), attr_int("keepdims", 1)]));
    nodes.push(node("Sigmoid", &["m"], "y", &[]));
    let graph = bytes_of(|g| {
        for n in &nodes {
            field_bytes(1, n, g);
        }
        field_bytes(2, b"conv", g);
        for i in &inits {
            field_bytes(5, i, g);
        }
        field_bytes(11, &value_info("x", Some(&[Some(1), Some(3), Some(s), Some(s)])), g);
        field_bytes(12, &value_info("y", None), g);
    });
    bytes_of(|m| {
        field_varint(1, 7, m);
        field_bytes(7, &graph, m);
        field_bytes(8, &bytes_of(|o| field_varint(2, 13, o)), m);
    })
}

/// A Fast-tier style segmenter (probability output) over the conv net.
fn conv_segmenter(dir: &Path, s: u64, channels: u64, layers: usize) -> Segmenter {
    let p = dir.join("picked").join(format!("conv-{s}-{channels}-{layers}.onnx"));
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, conv_onnx(s, channels, layers)).unwrap();
    let model = ModelSpec { size: s as usize, ..*spec("rmbg-1.4").unwrap() };
    Segmenter::load(&model, &p).unwrap()
}

fn test_photo(w: usize, h: usize) -> Vec<u8> {
    (0..w * h).flat_map(|i| [(i * 7 % 251) as u8, (i * 13 % 241) as u8, (i * 29 % 239) as u8, 255]).collect()
}

/// Tract on the shared pool gives the same bits on 1 thread and on N threads
/// (its kernels split over output panels, never over the reduction).
#[test]
fn tract_output_is_identical_on_one_and_many_threads() {
    init_executor();
    let dir = temp_models();
    let seg = conv_segmenter(&dir, 64, 32, 3);
    let (w, h) = (96, 80);
    let photo = test_photo(w, h);
    let on = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        pool.install(|| seg.predict(&photo, w, h)).unwrap().expect("a clear subject")
    };
    let bits = |v: Vec<f32>| v.into_iter().map(f32::to_bits).collect::<Vec<u32>>();
    let one = bits(on(1));
    assert_eq!(one.len(), w * h);
    for n in [2, 4, 7] {
        assert_eq!(one, bits(on(n)), "{n} threads");
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// Cancel is checked before every model stage: the run stops at the stage where the flag goes up,
/// with an error `is_cancelled` recognises, and does no further work.
#[test]
fn cancel_stops_predict_at_the_next_model_stage() {
    init_executor();
    let dir = temp_models();
    let seg = conv_segmenter(&dir, 32, 8, 6);
    let photo = test_photo(40, 40);
    let checks = std::sync::atomic::AtomicUsize::new(0);
    let count = || {
        checks.fetch_add(1, Ordering::SeqCst);
        false
    };
    assert!(seg.predict_checked(&photo, 40, 40, &count).unwrap().is_some());
    let full = checks.swap(0, Ordering::SeqCst);
    assert!(full > 8, "one check per stage plus before and after: {full}");
    let stop_at_4th = || checks.fetch_add(1, Ordering::SeqCst) + 1 >= 4;
    let err = seg.predict_checked(&photo, 40, 40, &stop_at_4th).unwrap_err();
    assert!(is_cancelled(&err), "{err:#}");
    assert_eq!(checks.load(Ordering::SeqCst), 4, "no stage ran after the cancel");
    // The public API: a flag that is already set runs nothing, for subject and sky alike.
    let cancelled = AtomicBool::new(true);
    assert!(is_cancelled(&seg.predict_with_cancel(&photo, 40, 40, &cancelled).unwrap_err()));
    let fresh = AtomicBool::new(false);
    assert!(seg.predict_with_cancel(&photo, 40, 40, &fresh).unwrap().is_some());
    let sky_path = picked(&dir, "sky.onnx", Some((32, 32)), Out::Map);
    let sky = Segmenter::load(&ModelSpec { size: 32, ..*spec("sky-mobileseg").unwrap() }, &sky_path);
    if let Ok(sky) = sky
        && sky.predict_sky(&photo, 40, 40).is_ok()
    {
        assert!(is_cancelled(&sky.predict_sky_with_cancel(&photo, 40, 40, &cancelled).unwrap_err()));
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// A cancel from another thread ends a long run well before it would have finished.
#[test]
fn cancel_from_another_thread_ends_a_long_predict_promptly() {
    init_executor();
    let dir = temp_models();
    let seg = conv_segmenter(&dir, 192, 48, 12);
    let (w, h) = (192, 192);
    let photo = test_photo(w, h);
    let t = std::time::Instant::now();
    assert!(seg.predict(&photo, w, h).unwrap().is_some());
    let full = t.elapsed();
    let flag = AtomicBool::new(false);
    let t = std::time::Instant::now();
    let r = std::thread::scope(|sc| {
        let run = sc.spawn(|| seg.predict_with_cancel(&photo, w, h, &flag));
        std::thread::sleep(full / 5);
        flag.store(true, Ordering::Relaxed);
        run.join().unwrap()
    });
    let cancelled_after = t.elapsed();
    assert!(is_cancelled(&r.unwrap_err()));
    // One stage of twelve plus scheduling noise; generous for a loaded machine.
    assert!(cancelled_after < full.mul_f32(0.8) || full < std::time::Duration::from_millis(30), "cancelled after {cancelled_after:?}, full run {full:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn shared_matte_entrypoint_needs_no_model() {
    let mask = vec![0.2; 24];
    let params = li_matte::RefineParams::default();
    assert_eq!(li_matte::matting::refine_buffer(None, &mask, 6, 4, &params), mask);
    assert_eq!(li_matte::refine::MAX_SOLVE_EDGE, 1024);
}
