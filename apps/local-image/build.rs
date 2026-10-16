//! Windows only: embed the app icon and version info (VERSIONINFO) into `local-image.exe`.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `LOCAL_IMAGE_REQUIRE_WINRES=1` (set by the
//! release workflow) turns it into an error.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/local-image.ico");
    println!("cargo:rerun-if-env-changed=LOCAL_IMAGE_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/local-image.ico")
        .set("ProductName", "Local Image")
        .set("FileDescription", "Local Image: photo editor with local AI")
        .set("CompanyName", "Local Image contributors")
        .set("LegalCopyright", "Copyright (c) the Local Image contributors (and the PhotoCraft and LightCraft authors). GPL-3.0-or-later.")
        .set("OriginalFilename", "local-image.exe")
        .set("InternalName", "local-image");
    if let Err(e) = res.compile() {
        if std::env::var_os("LOCAL_IMAGE_REQUIRE_WINRES").is_some() {
            panic!("embedding Windows resources failed: {e}");
        }
        println!("cargo:warning=local-image.exe built without icon/version resources: {e}");
    }
}
