use super::*;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

fn rgb(pixels: &[u8], width: usize, height: usize) -> FaceImage<'_> {
    FaceImage { pixels: RgbPixels::U8(pixels), width, height }
}

fn approx(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-3, "{a} != {b}");
}

fn blank_outputs(w: usize, h: usize) -> YunetOutputs {
    let mut out = YunetOutputs::default();
    for (i, stride) in [8, 16, 32].into_iter().enumerate() {
        let n = (w / stride) * (h / stride);
        out.cls[i] = vec![0.0; n];
        out.obj[i] = vec![0.0; n];
        out.bbox[i] = vec![0.0; 4 * n];
        out.kps[i] = vec![0.0; 10 * n];
    }
    out
}

#[test]
fn yunet_decodes_the_spec_cell_and_landmarks_at_all_strides() {
    for (level, stride) in [8, 16, 32].into_iter().enumerate() {
        let mut out = blank_outputs(128, 128);
        let idx = 2 * (128 / stride) + 3;
        out.cls[level][idx] = 0.81;
        out.obj[level][idx] = 1.0;
        out.bbox[level][4 * idx..4 * idx + 4].copy_from_slice(&[0.5, 0.5, 2.0f32.ln(), 2.0f32.ln()]);
        for n in 0..5 {
            out.kps[level][10 * idx + 2 * n] = n as f32 / 10.0;
            out.kps[level][10 * idx + 2 * n + 1] = n as f32 / 5.0;
        }
        let faces = decode_yunet(&out, 128, 128, 0.9).unwrap();
        assert_eq!(faces.len(), 1);
        let face = &faces[0];
        approx(face.x + face.w / 2.0, 3.5 * stride as f32);
        approx(face.y + face.h / 2.0, 2.5 * stride as f32);
        approx(face.w, 2.0 * stride as f32);
        approx(face.h, 2.0 * stride as f32);
        approx(face.score, 0.9);
        for n in 0..5 {
            approx(face.landmarks[n][0], (3.0 + n as f32 / 10.0) * stride as f32);
            approx(face.landmarks[n][1], (2.0 + n as f32 / 5.0) * stride as f32);
        }
    }
}

#[test]
fn yunet_clamps_probabilities_and_handles_bad_outputs_without_panics() {
    let mut out = blank_outputs(32, 32);
    out.cls[0][0] = 2.0;
    out.obj[0][0] = 1.0;
    assert_eq!(decode_yunet(&out, 32, 32, 0.9).unwrap().len(), 1);
    out.obj[0][0] = -1.0;
    assert!(decode_yunet(&out, 32, 32, 0.9).unwrap().is_empty());
    out.obj[0][0] = f32::NAN;
    assert!(decode_yunet(&out, 32, 32, 0.9).unwrap().is_empty());
    out.obj[0][0] = 1.0;
    out.bbox[0][2] = 1000.0;
    assert!(decode_yunet(&out, 32, 32, 0.9).unwrap().is_empty());
    out.bbox[0][2] = 0.0;
    out.kps[0][0] = f32::INFINITY;
    assert!(decode_yunet(&out, 32, 32, 0.9).unwrap().is_empty());
    out.cls[1].clear();
    assert!(decode_yunet(&out, 32, 32, 0.9).is_err());
    assert!(decode_yunet(&out, 31, 32, 0.9).is_err());
    assert!(decode_yunet(&out, 32, 32, f32::NAN).is_err());
}

fn bbox(x: f32, score: f32) -> FaceBox {
    FaceBox { x, y: 0.0, w: 90.0, h: 90.0, score, landmarks: REFERENCE_LANDMARKS }
}

#[test]
fn nms_suppresses_point_eight_overlap_keeps_disjoint_and_is_stable() {
    let a = bbox(0.0, 0.91);
    let b = bbox(10.0, 0.95);
    let c = bbox(200.0, 0.92);
    approx(intersection_over_union(&a, &b), 0.8);
    assert_eq!(nms(vec![a, b.clone(), c.clone()], 0.3, 5000), vec![b, c]);
    let a = bbox(0.0, 0.95);
    let b = bbox(10.0, 0.95);
    assert_eq!(nms(vec![a.clone(), b], 0.3, 5000), vec![a]);
    assert_eq!(nms(vec![bbox(0.0, 0.95), bbox(10.0, 0.94), bbox(200.0, 0.93)], 0.3, 2).len(), 1, "top-k applies before NMS");
    assert_eq!(nms(vec![bbox(0.0, 0.95), bbox(200.0, 0.94)], 0.3, 0).len(), 2, "zero means unlimited");
    assert!(nms(vec![bbox(0.0, f32::NAN)], 0.3, 5000).is_empty());
    assert_eq!(nms(vec![bbox(0.0, 0.95), bbox(10.0, 0.94)], 0.8, 5000).len(), 2, "IoU equality survives");
}

