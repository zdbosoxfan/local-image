//! Optimized queries must preserve the old comparator's exact Unicode/tie ordering.
use lightcraft_catalog::{Catalog, Filter, Op, Photo, PhotoId, Sort, SortKey, Source};
#[test]
fn cached_name_keys_match_reference_order_in_both_directions() {
    let mut c = Catalog::new();
    for (i, name) in ["IMG_20.ARW", "img_2.arw", "Ångström.ARW", "ångström.arw", "İmage.ARW", "IMG_20.ARW", "ΣΟΣ.ARW"].into_iter().enumerate()
    {
        let mut p = Photo::new(PhotoId(i as u64 + 1), Source::Demo { scene: 0 }, name, "ARW", 6000, 4000, "2026-09-01");
        p.meta.title = "Conference speaker".into();
        p.meta.camera = "Sony Event Camera".into();
        p.rating = (i % 6) as u8;
        c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    for text in ["", "CONFERENCE camera:SONY rating:>2", "name:ÅNGSTRÖM", "name:ΣΟΣ"] {
        let f = Filter { text: text.into(), ..Default::default() };
        for ascending in [false, true] {
            let mut expected: Vec<_> = c.photos().filter(|p| f.matches(p, &c)).collect();
            expected.sort_by(|a, b| {
                let o = a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase()).then(a.id.cmp(&b.id));
                if ascending { o } else { o.reverse() }
            });
            let expected: Vec<_> = expected.iter().map(|p| p.id).collect();
            assert_eq!(c.query(&f, &Sort { key: SortKey::FileName, ascending, ..Default::default() }), expected);
        }
    }
}
