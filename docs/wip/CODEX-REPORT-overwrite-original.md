# Overwrite Original in the Compositing → Library round trip

Continued the existing Develop-layer bridge, worker-rendered composite previews, round-trip notice,
Library save notifications, atomic writer and host headless harness. Changes are left uncommitted.

## Behavior

- Added **Overwrite Original** beside Save, Keep Editing and Dismiss in the Library notice card.
  File now has **Save Over Original…** and **Save Over Original and Return to Library…**, alongside
  the existing PSD save/return action. Ordinary Save/⌘S keeps its PSD copy behavior.
- A modal names the source and says it replaces the original, with the previous version recoverable
  from Trash. Cancel (including Escape/backdrop dismissal) performs no write or catalog operation.
  **Don't ask again** uses the existing serde-defaulted dialog preferences map; it is only changed
  after a successful overwrite. Preference persistence now runs while Library/Develop is visible.
- The encoder flattens the existing document into the original JPEG/PNG/TIFF/WebP/PSD/PSB format, retains
  its channel model and supported sample depth, and converts from the composite profile into the
  original profile with the existing CMS. The original ICC bytes are embedded. JPEG quality comes
  from the existing export preference. Metadata and upright orientation are retained; TIFF EXIF/GPS/
  Interop IFDs and private tags are carried through the existing TIFF reader/writer, while JPEG
  IPTC, extended XMP and opaque APP metadata are preserved in addition to the codec's metadata.
  Obsolete TIFF raster storage/layer data is removed. BigTIFF sources remain BigTIFF. Stale EXIF
  thumbnail directories and MPF preview references are removed so fast thumbnail decoding reads
  the new pixels rather than an embedded copy of the previous image.
- Encoding finishes before filesystem changes. The existing `li-ai::trash` record/name-reservation
  helper now also supports copying a previous version into Trash, retaining its original restore
  path and permissions. The source remains in place until the existing `photocraft-format` atomic
  writer writes/syncs a sibling temp file and renames it over the original. Trash failures abort
  replacement. The previous bytes remain recoverable if replacement subsequently fails.
- The existing Library `photo.reload` updates thumbnail/preview/loupe content and records the catalog
  change as one undo step, with Develop settings retained. The saved revision clears the unsaved
  composite and its pending preview; notices identify the overwritten path and recovery in Trash.
  The layered document, its path, and Compositing history remain available; a later normal Save can
  still save a PSD. Compositing undo changes the open document, without writing to the source file.
- Camera raw extensions disable both card and menu actions with the requested tooltip. The writer
  also rejects raw bytes, including DNG disguised as TIFF. HEIF has no writer and AVIF has no source
  decoder/profile-preserving round trip in the existing codecs, so these options are disabled with
  a format explanation. Animated, multi-page and damaged sources are rejected before replacement.
  On platforms where the existing system-Trash helper is unavailable, overwrite is disabled.
- New English strings are explicitly listed in `PENDING_OVERWRITE_TRANSLATIONS`, used by both menu
  and literal coverage gates. Existing preferences and documents retain their loading behavior.
  No dependencies, unsafe code, C/C++ code, `-sys` crates, or persisted schema changes were added.

## Sources followed

