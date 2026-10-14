# Sony and DJI decoder, merge and metadata report

2026-10-14. Continued the existing native/rawler routing and comparison work in
this branch. Changes are uncommitted. The task's final absolute-minimum testing
rule was followed: focused new tests, small requested real-file measurements,
check and strict clippy; no full suites, GPU acceptance tests or benchmarks.

## 1–2. Sony decoding and measured routing

Twenty-one CC0 samples across sixteen bodies cover ARW2, uncompressed, lossless
large, M/S linear RGB, APS-C crop, old SR2/ARW1, composed ARQ, and new ARW6 normal
and HQ. **719,037,136 reference observations agree exactly after black
subtraction** in the explicitly described comparison domains below. This is a
bounded representative measurement, not an exhaustive measurement of every
body/firmware/mode combination. Source URLs and SHA-256 values are pinned in
`crates/lc-raw/tests/fixtures/sony-excellence-samples.tsv`; source data stays in
`target/sony-compare`. Native remains first; only legacy ARW1 uses rawler.

ARW2's inverse curve was clamped prematurely to nominal white 16,383. Valid
encoded highlights reach 16,628. Removing the storage clamp fixes the reported
245-DN outliers; nominal development white remains separate. On CC0 2414,
28,086 restored values change the complete sensor fingerprint to
`b873f68087aea89f`; clamping the independent LibRaw buffer reproduces the old
`105c47702bbea90f` exactly. Only that Sony contract golden was changed; NEF was
untouched. Curve breakpoints now apply the reference's twelve-bit mask.

DSC-R1 SR2 stores big-endian pixels in a little-endian TIFF and uses a per-file
private-calibration key. Native decoding now honors that key, maps per-channel
black to the GRBG spatial pattern, retains it through calibration, and reads WB.
ARQ's R/G1/G2/B layout becomes floating RGB with the exact half-DN green mean.
An explicit LibRaw RGBG-to-RGB projection, rather than an integer rounding or
component-count disguise, supplies its comparison. ARQ now receives the same
camera white and matrices as the source ARWs. M/S conversion uses LibRaw's
actual float BT.601 neutral/truncation convention and unity WB because camera
WB is already applied. The two M/S files improved from maximum 2 DN and mean
1.246177/1.247552 DN to **zero**, across 67,450,272 RGB observations. Remaining
white, WB representation and crop differences are retained in the ledger.

ARW6 is a complete safe Rust implementation following pinned LibRaw
`830ac963b40223c6a25add5be65036fa8d56e2d6`. Independently compiled C++ reference
buffers match both normal and HQ over **33,003,520 samples each**, maximum/mean
0 DN. Its black 1,024 / white 39,002 / sixteen-bit domain is protected from
ordinary fourteen-bit camera-table overrides. The entire 4,096-entry inverse
curve is preserved; LE16 SHA-256
`53a14c1cc233473cb3453d3a5650368591cae09c8b4af4ea2ee5b135deba55e5`.
Installed LibRaw 0.22.2 and both installed RGB tools reject this new codec; the
pinned external master supplies the independent reference, never a dependency.

Legacy A100's root SubIFDs tag points to raw entropy rather than an IFD. The
adapter recognizes the bounded Sony MRI structure, validates its WB blocks,
and uses rawler's public IFD/ArwDecoder interfaces without traversing that pixel
pointer as TIFF metadata. Ordinary unknown-type/nesting/allocation guards stay
strict, and the call retains panic containment. This fixes real-file rejection
without letting random entropy counts drive upstream allocations. Rawler and
LibRaw differ by one sensor column and CFA origin; the explicitly aligned
visible crop matches **10,088,000 samples exactly**. The unmatched full-sensor
geometry remains documented rather than silently resizing a reference.

