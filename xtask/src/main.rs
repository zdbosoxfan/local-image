//! Build tasks for Local Image, in Rust (`cargo xtask <task>`):
//!
//! ```text
//! cargo xtask package linux [--skip-build]
//!     dist/release/local-image-<version>-linux-<arch>.tar.gz (+ .sha256): the app and CLI,
//!     README, LICENSE and licenses/. Unpack and run `./local-image --install` (or
//!     `--install --system`) to add it to the application menu.
//! cargo xtask package windows [--arch x64|x86|arm64] [--skip-build]
//!     an MSI (WiX v5: `dotnet tool install -g wix`) and a portable zip, both signed when signing
//!     secrets are set (see `sign`).
//! cargo xtask check-icons [path/to/local-image.wxs]
//!     the MSI's advertised-shortcut icon references (ICE50), without building.
//! cargo xtask upstream-check [--offline]
//!     for every algorithm ported from another project (docs/PORTS.md), the upstream commits that
//!     touched its source file since the commit it was ported from — what to review and re-port.
//!     Clones go to target/upstream/ (blob-less); --offline skips fetching.
//! cargo xtask attributions [--fetch]
//!     regenerate assets/attributions.json (Settings › Attributions) from
//!     assets/attributions-curated.json, docs/PORTS.md, the AI model tables and Cargo.lock.
//!     Offline; --fetch first runs `cargo fetch` so every crate's licence can be read.
//! cargo xtask sign <files…>
//!     Authenticode signing with signtool, from WINDOWS_CERTIFICATE (+ _PASSWORD) or Azure Trusted
//!     Signing (AZURE_TENANT_ID, AZURE_CLIENT_ID, AZURE_CLIENT_SECRET, AZURE_SIGNING_ENDPOINT,
//!     AZURE_SIGNING_ACCOUNT, AZURE_CERT_PROFILE); with neither, files stay unsigned.
//! ```
//!
//! The version comes from `[workspace.package] version` in the root `Cargo.toml`
//! (`LOCAL_IMAGE_VERSION` overrides it).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

mod assets;
mod attributions;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask sits in the workspace").to_path_buf()
}

fn version() -> Result<String> {
    if let Ok(v) = std::env::var("LOCAL_IMAGE_VERSION")
        && !v.is_empty()
    {
        return Ok(v);
    }
    let toml = std::fs::read_to_string(root().join("Cargo.toml"))?;
    let mut in_pkg = false;
    for line in toml.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_pkg = l == "[workspace.package]";
            continue;
        }
        if in_pkg
            && let Some(v) = l.strip_prefix("version")
            && let Some(v) = v.trim().strip_prefix('=')
        {
            return Ok(v.trim().trim_matches('"').to_owned());
        }
    }
    bail!("could not read [workspace.package] version from Cargo.toml")
}

fn run(what: &str, cmd: &mut Command) -> Result<()> {
    println!("==> {what}");
    let status = cmd.status().with_context(|| format!("{what}: could not start {:?}", cmd.get_program()))?;
    if !status.success() {
        bail!("{what} failed ({status})");
    }
    Ok(())
}

fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| root().join("target"))
}

fn dist() -> Result<PathBuf> {
    let d = std::env::var_os("DIST").map(PathBuf::from).unwrap_or_else(|| root().join("dist").join("release"));
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &dest)?;
        } else {
            std::fs::copy(e.path(), dest)?;
        }
    }
    Ok(())
}

fn sha256_hex(path: &Path) -> Result<String> {
    use sha2::Digest as _;
    let mut h = sha2::Sha256::new();
    std::io::copy(&mut std::fs::File::open(path)?, &mut h)?;
    Ok(hex::encode(h.finalize()))
}

fn build_env(cmd: &mut Command) {
    let sha = Command::new("git").args(["rev-parse", "HEAD"]).current_dir(root()).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    if let Some(sha) = sha.filter(|s| !s.is_empty()) {
        cmd.env("PHOTOCRAFT_BUILD_SHA", sha);
    }
    // UTC date without a date crate: days since the epoch to y-m-d (civil-from-days).
    let days = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    cmd.env("PHOTOCRAFT_BUILD_DATE", format!("{y:04}-{m:02}-{d:02}"));
}

// ------------------------------------------------------------------------------ Linux

