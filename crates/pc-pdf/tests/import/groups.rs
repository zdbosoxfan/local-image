//! Original-PDF Poppler oracles: never export the imported scene as reference.
use super::*;
use photocraft_doc::{BlendMode, Fill, GroupIsolation};
use std::{path::PathBuf, process::Command, sync::OnceLock};

const PAINT: &str = "/M gs 0.8 0.25 0.5 rg 4 4 40 48 re f /S gs 0.15 0.7 0.4 rg 20 12 40 48 re f";
const STATES: &str = "/ExtGState << /M << /BM /Multiply /ca 0.7 >> /S << /BM /Screen /ca 0.6 >> >>";
fn form(isolation: &str, body: &str, resources: &str) -> Vec<u8> {
    fixture::stream(
        &format!("/Type /XObject /Subtype /Form /BBox [8 8 56 56] /Group << /S /Transparency /CS /DeviceRGB {isolation} >> /Resources << {resources} >>"),
        body.as_bytes(),
    )
}
fn grouped(isolation: &str, opacity: f32, backdrop: f32) -> Vec<u8> {
    fixture::page(
        "q /B gs 0.2 0.55 0.85 rg 0 0 64 64 re f Q /A gs /X Do",
        &format!("/ExtGState << /A << /ca {opacity} >> /B << /ca {backdrop} >> >> /XObject << /X 5 0 R >>"),
        "",
        &[form(isolation, PAINT, STATES)],
    )
}
fn reference_raw(bytes: &[u8], label: &str) -> image::RgbaImage {
    let dir = std::env::current_dir().unwrap().join("target/refvec/pdf-import-groups");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{label}-{}.pdf", std::process::id()));
    std::fs::write(&file, bytes).unwrap();
    let prefix = file.with_extension("");
    // Use Splash, as the existing export oracles do. Cairo's output device
    // ignores group isolation. Splash composites white only after page drawing;
    // separate closed-form checks below retain transparent/partial alpha proof.
    let output =
        Command::new("pdftoppm").args(["-singlefile", "-png", "-r", "72"]).arg(&file).arg(&prefix).output().expect("Poppler Splash pdftoppm is required");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let mut png = prefix.into_os_string();
    png.push(".png");
    image::open(PathBuf::from(png)).unwrap().into_rgba8()
}
const SAMPLES: [i32; 7] = [2, 10, 18, 30, 50, 58, 62];
fn reference(bytes: &[u8], label: &str) -> image::RgbaImage {
    static CALIBRATED: OnceLock<()> = OnceLock::new();
    CALIBRATED.get_or_init(|| {
        // Independent single-overlap witness, before trusting any group oracle.
        // Nonisolated Multiply: B*(1-a+a*S); isolated: B*(1-a)+a*S.
        for (isolation, expected) in [("/I false", [0.172_f32, 0.26125, 0.5525]), ("/I true", [0.62, 0.34, 0.605])] {
            let rendered = reference_raw(&grouped(isolation, 1.0, 1.0), &format!("calibration-{}", isolation.ends_with("true")));
            let pixel = rendered.get_pixel(10, 30).0;
            for channel in 0..3 {
                assert!(
                    (f32::from(pixel[channel]) / 255.0 - expected[channel]).abs() < 0.008,
                    "Poppler oracle does not preserve {isolation}: {pixel:?}, expected {expected:?}; do not use an isolated-only backend"
                );
            }
        }
    });
    reference_raw(bytes, label)
}
// The pinned Hayro 0.7.0 public paint/BC conversion goes through RGBA8:
// color.rs f32_to_u8 / ToRgb::to_alpha_color round to nearest 1/255, including
// paint opacity. These independently calculated fixture inputs account for
// that declared precision boundary; group callback opacity remains unquantized.
// Original decimal PDF bytes and Splash RGB comparison tolerances stay intact.
const FIRST_PAINT_ALPHA: f32 = 179.0 / 255.0; // .7
const SECOND_PAINT_ALPHA: f32 = 153.0 / 255.0; // .6
const MASK_PAINT_ALPHA: f32 = 191.0 / 255.0; // .75
const MASK_SOURCE_RGB: [f32; 3] = [128.0 / 255.0, 204.0 / 255.0, 77.0 / 255.0]; // .5/.8/.3
const MASK_BC_RGB: [f32; 3] = [204.0 / 255.0, 102.0 / 255.0, 51.0 / 255.0]; // .8/.4/.2
fn art_alpha(x: i32, y: i32) -> f32 {
    let (x, y) = (x as f32 + 0.5, 63.5 - y as f32);
    if !(8.0..56.0).contains(&x) || !(8.0..56.0).contains(&y) {
        return 0.0;
    }
    let first = if x < 44.0 && y < 52.0 { FIRST_PAINT_ALPHA } else { 0.0 };
    let second = if x >= 20.0 && y >= 12.0 { SECOND_PAINT_ALPHA } else { 0.0 };
    1.0 - (1.0 - first) * (1.0 - second)
}
fn alpha_oracle(document: &Document, expected: impl Fn(i32, i32) -> f32, label: &str) {
    let rendered = photocraft_compose::flatten(document);
    for y in SAMPLES {
        for x in SAMPLES {
            let alpha = rendered.get(x, y)[3];
            assert!((alpha - expected(x, y)).abs() < 0.00001, "{label}: alpha at ({x},{y}) was {alpha}, expected {}", expected(x, y));
        }
    }
}
fn oracle_doc(bytes: &[u8], document: &Document, label: &str) {
    let expected = reference(bytes, label);
    let actual = photocraft_compose::flatten(document);
    assert_eq!(expected.dimensions(), (64, 64));
    let mut mean = 0.0_f64;
    let mut worst = 0.0_f32;
    // Integer-aligned fixture geometry: sample away from antialias boundaries.
    for y in SAMPLES {
        for x in SAMPLES {
            let a = actual.get(x, y);
            let e = expected.get_pixel(x as u32, y as u32).0.map(|v| f32::from(v) / 255.0);
            for k in 0..3 {
                let delta = (a[k] * a[3] + 1.0 - a[3] - e[k]).abs();
                mean += f64::from(delta);
                worst = worst.max(delta);
            }
        }
    }
    mean /= 49.0 * 3.0;
    assert!(mean < 0.008 && worst < 0.025, "{label}: RGB-on-white mean={mean}, worst={worst}");
}
fn oracle(bytes: &[u8], label: &str) -> Document {
    let document = doc(import(bytes, &ImportOptions::default()).unwrap());
    oracle_doc(bytes, &document, label);
    document
}
#[test]
fn pdf_groups_original_poppler_missing_false_true_opacity_and_partial_backdrop() {
    for (i, isolation) in ["", "/I false", "/I true"].iter().enumerate() {
        for (j, opacity) in [1.0, 0.5].iter().enumerate() {
            for (k, backdrop) in [1.0, 0.4, 0.0].iter().enumerate() {
                let d = oracle(&grouped(isolation, *opacity, *backdrop), &format!("isolation-{i}-{j}-{k}"));
                alpha_oracle(&d, |x, y| backdrop + (1.0 - backdrop) * opacity * art_alpha(x, y), &format!("isolation-{i}-{j}-{k}"));
                let groups: Vec<_> = d.walk().into_iter().filter(|(_, _, l)| l.name == "PDF transparency").collect();
                assert_eq!(groups.len(), 1);
                let layer = groups[0].2;
                assert_eq!(layer.blend, BlendMode::Normal);
                let LayerContent::Group(group) = &layer.content else { panic!("group") };
                assert_eq!(group.isolation, if i == 2 { GroupIsolation::Isolated } else { GroupIsolation::NonIsolated });
                assert_eq!(layer.opacity, *opacity);
                assert_eq!(layer.fill_opacity, 1.0);
            }
        }
    }
}
#[test]
fn pdf_groups_mixed_nested_repeated_forms_clips_and_image_interleaving() {
    let image = fixture::stream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8", &[110, 70, 180]);
    let bytes = fixture::page(
        "0.2 0.55 0.85 rg 0 0 64 64 re f /X Do q 1 0 0 1 8 0 cm /X Do Q q 6 0 0 6 0 0 cm /Im Do Q",
        "/XObject << /X 5 0 R /Im 7 0 R >>",
        "",
        &[form("/I false", "/Span BMC q 8 8 36 36 re W n /Inner Do Q EMC /Inner Do", "/XObject << /Inner 6 0 R >>"), form("/I true", PAINT, STATES), image],
    );
    let d = oracle(&bytes, "nested-repeated-image");
    let isolation: Vec<_> = d
        .walk()
        .into_iter()
        .filter(|(_, _, l)| l.name == "PDF transparency")
        .filter_map(|(_, _, l)| if let LayerContent::Group(g) = &l.content { Some(g.isolation) } else { None })
        .collect();
    assert_eq!(isolation.iter().filter(|&&i| i == GroupIsolation::NonIsolated).count(), 2);
    assert_eq!(isolation.iter().filter(|&&i| i == GroupIsolation::Isolated).count(), 4);
    assert_eq!(isolation.iter().filter(|&&i| i == GroupIsolation::Auto).count(), 1);
}
#[test]
fn pdf_groups_nonisolated_masks_use_bc_before_child_blend_and_transfer() {
    for mode in ["Luminosity", "Alpha"] {
        for (index, backdrop) in [1.0_f32, 0.4, 0.0].into_iter().enumerate() {
            let page_body = "q /B gs 0.2 0.55 0.85 rg 0 0 64 64 re f Q /A gs /X Do";
            let resources = format!(
                "/ExtGState << /B << /ca {backdrop} >> /A << /ca 0.5 /SMask << /S /{mode} /G 6 0 R /BC [0.8 0.4 0.2] /TR 7 0 R >> >> >> /XObject << /X 5 0 R >>"
            );
            let mask_body = "/Span BMC 0 0 48 64 re W n /M gs 0.5 0.8 0.3 rg 0 0 40 64 re f EMC";
            let mask_resources = "/ExtGState << /M << /BM /Multiply /ca 0.75 >> >>";
            let objects = [
                form("/I false", PAINT, STATES),
                form("/I false", mask_body, mask_resources),
                b"<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 2 >>".to_vec(),
            ];
            let bytes = fixture::page(page_body, &resources, "", &objects);
            let label = format!("mask-{mode}-{index}");
            let scene = doc(import(&bytes, &ImportOptions::default()).unwrap());
            let mask_value = |x: i32, y: i32| {
                let (x, y) = (x as f32 + 0.5, 63.5 - y as f32);
                let painted = (8.0..40.0).contains(&x) && (8.0..56.0).contains(&y);
                let scalar: f32 = if mode == "Alpha" {
                    if painted { MASK_PAINT_ALPHA } else { 0.0 }
                } else {
                    // Independent Multiply over BC, using the documented
                    // quantized paint inputs, then PDF RGB luminosity.
                    let rgb: [f32; 3] = std::array::from_fn(|channel| {
                        MASK_BC_RGB[channel] * if painted { 1.0 - MASK_PAINT_ALPHA + MASK_PAINT_ALPHA * MASK_SOURCE_RGB[channel] } else { 1.0 }
                    });
                    0.3 * rgb[0] + 0.59 * rgb[1] + 0.11 * rgb[2]
                };
                scalar * scalar // independent FunctionType2 N=2 transfer
            };
            let mask = scene.walk().into_iter().find_map(|(_, _, l)| (l.name == "PDF transparency").then_some(l.mask.as_ref()).flatten()).unwrap();
            for y in SAMPLES {
                for x in SAMPLES {
                    assert!((mask.surface.pixel(x, y)[0] - mask_value(x, y)).abs() < 0.00001, "{label}: scalar mask at ({x},{y})");
                }
            }
            alpha_oracle(&scene, |x, y| backdrop + (1.0 - backdrop) * 0.5 * art_alpha(x, y) * mask_value(x, y), &label);
            if mode == "Luminosity" {
                // ISO 32000-1 11.5.3 requires the mask's initial backdrop to be
                // opaque BC. Splash instead copies the current page for I=false
                // and adds BC afterwards. Retain the unmodified importer input
                // and original-render evidence, but compare against this exact
                // source-equivalent witness: isolated mask, opaque Normal BC
                // painted before the original local clip/Multiply commands.
                // This equivalence is specific to this Luminosity/K=false/RGB
                // fixture; it is not valid for Alpha or general PDF rewrites.
                let original_reference = reference(&bytes, &format!("{label}-original-splash"));
                let mut witness_objects = objects.clone();
                witness_objects[1] = form("/I true", &format!("0.8 0.4 0.2 rg 0 0 64 64 re f\n{mask_body}"), mask_resources);
                let witness = fixture::page(page_body, &resources, "", &witness_objects);
                let witness_reference = reference(&witness, &format!("{label}-source-equivalent"));
                eprintln!(
                    "{label}: original Splash {:?}; BC-initial source-equivalent {:?}",
                    original_reference.get_pixel(10, 30).0,
                    witness_reference.get_pixel(10, 30).0
                );
                oracle_doc(&witness, &scene, &format!("{label}-BC-initial"));
            } else {
                oracle_doc(&bytes, &scene, &label);
            }
        }
    }
}