fn transform(m: &Affine, p: [f32; 2]) -> [f32; 2] {
    std::array::from_fn(|c| m[c][0] * p[0] + m[c][1] * p[1] + m[c][2])
}

#[test]
fn similarity_is_identity_and_undoes_scale_rotation_translation() {
    let identity = similarity_transform(&REFERENCE_LANDMARKS).unwrap();
    assert_eq!(identity, [[1.0, -0.0, 0.0], [0.0, 1.0, 0.0]]);
    for angle in [-2.3f32, -0.7, 0.0, 1.4] {
        for scale in [0.4, 1.0, 3.5] {
            let src =
                REFERENCE_LANDMARKS.map(|[x, y]| [scale * (x * angle.cos() - y * angle.sin()) + 120.0, scale * (x * angle.sin() + y * angle.cos()) - 70.0]);
            let m = similarity_transform(&src).unwrap();
            for (p, target) in src.into_iter().zip(REFERENCE_LANDMARKS) {
                let actual = transform(&m, p);
                approx(actual[0], target[0]);
                approx(actual[1], target[1]);
            }
            assert!(m[0][0] * m[1][1] - m[0][1] * m[1][0] > 0.0);
        }
    }
}

#[test]
fn similarity_is_least_squares_without_reflection_and_rejects_degeneracy() {
    let src = REFERENCE_LANDMARKS.map(|[x, y]| [-x, y]);
    let m = similarity_transform(&src).unwrap();
    assert!(m[0][0] * m[1][1] - m[0][1] * m[1][0] > 0.0);
    // Normal equations: residuals sum to zero and are orthogonal to rotation/scale terms.
    let noisy = std::array::from_fn(|i| [REFERENCE_LANDMARKS[i][0] * 1.7 + i as f32, REFERENCE_LANDMARKS[i][1] * 1.7 - (i * i) as f32]);
    let m = similarity_transform(&noisy).unwrap();
    let mut residual = [0.0f32; 4];
    for (p, target) in noisy.into_iter().zip(REFERENCE_LANDMARKS) {
        let predicted = transform(&m, p);
        let dx = predicted[0] - target[0];
        let dy = predicted[1] - target[1];
        residual[0] += dx;
        residual[1] += dy;
        residual[2] += dx * p[0] + dy * p[1];
        residual[3] += -dx * p[1] + dy * p[0];
    }
    assert!(residual.iter().all(|r| r.abs() < 0.01), "{residual:?}");
    assert!(similarity_transform(&[[1.0; 2]; 5]).is_err());
    assert!(similarity_transform(&[[f32::NAN; 2]; 5]).is_err());
}

#[test]
fn warp_constant_rgb_is_constant_and_uses_bilinear_black_borders() {
    let data = [31, 82, 173].repeat(112 * 112);
    let m = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let out = warp_affine_112(rgb(&data, 112, 112), &m).unwrap();
    assert!(out.as_chunks::<3>().0.iter().all(|p| *p == [31.0, 82.0, 173.0]));
    let data = [0, 0, 0, 100, 0, 0, 0, 100, 0, 100, 100, 0];
    let m = [[1.0, 0.0, -0.5], [0.0, 1.0, -0.5]];
    let out = warp_affine_112(rgb(&data, 2, 2), &m).unwrap();
    assert_eq!(&out[..3], &[50.0, 50.0, 0.0]);
    assert_eq!(&out[3..6], &[50.0, 25.0, 0.0]);
    assert_eq!(&out[6..9], &[0.0, 0.0, 0.0]);
    assert!(warp_affine_112(rgb(&data, 2, 2), &[[0.0; 3]; 2]).is_err());
}

