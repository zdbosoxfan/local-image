#![allow(dead_code)]
use image::{Rgb, RgbImage, RgbaImage};
use pc_trace::{ColorPath, Trace};
use photocraft_doc::{Path, PathOp, Subpath};
use photocraft_geom::{Affine, Rect};
use photocraft_testkit::vector as metrics;
use photocraft_vector::{path_coverage, shapes};
use std::path::Path as FsPath;
pub type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
#[derive(Clone, Copy, Debug)]
pub enum Variant {
    Clean,
    Jpeg,
    Blur,
    Noise,
    Resample,
    Scan,
}
impl Variant {
    pub const ALL: [Self; 6] = [Self::Clean, Self::Jpeg, Self::Blur, Self::Noise, Self::Resample, Self::Scan];
    pub fn name(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Jpeg => "jpeg60",
            Self::Blur => "blur1",
            Self::Noise => "noise4",
            Self::Resample => "half",
            Self::Scan => "scan",
        }
    }
}
#[derive(Clone)]
pub struct Case {
    pub id: String,
    pub clean: RgbImage,
    pub input: RgbaImage,
    pub truth: Trace,
}
struct Outline {
    path: kurbo::BezPath,
    scale: f64,
    x: f64,
}
impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to((self.x + f64::from(x) * self.scale, 172. - f64::from(y) * self.scale));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to((self.x + f64::from(x) * self.scale, 172. - f64::from(y) * self.scale));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.path.quad_to(
            (self.x + f64::from(x1) * self.scale, 172. - f64::from(y1) * self.scale),
            (self.x + f64::from(x) * self.scale, 172. - f64::from(y) * self.scale),
        );
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path.curve_to(
            (self.x + f64::from(x1) * self.scale, 172. - f64::from(y1) * self.scale),
            (self.x + f64::from(x2) * self.scale, 172. - f64::from(y2) * self.scale),
            (self.x + f64::from(x) * self.scale, 172. - f64::from(y) * self.scale),
        );
    }
    fn close(&mut self) {
        self.path.close_path();
    }
}
fn wordmark() -> TestResult<Path> {
    let font = ttf_parser::Face::parse(include_bytes!("../../../../assets/fonts/Inter-Regular.ttf"), 0)?;
    let mut outline = Outline { path: kurbo::BezPath::new(), scale: 105. / f64::from(font.units_per_em()), x: 24. };
    for c in "OBO".chars() {
        let id = font.glyph_index(c).ok_or("missing OFL glyph")?;
        font.outline_glyph(id, &mut outline).ok_or("missing glyph outline")?;
        outline.x += f64::from(font.glyph_hor_advance(id).ok_or("missing glyph advance")?) * outline.scale;
    }
    Ok(pc_trace::adapter::bezpath_to_path(&outline.path)?)
}
pub fn truth(index: usize, size: u32) -> TestResult<Trace> {
    let mut layers = vec![ColorPath { color: [255; 4], path: shapes::rect(0., 0., 256., 256.) }];
    let color = if index < 6 { [0, 0, 0, 255] } else { [[20, 80, 190, 255], [200, 35, 55, 255], [25, 155, 80, 255]][(index / 6) % 3] };
    let shape = match index % 6 {
        0 => shapes::ellipse(44., 44., 168., 168.),
        1 => shapes::rounded_rect(36., 50., 184., 156., [24.; 4]),
        2 => shapes::polygon(32., 32., 192., 192., 5, 0.45),
        3 => Path::new(vec![Subpath::polygon(&[(30., 98.), (145., 98.), (145., 52.), (228., 128.), (145., 204.), (145., 158.), (30., 158.)])]),
        4 => {
            let mut p = shapes::ellipse(36., 36., 184., 184.);
            let mut hole = shapes::ellipse(85., 85., 86., 86.);
            for s in &mut hole.subpaths {
                s.op = PathOp::Subtract;
            }
            p.subpaths.extend(hole.subpaths);
            p
        }
        _ => wordmark()?,
    };
    let variant = index / 6;
    let transform = Affine::translate(128., 128.).mul(&Affine::rotate((variant as f64 * 4.).to_radians())).mul(&Affine::translate(-128., -128.));
    layers.push(ColorPath { color, path: shape.transform(&transform) });
    if index >= 12 {
        layers.push(ColorPath { color: [235, 160, 20, 255], path: shapes::ellipse(154., 142., 52., 52.) });
    }
    if index >= 24 {
        layers.push(ColorPath { color: [0, 0, 0, 255], path: shapes::rect(24., 214., 208., 12.) });
    }
    let scale = Affine::scale(f64::from(size) / 256.);
    for l in &mut layers {
        l.path = l.path.transform(&scale);
    }
    let nodes = layers.iter().map(|l| metrics::node_count(&l.path)).sum();
    Ok(Trace { width: size, height: size, layers, nodes })
}
pub fn render(trace: &Trace) -> RgbImage {
    let mut out = RgbImage::from_pixel(trace.width, trace.height, Rgb([255; 3]));
    for layer in &trace.layers {
        let cov = path_coverage(&layer.path, Rect::new(0, 0, trace.width as i32, trace.height as i32));
        for (p, &a) in out.pixels_mut().zip(&cov) {
            let a = f64::from(a) * f64::from(layer.color[3]) / 255.;
            for (c, v) in p.0.iter_mut().zip(layer.color) {
                *c = (f64::from(*c) * (1. - a) + f64::from(v) * a).round() as u8;
            }
        }
    }
    out
}
fn noise(state: &mut u64) -> f64 {
    let mut u = 0.;
    for _ in 0..12 {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        u += (*state >> 32) as f64 / u32::MAX as f64;
    }
    u - 6.
}
pub fn degrade(clean: &RgbImage, variant: Variant) -> TestResult<RgbaImage> {
    let mut image = clean.clone();
    let (w, h) = image.dimensions();
    match variant {
        Variant::Clean => {}
        Variant::Jpeg => {
            let mut bytes = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 60).encode_image(&image)?;
            image = image::load_from_memory(&bytes)?.to_rgb8();
        }
        Variant::Blur => image = image::imageops::blur(&image, 1.),
        Variant::Noise => {
            let mut state = 42;
            for p in image.pixels_mut() {
                for c in &mut p.0 {
                    *c = (f64::from(*c) + noise(&mut state) * 4.).round().clamp(0., 255.) as u8;
                }
            }
        }
        Variant::Resample => {
            let small = image::imageops::resize(&image, w / 2, h / 2, image::imageops::FilterType::Triangle);
            image = image::imageops::resize(&small, w, h, image::imageops::FilterType::CatmullRom);
        }
        Variant::Scan => {
            let source = image.clone();
            let angle = 0.7f64.to_radians();
            let (sin, cos) = angle.sin_cos();
            let mut state = 91;
            for y in 0..h {
                for x in 0..w {
                    let dx = f64::from(x) - f64::from(w) / 2.;
                    let dy = f64::from(y) - f64::from(h) / 2.;
                    let sx = dx * cos + dy * sin + f64::from(w) / 2.;
                    let sy = -dx * sin + dy * cos + f64::from(h) / 2.;
                    let px = bilinear(&source, sx, sy);
                    let lighting = 0.83 + 0.13 * f64::from(x) / f64::from(w) + 0.03 * (f64::from(y) / 37.).sin();
                    let texture = noise(&mut state) * 2.;
                    image.put_pixel(x, y, Rgb(px.map(|v| (v * lighting + texture).round().clamp(0., 255.) as u8)));
                }
            }
        }
    }
    Ok(image::DynamicImage::ImageRgb8(image).into_rgba8())
}
fn bilinear(image: &RgbImage, x: f64, y: f64) -> [f64; 3] {
    let mut result = [0.; 3];
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (tx, ty) = (x - x.floor(), y - y.floor());
    for (dx, wx) in [(0, 1. - tx), (1, tx)] {
        for (dy, wy) in [(0, 1. - ty), (1, ty)] {
            let xx = ix + dx;
            let yy = iy + dy;
            let p =
                if xx >= 0 && yy >= 0 && xx < image.width() as i32 && yy < image.height() as i32 { image.get_pixel(xx as u32, yy as u32).0 } else { [255; 3] };
            for c in 0..3 {
                result[c] += f64::from(p[c]) * wx * wy;
            }
        }
    }
    result
}
pub fn case(index: usize, size: u32, variant: Variant) -> TestResult<Case> {
    let truth = truth(index, size)?;
    let clean = render(&truth);
    let input = degrade(&clean, variant)?;
    Ok(Case { id: format!("logo-{index:02}-{size}-{}", variant.name()), clean, input, truth })
}
pub fn render_svg(path: &FsPath) -> TestResult<RgbImage> {
    let data = std::fs::read(path)?;
    let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default())?;
    let size = tree.size().to_int_size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height()).ok_or("SVG raster allocation")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::default(), &mut pixmap.as_mut());
    let mut rgb = RgbImage::new(size.width(), size.height());
    for (p, v) in rgb.pixels_mut().zip(pixmap.pixels()) {
        let a = u16::from(v.alpha());
        p.0 = [v.red(), v.green(), v.blue()].map(|v| (u16::from(v) + 255 - a).min(255) as u8);
    }
    Ok(rgb)
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Score {
    pub id: String,
    pub fidelity: f64,
    pub ms_ssim: f64,
    pub delta_mean: f64,
    pub delta_p95: f64,
    pub iou: Vec<f64>,
    pub hausdorff: Vec<f64>,
    pub nodes: usize,
    pub node_ratio: f64,
    pub seconds: f64,
}
pub fn score(case: &Case, trace: &Trace, seconds: f64) -> TestResult<Score> {
    let result = render(trace);
    let f = metrics::fidelity(&case.clean, &result)?;
    let (delta_mean, delta_p95) = metrics::delta_e_ok(&case.clean, &result)?;
    let (iou, hausdorff) = geometry_scores(case, trace)?;
    Ok(Score {
        id: case.id.clone(),
        fidelity: f.fidelity,
        ms_ssim: f.ms_ssim,
        delta_mean,
        delta_p95,
        iou,
        hausdorff,
        nodes: trace.nodes,
        node_ratio: if case.truth.nodes == 0 { 0. } else { trace.nodes as f64 / case.truth.nodes as f64 },
        seconds,
    })
}
fn geometry_scores(case: &Case, trace: &Trace) -> TestResult<(Vec<f64>, Vec<f64>)> {
    let rect = Rect::new(0, 0, trace.width as i32, trace.height as i32);
    let mut iou = Vec::new();
    let mut hausdorff = Vec::new();
    for l in &case.truth.layers {
        if l.color == [255; 4] {
            continue;
        }
        let nearest = trace.layers.iter().min_by(|a, b| {
            metrics::delta_e(a.color[..3].try_into().unwrap_or([0; 3]), l.color[..3].try_into().unwrap_or([0; 3]))
                .total_cmp(&metrics::delta_e(b.color[..3].try_into().unwrap_or([0; 3]), l.color[..3].try_into().unwrap_or([0; 3])))
        });
        if let Some(n) = nearest {
            iou.push(metrics::iou(&path_coverage(&l.path, rect), &path_coverage(&n.path, rect), 0.5)?);
            hausdorff.push(metrics::hausdorff(&l.path, &n.path, 16)?);
        } else {
            iou.push(0.);
            hausdorff.push(f64::from(trace.width));
        }
    }
    Ok((iou, hausdorff))
}
pub fn median(values: &[f64]) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    if v.is_empty() {
        return 0.;
    }
    if v.len().is_multiple_of(2) { (v[v.len() / 2 - 1] + v[v.len() / 2]) / 2. } else { v[v.len() / 2] }
}
/// B.4 gates compare per-preset medians and per-image wins to the strongest baseline.
pub fn gates(ours: &[Score], competitors: &[Vec<Score>], strict: bool) -> TestResult {
    if ours.is_empty() || competitors.is_empty() {
        return Err("empty benchmark gate".into());
    }
    for c in competitors {
        if c.len() != ours.len() || c.iter().zip(ours).any(|(a, b)| a.id != b.id) {
            return Err("reference case list mismatch".into());
        }
    }
    let om = median(&ours.iter().map(|s| s.fidelity).collect::<Vec<_>>());
    let cm = competitors.iter().map(|c| median(&c.iter().map(|s| s.fidelity).collect::<Vec<_>>())).fold(0., f64::max);
    if om + 0.005 < cm {
        return Err(format!("median fidelity {om} < best {cm} - 0.005").into());
    }
    let wins = ours.iter().enumerate().filter(|(i, o)| competitors.iter().all(|c| o.fidelity >= c[*i].fidelity)).count();
    let target = if strict { 0.8 } else { 0.7 };
    if wins as f64 / (ours.len() as f64) < target {
        return Err(format!("competitor win fraction {wins}/{} < {target}", ours.len()).into());
    }
    let closest = competitors
        .iter()
        .min_by(|a, b| {
            let m = |c: &Vec<Score>| (median(&c.iter().map(|s| s.fidelity).collect::<Vec<_>>()) - om).abs();
            m(a).total_cmp(&m(b))
        })
        .ok_or("no closest competitor")?;
    let on = median(&ours.iter().map(|s| s.nodes as f64).collect::<Vec<_>>());
    let cn = median(&closest.iter().map(|s| s.nodes as f64).collect::<Vec<_>>());
    if on > cn * 1.2 {
        return Err(format!("median node count {on} exceeds 1.2 × {cn}").into());
    }
    Ok(())
}
pub fn reference_score(case: &Case, path: &FsPath) -> TestResult<Score> {
    let render = render_svg(path)?;
    let f = metrics::fidelity(&case.clean, &render)?;
    let (delta_mean, delta_p95) = metrics::delta_e_ok(&case.clean, &render)?;
    let data = std::fs::read(path)?;
    let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default())?;
    fn nodes(g: &resvg::usvg::Group) -> usize {
        g.children()
            .iter()
            .map(|n| match n {
                resvg::usvg::Node::Group(g) => nodes(g),
                resvg::usvg::Node::Path(p) => {
                    use resvg::tiny_skia::PathSegment as S;
                    let (mut total, mut count) = (0, 0);
                    let (mut first, mut last) = (None, None);
                    for segment in p.data().segments() {
                        match segment {
                            S::MoveTo(p) => {
                                total += count;
                                count = 1;
                                first = Some(p);
                                last = Some(p);
                            }
                            S::LineTo(p) | S::QuadTo(_, p) | S::CubicTo(_, _, p) => {
                                count += 1;
                                last = Some(p);
                            }
                            S::Close => {
                                // A closing curve/line can repeat the move's anchor;
                                // the same knot is not an additional editable node.
                                total += count - usize::from(count > 1 && first == last);
                                count = 0;
                            }
                        }
                    }
                    total + count
                }
                _ => 0,
            })
            .sum()
    }
    let nodes = nodes(tree.root());
    let (iou, hausdorff) = geometry_scores(case, &svg_truth(path)?)?;
    Ok(Score {
        id: case.id.clone(),
        fidelity: f.fidelity,
        ms_ssim: f.ms_ssim,
        delta_mean,
        delta_p95,
        iou,
        hausdorff,
        nodes,
        node_ratio: if case.truth.nodes == 0 { 0. } else { nodes as f64 / case.truth.nodes as f64 },
        seconds: 0.,
    })
}
/// Licensed external files supplied by the coordinator. SVGs have vector ground truth;
/// scans use their input as the fidelity target and have no ground-truth node ratio.
pub fn external_cases() -> TestResult<Vec<Case>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let manifest = root.join("inputs.json");
    if !manifest.is_file() {
        return Ok(Vec::new());
    }
    #[derive(serde::Deserialize)]
    struct Entry {
        file: String,
        license: String,
        url: String,
    }
    let entries: Vec<Entry> = serde_json::from_slice(&std::fs::read(manifest)?)?;
    let mut cases = Vec::new();
    for e in entries {
        if e.license.is_empty() || e.url.is_empty() || e.file.contains('/') || e.file.contains('\\') {
            return Err("corpus entry requires a local filename, licence and source URL".into());
        }
        let path = root.join(&e.file);
        let is_svg = path.extension().is_some_and(|s| s == "svg");
        let clean = if is_svg { render_svg(&path)? } else { image::open(&path)?.to_rgb8() };
        let input = image::DynamicImage::ImageRgb8(clean.clone()).into_rgba8();
        let truth = if is_svg { svg_truth(&path)? } else { Trace { width: clean.width(), height: clean.height(), layers: Vec::new(), nodes: 0 } };
        let stem = path.file_stem().and_then(|s| s.to_str()).ok_or("invalid case name")?;
        cases.push(Case { id: format!("external-{stem}"), clean, input, truth });
    }
    cases.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(cases)
}
/// Binary competitors use the monochrome subset of the supplied corpus.
pub fn external_cases_for(preset: pc_trace::Preset) -> TestResult<Vec<Case>> {
    let mut cases = external_cases()?;
    if matches!(preset, pc_trace::Preset::BlackWhite | pc_trace::Preset::Silhouette) {
        cases.retain(|c| c.clean.pixels().all(|p| p[0] == p[1] && p[1] == p[2]));
    }
    Ok(cases)
}
fn svg_truth(path: &FsPath) -> TestResult<Trace> {
    let data = std::fs::read(path)?;
    let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default())?;
    let size = tree.size().to_int_size();
    let mut layers = Vec::new();
    fn visit(g: &resvg::usvg::Group, layers: &mut Vec<ColorPath>) -> TestResult {
        for node in g.children() {
            match node {
                resvg::usvg::Node::Group(g) => visit(g, layers)?,
                resvg::usvg::Node::Path(p) => {
                    let Some(fill) = p.fill() else {
                        continue;
                    };
                    let resvg::usvg::Paint::Color(color) = fill.paint() else {
                        continue;
                    };
                    let mut bez = kurbo::BezPath::new();
                    for el in p.data().segments() {
                        use resvg::tiny_skia::PathSegment as S;
                        match el {
                            S::MoveTo(a) => bez.move_to((f64::from(a.x), f64::from(a.y))),
                            S::LineTo(a) => bez.line_to((f64::from(a.x), f64::from(a.y))),
                            S::QuadTo(a, b) => bez.quad_to((f64::from(a.x), f64::from(a.y)), (f64::from(b.x), f64::from(b.y))),
                            S::CubicTo(a, b, c) => {
                                bez.curve_to((f64::from(a.x), f64::from(a.y)), (f64::from(b.x), f64::from(b.y)), (f64::from(c.x), f64::from(c.y)))
                            }
                            S::Close => bez.close_path(),
                        }
                    }
                    let t = p.abs_transform();
                    let transform = Affine { m: [f64::from(t.sx), f64::from(t.ky), f64::from(t.kx), f64::from(t.sy), f64::from(t.tx), f64::from(t.ty)] };
                    let mut path = pc_trace::adapter::bezpath_to_path(&bez)?.transform(&transform);
                    path.fill_rule =
                        if fill.rule() == resvg::usvg::FillRule::EvenOdd { photocraft_doc::FillRule::EvenOdd } else { photocraft_doc::FillRule::NonZero };
                    layers.push(ColorPath { color: [color.red, color.green, color.blue, (fill.opacity().get() * 255.).round() as u8], path });
                }
                _ => {}
            }
        }
        Ok(())
    }
    visit(tree.root(), &mut layers)?;
    let nodes = layers.iter().map(|l| metrics::node_count(&l.path)).sum();
    Ok(Trace { width: size.width(), height: size.height(), layers, nodes })
}