- [Adobe: Open Lightroom Classic photos in external applications](https://helpx.adobe.com/lightroom-classic/desktop/work-with-external-editors/open-lightroom-photos-external-applications.html)
  documents Edit Original for JPEG/TIFF/PSD, excludes raw/DNG, and requires the same filename and
  format to update the catalog. This task's explicit flattened-composite save extends the existing
  Local Image Develop-layer workflow; it does not change how that layer is initially developed.
- [freedesktop.org Trash Specification 1.0](https://specifications.freedesktop.org/trash/1.0/)
  supplies the original-path record and unique-version semantics already implemented by `li-ai`.
- The established repository paths were reused: `apps/local-image/src/library_host.rs`,
  `crates/pc-ui-egui/src/tiff_options_ui.rs`, `crates/pc-ui-egui/src/prefs_ui.rs`,
  `crates/pc-io/src/flat.rs`, `crates/lc-tiff/src/writer.rs`, `crates/li-ai/src/trash.rs`, and
  `crates/pc-format/src/atomic.rs`. The overwrite-specific JPEG segment carry-through and TIFF
  metadata/raster merge are small new glue around these established readers, CMS and writers.

## Validation

All Cargo commands use `PATH=~/.cargo/bin:$PATH`, Rust 1.98.1, offline builds, and
`CARGO_BUILD_JOBS=3`; one Cargo build runs at a time. No GUI windows were opened and the owner's
running app/files were untouched. Tests use their own temporary photos, catalog and Trash directory.

Final results (2026-10-10):

- `cargo +1.98.1 check --offline -p local-image`: passed.
- `cargo +1.98.1 fmt` for the five changed crates: passed; `git diff --check`: passed.
- `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p local-image
  -p photocraft-ui-egui -p photocraft-io -p li-ai -p lightcraft-meta --all-targets -- -D warnings`:
  passed. Rust 1.98's fixed-size `as_chunks` lint was corrected in the new assertions.
- The final full test command below ran once, for the changed crates only, and exited 0:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 cargo +1.98.1 test --offline \
  -p local-image -p photocraft-ui-egui -p photocraft-io -p li-ai -p lightcraft-meta \
  -- --nocapture
```

| Changed crate | Passed | Failed | Existing ignored |
| --- | ---: | ---: | ---: |
| `local-image` | 86 | 0 | 2 |
| `photocraft-ui-egui` | 1,065 | 0 | 9 |
| `photocraft-io` | 427 | 0 | 2 |
| `li-ai` | 427 | 0 | 1 |
| `lightcraft-meta` | 84 | 0 | 0 |
| **Total** | **2,089** | **0** | **14** |

Counts include unit and integration tests; doc-test targets also passed (zero cases). The existing
ignored cases are benchmarks, manual screenshot/icon exports, user-supplied fixtures, a >2 GiB PSB
test needing about 12 GB RAM, an explicitly enabled hardware icon test, and a known unrelated
inventory-metadata coverage issue. No new test is ignored. Existing missing-`craft-fonts` build-script
warnings and the external runner's occasional empty-RSS diagnostic were left unchanged; neither
caused a failure.

Added 13 tests: five host UI cases, one Compositing File-menu click case, five native-format I/O
cases, one TIFF metadata-tree case and one Trash-copy/version-reservation case. They all passed.
The host cases verify JPEG/PNG replacement through actual notice/modal clicks; old open-file
handles still reading the previous bytes after rename; recoverable Trash bytes/path records;
refreshed painted grid/loupe pixels; success notices; cleared previews; retained Develop settings
and Compositing undo history; one catalog step; raw disabling; cancellation; failed Trash backup;
persistent confirmation preference; and saving the notice's document while another tab is active.
The File-menu test clicks both new commands and Cancel/Overwrite. All 22 existing i18n tests also
passed in a focused run, including menu/literal translation coverage.

### Real GPU and interactive measurements

Added `overwrite_original_gpu_real_canvas_and_library_reload`. It requires a Vulkan discrete
adapter, reads the actual Compositing GPU canvas texels, and checks the subsequent replacement
and painted Library grid/loupe. It passed on **NVIDIA GeForce RTX 5090, Vulkan, driver 615.71.09**,
both in the focused rerun and final full run. The Compositing GPU integration target also passed
**59 tests**, with its four existing performance benchmarks ignored. The logs contain no
`skipped: no GPU adapter` or llvmpipe result.

The final run measured actual host pointer clicks, synchronous encoding/Trash/atomic replacement,
catalog reload, worker thumbnail completion and the refreshed UI frame. The pixel assertions then
inspect the painted grid/loupe. Each source was the harness's 40×30 photo:

| Host path | Confirm → write/catalog reload | Confirm → refreshed grid frame |
| --- | ---: | ---: |
| CPU Compositing, PNG | 17.03 ms | 60.36 ms |
| CPU Compositing, JPEG | 17.02 ms | 50.35 ms |
| RTX 5090 Compositing canvas, PNG | 27.03 ms | 59.13 ms |

These are small-fixture interaction checks on the real host path, not a full-resolution performance
benchmark. Encoding uses the existing CPU format writers. Detailed logs are in
`target/overwrite-full-tests.log`, `target/overwrite-ui-host.log`, `target/overwrite-codecs.log`,
`target/overwrite-i18n.log`, and `target/overwrite-clippy-final.log`.

## Limits and remaining work

The requested supported-format round trip is complete. AVIF/HEIF remain disabled for the codec
limitations described above; enabling them needs a metadata/profile-preserving read/write pair.
PSD/PSB overwrite supports RGB, grayscale and CMYK; other PSD modes fail before Trash or replacement.
The existing Trash helper limits this feature to its supported desktop platforms. Vendor-specific
payloads such as TIFF MakerNotes are retained as opaque data; vendor-specific offsets inside them
are not interpreted or rebased by the existing TIFF writer. Translations remain explicitly pending
at the owner's request. No commit was made.
