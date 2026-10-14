# Camera calibration data

Four whole upstream files live in `crates/lc-raw/data/cameras/`. They are embedded offline,
parsed once and indexed. `manifest.json` records the source path, immutable commit and SHA-256.
`python3 scripts/camera_data.py` verifies every byte; `--fetch` restores the pinned bytes.
No application runtime download, C library, generated subset or camera-source fork is involved.

To update, choose a reviewed upstream commit, download each **whole** file, change its revision
and hash in the manifest, and run the verifier. Review schema changes against RawTherapee
`camconst.cc`, rawspeed `Camera.cpp` / `CameraSensorInfo.cpp`, and darktable `noiseprofiles.c`.
Then run the camera-data tests, the corpus audit and paired denoise measurements before updating
this document, PORTS, notices and `cargo +1.98.1 run --offline -p xtask -- attributions`.
Keep the source files unmodified, including comments, copyright and all camera entries.

## Precedence and coordinates

DNG calibration, masked-area logic, crops and opcodes stay authoritative; these vendor tables do
not override DNG tags. Vendor native and rawler decodes both pass through the same calibration hook:

* White: RawTherapee nearest ISO (ties choose higher), aperture scaling and `white_max`, then
  rawspeed explicit ISO sensor, then decoder/file measurements, then a generic rawspeed sensor
  replacing a nominal bit-depth maximum. Tables outside the
  stored bit range follow RT rawimage.cc: each integer channel white is shifted right until
  it fits TIFF bits per sample (including 14-to-12-bit Nikon capture modes).
* Black: robust per-frame optical-black measurement from table masked areas, then RT black,
  then rawspeed ISO black, then decoder/rawler. RT's >65535 absolute flag is decoded with the upstream low-16-bit mask;
  plain Nikon/Panasonic values are offsets from the decoder black. Black measurement uses
  same-CFA-phase medians, excludes a four-pixel perimeter, requires 32 samples per phase, and
  refuses rectangles overlapping the active image. Black patterns are reanchored after crops.
* Crop: RT exact full-frame size, then RT fallback `[0,0]`, then rawspeed, then decoder fallback when no explicit decoder/file crop exists. Zero or
  negative extent means remaining sensor dimension minus the origin and right/bottom removal.
  Out-of-sensor crops are rejected. An explicit decoder/file default crop retains
  its original coordinate representation; table crops fill files without an explicit crop.
  A rawspeed entry whose sensor whites all exceed the stored bit range is an incompatible
  mode and supplies no crop. The CFA remains anchored at the full sensor origin.
* Matrices: embedded/file and the existing calibrated camera matrix table (including RT overrides)
  and rawler runtime fallback retain priority; rawspeed supplies otherwise missing three-plane D65
  matrices. Unknown cameras remain on the existing fallback.
* rawspeed camera modes: default mode first. Without a default, matching bit-depth modes are
  used only when all their sensor/crop/black-area/matrix calibrations agree. sRaw, DNG, CHDK and
  unidentified aspect/compression variants never inherit an arbitrary mode's calibration.

Four vendor white entries are R/G1/B/G2, not four samples per pixel. Full normalization, lazy
samples and superpixel previews use the correct sensor phase. A derived colour model adjusts
analog balance for normalized channel ranges (including phase black levels). The source file
calibration and as-shot multipliers remain unchanged; headers never inherit sample-derived gains.
Camera WB preset conversion and the live pipeline use that derived model, so unclipped illumination
does not shift. A sensor DNG write accepts equal channel whites as a scalar; unequal vendor CFA
whites are explicitly rejected rather than silently truncated to red, because DNG WhiteLevel is
per sample. A future normalized floating-point DNG conversion can represent that case.
`saturation.rs` retains the image pile-up audit and uses channel whites when lifting plateaus.
For vendor raws, a statistically strong lower plateau caps that channel's white as the final
per-frame fallback. It preserves unpacked samples and header metadata; the detector's existing
population, spike, upper-half, margin and stray-above tests are unchanged.
PDAF repair follows RT's repeated row pattern (the final pattern item is its period), green-triplet
outlier detection on a marked row and its neighbours, and immutable detection before same-colour
clean-row interpolation. It avoids replacing isolated edges or every pixel on a PDAF row.

## Denoise and WB

Noise lookup uses exact normalized maker/model names and linearly interpolates between surrounding
ISO measurements. Skipped profiles are omitted; fitted negative `b` values are retained. ISO outside
that measured interval uses the existing image estimator, rather than extrapolated coefficients.
The calibrated path adapts the fit amplitude with the existing robust high-pass estimator in
camera coordinates, retaining measured channel ratios. This accounts for demosaic/resampling and
per-frame departures from laboratory measurements. It passes the resulting three fits into the
existing wavelet algorithm's VST in sensor
camera coordinates. Its matrix carries as-shot WB, baseline gain and camera conversion and is updated
by the same global WB matrix as the pixels. Coefficients never masquerade as Rec.2020 noise.
The GPU app path uses the exact host variance transform and runs the existing precise GPU
wavelet/reduction/synthesis stages; generic NR keeps its existing GPU implementation. Source facts and render cache keys include calibration.
Baked nonlinear DCP/look-table sources retain the estimator: no safe inverse of those tables is
available. Nonlinear colour calibration and film-negative settings also retain the estimator.
This is a quality limitation, not an assertion of parity with darktable's earlier NR stage.

The WB menu includes each camera's zero-tuning preset names, measured multipliers converted through
its camera colour model. Named generic presets use camera measurements when available; their old
Kelvin/tint values remain fallbacks. The command also accepts `preset: "NAME"`. The resolved Kelvin
and tint are stored as existing Custom settings, so old catalogs/settings load without migration;
matching values show the camera preset name again. Unsupported/relative-WB sources keep the
established menu. Upstream tuning variants are retained in the data and lookup API, but the UI exposes
zero-tuning presets; temperature/tint sliders remain the adjustment controls.

The interaction follows Adobe's official [Lightroom Classic white-balance help](https://helpx.adobe.com/ie/lightroom-classic/desktop/process-and-develop-photos/image-tone-color.html):
named presets, editable temperature/tint and Custom on adjustment. It uses the shared dropdown and
existing menu rows, theme tokens and the existing pointer-input test harness.

## Validation

`cargo +1.98.1 run --offline -p lightcraft-raw --features testgen --example camdata_audit -- target/rawdecode/*`
prints JSON lines with original/calibrated levels and crops, saturation pile-ups, and clipped-region
hue/chroma censuses. Membership is taken from calibrated sensor clipping and sensor-relative bins are
identical in both images; default crop changes are separately recorded. This is a calibration audit,
not a universal image-quality score: colored or partly clipped bins need not become neutral.
See `docs/wip/CODEX-REPORT-camdata.md` for measured results, reference versions and limitations.

Full CC0 repository evaluation: build the audit with `--features testgen`, copy its executable to
`target/camdata-corpus/camdata_audit`, then run `python3 scripts/camera_corpus.py`. Downloads
stream to target/ and verify the repository SHA-256; `--download-only` and `--audit-only` split
those stages. Logs distinguish decoder errors from calibration/header failures. The highlight
census uses valid-image sensor bins, reanchors black phases/deltas and separates full from partial
clipping. Numeric summary: `docs/wip/CAMDATA-corpus-metrics.json`.
