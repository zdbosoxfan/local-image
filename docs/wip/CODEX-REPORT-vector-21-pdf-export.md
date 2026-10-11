# Package 21 PDF export report

Date: 2026-10-10. Branch: `wip/vec-21-pdf-export`, starting HEAD `804373fb`.
Continued the merged foundation and previous SVG/export/projection work. The
PDF command was an empty slot; no separate PDF writer existed to discard.
All changes remain uncommitted and within package-owned paths.

**Status: package-owned implementation is present; complete integrated
acceptance is pending named shared dependencies.** The absence of package 22,
shared registrations and arbitrary shaped-glyph source mapping is not counted
as a pass. Exact activation fragments are in
[`vector-tasks/integration/21.md`](vector-tasks/integration/21.md).

## Implemented

- Safe Rust lower adapter using the approved krilla =0.8.2 graph. krilla owns
  PDF objects/resources/fonts/streams; no handwritten PDF engine or shaper.
  PDF 1.7 metadata, page points from DPI and one pixel/page transform with
  krilla's own top-left surface convention.
- Document, captured selection, single artboard and ordered artboard page list.
  Missing/duplicate target IDs fail before output; source remains detached.
  Finite bounded numeric input, bounded objects/depth/segments/pages/pixels/
  output bytes and cancellation checks.
- Ordinary native paths, curves, compound operations/inversion through the
  public pathops boundary, fill rules, stacked fills/strokes, width-relative
  dashes, caps/joins and public outlines for inside/outside closed strokes.
- Native linear/reflected/radial krilla shadings with native gradient frame,
  reverse/midpoint and independent alpha stops. Transparency resources,
  isolated group opacity, artboard/ordinary clips, vector mask density and
  empty reveal/inverted mask conventions. Pixel/feather masks become soft mask
  images while the artwork stays vector, with an aggregated warning.
- Unsupported paint, raster layers and effects use bounded public native
  rendering/compositing. Package 08 handles appearance and pattern resources;
  missing resources fail rather than using stale caches. Backdrop-dependent
  compositing flattens the scope with an explicit warning. Nonfinite raster/
  mask samples fail. Per-item unsupported features are not given fake vector
  editability.
- pc-text supplies layout, positioned glyphs, font bytes, outlines and
  decorations. Exact one-glyph/one-public-cluster horizontal default-instance
  text embeds/subsets with ToUnicode and independent text extraction. Text as
  curves uses pc-text outlines. Ambiguous mappings, variable/vertical/warped
  type or embedding restrictions outline with a warning. Faux-bold and other
  styling without an exact public outline use native raster output/warnings.
- Guarded `file.pdf.export`: document ID/revision/all targets, existing shared
  text engine, cancellation API, `journal=false`, no history/revision change.
- Real UI modal implementation, preferences defaults, captured scopes, page
  checkboxes, Text as curves/Compress options, Cancel/Escape, prepare-before-
  picker and freshness-before-write, existing file services and warning notice.
  Menu/router activation is exact and tested on disposable copies; real shared
  activation remains owned by 00. Existing raster export/print is untouched.
- Independent structural/Poppler oracle tests; four engine guard tests; four
  actual-click dialog tests; strict full-app RTX pointer preview/release and
  actual menu/modal/warning test. Public-22 roundtrip assertions are supplied
  behind the named integration feature and have not executed without 22.

## Verification

Commands use `PATH="$HOME/.cargo/bin:$PATH"`, cargo +1.98.1, `--offline`,
`CARGO_BUILD_JOBS=3`, one cargo build at a time. Real shared manifests and
registries are not owned here. `crates/pc-pdf/fixtures/export/bootstrap.py`
creates a disposable registered workspace under `target/pdf-export-workspace`,
using the real package-owned sources. It applies 15's exact shared activation
fragment on copies and explicitly refuses the missing 09 symbol provider.
That temporary refusal is not symbol rendering acceptance.

Focused checks of PDF, engine and UI compilation passed during development.

Formatting uses cargo fmt on a disposable three-crate formatting manifest whose
lib paths are only the owned modules, plus rustfmt on owned standalone test
roots. This avoids formatting unowned source through module/path symlinks.

Final commands (run from this worktree):