| Body | Variant / CC0 ID | Reference result after black subtraction | Selected decoder |
|---|---|---|---|
| ILCE-6000 | 12bit 12bit compressed (3:2) / 1971 | 100% exact, max/mean 0 DN; 24,240,576 samples | native |
| DSLR-A100 | compressed (3:2) / 2346 | 100% exact, max/mean 0 DN; 10,088,000 samples; visible crop, reference x + 1 | rawler (protected MRI) |
| ILCE-7M3 | 14bit 14bit compressed (3:2) / 2414 | 100% exact, max/mean 0 DN; 24,240,576 samples | native |
| ILCE-7M3 | 14bit 14bit uncompressed (3:2) there does not seem to be uncompressed 12 bit / 2418 | 100% exact, max/mean 0 DN; 24,240,576 samples | native |
| DSC-RX1 | 14bit 14bit compressed (3:2) / 2647 | 100% exact, max/mean 0 DN; 24,216,432 samples | native |
| DSC-R1 | 14bit 14bit uncompressed (3:2) / 3221 | 100% exact, max/mean 0 DN; 10,236,400 samples | native |
| ILCE-7RM3 | 14bit 14bit uncompressed (3:2) PixelShift ARQ / 3255 | 100% exact, max/mean 0 DN; 127,169,280 samples; RGBG green mean projected to RGB | native |
| ILCE-7RM4 | 14bit 14bit compressed (3:2) / 3480 | 100% exact, max/mean 0 DN; 61,005,568 samples | native |
| ILCE-9M2 | 14bit 14bit compressed (3:2) 14bit / 3989 | 100% exact, max/mean 0 DN; 24,240,576 samples | native |
| ILCE-1 | 14bit (4:3) AF-S mode, Aperture priority at f/8. Lossless Compressed raw full frame. In camera +1 exposure. / 4466 | 100% exact, max/mean 0 DN; 50,089,440 samples | native |
| ILCE-7SM3 | 14bit 14bit compressed (3:2) Full Frame / 5418 | 100% exact, max/mean 0 DN; 12,121,088 samples | native |
| DSC-RX100 | 12bit 12bit compressed (3:2) / 5811 | 100% exact, max/mean 0 DN; 20,181,312 samples | native |
| ILCE-7RM5 | 14bit (4:3) LJpeg w/ 2x2 MCU / 6237 | 100% exact, max/mean 0 DN; 26,216,960 samples | native |
| ILCE-7M4 | 14bit (4:3) Full frame raw "lossless-large" / 6929 | 100% exact, max/mean 0 DN; 32,947,264 samples | native |
| ILCE-7M4 | 4:3 Full frame raw "lossless-medium" / 6930 | 100% exact, max/mean 0 DN; 42,725,760 samples | native |
| ILCE-7M4 | 4:3 APS-C LossLess-Small / 6935 | 100% exact, max/mean 0 DN; 24,724,512 samples | native |
| ILCE-9M3 | 14bit (3:2) compressed (lossless, large) / 7362 | 100% exact, max/mean 0 DN; 24,168,240 samples | native |
| ILCE-1M2 | 14bit lossless (4:3) Lossless compressed Large-Size full-frame Sony Alpha1 II / 7828 | 100% exact, max/mean 0 DN; 53,477,376 samples | native |
| ILCE-7M5 | 14bit (4:3) full frame compressed lossless / 8845 | 100% exact, max/mean 0 DN; 36,700,160 samples | native |
| ILCE-7M5 | 14bit (3:2) full frame compressed hq / 8846 | 100% exact, max/mean 0 DN; 33,003,520 samples; pinned master | native |
| ILCE-7M5 | 14bit (3:2) full frame compressed / 8847 | 100% exact, max/mean 0 DN; 33,003,520 samples; pinned master | native |

## 3. Sony Pixel Shift

Library Photo > Photo Merge and the grid context menu expose Sony Pixel Shift.
The existing preview, progress/cancel, unique DNG filename, import, selection,
auto-settings and stack flow are reused. One composed ARQ or a complete 4/16
original capture is accepted. Sony group/count/shot metadata determines order;
missing/repeated shots, ordinary bursts, mixed groups/cameras/exposures/calibration
and incompatible crops are rejected. Original CFA observations are decoded with
static calibration before per-frame PDAF interpolation or saturation estimation.
Ordinary single-photo development keeps its normal repairs.