#[test]
fn letterbox_round_trips_coordinates_at_different_orientations() {
    for (w, h, expected) in
        [(6000, 4000, (1024, 683, 1024, 704)), (4000, 6000, (683, 1024, 704, 1024)), (112, 112, (1024, 1024, 1024, 1024)), (100000, 1, (1024, 1, 1024, 32))]
    {
        let l = Letterbox::new(w, h).unwrap();
        assert_eq!((l.resized_w, l.resized_h, l.pad_w, l.pad_h), expected);
        for p in [[0.0, 0.0], [w as f32 / 3.0, h as f32 / 4.0], [w as f32, h as f32]] {
            let back = l.to_original(l.to_input(p));
            assert!((back[0] - p[0]).abs() < 0.01 && (back[1] - p[1]).abs() < 0.01);
        }
    }
    assert!(Letterbox::new(0, 2).is_err());
}

#[test]
fn letterbox_bgr_zero_padding_and_three_sample_formats_agree() {
    let a = [17u8, 127, 250].repeat(3 * 2);
    let b: Vec<u16> = a.iter().map(|v| u16::from(*v) * 257).collect();
    let c: Vec<f32> = a.iter().map(|v| f32::from(*v) / 255.0).collect();
    let (tensor, l) = letterbox(rgb(&a, 3, 2)).unwrap();
    let plane = l.pad_w * l.pad_h;
    assert_eq!([tensor[0], tensor[plane], tensor[2 * plane]], [250.0, 127.0, 17.0]);
    assert!(tensor[l.resized_h * l.pad_w..plane].iter().all(|v| *v == 0.0));
    for pixels in [RgbPixels::U16(&b), RgbPixels::F32(&c)] {
        let (other, _) = letterbox(FaceImage { pixels, width: 3, height: 2 }).unwrap();
        assert!(tensor.iter().zip(other).all(|(a, b)| (a - b).abs() < 1e-4));
    }
    let m = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let expected = warp_affine_112(rgb(&a, 3, 2), &m).unwrap();
    let other = warp_affine_112(FaceImage { pixels: RgbPixels::U16(&b), width: 3, height: 2 }, &m).unwrap();
    assert!(expected.iter().zip(other).all(|(a, b)| (a - b).abs() < 1e-4));
}

#[test]
fn image_validation_rejects_malformed_and_nonfinite_input() {
    assert!(letterbox(rgb(&[], 0, 0)).is_err());
    assert!(letterbox(rgb(&[0; 2], 1, 1)).is_err());
    assert!(letterbox(rgb(&[], usize::MAX, 2)).is_err());
    assert!(letterbox(FaceImage { pixels: RgbPixels::F32(&[f32::NAN; 3]), width: 1, height: 1 }).is_err());
    let float = FaceImage { pixels: RgbPixels::F32(&[-2.0, 0.5, 4.0]), width: 1, height: 1 };
    let (out, l) = letterbox(float).unwrap();
    let plane = l.pad_w * l.pad_h;
    assert_eq!([out[0], out[plane], out[2 * plane]], [255.0, 127.5, 0.0]);
}

fn key(date: i64, text: &str, index: usize) -> NodeKey {
    NodeKey { photo_date: date, key: text.to_owned(), face_index: index }
}

fn vector(cos: f32) -> [f32; 128] {
    let mut out = [0.0; 128];
    out[0] = cos;
    out[1] = (1.0 - cos * cos).max(0.0).sqrt();
    out
}

#[test]
fn cosine_and_normalisation_are_robust_to_scale_and_bad_vectors() {
    let v = vector(0.6);
    approx(cosine(&vector(1.0), &v).unwrap(), 0.6);
    approx(cosine(&v.map(|v| v * f32::MAX), &v).unwrap(), 1.0);
    approx(cosine(&normalise_embedding(&v.map(|v| v * 17.0)).unwrap(), &v).unwrap(), 1.0);
    assert!(normalise_embedding(&[0.0; 128]).is_err());
    assert!(cosine(&v, &[f32::INFINITY; 128]).is_err());
}