```bash
python3 crates/pc-pdf/fixtures/export/bootstrap.py
PATH="$HOME/.cargo/bin:$PATH" cargo +1.98.1 fmt --manifest-path target/pdf-export-format/Cargo.toml -p photocraft-pdf -p photocraft-engine -p photocraft-ui-egui
PATH="$HOME/.cargo/bin:$PATH" rustfmt +1.98.1 --edition 2024 crates/pc-pdf/tests/export/main.rs crates/pc-pdf/tests/export/app_gpu.rs
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline --manifest-path target/pdf-export-workspace/Cargo.toml -p photocraft-pdf -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/target" cargo +1.98.1 test --offline --no-fail-fast --manifest-path target/pdf-export-workspace/Cargo.toml -p photocraft-pdf -p photocraft-engine -p photocraft-ui-egui -- --nocapture --test-threads=3
```

Full changed-crate suites ran **once**, with `--no-fail-fast` so every target ran.
Results before the final hard-stop regression/fix:

| Crate / suite | Passed | Failed | Ignored |
|---|---:|---:|---:|
| pc-pdf full suite | 13 | 0 | 0 |
| pc-engine full suite | 1623 | 0 | 19 |
| pc-ui-egui full suite, including actual PDF app GPU | 1139 | 3 | 18 |
| Total full run | 2775 | 3 | 37 |

Doc tests: 0 in each changed crate. Existing ignored cases remain ignored;
they include benchmarks, explicit GPU gates, pending node/persona integration,
one owner-supplied Camera Raw PSD and one coordinator path-text golden. They do
not count as acceptance. All package-21 tests have **zero ignored cases**.

The final review added one meaningful independent regression: PDF 1.7 §3.9.3
requires strictly increasing Type 3 stitching-function bounds. The test first
failed on `[0.5,0.5,0.75,0.75]`. The adapter now supplies one-ULP separated
krilla offsets and merges coincident representable offsets, preserving native
left/right limiting colors. Re-ran only the changed PDF tests by `pdf_export`
filter: **14 passed, 0 failed, 0 ignored**. This includes every PDF test, all
three hard color/alpha gradient styles, decoded RGB/alpha fallback equality,
font extraction and all existing rendering assertions. No second full engine
or UI suite was run. The four engine guard tests, four dialog click tests and
one full-app GPU test also passed, for **23 package-21 tests** across the final
focused/full evidence. Final all-target clippy and formatting/checks pass.

Full UI failures are explicit, not ratcheted away:

- `i18n::tests::complete_languages_translate_every_menu_string`: Japanese
  `PDF…` is missing from the shared catalog. The exact source is in owned
  `i18n/21.tsv`; 00 owns the catalog merge. The disposable test activates the
  real menu without silently inserting a fake translation.
- `i18n::tests::every_tl_literal_is_translated`: **65 existing** Japanese source
  omissions across merged vector packages, including `Filter for symbols`,
  appearance, node, persona and SVG/path-text controls. Package 21 adds no tl!
  macro callers and does not edit those sources/catalogs.
- `gpu::snapping_full_app_gpu_guide_preview_release_no_jump_and_override`:
  unowned unchanged `vector_packages/snapping/tests_gpu.rs:119` fails at the
  preview/release gate with **255/255** max difference. Source was verified
  against HEAD and left untouched. This failure was not independently diagnosed
  or fixed; do not claim the full UI GPU suite is clean. Its other nine tests
  passed. The main GPU binary separately passed 61 tests with 5 ignored.

Final logs are scratch files in `target/pdf-export-checks/{full-tests,
focused-pdf-final,focused-gpu,hard-stops-before,clippy-final}.log`.
The external `li-test-runner` wrapper printed arithmetic/integer diagnostics in
the focused GPU run after the Rust test passed and exited 0; that wrapper is
outside this worktree and was not modified. No adapter failure or successful
skip was accepted. The optional craft-fonts build warning also remains; CJK
embedding/rendering is not claimed by the Inter fixture.

## GPU and visual evidence

The PDF real-app test passed in the full suite, including a queried visible
warning label. Actual adapter: **NVIDIA GeForce RTX 5090**, discrete GPU,
**Vulkan**, driver **615.71.09**, PCI `0000:01:00.0`; preferred compositor
accumulator asserted **Rgba32Float**. Original texture, held-pointer frame and
released texture each measured premultiplied max difference **0.000000000**
against the native reference (required <=1/255). Gesture preview did not alter
revision/history; release published one edit. Menu/modal export created one
recorded PDF write with no document edit; a second raster-layer export displayed
the normal warning notice. See SOURCES.md for the exact 256×256 fixture.
Public-22 roundtrip is **not executed** and is **not included** in this pass.

