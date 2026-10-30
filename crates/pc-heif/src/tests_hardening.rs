use super::*;

#[derive(Clone)]
struct Extent {
    index: u32,
    offset: u32,
    length: u32,
}

struct Location {
    id: u16,
    method: u16,
    extents: Vec<Extent>,
}

fn boxed(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 8);
    out.extend_from_slice(&((payload.len() + 8) as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(payload);
    out
}

fn full_box(kind: &[u8; 4], version: u8, flags: u32, payload: &[u8]) -> Vec<u8> {
    let mut body = vec![version, (flags >> 16) as u8, (flags >> 8) as u8, flags as u8];
    body.extend_from_slice(payload);
    boxed(kind, &body)
}

fn heif(children: &[Vec<u8>]) -> Vec<u8> {
    let mut file = boxed(b"ftyp", b"heic\0\0\0\0mif1heic");
    let mut meta = vec![0; 4];
    for child in children {
        meta.extend_from_slice(child);
    }
    file.extend(boxed(b"meta", &meta));
    file
}

fn hdlr() -> Vec<u8> {
    let mut payload = vec![0; 4];
    payload.extend_from_slice(b"pict");
    payload.extend_from_slice(&[0; 12]);
    full_box(b"hdlr", 0, 0, &payload)
}

fn pitm(id: u16) -> Vec<u8> {
    full_box(b"pitm", 0, 0, &id.to_be_bytes())
}

fn infe(id: u16, kind: &[u8; 4], content_type: Option<&[u8]>) -> Vec<u8> {
    let mut payload = id.to_be_bytes().to_vec();
    payload.extend_from_slice(&[0, 0]);
    payload.extend_from_slice(kind);
    payload.extend_from_slice(b"item\0");
    if let Some(content_type) = content_type {
        payload.extend_from_slice(content_type);
        payload.push(0);
    }
    full_box(b"infe", 2, 0, &payload)
}

fn iinf(items: &[Vec<u8>]) -> Vec<u8> {
    let mut payload = (items.len() as u16).to_be_bytes().to_vec();
    for item in items {
        payload.extend_from_slice(item);
    }
    full_box(b"iinf", 0, 0, &payload)
}

fn iprp(properties: &[Vec<u8>], associations: &[(u16, Vec<u8>)]) -> Vec<u8> {
    let mut ipco = Vec::new();
    for property in properties {
        ipco.extend_from_slice(property);
    }
    let mut children = boxed(b"ipco", &ipco);
    let mut ipma = (associations.len() as u32).to_be_bytes().to_vec();
    for (id, indices) in associations {
        ipma.extend_from_slice(&id.to_be_bytes());
        ipma.push(indices.len() as u8);
        ipma.extend_from_slice(indices);
    }
    children.extend(full_box(b"ipma", 0, 0, &ipma));
    boxed(b"iprp", &children)
}

fn iloc(locations: &[Location], offset_size: u8, length_size: u8, index_size: u8) -> Vec<u8> {
    let mut payload = vec![(offset_size << 4) | length_size, index_size];
    payload.extend_from_slice(&(locations.len() as u16).to_be_bytes());
    for location in locations {
        payload.extend_from_slice(&location.id.to_be_bytes());
        payload.extend_from_slice(&location.method.to_be_bytes());
        payload.extend_from_slice(&[0, 0]);
        payload.extend_from_slice(&(location.extents.len() as u16).to_be_bytes());
        for extent in &location.extents {
            push_uint(&mut payload, extent.index, index_size);
            push_uint(&mut payload, extent.offset, offset_size);
            push_uint(&mut payload, extent.length, length_size);
        }
    }
    full_box(b"iloc", 1, 0, &payload)
}

fn push_uint(out: &mut Vec<u8>, value: u32, width: u8) {
    for index in (0..usize::from(width)).rev() {
        out.push((u64::from(value) >> (index * 8)) as u8);
    }
}

fn iref(kind: &[u8; 4], from: u16, to: &[u16]) -> Vec<u8> {
    iref_relations(&[(kind, from, to)])
}

fn iref_relations(references: &[(&[u8; 4], u16, &[u16])]) -> Vec<u8> {
    let mut children = Vec::new();
    for (kind, from, to) in references {
        let mut reference = from.to_be_bytes().to_vec();
        reference.extend_from_slice(&(to.len() as u16).to_be_bytes());
        for id in *to {
            reference.extend_from_slice(&id.to_be_bytes());
        }
        children.extend(boxed(kind, &reference));
    }
    full_box(b"iref", 0, 0, &children)
}

fn idat(bytes: &[u8]) -> Vec<u8> {
    boxed(b"idat", bytes)
}

fn grid(rows: u16, columns: u16, width: u16, height: u16) -> Vec<u8> {
    let mut out = vec![0, 0, (rows - 1) as u8, (columns - 1) as u8];
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&height.to_be_bytes());
    out
}