#[test]
fn clustering_is_stable_by_date_key_index_excludes_singletons_orders_by_size() {
    let mut red = [0.0; 128];
    red[0] = 1.0;
    let mut green = [0.0; 128];
    green[1] = 1.0;
    let mut blue = [0.0; 128];
    blue[2] = 1.0;
    let mut nodes = vec![
        FaceNode { key: key(3, "z", 0), embedding: red },
        FaceNode { key: key(1, "b", 1), embedding: green },
        FaceNode { key: key(1, "b", 0), embedding: green },
        FaceNode { key: key(2, "a", 0), embedding: red },
        FaceNode { key: key(1, "c", 0), embedding: red },
        FaceNode { key: key(0, "alone", 0), embedding: blue },
    ];
    let expected = chinese_whispers(&nodes).unwrap();
    assert_eq!(expected.len(), 2);
    assert_eq!(expected[0].members, vec![key(1, "c", 0), key(2, "a", 0), key(3, "z", 0)]);
    assert_eq!(expected[1].members, vec![key(1, "b", 0), key(1, "b", 1)]);
    for _ in 0..nodes.len() {
        nodes.rotate_left(1);
        assert_eq!(chinese_whispers(&nodes).unwrap(), expected);
    }
    nodes.reverse();
    assert_eq!(chinese_whispers(&nodes).unwrap(), expected);
    assert!(chinese_whispers(&[]).unwrap().is_empty());
    assert!(chinese_whispers(&[nodes[0].clone()]).unwrap().is_empty());
    assert!(chinese_whispers(&[nodes[0].clone(), nodes[0].clone()]).is_err());
}

#[test]
fn clustering_edge_threshold_is_inclusive_and_centroid_is_unit() {
    let a = FaceNode { key: key(0, "a", 0), embedding: vector(1.0) };
    let b = FaceNode { key: key(0, "b", 0), embedding: vector(0.5) };
    let cluster = chinese_whispers(&[a.clone(), b]).unwrap();
    assert_eq!(cluster.len(), 1);
    let norm = cluster[0].centroid.iter().map(|v| v * v).sum::<f32>();
    approx(norm, 1.0);
    assert!(chinese_whispers(&[a, FaceNode { key: key(0, "b", 0), embedding: vector(0.499) }]).unwrap().is_empty());
}

#[test]
fn equal_size_suggestions_sort_by_the_first_member() {
    let nodes = [
        FaceNode { key: key(1, "d", 0), embedding: vector(-1.0) },
        FaceNode { key: key(1, "c", 0), embedding: vector(-1.0) },
        FaceNode { key: key(1, "b", 0), embedding: vector(1.0) },
        FaceNode { key: key(1, "a", 0), embedding: vector(1.0) },
    ];
    let out = chinese_whispers(&nodes).unwrap();
    assert_eq!(out[0].members[0].key, "a");
    assert_eq!(out[1].members[0].key, "c");
}

#[test]
fn assignment_threshold_margin_runner_up_ties_and_rejections() {
    let k = key(0, "face", 0);
    let candidate = vector(1.0);
    let nobody = BTreeSet::new();
    let people = |scores: &[f32]| scores.iter().enumerate().map(|(i, c)| NamedPerson { id: i.to_string(), centroid: vector(*c) }).collect::<Vec<_>>();
    assert!(assign_person(&k, &candidate, &[], &nobody).unwrap().is_none());
    assert!(assign_person(&k, &candidate, &people(&[0.449]), &nobody).unwrap().is_none());
    assert_eq!(assign_person(&k, &candidate, &people(&[0.45]), &nobody).unwrap().unwrap().person, "0");
    assert!(assign_person(&k, &candidate, &people(&[0.8, 0.8]), &nobody).unwrap().is_none());
    assert!(assign_person(&k, &candidate, &people(&[0.8, 0.751]), &nobody).unwrap().is_none());
    assert_eq!(assign_person(&k, &candidate, &people(&[0.8, 0.75]), &nobody).unwrap().unwrap().person, "0");
    assert!(assign_person(&k, &candidate, &people(&[0.8, 0.749]), &nobody).unwrap().is_some());
    let rejected = BTreeSet::from([(k.clone(), "0".into())]);
    assert!(assign_person(&k, &candidate, &people(&[0.9, 0.7]), &rejected).unwrap().is_none());
    let rejected_runner = BTreeSet::from([(k.clone(), "1".into())]);
    assert!(assign_person(&k, &candidate, &people(&[0.8, 0.79]), &rejected_runner).unwrap().is_none(), "rejection does not erase ambiguity");
    let cluster = Cluster { label: k.clone(), members: vec![key(0, "other", 0), k], centroid: candidate };
    assert!(assign_cluster(&cluster, &people(&[0.9]), &rejected).unwrap().is_none());
    assert!(assign_cluster(&cluster, &people(&[0.9]), &nobody).unwrap().is_some());
}

