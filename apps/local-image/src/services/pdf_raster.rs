//! Linux PDF image import through a per-job systemd scope and a private bubblewrap namespace.
//! Requires system Poppler, bubblewrap, prlimit, ldd and a systemd user session. No unconfined retry.
use photocraft_ui_egui::pdf_raster::{Error, MAX_PAGES, MAX_PIXELS, Operation, Reply, Request};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: u64 = 64 << 20;
const DIAGNOSTIC_LIMIT: u64 = 64 << 10;
const WALL: Duration = Duration::from_secs(8);
const SYSTEMD_RUN: &str = "/usr/bin/systemd-run";
const SYSTEMCTL: &str = "/usr/bin/systemctl";
const PDFINFO: &str = "/usr/bin/pdfinfo";
const PDFTOPPM: &str = "/usr/bin/pdftoppm";
#[derive(Default)]
struct Admission {
    active: bool,
    uncertain_unit: Option<String>,
}
static ADMISSION: Mutex<Admission> = Mutex::new(Admission { active: false, uncertain_unit: None });
struct Permit<'a> {
    slot: &'a Mutex<Admission>,
}
impl<'a> Permit<'a> {
    fn acquire(slot: &'a Mutex<Admission>) -> Result<Self, Error> {
        let mut state = slot.lock().map_err(|_| Error::Unavailable("PDF image admission is disabled for this session".into()))?;
        if let Some(unit) = &state.uncertain_unit {
            return Err(Error::Unavailable(format!("PDF image import is disabled for this session: cleanup of {unit} was not confirmed")));
        }
        if state.active {
            return Err(Error::Unavailable("another PDF image import is still stopping".into()));
        }
        state.active = true;
        Ok(Self { slot })
    }
    fn poison(slot: &Mutex<Admission>, unit: &str) {
        if let Ok(mut state) = slot.lock() {
            state.uncertain_unit = Some(unit.to_owned());
        }
    }
}
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.slot.lock() {
            state.active = false;
        }
    }
}
fn uncertain_job(unit: &str) {
    Permit::poison(&ADMISSION, unit);
}
fn failed(e: impl std::fmt::Display) -> Error {
    Error::Failed(e.to_string())
}
fn cancelled(flag: &AtomicBool) -> Result<(), Error> {
    if flag.load(Ordering::Relaxed) { Err(Error::Cancelled) } else { Ok(()) }
}
fn clean_command(program: &str) -> Command {
    let mut c = Command::new(program);
    c.env_clear().env("PATH", "/usr/bin:/bin").env("LC_ALL", "C");
    c
}
fn private_file(path: &Path) -> Result<File, Error> {
    OpenOptions::new().write(true).create_new(true).mode(0o600).open(path).map_err(failed)
}
struct Scratch {
    path: PathBuf,
}
impl Scratch {
    fn new() -> Result<Self, Error> {
        let mut random = [0_u8; 16];
        File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut random)).map_err(failed)?;
        let suffix: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let path = PathBuf::from(format!("/tmp/local-image-pdf-{suffix}"));
        fs::DirBuilder::new().mode(0o700).create(&path).map_err(failed)?;
        Ok(Self { path })
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        // Only individual immutable input/config files and preopened output descriptors
        // reach the child. Unlinking these owned paths is safe even if cleanup lost the bus;
        // no private source bytes are intentionally left behind on that error path.
        let _ = fs::remove_dir_all(&self.path);
    }
}
fn bounded_read(path: &Path, max: u64) -> Result<Vec<u8>, Error> {
    let meta = fs::symlink_metadata(path).map_err(failed)?;
    if !meta.file_type().is_file() || meta.len() > max {
        return Err(Error::Limit("output byte"));
    }
    let mut bytes = Vec::new();
    File::open(path).map_err(failed)?.take(max + 1).read_to_end(&mut bytes).map_err(failed)?;
    if bytes.len() as u64 > max {
        return Err(Error::Limit("output byte"));
    }
    Ok(bytes)
}
fn file_within(path: &Path, max: u64) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file() && m.len() <= max)
}
fn trusted_file(path: &Path) -> Result<PathBuf, Error> {
    let real = path.canonicalize().map_err(|_| Error::Unavailable(format!("missing system runtime file {}", path.display())))?;
    if !["/usr/bin", "/usr/lib", "/usr/lib64", "/lib", "/lib64"].iter().any(|p| real.starts_with(p)) || !real.is_file() {
        return Err(Error::Unavailable("runtime files must resolve inside system binary/library directories".into()));
    }
    Ok(real)
}
fn library_lines(text: &str) -> Result<BTreeMap<PathBuf, PathBuf>, Error> {
    let mut out = BTreeMap::new();
    if text.contains("not found") {
        return Err(Error::Unavailable("Poppler has missing shared libraries".into()));
    }
    for line in text.lines() {
        let words: Vec<_> = line.split_whitespace().collect();
        let candidate = if words.get(1) == Some(&"=>") { words.get(2).copied() } else { words.first().copied() };
        if let Some(path) = candidate.filter(|p| p.starts_with('/')) {
            let dest = PathBuf::from(path);
            if !["/usr/lib", "/usr/lib64", "/lib", "/lib64"].iter().any(|p| dest.starts_with(p))
                || dest.components().any(|p| matches!(p, std::path::Component::ParentDir))
            {
                return Err(Error::Unavailable("unexpected shared-library location".into()));
            }
            out.insert(dest.clone(), trusted_file(&dest)?);
        }
    }
    if out.is_empty() || out.len() > 128 {
        return Err(Error::Unavailable("could not identify a bounded system library closure".into()));
    }
    Ok(out)
}
#[derive(Clone)]
struct Runtime {
    libraries: BTreeMap<PathBuf, PathBuf>,
    data: Vec<PathBuf>,
    tools: Vec<PathBuf>,
}
impl Runtime {
    fn discover(cancel: &AtomicBool) -> Result<Self, Error> {
        for p in [SYSTEMD_RUN, SYSTEMCTL, "/usr/bin/env", "/usr/bin/prlimit", "/usr/bin/bwrap", "/usr/bin/ldd", PDFINFO, PDFTOPPM] {
            trusted_file(Path::new(p))?;
        }
        let temp = Scratch::new()?;
        let mut libraries = BTreeMap::new();
        for (index, program) in [PDFINFO, PDFTOPPM].iter().enumerate() {
            cancelled(cancel)?;
            let path = temp.path.join(format!("ldd-{index}"));
            let file = private_file(&path)?;
            // Only fixed trusted executables reach ldd; never a PDF/user-supplied executable.
            let mut child = clean_command("/usr/bin/ldd").arg(program).stdout(file.try_clone().map_err(failed)?).stderr(file).spawn().map_err(failed)?;
            let start = Instant::now();
            let status = loop {
                if let Some(status) = child.try_wait().map_err(failed)? {
                    break status;
                }
                if cancel.load(Ordering::Relaxed) || start.elapsed() > Duration::from_secs(2) || !file_within(&path, DIAGNOSTIC_LIMIT) {
                    let _ = child.kill();
                    let _ = child.wait();
                    cancelled(cancel)?;
                    return Err(Error::Unavailable("system library discovery exceeded its limit".into()));
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            if !status.success() {
                return Err(Error::Unavailable("system library discovery failed".into()));
            }
            let bytes = bounded_read(&path, DIAGNOSTIC_LIMIT)?;
            libraries.extend(library_lines(std::str::from_utf8(&bytes).map_err(failed)?)?);
        }
        let data = ["/usr/share/fonts", "/usr/share/poppler"]
            .into_iter()
            .filter_map(|p| {
                let path = Path::new(p);
                path.canonicalize().ok().filter(|real| real.starts_with("/usr/share") && real.is_dir()).map(|_| path.to_path_buf())
            })
            .collect();
        Ok(Self { libraries, data, tools: vec![PathBuf::from(PDFINFO), PathBuf::from(PDFTOPPM)] })
    }
    fn sandbox(&self, input: &Path, fonts: &Path, payload: &[String]) -> Vec<String> {
        let mut a: Vec<String> = [
            "/usr/bin/env",
            "-i",
            "PATH=/usr/bin:/bin",
            "/usr/bin/prlimit",
            "--as=536870912:536870912",
            "--cpu=3:3",
            "--fsize=67108864:67108864",
            "--nofile=128:128",
            "--core=0:0",
            "--",
            "/usr/bin/bwrap",
            "--unshare-all",
            "--die-with-parent",
            "--new-session",
            "--clearenv",
            "--cap-drop",
            "ALL",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        for binary in &self.tools {
            a.extend(["--ro-bind".into(), binary.to_string_lossy().into_owned(), binary.to_string_lossy().into_owned()]);
        }
        for (dest, source) in &self.libraries {
            a.extend(["--ro-bind".into(), source.to_string_lossy().into_owned(), dest.to_string_lossy().into_owned()]);
        }
        for path in &self.data {
            a.extend(["--ro-bind".into(), path.to_string_lossy().into_owned(), path.to_string_lossy().into_owned()]);
        }
        a.extend([
            "--ro-bind".into(),
            input.to_string_lossy().into_owned(),
            "/input.pdf".into(),
            "--ro-bind".into(),
            fonts.to_string_lossy().into_owned(),
            "/fonts.conf".into(),
        ]);
        // All renderer-created paths live in memory-accounted private tmpfs. Only preopened
        // stdout/stderr descriptors can write host files, each under RLIMIT_FSIZE.
        a.extend(
            [
                "--tmpfs",
                "/output",
                "--dev",
                "/dev",
                "--chdir",
                "/output",
                "--setenv",
                "PATH",
                "/usr/bin",
                "--setenv",
                "LC_ALL",
                "C",
                "--setenv",
                "FONTCONFIG_FILE",
                "/fonts.conf",
                "--setenv",
                "XDG_CACHE_HOME",
                "/output/cache",
                "--remount-ro",
                "/",
                "--",
            ]
            .into_iter()
            .map(str::to_owned),
        );
        a.extend_from_slice(payload);
        a
    }
}
fn user_command(program: &str) -> Result<Command, Error> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).ok_or_else(|| Error::Unavailable("a systemd user session is required".into()))?;
    let valid = runtime.parent() == Some(Path::new("/run/user"))
        && runtime.file_name().is_some_and(|s| !s.is_empty() && s.to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
        && runtime.join("bus").exists();
    if !valid {
        return Err(Error::Unavailable("a local systemd user bus is required".into()));
    }
    let mut c = clean_command(program);
    c.env("XDG_RUNTIME_DIR", &runtime).env("DBUS_SESSION_BUS_ADDRESS", format!("unix:path={}/bus", runtime.display()));
    Ok(c)
}
fn control(unit: &str, args: &[&str]) -> Option<(bool, String)> {
    let mut c = user_command(SYSTEMCTL).ok()?;
    let mut child =
        c.args(["--user", "--no-ask-password"]).args(args).arg(unit).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let start = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            let mut bytes = Vec::new();
            child.stdout.take()?.take(4097).read_to_end(&mut bytes).ok()?;
            if bytes.len() > 4096 {
                return None;
            }
            return Some((status.success(), String::from_utf8(bytes).ok()?));
        }
        if start.elapsed() > Duration::from_secs(2) {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn unit_terminal(unit: &str) -> bool {
    control(unit, &["show", "--property=ActiveState", "--property=LoadState"])
        .is_some_and(|(_, s)| s.lines().any(|line| matches!(line, "ActiveState=inactive" | "ActiveState=failed")))
}
struct Job {
    unit: String,
    child: Child,
    terminal: bool,
}
impl Job {
    fn stop(&mut self) -> bool {
        if self.terminal {
            return true;
        }
        let _ = control(&self.unit, &["stop", "--no-block"]);
        let start = Instant::now();
        loop {
            if self.child.try_wait().ok().flatten().is_some() {
                // Scope mode execs the sandbox in this owned child (systemd run.c).
                // After it has exited, no delayed service request can create a renderer.
                // Also stop the whole scope to reap any namespace descendants.
                let _ = control(&self.unit, &["stop", "--no-block"]);
                if unit_terminal(&self.unit) {
                    self.terminal = true;
                    return true;
                }
            }
            if start.elapsed() > Duration::from_secs(2) {
                let _ = control(&self.unit, &["kill", "--kill-whom=all", "--signal=KILL"]);
            }
            if start.elapsed() > Duration::from_secs(5) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        if !self.stop() {
            uncertain_job(&self.unit);
            // RuntimeMaxSec independently bounds the service even if the user bus disappears.
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = control(&self.unit, &["stop", "--no-block"]);
        }
    }
}
fn execute(runtime: &Runtime, temp: &mut Scratch, label: &str, payload: &[String], max: u64, cancel: &AtomicBool) -> Result<Vec<u8>, Error> {
    cancelled(cancel)?;
    let suffix = temp.path.file_name().and_then(|p| p.to_str()).ok_or_else(|| failed("private path"))?;
    let unit = format!("{suffix}-{label}.scope");
    let stdout = temp.path.join(format!("{label}.out"));
    let stderr = temp.path.join(format!("{label}.err"));
    let output = private_file(&stdout)?;
    let diagnostic = private_file(&stderr)?;
    let mut c = user_command(SYSTEMD_RUN)?;
    c.args(["--user", "--scope", "--no-ask-password", "--collect", "--quiet", "--nice=5", "--expand-environment=no"])
        .arg(format!("--unit={unit}"))
        .args([
            "-p",
            "CPUQuota=100%",
            "-p",
            "MemoryMax=768M",
            "-p",
            "MemorySwapMax=0",
            "-p",
            "TasksMax=16",
            "-p",
            "RuntimeMaxSec=10s",
            "-p",
            "TimeoutStopSec=1s",
            "-p",
            "KillMode=control-group",
            "-p",
            "KillSignal=SIGKILL",
        ])
        .arg("--")
        .args(runtime.sandbox(&temp.path.join("input.pdf"), &temp.path.join("fonts.conf"), payload))
        .stdin(Stdio::null())
        .stdout(output)
        .stderr(diagnostic);
    let child = c.spawn().map_err(|e| Error::Unavailable(format!("cannot start contained renderer: {e}")))?;
    #[cfg(test)]
    if label == "render" {
        *RENDER_UNIT.lock().map_err(failed)? = Some(unit.clone());
    }
    let mut job = Job { unit, child, terminal: false };
    let start = Instant::now();
    let status = loop {
        let problem = if cancel.load(Ordering::Relaxed) {
            Some(Error::Cancelled)
        } else if start.elapsed() > WALL {
            Some(Error::Limit("time"))
        } else if !file_within(&stdout, max) || !file_within(&stderr, DIAGNOSTIC_LIMIT) {
            Some(Error::Limit("output byte"))
        } else {
            None
        };
        if let Some(error) = problem {
            if !job.stop() {
                uncertain_job(&job.unit);
                return Err(failed("renderer cleanup did not complete; its runtime deadline remains enforced"));
            }
            return Err(error);
        }
        if let Some(status) = job.child.try_wait().map_err(failed)? {
            // Success still requires an empty scope; no surviving descendant may escape.
            job.terminal = status.success() && unit_terminal(&job.unit);
            break status;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if !job.stop() {
        uncertain_job(&job.unit);
        return Err(failed("renderer cleanup did not complete; no page was imported"));
    }
    if !status.success() {
        let diagnostic = bounded_read(&stderr, DIAGNOSTIC_LIMIT).unwrap_or_default();
        let text = String::from_utf8_lossy(&diagnostic);
        return Err(Error::Unavailable(format!("contained renderer failed (no unconfined retry): {}", text.chars().take(600).collect::<String>())));
    }
    cancelled(cancel)?;
    let diagnostic = bounded_read(&stderr, DIAGNOSTIC_LIMIT)?;
    if diagnostic.iter().any(|b| !b.is_ascii_whitespace()) {
        // Poppler may exit successfully after omitting an undecodable image/font. Do not
        // publish that partial page as a faithful fallback merely because PNG encoding worked.
        let text = String::from_utf8_lossy(&diagnostic);
        return Err(failed(format!("renderer reported a warning; no page was imported: {}", text.chars().take(600).collect::<String>())));
    }
    bounded_read(&stdout, max)
}
#[derive(Debug)]
struct Page {
    pages: usize,
    media: [f64; 4],
    crop: [f64; 4],
    rotation: u16,
}
fn metadata(bytes: &[u8], number: usize) -> Result<Page, Error> {
    let text = std::str::from_utf8(bytes).map_err(failed)?;
    let mut pages = None;
    let mut media = None;
    let mut crop = None;
    let mut rotation = None;
    for line in text.lines() {
        if let Some(n) = line.strip_prefix("Pages:") {
            if pages.is_some() {
                return Err(failed("duplicate page count"));
            }
            pages = Some(n.trim().parse::<usize>().map_err(failed)?);
        }
        let (prefix, value) = match line.split_once(':') {
            Some(x) => x,
            None => continue,
        };
        let parts: Vec<_> = prefix.split_whitespace().collect();
        if parts.len() != 3 || parts[0] != "Page" || parts[1].parse::<usize>().ok() != Some(number) {
            continue;
        }
        match parts[2] {
            "MediaBox" | "CropBox" => {
                let values: Vec<f64> = value.split_whitespace().map(|v| v.parse::<f64>().map_err(failed)).collect::<Result<_, _>>()?;
                let rect: [f64; 4] = values.try_into().map_err(|_| failed("invalid page box"))?;
                if rect.iter().any(|v| !v.is_finite() || v.abs() > 1_000_000.0) || rect[2] <= rect[0] || rect[3] <= rect[1] {
                    return Err(failed("invalid page box"));
                }
                let slot = if parts[2] == "MediaBox" { &mut media } else { &mut crop };
                if slot.replace(rect).is_some() {
                    return Err(failed("duplicate page box"));
                }
            }
            "rot" => {
                let r = value.trim().parse::<u16>().map_err(failed)?;
                if ![0, 90, 180, 270].contains(&r) || rotation.replace(r).is_some() {
                    return Err(failed("invalid page rotation"));
                }
            }
            _ => {}
        }
    }
    let pages = pages.ok_or_else(|| failed("missing page count"))?;
    if pages == 0 || pages > MAX_PAGES || number == 0 || number > pages {
        return Err(Error::Limit("page"));
    }
    Ok(Page {
        pages,
        media: media.ok_or_else(|| failed("missing MediaBox"))?,
        crop: crop.ok_or_else(|| failed("missing CropBox"))?,
        rotation: rotation.ok_or_else(|| failed("missing page rotation"))?,
    })
}
fn dimensions(page: &Page, dpi: f32, crop: bool, limit: u64) -> Result<(u32, u32), Error> {
    if !dpi.is_finite() || !(1.0..=2400.0).contains(&dpi) {
        return Err(failed("DPI must be between 1 and 2400"));
    }
    let b = if crop { page.crop } else { page.media };
    let width = ((b[2] - b[0]) * f64::from(dpi) / 72.0).ceil();
    let height = ((b[3] - b[1]) * f64::from(dpi) / 72.0).ceil();
    if width < 1.0 || height < 1.0 || width > 1_000_000.0 || height > 1_000_000.0 || width * height > limit.min(MAX_PIXELS) as f64 {
        return Err(Error::Limit("page pixel"));
    }
    Ok(if page.rotation == 90 || page.rotation == 270 { (height as u32, width as u32) } else { (width as u32, height as u32) })
}
pub(super) fn run(request: Request, cancel: &AtomicBool) -> Result<Reply, Error> {
    lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
    cancelled(cancel)?;
    if request.bytes.len() > request.limits.bytes.min(64 << 20) {
        return Err(Error::Limit("input byte"));
    }
    let _permit = Permit::acquire(&ADMISSION)?;
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    let runtime = match RUNTIME.get() {
        Some(r) => r.clone(),
        None => {
            let r = Runtime::discover(cancel)?;
            let _ = RUNTIME.set(r.clone());
            r
        }
    };
    let mut temp = Scratch::new()?;
    private_file(&temp.path.join("input.pdf"))?.write_all(&request.bytes).map_err(failed)?;
    private_file(&temp.path.join("fonts.conf"))?.write_all(b"<?xml version=\"1.0\"?><!DOCTYPE fontconfig SYSTEM \"urn:fontconfig:fonts.dtd\"><fontconfig><dir>/usr/share/fonts</dir><cachedir>/output/font-cache</cachedir></fontconfig>").map_err(failed)?;
    let number = match request.operation {
        Operation::Metadata => 1,
        Operation::Render { page, .. } => page.checked_add(1).filter(|p| *p <= MAX_PAGES).ok_or(Error::Limit("page"))?,
    };
    let args = [PDFINFO.to_string(), "-f".into(), number.to_string(), "-l".into(), number.to_string(), "-box".into(), "/input.pdf".into()];
    let info = metadata(&execute(&runtime, &mut temp, "info", &args, DIAGNOSTIC_LIMIT, cancel)?, number)?;
    if info.pages > request.limits.objects {
        return Err(Error::Limit("page"));
    }
    let Operation::Render { dpi, crop, apply_rotation, .. } = request.operation else { return Ok(Reply::Metadata { pages: info.pages }) };
    let expected = dimensions(&info, dpi, crop, request.limits.pixels)?;
    let mut args = vec![
        PDFTOPPM.into(),
        "-f".into(),
        number.to_string(),
        "-l".into(),
        number.to_string(),
        "-singlefile".into(),
        "-r".into(),
        dpi.to_string(),
        "-png".into(),
    ];
    if crop {
        args.push("-cropbox".into());
    }
    args.push("/input.pdf".into()); // one PNG on stdout, no output prefix
    let png = execute(&runtime, &mut temp, "render", &args, OUTPUT_LIMIT, cancel)?;
    cancelled(cancel)?;
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(failed("renderer did not return PNG"));
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&png), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(expected.0);
    limits.max_image_height = Some(expected.1);
    limits.max_alloc = Some(128 << 20);
    reader.limits(limits);
    let decoded = reader.decode().map_err(failed)?;
    if (decoded.width(), decoded.height()) != expected {
        return Err(failed("rendered dimensions disagree with bounded page metadata"));
    }
    let rgba = decoded.to_rgba8();
    let rgba = if apply_rotation {
        rgba
    } else {
        match info.rotation {
            90 => image::imageops::rotate270(&rgba),
            180 => image::imageops::rotate180(&rgba),
            270 => image::imageops::rotate90(&rgba),
            _ => rgba,
        }
    };
    cancelled(cancel)?;
    Ok(Reply::Pixels { width: rgba.width(), height: rgba.height(), dpi, rgba: rgba.into_raw() })
}
#[cfg(test)]
#[path = "pdf_raster_tests.rs"]
mod tests;

#[cfg(test)]
static RENDER_UNIT: Mutex<Option<String>> = Mutex::new(None);
#[cfg(test)]
#[allow(dead_code)] // Used by the UI crate's production-provider acceptance harness.
pub(super) fn running_render_unit() -> Option<String> {
    let unit = RENDER_UNIT.lock().ok()?.clone()?;
    control(&unit, &["is-active", "--quiet"]).is_some_and(|(active, _)| active).then_some(unit)
}
#[cfg(test)]
#[allow(dead_code)] // Used by the UI crate's production-provider acceptance harness.
pub(super) fn stopped_render_unit(unit: &str) -> bool {
    unit_terminal(unit)
}
