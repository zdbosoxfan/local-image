# Vector integration final pass

Baseline: `bc8dec3a`. Changes remain uncommitted. All integration handoffs 00–47 and architecture/persona/quality contracts read. Package 47 supersedes earlier duplicate tool routes. Translations remain paused.

## Checklist

| Package | Item | Status |
|---|---|---|
| 00 integration | Verify existing gradient port; dependency audit; central GPU registration | Complete |
| 01 persona | Persona frame/pointer guard; registry; shortcuts; rasterize capture; toolbar reset | Complete |
| 02 live-shapes | Promote design geometry; shared drawing input/context/preview; app GPU | Complete |
| 03 node-editing | Node geometry/renderer/snapping providers; frame/input/context/preview; app GPU | Complete |
| 04 pen | Shared Pen defaults/input/keys/preview; app GPU | Complete |
| 05 pencil | Promote fitter and V1 backend; shared Pencil state/prefs/input; migrate app GPU | Complete |
| 06 corners | Promote live corners; shared lifecycle/context/input/projection; app GPU | Complete |
| 07 snapping | Geometry provider; manager prefs; shared snap cache/guides; Move HiDPI regression | Complete |
| 08 appearance | Appearance CPU routing; combined stroke provider; panel/options/preview; app GPU | Complete |
| 09 scaled-canvas | Native scaled modules; unified shape/text/symbol projection; damage/bounded split; GPU | Complete |
| 10 constructive-tools | Constructive state/tools/input/context/preview and provider; app GPU | Complete |
| 11 width-profiles | Combined width projection; state/options/input; rerun corrected tests; app GPU | Complete |
| 12 path-text | Native on-path module; type refresh/conversion; glyph membership; app GPU/golden | Complete |
| 13 svg-import | SVG Open/Place/Paste magic routes, dialog/state and app GPU | Complete |
| 14 svg-export | SVG export dialog/prefs/menu; projection; integrated import feature; app GPU | Complete |
| 15 symbols | Verify symbol activation/version/migration; crisp master expansion; panel/app GPU | Complete |
| 16 layout-constraints | Layout hooks/tools/window; clips; presets/units; constraints; app GPU | Complete |
| 17 vector-brushes | Promote brush geometry; common stroke provider; state/options/input; app GPU | Complete |
| 18 live-blend | Promote blend evaluator; context/menu/window/input/display; app GPU | Complete |
| 19 live-repeat | Live serde compatibility; repeat frame/input/display/menu/prefs; native/GPU | Complete |
| 20 live-envelope | Live serde compatibility; envelope state/menu/input/display/exchange; native/GPU | Complete |
| 21 pdf-export | PDF export dialog/menu/prefs/font mapping/common projection; integrated roundtrip/GPU | Complete |
| 22 pdf-import | PDF chooser/Open/Place magic routes; state/preview; integrated roundtrip/GPU | Complete |
| 23 psd-advanced-fallback | PSD checked projection/cache import/compositor roles/warnings; IO tests/app GPU | Complete; external gates skipped below |
| 24 affinity-import | Affinity Open/Place worker/magic routes, native destination policy, state/app GPU | Complete |
| 25 trace-centreline | Verify centreline registration; output/profile routing; tests/app GPU; scans skipped | Complete; external gates skipped below |
| 26 trace-logo-cleanup | Cleanup options/worker/dialog/statistics; app gates; external corpora skipped | Complete; external gates skipped below |
| 27 trace-cutout | Cutout frontend/output; fix seams/node counts; independent quality/GPU gates | Complete; external gates skipped below |
| 28 trace-dialog-output | Trace app state/prefs/menu/jobs; enable real providers; preview query/GPU | Complete |
| 29 library-vectorize-handoff | Library context target/selection fix; host/jobs/return bridge; modal and app gates | Complete |
| 30 frame-composer | Frame metadata/refresh/layout/native controls/input/preview and app GPU | Complete |
| 31 geometry-quality | Generate 345 Inkscape outputs/sidecars; SOURCES; tricky and app GPU quality gates | Complete |
| 32 vector-selection | Selection slot/storage/hooks/readers/tools/menu/frame/route; perf/zero-axis regression | Complete |
| 33 anchor-tools | Anchor command/activation/frame/routes/visible handles/convert; Pen prefs; app GPU | Complete |
| 34 transform-tools | Transform tools/state/prefs/keys/dialogs/input/preview; rerun corrected tests; app GPU | Complete |
| 35 clip-compound | Clip state/prefs/menu/keys/draw insertion/Layer drag/display; corrected tests/app GPU | Complete |
| 36 object-path-commands | Object Path frame/menu/clipboard/display; expansion providers; corrected tests/app GPU | Complete |
| 37 view-modes-isolation | Isolation/view runtime hooks/slot/scope; modes/eye/thumbnail/canvas/projection; app GPU | Complete |
| 38 eyedropper-styles | Style native list/link/hooks/pruning; readers; eyedropper/styles panel/routes; native/GPU | Complete |
| 39 global-colours-recolor | Swatch native book/targets/remapping/access; palette/recolor/ASE/preview; native/GPU | Complete |
| 40 blob-eraser | Blob/Eraser tools/state/options/input/paint/preview; app GPU | Complete |
| 41 vector-distort-tools | Distort specs/tools/options/activation/frame/input/context/preview; golden/app GPU | Complete |
| 42 live-paint | Live Paint variant/wire/expansion/projection; narrow hits/tests/tools/menu/input; GPU | Complete |
| 43 gradient-mesh | Mesh paint data/sampler/exhaustive matches; specs/tools/state/options/view/export; GPU | Complete |
| 44 measure-area | Measure specs/tools/frame/route/options; drawing scale persistence/setter; app GPU | Complete |
| 45 path-effects | Effects metadata/access/specs/refresh/projection/menu/panel/modal/expand/export; tests/GPU | Complete |
| 46 vector-toolbar | One toolbar/keymap prefs; 22 flyouts/proxy/menu adapters; icons/attribution CLI; app GPU | Complete |
| 47 shared-tools-unification | One shared runtime across personas; supersede duplicate 02/04 routing; migrated GPU | Complete |

## Defects and validation

All package fragments are wired to the production app and package 47 shared runtimes. Native Repeat/Envelope tagged payloads preserve live data through reload; scaled symbols expand crisp vector masters; snapping uses logical/device coordinate conversion consistently. A shared GPU lock serializes engine/UI GPU lifetimes while ordinary tests remain parallel.

Trace repairs cover T1 finite/simple fills, cleanup Pixel exact lattice behavior and Cutout shared seam/node quality. Live Paint checks narrow pointer-segment hits, stable source signatures and transactional spec edits. SVG import integration is enabled. Translations remain paused; English keys are recorded in pending/KEEP lists.

Review fixes include captured-target and failed-publication lifecycle, bounded generation/flattening, retained preview/cache work, native metadata and exchange validation. Complete individual verdicts below distinguish real fixes from rejected findings.

Failure-driven repairs also cover the shared Eyedropper and Type-on-curve routes in both personas; Pen modifier capture; node-lasso Escape ownership; native PDF/Outline Stroke menu routes; frame-composer controls in Box Type context; preset design units; and clip transforms during Move. Live-shape previews replace their captured layer, and detached corner projections clear baked corner metadata to prevent double rounding. Move now draws the scaled vector tiles prepared for its uniforms, fixing the actual half-size preview at HiDPI. The existing 1×/2× guide regression and three related Move GPU gates pass.

The deferred gates exposed additional production defects: zero-height freeform geometry was silently scaled by the options bar on idle frames; Repeat's detached preview was absent from common display routing; baked path effects retained their live effect metadata; Style Picker excluded raster sources; long vector options overflowed their fixed bar; and generic Delete dispatch took Curvature's selected-point key. These are repaired through production routes and covered by regression/full-app gates. Brush fitting now restores endpoints within its tolerance, and Break/Join retain dormant endpoint corner data until those knots become eligible again. The final Inkscape rerun also caught a redundant inner-join triangle correction that added area at short butt-capped dash corners. Raster paint and editable expansion now retain the shared Kurbo stroke geometry directly; native 4× pixel parity and all 345 external references pass with the original tolerances.

Detached live-group projections reserve generated layer IDs against the full source document and preceding groups. Isolated Blend projection previously collided with a following clipped raster layer, making GPU layer lookup wrong despite identical CPU composition; the full-app gate now asserts projection ID uniqueness.

GPU view compositors now retain independent resident caches for the base canvas and each scaled view, sharing the configured total memory budget. Common projected previews report conservative damage instead of refreshing the entire canvas for each pointer frame. Geometry toolbar eligibility reuses captures while document identity, preferences, selection and Alt intent remain unchanged, with freshness checked again at execution. Document ID lookup now stops at the first matching subtree, and selected-layer traversal uses membership lookup without allocating a path for every layer. Anchor-handle preparation only runs for anchor tools. The retained ordering/stale-selection contracts have focused regressions; release measurements below establish the resulting performance.

Dense distortion strokes use an additional Kurbo recursive-subdivision refit route for both preview and commit, retaining the same half-tolerance margin, corner boundaries and no-increased-node guard. The Object > Path simplifier retains its optimal-subdivision route. A 2,000-node independent distance/tolerance and replay regression passes; refit cost fell from 15.044 ms to 2.527 ms (including independent geometric checks) in the optimized test profile. The 10,000-case run also reduced an upstream contour-extraction panic and two approximate-fitting misses. Checked contour extraction now propagates a typed geometry error through normalization, offsets and Pathfinder. Refit verification uses bidirectional nearest-point samples, retries at tighter internal accuracy and preserves source runs when retries fail; all reduced seeds remain in the regression corpus. 

The property runner scales its rejection budget with accepted-case count without changing geometric preconditions or thresholds. Sampled point fitting retains already minimal single cubics instead of passing thousands of Puppet Warp pieces through redundant optimization and geometric checks; endpoint/tangent and sampled-error regressions cover the retained fitting contract. 

Fill tracing likewise avoids optimal-subdivision search while retaining its fidelity and node gates; release Logo 1024px took 0.070491 seconds and Photo 2048px 0.538769 seconds against 1.5/8 second budgets.

The text-on-path reference retains the supplied IoU > 0.97 gate and unchanged Inkscape outlines. Both renderers are compared at 8x resolution to reduce threshold-mask sensitivity to different antialias coverage sampling: IoU 0.991751. The reference generation and sampling details are in [path_text/SOURCES.md](../../crates/pc-ui-egui/tests/path_text/SOURCES.md).