// Small protobuf writers like src/tests.rs: no protobuf dependency, no real weights.
fn varint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let b = (v & 127) as u8;
        v >>= 7;
        out.push(if v == 0 { b } else { b | 128 });
        if v == 0 {
            break;
        }
    }
}
fn number(f: u32, v: u64, out: &mut Vec<u8>) {
    varint(u64::from(f) << 3, out);
    varint(v, out);
}
fn bytes(f: u32, v: &[u8], out: &mut Vec<u8>) {
    varint(u64::from(f) << 3 | 2, out);
    varint(v.len() as u64, out);
    out.extend_from_slice(v);
}
fn message(f: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut out = Vec::new();
    f(&mut out);
    out
}
fn info(name: &str, dims: &[u64]) -> Vec<u8> {
    message(|out| {
        bytes(1, name.as_bytes(), out);
        bytes(
            2,
            &message(|t| {
                bytes(
                    1,
                    &message(|v| {
                        number(1, 1, v);
                        bytes(
                            2,
                            &message(|s| {
                                for &d in dims {
                                    bytes(1, &message(|x| number(1, d, x)), s);
                                }
                            }),
                            v,
                        );
                    }),
                    t,
                )
            }),
            out,
        );
    })
}
fn attr(name: &str, values: &[u64], scalar: bool) -> Vec<u8> {
    message(|o| {
        bytes(1, name.as_bytes(), o);
        for &v in values {
            number(if scalar { 3 } else { 8 }, v, o);
        }
        number(20, if scalar { 2 } else { 7 }, o);
    })
}
fn node(op: &str, inputs: &[&str], output: &str, attrs: &[Vec<u8>]) -> Vec<u8> {
    message(|o| {
        for name in inputs {
            bytes(1, name.as_bytes(), o);
        }
        bytes(2, output.as_bytes(), o);
        bytes(3, format!("node_{output}").as_bytes(), o);
        bytes(4, op.as_bytes(), o);
        for a in attrs {
            bytes(5, a, o);
        }
    })
}
fn tensor(name: &str, dims: &[u64], datatype: u64, raw: &[u8]) -> Vec<u8> {
    message(|o| {
        for &d in dims {
            number(1, d, o);
        }
        number(2, datatype, o);
        bytes(8, name.as_bytes(), o);
        bytes(9, raw, o);
    })
}
fn floats(name: &str, dims: &[u64], values: &[f32]) -> Vec<u8> {
    tensor(name, dims, 1, &values.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>())
}
fn ints(name: &str, values: &[i64]) -> Vec<u8> {
    tensor(name, &[values.len() as u64], 7, &values.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>())
}
fn model(nodes: Vec<Vec<u8>>, constants: Vec<Vec<u8>>, size: u64, outputs: Vec<Vec<u8>>) -> Vec<u8> {
    let graph = message(|g| {
        for n in nodes {
            bytes(1, &n, g);
        }
        bytes(2, b"mock_faces", g);
        for c in constants {
            bytes(5, &c, g);
        }
        bytes(11, &info("x", &[1, 3, size, size]), g);
        for o in outputs {
            bytes(12, &o, g);
        }
    });
    message(|m| {
        number(1, 7, m);
        bytes(7, &graph, m);
        bytes(8, &message(|o| number(2, 13, o)), m);
    })
}

