use super::*;
use photocraft_affinity::{Limits, model};

fn container(children: Vec<F>, masters: Vec<F>, profile: Option<Vec<u8>>, entries: &[(&str, &[u8], Method)]) -> Vec<u8> {
    let spread =
        F::Obj(tag(b"Sprd"), vec![(tag(b"SprB"), F::F64s(vec![0., 0., 64., 64.])), (tag(b"SprT"), F::Bool(true)), (tag(b"Chld"), F::Shared(children))]);
    let mut root = vec![(tag(b"DocR"), F::Obj(tag(b"DocN"), vec![(tag(b"MpCh"), F::Shared(masters)), (tag(b"Chld"), F::Shared(vec![spread]))]))];
    if let Some(profile) = profile {
        let mut blob = vec![0x2d];
        blob.extend(tag(b"Data").0.to_le_bytes());
        blob.extend((profile.len() as u32).to_le_bytes());
        blob.extend(profile);
        root.push((tag(b"CMYP"), F::Obj(tag(b"Prof"), vec![(tag(b"Data"), F::Raw(blob))])));
    }
    let mut next = 10000;
    for (_, value) in &mut root {
        text_fixture::shared_objects(value, &mut next);
    }
    let stream = synth::stream(&root);
    let mut all = vec![("doc.dat", stream.as_slice(), Method::Zlib)];
    all.extend_from_slice(entries);
    synth::container(&all, None)
}
fn bitmap(width: i32, height: i32, format: u16, values: &[u8]) -> F {
    let mut fields = vec![(tag(b"BmpW"), F::I32(width)), (tag(b"BmpH"), F::I32(height)), (tag(b"Frmt"), F::Enum(format, 0))];
    for (index, value) in values.iter().enumerate() {
        fields.push((tag(&[b'S', b't', b'a', b'1' + index as u8]), F::U8s(vec![*value])));
    }
    F::Obj(tag(b"Rstr"), vec![(tag(b"Desc"), F::Str("Pixels".into())), (tag(b"Bitm"), F::Obj(tag(b"DyBm"), fields))])
}
fn embedded(entry: &str) -> F {
    F::Obj(
        tag(b"EmbN"),
        vec![
            (tag(b"Xfrm"), F::F64s(vec![1., 0., 32., 0., 1., 32.])),
            (tag(b"Bitm"), F::Obj(tag(b"EmBm"), vec![(tag(b"EmCn"), F::Obj(tag(b"EmCn"), vec![(tag(b"EmbC"), F::Entry(entry.into()))]))])),
        ],
    )
}
#[test]
fn editable_decorations_paragraphs_and_mixed_outlines_retain_layout() {
    let mut styled = text_fixture::attrs("Inter", "", 400, false, 16.);
    if let F::Obj(_, fields) = &mut styled {
        fields.iter_mut().find(|(tag_, _)| *tag_ == tag(b"Ints")).unwrap().1 = text_fixture::array_i32(b"Ints", &[1, 1]);
        fields.iter_mut().find(|(tag_, _)| *tag_ == tag(b"Objs")).unwrap().1 =
            F::Shared(vec![text_fixture::solid(1.), text_fixture::solid(1.), F::Obj(tag(b"LSty"), vec![(tag(b"Wght"), F::F64(3.))])]);
    }
    let bytes = text_fixture::document(
        vec![text_fixture::block(
            text_fixture::utf8("Wide\nplain"),
            vec![text_fixture::run(4, Some(styled)), text_fixture::run(10, Some(text_fixture::attrs("Inter", "", 400, false, 16.)))],
            &[0],
        )],
        false,
        72.,
        text_fixture::IDENTITY,
    );
    let native = photocraft_affinity::read(&bytes, Limits::default()).unwrap();
    let model::Kind::Text(text) = &native.spreads[0].nodes[0].kind else { panic!() };
    assert!(text.runs[0].underline && text.runs[0].strikethrough && text.runs[0].stroke.is_some());
    let scene = import(&bytes, &ImportOptions::default()).unwrap();
    let layers = walk(&scene.layers);
    let text = layers.iter().find_map(|l| if let LayerContent::Text(t) = &l.content { Some(t) } else { None }).unwrap();
    assert_eq!(text.text, "Wide\nplain");
    assert!(text.runs[0].style.underline && text.runs[0].style.strikethrough);
    assert!(layers.iter().any(|l| l.name == "Text outline" && matches!(l.content, LayerContent::Shape(_))));
    assert!(!text.runs[1].style.underline);
}
#[test]
fn embedded_pixels_share_parent_pixel_and_archive_budgets() {
    let inner = container(vec![bitmap(8, 8, 0, &[2, 0, 0, 2])], vec![], None, &[]);
    let mut data = b"EmDc\0\0\0\0".to_vec();
    data.extend(inner);
    let bytes = container(vec![embedded("embedded"), bitmap(8, 8, 0, &[2, 0, 0, 2])], vec![], None, &[("embedded", &data, Method::Zlib)]);
    assert!(matches!(photocraft_affinity::read(&bytes, Limits { max_pixels: 150, ..Default::default() }), Err(Error::Limit("decoded pixel count"))));
    let doc = photocraft_affinity::read(&bytes, Limits::default()).unwrap();
    assert!(matches!(doc.spreads[0].nodes[0].children[0].kind, model::Kind::Image(_)));
    assert_eq!(doc.spreads[0].nodes[0].children[0].class, tag(b"Rstr"));
    let scene = import(&bytes, &ImportOptions::default()).unwrap();
    assert_eq!(walk(&scene.layers).iter().filter(|l| matches!(l.content, LayerContent::Raster(_))).count(), 2);
    let limits = Limits { max_total: data.len(), ..Default::default() };
    assert!(matches!(photocraft_affinity::read(&bytes, limits), Err(Error::Limit(_))));
    let mut bad = data.clone();
    bad[88] ^= 1;
    let bad = container(vec![embedded("bad"), bitmap(2, 2, 0, &[2, 0, 0, 2])], vec![], None, &[("bad", &bad, Method::Zlib)]);
    assert!(matches!(photocraft_affinity::read(&bad, Limits::default()), Err(Error::Malformed(_))));
}
#[test]
fn master_instances_expand_with_unique_ids_and_bounded_copies() {
    let master = F::Def(42, vec![tag(b"Sprd")], vec![(tag(b"SprB"), F::F64s(vec![0., 0., 64., 64.])), (tag(b"Chld"), F::Shared(vec![curve()]))]);
    let instance = || F::Obj(tag(b"MPIN"), vec![(tag(b"SLnk"), F::Obj(tag(b"Link"), vec![(tag(b"ILOb"), F::Shared(vec![F::Ref(42)]))]))]);
    let bytes = container(vec![instance(), instance()], vec![master], None, &[]);
    let scene = import(&bytes, &ImportOptions::default()).unwrap();
    let layers = walk(&scene.layers);
    let ids: std::collections::HashSet<_> = layers.iter().map(|l| l.id).collect();
    assert_eq!(ids.len(), layers.len());
    assert_eq!(layers.iter().filter(|l| matches!(l.content, LayerContent::Shape(_))).count(), 2);
    let mut options = ImportOptions::default();
    options.limits.objects = 3;
    assert!(matches!(import(&bytes, &options), Err(Error::Limit(_))));
    let mut embedded_data = b"EmDc\0\0\0\0".to_vec();
    embedded_data.extend(bytes);
    let outer = container(vec![embedded("inner")], vec![], None, &[("inner", &embedded_data, Method::Zlib)]);
    let doc = photocraft_affinity::read(&outer, Limits::default()).unwrap();
    assert!(doc.warnings.iter().any(|w| w.contains("master pages of embedded documents")));
    assert!(doc.spreads[0].nodes[0].children.iter().all(|n| !matches!(n.kind, model::Kind::MasterInstance { .. })));
}
#[test]
fn cmyk_profile_and_coated_fallback_match_the_cms_transform() {
    let profile = photocraft_cms::Builtin::CoatedCmyk.profile();
    let encoded = profile.to_bytes().as_ref().clone();
    let transform =
        photocraft_cms::Transform::new(profile, photocraft_cms::Builtin::Srgb.profile(), photocraft_cms::Intent::RelativeColorimetric, true).unwrap();
    let mut expected = [0.; 3];
    transform.eval(&[1., 0., 0., 0.], &mut expected);
    for profile in [None, Some(encoded)] {
        let bytes = container(vec![bitmap(2, 2, 4, &[2, 0, 0, 0, 2]), bitmap(2, 2, 4, &[2, 0, 0, 0, 0])], vec![], profile.clone(), &[]);
        let scene = import(&bytes, &ImportOptions::default()).unwrap();
        let layers = walk(&scene.layers);
        let surfaces: Vec<_> = layers.iter().filter_map(|l| l.surface()).collect();
        let actual = surfaces[0].rgba(0, 0);
        for i in 0..3 {
            assert!((actual[i] - expected[i]).abs() <= 1. / 255., "embedded={} actual={actual:?} expected={expected:?}", profile.is_some());
        }
        assert_eq!(actual[3], 1.);
        assert_eq!(surfaces[1].rgba(0, 0)[3], 0.);
        assert_eq!(scene.warnings.iter().any(|w| w.message.contains("built-in coated")), profile.is_none());
        assert!((actual[1] - 1.).abs() > 0.02 || (actual[2] - 1.).abs() > 0.02);
    }
    let bytes = container(vec![bitmap(2, 2, 4, &[2, 0, 0, 0, 2])], vec![], Some(vec![0; 128]), &[]);
    assert!(matches!(import(&bytes, &ImportOptions::default()), Err(Error::Malformed("invalid CMYK profile"))));
}
#[test]
fn tables_keep_cell_text_and_ignore_mismatched_formatting() {
    let table = |lines: usize| {
        F::Obj(
            tag(b"TxtT"),
            vec![
                (
                    tag(b"StSt"),
                    F::Obj(tag(b"Stry"), vec![(tag(b"Blok"), F::Shared(vec![text_fixture::block(text_fixture::utf8("A\u{1f}B\u{1f}"), vec![], &[])]))]),
                ),
                (
                    tag(b"TxtH"),
                    F::Obj(
                        tag(b"TabH"),
                        vec![(
                            tag(b"Tabl"),
                            F::Obj(
                                tag(b"Tabl"),
                                vec![
                                    (
                                        tag(b"CPos"),
                                        F::Obj(
                                            tag(b"Posn"),
                                            vec![(tag(b"Posn"), text_fixture::array_f64(b"Posn", &(0..lines).map(|v| v as f64 * 20.).collect::<Vec<_>>()))],
                                        ),
                                    ),
                                    (tag(b"RPos"), F::Obj(tag(b"Posn"), vec![(tag(b"Posn"), text_fixture::array_f64(b"Posn", &[0., 30.]))])),
                                    (
                                        tag(b"Cell"),
                                        F::Obj(tag(b"Cell"), vec![(tag(b"Cell"), F::Shared(vec![F::Obj(tag(b"Cell"), vec![(tag(b"BrLf"), F::Bool(true))])]))]),
                                    ),
                                ],
                            ),
                        )],
                    ),
                ),
            ],
        )
    };
    let bytes = container(vec![table(3)], vec![], None, &[]);
    let doc = photocraft_affinity::read(&bytes, Limits::default()).unwrap();
    let model::Kind::Table(table_) = &doc.spreads[0].nodes[0].kind else { panic!() };
    assert_eq!(table_.cells[0].runs[0].text, "A");
    assert_eq!(table_.cells[1].runs[0].text, "B");
    assert!(table_.cells.iter().all(|c| !c.merged_left));
    assert!(doc.warnings.iter().any(|w| w.contains("cells don't match")));
    let scene = import(&bytes, &ImportOptions::default()).unwrap();
    let layers = walk(&scene.layers);
    assert_eq!(layers.iter().filter(|l| matches!(l.content, LayerContent::Text(_))).count(), 2);
    let doc = photocraft_affinity::read(&container(vec![table(4098)], vec![], None, &[]), Limits::default()).unwrap();
    assert!(matches!(doc.spreads[0].nodes[0].kind, model::Kind::Unsupported));
}

