# PDF fallback native-runtime independent source review

**READY for native/runtime integration and central execution: frozen v2 resolves PF-R1, and the frozen v3 geometry correction resolves PF-R2.** This approval covers the native adapter/geometry portion only; the bundled UI correction remains with the independent UI reviewer. No unconfined retry or owner HOME/config mount was found. The remainder is structurally ready for the required central runtime/appearance gates, but source review does not prove those gates. UI workflow review is separately owned.

Read-only review of frozen `pdf-raster-fallback-v1`: implementation patch SHA-256 `31d22cd9f151479936cf3a27e3618db8e8c50d0ca3002c706aa74baa43c88633`, base `a8a6ddcfccea8744c4ff76df0503a048f683d23c`. All manifest-listed artifact byte counts/hashes verified; all eight frozen source copies and owner-checkout files matched the supplied source hashes at inspection. No code edits, builds/tests, renderer/process/runtime probes, GPU/model work or network calls performed. Only this report written.

## Final native/geometry preservation check — v3

Reviewed native-test portion of `pdf-raster-fallback-v3/correction-from-v2.patch`, SHA-256 `48bb1a1ea8b62031ce627d2368b833d72bc13f75cf4561d65a54cd3b1ff226ee`, against the exact v2 base. The supplied v3 sha256.json is a two-file before/source manifest (there is no separate artifact manifest.json); both frozen copies, owner copies and declared v2 base hashes match. Native-test source SHA-256 `e73e45d6dec9a3cb80f6aa8ab6579cff78b5bbf1e7477e46d62d3c802aad9f43`. Production native adapter remains byte-identical to reviewed v2, SHA-256 `f9bfc0403da20adcbf09a39e4056e7943d3fbeb0bf5cefc608842b4403b5668b`.

**PF-R2 closed.** geometry_pdf now declares two pages in correct object order: Pages Kids[3,5], Count2; first page has distinct12×18 MediaBox and magenta content object4; second page is the nonzero-origin quadrant/marker geometry with content object6. Every one of the16geometry combinations now explicitly requests zero-based page1. Thus wrong first-page selection fails both size and color expectations. Existing independent rotation/marker/control formulas, byte tolerance, opaque-alpha assertions and Type4 page0 constant-red control are unchanged. This preserves actual selected-page routing while strengthening the former dimensions-only coverage, without another matrix or production change.

The delta also contains PC pdf_import UI ownership changes; hashes were verified for artifact integrity, but those semantic changes are not accepted by this native-runtime review. Root should use the separate UI review before combining approvals.

**Disposition: READY** for the reviewed v2 native adapter plus v3 native-test correction. No runtime execution, renderer output, startup acknowledgement compatibility, cgroup/namespace behavior or teardown success is claimed. Root still owns the explicit ignored production gate, JPX/JBIG2 fidelity, failure/stress, visual/placement and installed dependency evidence. Full FS009 remains open. Reviewed 2026-10-12T19:37:19.935140+00:00; reviewer frozen.

## Exact frozen v2 re-review (PF-R2 follow-up resolved above)

Reviewed `pdf-raster-fallback-v2/implementation.patch`, SHA-256 `46ce29ff41105a2e1150cc6cfff433561c19a442f0a928eb98d8156668146c4c`, and correction delta `c141a97cd3686b4ea313a6b86d458f07256d9304de7d56fa8df8df02a94d7fad`. All manifest artifact hashes/byte counts and all nine frozen source hashes verified; owner source matched at inspection. Re-review time 2026-10-12T19:32:50.976583+00:00. No tests/runtime/builds or source edits performed. UI publication/Place corrections remain under the separate UI reviewer.

**PF-R1 resolved:** native adapter lines25-60 replaces a bare active boolean with Mutex<Admission> containing active and exact uncertain_unit. Permit::acquire rejects retained uncertainty before considering the active slot; Permit::drop clears only active and cannot erase uncertain_unit. run507-512 acquires this guard before runtime/input activity. Both explicit failed-cleanup return405-408 and unsuccessful Job::drop341-348 retain the exact owned unit. There is no automatic not-found recovery or forgotten-unit re-admission. Conservative session disable is an acceptable bounded correction; automatic recovery is not required.