fn package_linux(skip_build: bool) -> Result<()> {
    let v = version()?;
    let arch = std::env::consts::ARCH;
    println!("Local Image {v} for Linux {arch}");
    if !skip_build {
        let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
        c.args(["build", "--release", "--locked", "-p", "local-image", "-p", "local-image-cli"]).current_dir(root());
        build_env(&mut c);
        run("cargo build", &mut c)?;
    }
    let name = format!("local-image-{v}-linux-{arch}");
    let stage = target_dir().join("linux-package").join(&name);
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage)?;
    for bin in ["local-image", "local-image-cli"] {
        let from = target_dir().join("release").join(bin);
        std::fs::copy(&from, stage.join(bin)).with_context(|| format!("{} (build first, or drop --skip-build)", from.display()))?;
    }
    for f in ["README.md", "LICENSE"] {
        std::fs::copy(root().join(f), stage.join(f))?;
    }
    copy_dir(&root().join("licenses"), &stage.join("licenses"))?;
    run("local-image-cli --version", Command::new(stage.join("local-image-cli")).arg("--version"))?;
    let out = dist()?.join(format!("{name}.tar.gz"));
    {
        let gz = flate2::write::GzEncoder::new(std::fs::File::create(&out)?, flate2::Compression::best());
        let mut tar = tar::Builder::new(gz);
        tar.mode(tar::HeaderMode::Deterministic);
        tar.append_dir_all(&name, &stage)?;
        tar.into_inner()?.finish()?;
    }
    std::fs::write(out.with_extension("gz.sha256"), format!("{}  {name}.tar.gz\n", sha256_hex(&out)?))?;
    println!("{} ({} MB)", out.display(), std::fs::metadata(&out)?.len() / 1_000_000);
    Ok(())
}

// ------------------------------------------------------------------------------ Windows

/// `(PE machine, subsystem)` of an executable.
fn pe_header(path: &Path) -> Result<(u16, u16)> {
    let b = std::fs::read(path)?;
    let at = |o: usize, n: usize| b.get(o..o + n).context("not a PE file");
    let pe = u32::from_le_bytes(at(0x3C, 4)?.try_into()?) as usize;
    let machine = u16::from_le_bytes(at(pe + 4, 2)?.try_into()?);
    let subsystem = u16::from_le_bytes(at(pe + 0x5C, 2)?.try_into()?);
    Ok((machine, subsystem))
}