#[path = "../support/affinity_docs.rs"]
mod affinity_docs;
#[test]
fn native_effect_records_keep_hierarchy_shadow_axes_and_editable_bevel() {
    let bytes = affinity_docs::EFFECTS;
    let native = photocraft_affinity::read(bytes, Limits::default()).unwrap();
    assert!(matches!(native.spreads[0].nodes[0].effects[0], model::Effect::ColorOverlay { .. }));
    assert!(matches!(native.spreads[0].nodes[1].effects[0], model::Effect::DropShadow { distance: 8., .. }));
    let scene = import(bytes, &ImportOptions::default()).unwrap();
    let layers = walk(&scene.layers);
    let parent = layers.iter().find(|l| l.name == "Parent red").unwrap();
    let child = layers.iter().find(|l| l.name == "Child blue").unwrap();
    assert!(parent.effects.enabled && child.effects.enabled);
    let shadows: Vec<_> = layers
        .iter()
        .filter_map(|l| l.effects.items.first())
        .filter_map(|e| if let photocraft_doc::effects::Effect::DropShadow(s) = e { Some(s) } else { None })
        .collect();
    assert_eq!(shadows.len(), 2);
    assert!((shadows[0].angle - 90.).abs() < 1e-5);
    assert_eq!(shadows[1].angle, 180.);
    assert!(shadows.iter().all(|s| !s.use_global_light));
    assert!(scene.warnings.iter().any(|w| w.feature == "bevel-approximation"));
}

