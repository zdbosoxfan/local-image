# Camera profiles

Local Image reads DNG camera profiles (`.dcp`) from your own folders. Use a profile
for the camera body it was measured for; a lens profile (`.lcp`) corrects lens
geometry and shading, and does not replace a camera colour profile.

## Load Adobe or custom profiles

1. Install Adobe DNG Converter or Camera Raw using your own Adobe installation.
2. Open **Settings → Import → Camera profiles folder**. **Choose…** selects a
   folder containing DCPs and LCPs; subfolders are searched. **Reload Profiles**
   rescans it after adding or replacing files.
3. Open a raw photograph in **Develop → Camera Profile**. The picker lists
   compatible profiles in **Adobe**, **Camera Matching**, and **Custom Camera
   Profiles** groups, alongside **Automatic**, **Matrix Only**, and a compatible
   bundled profile. The DCP's description supplies its display name.
4. Select your profile and check a neutral object and skin in the photograph.
   Profile choice is saved with the develop settings. Missing profiles fall back
   gracefully; reconnect the folder and select the profile again when available.

Standard Adobe locations searched on their native platforms:

| Platform | Shared camera / lens profiles | Personal camera / lens profiles |
|---|---|---|
| Windows | `C:\ProgramData\Adobe\CameraRaw\CameraProfiles` / `LensProfiles` | `%APPDATA%\Adobe\CameraRaw\CameraProfiles` / `LensProfiles` |
| macOS | `/Library/Application Support/Adobe/CameraRaw/CameraProfiles` / `LensProfiles` | `~/Library/Application Support/Adobe/CameraRaw/CameraProfiles` / `LensProfiles` |

