# Match Look continuation and photographic acceptance

Research and implementation continuation, October 13, 2026. Development remains on
`wip/cx-matchlook`; `v2` is the integration target. The continuation includes v2 at
`9969623848982e344bbbf6fa2bc50c6ae799cdbb` and preserves prior Match Look work.
The owner resumed development and supplied two private photography sessions for testing.
The earlier paused status and ten-reference choice in historical plans are superseded.

## Feature behavior

Match Look should transfer the finished reference's color and tone into fresh editable
adjustments for each target. The primary workflow is edited Sony A1 ARW, merged linear
DNG or DJI/Hasselblad DNG to different RAW captures. A finished JPEG is also a valid
reference. Recovering a reference's original editing recipe is unnecessary.

Capture One documents four groups: Normalize (exposure and WB); Light and Contrast
(tone sliders, input levels and Luma curve); Color Adjust (saturation, Basic Color Editor,
B&W); Color Tone (master and tonal color balance plus RGB channel curves). Normalize is
independent of Impact. Applying on a filled layer leaves B&W global. Presets retain the
reference, selected adjustments and Impact and adapt individually to new images.
[Official Match Look documentation](https://support.captureone.com/hc/en-us/articles/22188770298269-Match-Look-Tool).
The official launch demonstration cited in the supplied research supports 20 references.

Clarity, texture, detail, geometry and reconstructed masks are outside global look
matching. The application still needs those editing capabilities to reproduce a finished
architectural product when the reference uses them. The controls exposed by Local Image
do not map one-to-one to every Capture One field; matching must be evaluated on rendered
photographs, with output settings editable in Local Image's existing tools.

## Research conclusions

Whole-frame histogram equality confounds editing with scene composition. A forest-heavy
aerial and a close facade view can share a look while having different color and lightness
distributions. The objective must reject absent reference content and compare supported
appearance groups or trustworthy corresponding regions.

| Source | Useful mechanism | Application limit |
|---|---|---|
| [darktable Color Mapping](https://docs.darktable.org/usermanual/development/en/module-reference/processing-modules/color-mapping/) | Chromatic clusters, color proximity versus dominance, independent tonal matching | Dominance still follows scene area; pixel transform does not supply editable RAW settings |
| [RawTherapee histogram source](https://github.com/RawTherapee/RawTherapee/blob/dev/rtengine/histmatching.cc) | Sparse luminance curve from populated CDF mapping; fallbacks and cache keyed to input profile | Its source/reference are the same capture, RAW versus embedded JPEG |
| [Automatic Content Aware Color and Tone Stylization](https://openaccess.thecvf.com/content_cvpr_2016/html/Lee_Automatic_Content-Aware_Color_CVPR_2016_paper.html) | Regularized color and monotone tone mappings; content-dependent reference selection | Statistical similarity remains distinct from matching actual surfaces |
| [Progressive Color Transfer](https://arxiv.org/html/1710.00756) | Correspondence and transfer refine together; semantic features help avoid unrelated pairings | Spatially varying pixel transforms cannot simply be substituted for global editor controls |
| [G'MIC author explanation](https://discuss.pixls.us/t/new-filter-colors-transfer-colors-multi/58847) | Joint color distributions, variational registration and explicit correspondence constraints | Final 3D LUT has no inherent semantic recognition and is not a slider solve |
| [Regularized Discrete Optimal Transport](https://arxiv.org/abs/1307.5551) | Relaxed transport addresses differing color-mode masses and regularization limits artifacts | Balanced transport can force unrelated materials to match |

The implemented opt-in experiment uses frozen source membership, robust conditional color/tone
statistics, capped influence per supported group, and conservative rejection of ambiguous
or absent groups. Reference blending should eventually operate per supported group.
It keeps full-frame clipping diagnostics and weak fallback statistics. Do not let the optimizer
improve its score simply by moving pixels between hue classes.

Optional pretrained correspondence can use standard DINOv2 features and mutual matches
without training. MIT POT supplies a partial transport baseline; MIT dino-vit-features
supplies correspondence examples; standard DINOv2 is Apache 2.0. This continuation does
not add these dependencies or weights. Their performance and architecture correspondence
quality require measurement. Noncommercial SCT code is not an acceptable unreviewed port.

## Current implementation changes

- Matching excludes clarity fitting, texture residuals and layer Effects installation.
- Color Tone solves three interior knots in each RGB channel, retains existing vertices,
  constrains moves to avoid reversals, and writes ordinary editable point curves.
- Channel curves participate in Impact and Color Tone reset independently of Luma/Light.
- Existing RGB-mode master curves retain their mode. Their Light fit uses Light sliders;
  a new layer can use its independent luminance-mode curve.
- B&W treatment is global in layer mode; the change summary follows that global value.
- Solver, analysis, saved-look validation, catalog migration and UI accept 20 references.
- WB Amount copies the solved preset/mode exactly instead of converting an unchanged
  tint to Custom mode.
- A normalization seed forces fresh derivative probes before fitting color; the probe
  budget scales with editable parameter count.
- An 8 by 8 lightness-rank descriptor supplies conservative structural agreement when
  color histograms differ because of editing. Flat/simple-gradient frames and low
  agreement keep the prior fallback; this is not semantic recognition.
- The full rendered tone objective retains reference quantiles. The curve-only slope
  projection is used for a warm seed and a legacy diagnostic, rather than redefining the
  reference's blacks/highlights. Physical endpoint targets are bounded. The misleading
  tonal-reachability chip and warning are removed.
- Opt-in batch consistency uses a shared selected-color solution and per-target
  normalization/light refinement. This app's explicit lock list is not a claim about
  Capture One's unpublished internal list.

The existing statistics/preset schema still stores analyzed evidence rather than original
reference imagery. Per-adjustment exclusions, saved reference previews and stronger
correspondence remain separate development work.

## Photographic acceptance

The owner's Kaufman Zum session provides 340 RAW/DNG files and 95 sRGB JPEGs. Session
process history directly maps 69 proofs and 22 numbered finals to source files. Private
manifests and rendered comparisons stay outside the repository.

The merged Sony HDRs are three-channel unsigned 16-bit LinearRaw DNGs, not Bayer or
sRGB. BaselineExposure is substantial and must be applied once. Drone DNGs identify as
Hasselblad L3D-100c, with lossless JPEG CFA samples up to about 100.7 MP. Embedded previews
must not replace native RAW tests. Finals use crop/perspective changes and local layers;
some interiors contain nine local layers.

Eleven sidecars contain a Match Look style record. The stored five-knot RGB curves and
color/tone controls provide concrete examples of editable results. Several are marked
modified, so coefficient differences do not establish proprietary solver behavior. Some
variants store their look in the SL record with little AL data: an AL-only reading loses
those edits. Proof export history does not retain historical settings snapshots.

Separate the following evaluations:

1. Native RAW/LinearRaw decoding, baseline exposure, color management and headroom.
2. Whether the editable global controls can represent a useful approximation of a final.
3. Same-source reference matching, with geometry accounted for before spatial metrics.
4. Cross-view and cross-camera transfer on held-out photographs.
5. Independent neutral, wood, foliage, sky and window measurements plus visual judgment.
6. Batch color locks, partial Impact, normalization, layers, cancellation and one Undo.

Compare static Sync and a manual Local Image finish with Match Look. Use occupancy
perturbations and absent-material cases to expose content bias. A lower optimized
statistics error is diagnostic evidence, not proof of photographic quality. Excellent
reproduction of the owner's product is the acceptance goal; it is not yet a measured
completion claim.

## Current photographic results

Native evaluation loaded four Kaufman exterior/drone/Sony targets and four merged HDR
interior/panorama targets against the owner's finished drone and living-room JPEGs.
The large references were decoded through the app loader with ICC handling and preview
scaling. Output includes before, 65% and 100% renders, editable settings, clipping and
loading/solve diagnostics. Eight additional cases compare the experimental conditional
objective on the same targets.

The baseline brightens the dark interiors substantially, but cross-room transfer still
shows casts and insufficient bedroom brightness. The appearance experiment reduces some
interior casts and brightening, with mixed exterior results. It remains off by default.
Neither objective currently meets an excellent reproduction claim across the whole set.

The finished input fixes are isolated for v2 in PR #13. Full Match Look remains on the
feature branch pending better correspondence, saved reference/preset workflow completion,
per-adjustment exclusions, and representative photographic acceptance. No photographs,
Capture One session database, sidecars or private manifests are committed.

## Diagnosis and corrected photographic results

The exact bedroom RAW and its final initially produced similarity .35337 from the
color signature. The resulting square-root attenuation transferred only 59.4 percent
of the necessary median-lightness change. Reference median OkLab L was .77118; the
adapted goal was .58169 and the actual result .58129. Agreement with that surrogate
was not agreement with the photograph.

For full normalization, the previous whole-frame CDF slope projection then changed the
reference fifth-percentile lightness from .38504 to .52372 and created a q99 target above
1.0. A curve's incremental comfort range cannot bound exposure, HDR and tone controls
combined. The corrected objective keeps the reference quantiles. The existing editable
curve guard still prevents new curve reversals and excessive incremental slopes.

The spatial evidence experiment separated sampled same captures from other sampled
compositions: 8 by 8 OkLab block-rank correlations were about .915 for the bedroom,
.926 for the living room and .993 for the drone exterior; the largest sampled unrelated
pair was .718. These are development examples, not a held-out recognition benchmark.
The implemented descriptor uses the same 128-pixel source geometry for reference and
target. The corrected bedroom case receives full normalization and exactly preserves
all five reference lightness targets. Rendered color and local-lighting differences
still require independent photographic acceptance.

The final checkpoint includes 45 focused pipeline tests, 21 focused engine tests and
physical RTX 5090 Vulkan CPU/GPU parity for global and layer results. The raw-input
changes separately passed 218 RAW tests and 12 engine file tests. UI checks, formatting
and strict Clippy are recorded with the checkpoint. Performance/corpus tests excluded
from normal suites are not represented as passed.