#[test]
fn checksum_failures_keep_their_archive_charge_for_the_next_entry() {
    let mut bytes = synth::container(&[("bad", b"1234567890", Method::Stored), ("good", b"abcdefghij", Method::Stored)], None);
    bytes[76] ^= 1;
    let mut archive = photocraft_affinity::Archive::open(&bytes, Limits { max_total: 15, ..Default::default() }).unwrap();
    assert!(matches!(archive.read("bad"), Err(Error::Malformed("archive entry checksum mismatch"))));
    assert_eq!(archive.extracted(), 10);
    assert!(matches!(archive.read("good"), Err(Error::Limit("document too large"))));
}

#[test]
fn paragraph_styles_and_forced_line_breaks_keep_distinct_layout_ranges() {
    let mut block =
        text_fixture::block(text_fixture::utf8("A\u{3}B\nC"), vec![text_fixture::run(5, Some(text_fixture::attrs("Inter", "", 400, false, 16.)))], &[]);
    if let F::Obj(_, fields) = &mut block {
        let paragraph = |end, align, spacing| {
            vec![
                (tag(b"Indx"), F::I32(end)),
                (
                    tag(b"Item"),
                    F::Obj(
                        tag(b"PAtt"),
                        vec![
                            (tag(b"Ints"), text_fixture::array_i32(b"Ints", &[align])),
                            (tag(b"Doub"), text_fixture::array_f64(b"Doub", &[1.2, 0., 0., 0., 0., spacing, 4.])),
                        ],
                    ),
                ),
            ]
        };
        fields.iter_mut().find(|(key, _)| *key == tag(b"PAtt")).unwrap().1 =
            F::Obj(tag(b"PaAS"), vec![(tag(b"Runs"), F::Objs(tag(b"PaAR"), vec![paragraph(4, 0, 0.), paragraph(5, 2, 6.)]))]);
    }
    let bytes = text_fixture::document(vec![block], false, 72., text_fixture::IDENTITY);
    let scene = import(&bytes, &ImportOptions::default()).unwrap();
    let layers = walk(&scene.layers);
    let text = layers.iter().find_map(|l| if let LayerContent::Text(t) = &l.content { Some(t) } else { None }).unwrap();
    assert_eq!(text.text, "A\u{3}B\nC");
    assert_eq!(text.paragraphs.iter().map(|p| p.len).collect::<Vec<_>>(), [4, 1]);
    assert_eq!(text.paragraphs[1].style.align, photocraft_doc::text::TextAlign::Right);
    assert_eq!(text.paragraphs[1].style.space_before_pt, 6.);
    let layout = photocraft_text::TextEngine::new().layout(text, 72.);
    assert_eq!(layout.lines.len(), 3);
    assert_eq!(layout.lines.iter().map(|l| l.paragraph).collect::<Vec<_>>(), [0, 0, 1]);
}

