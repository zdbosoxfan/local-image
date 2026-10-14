**J3 / D9: one shared metadata backend — completed, uncommitted (2026-10-14)**

Continued the shared TIFF directory reader and Exif dimension/thumbnail adapters already present
in this branch. Those implementations were retained rather than replaced. The remaining editor
metadata implementations now use `lightcraft-meta` and `lightcraft-tiff`.

**Completed slices**

1. `pc-algo::exif::{read,build}` are compatibility adapters over the shared EXIF reader/writer.
   `CameraInfo` and its exposure-factor API remain available. The editor gains the Library's
   APEX shutter/aperture fallbacks, extended ISO and camera-model fallbacks. The shared writer
   now retains the editor writer's 35 mm-equivalent focal length, and writes extended ISO
   instead of clipping values above 65535. Original EXIF payloads are never rebuilt by the
   orientation/dimension patches, protecting opaque MakerNote offsets.
2. File Info's string scanners, entity replacements and XML splicer are deleted. Its public
   `read_file_info`/`write_file_info` APIs are thin re-exports of the shared backend. The real
   `file.fileInfo` command uses the fallible writer and reports malformed-packet errors before
   modifying document/history. Partial edits use the existing namespace-aware `merge_xmp`;
   unedited metadata, creator sequences, localized alternatives and developer settings survive.
   `author_title` is a defaulted optional Metadata field, read/written as
   `photoshop:AuthorsPosition`; old serialized metadata still deserializes.
3. EXIF orientation reading/uprighting lives in `lc-tiff::exif`; the codec APIs re-export it.
   The first duplicate governs reads, while every valid duplicate is normalized on export.
   Both byte orders, classic TIFF/BigTIFF, SHORT/LONG, Exif prefixes, padding and opaque bytes
   are supported. Uprighting no longer builds an entry vector or allocates a value buffer for
   each entry; unchanged EXIF remains borrowed. Existing `pc-io` thumbnail/dimension functions
   already delegate to this same TIFF backend and remain intact.
4. XMP orientation normalization lives in `lc-meta`, using XML events and namespace bindings.
   It handles aliases, entity/CDATA values and legacy editor fragments/standalone attributes.
   Foreign namespaces, comments and ordinary text are preserved. Codec orientation policy
   (`keep_orientation`) and pixel orientation routines remain unchanged.
5. Save for Web copyright/contact filtering uses shared parsed metadata and the shared writer.
   It keeps rights and complete creator lists while omitting GPS, titles, keywords, ratings,
   camera information and developer settings. None/All choices retain their existing meaning.

Namespace review also corrected two shared-backend issues required for safe File Info merging:
foreign URIs bound to reserved prefixes must not become standard properties, and unqualified
XML attributes must not inherit an element's default namespace. The preservation tests cover
both reading and editing these packets.

**Measured quality and latency**

Small synthetic, non-client samples only; no long sweeps or benchmark suite. Before results
were captured from this branch's original implementations before replacement. Timing uses the
real `Session::execute("file.fileInfo", ...)` façade used by the app frontends, including JSON
results, document edits and history. It does not open a GUI. The same 8-field command sample
uses 100 reads and 20 edits, with the repository's optimized development/test profiles.

| Sample | Before | After |
|---|---:|---:|
| XMP fields read correctly: simple title, namespace alias, x-default ordering, CDATA, numeric entity, lowercase copyright Boolean | 1/6 | 6/6 |
| ISO/APEX/35 mm camera fields correct in synthetic standard-tag sample | 1/4 | 4/4 |
| ISO 128000 written/read | 65535 | 128000 |
| APEX shutter Tv=6 | missing | 1/64 s |
| APEX aperture Av=6 | missing | f/8 |
| 35 mm-equivalent focal length | 52 mm | 52 mm |
| File Info read | 5.19 µs/call | 17.51 µs/call |
| File Info edit | 29.69 µs/call | 179.74 µs/call |

The measured command path is slower: namespace parsing, XML validation and preservation checks
replace fragile string assumptions. Quality takes priority; edits stayed below 0.2 ms in this
small local sample. These are sample timings, not hardware-independent guarantees or a claimed
speedup. No GPU performance claim or measured heap-memory claim is made. The EXIF rewrite's
removed temporary allocations and borrowed unchanged path are implementation properties.

**Parity gates and verification**

PSD/TIFF goldens were captured before replacing the orientation backend and were unchanged
afterward (FNV-1a over every exported byte, plus exact length):

| Export | Bytes | Before and after byte hash |
|---|---:|---|
| PSD | 570 | `5573b328077b7c9d` |
| TIFF | 325 | `fbffddd714c68a3a` |

The new Library/editor round-trip test uses the Library's shared writer, real editor commands,
undo/redo, export/reimport through PSD, TIFF and JPEG, then the Library writer back into File
Info. It verifies all eight File Info fields through each format, creator punctuation/list
retention, and the retained Library develop-settings packet. The TIFF test additionally checks
128 valid orientation combinations and invalid-first duplicates/truncation without changing
opaque bytes. Existing golden fixtures were not edited or rebased.

| Changed crate | New filtered tests passed |
|---|---:|
| `lightcraft-meta` | 5 |
| `lightcraft-tiff` | 1 |
| `photocraft-algo` | 1 |
| `photocraft-codecs` | 1 |
| `photocraft-engine` | 5 |
| `photocraft-io` | 1 |
| Total | 14 |

