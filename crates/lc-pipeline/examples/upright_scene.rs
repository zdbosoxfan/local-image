//! Print the Upright analysis and each mode's correction for a demo scene.
//! `SCENE="Lake" cargo run --release -p lightcraft-pipeline --example upright_scene`
use lightcraft_develop::{DevelopSettings, Upright};
use lightcraft_pipeline::SourceInfo;
use lightcraft_pipeline::upright::{analyze_source, solve};

fn main() {
    let scene =
        lightcraft_scenes::demo_library().into_iter().find(|s| s.name.starts_with(std::env::var("SCENE").as_deref().unwrap_or("Lake"))).unwrap();
    let src = scene.render_fit(768);
    let a = analyze_source(&src, &SourceInfo::default(), &DevelopSettings::default());
    eprintln!("vert {} horiz {} vvp {:?} hvp {:?}", a.vertical.len(), a.horizontal.len(), a.vertical_vp, a.horizontal_vp);
    for s in a.vertical.iter().take(20) {
        eprintln!("  v {:.3},{:.3} -> {:.3},{:.3} len {:.3}", s.a.x, s.a.y, s.b.x, s.b.y, s.len());
    }
    for m in [Upright::Level, Upright::Vertical, Upright::Full, Upright::Auto] {
        eprintln!("{m:?}: {:?}", solve(m, &a).0);
    }
}
