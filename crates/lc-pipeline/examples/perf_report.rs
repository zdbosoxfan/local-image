//! Time a typical edited render of a 6 MP scene to a 2560x1440 fit.
//! `cargo run --release -p lightcraft-pipeline --example perf_report`
use lightcraft_develop::DevelopSettings;
use lightcraft_pipeline::{RenderRequest, SourceInfo, render};

fn main() {
    let src = lightcraft_scenes::demo_library()[0].render(3000, 2000);
    let mut s = DevelopSettings::default();
    s.light.shadows = 40.0;
    s.effects.clarity = 20.0;
    s.effects.dehaze = 10.0;
    s.color.vibrance = 20.0;
    let t = std::time::Instant::now();
    let n = 5;
    for _ in 0..n {
        let _ = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(2560, 1440));
    }
    eprintln!("render 2160x1440 from 6 MP: {:.1} ms", t.elapsed().as_secs_f64() * 1000.0 / n as f64);
}
