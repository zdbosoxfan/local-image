#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::vector_design::exchange::ImportLimits;
use std::sync::Arc;
#[allow(dead_code)]
mod fixture {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/pc-pdf/fixtures/import/source.rs"));
}

fn info() -> Vec<u8> {
    b"Pages: 2\nPage 2 rot: 90\nPage 2 MediaBox: 0 0 80 40\nPage 2 CropBox: 10 5 70 35\n".to_vec()
}
#[test]
fn page_geometry_rejects_ambiguous_unbounded_or_incomplete_metadata() {
    let page = metadata(&info(), 2).unwrap();
    assert_eq!(dimensions(&page, 144.0, true, MAX_PIXELS).unwrap(), (60, 120));
    assert_eq!(dimensions(&page, 72.0, false, MAX_PIXELS).unwrap(), (40, 80));
    assert!(dimensions(&page, f32::NAN, true, MAX_PIXELS).is_err());
    assert!(dimensions(&page, 2400.0, true, 10).is_err());
    for text in [
        "Pages: 1\nPage 1 rot: 0\n",
        "Pages: 1\nPages: 2\nPage 1 rot: 0\n",
        "Pages: 0\nPage 1 rot: 0\n",
        "Pages: 100000\nPage 1 rot: 0\n",
        "Pages: 1\nPage 1 rot: 45\nPage 1 MediaBox: 0 0 1 1\nPage 1 CropBox: 0 0 1 1\n",
        "Pages: 1\nPage 1 rot: 0\nPage 1 MediaBox: 0 0 nan 1\nPage 1 CropBox: 0 0 1 1\n",
    ] {
        assert!(metadata(text.as_bytes(), 1).is_err(), "{text}");
    }
}
#[test]
fn private_output_reader_refuses_symlinks_and_oversize_files() {
    let temp = Scratch::new().unwrap();
    let source = temp.path.join("out");
    private_file(&source).unwrap().write_all(&[1; 16]).unwrap();
    assert!(bounded_read(&source, 15).is_err());
    assert_eq!(bounded_read(&source, 16).unwrap().len(), 16);
    let link = temp.path.join("link");
    std::os::unix::fs::symlink(&source, &link).unwrap();
    assert!(bounded_read(&link, 100).is_err());
    assert!(library_lines("libbad => /home/unknown/lib.so (0x1)").is_err());
    assert!(library_lines("libbad => not found").is_err());
}
#[test]
fn uncertain_owned_job_poison_survives_guard_drop_and_denies_readmission() {
    let slot = Mutex::new(Admission::default());
    {
        let _lease = Permit::acquire(&slot).unwrap();
        assert!(Permit::acquire(&slot).is_err());
    }
    {
        let _lease = Permit::acquire(&slot).unwrap();
    }
    let lease = Permit::acquire(&slot).unwrap();
    Permit::poison(&slot, "local-image-pdf-owned-uncertain.service");
    drop(lease);
    let state = slot.lock().unwrap();
    assert!(!state.active);
    assert_eq!(state.uncertain_unit.as_deref(), Some("local-image-pdf-owned-uncertain.service"));
    drop(state);
    let Err(error) = Permit::acquire(&slot) else {
        panic!("uncertain cleanup must not release admission");
    };
    assert!(error.to_string().contains("local-image-pdf-owned-uncertain.service"));
}
fn geometry_pdf(rotation: u16) -> Vec<u8> {
    fixture::pdf(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 12 18] /Resources << >> /Contents 4 0 R >>".to_vec(),
        fixture::stream("", b"1 0 1 rg 0 0 12 18 re f"),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [10 20 90 60] /CropBox [20 25 80 55] /Rotate {rotation} /Resources << >> /Contents 6 0 R >>")
            .into_bytes(),
        fixture::stream("", b"1 0 0 rg 10 20 40 20 re f 0 1 0 rg 50 20 40 20 re f 0 0 1 rg 10 40 40 20 re f 1 1 0 rg 50 40 40 20 re f 0 0 0 rg 21 26 4 4 re f"),
    ])
}
fn rotated_pixel(x: u32, y: u32, width: u32, height: u32, rotation: u16) -> (u32, u32) {
    match rotation {
        90 => (height - 1 - y, x),
        180 => (width - 1 - x, height - 1 - y),
        270 => (y, width - 1 - x),
        _ => (x, y),
    }
}
fn calculator() -> Vec<u8> {
    fixture::page(
        "/S sh",
        "/Shading << /S 5 0 R >>",
        "",
        &[
            b"<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 64 0] /Function 6 0 R /Extend [true true] >>".to_vec(),
            fixture::stream("/FunctionType 4 /Domain [0 1] /Range [0 1 0 1 0 1]", b"{ pop 1 0 0 }"),
        ],
    )
}
/// Required coordinator desktop gate. CI has no user manager or namespace entitlement.
/// This opt-in test never retries without containment and must pass before shipping fallback.
#[test]
#[ignore = "requires the production Linux systemd/bwrap/Poppler runtime; coordinator must run explicitly"]
fn pdf_raster_linux_runtime_geometry_calculator_cancel_and_output_limit() {
    let bytes = Arc::<[u8]>::from(calculator());
    let request = |operation| Request { bytes: bytes.clone(), limits: ImportLimits::default(), operation };
    assert!(matches!(run(request(Operation::Metadata), &AtomicBool::new(false)).unwrap(), Reply::Metadata { pages: 1 }));
    let Reply::Pixels { width, height, rgba, .. } =
        run(request(Operation::Render { page: 0, dpi: 72.0, crop: true, apply_rotation: true }), &AtomicBool::new(false)).unwrap()
    else {
        panic!("pixels");
    };
    assert_eq!((width, height), (64, 64));
    // Independent analytic oracle: calculator always returns pure red.
    for p in rgba.as_chunks::<4>().0 {
        assert!(p[0] >= 254 && p[1] <= 1 && p[2] <= 1 && p[3] == 255, "{p:?}");
    }
    for jbig2 in [false, true] {
        let bytes = Arc::from(fixture::fallback_codec(jbig2));
        let Reply::Pixels { width, height, rgba, .. } = run(
            Request { bytes, limits: ImportLimits::default(), operation: Operation::Render { page: 0, dpi: 72.0, crop: true, apply_rotation: true } },
            &AtomicBool::new(false),
        )
        .unwrap() else {
            panic!("pixels");
        };
        assert_eq!((width, height), (64, 64));
        // Original pre-encoding pixels, independent of Poppler/our renderer.
        let colors = if jbig2 { [[0, 0, 0], [255, 255, 255], [255, 255, 255], [0, 0, 0]] } else { [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]] };
        for (x, y, index) in [(16, 16, 0), (48, 16, 1), (16, 48, 2), (48, 48, 3)] {
            let offset = (y * width + x) as usize * 4;
            assert_eq!(&rgba[offset..offset + 3], &colors[index], "codec jbig2={jbig2}");
            assert_eq!(rgba[offset + 3], 255);
        }
    }
    // Independent oracle: authored PDF-space quadrants and an off-centre black marker.
    // The expected samples derive from these coordinates, never from another Poppler run.
    for rotation in [0, 90, 180, 270] {
        for crop in [false, true] {
            for apply_rotation in [false, true] {
                let reply = run(
                    Request {
                        bytes: Arc::from(geometry_pdf(rotation)),
                        limits: ImportLimits::default(),
                        operation: Operation::Render { page: 1, dpi: 72.0, crop, apply_rotation },
                    },
                    &AtomicBool::new(false),
                )
                .unwrap();
                let Reply::Pixels { width, height, rgba, .. } = reply else {
                    panic!("pixels");
                };
                let (w, h) = if crop { (60, 30) } else { (80, 40) };
                let angle = if apply_rotation { rotation } else { 0 };
                assert_eq!((width, height), if angle == 90 || angle == 270 { (h, w) } else { (w, h) });
                let sample = |x, y, expected: [u8; 3]| {
                    let (x, y) = rotated_pixel(x, y, w, h, angle);
                    let index = ((y * width + x) * 4) as usize;
                    for c in 0..3 {
                        assert!(
                            rgba[index + c].abs_diff(expected[c]) <= 1,
                            "rotation={rotation},crop={crop},apply={apply_rotation},pixel={x},{y}: {:?}",
                            &rgba[index..index + 4]
                        );
                    }
                    assert_eq!(rgba[index + 3], 255);
                };
                sample(w / 4, h / 4, [0, 0, 255]);
                sample(3 * w / 4, h / 4, [255, 255, 0]);
                sample(w / 4, 3 * h / 4, [255, 0, 0]);
                sample(3 * w / 4, 3 * h / 4, [0, 255, 0]);
                let (x, y) = if crop { (3, 27) } else { (13, 32) };
                sample(x, y, [0, 0, 0]);
                sample(x + 6, y, [255, 0, 0]);
            }
        }
    }
    let mut runtime = Runtime::discover(&AtomicBool::new(false)).unwrap();
    runtime.tools.push(PathBuf::from("/usr/bin/sleep"));
    runtime.tools.push(PathBuf::from("/usr/bin/head"));
    let mut temp = Scratch::new().unwrap();
    private_file(&temp.path.join("input.pdf")).unwrap().write_all(&bytes).unwrap();
    private_file(&temp.path.join("fonts.conf")).unwrap().write_all(b"<fontconfig></fontconfig>").unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let sender = stop.clone();
    let unit = format!("{}-cancel.scope", temp.path.file_name().unwrap().to_str().unwrap());
    let waiting_unit = unit.clone();
    let trigger = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !control(&waiting_unit, &["is-active", "--quiet"]).is_some_and(|(active, _)| active) {
            assert!(Instant::now() < deadline, "contained waiting unit never became active");
            std::thread::sleep(Duration::from_millis(10));
        }
        sender.store(true, Ordering::Relaxed);
    });
    assert_eq!(execute(&runtime, &mut temp, "cancel", &["/usr/bin/sleep".into(), "30".into()], 1024, &stop).unwrap_err(), Error::Cancelled);
    trigger.join().unwrap();
    assert!(unit_terminal(&unit), "owned unit must have no live service after cancellation");
    assert!(
        execute(&runtime, &mut temp, "overflow", &["/usr/bin/head".into(), "-c".into(), "131072".into(), "/dev/zero".into()], 1024, &AtomicBool::new(false))
            .is_err()
    );
    let unit = format!("{}-overflow.scope", temp.path.file_name().unwrap().to_str().unwrap());
    assert!(unit_terminal(&unit));
}