Four shots provide measured full RGB with two-green averaging; sixteen shots
interleave the published half-pixel phases and double both dimensions. Full
crops remain intact when active-area neighbours exist. Fusion precedes source
resizing. Motion uses repeated-green disagreement and first-shot local red/blue
ranges with a four-sigma variance threshold. Existing whole camera/ISO noise
profiles retain their channel ratios and adapt amplitude from bounded native-CFA
noise measurements; unavailable profiles use the existing robust estimator plus
sample-quantization floors. The dilated mask uses the first shot's existing
noise-adaptive Auto demosaic, with the correct half phase. No new demosaic or
hardware-tuned thread count was added.

Output is a 32-bit floating LinearRaw DNG. It retains finite negative/fractional
shadow values, normalization-aware colour/forward matrices, vendor WB converted
to AsShotNeutral, profile tone data, baseline exposure, orientation and capture
metadata. Sony correction curves also round-trip as original numeric tags.
Already-composed ARQ is a conversion; the UI states that motion correction
requires original shots.

Real CC0 A7R III shots 3251–3254 and Sony ARQ 3255 are SHA-pinned under
`target/sony-dji/pixelshift`. Independent direct sensor extraction on a 128²
crop matches all **49,152 RGB values exactly**, maximum/mean 0 DN, proving the
four-shot order. Final normalized Rust fusion compares **43,479 channels in
14,493 static pixels**, maximum **7.45e-9**, mean rounded **0.000000000**;
**1,891 pixels (11.5417%)** use conservative motion fallback. The metric does
not misrepresent fallback pixels as exact measured fusion. Independent synthetic
4/16 scenes yield maximum camera-RGB errors **3.0e-8 / 6.0e-8** and exercise
subtle 5-DN green motion, blue-only motion, half-phase fallback, WB/matrix/profile
round trip, negative/fractional FP32 storage, invalid captures and cancellation.

No real sixteen-source CC0 set was found in the current repository. Its physical
sequence is verified against published Sony/make_arq mapping and independent
synthetic sampling. Motion with indistinguishable local colour statistics and
arbitrary camera jitter/rotation cannot be guaranteed; ambiguous chromatic
texture may trade multi-shot detail for the first-shot fallback. These are
explicit limits of the implemented conservative method, not claims of complete
motion reconstruction.

Memory admission uses the existing shared HeavyJobs profile/live headroom,
checked dimensions, sequential source decode and estimated scratch/output/encode
storage. TIFF allocation counts use actual integer/float widths; large buffers
stream chunks under the caller's scoped memory allowance. Standalone decode
retains its defensive 1 GiB default. A smaller machine receives an explicit
memory estimate instead of a host-specific tuning or silent resolution loss.

## 4. Sony metadata and lens handoff

Standard EXIF lens/body serial fields remain primary. The Metadata panel adds
separate internal serial, shutter count, Creative Style/Look, recorded profile
gamma, focus mode/area, valid flexible-spot positions, capture sequence/Pixel Shift
and documented numeric lens ID when no exact model name exists. A PP slot is
not inferred from gamma because Sony slots can be customized. Unknown packed
layouts/electronic-shutter offsets remain unset rather than borrowing another
body's layout. Old Metadata/catalog/source documents load through serde defaults;
old stored photos need the existing Reload/Relink to populate new fields.

Pinned complete ExifTool 13.40 and snapshot 13.59 supply independent references.
Five CC0 generations and the owner's authorized local test shots match the
covered fields. Latest A7 V layout shifts are explicitly handled; the older
reference misread them. Owner-derived numbers and identifiers stay in ignored
reference caches and do not enter source, reports, images or uploads.

Signed Sony raw SubIFD tags 0x7037/0x7035/0x7032 are read only after matching
count-prefix validation. Distortion, red/blue CA and vignette arrays stay whole
in typed metadata and `SourceInfo.sony_lens`; DNG conversion preserves them as
original numeric tags. Camera-control maker-note settings are not mistaken for
correction curves. The sampled optics renderer belongs to the parallel
lens-corrections workstream: this change hands off the exact data and does not
fit lossy polynomials or duplicate that implementation.

## 5. DJI DNGs and telemetry

