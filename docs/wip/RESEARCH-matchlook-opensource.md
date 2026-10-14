# Research: open-source look-matching for Match Look

Status: in progress (started 2026-10-13). Findings appended below as gathered.

## darktable (GPL-3.0-or-later; direct port OK with attribution)

### colormapping.c (~650 lines C) 
Source: https://raw.githubusercontent.com/darktable-org/darktable/master/src/iop/colormapping.c
- Reinhard-2001-inspired, in Lab. Acquire "source" (reference) and "target" images; stores L cumulative histogram (2048 bins), up to 5 k-means clusters in a/b plane (mean, std, weight).
- Process: map each target cluster to a source cluster (cost = a/b distance blended with weight difference, `dominance` param); L channel = histogram matching (target CDF -> source inverse CDF) blended by `equalization`, then difference smoothed with bilateral filter to avoid banding; a/b = per-cluster mean/variance transfer, soft-blended with Shepard inverse-distance weights.
- Output: PIXELS (not sliders). Only 1 reference. Weak spots: not applicable directly; but the L-histogram-matching + smoothed-difference-field idea addresses end-point matching in tone space. Idea-only/port.

### channelmixerrgb.c (colour calibration)
Source: https://raw.githubusercontent.com/darktable-org/darktable/master/src/iop/channelmixerrgb.c
- "AI detect" illuminant (`_auto_detect_WB`, grey-edges style, Minkowski p=8) is COMPILED OUT (`#ifdef AI_ACTIVATED`; "AI feature not good enough so disabled"). Detect-from-surfaces/edges is a weighted chromaticity mean.
- Checker-based profile: white-patch exposure, least-squares black offset (exposure = cov/var), illuminant analytic in LMS, 3x9 weighted linear system via pseudo_solve_gaussian, weights by mode (saturation/hue gaussians/dE). No explicit regularisation (only weighting/clipping). Reports CIEDE2000 dE.
- Useful as: CIEDE2000 metric + weighted-LS design; not a reference-image matcher.