#[test]
fn pdf_groups_native_roundtrip_child_edit_and_changed_backdrop_remain_live() {
    let original = grouped("", 0.5, 1.0);
    let d = oracle(&original, "roundtrip-original");
    let native = photocraft_format::save_to_bytes(&d, &Default::default()).unwrap();
    let mut loaded = photocraft_format::load_from_bytes(&native).unwrap();
    oracle_doc(&original, &loaded, "roundtrip-restored");
    let path = loaded.walk().into_iter().find(|(_, _, l)| matches!(l.content, LayerContent::Shape(_))).unwrap().2.id;
    let format = loaded.pixel_format();
    let bounds = loaded.bounds();
    let backdrop = loaded.layer_mut(path).unwrap();
    let LayerContent::Shape(shape) = &mut backdrop.content else { panic!("shape") };
    shape.fill = Some(Fill::Solid(photocraft_doc::Color::rgb(0.7, 0.3, 0.2)));
    // The compositor consumes prepared shape surfaces. A native edit must
    // rebuild its cache, as the editor and existing import-edit tests do.
    shape.cache = Some(photocraft_vector::render_shape(shape, format, bounds));
    let changed = String::from_utf8(original.clone()).unwrap().replace("0.2 0.55 0.85", "0.7 0.30 0.20");
    // Equal-length substitution preserves original PDF xref offsets.
    assert_eq!(changed.len(), original.len());
    oracle_doc(changed.as_bytes(), &loaded, "roundtrip-backdrop-edit");
    let child = loaded.walk().into_iter().filter(|(_, _, l)| matches!(l.content, LayerContent::Shape(_))).nth(1).unwrap().2.id;
    loaded.layer_mut(child).unwrap().visible = false;
    let changed_child = changed.replace("4 4 40 48 re f", "4 4 40 48 re n");
    oracle_doc(changed_child.as_bytes(), &loaded, "roundtrip-child-edit");
    let native = photocraft_format::save_to_bytes(&loaded, &Default::default()).unwrap();
    let reopened = photocraft_format::load_from_bytes(&native).unwrap();
    assert_eq!(photocraft_compose::flatten(&reopened).px, photocraft_compose::flatten(&loaded).px);
    assert!(!reopened.layer(child).unwrap().visible);
}
#[test]
fn pdf_groups_marker_spoof_balance_expansion_cancel_and_unsupported_semantics() {
    for payload in ["/PCPDFGroup5g0 BMC EMC", "/PCPDF#47roup5g0 BMC EMC", "EMC", "/Span BMC", "Q", "q", "q Q Q"] {
        let bytes = fixture::page("/X Do", "/XObject << /X 5 0 R >>", "", &[form("", payload, "")]);
        assert!(import(&bytes, &ImportOptions::default()).is_err(), "{payload}");
    }
    let bytes = grouped("", 0.5, 1.0);
    let mut options = ImportOptions::default();
    options.limits.decompressed_bytes = 100;
    assert!(matches!(import(&bytes, &options), Err(ImportError::Limit(_))));
    options = ImportOptions::default();
    options.limits.segments = 4;
    assert!(matches!(import(&bytes, &options), Err(ImportError::Limit(_))));
    assert!(matches!(import_with(&bytes, &ImportOptions::default(), &PdfOptions::default(), &AtomicBool::new(true)), Err(ImportError::Cancelled)));
    for (isolation, state) in [("/K true", ""), ("/I 1", "")] {
        let bytes = fixture::page("/A gs /X Do", &format!("/ExtGState << /A << {state} >> >> /XObject << /X 5 0 R >>"), "", &[form(isolation, PAINT, STATES)]);
        assert!(import(&bytes, &ImportOptions::default()).is_err(), "{isolation}, {state}");
    }
}

