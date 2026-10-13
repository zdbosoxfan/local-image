#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use crate::{ColorMode, aco, ase, gpl, kpl};
const ACO: &[u8] = include_bytes!("swatches_fixtures/fixture.aco");
const GPL: &[u8] = include_bytes!("swatches_fixtures/fixture.gpl");
const XML: &[u8] = include_bytes!("swatches_fixtures/colorset.xml");
#[test]
fn swatches_formats_aco_independent_v1_v2_fixture_roundtrip_and_truncations() {
    let result = aco::read(ACO).unwrap();
    assert_eq!(result.palette.swatches.len(), 4);
    assert_eq!(result.palette.swatches[0].name, "Café 🎨");
    assert_eq!(result.palette.swatches[1].color.mode, ColorMode::Cmyk);
    assert_eq!(result.palette.swatches[1].color.c[0], 0.);
    assert_eq!(result.palette.swatches[2].color.mode, ColorMode::Lab);
    assert_eq!(result.palette.swatches[3].color.c[0], 0.25);
    assert_eq!(aco::write(&result.palette).unwrap(), ACO);
    assert_eq!(aco::read(&ACO[..44]).unwrap().palette.swatches.len(), 4);
    assert_eq!(aco::read(&ACO[44..]).unwrap(), result);
    for i in 0..ACO.len() {
        if i != 44 {
            assert!(aco::read(&ACO[..i]).is_err(), "truncation {i}");
        }
    }
    let mut bad = ACO.to_vec();
    bad[2..4].copy_from_slice(&4097u16.to_be_bytes());
    assert!(aco::read(&bad).is_err());
    let mut bad = ACO[44..].to_vec();
    bad[14..18].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(aco::read(&bad).is_err());
    let mut bad = ACO.to_vec();
    bad.push(0);
    assert!(aco::read(&bad).is_err());
}
#[test]
fn swatches_formats_gpl_names_unicode_quantized_roundtrip_and_invalid_numbers() {
    let p = gpl::read(GPL).unwrap().palette;
    assert_eq!(p.groups[0].name, "Café 🎨");
    assert_eq!(p.groups[0].swatches[0].name, "Blue with spaces");
    let result = gpl::read(&gpl::write(&p).unwrap()).unwrap();
    assert_eq!(result.palette.groups[0].swatches, p.groups[0].swatches);
    for tail in ["256 0 0 Red", "NaN 0 0 Red", "1 2 Red", "-1 2 3 Red"] {
        assert!(gpl::read(format!("GIMP Palette\n{tail}").as_bytes()).is_err());
    }
    assert!(gpl::read(b"GIMP Palette\n1 2 3 name\0bad").is_err());
    assert!(gpl::read(&[255]).is_err());
}
#[test]
fn swatches_formats_kpl_real_schema_spot_group_models_roundtrip_truncations() {
    let p = kpl::read_xml(XML).unwrap().palette;
    assert_eq!(p.swatches[0].name, "Café & 🎨");
    assert!(p.groups[0].swatches[0].spot);
    assert!(p.groups[0].swatches[0].global);
    assert_eq!(p.groups[0].swatches[1].color.mode, ColorMode::Lab);
    let output = kpl::write_xml(&p).unwrap();
    assert_eq!(kpl::read_xml(&output).unwrap().palette, p);
    for i in 0..XML.len() {
        assert!(kpl::read_xml(&XML[..i]).is_err(), "truncation {i}");
    }
    for xml in [
        "<!DOCTYPE ColorSet [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><ColorSet/>",
        "<ColorSet><Group name='a'></ColorSet>",
        "<ColorSet/><ColorSet/>",
        "<ColorSet><ColorSetEntry name='a'/></ColorSet>",
        "<ColorSet><Group name='a' name='b'/></ColorSet>",
        "<ColorSet><ColorSetEntry><sRGB r='NaN' g='0' b='0'/></ColorSetEntry></ColorSet>",
        "<ColorSet><Group name='&evil;'/></ColorSet>",
    ] {
        assert!(kpl::read_xml(xml.as_bytes()).is_err(), "{xml}");
    }
}
#[test]
fn swatches_formats_budgets_and_deterministic_hostile_corpus() {
    let huge = vec![b' '; ase::MAX_BYTES + 1];
    assert!(aco::read(&huge).is_err());
    assert!(gpl::read(&huge).is_err());
    assert!(kpl::read_xml(&huge).is_err());
    let many = format!("GIMP Palette\n{}", "1 2 3 Name\n".repeat(ase::MAX_ENTRIES));
    assert!(gpl::read(many.as_bytes()).is_err());
    let many = format!("<ColorSet>{}</ColorSet>", "<Group name='g'/>".repeat(ase::MAX_ENTRIES + 1));
    assert!(kpl::read_xml(many.as_bytes()).is_err());
    let mut seed = 42u64;
    for n in 0..512 {
        let bytes: Vec<_> = (0..n)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                (seed >> 32) as u8
            })
            .collect();
        let _ = aco::read(&bytes);
        let _ = gpl::read(&bytes);
        let _ = kpl::read_xml(&bytes);
        let _ = ase::read(&bytes);
    }
}

#[test]
fn swatches_formats_kpl_profile_inventory_callback_entities_and_limits() {
    let xml = b"<?xml version='1.0'?><Profiles><Profile name='A &amp; B' filename='p.icc' colorModelId='RGBA' colorDepthId='F32'/></Profiles>";
    assert_eq!(kpl::read_profiles_xml(xml).unwrap(), vec![("A & B".into(), "p.icc".into())]);
    for bad in [
        "<Profiles><Profile name='A'/></Profiles>",
        "<Profiles><Profile name='A' filename='p'/><Profile name='A' filename='q'/></Profiles>",
        "<Profiles><Profile name='A' filename='p'></Profile></Profiles>",
        "<!DOCTYPE Profiles><Profiles/>",
        "<Profiles/>trailing",
    ] {
        assert!(kpl::read_profiles_xml(bad.as_bytes()).is_err());
    }
    let xml = b"<ColorSet><ColorSetEntry name='Profile Ink' spot='true'><RGB r='0.25' g='0.5' b='0.75' space='Embedded'/></ColorSetEntry></ColorSet>";
    let mut called = 0;
    let p = kpl::read_xml_with_profiles(xml, |_, name| {
        assert_eq!(name, "Embedded");
        called += 1;
        Ok(Some(crate::Color::WHITE))
    })
    .unwrap();
    assert_eq!(called, 1);
    assert!(p.warnings.is_empty());
    assert!(p.palette.swatches[0].spot);
    assert_eq!(p.palette.swatches[0].color, crate::Color::WHITE);
    assert_eq!(kpl::read_xml(xml).unwrap().warnings.len(), 1);
    let xml=b"<ColorSet><ColorSetEntry name='Annotated'><sRGB r='0.25' g='0.5' b='0.75'/><metadata name='note' type='string' value='Krita annotation'/><Position row='0' column='0'/></ColorSetEntry></ColorSet>";
    let result = kpl::read_xml(xml).unwrap();
    assert_eq!(result.palette.swatches[0].color, crate::Color::rgb(0.25, 0.5, 0.75));
    assert_eq!(result.warnings.len(), 1);
}