/// Dynamic twelve-output YuNet layout. A bright 8×8 patch yields one stride-8 candidate.
/// Declared 640 output shapes are intentionally stale, as in the pinned detector.
fn mock_detector(short_side: f32) -> Vec<u8> {
    let bbox = [0.5, 0.5, (short_side / 8.0).ln(), (short_side / 8.0).ln()];
    let landmarks: Vec<_> = REFERENCE_LANDMARKS.iter().flat_map(|p| p.map(|v| v / 16.0 - 3.0)).collect();
    let constants = vec![
        floats("divisor", &[], &[255.0]),
        floats("zero", &[], &[0.0]),
        floats("box", &[1, 1, 4], &bbox),
        floats("points", &[1, 1, 10], &landmarks),
        ints("shape", &[1, -1, 1]),
    ];
    let mut nodes = Vec::new();
    let mut outputs = Vec::new();
    for s in [8u64, 16, 32] {
        let pool = format!("pool{s}");
        let mean = format!("mean{s}");
        let scaled = format!("scaled{s}");
        let cls = format!("cls_{s}");
        let obj = format!("obj_{s}");
        let zeros = format!("zeros{s}");
        let bbox = format!("bbox_{s}");
        let kps = format!("kps_{s}");
        nodes.push(node("AveragePool", &["x"], &pool, &[attr("kernel_shape", &[s, s], false), attr("strides", &[s, s], false)]));
        nodes.push(node("ReduceMean", &[&pool], &mean, &[attr("axes", &[1], false), attr("keepdims", &[0], true)]));
        nodes.push(node("Div", &[&mean, "divisor"], &scaled, &[]));
        nodes.push(node("Reshape", &[&scaled, "shape"], &cls, &[]));
        nodes.push(node("Identity", &[&cls], &obj, &[]));
        nodes.push(node("Mul", &[&cls, "zero"], &zeros, &[]));
        nodes.push(node("Add", &[&zeros, "box"], &bbox, &[]));
        nodes.push(node("Add", &[&zeros, "points"], &kps, &[]));
        for (name, channels) in [(&cls, 1), (&obj, 1), (&bbox, 4), (&kps, 10)] {
            outputs.push(info(name, &[1, (640 / s).pow(2), channels]));
        }
    }
    outputs.reverse(); // Import must find ONNX names, not indices or node names.
    model(nodes, constants, 640, outputs)
}

/// Mimics SFace's internal pixel normalisation, then gathers channel means to 1×128.
fn mock_embedder(dim: usize) -> Vec<u8> {
    let constants =
        vec![floats("offset", &[], &[127.5]), floats("scale", &[], &[1.0 / 128.0]), ints("channels", &(0..dim).map(|i| (i % 3) as i64).collect::<Vec<_>>())];
    let nodes = vec![
        node("Sub", &["x", "offset"], "centred", &[]),
        node("Mul", &["centred", "scale"], "normalised", &[]),
        node("ReduceMean", &["normalised"], "mean", &[attr("axes", &[2, 3], false), attr("keepdims", &[0], true)]),
        node("Gather", &["mean", "channels"], "embedding", &[attr("axis", &[1], true)]),
    ];
    model(nodes, constants, 112, vec![info("embedding", &[1, dim as u64])])
}

struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new(side: f32, dim: usize) -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("li-seg-faces-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(dir.join("segmentation")).unwrap();
        std::fs::write(dir.join("segmentation").join(YUNET.file), mock_detector(side)).unwrap();
        std::fs::write(dir.join("segmentation").join(SFACE.file), mock_embedder(dim)).unwrap();
        Self(dir)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn patch(w: usize, h: usize, value: u8) -> Vec<u8> {
    let mut pixels = vec![0; w * h * 3];
    // At long edge 1024, exactly one stride-8 cell is illuminated.
    let scale = w.max(h) / 1024;
    let x0 = (w / scale / 16 / 8 * 8) * scale;
    let y0 = (h / scale / 16 / 8 * 8) * scale;
    for y in y0..y0 + 8 * scale {
        for x in x0..x0 + 8 * scale {
            pixels[(y * w + x) * 3..(y * w + x) * 3 + 3].fill(value);
        }
    }
    pixels
}

#[test]
fn full_mock_path_loads_named_outputs_aligns_embeds_and_caches_shapes() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<FaceModels>();
    let scratch = Scratch::new(64.0, 128);
    let models = FaceModels::load(&scratch.0).unwrap();
    for (w, h) in [(1024, 1024), (2048, 1024), (1024, 2048)] {
        let pixels = patch(w, h, 255);
        let image = rgb(&pixels, w, h);
        let faces = models.faces(image).unwrap();
        assert_eq!(faces.len(), 1);
        let f = &faces[0];
        let l = Letterbox::new(w, h).unwrap();
        approx(f.rect[2], 64.0 / l.scale);
        approx(f.rect[3], 64.0 / l.scale);
        approx(f.rect[0] + f.rect[2] / 2.0, (l.resized_w / 16 / 8 * 8 + 4) as f32 / l.scale);
        assert_eq!(f.score, 1.0);
        approx(f.embedding.iter().map(|v| v * v).sum::<f32>(), 1.0);
        assert_eq!(faces, models.faces(image).unwrap());
        let box_ = models.detect(image).unwrap().remove(0);
        assert_eq!(f.embedding, models.embed(image, &box_).unwrap());
    }
    assert_eq!(models.detector_plans.lock().unwrap().len(), 3);
    // A flat black image must not yield spurious faces.
    assert!(models.faces(rgb(&vec![0; 1024 * 1024 * 3], 1024, 1024)).unwrap().is_empty());
}