Job308-328 now observes the trusted systemd-run supervisor's exact `Running as unit: <owned unit>` prefix before terminal/collected state can finish an ordinary cancellation. This text is in the separate supervisor file, not PDF-controlled renderer stdout/stderr. Removing --quiet makes that acknowledgement available. A launcher --wait success still confirms ordinary successful completion. Lost/unrecognized acknowledgement fails closed into the poisoned session rather than permitting a retry. Actual installed-systemd announcement/teardown behavior still needs central runtime proof; source review does not claim it ran.

The local-Mutex pure regression45-64 proves a concurrent acquire is denied, normal drop permits reacquisition, poison retains exact unit after guard drop, and the next acquire fails with that identity. It does not execute or mutate the process-global production slot.

Containment/resource invariants are unchanged: exact library discovery/parser/sandbox payload, clean environment, narrow read-only mounts, private tmpfs, no HOME/config mount or unconfined fallback; same systemd MemoryMax/Swap/Tasks/CPU/time/kill properties; same prlimit address-space/CPU/file/FD/core limits; same input/PNG/diagnostic and decoder caps; same metadata/dimensions/DPI/page/opaque scene checks. The only launcher-argv policy change is removing --quiet for trusted startup evidence. No Rust dependency or runtime-provider broadening added.

**Orientation/origin oracle is materially stronger and mathematically sound:** authored MediaBox[10,20,90,60], CropBox[20,25,80,55] give80×40 and60×30 at72DPI. Four PDF-space quadrants map to blue/yellow upper and red/green lower output quadrants before page rotation. The black rectangle[21,26]–[25,30] is independently sampled at media(13,32) and crop(3,27); both land inside the same authored marker at pixel centre, while x+6 is independently red. Right-angle pixel transforms `(h−1−y,x)`, `(w−1−x,h−1−y)`, `(y,w−1−x)` correctly express clockwise90/180/270 and ignored rotation expects0. All16crop/media×apply/ignore×angle cases check RGB within one byte and alpha255, not Poppler against itself. Type4 constant-red oracle remains.

### Historical v2 PF-R2 — P2 (resolved in v3 above)

`apps/local-image/src/services/pdf_raster_tests.rs:114-121` now renders only page0 of a single-page geometry_pdf. It **replaces** all four v1 `fixture::pages()` runtime requests for page1 (the actual second page). Thus it strengthens orientation/origin but removes the previous executable page-selection control. The pure metadata(bytes,2) unit test does not cover the provider's `-f/-l` selection/zero-to-one-based conversion. The report's “no prior assertion weakened” needs this coverage retained.

Minimal correction: retain one bounded second-page render with an independent distinct expected size/color, or give geometry_pdf a distinct decoy first page and select page1 for the existing geometry loop. No additional angle matrix, production change or resource expansion needed. This is a preservation gap, not a claimed product off-by-one bug. Sent to owner/root; final READY can follow its exact frozen test delta. Runtime execution remains root-owned regardless.

V2 runtime source hash `f9bfc0403da20adcbf09a39e4056e7943d3fbeb0bf5cefc608842b4403b5668b`; native tests `89ddd1def00a51d61cd0882ba209994b81be52fd680aab4c69e18e6d4858c62f`. Other exact v2 hashes remain in its verified sha256.json.

## Historical v1 PF-R1 — P2 (resolved in v2 above)

Source `apps/local-image/src/services/pdf_raster.rs`:

- `execute:357-360` explicitly handles `Job::stop()==false` by returning a cleanup-not-complete failure.
- `Job::drop:294-300` retries, then kills/reaps the **systemd-run waiter**, with one further best-effort unit stop. It can still leave exact service termination unconfirmed, as its own comment acknowledges when the user bus disappears.
- `run:463-473` nevertheless drops `Active`, which always stores ACTIVE=false. The only `unit` string was inside the now-dropped Job. A subsequent request can therefore enter while previous service/start ownership is unresolved.

This violates the claimed single active renderer/still-stopping admission boundary precisely on the supported cleanup-error path. Kill of systemd-run is not equivalent to stopping its transient service. RuntimeMaxSec bounds an activated unit, but is not positive terminal confirmation and does not by itself settle a delayed unit-creation/start race. The code already returns an error instead of publishing; preserve that fail-closed behavior across the **next admission** as well.