fn property(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    boxed(kind, payload)
}

fn hvc_c() -> Vec<u8> {
    let mut config = vec![0; 23];
    config[0] = 1;
    config[16] = 1;
    config[21] = 3;
    property(b"hvcC", &config)
}

fn ispe(width: u32, height: u32) -> Vec<u8> {
    let mut payload = vec![0; 4];
    payload.extend_from_slice(&width.to_be_bytes());
    payload.extend_from_slice(&height.to_be_bytes());
    property(b"ispe", &payload)
}

fn xmp_with_source_extents(source_extent_count: usize) -> Vec<u8> {
    let mut source_extents = Vec::with_capacity(source_extent_count);
    for _ in 0..source_extent_count {
        source_extents.push(Extent { index: 0, offset: 0, length: 1 });
    }
    heif(&[
        hdlr(),
        pitm(1),
        iinf(&[infe(1, b"hvc1", None), infe(2, b"mime", Some(b"application/rdf+xml")), infe(3, b"hvc1", None)]),
        iref(b"cdsc", 2, &[1]),
        iloc(
            &[Location { id: 2, method: 2, extents: vec![Extent { index: 3, offset: 0, length: 0 }] }, Location { id: 3, method: 1, extents: source_extents }],
            0,
            4,
            4,
        ),
        idat(b"x"),
    ])
}

fn repeated_tile_grid(rows: u16, columns: u16) -> Vec<u8> {
    let mut idat_bytes = grid(rows, columns, columns, rows);
    idat_bytes.extend_from_slice(&[0; 3]);
    let tile_ids = vec![2; usize::from(rows) * usize::from(columns)];
    heif(&[
        hdlr(),
        pitm(1),
        iinf(&[infe(1, b"grid", None), infe(2, b"hvc1", None)]),
        iref(b"dimg", 1, &tile_ids),
        iloc(
            &[
                Location { id: 1, method: 1, extents: vec![Extent { index: 0, offset: 0, length: 8 }] },
                Location { id: 2, method: 1, extents: vec![Extent { index: 0, offset: 8, length: 3 }] },
            ],
            4,
            4,
            0,
        ),
        iprp(&[hvc_c(), ispe(1, 1)], &[(2, vec![1, 2])]),
        idat(&idat_bytes),
    ])
}

fn primary_grid_with_many_extents(extent_count: usize) -> Vec<u8> {
    let mut descriptor = grid(1, 1, 1, 1);
    descriptor.push(0);
    let descriptor_extents = (0..extent_count).map(|_| Extent { index: 3, offset: 0, length: 8 }).collect();
    heif(&[
        hdlr(),
        pitm(1),
        iinf(&[infe(1, b"grid", None), infe(2, b"hvc1", None), infe(3, b"hvc1", None)]),
        iref(b"dimg", 1, &[2]),
        iloc(
            &[
                Location { id: 1, method: 2, extents: descriptor_extents },
                Location { id: 2, method: 1, extents: vec![Extent { index: 0, offset: 8, length: 1 }] },
                Location { id: 3, method: 1, extents: vec![Extent { index: 0, offset: 0, length: 8 }] },
            ],
            4,
            4,
            4,
        ),
        iprp(&[hvc_c(), ispe(1, 1)], &[(2, vec![1, 2])]),
        idat(&descriptor),
    ])
}

