# Per-camera data files: implementation and validation

Status: implementation and whole-corpus quality validation complete. Final check logs are retained under target/.
Changes are uncommitted. Continued the branch's existing native/rawler boundary at `c9fd969c`;
no decoder replacement, dependency fork, Match Look edit, GUI window or client-photo access.
The pre-existing edit to `CODEX-TASK.md` was retained without modification.

## Delivered

All four requested files are whole, byte-identical upstream data, embedded offline and parsed once.
The SHA-256-pinned manifest and `scripts/camera_data.py` verify/restore them; the update procedure,
precedence and mode guards are documented in `docs/CAMERA-DATA.md`.

| Data | Pin | Coverage in this pin | License |
|---|---|---|---|
| RawTherapee camconst.json | `5f486d3678b34c74ba0c63571c17babe20935019` | 308 entries / 490 names | GPL-3.0-or-later |
| rawspeed cameras.xml | `0c16fee7772594e98897bded01fc31bc3409b4cb` | 1,402 camera/mode entries | CC-BY-SA-3.0 |
| darktable noiseprofiles.json | `733bd69f32cac7ff5e41025115942772add1f088` | 437 bodies / 8,672 profile entries | GPL-3.0-or-later |
| darktable wb_presets.json | same darktable pin | 485 bodies / 20,284 preset entries | GPL-2.0-or-later |

* ISO/channel white levels, aperture scaling and white_max reach full development, lazy normalized
  samples and binned previews. RT `rawimage.cc`'s integer right-shift mapping handles different
  stored bit depths. A strong per-frame saturation plateau supplies the final lower clip fallback.
* ISO black levels and table optical-black areas are applied at the shared decode boundary. Phase
  medians exclude a four-pixel boundary, reject hot pixels and retain the correct active-area phase.
  RT absolute/offset conventions follow upstream. Explicit file/measured crops retain precedence;
  table frame variants fill missing crops and incompatible mode/rectangle data is rejected.
* rawspeed's three-plane matrices fill otherwise missing calibration, behind the existing file,
  calibrated matrix-table and rawler runtime sources. DNG calibration remains authoritative.
* PDAF row periods/offsets drive RT's green-triplet outlier detector, including adjacent rows.
  Corrections use untouched detection inputs and same-colour clean-row interpolation. Isolated
  legitimate detail and cameras without a known pattern are preserved.
* Per-camera/ISO noise fits interpolate between measurements; negative fitted offsets are retained.
  They operate through a camera-space VST, with WB/baseline/camera conversion tracked explicitly.
  The existing robust image estimator adapts fit amplitude to the actual demosaic/resampling/frame;
  measured channel ratios remain. This measured hybrid was selected over the fixed-profile path.
* Camera WB names appear in the existing shared dropdown. Zero-tuning measurements resolve through
  the camera model to temperature/tint; values use the existing Custom settings representation.
  Generic presets use measurements when available and retain their prior fallback values. No
  catalog/settings migration is needed. New variable strings are on the i18n pending list.

Normalization gain is a derived colour model, not a mutation of stored colour tags or vendor WB.
This keeps header/full facts equal even when full decoding measures black from samples. Render
cache keys include calibrated noise and WB context, including layer NR. The GPU app path shares
the exact CPU calibrated variance transform and runs the existing
verified wavelet/reduction/synthesis kernels on the GPU. Generic NR keeps its native GPU path.

Added checks: 14 camera-data raw tests, 2 pipeline calibration/cache tests, 1 real-GPU parity test,
1 WB pointer/Undo test (both themes), and 1 opt-in real-source engine integration test explicitly run.

No new Cargo.lock packages or C/-sys dependencies were added. The raw crate reuses the workspace's
existing serde_json and quick-xml packages; XML enables its existing serde feature. This is needed to
consume the intact JSON/XML rather than inventing or generating partial camera tables.

## Quality evidence

The initial fixed-profile path improved the Canon 7D ISO 6400 crop from 34.47 to 38.61 dB, but
underperformed the existing estimator at 40.82 dB. Neutral shadow bias alone gave 38.64 dB. The
camera-space amplitude adaptation retained measured channel ratios and removed that regression:

| Canon 7D RawNIND pair | Unfiltered PSNR / SSIM | Calibrated adaptive NR | Existing estimator |
|---|---|---|---|
| ISO 3200 vs ISO 100 | 37.55 / 0.86476 | 43.67 / 0.96699 | 43.62 / 0.96684 |
| ISO 6400 vs ISO 100 | 34.47 / 0.77056 | 40.84 / 0.94247 | 40.82 / 0.94286 |

The gains over the estimator are small (+0.05 / +0.02 dB), and ISO 6400 SSIM is slightly lower.
These two crops of one camera establish a regression check, not superiority across cameras.
Measurements use RCD camera-space crops, fixed 60% luminance/colour amounts, a 488×488 interior,
integer registration and exposure fits determined from noisy input and held fixed for filtered
outputs. `scripts/camera_pair_metrics.py` computes PSNR (peak 1) and 7×7 SSIM. Numeric artifacts are
in `docs/wip/CAMDATA-paired-metrics.json`; photos and intermediate buffers stay in target/.