#[test]
fn centered_table_text_overflows_short_cells_without_losing_lines() {
    let table = F::Obj(
        tag(b"TxtT"),
        vec![
            (
                tag(b"StSt"),
                F::Obj(
                    tag(b"Stry"),
                    vec![(
                        tag(b"Blok"),
                        F::Shared(vec![text_fixture::block(
                            text_fixture::utf8("A\nB"),
                            vec![text_fixture::run(3, Some(text_fixture::attrs("Inter", "", 400, false, 16.)))],
                            &[0],
                        )]),
                    )],
                ),
            ),
            (
                tag(b"TxtH"),
                F::Obj(
                    tag(b"TabH"),
                    vec![(
                        tag(b"Tabl"),
                        F::Obj(
                            tag(b"Tabl"),
                            vec![
                                (tag(b"CPos"), F::Obj(tag(b"Posn"), vec![(tag(b"Posn"), text_fixture::array_f64(b"Posn", &[10., 40.]))])),
                                (tag(b"RPos"), F::Obj(tag(b"Posn"), vec![(tag(b"Posn"), text_fixture::array_f64(b"Posn", &[25., 27.]))])),
                                (
                                    tag(b"Cell"),
                                    F::Obj(tag(b"Cell"), vec![(tag(b"Cell"), F::Shared(vec![F::Obj(tag(b"Cell"), vec![(tag(b"AliY"), F::Enum(1, 0))])]))]),
                                ),
                            ],
                        ),
                    )],
                ),
            ),
        ],
    );
    let scene = import(&container(vec![table], vec![], None, &[]), &ImportOptions::default()).unwrap();
    let layers = walk(&scene.layers);
    let text = layers.iter().find_map(|l| if let LayerContent::Text(t) = &l.content { Some(t) } else { None }).unwrap();
    let layout = photocraft_text::TextEngine::new().layout(text, 72.);
    assert_eq!(layout.lines.len(), 2);
    assert!(text.transform.m[5] < 25., "centered overflow must extend above the cell");
    let cache = text.cache.as_ref().unwrap();
    assert!(cache.content_bounds().y0 < 25 && cache.content_bounds().y1 > 27);
}