#[test]
fn mock_sface_receives_rgb_255_without_double_normalisation() {
    let scratch = Scratch::new(64.0, 128);
    let models = FaceModels::load(&scratch.0).unwrap();
    let pixels = [255, 128, 0].repeat(112 * 112);
    let b = FaceBox { landmarks: REFERENCE_LANDMARKS, ..bbox(0.0, 1.0) };
    let embedding = models.embed(rgb(&pixels, 112, 112), &b).unwrap();
    let expected = normalise_embedding(&std::array::from_fn(|i| ([127.5, 0.5, -127.5][i % 3]) / 128.0)).unwrap();
    for (a, b) in embedding.into_iter().zip(expected) {
        approx(a, b);
    }
    let u16pixels: Vec<_> = pixels.iter().map(|v| u16::from(*v) * 257).collect();
    let floatpixels: Vec<_> = pixels.iter().map(|v| f32::from(*v) / 255.0).collect();
    for pixels in [RgbPixels::U16(&u16pixels), RgbPixels::F32(&floatpixels)] {
        let actual = models.embed(FaceImage { pixels, width: 112, height: 112 }, &b).unwrap();
        for (a, b) in actual.into_iter().zip(embedding) {
            approx(a, b);
        }
    }
}

#[test]
fn full_mock_path_applies_score_and_short_side_thresholds() {
    let pixels = patch(1024, 1024, 229); // 229/255 < .9
    let scratch = Scratch::new(40.0, 128);
    let models = FaceModels::load(&scratch.0).unwrap();
    assert!(models.faces(rgb(&pixels, 1024, 1024)).unwrap().is_empty());
    let pixels = patch(1024, 1024, 230); // 230/255 > .9
    assert_eq!(models.faces(rgb(&pixels, 1024, 1024)).unwrap().len(), 1);
    let small = Scratch::new(39.9, 128);
    let models = FaceModels::load(&small.0).unwrap();
    assert!(models.faces(rgb(&pixels, 1024, 1024)).unwrap().is_empty());
}

#[test]
fn mock_models_can_run_concurrently_with_identical_results() {
    let scratch = Scratch::new(64.0, 128);
    let models = FaceModels::load(&scratch.0).unwrap();
    let pixels = patch(1024, 2048, 255);
    std::thread::scope(|scope| {
        let a = scope.spawn(|| models.faces(rgb(&pixels, 1024, 2048)).unwrap());
        let b = scope.spawn(|| models.faces(rgb(&pixels, 1024, 2048)).unwrap());
        assert_eq!(a.join().unwrap(), b.join().unwrap());
    });
}

#[test]
fn model_loading_and_output_errors_are_reported() {
    assert!(FaceModels::load(Path::new("/nonexistent-li-seg-faces")).is_err());
    let scratch = Scratch::new(64.0, 127);
    let models = FaceModels::load(&scratch.0).unwrap();
    let pixels = patch(1024, 1024, 255);
    assert!(models.faces(rgb(&pixels, 1024, 1024)).unwrap_err().to_string().contains("SFace output shape"));
    std::fs::write(scratch.0.join("segmentation").join(YUNET.file), b"incomplete download").unwrap();
    assert!(FaceModels::load(&scratch.0).is_err());
}

#[test]
fn pinned_model_metadata_matches_the_build_spec() {
    assert_eq!(YUNET.bytes, 232589);
    assert_eq!(SFACE.bytes, 38696353);
    assert_eq!(YUNET.licence, "MIT");
    assert_eq!(SFACE.licence, "Apache-2.0");
    for file in [YUNET, SFACE] {
        assert_eq!(file.sha256.len(), 64);
        assert!(file.url.starts_with("https://huggingface.co/opencv/"));
        assert!(!file.url.contains("/main/"));
    }
}