#[test]
fn pdf_groups_indirect_dictionary_and_form_local_clip_keep_marker_ownership() {
    for isolation in ["true", "false"] {
        let bytes = fixture::page(
            "0.2 0.55 0.85 rg 0 0 64 64 re f /X Do",
            "/XObject << /X 5 0 R >>",
            "",
            &[
                fixture::stream(
                    &format!("/Subtype /Form /BBox [8 8 56 56] /Group 6 0 R /Resources << {STATES} >>"),
                    format!("/Outer BMC 8 8 36 36 re W n /Inner BMC {PAINT} EMC EMC").as_bytes(),
                ),
                format!("<< /S /Transparency /CS /DeviceRGB /I {isolation} >>").into_bytes(),
            ],
        );
        oracle(&bytes, &format!("indirect-local-clip-{isolation}"));
    }
}

#[test]
fn pdf_groups_page_marked_content_can_span_content_array_members() {
    let bytes = fixture::pdf(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Resources << /XObject << /X 6 0 R >> >> /Contents [4 0 R 5 0 R] >>".to_vec(),
        fixture::stream("", b"/Span BMC 0.2 0.55 0.85 rg 0 0 64 64 re f"),
        fixture::stream("", b"/X Do EMC"),
        form("", PAINT, STATES),
    ]);
    oracle(&bytes, "split-page-marked-content");
}