fn alpha_with_many_extents(extent_count: usize) -> Vec<u8> {
    let alpha = full_box(b"auxC", 0, 0, b"urn:mpeg:hevc:2015:auxid:1\0");
    let mut alpha_extents = Vec::with_capacity(extent_count);
    for _ in 0..extent_count {
        alpha_extents.push(Extent { index: 3, offset: 0, length: 1 });
    }
    heif(&[
        hdlr(),
        pitm(1),
        iinf(&[infe(1, b"hvc1", None), infe(2, b"hvc1", None), infe(3, b"hvc1", None)]),
        iref(b"auxl", 2, &[1]),
        iloc(
            &[
                Location { id: 1, method: 1, extents: vec![Extent { index: 0, offset: 0, length: 1 }] },
                Location { id: 2, method: 2, extents: alpha_extents },
                Location { id: 3, method: 1, extents: vec![Extent { index: 0, offset: 1, length: 1 }] },
            ],
            4,
            4,
            4,
        ),
        iprp(&[hvc_c(), ispe(1, 1), alpha], &[(1, vec![1, 2]), (2, vec![1, 2, 3])]),
        idat(b"xy"),
    ])
}

fn alpha_grid_with_tile_extents(extent_count: usize) -> Vec<u8> {
    let alpha = full_box(b"auxC", 0, 0, b"urn:mpeg:hevc:2015:auxid:1\0");
    let mut descriptor = grid(1, 1, 1, 1);
    descriptor.push(0);
    let tile_extents = vec![Extent { index: 0, offset: 0, length: 0 }; extent_count];
    heif(&[
        hdlr(),
        pitm(1),
        iinf(&[infe(1, b"hvc1", None), infe(2, b"grid", None), infe(3, b"hvc1", None)]),
        iref_relations(&[(b"auxl", 2, &[1]), (b"dimg", 2, &[3])]),
        iloc(
            &[
                Location { id: 1, method: 1, extents: vec![Extent { index: 0, offset: 0, length: 0 }] },
                Location { id: 2, method: 1, extents: vec![Extent { index: 0, offset: 0, length: 0 }] },
                Location { id: 3, method: 1, extents: tile_extents },
            ],
            0,
            0,
            0,
        ),
        iprp(&[hvc_c(), ispe(1, 1), alpha], &[(1, vec![1, 2]), (2, vec![2, 3]), (3, vec![1, 2])]),
        idat(&descriptor),
    ])
}

fn overlapping_tile_grid(tile_bytes: usize) -> Vec<u8> {
    let mut idat_bytes = grid(1, 2, 2, 1);
    idat_bytes.extend(vec![0x5a; tile_bytes]);
    heif(&[
        hdlr(),
        pitm(1),
        iinf(&[infe(1, b"grid", None), infe(2, b"hvc1", None), infe(3, b"hvc1", None)]),
        iref(b"dimg", 1, &[2, 3]),
        iloc(
            &[
                Location { id: 1, method: 1, extents: vec![Extent { index: 0, offset: 0, length: 8 }] },
                Location { id: 2, method: 1, extents: vec![Extent { index: 0, offset: 8, length: tile_bytes as u32 }] },
                Location { id: 3, method: 1, extents: vec![Extent { index: 0, offset: 8, length: tile_bytes as u32 }] },
            ],
            4,
            4,
            0,
        ),
        iprp(&[hvc_c(), ispe(1, 1)], &[(2, vec![1, 2]), (3, vec![1, 2])]),
        idat(&idat_bytes),
    ])
}

fn primary_with_dimensions(width: u32, height: u32) -> Vec<u8> {
    primary_with_dimensions_and_alpha(width, height, false)
}

fn primary_with_dimensions_and_alpha(width: u32, height: u32, has_alpha: bool) -> Vec<u8> {
    let alpha = full_box(b"auxC", 0, 0, b"urn:mpeg:hevc:2015:auxid:1\0");
    let mut items = vec![infe(1, b"hvc1", None)];
    let mut references = Vec::new();
    let mut locations = vec![Location { id: 1, method: 1, extents: vec![Extent { index: 0, offset: 0, length: 1 }] }];
    let mut properties = vec![hvc_c(), ispe(width, height)];
    let mut associations = vec![(1, vec![1, 2])];
    if has_alpha {
        items.push(infe(2, b"hvc1", None));
        references.push(iref(b"auxl", 2, &[1]));
        locations.push(Location { id: 2, method: 1, extents: vec![Extent { index: 0, offset: 1, length: 1 }] });
        properties.push(alpha);
        associations.push((2, vec![1, 2, 3]));
    }
    let mut children = vec![hdlr(), pitm(1), iinf(&items)];
    children.extend(references);
    children.extend([iloc(&locations, 4, 4, 0), iprp(&properties, &associations), idat(b"xy")]);
    heif(&children)
}

