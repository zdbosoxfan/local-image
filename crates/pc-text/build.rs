//! Optional craft-fonts build input (https://github.com/storytold/craft-fonts, recipe from its
//! `docs/integration.md`; rules in `craftrules/standards/fonts.md`).
//!
//! With `CRAFT_FONTS_DIR=<craft-fonts checkout>`, every font in its `fonts/manifest.txt` is
//! embedded and exposed as `photocraft_text::CRAFT_FONTS`; unset, `CRAFT_FONTS` is empty (an empty
//! value counts as unset). This is the only place the fonts are embedded: every other crate
//! (`lightcraft_engine::CRAFT_FONTS`, the egui UIs) re-exports this table, so each font is in the
//! binary once. A relative path is resolved against the workspace root first, then this package.
//! A bad checkout is a warning, or an error with `CRAFT_FONTS_REQUIRED=1` (release builds).
//! craft-fonts is never a Cargo dependency, and this script only reads a local directory.
//!
//! The web build (wasm32) embeds nothing: the fonts don't fit its per-file size cap (measured
//! 2026-10-06: the UI face alone took the release wasm from 24.19 MB to 28.87 MB, over the
//! 24 MiB gate).
#![expect(clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed")]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=CRAFT_FONTS_DIR");
    println!("cargo::rerun-if-env-changed=CRAFT_FONTS_REQUIRED");
    let wasm = std::env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|a| a == "wasm32");
    let mut src = String::from("pub static CRAFT_FONTS: &[CraftFont] = &[\n");
    let mut embedded = 0;
    if let Some(dir) = std::env::var_os("CRAFT_FONTS_DIR").filter(|d| !d.is_empty() && !wasm).map(PathBuf::from).map(resolve) {
        match craft_fonts(&dir) {
            Ok(entries) => {
                embedded = entries.lines().count();
                src.push_str(&entries);
            }
            Err(e) if std::env::var_os("CRAFT_FONTS_REQUIRED").is_some() => {
                println!("cargo::error=CRAFT_FONTS_DIR={}: {e}", dir.display());
            }
            Err(e) => println!("cargo::warning=building without craft-fonts: CRAFT_FONTS_DIR={}: {e}", dir.display()),
        }
    }
    src.push_str("];\n");
    // Without them the app still runs and every language is still selectable, so Chinese and
    // Japanese just paint boxes: the one failure a user cannot diagnose from what they see.
    if embedded == 0 && !wasm {
        println!(
            "cargo::warning=no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. \
             Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with \
             CRAFT_FONTS_DIR=../craft-fonts"
        );
    }
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default()).join("craft_fonts.rs");
    if let Err(e) = std::fs::write(&out, src) {
        println!("cargo::error=writing {}: {e}", out.display());
    }
}

/// Resolve a relative `CRAFT_FONTS_DIR` against both roots a caller might mean: a build script's
/// cwd is the *package* root (`crates/pc-text`), while the `../craft-fonts` the docs use is written
/// relative to the *workspace* root and would otherwise resolve to `crates/craft-fonts`.
fn resolve(dir: PathBuf) -> PathBuf {
    if dir.is_absolute() {
        return dir;
    }
    let package = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let workspace = package.parent().and_then(Path::parent).unwrap_or(package.as_path());
    let candidates = [workspace.join(&dir), package.join(&dir)];
    candidates.iter().find(|candidate| candidate.is_dir()).cloned().unwrap_or_else(|| candidates[0].clone())
}

/// One `CraftFont { .. }` initialiser per manifest line.
fn craft_fonts(dir: &Path) -> Result<String, String> {
    let manifest = dir.join("fonts/manifest.txt");
    println!("cargo::rerun-if-changed={}", manifest.display());
    let text = std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let mut out = String::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let f: Vec<&str> = line.split(" | ").map(str::trim).collect();
        let [family, style, file, scripts, ..] = f.as_slice() else {
            return Err(format!("malformed manifest line: {line}"));
        };
        let path = dir.join(file).canonicalize().map_err(|e| format!("{file}: {e}"))?;
        println!("cargo::rerun-if-changed={}", path.display());
        let scripts: Vec<String> = scripts.split(',').map(|s| format!("{:?}", s.trim())).collect();
        let _ = writeln!(
            out,
            "    CraftFont {{ family: {family:?}, style: {style:?}, scripts: &[{}], bytes: include_bytes!({:?}) }},",
            scripts.join(", "),
            path.display().to_string(),
        );
    }
    Ok(out)
}
