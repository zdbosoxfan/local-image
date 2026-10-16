# Package 00 source and fixture records

All reference inputs are read-only. Generated logs/metadata stay under
`target/refvec/vector-00/`; the test inputs are synthetic, generated in Rust, and
GPL-3.0-or-later as part of Local Image. No brand assets were copied.

## Retained V0

The V0 implementation is already merged in baseline `2e8c671f`. Its original
report is `docs/wip/CODEX-REPORT-vector-v0.md`. Pinned PhotoCraft Git objects
are available in this repository; no source was fetched from an unpinned branch.

- [#865](https://github.com/storytold/photocraft/commit/75ff41a26beb445413f6ee9248eb40b42152bd77):
  `crates/vector/src/edit.rs`, `crates/engine/src/path_edit_cmds.rs`,
  `crates/ui-egui/src/direct_select.rs` and `direct_select_tests.rs`.
- [#747](https://github.com/storytold/photocraft/commit/8302f57190c83439adb961ae23b9b110e819dea3):
  `crates/vector/src/raster.rs`.
- [#1321](https://github.com/storytold/photocraft/commit/d438f842256435dba7aa41ca306656a216b98fbf):
  `crates/vector/src/{lib,tests}.rs`, `crates/compose/src/tests.rs`.

PhotoCraft is MIT OR Apache-2.0. The existing notices and per-file PORTS rows
are retained. `00-provenance.tsv` records verified Git blob hashes for #865/#747.
The #1321 Paint/gradient block before the CompiledShape declaration has the
same Git blob hash in both source extracts:
`55ccce55128fa00873bd5fdbcbdf5c7c9658ef93`. Other renderer adaptations retain
Local Image's sparse caches and existing Move/depth behavior.
`00-gradient-tests.tsv` records identical SHA-256 hashes of the four retained
upstream analytic/fill-layer parity test functions.

## Dependency audit

- VectorCraft at `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, read-only local
  `~/.local/share/local-image-dev/engine-sources/vectorcraft` Cargo.toml/Cargo.lock.
  This supplies the exact linesweeper/kurbo/polycool pins; the linesweeper archive
  checksum is copied from that lock and independently matched to the local archive.
- `docs/wip/vector-tasks/pdf-dependency-audit.md` and `pdf-dependencies.tsv`,
  including the selected-feature manifests for krilla 0.8.2, hayro-interpret 0.7.0
  and hayro-syntax 0.7.2. Cached exact-version normalized manifests, sources and
  crate archives were inspected and SHA-256 checked.
- [krilla 0.8.2 manifest](https://docs.rs/crate/krilla/0.8.2/source/Cargo.toml.orig),
  [hayro-interpret 0.7.0 manifest](https://docs.rs/crate/hayro-interpret/0.7.0/source/Cargo.toml.orig),
  [hayro-syntax 0.7.2 manifest](https://docs.rs/crate/hayro-syntax/0.7.2/source/Cargo.toml.orig).

`00-audit.py` reproduces the comparison with the baseline lock and selected PDF
allowlist. `00-dependencies.tsv` publishes the unified V1/PDF closure with
versions, licenses, features, archive checksums and build-script status. Existing
libm/serde build scripts contain Rust configuration/version detection, alongside
the four already identified in the original audit. No new package has a build
script, native links declaration or C/-sys dependency. Dependency-internal unsafe
in hayro-syntax is the already documented architecture exception; this is not a
claim that the dependency graph forbids unsafe internally.
An isolated metadata-only prefetch manifest under `target/refvec/vector-00/prefetch`
is seeded from the current lock and fetched with `--offline`. Its 86 registry
packages are all in the unified 88-package allowlist. All archives were already
cached; no download, whole-workspace fetch or out-of-worktree reference edit was
needed.

## Offscreen app fixture

`pc-ui-egui/tests/gpu/vector_integration.rs` follows the existing `layout_perf.rs`
app GPU observer/readback pattern and Move pointer input path. The existing
egui_kittest 0.36.2 and wgpu 30.0.1 APIs are read from the cached Rust sources.

Parameters: 320×240 transparent RGB document; rectangle at
(60.25, 50.5), size 115.5×120; 4.5 px gradient stroke; 2 px outside stroke effect
(normal, opacity 0.7, RGB 0.2/0.5/0.9); all five gradient styles,
both alignments, U8/U16/F32. Unsorted black/pink/white stops at 0/0.5/1, angle 31°,
scale 1.3, reverse, opacity 0.9→0.4, midpoints 0.25/0.75, offset (0.1,-0.15),
world-coordinate dither. Actual pointer moves are (7,4) and (21,12) document
pixels; release commits once, undo restores the path, and window focus loss
cancels another live preview without history. Every phase is read back from the
app's GPU canvas and compared against a CPU composite with freshly minted split
cache input identities, premultiplied
max diff ≤1/255. Hardware identity, Vulkan, DiscreteGpu and Rgba32Float support
are asserted; failures cannot become adapter skips. No performance claim is made.
The visible effect selects the app compositor's live Move preview; independent
Move planes use a screen-space shader and leave the document's base texture
unchanged. The existing Move-plane tests separately cover that retained path.
The established Move preview translates cached pixels until release; the oracle
preserves those pixels while preventing reuse of the app's split entry. Release
is additionally compared with freshly rasterized geometry/paint. Fit-to-window
and contextual overlays are disabled to keep the pointer input at 1 document
pixel per screen point.

## Native/settings/PSD references

The frozen native fixture is `pc-format/tests/fixtures/vector-v1.pcraft`, with its
source JSON and existing fixture-generation example; it is Local Image's own
GPL-3.0-or-later fixture. Legacy preferences are the existing engine toolset
migration tests' synthetic JSON.

The prior builder's corpus is at the sibling worktree
`agent-a402385380348640e/target/refvec/v0/corpus/psd`, read-only. Its 170 inputs
are checked against the saved SHA-256 manifest pinned to PhotoCraft
`25e349b048f37615d7986774ff7714ba0822646e:xtask/psd-corpus.sha256`.
Sources: psd-tools at `96eb134c17b2c65edf4c4151c0f00b802ada86c2` (`tests/psd_files`)
and ag-psd at `387049670cb89b88fb8fe1b7c01aeacf98dd2e3b` (`test`), both MIT;
their license files accompany the corpus. No proprietary owner fixtures are
supplied or counted. A temporary worktree-local `corpus/psd` symlink is used
only while running the unchanged corpus tests and removed afterward.
