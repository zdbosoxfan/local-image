# J23 / D11 — filmstrip on the shared preview backend

Completed on the existing branch; changes are uncommitted. The pre-existing modification to
`CODEX-TASK.md` was preserved. Code edits are confined to `filmstrip_ui.rs` and `library_host.rs`.
No outside code edits, dependencies, Cargo.lock changes, unsafe code, settings/schema changes,
GUI windows, Desktop access, or owner-photo use.

## Merge and behaviour

- Deleted the filmstrip's permanent worker threads, FIFO queue, result queue and independent
  EXIF pixel walker. Its public folder/navigation/save APIs remain; `thumbnail` remains a thin
  codec adapter, with orientation delegated to the shared codec.
- The filmstrip submits only visible cells plus four neighbours on each side. The host requests
  catalog photos through `grid::request_thumb` and shares the actual Library texture: develop
  settings, crop, camera profiles, embedded RAW stand-ins, cache keys, priorities and monitor CMS
  follow the existing Library implementation. The active virtual copy takes precedence over its
  master; otherwise the master is used. Showing the strip does not browse/import into the user's
  catalog or change its selection.
- Folder-only files run on the existing `Renderer` / `lc-preview::JobPool`. An isolated in-memory
  session uses Library import defaults, presets and XMP naming, reads sidecars without writing
  them, and produces a normal `MediaCache` thumbnail keyed by true source content, settings,
  size and renderer version. It shares the open Library's cache; without a disk-backed Library,
  the same backend uses the application cache's `filmstrip` directory. No second decoder/cache
  implementation was introduced.
- Folder RAWs retain a higher-priority embedded-preview stage before the developed render.
  Full development uses `memory::in_background`, preserving the hardware-adaptive memory gate
  and existing profile-sized worker count. Source/sidecar timestamps are checked once per second;
  a Save revision refreshes immediately. Hashing/probing/decoding run on workers.
- Folder epoch, save revision, selected source, and expected render keys reject obsolete pixels.
  Changed textures and worker completions request repaint. Texture requests remain bounded by
  the viewport; private worker bookkeeping resets after 128 new slots. Shared Library thumbnails
  retain its established 600-texture limit rather than flushing on module changes.
- Kept navigation, edited-copy precedence, Save & Next and hide/show. Fixed following a distant
  item outside the current viewport. Updated affected chrome to semantic hover/border/radius/
  badge tokens, standard type-scale helpers, shared checkbox/menu widgets and accessible cell
  labels. Format text now uses the documented 10.5 pt minimum instead of 8.5 pt.

## Small-sample evidence

Final run, synthetic test data only; these are CPU measurements, not GPU or broad performance
claims. Interactive timing invokes the real `Host::logic` / `Host::ui` path through the existing
headless harness, without opening a window.

| Check | Before / control | Shared result |
| --- | --- | --- |
| Developed/cropped catalog golden | Legacy thumbnail 40×30; normalized-crop RGB MAE 59.003 against the developed target | 85×128, MAE 0; exact Library pixels, one shared texture, zero extra folder workers |
| RAW orientation with JPEG lacking EXIF | Legacy 48×32 landscape | Shared 32×48 portrait; oriented first pixel exact |
| Eight EXIF layouts with partial alpha | Explicit legacy pixel-order goldens | All eight exact, including swapped dimensions |
| Folder disk reuse, 192×128 fixture | Cold render 2.351 ms, one source decode | Warm reopen including probe/hash 0.665 ms, zero source decodes |
| Interactive settled host | Five actual host frames, one visible catalog thumbnail | Mean 5.024 ms/frame; no render pending after settling |
| Large folder UI | 10,000 file records | At most 22 requested cells; retained filmstrip textures no greater than requests |

The existing shared disk cache is JPEG, not lossless: cold-to-warm RGBA MAE was 0.6404/255,
maximum channel difference 5/255 on the gradient sample. This inherited cache tradeoff is
reported explicitly; catalog filmstrip/Library parity shares the same texture and has zero
additional conversion error. Warm reopen still reads/probes/hashes the source; “zero decode”
does not mean zero file reads. Equal-length BMP overwrites produced different content cache keys
and different pixels. Small timings are observational, with no machine-specific tuning.

## Verification

Followed the task's **ABSOLUTE MINIMUM TESTING** override. One cargo process at a time, offline,
toolchain `+1.98.1`, inherited `CARGO_BUILD_JOBS=4`, unchanged target directory.

- `cargo +1.98.1 fmt -p photocraft-ui-egui -p local-image` — completed.
- `cargo +1.98.1 check --offline -p photocraft-ui-egui -p local-image` — passed; final host-only
  follow-up after its last key-validation change also passed.
- `cargo +1.98.1 clippy --offline -p photocraft-ui-egui -p local-image --all-targets -- -D warnings`
  — passed; final host-only follow-up also passed.
- `cargo +1.98.1 test --offline -p photocraft-ui-egui --lib j23_ -- --nocapture` — **2 passed**.
  Exercises actual cell click, navigation, hide/show, distant-item scrolling, request bounds,
  stale folder/save rejection, and the eight orientation goldens.
- `cargo +1.98.1 test --offline -p local-image j23_ -- --nocapture` — **4 passed**.
  Covers the real host path, developed/cropped parity, catalog/selection preservation, shared
  cache identity, deduplication, eviction, disk reopen, equal-sized save invalidation and RAW
  container orientation.
- Diff whitespace check on edited source files — passed. The inherited task-file EOF whitespace
  was left untouched. No full suites, GPU acceptance runs, benchmarks or quality sweeps were run;
  **GPU tests added: 0**, appropriate to this CPU thumbnail/backend change and the explicit override.

Logs are under `target/j23-*.log`. The initial host-test failure log is retained separately:
its comparison indexed a small legacy image using an upscaled target's coordinates. The reference
now explicitly crops/resizes before comparison; the exact Library golden assertion is unchanged.

TEST-CHANGED: Existing `modal_strip_harness` drops its obsolete `requested` worker-state fixture
field. Its interaction/save assertions are unchanged.

TEST-CHANGED: Existing `thumbnails_decode_and_orient` no longer calls the deleted private walker;
the new eight-layout pixel/alpha golden replaces its single rotated-dimensions assertion.

## Sources and remaining scope

Followed audit §1, §3 D11 and §4 J23, `docs/DESIGN-SYSTEM.md`, and the established source APIs in
`lc-ui-egui/src/{render.rs,panels/grid.rs}`, `lc-engine/src/{media.rs,files.rs,import.rs}`,
`lc-preview/src/{lib.rs,pool.rs}`, and `pc-codecs/src/orientation.rs`.

Adobe's official [Workspace basics](https://helpx.adobe.com/lightroom-classic/desktop/workspace/workspace-basics.html)
and [View photos](https://helpx.adobe.com/lightroom-classic/desktop/viewing-photos/view-photos.html)
support retaining a filmstrip for browsing across modules and selecting previous/next photos.
No new UI strings require an i18n pending entry. Gallery/document composites remain their
deliberate document-rendering APIs; they were not D11's owned filmstrip worker and were not changed.
No implementation work remains for J23; combined acceptance belongs to the coordinator as requested.