## RawTherapee / ART
### histmatching.cc (Alberto Griggio 2018, GPL-3.0-or-later, ~300 lines C++)
Source: https://raw.githubusercontent.com/Beep6581/RawTherapee/dev/rtengine/histmatching.cc
- "Auto-matched tone curve": renders raw neutrally (fast demosaic, no output ICC) and compares with embedded JPEG thumbnail (both resized, nearest, aspect-crop). Luminance CDFs for both; for each raw level find JPEG level with equal CDF (monotone linear walk). Tails outside populated bins marked -1 and excluded.
- mappingToCurve: choose anchor (first point where mapping >= identity, from idx 15), sample at adaptive steps (x step starts 0.05, x1.4 growth -> fewer points in highlights), prune concavities in upper S, 8-15 CatmullRom points, fallback linear if <4 points. No LS/regulariser; monotonicity only implicit.
- Output: CURVE (control points) = closest in spirit to our slider/curve output. Directly portable: CDF matching -> sparse monotone curve; ideas for tails/clipping.
- Forum: bad results with dark corners/clipping (https://discuss.pixls.us/t/rt-issue-with-auto-matched-curve-with-this-shot/13629/22). Rawpedia docs gap https://discuss.pixls.us/t/auto-matched-curve/19110/

## G'MIC (CeCILL, GPL-compatible; C++; huge repo)
Sources: https://gmic.eu/reference/transfer_pca.html , https://github.com/GreycLab/gmic , https://discuss.pixls.us/t/new-filter-colors-transfer-colors-multi/58847
- Licence: CeCILL family (LGPL-like/GPL-compatible per gmic.eu). Copyright David Tschumperle / GREYC. CeCILL v2.1 explicitly allows relicensing under GPL, so porting a small command to Rust is fine with attribution.
- Commands: `transfer_pca` (match mean + covariance of RGB between images = linear 3x3+offset colour transfer, tiny to port), `transfer_histogram` (per-channel histogram match), `transfer_rgb` (Pouli/Reinhard-style/ variational; gamma weighting of colour occurrences, guided-filter regularisation iterations, optional luminosity constraints). Newer G'MIC 4.0 "Transfer Colors [Multi]" (PCA / histogram / variational; brightness preservation; spatial regularization steps), multi-layer.
- Output: pixels. Idea-only for sliders. PCA transfer is a good "global target" for the solver; regularisation by guided filter is spatial, not slider-side.

## Rust crates
- `palette-transfer` (awxkee; BSD-3-Clause OR Apache-2.0; 13 commits, 4 stars, tiny): https://docs.rs/palette-transfer , https://github.com/awxkee/palette-transfer ; statistics-based per-colourspace (OKLab etc.) palette copy with strength param. Pixels. Pure Rust? depends on colorutils-rs and rayon (check). Reinhard-class only. Low quality/maturity -> borrow idea; 50-line reimplementation.
- No crate found for histogram matching or Reinhard/IDT; roll our own (trivial).

## Look-matching / slider prediction projects
- Capture One Match Look (closed; v16.5): uses existing adjustment tools (WB, contrast, Color Editor) so output stays editable; multi refs = weighted average. https://support.captureone.com/hc/en-us/articles/22188770298269 ; reviews note failures on event/landscape https://support.captureone.com/hc/de/community/posts/22949827057949-Match-Look-question . No open reverse-engineering found.
- Ashla (JeremySNR/ashla-lightroom): Lightroom Classic plugin, reference mode, LLM (OpenAI) returns develop-settings JSON clamped to ranges. Cloud model -> not usable; mapping/clamping prompt is only an idea. https://github.com/JeremySNR/ashla-lightroom (licence not checked)
- blitz: .cube LUT from reference (closed?). Zabaleta & Bertalmio 2018 raw style transfer (tone/colour palette/contrast): https://www.nt.uni-saarland.de/publications/zabaleta2018phororealistic/
- ChromaSoul StyleTransfer (AGPLv3, FastPhotoStyle) - pixels, neural, AGPL - not usable.
- DeepLPF (CVPR2020, FiveK): predicts parameters of local graduated/elliptical/polynomial filters https://openaccess.thecvf.com/content_CVPR_2020/html/Moran_DeepLPF_Deep_Local_Parametric_Filters_for_Image_Enhancement_CVPR_2020_paper.html (repo sjmoran/DeepLPF; licence unchecked)
- Neural Color Operators (arXiv 2207.08080): sequential brightness/contrast/etc. operators each with scalar param: https://arxiv.org/pdf/2207.08080
- Auto_ACR HF model (MIT-tagged): https://huggingface.co/Willlllllllllll/Auto_ACR_base/blob/main/README.md , repo github.com/BroWo1/Auto_ACR - to verify.
- MIT-Adobe FiveK dataset licence is NOT MIT (non-commercial/research terms; check) - training data only.
- darktable-chart: fits a darktable style (tone curve + colour LUT) from raw vs JPEG pair of a colour chart https://docs.darktable.org/usermanual/5.6/en/special-topics/darktable-chart/index.xml (calibration, needs chart).

## Auto_ACR (BroWo1) - slider-predicting model
Sources: https://github.com/BroWo1/Auto_ACR , https://huggingface.co/Willlllllllllll/Auto_ACR_base/blob/main/README.md
- Predicts ACR sliders + tone curves from a single RAW preview (timm backbone, slider and curve heads, DIFFERENTIABLE edit layer applying sliders in linear ProPhoto; optional LoRA). Trained on MIT-Adobe FiveK (expert c). No reference-image conditioning, no ONNX export mentioned, no param count; HF card says MIT but the GitHub repo shows no licence file -> licence UNCLEAR. Weights `Auto_ACR_base_v0.2.pt`.
- Use: idea only (differentiable slider->pixel model; fit sliders by gradient descent with box/param priors). Not a Match Look replacement (no reference).

## Python libs
- python-color-transfer (pengbo-learn; MIT; Python; 44 commits): https://github.com/pengbo-learn/python-color-transfer - mean_std (RGB), lab_transfer (Reinhard), pdf_transfer (Pitie iterative distribution transfer) with optional regrain; 0.04-3s/pair. Pixels. Port pdf_transfer idea = IDT. Doesn't help sliders.
- color_transfer (jrosebr1; MIT): https://github.com/jrosebr1/color_transfer - Reinhard Lab mean/std; ~tiny. Pixels. Trivial to port.
- colour-science (BSD-3-Clause): https://github.com/colour-science/colour/blob/develop/colour/characterisation/correction.py - colour correction matrices: Cheung 2004, Finlayson 2015 root-polynomial (exposure-invariant!), Vandermonde; solved by Moore-Penrose LS, NO regularisation. Output: matrix. Root-polynomial idea is useful for exposure-invariant colour fits.
- scikit-image match_histograms (BSD-3): per-channel CDF interpolation (np.interp of source quantiles to template quantiles). Pixels. Trivial.
- OpenCV xphoto: only white balance (GrayworldWB, SimpleWB, learning-based WB) + TonemapDurand; no colour transfer or histogram matching. OpenCV core Apache-2.0, contrib mixed. https://docs.opencv.org/4.12.0/javadoc/org/opencv/xphoto/package-summary.html
- Krita "Color Transfer" filter (manual: https://docs.krita.org/en/reference_manual/filters/colors.html): converts colours to those of a reference (Reinhard-type), reported slow; code licence not verified (Krita is GPL-3 anyway but it is C++/Qt, pixels).

## darktable exposure.c deflicker / spot (GPL-3.0-or-later)
Source: https://raw.githubusercontent.com/darktable-org/darktable/master/src/iop/exposure.c
- Deflicker: 65536-bin raw histogram; pick percentile; EV = log2((raw-black)/(white-black)); exposure = target_EV - EV (default percentile 50, target -4 EV). Spot: exposure = -log2(Y_in / Y_target) - compensation. Black only clamped below white (black >= white -> white-0.01). No full histogram match.
- Use: closed-form exposure init that is exactly percentile-based -> a robust warm start for our solver and a way to match highlight end-point (use 95-99th percentile) and shadow end-point (1-5th) independently.

## ART
- No reference-matching feature found (https://discuss.pixls.us/t/art-and-opencolorio/32322 only CLF/OCIO LUTs). ART inherits RT histmatching.

## More projects
- Lightroom "Match Total Exposures" is a built-in command (not a plugin): copies effective exposure (EXIF shutter/aperture/ISO/bias + Exposure slider) of the most-selected photo to others; exposure ONLY, no WB. https://petapixel.com/2014/12/15/tip-use-magical-match-total-exposures-feature-lightroom-quick-fix/ , https://community.adobe.com/t5/lightroom-classic-discussions/sdk-question-match-total-exposure/m-p/12887487/highlight/true . Idea: EXIF-based EV delta as prior/centre for exposure slider (regulariser).
- DaVinci Resolve Shot Match (closed): matches colour, contrast, brightness of target to reference; applies a hidden final adjustment; "moves closer without over-stretching a weak image" (splits the difference), does not fix clipping. https://larryjordan.com/articles/how-to-color-match-clips-in-davinci-resolve-20/ . Idea: bounded/partial match = regularisation by design.
- Blender Shot Matcher (semagnum; GPL-3.0; Python): maps source black/white min/max to target via Color Balance node; no optimisation. https://github.com/semagnum/shot-matcher . Too naive (min/max, outlier-sensitive); use percentiles instead.
- two_img_lut (pinaki82; MIT-0; C; 27 commits): .cube LUT from source+graded image pair, nearest-neighbour fill for empty cells. https://github.com/pinaki82/two_img_lut . Pixels/LUT; needs same-content pairs, not applicable to different scenes.
- DeepLPF (sjmoran; MIT code; 1.7M params, 6.9 MB, PyTorch): predicts cubic tone/colour curve + graduated + elliptical filter params. FiveK photos "cannot be redistributed". https://github.com/sjmoran/DeepLPF . Single image, no reference -> would need retraining; ONNX->tract feasible but optional.
- Neural Preset (ZHKKKe, CVPR23): deterministic per-image colour mapping matrix; code CC BY-NC-SA 4.0 -> NOT usable (non-commercial, ShareAlike incompatible with GPL). https://github.com/zhkkke/neuralpreset
- Rust optimisers (crates.io API): `levenberg-marquardt` 0.15.0 (MIT, nalgebra, 1.4M dl) ; `argmin` 0.11.0 (MIT OR Apache-2.0, pure Rust, L-BFGS/Nelder-Mead/etc, 5.3M dl); `cmaes` 0.2.2 (MIT OR Apache-2.0); `nlopt` is a -sys wrapper -> forbidden.

## Notes on the three weak spots (own analysis, not from sources)
1. Slider limits: add Tikhonov/ridge penalty lambda*||(s-s0)/range||^2 with s0 from closed-form init (EXIF EV delta / percentile exposure) and optimise in a bounded reparameterisation (s = range*tanh(u)) or a barrier; run LM (levenberg-marquardt crate) or L-BFGS (argmin). Resolve Shot Match "split the difference" behaviour = partial match is acceptable.
2. Limited range per pass: do coarse-to-fine multi-pass: first stage closed-form (exposure percentile, WB by grey-edges/LMS), then fit remaining; or iterate with re-rendered target; compute pseudo-target pixels (Reinhard/PCA/IDT/CDF match) and fit sliders to that rather than to raw reference stats.
3. End-points: separate loss terms on 1-5th and 95-99th percentile luminance (darktable deflicker style), exclude clipped tails (RT excludes bins outside populated range), weight residuals by 1/local density (RT uses denser points in shadows only up to 0.05 growth x1.4).

## RANKING: top 5 to borrow
1. RawTherapee histmatching.cc (GPL-3+, port): luminance CDF match -> sparse, concavity-pruned curve; tails excluded. Feed curve-region/highlight/shadow sliders; addresses end-points partly.
2. darktable exposure.c deflicker/spot (GPL-3+, port formulas): percentile -> EV closed-form; use for warm start and independent shadow/highlight percentile anchors. Addresses limits (good init) and end-points.
3. darktable colormapping.c (GPL-3+, adapt): L-histogram match with bilateral-smoothed difference field + a/b k-means cluster mean/variance transfer with Shepard weights -> pseudo-target pixels and colour-grading targets.
4. Solver tooling: `levenberg-marquardt` / `argmin` (MIT / MIT-OR-Apache, pure Rust) + ridge prior + tanh bounded reparameterisation (+ idea from Auto_ACR differentiable edit layer) - directly fixes limit-hitting and per-pass range.
5. Reinhard / G'MIC transfer_pca / python-color-transfer pdf_transfer (MIT, CeCILL, MIT): cheap pixel-domain pseudo-target (mean/cov match in Lab/OKLab; IDT for non-linear). Then fit sliders to pseudo-target. Port; <100 lines each.
