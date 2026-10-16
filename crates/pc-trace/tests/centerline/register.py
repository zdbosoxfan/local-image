#!/usr/bin/env python3
"""Exercise package 00's exact routing fragments in a disposable registration copy.
Never modifies shared files in the real worktree. Re-run before verification.
"""
from pathlib import Path
import shutil
import subprocess
import tarfile
import io

ROOT = Path(__file__).resolve().parents[4]
DEST = ROOT / "target/centerline25-registration/worktree"
DEST.mkdir(parents=True, exist_ok=True)
if not (DEST / "Cargo.toml").exists():
    archive = subprocess.check_output(["git", "archive", "HEAD"], cwd=ROOT)
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        tar.extractall(DEST, filter="data")
# Symlink all unchanged crates; the two changed crates need private shared roots.
for source in (ROOT / "crates").iterdir():
    target = DEST / "crates" / source.name
    if source.name not in ("pc-trace", "pc-engine", "pc-pdf"):
        if target.is_symlink():
            continue
        shutil.rmtree(target)
        target.symlink_to(source, target_is_directory=True)
for crate in ("pc-trace", "pc-engine", "pc-pdf"):
    source = ROOT / "crates" / crate
    target = DEST / "crates" / crate
    if target.is_symlink():
        target.unlink()
        target.mkdir()
    for file in source.rglob("*"):
        if file.is_file():
            out = target / file.relative_to(source)
            out.parent.mkdir(parents=True, exist_ok=True)
            if out.exists() or out.is_symlink():
                out.unlink()
            if file.relative_to(source).as_posix() in ("src/lib.rs", "src/spline.rs", "tests/it/main.rs", "tests/it/common/mod.rs") or (crate == "pc-pdf" and file.name == "Cargo.toml"):
                shutil.copyfile(file, out)
            else:
                out.symlink_to(file)
lib = DEST / "crates/pc-trace/src/lib.rs"
s = lib.read_text().replace("pub mod adapter;", "pub mod adapter;\npub mod centerline;")
s = s.replace("pub enum Fitter {", "pub enum Mode { Fill, Centerline }\n#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]\n#[serde(rename_all = \"snake_case\")]\npub enum Fitter {")
s = s.replace("pub struct Params {", "pub struct Params {\n    #[serde(skip_serializing_if = \"Option::is_none\")]\n    pub mode: Option<Mode>,")
s = s.replace("let mut p = Self {", "let mut p = Self {\n            mode: None,")
s = s.replace("p.adaptive = true;", "p.adaptive = true;\n                p.speckle = 0;\n                if preset == Preset::Sketch { p.detail = 0.8; }")
s = s.replace("    pub fn validate(&self) -> Result<(), Error> {\n        if matches!(self.preset, Preset::LineArt | Preset::Sketch) {\n            return Err(Error::InvalidParams(\"centreline tracing is not available\"));\n        }", """    pub fn resolved_mode(&self) -> Mode {
        self.mode.unwrap_or(if matches!(self.preset, Preset::LineArt | Preset::Sketch) { Mode::Centerline } else { Mode::Fill })
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.resolved_mode() == Mode::Centerline && !matches!(self.preset, Preset::LineArt | Preset::Sketch | Preset::BlackWhite) {
            return Err(Error::InvalidParams("centreline requires Line Art, Sketch or Black & White"));
        }
        if self.resolved_mode() == Mode::Fill && matches!(self.preset, Preset::LineArt | Preset::Sketch) {
            return Err(Error::InvalidParams("Line Art and Sketch require centreline mode"));
        }""")
s = s.replace("pub struct Trace {", "pub struct Trace {\n    #[serde(default, skip_serializing_if = \"Vec::is_empty\")]\n    pub strokes: Vec<centerline::StrokePath>,")
s = s.replace("    params.validate()?;\n    let check", """    params.validate()?;
    if params.resolved_mode() == Mode::Centerline {
        let out = centerline::trace_with_cancel(image, params, cancel)?;
        return Ok(Trace { width: out.width, height: out.height, layers: Vec::new(), strokes: out.strokes, nodes: out.nodes });
    }
    let check""")
s = s.replace("height: image.height, layers:", "height: image.height, strokes: Vec::new(), layers:").replace("height: image.height, layers,", "height: image.height, strokes: Vec::new(), layers,")
lib.write_text(s)
spline = DEST / "crates/pc-trace/src/spline.rs"
s = spline.read_text().replace('pub fn refine(path: &BezPath, tolerance: f64) -> BezPath {', '''pub fn refine(path: &BezPath, tolerance: f64) -> BezPath {
    refine_with_angle(path, tolerance, 1e-3)
}
pub fn refine_with_angle(path: &BezPath, tolerance: f64, angle_thresh: f64) -> BezPath {''').replace('.angle_thresh(1e-3)', '.angle_thresh(angle_thresh)')
spline.write_text(s)
common = DEST / "crates/pc-trace/tests/it/common/mod.rs"
s = common.read_text().replace("Trace { width:", "Trace { strokes: Vec::new(), width:")
common.write_text(s)
main = DEST / "crates/pc-trace/tests/it/main.rs"
main.write_text(main.read_text() + '\n#[path = "../centerline/routing.rs"]\nmod centerline_routing;\n#[path = "../centerline/benchmark.rs"]\nmod centerline_benchmark;\n')
# Baseline 4e05417d merged package 22's engine calls but not its PDF root /
# production dependencies. Apply its EXISTING integration/22.md registration,
# solely in this disposable copy, so the unchanged engine compiles.
pdf = DEST / "crates/pc-pdf/src/lib.rs"
pdf.write_text(pdf.read_text() + '\npub mod import;\npub use import::{ImportError, PageBox, PageInfo, PdfInfo, PdfOptions, import, import_with, info};\n')
manifest = DEST / "crates/pc-pdf/Cargo.toml"
manifest.write_text(manifest.read_text().replace('[dependencies]', '[dependencies]\nhayro-syntax.workspace = true\nhayro-interpret.workspace = true\nflate2.workspace = true\nimage.workspace = true'))
print(DEST)