Poppler 26.01.0 rendered all export fixtures at 96 DPI. Mean normalized RGB
errors: rectangle 0, ellipse 0.0004232, compound subtract 0.0020125; ordinary
linear/reflected/radial gradients 0.0004642/0.0006235/0.0011833; hard-stop
variants 0.0021336/0.0049257/0.0058381; group opacity/clip 0.0004136; vector
mask density 0.0006068; feather/pixel masks 0.0034335/0.0022717; stacked
appearance 0.0005719; text 0.0034446. Largest measured error is raster diamond
0.0066891, below the documented **0.008** gate; its embedded RGB and alpha
payloads equal native bytes exactly. The image-render difference includes a
faint Poppler soft-mask edge; it is not an all-pixel roundtrip parity claim.
All path-fixture silhouette disagreements satisfy the documented one-pixel
boundary condition. Raw visible IoU is diagnostic (inside dashed stroke as low
as 0.8289054), not silently presented as exact coverage equivalence.

Visual inspection of actual Poppler PNGs and CPU PNGs covered compound holes,
positioned text, appearance stacks and the raster diamond. FontFile2 subset
is under 100 KB, ToUnicode present, and independent `pdftotext` output contains
`ABC Café`. Pure path/font/shading cases contain no Image XObjects; feathered/
pixel masks explicitly retain vector path artwork beside their soft-mask image.

## Remaining integration gates

1. **00 registrations/catalogs/attribution:** real workspace crate/dependencies,
   merged PDF root with 22, UI aggregate/state/frame/menu entries, translated
   labels/warnings and central GPU test registration. None of those central
   files was edited here. The existing incomplete translation catalog is not
   treated as a passing full UI suite.
2. **22 public importer:** no `pc-pdf/src/import.rs` or 22 integration fragment
   exists in this branch. Activate `pdf-import-integrated` on the real public
   importer and execute original/roundtrip CPU and real canvas premultiplied
   max difference <=1/255. Poppler rendering is independent export evidence,
   not a substitute for that missing integrated roundtrip gate.
3. **pc-text public source mapping/variable coordinates:** PlacedGlyph has no
   source byte range. Full arbitrary shaped-glyph embedding needs pc-text to
   expose ranges captured from the existing Parley layout and resolved font
   instance coordinates. Conservative outlines preserve appearance today;
   this does not fulfill all embedded Unicode semantics. Exact requirements
   and boundaries are in 21's integration note; no independent shaping or
   guessed ligature/mark Unicode was added.
4. **Common feature projection:** corners, path text, live geometry, symbols
   and advanced stroke projection must be the same ordinary detached result
   used by display. Missing providers return explicit errors. Package 15's
   model activation and 09's Symbol handling are still central dependencies.

No proprietary/external artwork corpus was supplied. All export fixture
geometry/strings/checker patterns are original synthetic GPL-3.0-or-later
fixtures; existing Inter font is OFL-1.1. Missing corpus cases are missing,
not passed or silently skipped. No performance claim is made.

## Sources and decisions

Followed architecture owner decision 3 and the approved pinned PDF graph;
VectorCraft `crates/pdf/src/export.rs` at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092` (local git pin verified),
[krilla 0.8.2 Surface API](https://docs.rs/krilla/0.8.2/krilla/surface/struct.Surface.html)
and cached examples/source, [PDF 1.7 reference](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/pdfreference1.7old.pdf),
[OpenType OS/2 embedding flags](https://learn.microsoft.com/en-us/typography/opentype/spec/os2#fstype),
and the existing public pc-svg/pathops/vector/appearance/text/compositor APIs.
Authoritative rendering and shaping stayed in their existing crates. Conservative
source mapping and detached format/UI guards are Local Image adaptations.
Retained VectorCraft notices and supplied one PORTS row per adapted file in 21's
integration lane for 00's attribution regeneration. Detailed versions,
parameters, tool commands and fidelity thresholds are in
[`../../crates/pc-pdf/fixtures/export/SOURCES.md`](../../crates/pc-pdf/fixtures/export/SOURCES.md).