Independent darktable 5.6.2 reference exports use actual per-file rawprepare/WB parameters, disable
exposure/sigmoid/colour calibration, export float linear Rec.2020, and compare profiled wavelet-auto
NR off/on. The reproducible `scripts/camdata_darktable_reference.py` was run and reproduced results:

| darktable pair | NR off PSNR / SSIM | Profiled NR on |
|---|---|---|
| ISO 3200 | 33.50 / 0.75175 | 38.09 / 0.88761 |
| ISO 6400 | 30.31 / 0.61726 | 34.80 / 0.80047 |

Absolute scores between our camera-space outputs and darktable's Rec.2020 outputs are **not**
apples-to-apples. Each pipeline is compared to its own clean decode. These numbers show the
within-pipeline NR benefit; they do not establish that our renderer beats darktable.
RawNIND is actually CC-BY-SA-4.0, not CC0: its Dataverse license was checked, credits retained and
all three files SHA-256 verified by `scripts/rawnind_camdata_samples.py`.

The synthetic camera checks also prove: the published Canon 5D II ISO 160 plateau (12700) moves
from <0.8 normalized intensity to exactly 1; phase optical-black measurement eliminates a +6 EV
dark-frame bias to <1e-8; stripe repair fixes a repeated marked line while keeping isolated detail;
the synthetic row-period Fourier peak drops 20.56 dB;
unclipped chromaticity and original header colour/WB facts survive unequal channel white levels.
These are synthetic dark/stripe frames, not a claim about an owner's lens-cap capture.

## Whole-corpus audit

All 1,870 CC0 samples in the raw.pixls.us repository snapshot were downloaded
under `target/camdata-corpus/raw`, approximately 54.43 GiB, with the published per-file SHA-256 and
zero download errors. Other licenses were excluded. The resumable `scripts/camera_corpus.py`
audits every sample and reports decoder failures separately. It then renders twenty heavily clipped
files selected by at least 100 bins containing clipping in all three sensor colours.

The broader audit exposed the missing RT bit-depth mapping and triggered the strict plateau
fallback. Flags were cross-checked with darktable's rawspeed identifier; complete records are retained.
Final result: 1,587 valid decodes, all with matching header/full facts; 283 decoder failures
(58 Unsupported, 144 unrecognized containers, 34 Corrupt and 47 resource-limit results). These
are decoder coverage/correctness boundaries, not camera-table parse failures; no foreign codec
was patched to hide them. The static-table versus plateau audit flagged twelve files; the strict
per-frame fallback supplies their lower clip coordinates. Complete flags/records are retained in
`target/camdata-corpus/levels.jsonl` and summarized in `docs/wip/CAMDATA-corpus-metrics.json`.
PDAF repair changed 6,267,004 samples in 85 files; this is an applicability count, not a claim
that every changed real pixel has a paired ground truth.

Twenty integer-CFA vendor files, each with at least 100 fully clipped valid-image bins, were
rendered before/after with Clip Neutral. Bins are matched in sensor coordinates, exclude borders
and are reanchored for black phase/deltas. Across 514,292 fully clipped bins the requested
magenta/cyan criterion (hue 280–340 degrees, chroma >0.1) counted 1 before and **0 after**.
Partial clipping is reported separately because legitimate coloured lights must not be called
a calibration failure. DNGs with post-demosaic opcodes are not included in this vendor-table hue
census; their levels are still audited. This is measured clipping in public samples, not an
assertion that contributors deliberately shot overexposure brackets.

One D700 audit was interrupted by pausing the runner for timing, causing its wall-time timeout;
the exact-name recheck passed and its interruption/recheck record was preserved. A first
attempt used a default-feature audit executable and produced no measurements; its error
log is retained separately. The final audit uses a copied, SHA-256-recorded testgen executable so
ordinary Cargo test/example rebuilds cannot overwrite it during the run.

## Checks and app performance

Offline toolchain 1.98.1, inherited CARGO_BUILD_JOBS=4, retained target/.
The owner's resume instruction superseded the task's earlier end-of-job testing rule: only
cargo check, strict clippy on touched crates and newly written tests filtered by name are now
permitted. The full suites, GPU acceptance and benchmark results below were completed before
that instruction. No such runs were repeated after resuming; the existing logs were retained.
The final offline cargo check passed for all five touched crates after resuming
(`target/camdata-check-final.log`). Final strict clippy logs are
`target/camdata-clippy-{raw,pipeline,gpu,engine,ui}.log`; all passed.
Full suites were confined to changed crates; later calibration fixes were checked by targeted tests.
Full GPU validation used LC_REQUIRE_GPU=1 and identified NVIDIA GeForce RTX 5090 / Vulkan, driver
615.71.09. The new calibrated-NR parity test actually renders on that device and checks CPU/GPU
output within 3 LSB; it also verifies that the calibrated host stage was used.