#[test]
#[ignore = "coordinator: LI_SEG_TEST_MODELS points to downloaded YuNet/SFace; no downloads"]
fn real_weights_migrant_mother_twice_and_no_tetons_faces() {
    let dir = std::env::var_os("LI_SEG_TEST_MODELS").expect("set LI_SEG_TEST_MODELS to the model directory");
    let models = FaceModels::load(Path::new(&dir)).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/upstream/lightcraft/images");
    let mother = image::open(root.join("ba-migrant-mother.jpg")).unwrap().to_rgb8();
    // This repository fixture is already a 3200×2000 before/after screenshot containing
    // the two copies side by side. Duplicating the whole fixture would yield four faces.
    let faces = models.faces(rgb(mother.as_raw(), mother.width() as usize, mother.height() as usize)).unwrap();
    assert!(!faces.is_empty());
    assert!(faces.iter().all(|f| f.score >= 0.9));
    assert_eq!(faces.len(), 2, "two copies should leave two main faces after NMS");
    let mut faces = faces;
    faces.sort_by(|a, b| a.rect[0].total_cmp(&b.rect[0]));
    // Reference fixture's gallery frame is 3200 px across; compare in that frame.
    for (face, expected_x) in faces.iter().zip([620.0, 1868.0]) {
        let x = (face.rect[0] + face.rect[2] / 2.0) * 3200.0 / mother.width() as f32;
        let y = (face.rect[1] + face.rect[3] / 2.0) * 3200.0 / mother.width() as f32;
        assert!((x - expected_x).abs() < 180.0, "face centre x={x}");
        assert!((y - 572.0).abs() < 180.0, "face centre y={y}");
    }
    assert!(cosine(&faces[0].embedding, &faces[1].embedding).unwrap() >= 0.6);
    let tetons = image::open(root.join("ba-tetons.jpg")).unwrap().to_rgb8();
    assert!(models.faces(rgb(tetons.as_raw(), tetons.width() as usize, tetons.height() as usize)).unwrap().is_empty());
}

#[test]
#[ignore = "24 MP CPU benchmark with tiny local models"]
fn bench_faces_24mp_mock_models() {
    use std::time::Instant;
    let scratch = Scratch::new(64.0, 128);
    let models = FaceModels::load(&scratch.0).unwrap();
    let (w, h) = (6000, 4000);
    let mut pixels = vec![0u8; w * h * 3];
    // Produce one full detector cell after the resize of this non-integer aspect ratio.
    for y in 1490..1570 {
        for x in 2990..3070 {
            pixels[(y * w + x) * 3..(y * w + x) * 3 + 3].fill(255);
        }
    }
    let image = rgb(&pixels, w, h);
    let start = Instant::now();
    let (input, layout) = letterbox(image).unwrap();
    let letterbox_time = start.elapsed();
    let start = Instant::now();
    let values = models.detector_plan(layout.pad_w, layout.pad_h).unwrap()(Tensor::from_shape(&[1, 3, layout.pad_h, layout.pad_w], &input).unwrap()).unwrap();
    let inference_time = start.elapsed();
    let start = Instant::now();
    let outputs = outputs_from_values(&values, layout.pad_w, layout.pad_h).unwrap();
    let boxes = nms(decode_yunet(&outputs, layout.pad_w, layout.pad_h, SCORE_MIN).unwrap(), NMS_IOU, NMS_TOP_K);
    let decode_time = start.elapsed();
    assert!(!boxes.is_empty());
    let landmarks = boxes[0].landmarks.map(|p| layout.to_original(p));
    let start = Instant::now();
    let matrix = similarity_transform(&landmarks).unwrap();
    let _aligned = warp_affine_112(image, &matrix).unwrap();
    let align_time = start.elapsed();
    let start = Instant::now();
    let faces = models.faces(image).unwrap();
    let full_time = start.elapsed();
    assert!(!faces.is_empty());
    eprintln!(
        "24 MP mock: letterbox={letterbox_time:?}, detector={inference_time:?}, decode+NMS={decode_time:?}, align={align_time:?}, faces={full_time:?}, count={}",
        faces.len()
    );
}
