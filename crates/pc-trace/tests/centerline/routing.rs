use pc_trace::{Image, Mode, Params, Preset};
#[test]
fn centerline_public_mode_routing_and_legacy_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let old: Params = serde_json::from_str("{}")?;
    assert_eq!(old.resolved_mode(), Mode::Fill);
    for preset in [Preset::LineArt, Preset::Sketch] {
        let p = Params::for_preset(preset);
        p.validate()?;
        assert_eq!(p.resolved_mode(), Mode::Centerline);
        let out = pc_trace::trace(Image { width: 1, height: 1, rgba: &[0, 0, 0, 255] }, &p)?;
        assert!(out.layers.is_empty());
        assert_eq!(out.strokes.len(), 1);
        let p = Params { mode: Some(Mode::Fill), ..p };
        assert!(p.validate().is_err());
    }
    let p = Params { mode: Some(Mode::Centerline), ..Params::for_preset(Preset::BlackWhite) };
    p.validate()?;
    for preset in [Preset::Logo, Preset::FewColors, Preset::Silhouette, Preset::Photo, Preset::PixelArt] {
        let p = Params::for_preset(preset);
        p.validate()?;
        assert_eq!(p.resolved_mode(), Mode::Fill);
        assert!(Params { mode: Some(Mode::Centerline), ..p }.validate().is_err());
    }
    let legacy: pc_trace::Trace = serde_json::from_str(r#"{"width":1,"height":1,"layers":[],"nodes":0}"#)?;
    assert!(legacy.strokes.is_empty());
    assert!(!serde_json::to_string(&legacy)?.contains("strokes"));
    Ok(())
}