fn package_windows(arch: &str, skip_build: bool) -> Result<()> {
    let v = version()?;
    // MSI ProductVersion is numeric (major.minor.build): pre-release tags are dropped there.
    let msi_version = v.split('-').next().unwrap_or(&v).to_owned();
    let (target, machine) = match arch {
        "x64" => ("x86_64-pc-windows-msvc", 0x8664),
        "x86" => ("i686-pc-windows-msvc", 0x14C),
        "arm64" => ("aarch64-pc-windows-msvc", 0xAA64),
        a => bail!("unknown --arch {a} (x64, x86, arm64)"),
    };
    println!("Local Image {v} for Windows {arch} ({target})");
    if !skip_build {
        let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
        c.args(["build", "--release", "--locked", "-p", "local-image", "-p", "local-image-cli", "--features", "local-image/heif", "--target", target])
            .current_dir(root());
        // Static CRT: no VC++ redistributable needed; scoped to the target so build scripts and
        // proc-macros are unaffected.
        c.env(format!("CARGO_TARGET_{}_RUSTFLAGS", target.to_uppercase().replace('-', "_")), "-C target-feature=+crt-static");
        // Fail the build (rather than warn) if the icon and VERSIONINFO can't be embedded.
        c.env("LOCAL_IMAGE_REQUIRE_WINRES", "1");
        build_env(&mut c);
        run("cargo build", &mut c)?;
    }
    let bin = target_dir().join(target).join("release");
    // The app must be a GUI program (no console window); the CLI a console one.
    for (exe, subsystem) in [("local-image.exe", 2u16), ("local-image-cli.exe", 3)] {
        let (m, s) = pe_header(&bin.join(exe))?;
        if m != machine {
            bail!("{exe} is for machine {m:#x}, expected {machine:#x} ({arch})");
        }
        if s != subsystem {
            bail!("{exe} has PE subsystem {s}, expected {subsystem}");
        }
        println!("ok {exe}: {arch}, PE subsystem {s}");
    }
    let stage = target_dir().join("windows-package").join(arch);
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage)?;
    let exes: Vec<PathBuf> = ["local-image.exe", "local-image-cli.exe"].iter().map(|e| stage.join(e)).collect();
    for e in &exes {
        std::fs::copy(bin.join(e.file_name().expect("file name")), e)?;
    }
    sign(&exes)?;

    // MSI.
    let wxs = root().join("packaging").join("windows").join("local-image.wxs");
    check_icons(&wxs)?;
    let msi = dist()?.join(format!("local-image-{v}-windows-{arch}.msi"));
    run(
        "wix build",
        Command::new("wix")
            .arg("build")
            .arg(&wxs)
            .args(["-arch", arch, "-d", &format!("Version={msi_version}"), "-d"])
            .arg(format!("BinDir={}", stage.display()))
            .arg("-d")
            .arg(format!("IconPath={}", root().join("assets").join("app-icon").join("local-image.ico").display()))
            .arg("-o")
            .arg(&msi),
    )?;
    run(
        "MSI shortcut icon validation (ICE50)",
        Command::new("wix").args(["msi", "validate"]).arg(&msi).args(["-ice", "ICE50", "-intermediateFolder"]).arg(stage.join("msi-validation")),
    )?;
    let _ = std::fs::remove_file(msi.with_extension("wixpdb"));
    sign(std::slice::from_ref(&msi))?;

    // Portable zip: the executables, README, licences, and portable.txt (settings beside the exe).
    let name = format!("local-image-{v}-windows-{arch}-portable");
    let zip_path = dist()?.join(format!("{name}.zip"));
    let mut z = zip::ZipWriter::new(std::fs::File::create(&zip_path)?);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut add = |rel: &str, path: &Path| -> Result<()> {
        z.start_file(format!("{name}/{rel}"), opts)?;
        std::io::copy(&mut std::fs::File::open(path)?, &mut z)?;
        Ok(())
    };
    for e in &exes {
        add(&e.file_name().expect("file name").to_string_lossy(), e)?;
    }
    for f in ["README.md", "LICENSE"] {
        add(f, &root().join(f))?;
    }
    add("portable.txt", &root().join("packaging").join("windows").join("portable.txt"))?;
    for e in std::fs::read_dir(root().join("licenses"))? {
        let e = e?;
        add(&format!("licenses/{}", e.file_name().to_string_lossy()), &e.path())?;
    }
    z.finish()?;

    let host_arm = std::env::consts::ARCH == "aarch64";
    if arch != "arm64" || host_arm {
        run("local-image-cli --version", Command::new(&exes[1]).arg("--version"))?;
    } else {
        println!("skipping local-image-cli --version: an {arch} build doesn't run on this machine");
    }
    println!("{}\n{}", msi.display(), zip_path.display());
    Ok(())
}

// ------------------------------------------------------------------------------ check-icons

/// ICE50: advertised shortcut icon ids must exist and carry the target file's extension
/// (Windows Installer caches the icon under that id; a wrong extension shows a generic icon).
fn check_icons(wxs: &Path) -> Result<()> {
    use quick_xml::events::Event;
    let xml = std::fs::read_to_string(wxs).with_context(|| wxs.display().to_string())?;
    let mut reader = quick_xml::Reader::from_str(&xml);
    let attr = |e: &quick_xml::events::BytesStart, k: &str| -> Option<String> {
        e.attributes().flatten().find(|a| a.key.as_ref() == k.as_bytes()).and_then(|a| a.unescape_value().ok()).map(|v| v.into_owned())
    };
    let (mut icons, mut shortcuts, mut arp) = (Vec::new(), Vec::new(), None);
    let mut file_stack: Vec<Option<String>> = Vec::new();
    loop {
        match reader.read_event()? {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == b"Icon" => icons.extend(attr(&e, "Id")),
            Event::Start(e) if e.name().as_ref() == b"File" => file_stack.push(attr(&e, "Name").or_else(|| attr(&e, "Source"))),
            Event::End(e) if e.name().as_ref() == b"File" => {
                file_stack.pop();
            }
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == b"Shortcut" && attr(&e, "Advertise").as_deref() == Some("yes") => {
                let file = file_stack.last().cloned().flatten().unwrap_or_default();
                shortcuts.push((attr(&e, "Id").unwrap_or_default(), attr(&e, "Icon").unwrap_or_default(), file));
            }
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == b"Property" && attr(&e, "Id").as_deref() == Some("ARPPRODUCTICON") => {
                arp = attr(&e, "Value")
            }
            _ => {}
        }
    }
    let ext = |s: &str| s.rsplit_once('.').map(|(_, e)| format!(".{}", e.to_ascii_lowercase())).unwrap_or_default();
    for (id, icon, file) in &shortcuts {
        if !icons.contains(icon) {
            bail!("Advertised shortcut '{id}' references missing icon '{icon}'.");
        }
        if !matches!(ext(icon).as_str(), ".exe" | ".ico") {
            bail!("Advertised shortcut '{id}' icon '{icon}' must have an .exe or .ico extension (ICE50).");
        }
        // Source paths use Windows separators even when this runs on Linux.
        let name = file.rsplit(['\\', '/']).next().unwrap_or(file);
        if ext(icon) != ext(name) {
            bail!("Advertised shortcut '{id}' icon '{icon}' must match target '{name}' extension (ICE50).");
        }
    }
    if let Some(a) = arp
        && !icons.contains(&a)
    {
        bail!("ARPPRODUCTICON references missing icon '{a}'.");
    }
    println!("Windows shortcut icon references ok");
    Ok(())
}