The test environment sets `__GL_SHADER_DISK_CACHE=0`: the NVIDIA process teardown previously exited 139 after successful GPU assertions; with this setting the isolated test processes exit cleanly. This is an empirical test-process workaround, with no app driver configuration changed. NVIDIA documents the variable in its [driver README](https://download.nvidia.com/XFree86/Linux-x86_64/550.144.03/README/openglenvvariables.html).

Final validation is complete. Builds use Rust 1.98.1, offline, `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=3` and `li-rustc-gate`, with serial Cargo invocations. Full workspace check and strict all-target Clippy ran once, followed by failure/affected-target resumes, covering all **56 workspace packages**. The full workspace test run ran once, followed by focused repairs and resumes: **10,533 distinct passing cases / zero unresolved failures**, with **71 ignored registrations**. The two explicitly skipped Photoshop corpus tests are separate from that count. Case identity uses source target and test name across Cargo feature hashes; property-generated inputs are not added to the case count.

All **69 cases in the original deferred acceptance batch now pass** on the real RTX 5090. Release/RTX gates, geometry property runs with `PROPTEST_CASES=10000`, and final checks of the latest fixes are complete. The remaining ignored registrations cover optional benchmarks/diagnostics, missing licensed files/weights and duplicate registrations of the available Cutout competitor gate already executed through its integration-test target; they are not unresolved vector GPU gates. Historical failures, repair runs, exact commands, measurements, log hashes and final per-case outcomes are recorded in [the validation ledger](vector-integration-validation.json).

| Validation | Final result |
|---|---|
| Workspace check, all targets; strict workspace Clippy, all targets, `-D warnings` | Pass; affected 15-package resumes also pass |
| Full workspace tests plus focused failure/affected-target resumes | 10,533 distinct passes; zero unresolved failures |
| Deferred vector release/RTX 5090 acceptance batch | 69/69 pass |
| PathOps geometry properties at 10,000 cases; reduced regression seeds | Pass; no thresholds relaxed |
| Geometry full-app GPU, brush and pencil live GPU after the final fitter changes | Pass |
| Inkscape sidecar preparation and geometry golden/ratchet gate after final PathOps changes | Pass; 345/345, zero ratchets |
| Paused-i18n literal coverage; final `cargo +1.98.1 fmt --all -- --check` | Pass |
| Attributions regeneration; dependency package-entry audit | Pass; zero Cargo.lock additions/removals |

The full workspace test command was `cargo +1.98.1 test --workspace --no-fail-fast --offline -- --skip corpus_shape_coverage_matches_photoshop --skip corpus_vector_blocks_survive_roundtrip`; those two missing-corpus gates are documented below. All app validation used production headless/offscreen harnesses, as required by the GUI prohibition.

Completed geometry acceptance: **345/345**, zero ratchets, unchanged tolerances. Exact Inkscape inputs/actions/raw outputs and the two analytically equivalent conditioning recipes are retained in [tricky/SOURCES.md](../../crates/pc-pathops/tests/tricky/SOURCES.md). Cargo.lock adds **zero package entries** relative to the baseline; no new C/-sys dependency.

## Release measurements

RTX 5090 / Vulkan discrete adapter / NVIDIA 615.71.09. Pointer timings include the actual app step, GPU work and offscreen readback; tests check Rgba32Float compositing and the real float canvas. These are headless measurements, with no desktop window opened.

| Gate | Initial warm p95 | Measured repaired warm p95 | Result / budget |
|---|---:|---:|---|
| 6000×4000, 500-shape scaled node drag | 51.595 ms | 15.674 ms | Pass, <16 ms |
| 500-object selection | 70.570 ms | 5.957 ms | Pass, <16 ms |
| 5000-path view mode | 17.978 ms | 10.392 ms | Pass, <16 ms |
| 2000-node distortion, all eight tools | Warp 20.734 ms; stopped at first failure | 6.916–8.769 ms | Pass, ≤16.667 ms |
| Shared Pen, 6000×4000 / 500 shapes / zoom 2 | 86.619 ms | 34.262 ms | Two app steps + readback per sample; measured gate passes |
| Shared Rectangle, same fixture | 91.803 ms | 35.514 ms | Two app steps + readback per sample; measured gate passes |

The scaled-node run reduced uploads across 64 frames from 12,544 to 64 (64 rebuilds). Shared Pen's last-frame uploads fell from 253 to 1 and RSS after the 64-frame run from 1,901,797,376 to 518,467,584 bytes; Rectangle fell from 253 to 1 uploads and 1,982,812,160 to 758,308,864 bytes. Cold first-frame and commit costs remain separately recorded in the validation ledger. Distort first frames took 11.582–14.751 ms; releases took 8.901–11.235 ms except Puppet Warp at 51.927 ms. The warm pointer budget does not apply to commit; its full-app commit/undo/cancel gate passes.

Other completed measurements: 20×20 mesh median/p95 14.490/14.706 ms; Live Paint click-to-painted-frame median/p95 85.556/88.609 ms; linked global-colour changes across 2000 objects median/p95 369.572/438.650 ms (commit 93.458 ms). The latter two gates are reported under their supplied budgets/measurement contracts; no 60 Hz claim is made for them.

## Skipped external gates

- Package 23: `corpus_shape_coverage_matches_photoshop` and `corpus_vector_blocks_survive_roundtrip`; `corpus/psd` is absent.
- Packages 25/26: external scan/logo corpora absent; tests/corpus contains its source note only.
- Complete trace B.4 competitor/fitter-selection matrix: the required full references and vtracer 1.0.0-alpha.4 / 0.6.5 executables are absent. `/usr/bin/potrace` is present. The inherited matched Logo references cover three shapes, three sizes and six variants; only the supplied 18-case 256px Cutout competitor gate is claimed. No complete-matrix quality result or benchmark-selected fitter default is claimed. See [references/SOURCES.md](../../crates/pc-trace/tests/references/SOURCES.md).

## DeepSeek verdicts

Verified all **504 findings** from the local DeepSeek JSONL: **176 fixed, 328 rejected**. Rejected findings include incorrect Rust/egui behavior claims, private-kernel inputs already checked at public boundaries, intentional native contracts and hardware requirements explicitly imposed by this task. The JSON artifact retains every original finding and a stable index: [verdicts](vector-integration-review-verdicts.json).

| # | File:line | Verdict | Verification / repair |
|---|---|---|---|

| 1 | `crates/pc-affinity/src/tests.rs:162` | Rejected | PNG decoding accepts unknown ancillary chunks with the reserved bit set; W3C PNG section 5.4 advises decoders not to complain about this bit. Existing regression covers the behavior. |
| 2 | `crates/pc-affinity/src/tests.rs:65` | Fixed | Changed only the two-byte version field to 13, preserving flags and persona magic. Existing rejects_assets_versions_offsets_lengths_and_bombs test now exercises the version check independently. |
| 3 | `crates/lc-ui-egui/src/vectorize_handoff.rs:22` | Rejected | Handoff 29 and request regression explicitly require a captured catalog revision. A rating change is a stale-request boundary; preserving the captured state is intentional. |
| 4 | `crates/pc-affinity/src/import.rs:63` | Rejected | preview() borrows a bounded 16 MiB PNG slice and checks dimensions. State::image preflights encoded pixel dimensions, image decoder Limits and prepared cache bytes against ImportOptions before decoding. |
| 5 | `crates/pc-affinity/src/import.rs:71` | Rejected | Preview.png is a borrowed &[u8], not a Vec. Encoded takes ownership once; to_vec is necessary for the existing image adapter and does not clone an already-owned preview. |
| 6 | `crates/pc-color/src/ase.rs:122` | Rejected | Color::BLACK initializes alpha=1. LAB changes only mode and c; c[3] is an unused component, distinct from alpha. |
| 7 | `crates/pc-color/src/ase.rs:185` | Rejected | ASE LAB L uses normalized 0..1 encoding; reader also tolerates external 0..100 writers. Write/read preserve the native normalized value. |
| 8 | `crates/pc-color/src/ase.rs:137` | Rejected | Spot is a global color by design in the native book; ASE writer prioritizes the spot tag and preserves it. |
| 9 | `crates/pc-doc/src/vector_design/appearance.rs:43` | Rejected | StrokeItem has container serde(default) using its explicit Default implementation, where visible=true. |
| 10 | `crates/pc-doc/src/vector_design/appearance.rs:53` | Rejected | StrokeItem serde(default) uses its explicit Default, where arrow_scale=[1,1]. |
| 11 | `crates/pc-doc/src/vector_design/live.rs:8` | Fixed | LiveGroup wire variants now contain a spec field, separating the outer live-group kind from each inner repeat/envelope tag. Legacy flat wire reader retained; native round-trip regressions added. |
| 12 | `crates/pc-affinity/src/synth.rs:216` | Rejected | Pinned ruzstd 0.9.0 encoding::compress_to_vec returns Vec<u8>; source signature verified in Cargo registry. |
| 13 | `crates/pc-affinity/src/synth.rs:125` | Rejected | Shared class-chain final marker intentionally has no u16; stream::Parser::shared reads marker1/tag then fields directly. Inline/nonfinal headers use u16. |
| 14 | `crates/pc-affinity/src/synth.rs:117` | Rejected | synth is a development fixture constructor, and Def explicitly requires a nonempty class chain. No untrusted input reaches it; runtime reader separately handles malformed chains. |
| 15 | `crates/pc-affinity/src/preview.rs:113` | Rejected | Compressed metadata/APNG preview recovery is explicitly unsupported to bound metadata allocation; native import remains available. This is a stated recovery policy, not a silent loss. |
| 16 | `crates/pc-affinity/src/geometry.rs:124` | Rejected | bounded_outline returns node-space geometry. world affects absolute corner-radius normalization only; compound applies child local once and model applies the parent world once. |
| 17 | `crates/pc-affinity/src/geometry.rs:99` | Rejected | outline API explicitly returns node-space geometry. model::Reader::kind and attached_masks apply world after returning; curve geometry is not expected in world coordinates here. |
| 18 | `crates/pc-affinity/src/geometry.rs:131` | Fixed | The compound adapter rejects any unreadable operand and warns that the whole compound is omitted, instead of silently returning partial geometry. |
| 19 | `crates/pc-affinity/src/text.rs:114` | Fixed | Validate FrmB/ArtV before text extraction and reject nonfinite baseline sums; synthetic public-reader regression covers NaN/Inf. |
| 20 | `crates/pc-affinity/src/text.rs:212` | Fixed | Discarded stroke gradients now charge the per-text attribute budget; shared descriptor regression verifies bounded repeated decoding and preserved text. |
| 21 | `crates/pc-affinity/src/text.rs:161` | Fixed | Charge TextRun Vec capacity and owned text before push; expanded-model limit regression uses many one-character runs. |
| 22 | `crates/pc-affinity/src/import/paint.rs:75` | Rejected | Stop.bias is a property of its stop. Sorting complete Stop records preserves that association; the source model defines the segment from that stop to the next, ordered by position. |
| 23 | `crates/pc-engine/src/layer_copy_cmds.rs:108` | Rejected | color_cmds::convert_document converts all symbol definitions, and layer-copy depth conversion also visits master trees before publishing. |
| 24 | `crates/pc-compose/src/shape_split.rs:123` | Rejected | GPU plan structure keys include canvas dimensions; actual ShapeSplit cache uses the real canvas. Rect::EMPTY here is only one part of structural state. |
| 25 | `crates/pc-compose/src/shape_split.rs:354` | Rejected | Nonfloat Surface writes clamp/quantize every channel through the raster sample type. This branch uses that same write path. |
| 26 | `crates/pc-engine/src/transform_cmds.rs:199` | Fixed | Whole-object Symbol transforms now also transform the entire linked raster mask, matching Shape/Text/Group. Included in transform regressions. |
| 27 | `crates/pc-engine/src/artboard_cmds.rs:123` | Rejected | rect_param already calls constraints::arithmetic::validate_rect, bounding coordinates and far edges before integer conversion. |
| 28 | `crates/pc-engine/src/vector_api/tests.rs:24` | Rejected | Session::edit uses Arc copy-on-write. A captured Arc is an immutable before-state; pointer and document assertions are valid and detect edits. |
| 29 | `crates/pc-engine/src/prefs.rs:760` | Fixed | Serialize default persona/vectorSet/pixelTool/vectorTool fields so prefs.get/set can address them. Added preferences API regression. |
| 30 | `crates/pc-engine/src/prefs.rs:1602` | Fixed | edit.toolbar reset selects the active persona default. Added Vector customization/reset regression retaining Pixel selection. |
| 31 | `crates/pc-engine/src/prefs.rs:1616` | Fixed | edit.toolbar copies, filters and updates the current persona set; added Vector hide/reorder regression. |
| 32 | `crates/pc-affinity/src/paint.rs:236` | Fixed | Bound position collection to MAX_STOPS and two fields per record before allocation. Report truncation; synthetic oversized-position regression added. |
| 33 | `crates/pc-engine/src/path_edit_cmds.rs:65` | Rejected | The document parameter is a captured active-document identity guard. node::guarded rejects a nonactive document before editing; no other target document is edited. |
| 34 | `crates/pc-engine/src/vector_packages.rs:37` | Fixed | The package aggregate now registers every declared command module, including selection/isolation/view. Prior builder/coordinator omission is repaired. |
| 35 | `crates/pc-affinity/src/model.rs:377` | Fixed | Validate source spread/default/page dimensions and finite coordinates before model publication. Missing legacy bounds retain the existing recovery path. |
| 36 | `crates/pc-affinity/src/model.rs:482` | Fixed | Report attached plus child pixel masks explicitly before applying the documented first-mask recovery policy. |
| 37 | `crates/pc-engine/src/toolsets.rs:291` | Rejected | Every SHARED_ADDITIONS ID, including PointTransform, occurs in VECTOR_TOOLS. canonical_tool includes VECTOR_TOOLS and recognizes them. |
| 38 | `crates/pc-engine/src/vector_export_warnings.rs:29` | Rejected | export_psd captures the active document and revision as a stale-export guard. It is not an arbitrary open-tab export API. |
| 39 | `crates/pc-engine/src/vector_cmds.rs:452` | Rejected | All production translate_vectors callers supply integer offsets from raster/document moves; arbitrary fractional transforms use symbols::render_instance instead. |
| 40 | `crates/pc-engine/src/vector_cmds.rs:452` | Rejected | shift_surface uses sparse Surface::translated, an offset-only operation without the alleged massive allocation; production offsets are bounded integers. |
| 41 | `crates/pc-engine/src/vector_packages/affinity/tests.rs:110` | Rejected | Task/handoff explicitly requires a discrete Vulkan RTX 5090 and forbids adapter skips. This acceptance assertion is required. |
| 42 | `crates/pc-engine/src/vector_packages/affinity/tests.rs:126` | Fixed | Added explicit CPU/GPU vector length equality before per-pixel comparison. |
| 43 | `crates/pc-engine/src/vector_packages/affinity/tests.rs:150` | Fixed | Added explicit fallback CPU/GPU vector length equality before comparison. |
| 44 | `crates/pc-engine/src/pathops_cmds.rs:74` | Fixed | Boolean operations reject explicit non-string and unknown operation values; focused regression passes. |
| 45 | `crates/pc-engine/src/pathops_cmds.rs:224` | Rejected | keep_compound stores live booleans in PathOp components. ShapeLayer.live denotes parametric shapes, not boolean components; clearing it is correct. |
| 46 | `crates/pc-engine/src/vector_api.rs:50` | Fixed | Borrowed shape-target preflight revalidates every target at publication without cloning shapes; late-lock atomicity regression passes. |
| 47 | `crates/pc-affinity/src/container.rs:399` | Rejected | Predictor 0xC0 is an unsupported archive encoding, returned explicitly. No cited upstream rule establishes a correct decoder; unsupported data must not be guessed. |
| 48 | `crates/pc-affinity/src/container.rs:332` | Rejected | Preallocation is bounded by caller-declared and enforced per-entry/total limits, at most 64 MiB. Decompression is capped at declared size+1; not unbounded. |
| 49 | `crates/pc-affinity/src/raster.rs:171` | Fixed | Replaced per-pixel Vec allocation with a fixed five-channel stack array. Existing RGBA/CMYK/mask decoding regressions retain pixel semantics. |
| 50 | `crates/pc-engine/src/vector_packages/appearance.rs:63` | Rejected | UI appearance preview uses target-local prepared caches and generation-cached detached documents. Commands intentionally prepare a synchronous bounded preview; pointer-only frames do not rerender. |
| 51 | `crates/pc-doc/src/vector_design/swatches.rs:149` | Rejected | Native Load remaps layer IDs but deliberately retains SymbolId values, asserted by symbols_cached_instances_zip_directory_and_layer_id_remap. PaintTarget.symbol must therefore remain unchanged. |
| 52 | `crates/pc-doc/src/vector_design/swatches.rs:98` | Fixed | Reserve UTF-16 suffix space before generating duplicate palette names, preserving Unicode scalar boundaries. Added long-emoji ASE/native roundtrip regression. |
| 53 | `crates/pc-engine/src/vector_packages/affinity.rs:184` | Rejected | Affinity imported artboard geometry and request offsets are bounded to one million pixels before translate, so i32 addition cannot overflow. |
| 54 | `crates/pc-affinity/src/stream.rs:401` | Fixed | Check the final shared class-chain length after the terminal marker; 65-class fixture now fails the 64-class limit. |
| 55 | `crates/pc-engine/src/vector_packages/anchor_tools.rs:270` | Rejected | Matching the NewShape variant copies only its Copy Option<LayerId> field. The compiled UI/engine already proves this is legal; no borrowed owned target is moved. |
| 56 | `crates/pc-engine/src/vector_packages/anchor_tools.rs:74` | Rejected | node::path resolves the requested namespace. A layer target has its layer path; nonlayer work/named targets have no layer corners to validate. |
| 57 | `crates/pc-engine/src/vector_packages/anchor_tools.rs:219` | Rejected | curvature_fit::fit checks every point for finite coordinates and MAX_COORDINATE before fitting; caller receives the error before cache or publication. |
| 58 | `crates/pc-engine/src/vector_packages/appearance/gpu.rs:20` | Rejected | The task expressly requires these RTX 5090 Vulkan acceptance gates without software skips. |
| 59 | `crates/pc-engine/src/align_cmds.rs:303` | Fixed | Automatic spacing now uses the maximum far edge across bounds. Added a wide-middle regression verifying gap and retained outer extent. |
| 60 | `crates/pc-engine/src/vector_packages/clip_compound/tests.rs:363` | Rejected | The test compares complete path metadata and PSD conversion elsewhere; this supplementary coordinate zip checks expected points. No production mismatch is evidenced. |
| 61 | `crates/pc-engine/src/vector_packages/blend.rs:124` | Fixed | Blend creation now checks every ancestor for live metadata, including through ordinary groups. Added atomic nested-ancestor rejection regression. |
| 62 | `crates/pc-engine/src/vector_packages/drawing/tests.rs:136` | Rejected | prepare expects a ShapeLayer third argument and returns ShapeLayer. The second call passes exactly that type and compiles. |
| 63 | `crates/pc-engine/src/vector_packages/distort_tools/tests.rs:70` | Rejected | The cited test calls select_layer(ids[0]) and clears only additional selected_layers; active_layer remains ids[0]. The next test separately covers no-active all-layer fallback. |
| 64 | `crates/pc-engine/src/vector_packages/corners.rs:104` | Rejected | Corner preview_document runs only on changed drag geometry. The shell caches its Arc by generation for pointer-only frames; no per-frame full clone occurs when warm. |
| 65 | `crates/pc-engine/src/vector_packages/corners.rs:149` | Rejected | metadata and gesture.edits both derive one-for-one from the same bounded raw edits array; snapshot_shapes preserves order and rejects duplicate/invalid targets. Lengths cannot differ. |
| 66 | `crates/pc-engine/src/vector_packages/blob_eraser/tests.rs:164` | Rejected | Both compared masks are rasterized over the same explicit rectangle; their lengths are guaranteed by the coverage API. No missing production pixels are shown. |
| 67 | `crates/pc-engine/src/vector_packages/blob_eraser.rs:139` | Rejected | Capture.validate calls vector_api::snapshot_shapes for the complete target set, which rejects inherited all/pixel locks even for limited selection. Locked edits never publish. |
| 68 | `crates/pc-engine/src/vector_packages/blob_eraser.rs:175` | Rejected | snapshot_shapes is a pure clone/preflight helper and does not create history snapshots or call Session::edit. |
| 69 | `crates/pc-engine/src/vector_packages/blob_eraser.rs:618` | Rejected | Stroke updates prepare bounded geometry/cache previews; pointer-only warm frames use display_doc caching. The required release performance gate measures the actual route. |
| 70 | `crates/pc-compose/src/scaled_view.rs:129` | Rejected | FxPaint::Pattern has only name/id/scale, with no phase field. Scale is updated; the reported phase cannot exist in this model. |
| 71 | `crates/pc-engine/src/vector_packages/drawing.rs:68` | Rejected | Document::effective_locks handles missing paths by defaults, then drawing creation returns NoLayer. It does not panic or publish. |
| 72 | `crates/pc-engine/src/vector_packages/constraints.rs:34` | Rejected | Artboards are LayerContent::Group with group.artboard metadata. The Group match supports both ordinary groups and artboards. |
| 73 | `crates/pc-engine/src/vector_packages/constraints.rs:12` | Rejected | Artboard children are group.children and has_constraints recursively examines that same vector. |
| 74 | `crates/pc-engine/src/vector_packages/constraints.rs:27` | Rejected | arithmetic::resize validates old/new/child rectangles before axis arithmetic. Zero width/height is rejected before division. |
| 75 | `crates/pc-engine/src/vector_packages/colours.rs:599` | Rejected | Global/spot swatches are restricted to solids by SwatchBook.validate and resolve. Gradient swatches are local; global gradient-stop links reference solid global swatches. |
| 76 | `crates/pc-engine/src/vector_packages/colours.rs:455` | Rejected | Merge intentionally replaces linked source colors with the destination swatch at the existing tint. The merge regression explicitly verifies this behavior; it does not promise preserving deleted swatch colors. |
| 77 | `crates/pc-engine/src/vector_packages/colours.rs:486` | Rejected | Group assigns the supplied members to a group, retaining other existing members. Ungroup is the separate remove-membership operation; no replacement-membership contract is present. |
| 78 | `crates/pc-engine/src/vector_packages/colours.rs:284` | Rejected | selection is the document object namespace and Session selected_layers never contains master child IDs. Whole-document collection separately visits symbol masters with explicit symbol namespaces. |
| 79 | `crates/pc-engine/src/vector_packages/envelope/outlines.rs:15` | Rejected | Each PaintedOutline is an independent glyph region with hole contours. Combine starts each glyph; Join retains its internal winding holes. Union of glyph regions is intentional. |
| 80 | `crates/pc-engine/src/vector_packages/layout/tests.rs:416` | Rejected | json! serializes capture by reference, so repeated loop use does not move it. Engine/UI test compilation confirms this. |
| 81 | `crates/pc-engine/src/vector_packages/layout/gpu.rs:15` | Rejected | The task explicitly requires real RTX 5090 Vulkan gates; skipping the mandated hardware would weaken validation. |
| 82 | `crates/pc-engine/src/vector_packages/distort_tools.rs:391` | Rejected | Scratch add_document retains the source DocId; its private revision does not replace the session capture guard. |
| 83 | `crates/pc-engine/src/vector_packages/distort_tools.rs:423` | Rejected | The detached document retains DocId; one Session.edit publishes the result and owns the undo revision. |
| 84 | `crates/pc-engine/src/vector_packages/distort_tools.rs:208` | Rejected | Prepared.document is consumed by the production distort preview/display_document adapter. |
| 85 | `crates/pc-engine/src/vector_packages/constraints/arithmetic.rs:70` | Rejected | Transform coefficient and determinant budgets intentionally reject unrepresentable affine maps even when source coordinates are individually bounded. |
| 86 | `crates/pc-engine/src/vector_packages/constraints/arithmetic.rs:73` | Rejected | The bounded affine representation must validate the resulting coefficients and determinant independently of input coordinate boxes. |
| 87 | `crates/pc-engine/src/vector_packages/brushes.rs:130` | Rejected | Prepared.document is consumed by the brush canvas preview with compiled appearance caches. |
| 88 | `crates/pc-engine/src/vector_packages/isolation/tests.rs:232` | Rejected | Adding a group above the deepest leaf adds one level; MAX_DEPTH + 1 is correctly rejected. |
| 89 | `crates/pc-engine/src/vector_packages/live_paint/gpu.rs:40` | Fixed | Added explicit CPU/GPU pixel-count equality before zip, so an empty/truncated GPU result cannot pass the comparison. |
| 90 | `crates/pc-engine/src/vector_packages/envelope.rs:184` | Rejected | Decoration style indices originate in validated internal text layout; external input cannot inject a Layout with arbitrary indices. |
| 91 | `crates/pc-engine/src/vector_packages/envelope.rs:412` | Rejected | The automation command returns projected paths by contract; the production UI uses the detached document directly and does not serialize JSON per pointer frame. |
| 92 | `crates/pc-engine/src/vector_packages/envelope.rs:398` | Rejected | Cache preparation validates every target before the single native publication; callers need not have run a prior preview. |
| 93 | `crates/pc-engine/src/vector_packages/envelope.rs:317` | Rejected | Other live envelopes must be projected for the detached preview; the shared UI caches warm document projection by source Arc and revision. |
| 94 | `crates/pc-engine/src/vector_packages/mesh/tests.rs:122` | Fixed | Mesh command regression now unconditionally requires native schema success, stored data, undo and no direct-handler journal entry; focused regression passes. |
| 95 | `crates/pc-engine/src/vector_packages/live_paint/tests.rs:42` | Rejected | Arc<Document> snapshots are immutable; writes use copy-on-write, so pointer identity detects publication rather than hiding mutation. |
| 96 | `crates/pc-engine/src/vector_packages/live_paint/tests.rs:124` | Fixed | Live Paint tests now require available native schema success and separately test an invalid empty graph; focused regression passes. |
| 97 | `crates/pc-engine/src/vector_packages/mesh.rs:313` | Rejected | Mesh::regular rejects zero and out-of-budget dimensions before division; Mesh.validate checks geometry. |
| 98 | `crates/pc-engine/src/vector_packages/mesh.rs:182` | Rejected | Mesh and fill appearance variants retain envelope fields; native mesh command regression compiles and passes. |
| 99 | `crates/pc-engine/src/vector_packages/expand.rs:111` | Fixed | Fill/stroke-only expansion rejects path text unless object expansion is enabled; focused regression passes. |
| 100 | `crates/pc-engine/src/vector_packages/expand.rs:175` | Fixed | Fill/stroke-only expansion rejects live groups before geometry could be discarded; focused regression passes. |
| 101 | `crates/pc-engine/src/vector_packages/expand.rs:251` | Rejected | Capture validation rejects overlapping ancestor/descendant targets before aggregate knot preparation. |
| 102 | `crates/pc-engine/src/vector_packages/expand.rs:165` | Fixed | The UI caches exact expand availability per source Arc, revision and selected IDs, avoiding repeated full preparation in an open menu. |
| 103 | `crates/pc-engine/src/vector_packages/path_commands/clipboard.rs:123` | Rejected | Document.path_of returns the root index for a root layer; valid root paths are not empty. |
| 104 | `crates/pc-engine/src/vector_packages/live_paint.rs:405` | Fixed | Flood Fill now rejects Mesh paint explicitly without changing the document; focused native mesh regression passes. |
| 105 | `crates/pc-engine/src/vector_packages/live_paint.rs:383` | Rejected | InBetween inserts a sibling of the envelope group, not a child; the inherited envelope is applied once. Inside resets child envelope fields. |
| 106 | `crates/pc-engine/src/vector_packages/live_paint.rs:327` | Rejected | Graph::new validates the nonempty 1..128 source budget before indexing; empty-graph regression passes. |
| 107 | `crates/pc-engine/src/vector_packages/layout.rs:389` | Rejected | Borrowed Copy fields and destructuring are valid Rust; engine unit targets compile. |
| 108 | `crates/pc-engine/src/vector_packages/layout.rs:326` | Rejected | Borrowed Copy fields and destructuring are valid Rust; engine unit targets compile. |
| 109 | `crates/pc-engine/src/vector_packages/layout.rs:245` | Rejected | transform_layer_map transforms group children but not artboard metadata; the layout path updates artboard bounds once. |
| 110 | `crates/pc-engine/src/vector_packages/clip_compound.rs:444` | Fixed | Move Outside validates the parent envelope and inherited locks without recursively rejecting unselected locked siblings; focused regression passes. |
| 111 | `crates/pc-engine/src/vector_packages/clip_compound.rs:271` | Fixed | Release Clipping rejects nondefault mask density/feather before editing, preserving unsupported mask semantics; atomic failure regression passes. |
| 112 | `crates/pc-engine/src/vector_packages/path_text/gpu.rs:16` | Rejected | The task explicitly mandates RTX 5090 Vulkan validation. |
| 113 | `crates/pc-engine/src/vector_packages/path_text/gpu.rs:36` | Fixed | Added explicit CPU/GPU pixel-count equality before zip, so an empty/truncated GPU result cannot pass the comparison. |
| 114 | `crates/pc-engine/src/vector_packages/path_commands/tests.rs:60` | Rejected | The fixture is recreated inside each operation loop, so average-axis cases are independent. |
| 115 | `crates/pc-engine/src/vector_packages/path_commands/tests.rs:235` | Rejected | Assigning an Arc fixture does not increment session revision; the freshly captured lock preflight still rejects the target. |
| 116 | `crates/pc-engine/src/vector_packages/path_commands.rs:527` | Rejected | Disjoint field partial moves and closures are valid Rust; engine unit targets compile. |
| 117 | `crates/pc-engine/src/vector_packages/pdf_import/tests.rs:106` | Rejected | This integration task requires actual RTX hardware validation, rather than a portable skip. |
| 118 | `crates/pc-engine/src/vector_packages/pdf_import/tests.rs:91` | Fixed | PDF glyph undo regression now asserts that the actual glyph path, as well as its name, is restored. |
| 119 | `crates/pc-engine/src/vector_packages/path_effects.rs:348` | Rejected | Session.edit owns undo; the execute journal flag records a separate automation journal. These do not create duplicate undo entries. |
| 120 | `crates/pc-engine/src/vector_packages/measure.rs:442` | Rejected | The UI uses an AreaCache keyed by source document and revision; the automation command is a standalone query. |
| 121 | `crates/pc-engine/src/vector_packages/measure.rs:138` | Fixed | Distance gestures now build their detached snap cache lazily only when snapping is enabled and not bypassed; disabled snapping does not inherit area or snap object budgets. |
| 122 | `crates/pc-engine/src/vector_packages/constructive.rs:434` | Fixed | Every scissors piece retains fill rule, inversion and source component operation; focused regression passes. |
| 123 | `crates/pc-engine/src/vector_packages/constructive.rs:697` | Fixed | Every erase piece retains fill rule and inversion; focused regression passes. |
| 124 | `crates/pc-engine/src/vector_packages/isolation.rs:494` | Rejected | The enum has Copy payloads and the borrowed pattern is valid Rust; engine unit targets compile. |
| 125 | `crates/pc-engine/src/vector_packages/isolation.rs:256` | Rejected | DocState.doc is Arc<Document>; dereferencing and cloning the Document is correct. |
| 126 | `crates/pc-engine/src/vector_packages/isolation.rs:262` | Rejected | insert_above handles an absent ID without panic; captured-target validation prevents publishing a stale result. |
| 127 | `crates/pc-engine/src/vector_packages/isolation.rs:256` | Rejected | The bounded detached clone is prepared at insertion/commit, not each pointer frame, and is used for depth validation. |
| 128 | `crates/pc-engine/src/vector_packages/isolation.rs:112` | Rejected | Navigating to a parent expands the current scope; the existing scoped selection remains a valid subset. |
| 129 | `crates/pc-engine/src/vector_packages/pen/gpu.rs:7` | Rejected | The pinned wgpu 30 InstanceDescriptor has new_without_display_handle; engine tests compile. |
| 130 | `crates/pc-engine/src/vector_packages/pen/gpu.rs:13` | Rejected | Actual RTX 5090 validation is explicitly required by the task. |
| 131 | `crates/pc-engine/src/vector_packages/pdf_import.rs:135` | Rejected | The PDF scene producer enforces depth budgets before translation consumes the tree. |
| 132 | `crates/pc-engine/src/vector_packages/pdf_export.rs:69` | Rejected | The production PDF worker uses binary export directly. JSON byte arrays are the existing explicit automation result contract. |
| 133 | `crates/pc-engine/src/vector_packages/path_text.rs:310` | Fixed | Path-text hit testing now skips ineligible foreground paths before choosing the nearest eligible curve; focused regression passes. |
| 134 | `crates/pc-engine/src/vector_packages/node.rs:469` | Rejected | Optional guards preserve the existing immediate automation API; captured UI gestures provide document and revision guards. |
| 135 | `crates/pc-engine/src/vector_packages/node.rs:306` | Rejected | snapshot_shapes clones detached shape edits and does not edit Session, history or journal. |
| 136 | `crates/pc-engine/src/vector_packages/node.rs:177` | Rejected | The downstream insert implementation validates the finite parameter range before applying geometry. |
| 137 | `crates/pc-engine/src/vector_packages/node.rs:269` | Fixed | Node provenance now indexes exact anchors once, canonicalizes signed zero, and preserves unchanged coincident-knot priority; focused regression passes. |
| 138 | `crates/pc-engine/src/vector_packages/pen.rs:77` | Rejected | snapshot_shapes returns one edit per requested valid shape or an error; a successful singleton request cannot be empty. |
| 139 | `crates/pc-engine/src/vector_packages/pen.rs:76` | Rejected | snapshot_shapes validates inherited locks before returning shape edits. |
| 140 | `crates/pc-engine/src/vector_packages/recolor/tests.rs:67` | Rejected | The session document is Arc<Document>, and dereferencing it before Document::clone is correct. |
| 141 | `crates/pc-engine/src/vector_packages/styles/native_acceptance.rs:47` | Rejected | Offsets 46, 92 and 138 select the three peers; a fourth peer would require offset 184 and is absent as intended. |
| 142 | `crates/pc-engine/src/vector_packages/repeat.rs:137` | Rejected | children_mut returns a mutable vector; this use compiles. |
| 143 | `crates/pc-engine/src/vector_packages/snapping.rs:41` | Rejected | View::tolerance validates and returns a scalar; it does not mutate View. Settings remain separately validated by queries. |
| 144 | `crates/pc-engine/src/vector_packages/snapping/tests.rs:239` | Rejected | Settings rejects unknown fields through serde and the engine test target compiles. |
| 145 | `crates/pc-engine/src/vector_packages/recolor.rs:383` | Rejected | Access.write mutates only Prepared.document, a detached clone, before the native session transaction. |
| 146 | `crates/pc-engine/src/vector_packages/recolor.rs:170` | Fixed | Farthest-first seeding now maintains nearest distances incrementally, reducing its worst case from cubic to quadratic while retaining tie ordering. A 1024-group deterministic regression was added. |
| 147 | `crates/pc-engine/src/vector_packages/styles/tests.rs:224` | Fixed | Corrected the test run/paragraph byte lengths to cover its five-byte UTF-8 text exactly. |
| 148 | `crates/pc-engine/src/vector_packages/pencil.rs:329` | Rejected | Prepared.document is used by the production pencil detached preview, and raster Surface tiles use copy-on-write. It is not an unused deep copy of pixel data. |
| 149 | `crates/pc-engine/src/vector_packages/pencil.rs:160` | Fixed | Capture now rejects identical Sculpt endpoints up front; a regression was added. The old prepare path also rejected them, so the claimed successful preview was inaccurate. |
| 150 | `crates/pc-engine/src/vector_packages/pencil.rs:272` | Fixed | prepare_with retains and reuses its one captured shape snapshot for Continue/Sculpt/SculptRange. |
| 151 | `crates/pc-engine/src/vector_packages/svg_import/tests.rs:115` | Rejected | The task explicitly requires RTX 5090 hardware validation. |
| 152 | `crates/pc-engine/src/vector_packages/svg_export.rs:60` | Fixed | Removed the duplicate synchronous post-export scoped-document preflight; the immutable Session cannot change during export. |
| 153 | `crates/pc-engine/src/vector_packages/svg_export.rs:15` | Fixed | Corrected command documentation to show separate scope variants and required layers/layer fields. |
| 154 | `crates/pc-engine/src/vector_packages/svg_export.rs:69` | Rejected | The JSON byte-array result is the existing automation contract; the production SVG save worker consumes binary output directly. |
| 155 | `crates/pc-engine/src/vector_packages/svg_import.rs:58` | Rejected | The detached placement preview needs a candidate document, with shared COW surfaces. Importing/parsing and source preparation happen once rather than per pointer update. |
| 156 | `crates/pc-engine/src/vector_packages/svg_import.rs:156` | Rejected | photocraft_svg::import preflight validates finite DPI within 1..9600 before usvg sees it. |
| 157 | `crates/pc-engine/src/vector_packages/pdf_import/app_gpu.rs:78` | Rejected | Rust parses subtraction after the entire if expression; the alpha branch subtracts b[i]. Engine GPU test targets compile. |
| 158 | `crates/pc-engine/src/vector_packages/pdf_import/app_gpu.rs:106` | Rejected | Placement translates the imported scene by the pointer delta; it does not resize the artwork to the drag rectangle. The finding assumes incorrect bounds. |
| 159 | `crates/pc-engine/src/vector_packages/trace_centerline.rs:125` | Rejected | ProfilePoint left/right are f64. The f32 width cast is unrelated; engine tests compile. |
| 160 | `crates/pc-engine/src/vector_packages/trace_cutout.rs:96` | Rejected | render_shape uses CompiledShape bounds clipping and sparse tile allocation; a document clip is not a full-canvas bitmap allocation. |
| 161 | `crates/pc-engine/src/vector_packages/trace_cleanup/transaction/tests.rs:29` | Rejected | Arc<Document> is immutable and edits use COW. The retained Arc remains the old value after publication. |
| 162 | `crates/pc-engine/src/vector_packages/styles/text.rs:116` | Rejected | Surface is an unbounded sparse tile map; Surface::new correctly starts with no allocated tiles, and writes allocate them. |
| 163 | `crates/pc-engine/src/vector_packages/styles/text.rs:120` | Rejected | Surface uses absolute document coordinates for sparse tiles, including rasterize_warped results; no local-buffer subtraction is appropriate. |
| 164 | `crates/pc-engine/src/vector_packages/trace_centerline/gpu.rs:10` | Rejected | The pinned wgpu 30 API contains new_without_display_handle; engine unit targets compile. |
| 165 | `crates/pc-engine/src/vector_packages/trace_centerline/gpu.rs:16` | Rejected | The task explicitly requires real RTX 5090 Vulkan validation. |
| 166 | `crates/pc-engine/src/vector_packages/trace_cleanup/transaction.rs:81` | Rejected | vector_api::validate_path runs before this coordinate budget check and rejects NaN/Inf. |
| 167 | `crates/pc-engine/src/vector_packages/trace_cutout/gpu.rs:45` | Rejected | Rust value and type/module namespaces are separate; the local cpu buffer does not shadow the cpu module in cpu::legacy_projector. |
| 168 | `crates/pc-engine/src/vector_packages/styles.rs:822` | Rejected | Sparse Surface::rgba returns transparent for absent/outside tiles; it does not index a finite bitmap. |
| 169 | `crates/pc-engine/src/vector_packages/styles.rs:667` | Rejected | The compiled outline plan budgets the actual on-path raster cache as well as explicit appearance caches; excluding it would undercount allocation. |
| 170 | `crates/pc-engine/src/vector_packages/styles.rs:314` | Fixed | Moved the raw FX byte limit before serde serialization; a regression requires that exact budget error for oversized input. |
| 171 | `crates/pc-engine/src/vector_packages/symbols.rs:441` | Rejected | Layer::duplicate assigns fresh LayerIds recursively, with an existing document regression covering this behavior. |
| 172 | `crates/pc-engine/src/vector_packages/symbols.rs:205` | Fixed | Symbol placement now checks only the insertion neighbor and its inherited locks, preserving unmodified locked descendants. A regression was added. |
| 173 | `crates/pc-format/examples/vector_arch_fixture.rs:24` | Rejected | Cargo run inherits the invoking shell working directory; it does not change to the package directory as alleged. |
| 174 | `crates/pc-engine/src/vector_packages/transform_tools/conversion.rs:75` | Rejected | Identity cluster transforms intentionally coincide. Each glyph reads its own g.style color, faux-bold and face; glyph styles are not taken from cluster zero. |
| 175 | `crates/pc-engine/src/vector_packages/trace_output.rs:144` | Rejected | Session.start invokes the command handler synchronously; the handler takes the mailbox before spawning the latest-key worker. Cleanup removes only an unconsumed validation-failure mailbox. |
| 176 | `crates/pc-engine/src/vector_packages/trace_output.rs:173` | Rejected | Sparse Surface::read_region supports arbitrary document rectangles and returns transparent pixels for absent tiles. |
| 177 | `crates/pc-engine/src/vector_packages/symbols/expansion.rs:29` | Rejected | Transforms are absolute and applied once by prepare_expansion. The outer instance envelope is in its containing coordinate space, while master content receives the instance transform. |
| 178 | `crates/pc-engine/src/vector_packages/symbols/expansion.rs:37` | Fixed | Replaced Group struct update with explicit expanded/artboard fields, avoiding an unnecessary clone of original children. |
| 179 | `crates/pc-engine/src/vector_toolbar_layout.rs:295` | Fixed | Vector accelerator overrides now replace conflicting chord owners only, preserving custom nonconflicting chords for the same target; a regression was added. |
| 180 | `crates/pc-engine/src/vector_packages/transform_tools.rs:512` | Rejected | metadata::paint explicitly errors when Transform Patterns is enabled for a non-affine Distort map; it does not silently skip the request. |
| 181 | `crates/pc-engine/src/vector_packages/transform_tools.rs:700` | Fixed | Reshape replay now matches retained root LayerIds before falling back to positional correspondence for copies/new targets; reordered-selection regression was added. |
| 182 | `crates/pc-format/src/migrate.rs:33` | Rejected | migrate_with returns the original file version as its existing contract and tests require; the manifest is upgraded in place. Production callers discard the return value. |
| 183 | `crates/pc-engine/src/vector_packages/width/gpu.rs:5` | Rejected | The pinned wgpu 30 API has new_without_display_handle; unit targets compile. |
| 184 | `crates/pc-engine/src/vector_packages/width.rs:34` | Rejected | snapshot_shapes returns an error for any non-shape target and exactly one edit for a valid singleton; it never filters into an empty success. |
| 185 | `crates/pc-engine/src/vector_packages/transform_tools/contours.rs:16` | Rejected | Separate Combine components intentionally preserve each glyph outline winding/holes before unioning overlapping glyphs; resetting per PaintedOutline is required. |
| 186 | `crates/pc-engine/src/vector_packages/pen/gesture.rs:278` | Rejected | Extension at the first endpoint traverses the new segment in reverse; closing shapes the arriving handle. Existing endpoint/closing replay tests enforce these directions. |
| 187 | `crates/pc-engine/src/vector_packages/pen/gesture.rs:110` | Fixed | Pen begin now checks the aggregate knot budget before adding a detached subpath; a max-budget regression was added. |
| 188 | `crates/pc-engine/src/vector_packages/pen/gesture.rs:298` | Fixed | Zero-length line release now increments generation after truncating the draft; a generation regression was added. |
| 189 | `crates/pc-engine/src/vector_toolbar_layout/tests.rs:36` | Rejected | Vec::contains permits comparing a shorter borrowed str with stored static str values; engine test targets compile. |
| 190 | `crates/pc-engine/src/vector_packages/transform_tools/metadata.rs:7` | Rejected | Transform preflight rejects singular/near-zero scale affine maps before metadata::similarity is called. |
| 191 | `crates/pc-engine/src/vector_packages/transform_tools/metadata.rs:192` | Rejected | metadata::paint is called only on detached candidate layers; any error discards the candidate before Session publication. No native fill is partially mutated. |
| 192 | `crates/pc-engine/src/vector_packages/transform_tools/metadata.rs:159` | Rejected | Effects are scaled on detached candidates, and prepare propagates errors before publication. The original document remains unchanged. |
| 193 | `crates/pc-engine/src/vector_packages/selection.rs:328` | Rejected | Handoff 32 explicitly permits Group Selection to descend sibling scope for leaf hits, including root sibling groups, before selecting an ancestor. |
| 194 | `crates/pc-engine/src/vector_packages/selection.rs:464` | Rejected | No fill is a valid appearance attribute shared by unfilled objects; Same Fill/Stroke deliberately compares the topmost optional paints. |
| 195 | `crates/pc-engine/src/vector_packages/selection.rs:497` | Rejected | Wand compares absent paints as a shared appearance attribute, consistently with Same Fill/Stroke; this is intentional matching semantics. |
| 196 | `crates/pc-engine/src/vector_packages/selection.rs:766` | Rejected | Named-artboard queries use geometric overlap within the authoritative current selection/isolation scope; they must not select outside that scope. |
| 197 | `crates/pc-engine/src/vectorize_cmds.rs:48` | Rejected | trace_output::check validates the captured document, revision and source after deserialization and before publication. |
| 198 | `crates/pc-engine/src/vector_packages/symbols/render.rs:182` | Rejected | Native Group has no hierarchical affine field; expansion bakes absolute child transforms once. |
| 199 | `crates/pc-engine/src/vector_packages/symbols/render.rs:175` | Fixed | Fixed: removed the redundant full document clone before computing expanded symbol bounds; compositing still owns its necessary detached document. |
| 200 | `crates/pc-io/src/vector_fallback.rs:321` | Rejected | Surface is sparse and size-free by design; write_region allocates its tiles without a canvas-sized constructor. |
| 201 | `crates/pc-io/src/vector_fallback.rs:207` | Fixed | Fixed: PSD projection now includes vector_mask in the compositing-envelope equality check; regression rejects a provider that discards it. |
| 202 | `crates/pc-engine/src/vector_packages/symbols/validation.rs:121` | Fixed | Fixed: symbol validation charges aggregate text-run variations before scanning them; aggregate-budget regression passed. |
| 203 | `crates/pc-engine/src/vector_snap.rs:305` | Rejected | Spacing snap targets are object bounds; work/saved paths are node and curve targets rather than drawable objects. |
| 204 | `crates/pc-engine/src/vector_snap.rs:420` | Fixed | Fixed: curve nearest-point queries first cull cached control bounds inflated by tolerance; callback-count regression passed. |
| 205 | `crates/pc-pathops/src/geom/hit.rs:58` | Rejected | geom::PathData has no fill-rule field; this primitive intentionally uses NonZero. Production selection uses the native rule-aware hit provider. |
| 206 | `crates/pc-pathops/src/geom/hit.rs:49` | Rejected | The helper is not the production selection hot path, which caches geometry and bounds. No demonstrated performance regression from this small detached primitive. |
| 207 | `crates/pc-format/src/convert.rs:647` | Fixed | Fixed: has_instances now iteratively validates every branch against MAX_GROUP_DEPTH instead of recursing or returning early on one symbol. |
| 208 | `crates/pc-pathops/src/fit.rs:87` | Rejected | fit_single_from is private; all callers provide nonempty matched sample/parameter vectors, including the fixed-size removal fitter. |
| 209 | `crates/pc-pathops/src/fit.rs:201` | Rejected | Private sample callers use positive constants 12, 16 and 40; zero is not an external input. |
| 210 | `crates/pc-pathops/src/geom/shapes.rs:135` | Rejected | The legacy unused geometry arc helper is not the live Arc implementation; design_shapes generates and validates actual start/end/sweep parameters. |
| 211 | `crates/pc-pathops/src/geom/shapes.rs:198` | Fixed | The public polar-grid helper emits no spokes for zero radial count. Added zero-spoke regression. |
| 212 | `crates/pc-pathops/src/geom/shapes.rs:176` | Rejected | Production shape generation uses bounded validated design-shape counts rather than these legacy low-level generators. |
| 213 | `crates/pc-pathops/src/geom/bez.rs:35` | Rejected | segments requires normalized MoveTo-based subpaths; native/SVG normalization injects MoveTo after ClosePath before further segments. |
| 214 | `crates/pc-pdf/src/export.rs:558` | Rejected | Mask density relaxes masked-out pixels toward visibility; it is not a uniform multiplication of every mask sample. |
| 215 | `crates/pc-pdf/src/export.rs:567` | Rejected | Layer fill_opacity applies to all native content including strokes, as in the shared compositor; appearance fill opacity is a separate field. |
| 216 | `crates/pc-pdf/src/export.rs:272` | Fixed | Fixed: selected artboards retain their full rectangle even with no children; exported page/background regression passed with Poppler. |
| 217 | `crates/pc-pdf/src/export.rs:509` | Fixed | Fixed: per-layer PDF raster fallbacks use composite bounds intersected with page bounds instead of always rasterizing the full page. |
| 218 | `crates/pc-pathops/src/geom/path.rs:137` | Rejected | SubPath::segment requires a valid segment index; external adapters check segment_count and empty paths never enter segment loops. |
| 219 | `crates/pc-pathops/src/geom/path.rs:84` | Rejected | A zero-length incoming smooth handle permits a nonzero outgoing tangent; collapsing the opposite handle would change the curve unnecessarily. |
| 220 | `crates/pc-pathops/src/boolean.rs:691` | Rejected | Production adapters use fallible try_ APIs and propagate errors. The defaulting convenience wrapper is retained upstream compatibility. |
| 221 | `crates/pc-pathops/src/boolean.rs:655` | Rejected | Refit precision and sweep-scale sliver tolerance serve distinct documented purposes; thin legitimate contours must survive the refit setting. |
| 222 | `crates/pc-pathops/src/boolean.rs:361` | Rejected | The native adapter propagates fallback errors rather than returning an empty path. The bounded segment cap is an intentional resource limit. |
| 223 | `crates/pc-pathops/src/boolean.rs:536` | Fixed | Fixed: tidy tries each complete smooth run and recursively splits failed fits, removing growing-prefix refitting; kernel acceptance covers geometry/node quality. |
| 224 | `crates/pc-io/src/psd_export.rs:531` | Fixed | Fixed: PSD emit computes one bounded fallback cache per layer and shares it between metadata and pixel-record serialization. |
| 225 | `crates/pc-pdf/src/import/geometry.rs:60` | Rejected | The last PDF knot has a corner outgoing handle; its incoming handle is the second control of the preceding cubic. Explicit closure transfers that handle correctly. |
| 226 | `crates/pc-pathops/src/adapter.rs:327` | Fixed | Fixed: every split piece retains the source fill rule, inversion and component operation. |
| 227 | `crates/pc-pathops/src/adapter.rs:178` | Rejected | MAX_SEGMENTS is the open-edge planar-arrangement budget; closed filled regions use the general fallible sweep with separate limits, as documented. |
| 228 | `crates/pc-svg/src/import.rs:57` | Rejected | Image dimensions, decoded-byte footprint and cache budgets are checked before decoding/allocation; compressed input has its own limit. |
| 229 | `crates/pc-svg/src/import.rs:62` | Rejected | SVG preflight already emits embedded-format warnings for unsupported data-image MIME types before usvg drops them. |
| 230 | `crates/pc-pdf/src/export/validate.rs:123` | Rejected | The JSON walker consumes serialization of typed bounded native effects/vector data, not arbitrary recursive external Value data. |
| 231 | `crates/pc-pdf/src/export/validate.rs:161` | Rejected | appearance::validate enforces the 128-item budget before item iteration; the later export budget is not the first guard. |
| 232 | `crates/pc-pdf/src/export/validate.rs:89` | Rejected | Document Size fields are u32, so NaN dimensions cannot exist. |
| 233 | `crates/pc-pdf/src/import/paint.rs:64` | Fixed | Fixed: normalized PDF dashes/offset use f64 division followed by finite f32 validation. Tiny-width regression passed; line_width is f32, contrary to part of the finding. |
| 234 | `crates/pc-pdf/src/import/paint.rs:138` | Fixed | Fixed: mesh and other shading outputs share the same finite-RGBA check; nonfinite-colour regression passed. |
| 235 | `crates/pc-pdf/src/import/paint.rs:200` | Rejected | bytes is the remaining decoded-byte budget, not source file size. Each expanded triangle is charged 4096 bytes, so less room cannot admit a triangle. |
| 236 | `crates/pc-pdf/src/export/paint.rs:107` | Rejected | Native Paint.at subtracts the half-pixel offset internally; the sampling call deliberately restores the desired gradient coordinate. |
| 237 | `crates/pc-pdf/src/export/paint.rs:127` | Rejected | PDF linear-gradient endpoints include the native half-pixel sampling convention; existing independent pixel oracles cover it. |
| 238 | `crates/pc-pathops/src/offset.rs:106` | Fixed | Fixed: closed inset classification excludes unrelated open paths; negative-offset/open-band regression passed. |
| 239 | `crates/pc-pathops/src/offset.rs:111` | Fixed | Fixed with 238: negative offsets preserve and union open stroke bands after evaluating the closed inset independently. |
| 240 | `crates/pc-svg/src/import/css.rs:114` | Rejected | Iterator.take_while consumes the terminating bracket; advancing again would discard the next token. |
| 241 | `crates/pc-svg/src/import/css.rs:279` | Rejected | roxmltree Node::ancestors includes the node itself, so its own attributes are honored. |
| 242 | `crates/pc-svg/src/import/paint.rs:17` | Rejected | usvg folds objectBoundingBox gradient coordinates into the normalized transform; applying bounds again would double-transform paint. |
| 243 | `crates/pc-svg/src/import/paint.rs:107` | Rejected | usvg radial radii are NonZeroPositiveF32; zero-radius gradients normalize to solid paint before this branch. |
| 244 | `crates/pc-svg/src/import/paint.rs:85` | Rejected | usvg stroke widths are NonZeroPositiveF32; zero-width strokes normalize away before conversion. |
| 245 | `crates/pc-svg/src/import/geometry.rs:36` | Rejected | tiny-skia-path injects MoveTo after ClosePath; the production normalized path iterator cannot present the claimed sequence. |
| 246 | `crates/pc-svg/src/export.rs:478` | Rejected | SVG layer fill_opacity mirrors the shared native compositor's all-content opacity, distinct from individual appearance fill opacity. |
| 247 | `crates/pc-svg/src/export.rs:372` | Rejected | Appearance-item blending is isolated within the native layer's appearance accumulation. Rasterizing that layer preserves its intended relation to the backdrop; layer-level blend modes trigger scoped flattening. |
| 248 | `crates/pc-svg/src/import/tree.rs:27` | Rejected | Native units have Millimeters rather than Centimeters; SVG physical dimensions are already converted to pixels using DPI, so displaying cm as mm preserves dimensions. |
| 249 | `crates/pc-svg/src/import/tree.rs:29` | Rejected | Native units have Points rather than Picas; physical SVG dimensions are resolved at DPI before assigning the display unit. |
| 250 | `crates/pc-svg/src/import/tree.rs:127` | Rejected | The importer reserves nesting room for wrapper groups and the root; accepting the raw configured depth without that allowance could exceed native MAX_GROUP_DEPTH. |
| 251 | `crates/pc-svg/src/import/tree.rs:120` | Fixed | Fixed: SVG labels are indexed once by ID and shared by layer naming/warning remapping, eliminating repeated descendant scans. |
| 252 | `crates/pc-svg/src/import/tree.rs:225` | Rejected | The two paths are separately materialized normalized geometries, and both count toward the expanded-work segment budget. |
| 253 | `crates/pc-pdf/src/export/text.rs:127` | Rejected | draw_solid multiplies opacity by Color.alpha; passing opacity 1 does not make decorations opaque. |
| 254 | `crates/pc-pdf/src/export/text.rs:74` | Fixed | PDF outlines group glyphs once by native color while preserving first-color compositing order. |
| 255 | `crates/pc-pdf/src/export/text.rs:116` | Rejected | Pinned krilla draw_path returns unit; there is no ignored Result. Geometry/budget checks occur before submission. |
| 256 | `crates/pc-pdf/src/export/text.rs:54` | Rejected | Pinned krilla draw_glyphs returns unit; validation errors are handled by the exporter before drawing. |
| 257 | `crates/pc-pdf/src/import/device.rs:469` | Fixed | PDF decoded image dimensions and pixel count are checked before transformed indexing; alpha callback reads are bounded. |
| 258 | `crates/pc-pdf/src/import/device.rs:183` | Rejected | Outer soft masks remain on parent drawing frames/layers; applying them again to each popped child group would multiply the mask twice. |
| 259 | `crates/pc-pathops/src/edit.rs:259` | Rejected | segment_is_line requires both connecting handles to equal their anchors, so unchanged handles remain straight after deleting the middle anchor. |
| 260 | `crates/pc-pathops/src/pathfinder.rs:363` | Rejected | Pathfinder Outline is a filled-area operation; open filled paths are deliberately implicitly closed, consistent with SVG/native fill semantics. |
| 261 | `crates/pc-trace/src/centerline.rs:75` | Rejected | Separate actual scans each consume work, and coarse cancellation polling is deliberately conservative. The budget bounds work rather than counting unique pixels once. |
| 262 | `crates/pc-svg/src/export/text.rs:123` | Rejected | raster_reason catches warped text before Writer::text, so warped decorations already use the shared raster fallback. |
| 263 | `crates/pc-svg/src/import/preflight.rs:19` | Fixed | Fixed: compressed input bytes and expanded gzip bytes now have independent limits; regression covers a valid small gzip and exact expanded limit. |
| 264 | `crates/pc-svg/src/import/preflight.rs:120` | Fixed | Fixed: only polygons add ClosePath to point-list reference-expansion weights; polylines retain their actual MoveTo/LineTo count. |
| 265 | `crates/pc-svg/src/import/preflight.rs:136` | Fixed | Fixed: the preflight base64 upper bound subtracts trailing padding while retaining a bounded preallocation check. |
| 266 | `crates/pc-svg/src/import/text.rs:199` | Rejected | roxmltree Node::range spans the entire element including descendants and closing tag, not only the opening tag. |
| 267 | `crates/pc-svg/src/import/text.rs:150` | Rejected | Node::ancestors includes the node itself; the element's own font-size is evaluated first. |
| 268 | `crates/pc-svg/src/import/text.rs:215` | Rejected | Node::ancestors includes the text element itself, so its xml:space declaration is respected. |
| 269 | `crates/pc-svg/src/import/text.rs:81` | Rejected | Quote tracking closes only the matching quote and opens a quote only when no quote is active; the other quote inside an attribute is harmless. |
| 270 | `crates/pc-svg/src/import/text.rs:257` | Rejected | Percentage editable text positions are explicitly warned as an approximation; the documented importer does not claim exact complex SVG text positioning. |
| 271 | `crates/pc-trace/src/cleanup/mod.rs:178` | Rejected | working_params only substitutes detected palette when params.palette is empty; preprocessing's palette honors explicit user colours. |
| 272 | `crates/pc-trace/src/centerline/mask.rs:13` | Rejected | Public trace_with_cancel validates Image length/dimensions before calling private mask::foreground. |
| 273 | `crates/pc-trace/src/centerline/mask.rs:14` | Rejected | Control.work is a conservative cancellation/work budget, not user-visible progress. Charging final chunks at 1024 is intentional. |
| 274 | `crates/pc-pathops/src/planar.rs:327` | Rejected | Production shape_builder calls fallible try_shape_builder and propagates errors. Legacy defaulting wrapper is not used to publish empty edits. |
| 275 | `crates/pc-pdf/src/import/preflight.rs:460` | Rejected | Preflight charges intermediate codec work and the normalized stream separately as a conservative decoded-resource bound; it is not a unique-byte counter. |
| 276 | `crates/pc-pdf/src/import/preflight.rs:454` | Rejected | Both declared and decoded JPEG dimensions are independently budgeted before interpretation to bound forged headers and decoded memory; this is deliberate conservative resource admission. |
| 277 | `crates/pc-svg/src/export/paint.rs:127` | Rejected | Paint.at internally removes the half-pixel offset, so the two side samples straddle the hard stop as intended. |
| 278 | `crates/pc-pdf/src/import/pages.rs:161` | Rejected | ImportedScene size is pixels and resolution_dpi determines conversion to physical display units; Points is a display preference, not a reinterpretation of pixel dimensions. |
| 279 | `crates/pc-text/src/view.rs:192` | Fixed | Scaled text projects glyph/decoration lines once per visible layout, retains a bounded color cache, and reuses it across tile rasterization. |
| 280 | `crates/pc-text/src/view.rs:190` | Rejected | Scaled rendering deliberately follows the existing native renderer's union by style colour, preserving its pixel parity and translucent coverage semantics. |
| 281 | `crates/pc-text/src/frame_compose.rs:297` | Rejected | Horizontal overset reports text that is drawn but exceeds the frame width; vertical exhaustion reports unplaced text. Both are legitimate overflow indicators. |
| 282 | `crates/pc-ui-egui/src/canvas.rs:2757` | Fixed | The press-owned brush resize flag is cleared before every early shared/vector/symbol routing return. |
| 283 | `crates/pc-ui-egui/src/dock.rs:317` | Fixed | Fixed: Symbols is now a real Layers home tab; selecting it activates Symbols rather than falling back to Layers. Headless regression passed. |
| 284 | `crates/pc-text/src/frame_compose/breaker.rs:601` | Fixed | Fixed: visible Knuth break ranges trim forced-break glyphs together with trailing spaces; next still consumes the break glyph. |
| 285 | `crates/pc-text/src/frame_compose/breaker.rs:313` | Fixed | Fixed: item construction retains the previous non-soft-hyphen glyph rather than repeatedly scanning backwards. |
| 286 | `crates/pc-text/src/frame_compose/breaker.rs:415` | Fixed | Fixed: limits beyond the exact bounded DP state count use the existing greedy breaker, which tracks the full consecutive-hyphen count. |
| 287 | `crates/pc-trace/src/cleanup/geometry.rs:227` | Rejected | Polygon recognition only returns candidates with fewer knots than the source; the subtraction is therefore nonnegative. |
| 288 | `crates/pc-trace/src/cleanup/geometry.rs:245` | Rejected | symmetrize preserves subpath count/order; zip pairs every original contour with its corresponding candidate. |
| 289 | `crates/pc-trace/src/cutout/topology.rs:17` | Rejected | BoundaryGraph construction guarantees nonempty endpoint chains before this private topology check. |
| 290 | `crates/pc-trace/src/cutout/topology.rs:9` | Rejected | Fitted geometry is created exactly once per graph segment and count/order are preserved by the fitter; no external mismatched list is accepted. |
| 291 | `crates/pc-trace/src/cutout/merge.rs:82` | Rejected | Cancelled LabelMap processing is detached worker state and discarded by the caller; partially remapped state is never published or reused. |
| 292 | `crates/pc-text/src/on_path.rs:147` | Rejected | Overflowing path text is clipped from the start of its available span; negative alignment gaps would place glyphs before that span and violate the overflow policy. |
| 293 | `crates/pc-text/src/on_path.rs:348` | Rejected | Decoration rectangles already include baseline shift from shaping; the cluster transform applies the same shift as glyphs, while center+shift only identifies the line. |
| 294 | `crates/pc-svg/src/export/validate.rs:88` | Rejected | Document size is u32; nonfinite width/height are impossible in this model. |
| 295 | `crates/pc-trace/src/cleanup/palette_trace.rs:167` | Fixed | Fixed: Pixel fitter crossing fallback regenerates exact lattice contours from the binary mask rather than fractional scalar-field contours. |
| 296 | `crates/pc-trace/src/cleanup/palette_trace.rs:96` | Fixed | Fixed with 295: Pixel mode skips scalar/working f64 field allocation entirely, retaining its exact binary mask path. |
| 297 | `crates/pc-ui-egui/src/lib.rs:1843` | Rejected | vector_packages.rs exists in this branch and the complete UI lib test target compiles. |
| 298 | `crates/pc-trace/src/cleanup/preprocess.rs:94` | Rejected | Options::validate requires finite filter_sigma in 1..=64 before private bilateral filtering. |
| 299 | `crates/pc-trace/src/cleanup/preprocess.rs:29` | Rejected | Image::validate rejects zero dimensions before private prepare reaches chunks_exact_mut. |
| 300 | `crates/pc-trace/src/centerline/distance.rs:91` | Rejected | The distance field includes outside boundary sites; under the pixel budget, distance is bounded by the shorter axis, whose squared half-extent fits u32 even for wide thin images. |
| 301 | `crates/pc-trace/src/cleanup/topology.rs:52` | Rejected | This checker deliberately detects proper transverse crossings, not coincident/retraced contacts. The latter do not by themselves imply an invalid filled contour. |
| 302 | `crates/pc-trace/src/cleanup/topology.rs:57` | Rejected | Endpoint contacts are excluded from the proper-crossing checker by contract; arbitrary contact detection would reject valid touching contours. |
| 303 | `crates/pc-trace/src/centerline/skeleton.rs:80` | Rejected | Thinning runs in a cancellable worker with a strict total work budget; the finding incorrectly places this operation on the UI thread. |
| 304 | `crates/pc-ui-egui/src/panels.rs:69` | Rejected | Vector persona activation restricts the tool to executable vector/shared IDs; a raster-only active tool is not a valid persona state. |
| 305 | `crates/pc-ui-egui/src/vector_packages/blend/app_hooks.rs:35` | Rejected | ToolEvent coordinates have already been converted to document coordinates by the app canvas. zoom is used only for screen-size hit tolerance. |
| 306 | `crates/pc-ui-egui/src/vector_packages/affinity/app_gpu.rs:17` | Rejected | This is the explicitly deferred RTX 5090 hardware acceptance gate requested by the owner, not a general portable test. |
| 307 | `crates/pc-ui-egui/src/toolsets_ui.rs:54` | Rejected | ToolbarCustomization::active resolves a valid set for the current persona, and sets_for returns that persona's catalog. |
| 308 | `crates/pc-ui-egui/src/toolsets_ui.rs:182` | Fixed | Toolbar rename state records its active set owner and clears when that owner changes. |
| 309 | `crates/pc-trace/src/centerline/graph.rs:100` | Fixed | Fixed: isolated-point stroke budget is checked only when adding a run; non-run skeleton pixels no longer reject an exactly-full result. |
| 310 | `crates/pc-trace/src/cutout/fit.rs:29` | Rejected | Private pixel receives nonempty graph chains guaranteed by BoundaryGraph construction. |
| 311 | `crates/pc-trace/src/cutout/fit.rs:80` | Rejected | Ring chains repeat their start and contain at least two points; empty/singleton rings cannot be constructed by the graph builder. |
| 312 | `crates/pc-trace/src/cutout/fit.rs:78` | Fixed | Pixel fitting bypasses eager point collection; large-ring fallback reuses converted lattice points. |
| 313 | `crates/pc-ui-egui/src/direct_select.rs:584` | Rejected | Legacy path commands now accept captured targets/document/revision through the integrated command adapters; target identity is not lost. |
| 314 | `crates/pc-ui-egui/src/vector_packages/blend.rs:170` | Rejected | Detached blend previews are bounded and lazily displayed; recomputing changed geometry on pointer motion is required. No idle repaint rebuild occurs. |
| 315 | `crates/pc-trace/src/cleanup/recognize.rs:137` | Rejected | Recognition is conservative and may decline chord-closed arcs that exceed its radial fit criterion; output retains the original editable curve and no data is lost. |
| 316 | `crates/pc-ui-egui/src/vector_packages/blob_eraser/app_gpu.rs:151` | Rejected | The performance statistic is an explicitly selected sorted percentile/median convention; the finding provides no failing timing or threshold regression. |
| 317 | `crates/pc-ui-egui/src/vector_packages/clip_compound.rs:55` | Fixed | Fixed: Draw Inside uses the single selected object when no explicit isolation insertion target is supplied, instead of an unrelated active layer. |
| 318 | `crates/pc-ui-egui/src/vector_packages/clip_compound.rs:65` | Fixed | Fixed: preview_draw updates the retained request in place, eliminating a clone of the old path that is immediately replaced. |
| 319 | `crates/pc-ui-egui/src/vector_packages/clip_compound.rs:296` | Rejected | The integrated code caches preview_target and only prepares when the hover target changes or preview is absent. |
| 320 | `crates/pc-ui-egui/src/vector_packages/blob_eraser.rs:93` | Rejected | Stabilizer::push validates initial sample coordinates and pressure before it is stored or passed to prepare. |
| 321 | `crates/pc-ui-egui/src/vector_packages/blob_eraser.rs:132` | Rejected | The owned detached request snapshots a bounded stroke for transactional preview; geometry must be regenerated when samples change. Performance is covered by the package's stroke gates. |
| 322 | `crates/pc-ui-egui/src/vector_packages/blob_eraser.rs:58` | Fixed | Fixed: Blob/Eraser consumes Escape only while its gesture is active, allowing other UI elements to receive it otherwise. |
| 323 | `crates/pc-ui-egui/src/vector_packages/constructive/tests.rs:33` | Rejected | Pinned egui CentralPanel::show accepts &mut Ui in this version; the complete UI lib target compiles with this call. |
| 324 | `crates/pc-ui-egui/src/vector_packages/brushes.rs:87` | Rejected | Interactive brush previews fit bounded changed samples and must reflect the current stroke; package performance gates evaluate the resulting latency rather than assuming a freeze. |
| 325 | `crates/pc-ui-egui/src/gpu_canvas.rs:602` | Fixed | Empty-damage GPU refresh returns before invalidating or copying the scaled document source. |
| 326 | `crates/pc-ui-egui/src/gpu_canvas.rs:2057` | Rejected | An unsuccessful projection is discarded instead of presenting stale pixels; valid document projections retain/reinsert the frame. The alleged allocation issue is not a correctness defect. |
| 327 | `crates/pc-ui-egui/src/gpu_canvas.rs:2089` | Rejected | Uniform/bind-group construction is bounded to dirty rendered tiles; the package explicitly measures tile/frame resource and latency budgets. |
| 328 | `crates/pc-ui-egui/src/vector_packages/anchor_tools.rs:746` | Rejected | Escape globally cancels active gestures in the host lifecycle, including text-focused options, as specified by shared-tool cancellation. |
| 329 | `crates/pc-ui-egui/src/vector_packages/anchor_tools.rs:667` | Fixed | Work path targets are deduplicated before vector-mask routing. |
| 330 | `crates/pc-ui-egui/src/vector_packages/anchor_tools.rs:380` | Fixed | Curvature capture resets on a node hit in a different target or subpath. |
| 331 | `crates/pc-ui-egui/src/vector_packages/anchor_tools.rs:633` | Rejected | Rubber-band fitting is bounded by MAX_POINTS and paints a live candidate that changes with hover. It is not document mutation and no performance failure is demonstrated. |
| 332 | `crates/pc-ui-egui/src/vector_packages/corners/tests.rs:29` | Rejected | Pinned egui CentralPanel::show accepts Ui in this version; the complete UI lib target compiles. |
| 333 | `crates/pc-ui-egui/src/vector_packages/drawing/app_gpu.rs:87` | Fixed | The filled-star GPU probe now requires opaque black rather than accepting transparent black. |
| 334 | `crates/pc-ui-egui/src/vector_packages/fill_stroke_proxy/tests.rs:268` | Rejected | docs/design is part of the repository and exists in a fresh checkout; the test writes into that committed directory. |
| 335 | `crates/pc-ui-egui/src/vector_packages/appearance.rs:104` | Rejected | Gesture::begin uses snapshot_shapes, which rejects empty selections before indexing the first edit. |
| 336 | `crates/pc-ui-egui/src/vector_packages/appearance.rs:524` | Rejected | Focus loss cancels an uncommitted detached gesture by the owner lifecycle contract; committing on focus loss would violate cancellation semantics. |
| 337 | `crates/pc-ui-egui/src/vector_packages/appearance.rs:381` | Rejected | Zero-stop gradients have defined native ramp behaviour and are validated as representable paint; removing the final stop does not produce invalid data. |
| 338 | `crates/pc-ui-egui/src/vector_packages/appearance.rs:104` | Fixed | Appearance item selection is clamped to the captured layer stack before canvas handle construction. |
| 339 | `crates/pc-ui-egui/src/vector_packages/corners.rs:412` | Rejected | CornerGesture::begin rejects empty/non-shape targets through snapshot_shapes before an edit can be indexed. |
| 340 | `crates/pc-ui-egui/src/vector_packages/constructive.rs:512` | Rejected | Shape Builder overlay is bounded to selected arrangement regions and repaints through egui; performance is a gate, not proof of a defect from allocation alone. |
| 341 | `crates/pc-ui-egui/src/vector_packages/constructive.rs:130` | Rejected | Escape cancels the transient contour delta by design; retaining an uncommitted offset as applied state would contradict cancellation. |
| 342 | `crates/pc-ui-egui/src/vector_packages/eyedropper.rs:402` | Fixed | Eyedropper previews are retained Arc<Document> snapshots in the egui bridge. |
| 343 | `crates/pc-ui-egui/src/vector_packages/eyedropper.rs:235` | Rejected | StylePicker movement beyond the drag threshold deliberately changes a click into marquee application, matching the documented tool gesture. |
| 344 | `crates/pc-ui-egui/src/vector_packages/eyedropper.rs:111` | Rejected | StylePicker loads the sampled style and applies it to existing selection in the same captured transaction; subsequent gestures apply the loaded style to hit targets. |
| 345 | `crates/pc-ui-egui/src/vector_packages/distort_tools/tests.rs:196` | Rejected | Integrated pointer-event harness tests compile and use the specified event sequencing; verify through the required package test run rather than assuming a missing-release failure. |
| 346 | `crates/pc-ui-egui/src/vector_packages/drawing.rs:360` | Fixed | Convert to Curves publishes valid numeric shape edits before conversion. |
| 347 | `crates/pc-ui-egui/src/vector_packages/library_vectorize.rs:98` | Rejected | JobEvent carries only terminal JobOutcome Done/Failed/Cancelled; progress uses a separate channel, so no progress event can consume opening state. |
| 348 | `crates/pc-ui-egui/src/vector_packages/live_paint/tests.rs:36` | Rejected | Pinned egui CentralPanel::show accepts Ui; the complete UI lib target compiles. |
| 349 | `crates/pc-ui-egui/src/vector_packages/live_paint/tests.rs:160` | Fixed | The integrated Live Paint regression unconditionally checks an unsupported target error and unchanged document/journal. |
| 350 | `crates/pc-ui-egui/src/vector_packages/path_commands/app_gpu.rs:164` | Fixed | GPU parity compares buffer lengths before per-pixel zipped comparisons. |
| 351 | `crates/pc-ui-egui/src/vector_packages/envelope.rs:108` | Fixed | Envelope Apply retains its pending gesture until publication succeeds. |
| 352 | `crates/pc-ui-egui/src/vector_packages/envelope.rs:65` | Fixed | Envelope provider invalidation clears stale previews and retained captures. |
| 353 | `crates/pc-ui-egui/src/vector_packages/envelope.rs:159` | Rejected | The context panel's detached editable spec is bounded by mesh/control budgets and shown only while the tool/panel is active; no unbounded hot-path regression is established. |
| 354 | `crates/pc-ui-egui/src/vector_packages/envelope.rs:190` | Rejected | Controls are non-publishing until a valid captured target is ready; unselected controls are harmless option drafts, not ignored document commands. |
| 355 | `crates/pc-ui-egui/src/vector_packages/live_paint.rs:159` | Fixed | Live Paint pointer motion borrows its retained captured graph instead of cloning it. |
| 356 | `crates/pc-ui-egui/src/vector_packages/layout/app_acceptance.rs:21` | Rejected | The task pins GPU acceptance to the real RTX 5090. These hardware gates deliberately fail when their required adapter is absent. |
| 357 | `crates/pc-ui-egui/src/vector_packages/live_paint/app_gpu.rs:79` | Rejected | ModifiersChanged exists in the pinned egui Event API; the integrated target compiles. |
| 358 | `crates/pc-ui-egui/src/vector_packages/fill_stroke_proxy.rs:269` | Fixed | Both exposed strips of the inactive overlapping paint chip now participate in its hit area. Fill/stroke kittest coverage exercises switching. |
| 359 | `crates/pc-ui-egui/src/vector_packages/fill_stroke_proxy.rs:449` | Fixed | Processed monochrome and color SVG bytes are retained in a OnceLock and shared by Arc, avoiding per-frame byte copies. |
| 360 | `crates/pc-ui-egui/src/vector_packages/layout/app_hooks.rs:153` | Rejected | The second return value is the point-size PPI used by Unit::to_px, not units per inch. 72 is correct. The production ruler now calls this bridge. |
| 361 | `crates/pc-ui-egui/src/vector_packages/path_commands.rs:352` | Fixed | Expand and Expand Appearance cache separate checked requests matching their actual appearance flags. |
| 362 | `crates/pc-ui-egui/src/vector_packages/frame_compose.rs:205` | Fixed | Frame drafts cancel on active-target changes and begin validates the requested targets before reusing a draft. Added target-switch regression. |
| 363 | `crates/pc-ui-egui/src/vector_packages/frame_compose.rs:190` | Fixed | Release ends the dragging flag before publication; a rejected commit retains a retryable draft without following the pointer. Added regression. |
| 364 | `crates/pc-ui-egui/src/vector_packages/frame_compose.rs:84` | Rejected | Detached engine previews require a document snapshot; raster caches are shared. Preparation is bounded and the pointer performance gates measure the production path. |
| 365 | `crates/pc-ui-egui/src/vector_packages/measure.rs:360` | Rejected | egui temporary typed data persists across frames until replaced or garbage-collected; it is not cleared each frame. The measure lifecycle tests cover persistence. |
| 366 | `crates/pc-ui-egui/src/vector_packages/measure.rs:122` | Fixed | Measure cancellation clears the pending distance when restoring the completed line, preventing stale failed-preview measurements. |
| 367 | `crates/pc-ui-egui/src/vector_packages/mesh.rs:168` | Fixed | Mesh motion updates the captured gesture directly with an absolute position rather than cloning its base gesture. |
| 368 | `crates/pc-ui-egui/src/vector_packages/mesh.rs:200` | Fixed | Mesh events emit one final preview request per event batch; release emits only its commit request. |
| 369 | `crates/pc-ui-egui/src/vector_packages/path_effects/app_hooks.rs:15` | Rejected | egui temporary data persists across frames. The owner-change comparison is reachable and the lifecycle tests exercise it. |
| 370 | `crates/pc-ui-egui/src/vector_packages/measure/app_gpu.rs:25` | Rejected | The owner explicitly requires real RTX 5090 gates; the ignored hardware acceptance intentionally asserts that adapter. |
| 371 | `crates/pc-ui-egui/src/vector_packages/path_text/app_hooks.rs:122` | Fixed | Path Text keys reports consumption only when a key was actually handled, leaving unrelated shortcuts available. |
| 372 | `crates/pc-ui-egui/src/vector_packages/distort_tools.rs:202` | Fixed | Distort routing sets the current document-space hover before Puppet Warp conversion/rig capture. |
| 373 | `crates/pc-ui-egui/src/vector_packages/distort_tools.rs:386` | Fixed | A successful warp is retained when fresh-rig creation fails; rollback applies only to a still-valid uncommitted capture. |
| 374 | `crates/pc-ui-egui/src/vector_packages/pdf_export.rs:25` | Fixed | Opening PDF export prunes saved artboard IDs against the active document. |
| 375 | `crates/pc-ui-egui/src/vector_packages/pdf_export.rs:97` | Fixed | Scope changes preserve saved page selections; the export request clears them only for scopes that do not use document pages. |
| 376 | `crates/pc-ui-egui/src/vector_packages/path_commands/app_hooks.rs:28` | Fixed | The keyboard frame materializes direct-selection anchors only when a pressed key event can dispatch a command. Menus capture selections on invocation. |
| 377 | `crates/pc-ui-egui/src/vector_packages/pen.rs:125` | Fixed | Pen release publishes before discarding its completed gesture; failed publication leaves the draft available. |
| 378 | `crates/pc-ui-egui/src/vector_packages/pen.rs:138` | Fixed | Pen Enter finishes a cloned gesture and clears the retained draft only after successful publication. |
| 379 | `crates/pc-ui-egui/src/vector_packages/pen.rs:167` | Fixed | Pen continuation chooses the actual single selected layer instead of an unrelated active-layer field. |
| 380 | `crates/pc-ui-egui/src/vector_packages/pen.rs:275` | Rejected | Transient screen-space tessellation is required for the current zoom; its segments and node counts are bounded. The release pointer gate covers this path. |
| 381 | `crates/pc-ui-egui/src/vector_packages/isolation.rs:65` | Fixed | Isolation clears a stale pointer capture when document validation fails. |
| 382 | `crates/pc-ui-egui/src/vector_packages/isolation.rs:199` | Fixed | Isolation handles Escape only when no application dialog is open. |
| 383 | `crates/pc-ui-egui/src/vector_packages/pdf_import.rs:82` | Fixed | An invalid replacement PDF reports its error while retaining the previously valid source and info. Added regression. |
| 384 | `crates/pc-ui-egui/src/vector_packages/pdf_import.rs:203` | Fixed | Changing PDF preview settings clears warning targets together with the staged scene. |
| 385 | `crates/pc-ui-egui/src/vector_packages/pdf_import.rs:178` | Rejected | The source is parsed and native caches prepared once at stage. Motion translates the prepared scene into a detached preview; it does not decode the PDF again. |
| 386 | `crates/pc-ui-egui/src/vector_packages/selection/app_gpu.rs:88` | Rejected | These are explicit real-adapter app acceptance tests required by the task. Missing hardware is a failure under LC_REQUIRE_GPU=1. |
| 387 | `crates/pc-ui-egui/src/vector_packages/pencil.rs:176` | Fixed | Alt closes only a newly created Pencil path; continuing an existing open curve stays open. |
| 388 | `crates/pc-ui-egui/src/vector_packages/pencil.rs:162` | Fixed | Backward sculpt preparation temporarily reverses the retained samples, restores them even on error, and avoids copying the growing vector. |
| 389 | `crates/pc-ui-egui/src/vector_packages/pencil.rs:179` | Rejected | The complete bounded fit is needed to give the interactive preview exactly the same geometry as publication. The fitter and release-frame gates bound this work. |
| 390 | `crates/pc-ui-egui/src/vector_packages/pencil.rs:206` | Rejected | Session::selected_layers filters IDs through the current document walk before these searches. A missing selected layer cannot abort the loop in this API. |
| 391 | `crates/pc-ui-egui/src/vector_packages/swatches_doc.rs:162` | Fixed | Swatches are grouped once by group ID before drawing the group rows, removing repeated full-list scans. |
| 392 | `crates/pc-ui-egui/src/vector_packages/swatches_doc.rs:62` | Fixed | The picker converts edited sRGB bytes back to the original CMYK, normalized Lab or grayscale mode. Added color-space/alpha regression. |
| 393 | `crates/pc-ui-egui/src/vector_packages/swatches_doc.rs:83` | Fixed | Successful swatch publication clears the old error; failed publication records the new error. |
| 394 | `crates/pc-ui-egui/src/vector_packages/layout.rs:401` | Rejected | Rust f64::signum returns 1 for positive zero and -1 for negative zero, not zero. Axis-aligned square drags retain a nonzero second extent. |
| 395 | `crates/pc-ui-egui/src/vector_packages/layout.rs:687` | Rejected | The visible Point Transform overlay builds bounded screen geometry from selected native nodes. This is projection work, and the production pointer-frame gate measures it. |
| 396 | `crates/pc-ui-egui/src/vector_packages/trace_dialog/tests.rs:118` | Rejected | pending_preview is the retained job ID; workers send results through a channel and cannot mutate the UI State. It clears only when the UI explicitly polls results. |
| 397 | `crates/pc-ui-egui/src/vector_packages/repeat/app_hooks.rs:19` | Rejected | egui temporary typed state survives frames. Repeat lifecycle/input tests exercise retained drafts and captures. |
| 398 | `crates/pc-ui-egui/src/vector_packages/swatches_doc/app_gpu.rs:167` | Fixed | The full document equality assertion now runs after the timed pointer-frame loop, leaving the measured samples free of comparison cost. |
| 399 | `crates/pc-ui-egui/src/vector_packages/recolor.rs:97` | Fixed | Recolor rows iterate borrowed source indices instead of cloning each vector every frame. |
| 400 | `crates/pc-ui-egui/src/vector_packages/recolor.rs:154` | Fixed | Hex validation checks ASCII hexadecimal digits explicitly, rejecting a leading plus sign in both parsing and Apply availability. |
| 401 | `crates/pc-ui-egui/src/vector_packages/path_effects.rs:115` | Fixed | The Path Effects panel identifies mixed selected stacks and suppresses index-based edits until the stacks match. |
| 402 | `crates/pc-ui-egui/src/vector_packages/transform_tools/dialogs.rs:36` | Rejected | Distance/Angle calls engine::move_by(distance, angle), which computes Cartesian offsets. |
| 403 | `crates/pc-ui-egui/src/vector_packages/transform_tools/dialogs.rs:203` | Rejected | Convert to Curves clears pending after its successful command sequence; the conversion dialog does not publish twice. |
| 404 | `crates/pc-ui-egui/src/vector_packages/transform_tools/dialogs.rs:52` | Fixed | Transform Each parses seed only when Random is enabled; deterministic transforms accept an unused empty seed. |
| 405 | `crates/pc-ui-egui/src/vector_packages/svg_import.rs:71` | Rejected | The integrated SVG command response always contains its warnings array, including an empty array. The checked response contract is shared with this caller. |
| 406 | `crates/pc-ui-egui/src/vector_packages/selection.rs:122` | Fixed | Anchor subtraction uses a HashSet for membership instead of a nested scan. |
| 407 | `crates/pc-ui-egui/src/vector_packages/selection.rs:182` | Fixed | Node-base comparisons borrow the captured sets instead of cloning them each frame. |
| 408 | `crates/pc-ui-egui/src/vector_packages/transform_tools/tests.rs:139` | Rejected | The regression uses the documented transform coordinates and was corrected by its package before integration. The integrated transform suite checks the final fixture. |
| 409 | `crates/pc-ui-egui/src/vector_packages/styles_panel.rs:215` | Fixed | Graphic Styles preview documents are shared Arc snapshots in the egui bridge rather than copied documents. |
| 410 | `crates/pc-ui-egui/src/vector_packages/styles_panel.rs:45` | Fixed | Unique style naming builds one name set and probes it, avoiding repeated scans for every candidate suffix. |
| 411 | `crates/pc-ui-egui/src/vector_packages/styles_panel.rs:179` | Rejected | Native styles are identified by stable IDs; duplicate display names are permitted by the native style contract. Rename preserves references. |
| 412 | `crates/pc-ui-egui/src/vector_packages/persona.rs:97` | Fixed | Selecting another tool explicitly clears the persona pointer guard along with the outgoing tool capture. |
| 413 | `crates/pc-ui-egui/src/vector_packages/width/tests.rs:150` | Rejected | The package's corrected test reopens the profile combo before selecting Fine; integration runs that final test. |
| 414 | `crates/pc-ui-egui/src/vector_packages/svg_export.rs:111` | Fixed | SVG export returns an error when the save-picker service is unavailable; an actual user picker cancellation remains cancellation. |
| 415 | `crates/pc-ui-egui/src/vector_packages/symbols.rs:376` | Fixed | Same-frame symbol press/release dispatches Down followed by Up so a quick click places exactly once. |
| 416 | `crates/pc-ui-egui/src/vector_packages/symbols.rs:370` | Rejected | The placement view transform is intentionally captured at pointer-down, consistent with the shared gesture coordinate contract and placement tests. |
| 417 | `crates/pc-ui-egui/src/vector_packages/symbols.rs:142` | Fixed | Edit Master now displays Capture::new failures through the symbol panel error state. |
| 418 | `crates/pc-ui-egui/src/vector_packages/view_modes.rs:259` | Rejected | The view cache reuses prepared frames keyed by document snapshot, revision, view and clip. Detached projection ownership is required; repeated unchanged frames reuse it. |
| 419 | `crates/pc-ui-egui/src/vector_packages/view_modes.rs:65` | Rejected | ViewState::outlined inherits outlined ancestors, and prepare walks each descendant independently. Group children receive their own X-ray projection/fade. |
| 420 | `crates/pc-vecfx/src/blend/spine.rs:68` | Fixed | Spine inverse lookup evaluates cubic subsegments and native arclength directly, without allocating PathData at each step or cloning for the tangent. |
| 421 | `crates/pc-vecfx/src/blend/spine.rs:80` | Rejected | geom::bez::tangent already falls back to the first noncoincident endpoint control or chord when the derivative vanishes. |
| 422 | `crates/pc-ui-egui/src/vector_packages/transform_tools.rs:588` | Fixed | Free Distort handles use the current quad corners and edge midpoints, matching the displayed distorted boundary. |
| 423 | `crates/pc-ui-egui/src/vector_packages/transform_tools.rs:602` | Fixed | Reshape overlays include descendants of selected groups, matching the native mouse-down selection scope. |
| 424 | `crates/pc-ui-egui/src/vector_packages/transform_tools.rs:512` | Fixed | Enter opens a transform dialog only when no pending transform or envelope request owns the state. |
| 425 | `crates/pc-ui-egui/src/vector_packages/transform_tools.rs:647` | Rejected | The fallback expression explicitly parenthesizes the conditional before division, giving both axes the same half-extent lever. |
| 426 | `crates/pc-vecfx/src/blend/correspondence.rs:167` | Rejected | blend::validate_path rejects empty subpaths before prepare can index normalized anchors. |
| 427 | `crates/pc-vecfx/src/blend/correspondence.rs:251` | Rejected | PathOp::Join intentionally groups later contours into the first contour's compound component; the operation belongs only on the component's first contour. |
| 428 | `crates/pc-ui-egui/src/vector_packages/view.rs:104` | Rejected | damage already compares resolution_dpi and returns full invalidation on a change. |
| 429 | `crates/pc-ui-egui/src/vector_packages/view.rs:35` | Rejected | Native scaled shape projection validates scale 1..64, finite coordinates and visible raster area before rendering; rejection regressions cover these limits. |
| 430 | `crates/pc-ui-egui/src/vector_packages/path_text.rs:295` | Fixed | Path Text keeps its draft/preview after a failed engine publication and clears them only after success. |
| 431 | `crates/pc-ui-egui/src/vector_packages/symbols/app_gpu.rs:145` | Rejected | matches! on this place expression inspects the enum discriminant without moving its payload. The pinned Rust compiler accepts the test. |
| 432 | `crates/pc-vecfx/src/liquify/mod.rs:102` | Fixed | Exactly MAX_DABS is accepted. Additional work is preflighted before mutation and rejected atomically; added exact-limit regression. |
| 433 | `crates/pc-vecfx/src/liquify/mod.rs:129` | Fixed | Liquify simplification now requires exactly one output contour rather than silently dropping additional contours. |
| 434 | `crates/pc-vecfx/src/liquify/mod.rs:116` | Rejected | The liquify kernel retains the input subpath vector and edits its anchors in place. It does not add/remove contours before the checked simplification stage. |
| 435 | `crates/pc-vecfx/src/liquify/mod.rs:99` | Fixed | Dabber preflights the appended work without cloning its retained samples/dabs; rejection leaves the prefix unchanged. Added atomicity regression. |
| 436 | `crates/pc-vecfx/src/brush/calli.rs:31` | Rejected | Track::new removes duplicate vertices and rejects fewer than two distinct vertices, guaranteeing positive track length before calligraphic projection. |
| 437 | `crates/pc-ui-egui/src/vector_packages/repeat.rs:220` | Fixed | Repeat Cancel is evaluated before automatic field publication and wins; arbitrary pointer release no longer commits a dirty panel draft. |
| 438 | `crates/pc-ui-egui/src/vector_packages/repeat.rs:310` | Rejected | InputState::focused is window focus, not keyboard widget focus. Canvas drags are preserved while the window is focused. |
| 439 | `crates/pc-ui-egui/src/vector_packages/repeat.rs:132` | Fixed | The single selected repeat/layer bounds path now borrows the source layer/children. Multiple selections copy only when a new repeat is explicitly requested. |
| 440 | `crates/pc-vecfx/src/envelope/mesh.rs:35` | Rejected | EnvelopeSpec validation requires nonzero rows/columns and exact grid size before the private grid evaluator is called. Pixel dimensions do not set grid dimensions. |
| 441 | `crates/pc-ui-egui/src/vector_packages/width.rs:198` | Rejected | The width capture updates the retained newly inserted point by its selected index. Its add/drag regression checks one point across multiple moves. |
| 442 | `crates/pc-ui-egui/src/vector_packages/trace_dialog.rs:546` | Fixed | Trace requests repaint while a preview receiver is pending, including the gap after the worker finishes and before UI result polling. |
| 443 | `crates/pc-vecfx/src/path_effects/render.rs:66` | Rejected | PixelRect::size uses width/height clamped to zero for inverted extents; its area cannot become negative. Large positive areas are rejected before allocation. |
| 444 | `crates/pc-vecfx/src/blend.rs:313` | Fixed | Maximum document ID scanning is iterative and includes symbol definitions, avoiding unbounded recursion. |
| 445 | `crates/pc-vecfx/src/blend.rs:384` | Fixed | Blend cache preparation checks document depth before walking and bounds recursive rendering. Added excessive-depth rejection regression. |
| 446 | `crates/pc-vecfx/src/path_effects/mapping.rs:24` | Fixed | Mapped-curve error measurement rejects nonfinite distances explicitly instead of allowing max reduction to hide NaN. |
| 447 | `crates/pc-vecfx/src/path_effects/mapping.rs:46` | Rejected | Final validate_path checks every mapped anchor and handle, including a single-knot output. |
| 448 | `crates/pc-vecfx/src/live_paint/gaps.rs:41` | Rejected | The candidate loop inserts endpoint a into used before handling endpoint-to-curve bridges, so each endpoint is reserved once. |
| 449 | `crates/pc-vecfx/src/live_paint/gaps.rs:33` | Fixed | Gap detection checks other subpaths of the same source while excluding only the endpoint's own subpath, preventing bridges from attached endpoints. |
| 450 | `crates/pc-vecfx/src/brush/warp.rs:147` | Rejected | ArtScale::Proportional intentionally uses the along-axis scale for both dimensions; Stretch and BetweenGuides use stroke width for cross scaling. |
| 451 | `crates/pc-vecfx/src/brush/warp.rs:103` | Rejected | Art-brush validation requires closed contours with at least three knots. Open art is rejected before bend; a closed contour's final endpoint repeats its first. |
| 452 | `crates/pc-vecfx/src/brush/warp.rs:295` | Rejected | Brush/appearance validation rejects zero/nonfinite scale and nonpositive stroke width before pattern placement. |
| 453 | `crates/pc-vecfx/src/brush/warp.rs:185` | Rejected | Scatter validation requires finite positive spacing before this private placement loop. |
| 454 | `crates/pc-vecfx/src/live_paint/mod.rs:234` | Rejected | Planar::try_shape_builder explicitly rekeys input shapes to zero-based slots. Region and edge source indices correctly use those slots; added multi-source regression. |
| 455 | `crates/pc-vecfx/src/live_paint/mod.rs:76` | Fixed | Live Paint face/edge edits validate a cloned candidate before publication, leaving the original spec unchanged on errors. Added atomicity regression. |
| 456 | `crates/pc-vecfx/src/live_paint/mod.rs:285` | Fixed | Touched-face testing skips already found faces and rejects disjoint face/gesture bounding boxes before intersecting boundary segments. |
| 457 | `crates/pc-vecfx/src/blend/paint.rs:113` | Fixed | Fill interpolation returns exact input clones at t=0 and t=1, including unequal gradient stop counts and midpoints. Added endpoint regression. |
| 458 | `crates/pc-vecfx/src/blend/paint.rs:192` | Rejected | Discrete appearance item types intentionally switch at the midpoint; continuous interpolation applies only to compatible paint/geometry fields. |
| 459 | `crates/pc-vecfx/src/blend/paint.rs:252` | Fixed | Perceptual distance includes extra gradient colors instead of truncating to the shorter vector. Added additional-color regression. |
| 460 | `crates/pc-vecfx/src/blend/paint.rs:255` | Rejected | Group/non-Group pairs are rejected. Unmatched group children collapse using their own retained paint, so they contribute no paint change to the spacing metric. |
| 461 | `crates/pc-vecfx/src/envelope.rs:97` | Fixed | Flat source bounds expand inward at the coordinate limit, keeping valid source geometry within the allowed frame. |
| 462 | `crates/pc-vector/src/appearance.rs:238` | Fixed | Compiled fill/mesh coverage inverts for inverted paths before compositing. Added interior/exterior regression. |
| 463 | `crates/pc-vector/src/appearance.rs:279` | Rejected | Coverage buffers are bounded per item and tile and required for ordered alpha compositing. Real raster and pointer performance gates measure this implementation. |
| 464 | `crates/pc-vector/src/curvature_fit.rs:39` | Rejected | The public fitter validates generated handles as well as anchors against its explicit coordinate contract; an out-of-bounds result is a checked error. |
| 465 | `crates/pc-vecfx/src/puppet/mod.rs:170` | Fixed | Puppet refitting restores source smooth/corner flags at original segment boundaries. Added nontranslation warp regression. |
| 466 | `crates/pc-vecfx/src/puppet/mod.rs:188` | Rejected | Zero-length fitted contours remain legal native paths; source singletons use the explicit fallback. Output validation rejects only malformed/budget-breaking geometry. |
| 467 | `crates/pc-vecfx/src/live_paint/signatures.rs:13` | Fixed | Live Paint boundary validation rejects segment/subpath indices equal to the maximum, since the upper bound is exclusive. |
| 468 | `crates/pc-vecfx/src/live_paint/signatures.rs:56` | Fixed | Edge signature nearest-boundary lookup is restricted to its specified source, so coincident paths cannot borrow another source's boundary. |
| 469 | `crates/pc-vecfx/src/path_effects/mod.rs:184` | Rejected | Disabled effects remain persisted native data and are validated against the same finite/bounded schema; rendering ignores their execution only. |
| 470 | `crates/pc-vecfx/src/path_effects/mod.rs:190` | Rejected | Transform with zero copies applies once to the original; copies N adds N duplicates. This is the documented Transform effect behavior. |
| 471 | `crates/pc-vecfx/src/path_effects/mod.rs:226` | Rejected | Scene validation is bounded by the small MAX_PAINT_PATHS limit and enforces growth after each effect application. It cannot scan an unbounded document. |
| 472 | `crates/pc-vecfx/src/envelope/fit.rs:104` | Rejected | Map::point validates mapped coordinates before returning, including the initial singleton anchor. |
| 473 | `crates/pc-vecfx/src/envelope/fit.rs:57` | Fixed | Envelope fitting retains the actual mapping error in its sample adapter and returns it instead of converting coordinate failures to Budget. |
| 474 | `crates/pc-vecfx/src/liquify/kernel.rs:89` | Rejected | Options is a type alias for LiquifyParams and implements Default in liquify/mod.rs. The crate compiles. |
| 475 | `crates/pc-vecfx/src/liquify/kernel.rs:170` | Rejected | Options::validate enforces positive bounded width/height before constructing the private kernel stroke. |
| 476 | `crates/pc-vecfx/src/liquify/kernel.rs:180` | Rejected | Zero width/height is rejected by the checked Options boundary before falloff is reachable. |
| 477 | `crates/pc-vecfx/src/liquify/kernel.rs:481` | Rejected | The checked Stroke::path wrapper applies the Simplify option after kernel finish and validates the result. |
| 478 | `crates/pc-vecfx/src/liquify/kernel.rs:413` | Fixed | Brush culling uses inclusive bounding-coordinate overlap for flat contours instead of admitting every flat contour. Added near/far line regression. |
| 479 | `crates/pc-vecfx/src/brush/track.rs:174` | Fixed | Brush preflight uses the native flattener's exact shared segment count and supplied tolerance; appearance/outline guards use its updated floor. |
| 480 | `crates/pc-vecfx/src/sweep.rs:65` | Rejected | Tip geometry must also satisfy the explicit output coordinate limit. A checked error for an out-of-bounds generated outline is the intended contract. |
| 481 | `crates/pc-vecfx/src/path_effects/distort.rs:74` | Rejected | Roughen noise advances with pts.len inside the sampling loop, giving distinct deterministic offsets at successive samples. |
| 482 | `crates/pc-vecfx/src/repeat.rs:118` | Fixed | Grid pitch positivity is checked only for axes with more than one instance; unused axes still require finite values. Added single-instance regression. |
| 483 | `crates/pc-vecfx/src/repeat.rs:130` | Fixed | Repeat matrices require finite coefficients while mapped source bounds enforce coordinate limits, allowing a large valid reflection translation. Added regression. |
| 484 | `crates/pc-vector/src/lib.rs:259` | Rejected | Midpoints are indexed between sorted gradient stops by the native gradient contract. Stops are normalized before sampling; midpoint semantics are not insertion order. |
| 485 | `crates/pc-vector/src/appearance/geometry.rs:208` | Rejected | Appearance validation bounds stroke width and arrow scale. Their maximum radius yields a finite bounded step count, and aggregate point limits are checked. |
| 486 | `crates/pc-vector/src/appearance/geometry.rs:392` | Fixed | Arrowheads locate the first and last open subpath rather than filtering only the absolute first/last contours. |
| 487 | `crates/pc-vector/src/appearance/geometry.rs:231` | Rejected | The center variable is the arrow's longitudinal x center; local y is centered on zero. The hollow contour is correctly centered. |
| 488 | `xtask/src/tool_art_vector.rs:676` | Rejected | Both output asset directories are tracked in the checkout. The regeneration command writes to those repository directories. |
| 489 | `crates/pc-vector/src/appearance/paint.rs:55` | Rejected | Appearance validation requires a finite positive pattern scale before the private Sampler is constructed. |
| 490 | `crates/pc-vector/src/tests.rs:466` | Rejected | The test ellipse radius and dimensions put the standard four-cubic approximation's area error inside the stated tolerance; the native suite verifies it. |
| 491 | `crates/pc-vector/src/view.rs:44` | Rejected | Scaled shape projection scales geometry and gradient frames together; tile parity tests compare whole and split visible bands. |
| 492 | `crates/pc-vector/src/view.rs:66` | Rejected | Tile projection is bounded by scale/visible-area limits and preserves exact document-space paint frames. Its performance is checked by the real scaled-canvas gates. |
| 493 | `crates/pc-vecfx/src/puppet/arap.rs:576` | Fixed | auto_pins returns an empty vector for count zero. Added regression. |
| 494 | `crates/pc-vector/src/projective.rs:249` | Fixed | Projective preflight evaluates cubic denominator extrema to detect an interior tangent horizon before fitting. Added even-multiplicity horizon regression. |
| 495 | `crates/pc-vecfx/src/repeat/projection.rs:133` | Fixed | Generated repeat layers clear source fill_cache when assigning new IDs, preventing stale ID/coordinate cache reuse. |
| 496 | `crates/pc-vecfx/src/width.rs:304` | Fixed | Adjacent dash spans separated by zero-length gaps are coalesced before caps are generated. Added zero-gap regression. |
| 497 | `crates/pc-vector/src/live_corners.rs:247` | Fixed | Live Corners accumulates and checks sample-table budgets incrementally before constructing subsequent edges/subpaths. |
| 498 | `crates/pc-vector/src/live_corners.rs:583` | Fixed | Live Corners locates ordered handles by binary search rather than scanning all handles for each corner. |
| 499 | `crates/pc-vector/src/design_shapes.rs:694` | Rejected | Resize converts the pointer through the inverse rotation before updating dimensions and reconstructs the center using the rotated fixed point. |
| 500 | `crates/pc-vector/src/design_shapes.rs:646` | Fixed | Ellipse callouts expose their tail handle but omit the unused corner-radius handle. |
| 501 | `crates/pc-vector/src/edit.rs:235` | Rejected | Checked edits use a detached document snapshot to guarantee atomic validation/publication; heavy raster caches are shared, and the production frame gates measure this cost. |
| 502 | `crates/pc-vector/src/edit.rs:156` | Fixed | Straight-segment bending calls the unchecked anchor mover inside the already checked edit, eliminating a nested full-path snapshot. |
| 503 | `crates/pc-vector/src/edit.rs:432` | Rejected | Selected anchors are sorted, deduplicated and removed in reverse order, giving deterministic results independent of input selection order. Sequential neighbor fitting is the defined algorithm. |
| 504 | `crates/pc-vector/src/pencil_fit.rs:323` | Rejected | Sculpt boundary insertion uses exact de Casteljau subdivision and preserves original cusp flags; split interior knots are smooth along the original curve. |
