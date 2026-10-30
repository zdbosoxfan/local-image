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
        out.push((value >> (index * 8)) as u8);
    }
}

fn iref(kind: &[u8; 4], from: u16, to: &[u16]) -> Vec<u8> {
    let mut reference = from.to_be_bytes().to_vec();
    reference.extend_from_slice(&(to.len() as u16).to_be_bytes());
    for id in to {
        reference.extend_from_slice(&id.to_be_bytes());
    }
    full_box(b"iref", 0, 0, &boxed(kind, &reference))
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
    idat_bytes.push(0);
    let tile_ids = vec![2; usize::from(rows) * usize::from(columns)];
    heif(&[
        hdlr(),
        pitm(1),
        iinf(&[infe(1, b"grid", None), infe(2, b"hvc1", None)]),
        iref(b"dimg", 1, &tile_ids),
        iloc(
            &[
                Location { id: 1, method: 1, extents: vec![Extent { index: 0, offset: 0, length: 8 }] },
                Location { id: 2, method: 1, extents: vec![Extent { index: 0, offset: 8, length: 1 }] },
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
    heif(&[
        hdlr(),
        pitm(1),
        iinf(&[infe(1, b"hvc1", None)]),
        iloc(&[Location { id: 1, method: 1, extents: vec![Extent { index: 0, offset: 0, length: 1 }] }], 4, 4, 0),
        iprp(&[hvc_c(), ispe(width, height)], &[(1, vec![1, 2])]),
        idat(b"x"),
    ])
}

#[test]
fn oversized_metadata_boxes_are_refused_before_heic_rs_parses_them() {
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

    let mut locations = vec![0, 0, 0, 0, 4, 0];
    locations.extend_from_slice(&1u16.to_be_bytes());
    locations.extend_from_slice(&1u16.to_be_bytes());
    locations.extend_from_slice(&1u16.to_be_bytes());
    locations.extend_from_slice(&[0, 0, 0, 1]);
    locations.resize((64 << 10) + 1, 0);
    let cases = [
        {
            let mut children = base.to_vec();
            children[3] = oversized_ipma;
            heif(&children)
        },
        {
            let mut children = base.to_vec();
            children.push(boxed(b"iloc", &locations));
            heif(&children)
        },
    ];
    let failures: Vec<_> = cases
        .into_iter()
        .zip(["associations", "item locations"])
        .filter_map(|(bytes, label)| {
            let result = probe(&bytes);
            (!matches!(&result, Err(Error::Limit(message)) if message.contains(label))).then_some((label, result))
        })
        .collect();
    assert!(failures.is_empty(), "{failures:?}");
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
fn metadata_skips_xmp_when_reconstruction_exceeds_the_shared_extent_budget() {
    let over_budget = metadata(&xmp_with_source_extents(4096)).2;
    assert!(over_budget.is_none(), "metadata returned {} bytes", over_budget.as_ref().map_or(0, String::len));

    let at_budget = metadata(&xmp_with_source_extents(4095)).2;
    assert_eq!(at_budget.as_deref().map(str::len), Some(4095));
}

#[test]
fn decode_refuses_more_than_twelve_transform_properties() {
    let mut properties: Vec<_> = (0..13).map(|_| property(b"irot", &[0])).collect();
    properties.extend([hvc_c(), ispe(1, 1)]);
    let mut associations = (1..=13).map(|index| (1, vec![index])).collect::<Vec<_>>();
    associations.push((1, vec![14, 15]));
    let bytes = heif(&[
        hdlr(),
        pitm(1),
        iinf(&[infe(1, b"hvc1", None)]),
        iprp(&properties, &associations),
        iloc(&[Location { id: 1, method: 1, extents: vec![Extent { index: 0, offset: 0, length: 1 }] }], 4, 4, 0),
        idat(b"x"),
    ]);
    let result = decode(&bytes, &Options::default());
    assert!(matches!(&result, Err(Error::Limit(message)) if message.contains("transform")), "{result:?}");
}

#[test]
fn decode_caps_output_samples_even_when_the_caller_raises_the_pixel_limit() {
    let bytes = primary_with_dimensions(600_000, 600_000);
    let options = Options { max_pixels: u64::MAX, ..Options::default() };
    let result = decode(&bytes, &options);
    assert!(matches!(&result, Err(Error::Limit(_))), "{result:?}");
}

#[test]
fn probe_refuses_grid_tile_data_larger_than_the_container_in_total() {
    let bytes = overlapping_tile_grid(30_000);
    let result = probe(&bytes);
    assert!(matches!(&result, Err(Error::Limit(message)) if message.contains("item data")), "{result:?}");
}