// ------------------------------------------------------------------------------ sign

fn warn(msg: &str) {
    if std::env::var_os("GITHUB_ACTIONS").is_some() {
        println!("::warning::{msg}");
    } else {
        eprintln!("warning: {msg}");
    }
}

fn find_signtool() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("SIGNTOOL") {
        return Ok(p.into());
    }
    if Command::new("signtool.exe").arg("/?").output().is_ok() {
        return Ok("signtool.exe".into());
    }
    let kits = PathBuf::from(std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| r"C:\Program Files (x86)".into())).join(r"Windows Kits\10\bin");
    let mut found = Vec::new();
    for e in std::fs::read_dir(&kits).into_iter().flatten().flatten() {
        let p = e.path().join("x64").join("signtool.exe");
        if p.exists() {
            found.push(p);
        }
    }
    found.sort();
    found.pop().context("signtool.exe not found (install the Windows SDK)")
}

fn sign(files: &[PathBuf]) -> Result<()> {
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let azure = ["AZURE_TENANT_ID", "AZURE_CLIENT_ID", "AZURE_CLIENT_SECRET", "AZURE_SIGNING_ENDPOINT", "AZURE_SIGNING_ACCOUNT", "AZURE_CERT_PROFILE"];
    let have_cert = var("WINDOWS_CERTIFICATE").is_some();
    let have_azure = azure.iter().all(|k| var(k).is_some());
    let names: Vec<String> = files.iter().map(|f| f.display().to_string()).collect();
    if !have_cert && !have_azure {
        warn(&format!("Windows signing secrets not set (WINDOWS_CERTIFICATE or AZURE_*): leaving unsigned: {}", names.join(", ")));
        return Ok(());
    }
    let tool = find_signtool()?;
    let tmp = std::env::temp_dir().join(format!("local-image-sign-{}", std::process::id()));
    std::fs::create_dir_all(&tmp)?;
    let result = (|| -> Result<()> {
        let common = ["sign", "/v", "/fd", "SHA256", "/td", "SHA256", "/d", "Local Image", "/du", "https://github.com/zdbosoxfan/local-image"];
        let mut c = Command::new(&tool);
        c.args(common);
        if have_cert {
            use base64::Engine as _;
            println!("Signing with WINDOWS_CERTIFICATE: {}", names.join(", "));
            let pfx = tmp.join("cert.pfx");
            std::fs::write(&pfx, base64::engine::general_purpose::STANDARD.decode(var("WINDOWS_CERTIFICATE").unwrap_or_default().trim())?)?;
            c.args(["/tr", &var("WINDOWS_TIMESTAMP_URL").unwrap_or_else(|| "http://timestamp.digicert.com".into()), "/f"]).arg(&pfx);
            if let Some(p) = var("WINDOWS_CERTIFICATE_PASSWORD") {
                c.args(["/p", &p]);
            }
        } else {
            println!("Signing with Azure Trusted Signing: {}", names.join(", "));
            // The dlib authenticates with DefaultAzureCredential (AZURE_TENANT_ID, AZURE_CLIENT_ID,
            // AZURE_CLIENT_SECRET from the environment).
            let mut body = Vec::new();
            ureq::get("https://www.nuget.org/api/v2/package/Microsoft.Trusted.Signing.Client").call()?.body_mut().as_reader().read_to_end(&mut body)?;
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(body))?;
            zip.extract(tmp.join("client"))?;
            let dlib = find_file(&tmp.join("client"), "Azure.CodeSigning.Dlib.dll", "x64")
                .context("Azure.CodeSigning.Dlib.dll not found in Microsoft.Trusted.Signing.Client")?;
            let metadata = tmp.join("metadata.json");
            std::fs::write(
                &metadata,
                serde_json::to_vec_pretty(&serde_json::json!({
                    "Endpoint": var("AZURE_SIGNING_ENDPOINT"),
                    "CodeSigningAccountName": var("AZURE_SIGNING_ACCOUNT"),
                    "CertificateProfileName": var("AZURE_CERT_PROFILE"),
                }))?,
            )?;
            c.args(["/tr", &var("WINDOWS_TIMESTAMP_URL").unwrap_or_else(|| "http://timestamp.acs.microsoft.com".into()), "/dlib"])
                .arg(dlib)
                .arg("/dmdf")
                .arg(&metadata);
        }
        run("signtool sign", c.args(files))?;
        run("signtool verify", Command::new(&tool).args(["verify", "/pa", "/v"]).args(files))
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

use std::io::Read as _;

fn find_file(dir: &Path, name: &str, under: &str) -> Option<PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(f) = find_file(&p, name, under) {
                return Some(f);
            }
        } else if p.file_name().is_some_and(|n| n == name) && p.components().any(|c| c.as_os_str() == under) {
            return Some(p);
        }
    }
    None
}