Nine real DNGs cover the available CC0 Mavic 3/Hasselblad, Mavic 3 Pro telephoto,
Mavic 4 Pro telephoto and Osmo Action cases, plus public reference-only Air 3/3S
and Mini 3/4 Pro samples. The latter are explicitly **not CC0** and are not
redistributed: archive/member/source hashes and rights are recorded separately.
All downloaded/derived data stays under target. Their 48 MP DNGs declare a 2×2
stored Bayer CFA: camera remosaicing is respected rather than inventing a 4×4
pattern from marketing sensor terminology. Avata 2's official specifications
list JPEG-only photos, so no nonexistent Avata 2 raw decoder is claimed.
Pocket/raw variants without an available permitted sample remain unmeasured.

The Mavic 3 file underreports its terminal StripByteCounts as 41,775,360 bytes,
while the complete 5,376×3,956×2-byte image (42,534,912 bytes) is present exactly
through EOF. The embedded JPEG confirms the declared row layout; reinterpreting
its row width as ActiveArea width produces severe striping. Native recovery now
uses the complete buffer only for an uncompressed terminal strip whose actual
remaining bytes exactly match declared dimensions. Truncated data or ambiguous
trailing bytes remain errors. This fixes the initial maximum **62,227 DN**, mean
**113.150345 DN**, **98.214450%** agreement near the missing declared tail.
The original file is unchanged; before/after ledgers and the layout diagnostic
stay under target. The new regression covers header/full recovery, real EOF
truncation and refusal to consume an ambiguous trailer.

GainMap now evaluates pixel centres, as Adobe's SDK does, with singleton-axis
handling and finite parameter validation. WarpRectilinear uses the SDK's
half-open bounds and coordinate convention. The raw/develop boundary preserves
list order, multiple operations and camera-plane CA. DJI built-in operations
run in camera RGB before colour conversion even with editable profile corrections
off, preventing omitted geometry and wrong GainMap/warp ordering. They are not
applied again in working RGB. Analytic SDK vectors cover non-central warp,
vignette and gain coordinates. A 64² grid per real gain map measured previous-to-
corrected gain changes up to **0.0576888%** (Mavic 3), with a per-file ledger at
`target/sony-dji/gain-map-coordinate-difference.tsv`; this is an operation-domain
coordinate correction, not a fabricated visual ΔE score.

DJI namespace URI is canonicalized independent of prefix. Absolute/relative
altitude, gimbal and flight yaw/pitch/roll, GPS and XYZ speed are typed separately;
original scalar properties (including RTK/calibration/unknown names and DJI
spelling aliases) remain verbatim for export. Standard EXIF GPS wins over DJI
fallback coordinates; DJI altitude is not relabelled as EXIF sea-level altitude.
Rendered metadata and Original/DNG sidecars retain telemetry under existing
export policy. Camera-stripping policies suppress it; Remove Location suppresses
the namespace, including unknown position properties. Existing original-export
refusal handles embedded positions that cannot safely be removed.

Installed darktable 5.6.2 / RawTherapee 5.13 supply bounded external RGB diagnostics:
Mini 4 Pro renders in both; RawTherapee also renders the tested Mavic 3, Mavic 3
Pro and Air 3S. Darktable rejects Mavic 3 with its rawspeed buffer error and
Mavic 3 Pro/Air 3S with unsupported lossless-JPEG predictor 6. Errors/renders are
cached; they are not disguised as CFA agreement. Sony 2414 renders in both, new
ARW6 does not. Independent unpacked metrics come from LibRaw, not RGB exports.

**All nine cases now match exactly after black subtraction: 291,474,048 CFA samples, maximum/mean 0 DN.** Native stays first; rawler still rejects the incorrect Mavic 3 strip count. Three real engine import/source cases preserve both embedded colour matrices and WB, develop finite images, keep built-in operations before colour conversion, and avoid applying them twice.

