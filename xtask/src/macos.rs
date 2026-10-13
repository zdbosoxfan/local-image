//! Native Apple Silicon bundle and Installer package. No runtime outside macOS is needed.
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::{build_env, copy_dir, copy_distribution_attributions, dist, root, run, sha256_hex, target_dir, version};

const ID: &str = "io.github.zdbosoxfan.LocalImage";

pub fn package(skip_build: bool) -> Result<()> {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        bail!("macOS packaging must run natively on an Apple Silicon Mac");
    }
    let version = version()?;
    let stage = target_dir().join("macos-package");
    let app = stage.join("Local Image.app");
    if app.exists() {
        std::fs::remove_dir_all(&app)?;
    }
    let contents = app.join("Contents");
    let bin = contents.join("MacOS");
    let resources = contents.join("Resources");
    std::fs::create_dir_all(&bin)?;
    std::fs::create_dir_all(&resources)?;
    if !skip_build {
        let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
        cmd.current_dir(root())
            .args([
                "build",
                "--release",
                "--locked",
                "-p",
                "local-image",
                "-p",
                "local-image-cli",
                "--features",
                "local-image/heif,local-image/sam,local-image-cli/heif",
            ])
            .env("MACOSX_DEPLOYMENT_TARGET", "13.0");
        build_env(&mut cmd);
        run("build Apple Silicon binaries", &mut cmd)?;
    }
    for name in ["local-image", "local-image-cli"] {
        let source = target_dir().join("release").join(name);
        let dest = bin.join(name);
        std::fs::copy(&source, &dest).with_context(|| format!("{}: build release binaries first", source.display()))?;
        run("verify arm64 executable", Command::new("lipo").arg("-verify_arch").arg("arm64").arg(&dest))?;
        verify_libraries(&dest)?;
        run("strip debug symbols", Command::new("strip").arg("-S").arg(&dest))?;
    }
    std::fs::write(contents.join("Info.plist"), info_plist(&version))?;
    std::fs::write(contents.join("PkgInfo"), "APPL????")?;
    for file in ["README.md", "LICENSE"] {
        std::fs::copy(root().join(file), resources.join(file))?;
    }
    copy_dir(&root().join("licenses"), &resources.join("licenses"))?;
    copy_distribution_attributions(&resources)?;
    make_icon(&stage, &resources)?;
    // Ad-hoc signing makes arm64 code valid locally. Developer ID signing is available to
    // distributors; a Developer ID installer identity can independently sign the .pkg below.
    let identity = std::env::var("MACOS_SIGN_IDENTITY").unwrap_or_else(|_| "-".into());
    for name in ["local-image-cli", "local-image"] {
        sign(&bin.join(name), &identity)?;
    }
    sign(&app, &identity)?;
    run("verify app signature", Command::new("codesign").args(["--verify", "--deep", "--strict", "--verbose=2"]).arg(&app))?;
    run("verify bundled CLI", Command::new(bin.join("local-image-cli")).arg("--version"))?;
    let name = format!("local-image-{version}-macos-arm64.pkg");
    let output = dist()?.join(&name);
    // Disable bundle relocation: upgrading must replace /Applications/Local Image.app,
    // not an arbitrary old copy on a mounted disk or in a user's Downloads folder.
    let components = stage.join("components.plist");
    // Use a separate root containing only the bundle (the stage also holds icon sources).
    let payload = stage.join("payload");
    if payload.exists() {
        std::fs::remove_dir_all(&payload)?;
    }
    std::fs::create_dir_all(&payload)?;
    run("stage app payload", Command::new("ditto").arg(&app).arg(payload.join("Local Image.app")))?;
    // pkgbuild's --analyze omits relocation keys on newer macOS. Specify the known single
    // bundle explicitly so upgrade behavior is identical across supported packaging hosts.
    std::fs::write(
        &components,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><array><dict>
<key>RootRelativeBundlePath</key><string>Local Image.app</string>
<key>BundleHasStrictIdentifier</key><true/>
<key>BundleIsRelocatable</key><false/>
<key>BundleIsVersionChecked</key><false/>
<key>BundleOverwriteAction</key><string>upgrade</string>
</dict></array></plist>
"#,
    )?;
    let mut cmd = Command::new("pkgbuild");
    cmd.arg("--root").arg(&payload).arg("--component-plist").arg(&components).args([
        "--identifier",
        ID,
        "--version",
        version.split('-').next().unwrap_or(&version),
        "--install-location",
        "/Applications",
    ]);
    let component = stage.join("LocalImage-component.pkg");
    run("build app component", cmd.arg(&component))?;
    let distribution = stage.join("Distribution.xml");
    let numeric = version.split('-').next().unwrap_or(&version);
    std::fs::write(
        &distribution,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<installer-gui-script minSpecVersion="2">
<title>Local Image</title>
<options customize="never" require-scripts="false" hostArchitectures="arm64" rootVolumeOnly="true"/>
<volume-check><allowed-os-versions><os-version min="13.0"/></allowed-os-versions></volume-check>
<choices-outline><line choice="app"/></choices-outline>
<choice id="app" visible="false"><pkg-ref id="{ID}"/></choice>
<pkg-ref id="{ID}" version="{numeric}" onConclusion="none">LocalImage-component.pkg</pkg-ref>
</installer-gui-script>
"#
        ),
    )?;
    let mut product = Command::new("productbuild");
    product.arg("--distribution").arg(&distribution).arg("--package-path").arg(&stage);
    if let Ok(identity) = std::env::var("MACOS_INSTALLER_SIGN_IDENTITY") {
        product.arg("--sign").arg(identity);
    }
    run("build macOS installer", product.arg(&output))?;
    std::fs::write(output.with_extension("pkg.sha256"), format!("{}  {name}\n", sha256_hex(&output)?))?;
    println!("{}", output.display());
    if identity == "-" {
        println!("Ad-hoc signed app; installer is for local testing. Developer ID signing and notarization require distributor credentials.");
    }
    Ok(())
}