fn gray_form(body: &str, resources: &str) -> Vec<u8> {
    fixture::stream(
        &format!("/Subtype /Form /BBox [0 0 64 64] /Group << /S /Transparency /CS /DeviceGray /I true >> /Resources << {resources} >>"),
        body.as_bytes(),
    )
}
fn gray_warning(document: &photocraft_doc::vector_design::exchange::ImportedScene) -> bool {
    document.warnings.iter().any(|warning| warning.feature == "group-colour-space-unverified")
}
#[test]
fn pdf_groups_isolated_gray_default_inherited_nested_overlap_stroke_and_opacity() {
    for (label, inherited) in [("default", ""), ("inherited", "0.35 g 0.6 G")] {
        let bytes = fixture::page(
            &format!("q 0.25 g 0 0 64 64 re f Q {inherited} /A gs /X Do"),
            "/ExtGState << /A << /ca 0.5 >> >> /XObject << /X 5 0 R >>",
            "",
            &[
                gray_form(
                    "/Tag BMC 4 4 52 52 re W n 8 8 32 40 re f /M gs 0.7 g 0.4 G 3 w 20 12 36 40 re B /Child Do EMC",
                    "/ExtGState << /M << /BM /Multiply /ca 0.6 >> >> /XObject << /Child 6 0 R >>",
                ),
                fixture::stream("/Subtype /Form /BBox [0 0 64 64] /Group << /S /Transparency /I true >>", b"0.5 g 20 20 20 20 re f"),
            ],
        );
        let scene = import(&bytes, &ImportOptions::default()).unwrap();
        assert!(!gray_warning(&scene), "neutral inherited paints and nested Forms must be proven");
        oracle_doc(&bytes, &doc(scene), &format!("gray-{label}"));
    }
}
#[test]
fn pdf_groups_isolated_gray_luminosity_mask_bc_and_gray_image_resources() {
    let bytes = fixture::page(
        "0.25 g 0 0 64 64 re f /A gs /X Do",
        "/ExtGState << /A << /ca 0.5 /SMask << /S /Luminosity /G 6 0 R /BC [0.6] >> >> >> /XObject << /X 5 0 R >>",
        "",
        &[
            gray_form("0.8 g 0 0 64 64 re f q 16 0 0 16 24 24 cm /Im Do Q", "/XObject << /Im 7 0 R >>"),
            gray_form("/Mask BMC 8 8 40 40 re W n /M gs 0.3 g 0 0 40 64 re f EMC", "/ExtGState << /M << /BM /Multiply /ca 0.75 >> >>"),
            fixture::stream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8", &[128]),
        ],
    );
    let scene = import(&bytes, &ImportOptions::default()).unwrap();
    assert!(!gray_warning(&scene), "gray images, masks and neutral BC must be proven");
    oracle_doc(&bytes, &doc(scene), "gray-mask-image");
}
#[test]
fn pdf_groups_legacy_colored_gray_is_retained_but_never_claimed_verified() {
    for (label, body, resources, extra) in [
        ("paint", "1 0 0 rg 0 0 64 64 re f", "", vec![]),
        (
            "image",
            "q 64 0 0 64 0 0 cm /Im Do Q",
            "/XObject << /Im 6 0 R >>",
            vec![fixture::stream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8", &[255, 0, 0])],
        ),
    ] {
        let mut objects = vec![gray_form(body, resources)];
        objects.extend(extra);
        let bytes = fixture::page("/X Do", "/XObject << /X 5 0 R >>", "", &objects);
        let scene = import(&bytes, &ImportOptions::default()).unwrap();
        assert!(gray_warning(&scene), "{label}: colored content must not pass the neutral proof");
        let expected = reference(&bytes, &format!("gray-unverified-{label}"));
        let native = photocraft_compose::flatten(&doc(scene)).get(30, 30);
        let pixel = expected.get_pixel(30, 30).0;
        // Splash does not implement general Group blending-space conversion.
        // It proves source execution here, not the unsupported Gray conversion.
        assert!(pixel[..3].iter().any(|&c| c < 240), "original source must paint");
        assert!((native[0] - native[1]).abs() > 0.5, "legacy RGB behavior remains explicitly unresolved");
    }
}
#[test]
fn pdf_groups_unproven_gray_shadings_and_color_defaults_keep_legacy_warning() {
    for resources in ["/ColorSpace << /DefaultGray /DeviceGray >>", "/Shading << /S 6 0 R >>"] {
        let shading = b"<< /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 64 0] /Function << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> /Extend [true true] >>".to_vec();
        let body = if resources.contains("Shading") { "/S sh" } else { "0.5 g 0 0 64 64 re f" };
        let bytes = fixture::page("/X Do", "/XObject << /X 5 0 R >>", "", &[gray_form(body, resources), shading]);
        let scene = import(&bytes, &ImportOptions::default()).unwrap();
        assert!(gray_warning(&scene));
    }
}

#[test]
fn pdf_groups_isolated_gray_page_retains_neutral_and_warns_colored_legacy_content() {
    let extra = "/Group << /S /Transparency /CS /DeviceGray /I true >>";
    let neutral = fixture::page("0.4 g 0 0 64 64 re f", "", extra, &[]);
    let scene = import(&neutral, &ImportOptions::default()).unwrap();
    assert!(!gray_warning(&scene));
    oracle_doc(&neutral, &doc(scene), "gray-page-neutral");
    let colored = fixture::page("1 0 0 rg 0 0 64 64 re f", "", extra, &[]);
    assert!(gray_warning(&import(&colored, &ImportOptions::default()).unwrap()));
}

#[test]
fn pdf_groups_gray_graphics_transfer_channels_are_unproved() {
    for key in ["TR", "TR2"] {
        for name in ["T", "SMask"] {
            let bytes = fixture::page(
                "/X Do",
                "/XObject << /X 5 0 R >>",
                "",
                &[
                    gray_form(&format!("/{name} gs 0.5 g 0 0 64 64 re f"), &format!("/ExtGState << /{name} << /{key} [6 0 R 7 0 R 8 0 R 9 0 R] >> >>")),
                    b"<< /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [1] /N 1 >>".to_vec(),
                    b"<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [0] /N 1 >>".to_vec(),
                    b"<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [0] /N 1 >>".to_vec(),
                    b"<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >>".to_vec(),
                ],
            );
            let scene = import(&bytes, &ImportOptions::default()).unwrap();
            assert!(gray_warning(&scene), "{key}/{name}: resource names cannot exempt graphics channel transfers");
            let original = reference(&bytes, &format!("gray-transfer-{key}-{name}"));
            assert_eq!(original.get_pixel(30, 30)[3], 255, "original-PDF transfer fixture must paint the oracle interior");
            let d = doc(scene);
            assert!(d.walk().iter().any(|(_, _, layer)| matches!(layer.content, LayerContent::Shape(_))));
        }
    }
}

#[test]
fn pdf_groups_gray_scalar_mask_transfer_remains_distinct_from_graphics_transfer() {
    let bytes = fixture::page(
        "/A gs /X Do",
        "/ExtGState << /A << /SMask << /S /Luminosity /G 6 0 R /TR 7 0 R /BC [0.2] >> >> >> /XObject << /X 5 0 R >>",
        "",
        &[gray_form("0.8 g 0 0 64 64 re f", ""), gray_form("0.5 g 0 0 64 64 re f", ""), b"<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 2 >>".to_vec()],
    );
    let scene = import(&bytes, &ImportOptions::default()).unwrap();
    assert!(!gray_warning(&scene), "a scalar mask TR changes coverage, not RGB channel equivalence");
    let document = doc(scene);
    alpha_oracle(&document, |_, _| (128.0_f32 / 255.0).powi(2), "gray-scalar-mask-transfer");
    oracle_doc(&bytes, &document, "gray-scalar-mask-transfer");
}
#[test]
fn pdf_groups_gray_indirect_form_and_image_selectors_cannot_hide_color() {
    for kind in ["Form", "Image"] {
        let inner = if kind == "Form" {
            fixture::stream("/Type /XObject /Subtype 7 0 R /BBox [0 0 64 64]", b"1 0 0 rg 0 0 64 64 re f")
        } else {
            fixture::stream("/Type /XObject /Subtype 7 0 R /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8", &[255, 0, 0])
        };
        let body = if kind == "Form" { "/Inner Do" } else { "q 64 0 0 64 0 0 cm /Inner Do Q" };
        let bytes =
            fixture::page("/X Do", "/XObject << /X 5 0 R >>", "", &[gray_form(body, "/XObject << /Inner 6 0 R >>"), inner, format!("/{kind}").into_bytes()]);
        let scene = import(&bytes, &ImportOptions::default()).unwrap();
        assert!(gray_warning(&scene), "{kind}: indirect selector must not escape executable/resource proof");
        let original = reference(&bytes, &format!("gray-indirect-{kind}"));
        let pixel = original.get_pixel(30, 30).0;
        assert_eq!(pixel[3], 255);
        // Require execution of the hidden resource, without claiming that
        // Poppler implements this unresolved Group color-space conversion.
        assert!(pixel[..3].iter().any(|&c| c < 240), "hidden source resource must paint");
        let native = photocraft_compose::flatten(&doc(scene)).get(30, 30);
        assert!((native[0] - native[1]).abs() > 0.5, "retained legacy RGB conversion remains explicitly unresolved");
    }
}

#[test]
fn pdf_groups_gray_real_mask_form_cannot_exempt_nested_graphics_transfer() {
    let bytes = fixture::page(
        "/A gs /X Do",
        "/ExtGState << /A << /SMask << /S /Luminosity /G 6 0 R /TR << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> >> >> >> /XObject << /X 5 0 R >>",
        "",
        &[
            gray_form("0.8 g 0 0 64 64 re f", ""),
            gray_form("/SMask gs 0.5 g 0 0 64 64 re f", "/ExtGState << /SMask << /TR2 [7 0 R 8 0 R 9 0 R 10 0 R] >> >>"),
            b"<< /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [1] /N 1 >>".to_vec(),
            b"<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [0] /N 1 >>".to_vec(),
            b"<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [0] /N 1 >>".to_vec(),
            b"<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >>".to_vec(),
        ],
    );
    let scene = import(&bytes, &ImportOptions::default()).unwrap();
    assert!(gray_warning(&scene), "nested graphics state must not inherit the enclosing scalar-mask role");
    let original = reference(&bytes, "gray-mask-nested-transfer");
    assert!(original.get_pixel(30, 30)[3] > 0);
}
#[test]
fn pdf_groups_gray_aliased_transfer_dictionary_never_reuses_a_scalar_exemption() {
    let bytes = fixture::page(
        "/X Do",
        "/XObject << /X 5 0 R >>",
        "",
        &[
            gray_form("/SMask gs /Alias gs 0.5 g 0 0 64 64 re f", "/ExtGState << /SMask 6 0 R /Alias 6 0 R >>"),
            b"<< /TR2 [7 0 R 8 0 R 9 0 R 10 0 R] >>".to_vec(),
            b"<< /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [1] /N 1 >>".to_vec(),
            b"<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [0] /N 1 >>".to_vec(),
            b"<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [0] /N 1 >>".to_vec(),
            b"<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >>".to_vec(),
        ],
    );
    let scene = import(&bytes, &ImportOptions::default()).unwrap();
    assert!(gray_warning(&scene));
    assert_eq!(reference(&bytes, "gray-aliased-transfer").get_pixel(30, 30)[3], 255);
}

fn outer_blend_pdf(mode: &str, isolation: &str, bg: [f32; 3], alpha: f32, mask: bool) -> Vec<u8> {
    let state_mask = if mask { "/SMask << /S /Alpha /G 6 0 R >>" } else { "" };
    fixture::page(
        &format!("q /B gs {} {} {} rg 0 0 64 64 re f Q /A gs /X Do", bg[0], bg[1], bg[2]),
        &format!("/ExtGState << /B << /ca {alpha} >> /A << /BM /{mode} /ca 0.5 {state_mask} >> >> /XObject << /X 5 0 R >>"),
        "",
        &[
            form(isolation, "/S gs 0.8 0.6 0.4 rg 8 8 48 48 re f", "/ExtGState << /S << /BM /Screen /ca 0.6 >> >>"),
            form("/I true", "/M gs 1 g 8 8 48 48 re f", "/ExtGState << /M << /ca 0.4 >> >>"),
        ],
    )
}
#[test]
fn pdf_groups_independent_outer_blends_match_original_splash_and_exact_alpha() {
    for mode in [
        "Multiply",
        "Screen",
        "Overlay",
        "Darken",
        "Lighten",
        "ColorDodge",
        "ColorBurn",
        "HardLight",
        "SoftLight",
        "Difference",
        "Exclusion",
        "Hue",
        "Saturation",
        "Color",
        "Luminosity",
    ] {
        for (index, alpha) in [0.0_f32, 0.4, 1.0].into_iter().enumerate() {
            for mask in [false, true] {
                let bytes = outer_blend_pdf(mode, "/I false", [0.2, 0.4, 0.6], alpha, mask);
                let label = format!("outer-{mode}-{index}-{mask}");
                let d = oracle(&bytes, &label);
                alpha_oracle(
                    &d,
                    |x, y| {
                        let (x, y) = (x as f32 + 0.5, 63.5 - y as f32);
                        let covered = (8.0..56.0).contains(&x) && (8.0..56.0).contains(&y);
                        alpha + (1.0 - alpha) * if covered { 0.6 * 0.5 * if mask { 0.4 } else { 1.0 } } else { 0.0 }
                    },
                    &label,
                );
                let layer = d.walk().into_iter().find(|(_, _, l)| l.name == "PDF transparency" && l.blend != BlendMode::Normal).unwrap().2;
                assert!(matches!(&layer.content,LayerContent::Group(g) if g.isolation==GroupIsolation::NonIsolated));
            }
        }
    }
}
/// Closed-form ISO 32000 group compositing (11.4.8, "Summary of group compositing computations")
/// for the nested fixture below, as RGB over white. Page backdrop B; nonisolated group X
/// (outer Multiply, ca 0.5) holding group Y (Screen, ca 0.8) and a Normal ca 0.4 square N.
/// A nonisolated Y starts from X's running state, which includes X's own backdrop; its group
/// alpha (coverage) is tracked apart from that backdrop, as is X's.
fn spec_nested(x: i32, y: i32, nested_isolated: bool) -> [f32; 3] {
    type State = ([f32; 3], f32);
    fn union(a: f32, b: f32) -> f32 {
        a + b - a * b
    }
    fn multiply(b: f32, s: f32) -> f32 {
        b * s
    }
    fn screen(b: f32, s: f32) -> f32 {
        b + s - b * s
    }
    fn normal(_: f32, s: f32) -> f32 {
        s
    }
    // One element composited onto the running state (C, alpha).
    fn paint(state: State, color: [f32; 3], alpha: f32, blend: fn(f32, f32) -> f32) -> State {
        let (c, ab) = state;
        let ar = union(ab, alpha);
        if ar == 0.0 {
            return state;
        }
        let k = alpha / ar;
        (std::array::from_fn(|i| (1.0 - k) * c[i] + k * ((1.0 - ab) * color[i] + ab * blend(c[i], color[i]))), ar)
    }
    // A group's result: its backdrop contribution removed (C = Cn + (Cn - C0)(a0/ag - a0)).
    fn result(initial: State, last: State, group_alpha: f32) -> State {
        if group_alpha == 0.0 {
            return ([0.0; 3], 0.0);
        }
        let (c0, a0) = initial;
        (std::array::from_fn(|i| last.0[i] + (last.0[i] - c0[i]) * (a0 / group_alpha - a0)), group_alpha)
    }
    let (px, py) = (x as f32 + 0.5, 63.5 - y as f32);
    let inside = |lo: f32, hi: f32| (lo..hi).contains(&px) && (lo..hi).contains(&py);
    let page = paint(([0.0; 3], 0.0), [0.2, 0.4, 0.6], 0.4, normal);
    let mut state = page;
    if inside(8.0, 56.0) {
        // X is nonisolated: it starts from the page state, with its own coverage at 0.
        let (x0, mut xs, mut xg) = (page, page, 0.0);
        let y0 = if nested_isolated { ([0.0; 3], 0.0) } else { xs };
        let ys = paint(y0, [0.8, 0.6, 0.4], 0.6, multiply);
        let (yc, ya) = result(y0, ys, 0.6);
        xs = paint(xs, yc, ya * 0.8, screen);
        xg = union(xg, ya * 0.8);
        if inside(20.0, 40.0) {
            xs = paint(xs, [0.4, 0.6, 0.8], 0.4, normal);
            xg = union(xg, 0.4);
        }
        let (xc, xa) = result(x0, xs, xg);
        state = paint(page, xc, xa * 0.5, multiply);
    }
    let (c, a) = state;
    c.map(|v| v * a + 1.0 - a)
}
#[test]
fn pdf_groups_nested_independent_blends_do_not_reuse_outer_backdrop_coverage() {
    for nested_isolation in ["/I false", "/I true"] {
        let bytes = fixture::page(
            "/B gs 0.2 0.4 0.6 rg 0 0 64 64 re f /A gs /X Do",
            "/ExtGState << /B << /ca 0.4 >> /A << /ca 0.5 /BM /Multiply >> >> /XObject << /X 5 0 R >>",
            "",
            &[
                form(
                    "/I false",
                    "/S gs /Y Do /N gs 0.4 0.6 0.8 rg 20 20 20 20 re f",
                    "/XObject << /Y 6 0 R >> /ExtGState << /S << /BM /Screen /ca 0.8 >> /N << /BM /Normal /ca 0.4 >> >>",
                ),
                form(nested_isolation, "/M gs 0.8 0.6 0.4 rg 8 8 48 48 re f", "/ExtGState << /M << /BM /Multiply /ca 0.6 >> >>"),
            ],
        );
        let isolated = nested_isolation.ends_with("true");
        let label = format!("outer-nested-{isolated}");
        let document = doc(import(&bytes, &ImportOptions::default()).unwrap());
        // Every fixture value is a multiple of 1/255, so the closed form is exact up to float error.
        let rendered = photocraft_compose::flatten(&document);
        for y in SAMPLES {
            for x in SAMPLES {
                let a = rendered.get(x, y);
                let e = spec_nested(x, y, isolated);
                for k in 0..3 {
                    let actual = a[k] * a[3] + 1.0 - a[3];
                    assert!((actual - e[k]).abs() < 0.002, "{label}: ({x},{y}) channel {k} was {actual}, ISO 32000 gives {}", e[k]);
                }
            }
        }
        if isolated {
            oracle_doc(&bytes, &document, &label);
        } else {
            // Poppler Splash draws a nonisolated group nested in a nonisolated group as if it were
            // isolated: it starts the nested group from the outer group's own (empty) coverage,
            // not from the backdrop the outer group itself composites over. Ghostscript and the
            // closed form above agree with each other and not with Splash here (at (10,10):
            // spec 0.612/0.675/0.737 on white, Splash 0.647/0.698/0.749, the isolated value), so
            // Splash is no oracle for this case. It must still differ from the isolated result.
            let mut as_isolated = 0.0_f32;
            for y in SAMPLES {
                for x in SAMPLES {
                    let (n, i) = (spec_nested(x, y, false), spec_nested(x, y, true));
                    as_isolated = as_isolated.max((0..3).map(|k| (n[k] - i[k]).abs()).fold(0.0, f32::max));
                }
            }
            assert!(as_isolated > 0.03, "{label}: the nonisolated nesting must change the result");
        }
    }
}
#[test]
fn pdf_groups_outer_blend_native_roundtrip_and_changed_backdrop_keep_independent_isolation() {
    let bytes = outer_blend_pdf("Multiply", "/I false", [0.2, 0.4, 0.6], 0.4, true);
    let original = oracle(&bytes, "outer-native-original");
    let native = photocraft_format::save_to_bytes(&original, &Default::default()).unwrap();
    let mut reopened = photocraft_format::load_from_bytes(&native).unwrap();
    assert_eq!(original, reopened);
    let id = reopened.walk().into_iter().find(|(_, _, l)| matches!(l.content, LayerContent::Shape(_))).unwrap().2.id;
    let format = reopened.pixel_format();
    let bounds = reopened.bounds();
    let LayerContent::Shape(shape) = &mut reopened.layer_mut(id).unwrap().content else { panic!("shape") };
    let mut color = photocraft_doc::Color::rgb(0.6, 0.2, 0.8);
    color.alpha = 0.4;
    shape.fill = Some(Fill::Solid(color));
    shape.cache = Some(photocraft_vector::render_shape(shape, format, bounds));
    let changed = outer_blend_pdf("Multiply", "/I false", [0.6, 0.2, 0.8], 0.4, true);
    oracle_doc(&changed, &reopened, "outer-native-backdrop-edit");
    let child_id = reopened.walk().into_iter().filter(|(_, _, l)| matches!(l.content, LayerContent::Shape(_))).nth(1).unwrap().2.id;
    let LayerContent::Shape(shape) = &mut reopened.layer_mut(child_id).unwrap().content else { panic!("shape") };
    let mut color = photocraft_doc::Color::rgb(0.2, 0.8, 0.6);
    color.alpha = 0.6;
    shape.fill = Some(Fill::Solid(color));
    shape.cache = Some(photocraft_vector::render_shape(shape, format, bounds));
    let changed = String::from_utf8(changed).unwrap().replace("0.8 0.6 0.4 rg", "0.2 0.8 0.6 rg").into_bytes();
    oracle_doc(&changed, &reopened, "outer-native-child-edit");
    let native = photocraft_format::save_to_bytes(&reopened, &Default::default()).unwrap();
    let final_doc = photocraft_format::load_from_bytes(&native).unwrap();
    oracle_doc(&changed, &final_doc, "outer-native-edited-reopen");
}