fn primary_with_wide_extents(extent_count: usize) -> Vec<u8> {
    heif(&[
        hdlr(),
        pitm(1),
        iinf(&[infe(1, b"hvc1", None)]),
        iloc(&[Location { id: 1, method: 1, extents: vec![Extent { index: 0, offset: 0, length: 1 }; extent_count] }], 8, 8, 0),
        iprp(&[hvc_c(), ispe(1, 1)], &[(1, vec![1, 2])]),
        idat(b"x"),
    ])
}

fn primary_with_over_budget_zero_width_extents() -> Vec<u8> {
    let empty_extent = Extent { index: 0, offset: 0, length: 0 };
    let locations = [
        Location { id: 1, method: 1, extents: vec![empty_extent.clone()] },
        Location { id: 2, method: 1, extents: vec![empty_extent.clone(); 32_768] },
        Location { id: 3, method: 1, extents: vec![empty_extent; 32_768] },
    ];
    heif(&[
        hdlr(),
        pitm(1),
        iinf(&[infe(1, b"hvc1", None), infe(2, b"hvc1", None), infe(3, b"hvc1", None)]),
        iloc(&locations, 0, 0, 0),
        iprp(&[hvc_c(), ispe(1, 1)], &[(1, vec![1, 2])]),
        idat(b"x"),
    ])
}

fn primary_with_over_budget_iloc_items() -> Vec<u8> {
    let mut payload = vec![0, 0];
    payload.extend_from_slice(&65_537_u32.to_be_bytes());
    for id in 2..=65_538_u32 {
        payload.extend_from_slice(&id.to_be_bytes());
        payload.extend_from_slice(&[0; 6]);
    }
    let locations = full_box(b"iloc", 2, 0, &payload);
    heif(&[hdlr(), pitm(1), iinf(&[infe(1, b"hvc1", None)]), locations, iprp(&[hvc_c(), ispe(1, 1)], &[(1, vec![1, 2])]), idat(b"x")])
}

#[test]
fn ipma_payloads_over_the_cap_are_refused_and_near_cap_are_accepted() {
    let base = [hdlr(), pitm(1), iinf(&[infe(1, b"hvc1", None)]), iprp(&[hvc_c(), ispe(1, 1)], &[(1, vec![1, 2])])];

    let mut associations = vec![0, 0, 0, 0];
    associations.extend_from_slice(&1u32.to_be_bytes());
    associations.extend_from_slice(&1u16.to_be_bytes());
    associations.extend_from_slice(&[2, 1, 2]);
    associations.resize((64 << 10) + 1, 0);
    let mut properties = Vec::new();
    properties.extend_from_slice(&hvc_c());
    properties.extend_from_slice(&ispe(1, 1));
    let mut iprp_children = boxed(b"ipco", &properties);
    iprp_children.extend(boxed(b"ipma", &associations));
    let oversized_ipma = boxed(b"iprp", &iprp_children);

    let mut children = base.to_vec();
    children[3] = oversized_ipma;
    let result = probe(&heif(&children));
    assert!(matches!(&result, Err(Error::Limit(message)) if message.contains("associations")), "{result:?}");

    let mut associations = Vec::with_capacity(21_841);
    associations.push((1, vec![1, 2]));
    associations.extend((1..21_841).map(|_| (1, Vec::new())));
    let under_cap = heif(&[
        hdlr(),
        pitm(1),
        iinf(&[infe(1, b"hvc1", None)]),
        iprp(&[hvc_c(), ispe(1, 1)], &associations),
        iloc(&[Location { id: 1, method: 1, extents: vec![Extent { index: 0, offset: 0, length: 1 }] }], 4, 4, 0),
        idat(b"x"),
    ]);
    assert!(matches!(probe(&under_cap), Ok(Info { width: 1, height: 1, .. })));
}

#[test]
fn probe_counts_repeated_grid_tile_references_toward_one_work_budget() {
    let over_budget = probe(&repeated_tile_grid(16, 256));
    assert!(matches!(&over_budget, Err(Error::Limit(message)) if message.contains("extent reads")), "{over_budget:?}");

    let at_budget = probe(&repeated_tile_grid(63, 65));
    assert!(matches!(&at_budget, Ok(Info { width: 65, height: 63, .. })), "{at_budget:?}");
}