#[test]
fn repeated_tiny_embedded_documents_share_the_node_allowance() {
    let inner = container(vec![bitmap(1, 1, 0, &[2, 0, 0, 2])], vec![], None, &[]);
    let mut data = b"EmDc\0\0\0\0".to_vec();
    data.extend(inner);
    let F::Obj(class, fields) = embedded("inner") else { panic!() };
    let mut nodes = vec![F::Def(42, vec![class], fields)];
    nodes.extend((0..7).map(|_| F::Ref(42)));
    let bytes = container(nodes, vec![], None, &[("inner", &data, Method::Stored)]);
    assert!(matches!(photocraft_affinity::read(&bytes, Limits { max_objects: 12, ..Default::default() }), Err(Error::Limit("too many layers"))));
}

#[test]
fn table_cell_product_is_capped_before_expanding_cells() {
    let positions = (0..258).map(|v| v as f64).collect::<Vec<_>>();
    let table = F::Obj(
        tag(b"TxtT"),
        vec![
            (tag(b"StSt"), F::Obj(tag(b"Stry"), vec![])),
            (
                tag(b"TxtH"),
                F::Obj(
                    tag(b"TabH"),
                    vec![(
                        tag(b"Tabl"),
                        F::Obj(
                            tag(b"Tabl"),
                            vec![
                                (tag(b"CPos"), F::Obj(tag(b"Posn"), vec![(tag(b"Posn"), text_fixture::array_f64(b"Posn", &positions))])),
                                (tag(b"RPos"), F::Obj(tag(b"Posn"), vec![(tag(b"Posn"), text_fixture::array_f64(b"Posn", &positions))])),
                            ],
                        ),
                    )],
                ),
            ),
        ],
    );
    let doc = photocraft_affinity::read(&container(vec![table], vec![], None, &[]), Limits::default()).unwrap();
    assert!(matches!(doc.spreads[0].nodes[0].kind, model::Kind::Unsupported));
    assert!(doc.warnings.iter().any(|w| w.contains("more than 65536 cells")));
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(64))]
    #[test]
    fn af5_stream_mutations_stay_bounded_without_panicking(edits in proptest::collection::vec((proptest::prelude::any::<usize>(), proptest::prelude::any::<u8>()), 0..12)) {
        let mut archive = photocraft_affinity::Archive::open(affinity_docs::EFFECTS, Limits::default()).unwrap();
        let mut stream = archive.read("doc.dat").unwrap();
        for (index, value) in edits { let len = stream.len(); stream[index % len] = value; }
        let bytes = synth::container(&[("doc.dat", &stream, Method::Stored)], None);
        let mut options = ImportOptions::default(); options.limits.decompressed_bytes = 16 << 20; options.limits.objects = 2000; options.limits.pixels = 1_000_000;
        let _result = import(&bytes, &options);
    }
}