| Body / sample | ID / storage | Native vs LibRaw after black | Embedded operations |
|---|---|---|---|
| Osmo Action | 3214: uncompressed | 100% exact, max/mean 0 DN; 12,000,000 CFA samples | FixVignetteRadial → WarpRectilinear → GainMap |
| Mavic Air 2 / FC7303 | 4785: uncompressed | 100% exact, max/mean 0 DN; 12,000,000 CFA samples | FixVignetteRadial → WarpRectilinear → GainMap |
| Mavic 3 / L2D-20c | 5140: uncompressed, terminal count recovery | 100% exact, max/mean 0 DN; 20,887,680 CFA samples | GainMap → WarpRectilinear |
| Mavic 3 Pro Cine telephoto / FC4382 | 7823: lossless JPEG predictor 6 | 100% exact, max/mean 0 DN; 12,192,768 CFA samples | WarpRectilinear |
| Mavic 4 Pro telephoto / FC9287 | 8041: lossless JPEG | 100% exact, max/mean 0 DN; 37,748,736 CFA samples | WarpRectilinear |
| Air 3S | air3s: lossless JPEG predictor 6 | 100% exact, max/mean 0 DN; 50,331,648 CFA samples | WarpRectilinear |
| Air 3, 48 MP remosaiced | air3: uncompressed | 100% exact, max/mean 0 DN; 48,771,072 CFA samples | GainMap → WarpRectilinear |
| Mini 3 Pro, 48 MP remosaiced | mini3pro: uncompressed | 100% exact, max/mean 0 DN; 48,771,072 CFA samples | GainMap → WarpRectilinear |
| Mini 4 Pro, 48 MP remosaiced | mini4pro: uncompressed | 100% exact, max/mean 0 DN; 48,771,072 CFA samples | GainMap → WarpRectilinear |

## Validation, compatibility and scope

Final formatting, offline check and strict all-target clippy passed for all
**nine changed crates**: `lightcraft-raw`, `lightcraft-meta`, `lightcraft-merge`,
`lightcraft-develop`, `lightcraft-pipeline`, `lightcraft-catalog`,
`lightcraft-engine`, `lightcraft-ui-egui`, and `local-image`. Commands used
`cargo +1.98.1`, the same retained target, the launcher's unchanged
`CARGO_BUILD_JOBS=4`, PATH with rustup first, and one Cargo lane. Python reference
scripts compile and pinned sample rights/hashes were checked.

**43 unique new focused tests passed**, including the ignored real-sample tests
with their required local environments. No full crate/workspace suite ran.

| Changed area | New focused tests | Coverage |
|---|---:|---|
| Raw | 20 | Eight Sony codec/calibration tests, four ARW6 protocol/edge/invalid-input tests, three real-reference goldens, SDK coordinates, bounded streaming/budget restoration, Sony-curve/DJI-XMP DNG roundtrip, MRI metadata/entropy isolation, terminal-strip recovery/refusal |
| Metadata | 8 | Layout/cipher and both byte orders, unknown layouts/capture groups, signed complete curves, DJI aliases/roundtrip/nonfinite values, standard-GPS precedence/datum, five CC0 bodies and authorized local owner reference fields |
| Merge | 7 | Independent 4/16 physical scenes, integer subtle-green and blue-only motion, correct Auto/half-phase fallback, invalid capture/cancel, WB/matrices/profile and negative/fractional FP32 DNG, real four-shot/ARQ comparison |
| Engine | 5 | Ordinary-burst refusal/no output, adaptive-memory arithmetic, JPEG/sidecar export policy, opcode handoff/origin, three real DJI import/develop sources |
| Library UI | 2 | Actual clicks on merge overlay/stack controls in both themes/widths and Metadata disclosures/lens ID; four rendered screenshots inspected |
| Pipeline | 1 | Signed Sony data handoff and old SourceInfo serde defaults |

Requested bounded decoder comparisons also exercised the existing diagnostic
entry point: eighteen initial Sony files, the two changed M/S files, nine DJI
files and the changed Mavic 3 case after strip recovery. Their observations and
reference versions are reported per format above; no timing benchmark ran.

Failures were retained and investigated: invalid synthetic ARW6 height fixtures
were independently rejected by the pinned C++ implementation, then corrected
with valid vectors plus explicit invalid checks; the old MRI guard misparsed
real entropy; a metadata and a terminal-strip fixture initially used the TIFF
writer's data-before-directory layout incorrectly; the first motion classifier
needed phase-aware colour brackets and measured profile amplitude; clippy first
picked Fedora's system driver against rustup metadata, then reported five
idiomatic ARW6 style issues. Matching the prescribed PATH and applying the lint
suggestions resolved the latter. No test/lint was disabled or weakened, target
cache was not swept, and the source reference goldens were not blindly rebuilt.