Minimal coherent correction: retain an unresolved owned-unit record/poisoned permit on cleanup uncertainty; refuse or reconcile later requests against that exact unit until terminal/not-found state is positively observed. Do not release an unconditional boolean permit while discarding unit identity. Preserve fresh random names and bounded stop/kill/reap; never kill unrelated units or retry unconfined. Add a deterministic injected cleanup-failure/control test proving the next request cannot start a second renderer while ownership remains unresolved, plus recovery once exact-unit terminal state is observed. Sent to source owner/root before freeze.

This is an ownership-state defect found by source control flow, not a claim that the current desktop has leaked a running process or an exploit demonstration.

## Containment/resource paths reviewed

- Fixed system tools only (`:21-24,112-115`): systemd-run/systemctl, env, prlimit, bwrap, ldd, pdfinfo and pdftoppm. Library discovery runs ldd only on the two fixed trusted executables (`:118-141`), with clean env,2s deadline and64KiB diagnostics; no PDF/user executable is passed to ldd.
- `trusted_file`75-80 and `library_lines`82-103 require canonical system binary/library roots, forbid unexpected library mount roots and ParentDir, reject missing libraries and cap the discovered closure. Runtime is cached only after success. No new Rust package/version/edge or Cargo.lock modification appears in the eight-file patch.
- `Runtime::sandbox`152-223 uses clean payload env, private namespaces including network, dropped capabilities, no owner HOME, individual read-only renderer/library/input/private-fontconfig mounts and system font/Poppler-data trees. `/output` is private tmpfs; root is read-only. No writable host directory bind or fallback launch path exists. User/system personal font configuration is not mounted; generated fonts.conf names only system fonts and a private cache.
- systemd resource properties315-334 enforce per-unit CPUQuota100%, MemoryMax768M, MemorySwapMax0, TasksMax16, RuntimeMax10s, TimeoutStop1s and group kill. prlimit158-162 adds512MiB AS,3CPU seconds,64MiB per file,128FD and no core. File creation uses random private0700 scratch and0600 create_new outputs/input. Renderer host writes are its stdout/stderr files; other created files are tmpfs charged to cgroup memory. Parent polls time and response/diagnostic/supervisor sizes; no code path drops containment to obtain pixels.
- `user_command`225-235 builds its bus address only from a numeric /run/user path with a bus; it does not forward caller DBUS_SESSION_BUS_ADDRESS or arbitrary process environment. Service name derives solely from private random scratch and internal stage label. Cleanup targets exactly this owned unit, never broad process matching. Terminal success via systemd-run --wait is distinguished from a plain renderer PID exit.
- Input is bounded by caller bytes and64MiB (`:459-462`). Global single-import admission is useful on ordinary paths, subject to PF-R1. Metadata, PNG, diagnostics and final image work remain off the UI thread through the injected service.

## Provider parsing / pixels / geometry

- Metadata parsing393-445 requires a single Pages field, exactly one selected page's valid finite MediaBox/CropBox and right-angle rotation; page count1..1024 and selected index checked. Values/width/height are bounded. The prior saved **desktop probe**, not a new run, shows Poppler26.01's `Page    1 rot/MediaBox/CropBox` prefixes matching this parser (`pdf-fallback-desktop-probe/receipt.json`); this is only a provider-format sanity check, not validation of the new production launcher.
- Render re-queries selected-page metadata from the exact immutable input; page→one-based conversion is checked. Page count is also limited by request.objects. Width/height use chosen box and DPI, then swap for90/270; dimensions cap at1,000,000 each and16M pixels/caller limit before host image allocation.
- Fixed pdftoppm argv selects one page, requested DPI, PNG and optional CropBox; no output prefix emits stdout. Host decode checks PNG signature, decoder max dimensions and128MiB decoder allocation, then exact dimensions. Ignore-rotation applies the inverse right-angle rotation after decoded size verification; no scaling is used to hide mismatch. Existing source version does not silently accept UserUnit/provider rounding disagreements: mismatched dimensions return failure. Such refusal is a limitation to record, not evidence of universal geometry support.
- UI service boundary `crates/pc-ui-egui/src/pdf_raster.rs:51-85` independently checks dimension/product/RGBA length, finite DPI and fully opaque alpha, then builds one detached Raster layer with an explicit white-page/noneditable warning. Per-row cancellation checks remain during Surface construction. Metadata replies cannot become image scenes. Native adapter cancellation checks occur before launch, during both contained stages and after decode/rotation.
- stderr warnings cause refusal even on exit0 (`native:377-382`), avoiding acceptance of a partially omitted image/font simply because PNG encoding succeeded. Nonzero runtime exits remain errors and cannot publish a partial reply.