All commands used `PATH=$HOME/.cargo/bin:$PATH`, `cargo +1.98.1`, offline mode where applicable,
the same worktree `target/`, and the launcher's unchanged `CARGO_BUILD_JOBS=4`. Only one cargo
process ran at a time.

- Formatting: `cargo +1.98.1 fmt -p <each of the six changed packages>`.
- Check: `cargo +1.98.1 check --offline` with the six changed packages selected — passed.
- Lint: `cargo +1.98.1 clippy --offline` with the same six packages,
  `--all-targets -- -D warnings` — passed.
- Tests: only new tests selected by `j03_`, plus the specific localized-preservation test
  after correcting its fixture. No full crate/workspace suites, GPU acceptance, benchmarks,
  quality sweeps or existing broad suites were run, following the overriding minimum-test rule.
- GPU tests added: 0. UI/rendering code was not changed; no GUI windows were opened.

Evidence stays under ignored `target/`: `j03-baseline.log`, `j03-engine-before.log`,
`j03-quality-before.log`, `j03-camera-before.log`, `j03-localized-test.log`,
`j03-remaining-tests.log`, `j03-clippy.log`, and `j03-check-final.log`.
Development failures are retained in `j03-tests.log`, `j03-tests-final.log` and
`j03-core-tests.log`: an exhaustive Metadata fixture needed the new optional field, the
round-trip test needed the document's edit API rather than direct Arc mutation, and a new
localized-title fixture initially failed to insert its item because of writer whitespace.
Those were corrected; assertions were preserved and the failing preservation test passed.
An initial check also required an explicit XML-element stack type; it was fixed before tests.
The unrelated existing craft-fonts build-script notice still appears in logs.

TEST-CHANGED: `lc-meta/src/xmp.rs::tests::full` adds `author_title: None` to its exhaustive
Metadata initializer. Existing assertions/expected outputs remain unchanged. No existing
export golden changed.

**User-visible changes and compatibility**

File Info now reads aliases, CDATA, numeric entities, lowercase copyright values and the
x-default language correctly. Unchanged author display strings retain imported creator arrays;
a changed scalar author is one literal creator (commas/semicolons are valid name text), and an
explicit array replaces the sequence. Keyword string delimiters retain the old behavior;
keyword arrays retain literal punctuation. Unedited localized alternatives survive. Editing
a localized title/description still replaces that property's alternatives, consistent with
the old scalar File Info interface. Malformed XML is preserved and edits report an error
instead of silently replacing the packet. Copyright-only exports of otherwise empty metadata
now omit an empty/unknown-only packet. Namespace-collision properties are kept as foreign
properties instead of being mistaken for standard fields.

Old documents/catalog metadata still load, and legacy orientation fragments remain supported.
Public old-crate entry points and command field names remain available. No new UI strings or
controls were added, so no design-system or i18n pending-list edits were needed.

**Required edits outside the listed source-function ownership**

- `pc-algo/Cargo.toml`, `pc-codecs/Cargo.toml`, `pc-engine/Cargo.toml` and `Cargo.lock`: three
  dependency edges to the existing workspace `lightcraft-meta` crate.
- `lc-meta/Cargo.toml`: move the already-present/locked `serde_json` dependency from dev to
  regular dependencies so the existing File Info JSON contract can be re-exported directly.
- `pc-engine/src/file_cmds.rs::file_info`: one fallible-backend call/error mapping, required
  to prevent an invalid edit being silently accepted; new command tests are in this owned file.
- Additive export-golden test in `pc-io/src/flat.rs`; production functions outside the owned
  metadata adapters were not changed.

Cargo.lock package-name/version/source comparison shows **0 added packages and 0 removed
packages**. No new external crate, C/C++, `-sys` dependency, unsafe code or commit was introduced.
No owner photographs, Desktop contents, app windows or ComfyUI jobs were used. The pre-existing edit to
`docs/wip/CODEX-TASK.md` was left untouched.

**Established references followed**

- Existing shared TIFF reader/writer/directory code, `lc-meta` EXIF/XMP parser/writer,
  `xmp_merge` preservation rules, and this branch's safe thumbnail/dimension patch adapters.
- [Adobe Photoshop metadata and File Info help](https://helpx.adobe.com/photoshop/using/metadata-notes.html):
  XMP as shared file metadata across application surfaces.
- [Adobe Dublin Core schema](https://developer.adobe.com/xmp/docs/xmp-namespaces/dc/) and
  [XMP data types](https://developer.adobe.com/xmp/docs/xmp-namespaces/xmp-data-types/): ordered
  creators and language alternatives/x-default.
- [Adobe TIFF namespace](https://developer.adobe.com/xmp/docs/xmp-namespaces/tiff/): orientation
  semantics and namespace URI.
- [CIPA Exif 2.32 translation](https://www.cipa.jp/std/documents/e/DC-X008-Translation-2019-E.pdf):
  SHORT PhotographicSensitivity, LONG ISO Speed and SensitivityType tags.

No J3 implementation work remains. Full integrated and GPU suites remain with the coordinator
under the owner's minimum-testing rule. All task changes are left uncommitted.