// ------------------------------------------------------------------------------ main

const USAGE: &str = "usage: cargo xtask <task>
  package linux [--skip-build]
  package windows [--arch x64|x86|arm64] [--skip-build]
  check-icons [file.wxs]
  upstream-check [--offline]
  assets
  tool-icons
  attributions [--fetch]
  sign <files…>";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let value = |f: &str| args.iter().position(|a| a == f).and_then(|i| args.get(i + 1)).cloned();
    match args.first().map(String::as_str) {
        Some("package") => match args.get(1).map(String::as_str) {
            Some("linux") => package_linux(flag("--skip-build")),
            Some("windows") => package_windows(&value("--arch").unwrap_or_else(|| "x64".into()), flag("--skip-build")),
            _ => bail!("{USAGE}"),
        },
        Some("check-icons") => check_icons(&args.get(1).map(PathBuf::from).unwrap_or_else(|| root().join("packaging").join("windows").join("local-image.wxs"))),
        Some("upstream-check") => upstream_check(flag("--offline")),
        Some("assets") => assets::run(&root()),
        Some("tool-icons") => run(
            "render tool icon contact sheet",
            Command::new("cargo").current_dir(root()).env("CARGO_BUILD_JOBS", "3").args([
                "+1.98.1",
                "test",
                "--offline",
                "--lib",
                "-p",
                "photocraft-ui-egui",
                "write_tool_icon_contact_sheet",
                "--",
                "--ignored",
                "--nocapture",
            ]),
        ),
        Some("attributions") => attributions::run(&root(), flag("--fetch")),
        Some("sign") if args.len() > 1 => sign(&args[1..].iter().map(PathBuf::from).collect::<Vec<_>>()),
        _ => {
            println!("{USAGE}");
            Ok(())
        }
    }
}

// ------------------------------------------------------------------ upstream-check

/// A row of docs/PORTS.md: one ported algorithm.
#[derive(Debug, PartialEq)]
struct Port {
    ours: String,
    project: String,
    path: String,
    commit: String,
}

/// Where each upstream project's git history lives.
fn upstream_repo(project: &str) -> Option<&'static str> {
    match project.to_ascii_lowercase().as_str() {
        "darktable" => Some("https://github.com/darktable-org/darktable"),
        "rawtherapee" => Some("https://github.com/Beep6581/RawTherapee"),
        "art" => Some("https://github.com/artpixls/ART"),
        "vkdt" => Some("https://github.com/hanatos/vkdt"),
        "ansel" => Some("https://github.com/aurelienpierreeng/ansel"),
        "lightzone" => Some("https://github.com/ktgw0316/LightZone"),
        _ => None,
    }
}