## Eligibility and original-scope boundary

Central `ImportError::raster_fallback_reason` matches only static typed Unsupported JPX/JBIG2 image-decoder and calculator-function reasons. Invalid, Limit, Cancelled, password/encryption, external-stream, generic-codec and K=true refusals are not mapped. Preflight classifies eligible image codecs at their explicit refusal sites, and header detection is shared/bounded through offsets0..1024. A first eligible error is not certification that the rest of that PDF is safe; the independent renderer containment remains essential, as the code comment correctly states.

FS009-SCOPE-CHECK remains controlling: this executable, explicit image route can satisfy supported remainder cases after fidelity/runtime proof; a warning-only/nonworking route cannot. Named ordinary native features remain required and are not moved wholesale into fallback. K=true refusal is allowed. This review does not require universal native knockout, touchscreen-style features or a nonempty-password UI and does not close FS009 overall.

## Remaining central evidence (not source pass claims)

- Run the actual newly authored production adapter, not only the earlier tiny probe: metadata, stdout PNG, warning policy, namespace/cgroup availability, exact cleanup, cancellation/output overflow and uncertain-cleanup admission control after PF-R1 correction.
- Existing runtime geometry loop in v1 checks dimensions for four crop/media×rotation combinations but cannot establish orientation/origin/selected-page pixels. Source owner reports independent orientation/origin pixel controls are being added; review them in the next frozen delta. The Type4 red calculator oracle is independent of Poppler output but uniform red alone does not test geometric orientation.
- Real independent JPX/JBIG2 fixtures remain necessary before advertising those allowed categories as validated; typed routing and sandboxing do not prove decoder fidelity. Resource/descendant/time/cancel negatives and staged/installed dependency readiness remain central.
- Release must declare/provide verified distro runtime dependencies. Current app/CLI package does not include them. Explicit Unavailable behavior is appropriate on missing prerequisites but cannot by itself close the requested installed Linux fallback outcome.

## Frozen v1 source hashes

| File | SHA-256 |
|---|---|
| `apps/local-image/src/services.rs` | `48c0190ff648bdefd96b4708846645d7d479b533351a4e707ffd3354a607fafc` |
| `apps/local-image/src/services/pdf_raster.rs` | `1d86385380fd78a0e664a18e9ccc29bb80093cc55a6ba6d425d55b894dc40a6b` |
| `apps/local-image/src/services/pdf_raster_tests.rs` | `ec2c24ed777c54335a1c6b441d254deb3551aa1cf5ab8198b8cd27d77b40240a` |
| `crates/pc-pdf/src/import.rs` | `1ad5e214df07a71d5482d1a27cdeb7d1c39b42bed3556edf9dcdb3b7cd587345` |
| `crates/pc-pdf/src/import/preflight.rs` | `09672dde94fc0f61506093466639fe3ac7478bffa07df9f856cbb8b6119d8f29` |
| `crates/pc-ui-egui/src/lib.rs` | `810647a4f2b0df249d25c2f73c3b31cf5ebb71d5cd47571f7621ff1513222944` |
| `crates/pc-ui-egui/src/pdf_raster.rs` | `9815f0e5f6c95c21ba9a063240e9ab5c8e1aaf7f86e7da4aadc7f65ad356bd99` |
| `crates/pc-ui-egui/src/vector_packages/pdf_import.rs` | `0f0b68092ca28bf7abca3ef7fdf703a768a0f5367dc79bb147a222f7a64e4201` |

Reviewed 2026-10-12T19:06:28.090412+00:00. Waiting for corrected frozen ownership/geometry delta; no runtime proof claimed.