#[test]
fn probe_bounds_the_primary_grid_descriptor_before_heic_rs_rebuilds_it() {
    let bytes = primary_grid_with_many_extents(4097);
    let result = probe(&bytes);
    assert!(matches!(&result, Err(Error::Limit(message)) if message.contains("extent reads")), "{result:?}");
}

#[test]
fn decode_bounds_the_alpha_item_before_heic_rs_reconstructs_it() {
    let bytes = alpha_with_many_extents(4096);
    let result = decode(&bytes, &Options::default());
    assert!(matches!(&result, Err(Error::Limit(message)) if message.contains("extent reads")), "{result:?}");
}

#[test]
fn probe_counts_alpha_grid_tile_extents_with_the_primary_work_budget() {
    let bytes = alpha_grid_with_tile_extents(4095);
    let result = probe(&bytes);
    assert!(matches!(&result, Err(Error::Limit(message)) if message.contains("extent reads")), "{result:?}");
}

#[test]
fn probe_reports_alpha_for_regular_and_grid_auxiliary_items() {
    for bytes in [alpha_with_many_extents(1), alpha_grid_with_tile_extents(1)] {
        let result = probe(&bytes);
        assert!(matches!(&result, Ok(Info { has_alpha: true, .. })), "{result:?}");
    }
}

#[test]
fn iloc_extent_budget_bounds_allocations_without_rejecting_large_payloads() {
    let many_extents = probe(&primary_with_wide_extents(4096));
    let zero_width_extents = probe(&primary_with_over_budget_zero_width_extents());
    let item_entries = probe(&primary_with_over_budget_iloc_items());
    assert!(
        matches!(&many_extents, Ok(Info { width: 1, height: 1, .. }))
            && matches!(&zero_width_extents, Err(Error::Limit(message)) if message.contains("extent"))
            && matches!(&item_entries, Err(Error::Limit(message)) if message.contains("item locations")),
        "many extents: {many_extents:?}; zero-width extents: {zero_width_extents:?}; item entries: {item_entries:?}"
    );
}

#[test]
fn metadata_skips_xmp_when_reconstruction_exceeds_the_shared_extent_budget() {
    let over_budget = metadata(&xmp_with_source_extents(4096)).2;
    assert!(over_budget.is_none(), "metadata returned {} bytes", over_budget.as_ref().map_or(0, String::len));

    let at_budget = metadata(&xmp_with_source_extents(4095)).2;
    assert_eq!(at_budget.as_deref().map(str::len), Some(4095));
}

#[test]
fn decode_refuses_more_than_twelve_transform_properties() {
    let with_transforms = |count: usize| {
        let mut properties: Vec<_> = (0..count).map(|_| property(b"irot", &[0])).collect();
        properties.extend([hvc_c(), ispe(1, 1)]);
        let mut associations = (1..=count).map(|index| (1, vec![index as u8])).collect::<Vec<_>>();
        associations.push((1, vec![(count + 1) as u8, (count + 2) as u8]));
        heif(&[
            hdlr(),
            pitm(1),
            iinf(&[infe(1, b"hvc1", None)]),
            iprp(&properties, &associations),
            iloc(&[Location { id: 1, method: 1, extents: vec![Extent { index: 0, offset: 0, length: 1 }] }], 4, 4, 0),
            idat(b"x"),
        ])
    };
    let over_limit = decode(&with_transforms(13), &Options::default());
    assert!(matches!(&over_limit, Err(Error::Limit(message)) if message.contains("transform")), "{over_limit:?}");

    let at_limit = decode(&with_transforms(12), &Options::default());
    assert!(!matches!(&at_limit, Err(Error::Limit(_))), "{at_limit:?}");
}

#[test]
fn decode_caps_output_samples_even_when_the_caller_raises_the_pixel_limit() {
    let options = Options { max_pixels: u64::MAX, ..Options::default() };
    let rgb = decode(&primary_with_dimensions(20_000, 20_000), &options);
    assert!(matches!(&rgb, Err(Error::Limit(_))), "{rgb:?}");

    let rgba = decode(&primary_with_dimensions_and_alpha(17_000, 17_000, true), &options);
    assert!(matches!(&rgba, Err(Error::Limit(_))), "{rgba:?}");
}

#[test]
fn probe_refuses_grid_tile_data_larger_than_the_container_in_total() {
    let bytes = overlapping_tile_grid(30_000);
    let result = probe(&bytes);
    assert!(matches!(&result, Err(Error::Limit(message)) if message.contains("item data")), "{result:?}");
}