#[test]
fn outlined_artistic_text_keeps_full_mask_and_node_shadow() {
    let scene = import(affinity_docs::TEXT_EFFECTS, &ImportOptions::default()).unwrap();
    let layers = walk(&scene.layers);
    let group = layers.iter().find(|l| l.name == "Masked outlined text").unwrap();
    assert!(matches!(group.content, LayerContent::Group(_)));
    assert!(group.mask.is_some());
    assert!(matches!(group.effects.items[0], photocraft_doc::effects::Effect::DropShadow(_)));
    let text = layers.iter().find_map(|l| if let LayerContent::Text(t) = &l.content { Some(t) } else { None }).unwrap();
    assert_eq!(text.text, "AB");
    let outline = layers.iter().find(|l| l.name == "Text outline").unwrap().surface().unwrap();
    let ink = text.cache.as_ref().unwrap().content_bounds();
    let over = outline.content_bounds();
    assert!(over.x0 < ink.x0 || over.y0 < ink.y0 || over.x1 > ink.x1 || over.y1 > ink.y1);
    assert_eq!(group.mask.as_ref().unwrap().surface.content_bounds(), photocraft_doc::Rect::new(0, 0, 128, 128));
}

#[test]
fn hostile_effect_geometry_never_publishes_a_scene() {
    for (field, value) in [(b"Radi", -1.), (b"Radi", 513.), (b"Radi", f64::NAN), (b"Angl", 1e300)] {
        let mut shape = curve();
        if let F::Def(_, _, fields) = &mut shape {
            fields.push((
                tag(b"FiEf"),
                F::Shared(vec![F::Obj(
                    tag(b"Shad"),
                    vec![
                        (
                            tag(b"Colr"),
                            F::Obj(tag(b"RGBA"), vec![(tag(b"_col"), F::Struct([0f32, 0., 0., 1.].into_iter().flat_map(f32::to_le_bytes).collect()))]),
                        ),
                        (tag(field), F::F64(value)),
                    ],
                )]),
            ));
        }
        assert!(matches!(import(&document(vec![shape]), &ImportOptions::default()), Err(Error::Limit(_) | Error::Malformed(_))));
    }
}

#[test]
fn native_text_metadata_is_reserved_before_layout_clones() {
    let attrs = text_fixture::attrs(&"F".repeat(16 * 1024), "", 400, false, 12.);
    let runs = (1..=64).map(|end| text_fixture::run(end, Some(attrs.clone()))).collect();
    let bytes = text_fixture::document(vec![text_fixture::block(text_fixture::utf8(&"A".repeat(64)), runs, &[0])], false, 72., text_fixture::IDENTITY);
    let mut options = ImportOptions::default();
    options.limits.decompressed_bytes = 8 << 20;
    assert!(matches!(import(&bytes, &options), Err(Error::Limit("text layout bytes"))));
}
