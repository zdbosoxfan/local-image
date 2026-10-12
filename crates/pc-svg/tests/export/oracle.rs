//! The quality plan specifies geometry IoU at 8x supersampled coverage. Colour is
//! compared separately at the requested output resolution, including all opacities.
//! Opaque paint isolates geometry from the 50% paint-opacity threshold, which is
//! unrelated to shape boundaries. Both renderer inputs retain their actual geometry.
use super::*;
pub(super) fn coverage_iou(doc: &Document, svg: &str) -> f64 {
    const SCALE: u32 = 8;
    let parsed = xml(svg);
    let mut edits = Vec::new();
    for node in parsed.descendants().filter(|n| n.is_element()) {
        for a in node.attributes() {
            let value = match a.name() {
                "fill" | "stroke" | "stop-color" if a.value() != "none" => Some("#000000".to_owned()),
                "opacity" | "fill-opacity" | "stroke-opacity" | "stop-opacity" => Some("1".to_owned()),
                "width" if node == parsed.root_element() => Some((doc.size.width * SCALE).to_string()),
                "height" if node == parsed.root_element() => Some((doc.size.height * SCALE).to_string()),
                _ => None,
            };
            if let Some(value) = value {
                edits.push((a.range_value(), value));
            }
        }
    }
    edits.sort_by_key(|a| std::cmp::Reverse(a.0.start));
    let mut opaque = svg.to_owned();
    for (range, value) in edits {
        opaque.replace_range(range, &value);
    }
    let actual = render(&opaque);
    let mut reference = doc.clone();
    reference.size = Size::new(doc.size.width * SCALE, doc.size.height * SCALE);
    let format = reference.pixel_format();
    let bounds = reference.bounds();
    let transform = photocraft_doc::Affine::scale(f64::from(SCALE));
    fn walk(layers: &mut [Layer], transform: &photocraft_doc::Affine, format: photocraft_doc::PixelFormat, bounds: photocraft_doc::Rect, dpi: f32) {
        for layer in layers {
            layer.opacity = 1.;
            layer.fill_opacity = 1.;
            if let Some(mask) = &mut layer.vector_mask {
                mask.path = mask.path.transform(transform);
            }
            match &mut layer.content {
                LayerContent::Shape(shape) => {
                    shape.path = shape.path.transform(transform);
                    if shape.fill.is_some() {
                        shape.fill = Some(Fill::Solid(Color::BLACK));
                    }
                    if let Some(stroke) = &mut shape.stroke {
                        stroke.paint = Fill::Solid(Color::BLACK);
                        stroke.opacity = 1.;
                        stroke.width *= 8.;
                    }
                    shape.cache = Some(photocraft_vector::render_shape(shape, format, bounds));
                }
                LayerContent::Text(text) => {
                    text.transform = transform.mul(&text.transform);
                    text.color = Color::BLACK;
                    for run in &mut text.runs {
                        run.style.color = Color::BLACK;
                    }
                    photocraft_text::TextEngine::new().render_layer(text, dpi, format);
                }
                LayerContent::Group(group) => walk(&mut group.children, transform, format, bounds, dpi),
                LayerContent::Fill(fill) => *fill = Fill::Solid(Color::BLACK),
                _ => panic!("coverage oracle supports vector fixtures"),
            }
        }
    }
    walk(&mut reference.layers, &transform, format, bounds, reference.resolution_dpi);
    let expected = photocraft_compose::flatten(&reference).to_rgba8();
    assert_eq!(actual.len() * 4, expected.pixels.len());
    // Integrate fractional covered areas: intersect=min(a,b), union=max(a,b).
    // Thresholding discards subpixel coverage and can turn a tiny AA difference
    // along a half-covered edge into an entire row/column of geometry errors.
    let (intersection, union) = actual
        .iter()
        .zip(expected.pixels.as_chunks::<4>().0.iter())
        .fold((0_u64, 0_u64), |(i, u), (a, b)| (i + u64::from(a[3].min(b[3])), u + u64::from(a[3].max(b[3]))));
    if union == 0 { 1.0 } else { intersection as f64 / union as f64 }
}

/// PNG-capable independent renderer for native-resolution raster fallback fixtures.
/// resvg's pinned dev feature set does not render embedded PNGs. Each Inkscape
/// invocation has a unique application ID and isolated XDG/TMP state; it never
/// connects to an existing user window. Missing tools and render failures are errors.
pub(super) fn native_raster_render(svg: &str) -> Vec<[u8; 4]> {
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let tag = format!("liExportTest{}r{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/svg-crop-qa").join(&tag);
    std::fs::create_dir_all(&dir).unwrap();
    for folder in ["config", "cache", "data", "state", "runtime", "tmp"] {
        std::fs::create_dir_all(dir.join(folder)).unwrap();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.join("runtime"), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let executable = std::env::var_os("LOCAL_IMAGE_INKSCAPE").unwrap_or_else(|| "inkscape".into());
    let command = || {
        let mut command = Command::new(&executable);
        command
            .arg(format!("--app-id-tag={tag}"))
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .env_remove("DBUS_STARTER_ADDRESS")
            .env_remove("DBUS_STARTER_BUS_TYPE")
            .env("XDG_CONFIG_HOME", dir.join("config"))
            .env("XDG_CACHE_HOME", dir.join("cache"))
            .env("XDG_DATA_HOME", dir.join("data"))
            .env("XDG_STATE_HOME", dir.join("state"))
            .env("XDG_RUNTIME_DIR", dir.join("runtime"))
            .env("TMPDIR", dir.join("tmp"));
        command
    };
    let run = |mut command: Command, log_name: &str| {
        let log_path = dir.join(log_name);
        let log = std::fs::File::create(&log_path).unwrap();
        command.stdout(Stdio::from(log.try_clone().unwrap())).stderr(Stdio::from(log));
        let mut child = command.spawn().unwrap_or_else(|error| {
            panic!("Inkscape 1.4+ with --app-id-tag is required for the PNG-capable SVG oracle; install inkscape on PATH or set LOCAL_IMAGE_INKSCAPE to its executable path (selected {executable:?}): {error}")
        });
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
                outcome => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("isolated Inkscape oracle timed out or failed: {outcome:?}; log {}", log_path.display());
                }
            }
        };
        assert!(status.success(), "Inkscape oracle failed: {}", std::fs::read_to_string(&log_path).unwrap());
    };
    let mut version = command();
    version.arg("--version");
    run(version, "version.txt");
    let version = std::fs::read_to_string(dir.join("version.txt")).unwrap();
    assert!(version.contains("Inkscape"), "renderer version must be recorded");
    let source = dir.join("source.svg");
    let output = dir.join("inkscape.png");
    std::fs::write(&source, svg).unwrap();
    let mut render = command();
    render
        .args(["--export-type=png", "--export-area-page", "--export-dpi=96", "--export-background-opacity=0"])
        .arg(format!("--export-filename={}", output.display()))
        .arg(&source);
    run(render, "render.log");
    let pixels = image::open(&output).expect("Inkscape must produce its PNG output").into_rgba8();
    eprintln!("native SVG raster oracle: {} · {}", version.trim(), output.display());
    pixels.as_raw().as_chunks::<4>().0.to_vec()
}