Logs and all initial failures remain in `target/sony-compare`,
`target/sony-ycbcr-compare`, `target/sony-dji-validation` and
`target/sony-dji/pixelshift`. `final-validation-status.json` records all eighteen
check/clippy commands with exit zero. GPU tests added/run: **0**, following the
explicit minimum-testing override. Attribution generation passed and `assets/attributions.json` includes the new
upstream pins and matching licence notices (516 code-port entries, unchanged
779 Rust package records). New upstream notices/pins/update instructions are
recorded. No old attribution or broader test suite was run under the minimum
testing rule.

No GUI window, running app, ComfyUI job or owner's Desktop folder was opened or
altered. Authorized owner test shots were read only; private references stay
under target. No GPU test or performance claim was added, per the final owner
minimum-testing rule. Interactive speed remains unmeasured; full required DJI
operations currently retain the full camera-RGB path before downscaling.

Unavoidable outside-area changes are shared SourceInfo/SonyLensData (lossless
optics handoff), catalog optional metadata fields, engine import/source/export/
sidecar/merge wiring, Library menus/panel/shared-widget use, host ARQ/SR2 picker
extensions, scripts/manifests, provenance/licences and generated attributions.
No dependencies or Cargo.lock packages changed. No unsafe code, C/-sys build
integration, model routing allow-list, Match Look edits or commits were added.
The user's pre-existing task-file modification was retained untouched.

Sources and update procedures: `docs/RAW-DECODERS.md`, `docs/PORTS.md`,
`licenses/libraw-NOTICE.md`, `licenses/exiftool-NOTICE.md`,
`licenses/make_arq-NOTICE.md` and the RawTherapee notice. External source/tool data
is pinned whole; imported camera/profile data was not edited.

- [LibRaw 0.22.2 Sony calibration](https://github.com/LibRaw/LibRaw/blob/0.22.2/src/metadata/sony.cpp) and [pinned ARW6 source](https://github.com/LibRaw/LibRaw/blob/830ac963b40223c6a25add5be65036fa8d56e2d6/src/decoders/sony_arw6.cpp).
- [Sony Pixel Shift instructions](https://support.d-imaging.sony.co.jp/app/imagingedge/en/instruction/1_2_about_psm.php), [Sony A7R V processing guidance](https://support.d-imaging.sony.co.jp/support/ilc/psms/ilce7rm5/l/index.php), [pinned make_arq](https://github.com/agriggio/make_arq/blob/912069a8e6d3c6738cd76ba0fe6c62ed4d314fe3/make_arq.py).
- [ExifTool Sony layouts](https://github.com/exiftool/exiftool/blob/2200871d9cef988051d2a99d67df3bda6cbb30a8/lib/Image/ExifTool/Sony.pm), [DJI namespace](https://github.com/exiftool/exiftool/blob/13.40/lib/Image/ExifTool/DJI.pm), [Sony customizable profile semantics](https://helpguide.sony.net/ilc/2040/v1/en/contents/TP1000416986.html), [Lightroom Metadata panel](https://helpx.adobe.com/lightroom-classic/desktop/organize-photos-in-lightroom-classic/metadata-basics-actions.html).
- [Adobe DNG specification and SDK](https://www.adobe.com/support/downloads/dng/dng_sdk.html); independent source coordinates checked in `dng_gain_map.cpp` / `dng_lens_correction.cpp`, cached under target/refvec.
- [DJI altitude/speed reference](https://dl.djicdn.com/downloads/p4-multispectral/20200717/P4_Multispectral_Image_Processing_Guide_EN.pdf), [Avata 2 official photo format](https://www.dji.com/avata-2/specs), [public supplemental DJI test samples](https://neukamp.de/dji-mini-4-pro-fotoqualitaet-im-vergleich/).
- [raw.pixls.us CC0 repository](https://raw.pixls.us/), [darktable CLI](https://docs.darktable.org/usermanual/development/en/special-topics/program-invocation/darktable-cli/), [RawTherapee CLI](https://rawpedia.rawtherapee.com/Command-Line).