#[test]
#[ignore = "internal child of the owned crash lifecycle gate"]
fn pdf_raster_owned_crash_child() {
    let path = std::env::var_os("LI_PDF_CRASH_SCRATCH").expect("only the crash gate starts this child");
    let mut temp = Scratch { path: path.into() };
    let mut runtime = Runtime::discover(&AtomicBool::new(false)).unwrap();
    runtime.tools.push(PathBuf::from("/usr/bin/sleep"));
    execute(&runtime, &mut temp, "crash", &["/usr/bin/sleep".into(), "30".into()], 1024, &AtomicBool::new(false)).unwrap();
}

#[test]
#[ignore = "requires the production Linux scope/bwrap runtime; kills only its owned test child"]
fn pdf_raster_parent_crash_reaps_namespace_descendants() {
    let temp = Scratch::new().unwrap();
    private_file(&temp.path.join("input.pdf")).unwrap().write_all(&calculator()).unwrap();
    private_file(&temp.path.join("fonts.conf")).unwrap().write_all(b"<fontconfig></fontconfig>").unwrap();
    let unit = format!("{}-crash.scope", temp.path.file_name().unwrap().to_str().unwrap());
    let test = format!("{}::pdf_raster_owned_crash_child", module_path!().split_once("::").unwrap().1);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", &test, "--nocapture"])
        .env("LI_PDF_CRASH_SCRATCH", &temp.path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let pids = loop {
        if let Some((true, path)) = control(&unit, &["show", "--property=ControlGroup", "--value"]) {
            let path = path.trim();
            if !path.is_empty() && Path::new(path).file_name() == Some(std::ffi::OsStr::new(&unit)) {
                let path = Path::new("/sys/fs/cgroup").join(path.trim_start_matches('/')).join("cgroup.procs");
                if let Ok(pids) = fs::read_to_string(path) {
                    let pids: Vec<u32> = pids.lines().filter_map(|s| s.parse().ok()).collect();
                    // bwrap, namespace init and the contained sleep must all have started.
                    if (3..=16).contains(&pids.len()) {
                        break pids;
                    }
                }
            }
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("crash child never entered its scope");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    child.kill().unwrap();
    child.wait().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let live: Vec<_> = pids.iter().filter(|pid| Path::new(&format!("/proc/{pid}")).exists()).collect();
        if live.is_empty() && unit_terminal(&unit) {
            break;
        }
        assert!(Instant::now() < deadline, "orphans after parent crash: {live:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