fn sign(path: &Path, identity: &str) -> Result<()> {
    let mut cmd = Command::new("codesign");
    cmd.args(["--force", "--sign", identity]);
    if identity != "-" {
        cmd.args(["--options", "runtime", "--timestamp"]);
    }
    run("sign native code", cmd.arg(path))
}

fn verify_libraries(binary: &Path) -> Result<()> {
    let out = Command::new("otool").arg("-L").arg(binary).output()?;
    if !out.status.success() {
        bail!("otool failed for {}", binary.display());
    }
    for line in String::from_utf8_lossy(&out.stdout).lines().skip(1) {
        let library = line.split_whitespace().next().unwrap_or("");
        if !library.starts_with("/System/Library/") && !library.starts_with("/usr/lib/") {
            bail!("non-system runtime dependency {library}; bundle it before distribution");
        }
    }
    Ok(())
}

fn make_icon(stage: &Path, resources: &Path) -> Result<()> {
    let iconset = stage.join("LocalImage.iconset");
    std::fs::create_dir_all(&iconset)?;
    for size in [16, 32, 128, 256, 512] {
        for scale in [1, 2] {
            let pixels = (size * scale).to_string();
            let suffix = if scale == 2 { "@2x" } else { "" };
            run(
                "resize app icon",
                Command::new("sips")
                    .args(["-z", &pixels, &pixels])
                    .arg(root().join("assets/app-icon/local-image-1024.png"))
                    .arg("--out")
                    .arg(iconset.join(format!("icon_{size}x{size}{suffix}.png"))),
            )?;
        }
    }
    run("compile macOS icon", Command::new("iconutil").args(["-c", "icns"]).arg(iconset).arg("-o").arg(resources.join("LocalImage.icns")))
}

fn info_plist(version: &str) -> String {
    let numeric = version.split('-').next().unwrap_or(version);
    let version = version.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>{ID}</string>
<key>CFBundleName</key><string>Local Image</string>
<key>CFBundleDisplayName</key><string>Local Image</string>
<key>CFBundleExecutable</key><string>local-image</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>{numeric}</string>
<key>CFBundleShortVersionString</key><string>{numeric}</string>
<key>CFBundleGetInfoString</key><string>Local Image {version}</string>
<key>CFBundleIconFile</key><string>LocalImage</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
<key>LSArchitecturePriority</key><array><string>arm64</string></array>
<key>NSHighResolutionCapable</key><true/>
<key>NSPrincipalClass</key><string>NSApplication</string>
<key>NSSupportsAutomaticGraphicsSwitching</key><true/>
<key>CFBundleDocumentTypes</key><array><dict>
<key>CFBundleTypeName</key><string>Image document</string>
<key>CFBundleTypeRole</key><string>Editor</string>
<key>LSHandlerRank</key><string>Alternate</string>
<key>CFBundleTypeExtensions</key><array>
<string>png</string><string>jpg</string><string>jpeg</string><string>tif</string><string>tiff</string>
<string>webp</string><string>avif</string><string>heic</string><string>heif</string><string>psd</string>
<string>psb</string><string>svg</string><string>pdf</string><string>pcraft</string>
<string>dng</string><string>cr2</string><string>cr3</string><string>nef</string><string>arw</string><string>raf</string>
</array></dict></array>
</dict></plist>
"#
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn metadata_identifies_native_app_and_document_handler() {
        let plist = super::info_plist("2.0.0-alpha.1");
        for field in [super::ID, "local-image", "13.0", "arm64", "CFBundleDocumentTypes", "heic", "2.0.0-alpha.1"] {
            assert!(plist.contains(field), "missing {field}");
        }
    }
}