On Linux, select the folders in your own Adobe/Wine installation explicitly.
Local Image references the files in place. Adobe DCPs and LCPs are not included in
our distribution, copied into the repository, or uploaded. Keep that folder
available when moving a catalog to another computer. Adobe's profile browser and
Camera Matching concepts are described in [Camera Raw's official profile help](https://helpx.adobe.com/camera-raw/desktop/edit-and-enhance-images/tone-and-color/adjust-color-rendering-camera-camera.html).

## Build a dual-illuminant DCP for each body

The profile belongs to a **specific body**, not just the model. Name it with the
body serial number or your inventory name. Use an actual ColorChecker Classic,
Passport, or Digital SG; a screen or printed imitation cannot substitute for a
measured reflective target. These steps follow [DCamProf's author documentation](https://rawtherapee.com/mirror/dcamprof/dcamprof.html)
and [RawTherapee's profiling workflow](https://rawpedia.pixls.us/how_to_create_dcp_color_profiles/).

1. Run **DCamProf externally**. It is GPL-3.0 C software, available from
   [Beep6581/dcamprof](https://github.com/Beep6581/dcamprof); it is never a dependency
   of Local Image. Install ArgyllCMS (`scanin`) and a raw converter for linear
   camera-RGB exports. Pin the tool version and keep its original license/data.
2. Shoot the chart under stable tungsten (approximately Standard A, 2856 K) and
   separately under daylight. Use the same body, lens, base ISO and chart. Fill
   enough of the frame for clean patch interiors. Avoid glare, mixed lighting,
   channel clipping and deep underexposure. Shoot a plain white card at the same
   position for optional flat-field correction. Take separate validation frames.
   An LED's colour temperature alone does not describe its spectrum; use a
   measured spectrum for difficult lighting.
3. Export each raw as a **16-bit linear, unwhite-balanced camera-RGB TIFF**:
   disable colour transforms, curves, denoise, sharpening, clipping recovery and
   lens adjustments. RawTherapee's **Export for profiling** workflow is suitable.
   Verify it really exports sensor RGB: a disabled-looking UI control or an
   arbitrary “none” output setting is not sufficient. Crop and rotate the target
   without reducing bit depth. Retain the original raw.
4. Select the reference for the physical chart. DCamProf's `cc24_ref.cie` is for
   pre-November-2014 Classic charts; `cc24_ref-new.cie` is for newer ones. For SG,
   use the chart's corresponding reference and layout files. Prefer a measured
   reference for your own chart. Run `scanin` and inspect its diagnostic TIFF to
   ensure every ROI lies inside the intended patch:

   ```sh
   scanin -v -p -dipn tungsten.tif ColorChecker.cht cc24_ref-new.cie
   scanin -v -p -dipn daylight.tif ColorChecker.cht cc24_ref-new.cie
   ```

   Use `ColorCheckerPassport.cht` for Passport. Correct nonuniform illumination
   with `dcamprof testchart-ff` before sampling if you captured a white-card shot.
5. Build one native calibration per light, then merge the **lower-temperature
   profile first**. In this example daylight was D65; use D50 instead if that is
   what your illumination was. With measured light, pass its spectrum file to
   `make-profile -i`; choose corresponding standard light codes with `make-dcp`
   `-i` / `-I` when constructing a dual DCP:

   ```sh
   dcamprof make-profile -i StdA tungsten.ti3 tungsten.json
   dcamprof make-profile -i D65 daylight.ti3 daylight.json
   dcamprof make-dcp -n "SONY ILCE-1" -d "A1 body 01 accurate" -t linear \
     tungsten.json daylight.json A1-body-01-accurate.dcp
   ```

   Set `-n` to the exact camera model understood by the converter, not the body
   nickname. Keep the JSONs, chart references, raw captures, spectrum and command
   log next to the profile. `-t linear` gives an accuracy-oriented starting point;
   create a separately named rendering variant with `-t acr` if you want a tone
   curve. The tone curve of a profile picked under **Custom Camera Profiles** is
   used by the **Camera** look; Automatic, the bundled profile and a picked
   profile that is no longer in the folder keep the camera's own curve, and Soft
   Film and Sigmoid keep their own curves. Leave LUT smoothing at its defaults until validation shows a reason
   to change it; a perfect fit to one chart can produce poor gradients elsewhere.
6. Put the resulting DCP in your chosen **Camera profiles folder**, click
   **Reload Profiles**, and select its description under **Custom Camera
   Profiles**. Validate tungsten, daylight and intermediate-light raw frames.
   Keep independent validation shots out of the calibration fit.

## Measure colour accuracy

Run the small harness from the repository root, using CC0 test captures under
`target/` or your private chart captures kept outside the repository. The example
uses the real raw decoder, RCD, camera transform and DCP HueSatMap. It omits tone
curves, look tables, baseline exposure, highlight repair and clipping so that
creative rendering does not conceal calibration errors:

```sh
PATH="$HOME/.cargo/bin:$PATH" cargo +1.98.1 run --offline -p lightcraft-raw \
  --example profile_chart -- chart.arw rois.csv profile.dcp > patches.csv
python3 scripts/profile_chart_deltae.py patches.csv cc24_ref-new.cie > results.json
```

`rois.csv` has `patch,x,y,width,height`, with fractions of the **oriented developed
image**. Use DCamProf IDs (`A01`–`D06`) for the Classic 24-patch chart; inspect the
ROI overlay before accepting measurements. The metric matches one exposure
scalar to neutral D05, converts XYZ D50 to Lab, and calculates CIEDE2000 with
unit parametric factors. It reports mean, median, maximum and the two skin patches.
It does not optimize patch colours. The target is **mean ΔE2000 < 2.5** on a
separate validation capture; also inspect skin, grays, gradients and the worst
patch. SG targets need their own reference and ROI IDs, and the current script's
Classic skin/neutral IDs must be adapted explicitly.

For an external comparison, render the same capture headlessly with RawTherapee,
RCD, the same DCP and camera white balance. Disable DCP tone curve (`ToneCurve`),
look table and baseline exposure; enable HueSatMap; export an uncompressed
16-bit TIFF with `RTv4_sRGB`. The companion reads the actual embedded ICC matrix
and transfer curves instead of assuming linear TIFF values:

```sh
python3 scripts/profile_chart_reference.py reference.tif rois.csv > reference-patches.csv
python3 scripts/profile_chart_deltae.py reference-patches.csv cc24_ref-new.cie
python3 -m unittest discover -s scripts/tests -p test_profile_chart_deltae.py -v
```

The metric's hue-wrap and zero-chroma cases are checked against
[Sharma, Wu and Dalal's published CIEDE2000 test pairs](https://hajim.rochester.edu/ece/sites/gsharma/ciede2000/).

## Small-sample evidence and limits

The 2026-10-14 check used raw.pixls.us **3013**, a CC0 Sony DSC-RX100M6
ColorChecker capture (source SHA-256
`3015022b00ce219bf2713d9de21608e631566d111c4468402748b794e312d94c`).
Its unmodified source, ROIs, reference PP3, TIFF, patch CSVs and JSON measurements
are under `target/colorprofiles-quality/`. Chart reference was the verbatim
DCamProf `cc24_ref-new.cie` from commit
`bffb659d6a0af82f7b6b2e6d0006d1824ac4d398`. The reference is nominal: the captured
chart's manufacturing date, measured reflectances and illuminant spectrum are
unknown.

| Render, as-shot WB | Mean ΔE2000 | Maximum | Mean of two skin patches |
|---|---:|---:|---:|
| Current matrix | 2.7297 | 7.1146 | 3.5019 |
| Bundled CC0 RX100M6 DCP, Local Image | 3.2265 | 4.8741 | 4.1925 |
| Same DCP, RawTherapee 5.13 | 3.2869 | 4.8935 | 4.2132 |

Local Image's DCP output agrees with the external evaluator at mean ΔE2000
**0.07881**, maximum **1.17548**, after the same neutral exposure matching.
This supports evaluator correctness on this sample. It does **not** establish
that the DCP improves accuracy: here it reduced the largest error but worsened
the mean and skin errors, and the requested mean below 2.5 was not achieved.
A profile must be selected by measured validation, and this single chart is not
a camera-wide or dual-illuminant acceptance result.

An additional **single-light test calibration** was actually generated externally
with DCamProf 1.0.6 (the pinned commit above), from 16 camera-RGB patch means
exported by `profile_chart … camera`. Eight chromatic patches were withheld from
both matrix and LUT fitting: A01, A02, A03, B01, B03, B06, C02 and C05. Both skin
patches were withheld. The generated DCP, **RX100M6 test chart held-out**, has
SHA-256 `80bd925ea2206eaca5d107a10e6e638241902bb1efca4a2d5f7c511e4a6c6050`;
Local Image successfully read and applied it.

| Test custom DCP | All 24 patches mean | Eight held-out patches mean | Held-out maximum |
|---|---:|---:|---:|
| Local Image | 2.9894 | 3.1172 | 3.9779 |
| RawTherapee 5.13 | 3.0715 | 3.1214 | — |

Local Image versus RawTherapee with this custom DCP had mean ΔE2000 0.12125,
maximum 2.20779. Fit-only mean was 2.9255 and skin mean 3.6479 in Local Image.
D50 was an explicit approximation for the unknown illumination; this is a
harness/import check, not a certified body calibration. Withholding patches in
one photograph does not substitute for an independent validation capture.
Neither the held-out nor full-chart result meets 2.5. Use the dual-light capture
recipe above to supply the missing measured lighting and chart reference before
claiming that accuracy target for an owner's body.

DCamProf source, unmodified Debian development-header packages, extracted headers,
local linker symlinks, build log and provenance are under `target/refvec/dcamprof/`.
Only the external tool uses C/lcms/libtiff; the Rust build has no new dependencies.
Calibration TIFF export via arbitrary RawTherapee `(none)` settings was rejected
when its gray patches showed clipped zero red; only Local Image's explicit
camera-RGB output was used in the reported fit. Intermediate files and the
failed export remain under `target/` for review.

The successful external calibration commands were:

```sh
# camera mode emits R/G/B in the CSV's X/Y/Z columns, before colour transforms.
profile_chart rx100m6-chart.arw rois.csv SONY_DSC-RX100M6.dcp camera > camera-patches.csv
# fit.ti3 pairs the 16 included sensor means (scaled to 0–100) with the
# unmodified chart reference rows; withheld IDs are removed, not assigned zeros.
dcamprof make-profile -i D50 fit.ti3 custom.json
dcamprof make-dcp -n "SONY DSC-RX100M6" -d "RX100M6 test chart held-out" \
  -t linear custom.json custom.dcp
```

Fit and reference are retained under `target/colorprofiles-quality/`. DCamProf
reported a different optimal target neutral from the camera's as-shot white
balance. The recorded numbers intentionally retain as-shot WB for all three
renderers. For your own profile's independent validation, measure WB from a
neutral patch under the validation light and record it alongside the measurements.

### Chart-neutral white balance follow-up

To measure the same standard step as a gray picker, append `neutral` to the
example command. It averages **only D05 camera RGB**, sets that neutral before
evaluating each matrix/profile, and leaves the colour patches untouched:

```sh
profile_chart chart.arw rois.csv profile.dcp neutral > neutral-patches.csv
python3 scripts/profile_chart_deltae.py neutral-patches.csv cc24_ref-new.cie
```

The example preserves the raw file's AnalogBalance, CameraCalibration and
calibration signature when overlaying a DCP. In neutral mode it overrides the
as-shot neutral, rather than discarding camera calibration. Exposure is still a
single D05 scalar. This gray was already part of the 16 fitting patches; the same
eight chromatic patches remain held out.

For an equivalent external check, RawTherapee's two global WB controls were
solved using **only the D05 XYZ chromaticity**, targeting D50. The saved PP3 uses
**5298 K**, **Green 1.03288411**, **Equal 1**, identically for both profiles. The
small reference-only solve used 13 headless renders; it never consulted a
chromatic patch or optimized the reported score. Temperature's integer resolution
left residual chromaticity ratios of about 2×10⁻⁶ (X/Y) and −5×10⁻⁵ (Z/Y).
Solver source/log and neutral TIFFs are under `target/colorprofiles-quality/`.
The nominal chart reference's D05 has a slight intrinsic tint, so an exact gray
picker does not imply zero ΔE to that nominal patch.

The measured D05 camera neutral was `[0.419630904, 1, 0.537576071]`, giving
multipliers `[2.3830466, 1, 1.8602018]`. Both the matrix and both DCP evaluations
used these same sensor-channel ratios. Results from this follow-up:

| Render, D05-neutral WB | All 24 mean ΔE2000 | Maximum | Eight held-out mean | Two skin patches mean |
|---|---:|---:|---:|---:|
| Current matrix | 2.7363 | 5.6819 | 2.6751 | 3.7581 |
| Bundled DCP, Local Image | 2.9768 | 4.5942 | 3.4366 | 3.9237 |
| Bundled DCP, RawTherapee | 3.0200 | 4.6066 | 3.4607 | 3.9416 |
| Custom DCP, Local Image | 2.7928 | 4.8858 | 3.3151 | 4.1895 |
| Custom DCP, RawTherapee | 2.8736 | 4.9703 | 3.3368 | 4.2415 |

With neutral WB, Local Image versus RawTherapee measured mean/max ΔE2000
**0.10360 / 0.33972** for the bundled profile and **0.14050 / 1.36966** for the
custom profile. This improves agreement on the worst bundled-reference patch,
but no full-chart or held-out mean meets **2.5**. The custom profile's full-chart
mean improves while its held-out skin errors increase; full-chart means alone
must not be used to select it for portraits. The original as-shot table remains
above because white balance is a separate user choice. The new exact measurements
are in `neutral-comparison.json` and the `*neutral*` CSV/JSON files under `target/`.

Lens-source precedence is **embedded vendor/DNG → lensfun → user LCP**. The
selected LCP remains available in the picker but applies only when no higher
profile source is active. Profiles are selected as a whole; CA and vignetting
never fill gaps by stacking another provider. The master Enable Lens
Corrections switch controls editable profiles. Built-in ordered DJI DNG
operations remain part of raw development and suppress all automatic fallbacks.
LCP CA honours Remove Chromatic Aberration, uses the same camera-plane sampling
basis as embedded CA, and suppresses blind automatic CA estimation when active.
LCP retains its existing bounded CPU geometry path; embedded radial corrections
and lensfun remain native GPU geometry.