/// The table rows of PORTS.md: columns are found by their header (`Our file`, `Upstream`/
/// `Project`, `Upstream path`, `Commit`), so the table can gain columns.
fn parse_ports(md: &str) -> Vec<Port> {
    let mut out = Vec::new();
    let mut cols: Option<(usize, usize, usize, usize)> = None;
    for line in md.lines().map(str::trim).filter(|l| l.starts_with('|')) {
        let cells: Vec<String> = line.trim_matches('|').split('|').map(|c| c.trim().trim_matches('`').to_string()).collect();
        if cells.iter().all(|c| c.chars().all(|ch| matches!(ch, '-' | ':' | ' '))) {
            continue;
        }
        let find = |pred: &dyn Fn(&str) -> bool| cells.iter().position(|c| pred(&c.to_ascii_lowercase()));
        if cols.is_none() {
            let ours = find(&|c| c.contains("our") || c.contains("local image") || c == "file");
            let project = find(&|c| c == "upstream" || c.contains("project"));
            let path = find(&|c| c.contains("path"));
            let commit = find(&|c| c.contains("commit"));
            if let (Some(a), Some(b), Some(c), Some(d)) = (ours, project, path, commit) {
                cols = Some((a, b, c, d));
            }
            continue;
        }
        let Some((a, b, c, d)) = cols else { continue };
        let get = |i: usize| cells.get(i).cloned().unwrap_or_default();
        let commit = get(d);
        if commit.len() >= 7 && commit.chars().all(|ch| ch.is_ascii_hexdigit()) {
            out.push(Port { ours: get(a), project: get(b), path: get(c), commit });
        }
    }
    out
}

fn upstream_check(offline: bool) -> Result<()> {
    let md = std::fs::read_to_string(root().join("docs").join("PORTS.md")).context("docs/PORTS.md")?;
    let ports = parse_ports(&md);
    if ports.is_empty() {
        println!("docs/PORTS.md lists no ports with an upstream commit.");
        return Ok(());
    }
    let cache = root().join("target").join("upstream");
    std::fs::create_dir_all(&cache)?;
    let mut fetched = std::collections::HashSet::new();
    let mut behind = 0;
    for p in &ports {
        let Some(url) = upstream_repo(&p.project) else {
            println!("? {} — unknown upstream project `{}`", p.ours, p.project);
            continue;
        };
        let dir = cache.join(p.project.to_ascii_lowercase());
        if fetched.insert(dir.clone()) && !offline {
            let ok = if dir.join(".git").is_dir() {
                Command::new("git").args(["-C"]).arg(&dir).args(["fetch", "--quiet", "--filter=blob:none", "origin"]).status()?.success()
            } else {
                Command::new("git").args(["clone", "--quiet", "--filter=blob:none", "--no-checkout", url]).arg(&dir).status()?.success()
            };
            if !ok {
                println!("! {}: could not fetch {url}", p.project);
                continue;
            }
        }
        let out = Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["log", "--format=%h %ad %s", "--date=short"])
            .arg(format!("{}..origin/HEAD", p.commit))
            .args(["--", &p.path])
            .output()?;
        if !out.status.success() {
            println!("! {} — {}:{} at {}: {}", p.ours, p.project, p.path, p.commit, String::from_utf8_lossy(&out.stderr).trim());
            continue;
        }
        let log = String::from_utf8_lossy(&out.stdout);
        let n = log.lines().count();
        if n == 0 {
            println!("✓ {} — {}:{} unchanged since {}", p.ours, p.project, p.path, &p.commit[..7.min(p.commit.len())]);
        } else {
            behind += 1;
            println!("• {} — {}:{} has {n} newer commit(s) since {}:", p.ours, p.project, p.path, &p.commit[..7.min(p.commit.len())]);
            for l in log.lines() {
                println!("    {l}");
            }
        }
    }
    println!("{} port(s), {behind} with upstream changes to review.", ports.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_installer_icons_pass_ice50() {
        check_icons(&root().join("packaging").join("windows").join("local-image.wxs")).unwrap();
    }

    #[test]
    fn ports_table_parses_by_header() {
        let md = "# Ports\n\n| Our file | Upstream | Upstream path | Upstream commit | Licence |\n|---|---|---|---|---|\n| `crates/lc-pipeline/src/negative.rs` | darktable | `src/iop/negadoctor.c` | 0123456789abcdef | GPL-3.0-or-later |\n| x | darktable | y | (pending) | z |\n";
        let p = parse_ports(md);
        assert_eq!(
            p,
            vec![Port {
                ours: "crates/lc-pipeline/src/negative.rs".into(),
                project: "darktable".into(),
                path: "src/iop/negadoctor.c".into(),
                commit: "0123456789abcdef".into()
            }]
        );
        assert!(upstream_repo("darktable").is_some() && upstream_repo("nope").is_none());
    }

    #[test]
    fn version_reads_the_workspace() {
        assert!(version().unwrap().starts_with("2."));
    }
}