| Crate full suite | Passed | Existing opt-in tests ignored |
|---|---:|---:|
| lightcraft-raw | 244 | 9 |
| lightcraft-pipeline | 423 | 3 |
| lightcraft-gpu | 98 | 11 |
| lightcraft-engine | 896 | 6 |
| lightcraft-ui-egui | 339 | 8 |

The full engine/UI runs preceded the final raw metadata/fallback and GPU optimization refinements;
final targeted raw, engine file-loading/DNG, WB-pointer and hardware parity checks cover those
changes without repeating unrelated large suites. The engine file/profile filter passed 12 tests;
the real-source test passed when explicitly activated. Clippy uses `--all-targets -- -D warnings`
on every changed crate, with testgen also enabled for the audit harness. Whole-file SHA checks
passed, Python tools compiled, and the attributions generator recorded the whole data plus
source-semantic ports. The new reusable variance-transform code did not change paired-NR
numerics; pipeline calibration tests still passed after the GPU wavelet optimization.

Real app path (`N=1 ONLY=drag`, Canon 7D ISO 6400, RTX 5090/Vulkan): final GPU wall times were
38.9 ms exposure, 14.8 ms draft highlights, 17.8 ms draft clarity, 193.3 ms draft NR and 95.2 ms
full 1920×1280 NR. CPU/GPU typical loupe difference was max 1 LSB / mean 0.0001 LSB. The initial
whole-CPU calibrated stage took 2050.9 ms for the full NR drag; the expensive wavelets now run
on the GPU. These are single-run shared-machine observations under different load, not a
controlled speedup ratio or a universal latency promise. No hardware-specific tuning was added.

The new WB test clicks the dropdown/preset with real pointer input in the existing headless harness,
checks resolved settings and Undo, and repeats in light/dark themes. The optional real-source test
was explicitly activated on CC0 Sony A7 III and the licensed Canon 7D raw; both returned a raw
source with camera colour, noise calibration and camera WB presets in the actual load_bytes path.

## Boundaries and remaining limitations

Baked nonlinear DCP/look-table sources, nonlinear colour-calibration/negative settings, unprofiled
bodies and ISO outside a measured interval retain the existing image estimator. Calibrated GPU NR
retains host variance transforms while the expensive wavelets execute on the verified GPU kernels.
A fully native calibrated variance shader would require independent verification.
All tuning variants remain in the whole WB data/API, but the UI exposes zero-tuning names and uses
the existing temperature/tint sliders for adjustment. Sensor DNG export rejects unequal vendor CFA
whites rather than silently truncating them to red; normalized float-DNG conversion for that case
is a separate export feature. Existing equal-white sensor and HDR DNG exports remain supported.

Unavoidable integration outside the camera-data reader: lc-raw normalization/binned/colour/DNG
writer/saturation boundaries; lc-pipeline source facts/cache/layer NR; lc-engine source loading and
WB command; lc-gpu calibrated host-stage bridge; the existing Develop WB menu and pointer tests;
PORTS, notices and generated app attributions. The native Nikon measured-body test was scoped to
its native decoder (unchanged expectations), while a separate camera-data test covers table modes.
The pinned rawler sensor contract uses testgen decoder-only APIs so calibration does not regenerate
or invalidate its independently checked sensor goldens. Match Look files were untouched.

## Sources followed

* [RT camconst.cc](https://github.com/RawTherapee/RawTherapee/blob/5f486d3678b34c74ba0c63571c17babe20935019/rtengine/camconst.cc)
  and [rawimage.cc](https://github.com/RawTherapee/RawTherapee/blob/5f486d3678b34c74ba0c63571c17babe20935019/rtengine/rawimage.cc):
  nearest ISO, aperture mapping/cap, TIFF-bit-depth shifts and absolute black flags.
* [RT PDAF filter](https://github.com/RawTherapee/RawTherapee/blob/5f486d3678b34c74ba0c63571c17babe20935019/rtengine/pdaflinesfilter.cc):
  repeated row period and green-triplet detection. Clean-row interpolation/phase medians are host adaptations.
* [rawspeed Camera.cpp](https://github.com/darktable-org/rawspeed/blob/0c16fee7772594e98897bded01fc31bc3409b4cb/src/librawspeed/metadata/Camera.cpp):
  XML sensors, ISO defaults, crops, aliases and three-plane matrices.
* [darktable noise profiles](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/common/noiseprofiles.c)
  and its pinned denoiseprofile.c: skip handling, interpolation, VST and profile coefficients.
* [Adobe Lightroom Classic WB help](https://helpx.adobe.com/ie/lightroom-classic/desktop/process-and-develop-photos/image-tone-color.html):
  named presets with editable temperature/tint. The UI uses the project's existing dropdown/menu widgets.
* [RawNIND paper](https://arxiv.org/abs/2501.08924) and
  [dataset](https://doi.org/10.14428/DVN/DEQCIM): paired raw evaluation and actual dataset license.

Camera-space amplitude adaptation and conservative mode agreement were worked out in this host
because the inherited working-space NR stage differs from darktable's earlier stage. Alternatives
were measured above; the fixed-profile regression was not accepted.
