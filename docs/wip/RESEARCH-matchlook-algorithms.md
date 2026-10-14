# Research: Match Look algorithms and literature

Status: in progress (written incrementally). Date: 2026-10-13.
Goal: improve the Match Look solver (rail-hitting sliders, +-2 EV / +-3200 K cap, weak L5/L95 match).
Sources are cited inline with URLs. Findings appended per topic.

## Part 1: Learned / white-box enhancement (verified facts)

- **Exposure (Hu et al., SIGGRAPH/TOG 2018)** https://arxiv.org/abs/1709.09602 (details from https://ar5iv.labs.arxiv.org/html/1709.09602):
  8 differentiable filters (Exposure 2^E, WB per-channel gains, per-channel colour curves (8 segments), tone curve, gamma,
  contrast/saturation/B&W as one blend parameter between original and fully-enhanced). Fixed 5-step sequence chosen by RL
  (discrete filter choice + continuous parameters), reward from a WGAN critic trained on UNPAIRED target-set photos (FiveK expert C, 500px).
  Interpretable-sliders-by-construction. **Useful regularisers they used:** tone-curve slope clamped to [0.5, 2.0], colour-curve slope [0.9, 1.1],
  penalty for reusing the same filter in a trajectory, fixed small step budget. Needs a GPU net, but the *design* (small ordered filter chain,
  bounded slopes, one-blend-parameter contrast/saturation) is directly borrowable.
- **CURL (Moran 2020)** https://arxiv.org/abs/1911.13175 : global Photoshop-style curves (differentiable), loss in RGB+Lab+HSV jointly.
  Borrow: score the match in several colour spaces (we use Oklab only), and curve-as-primitive.
- **DeepLPF (Moran 2020)** https://arxiv.org/abs/2003.13985 : regresses parameters of graduated / elliptical / polynomial local filters (Lightroom-like).
  Authors argue constraining to the editor's toolbox is "a natural form of regularization". Relevant only if we later add local-mask solving.
- **Image-adaptive 3D LUT (Zeng et al., TPAMI 2020)** https://arxiv.org/abs/2009.14468 : tiny CNN (<600K params) predicts weights mixing
  basis 3D LUTs; uses smoothness (L2) and MONOTONICITY (ReLU penalty on decreasing neighbouring entries) regularisers
  (described in https://arxiv.org/pdf/2209.01749 and https://arxiv.org/pdf/2503.23793). Output is a LUT, not sliders -> not interpretable;
  borrow only the monotonicity/smoothness regularisation idea for our fitted tone curve.
- **Neural Preset (Ke et al., CVPR 2023)** https://arxiv.org/abs/2303.13511 : two stages, colour *normalisation* (scene -> canonical state) then
  *stylisation* with a per-image colour-mapping matrix. Key idea for us: normalise the target (and the references!) to a canonical look before
  comparing, which is what makes transfer content-robust. Pixels not sliders; self-supervised NN.
- **StarEnhancer (Song et al., ICCV 2021)** https://arxiv.org/abs/2107.12898 : style encoder + curve encoder predicting piecewise-cubic curve knots
  (15 curves); style from a reference image works for unseen styles. Curves, not sliders; NN.
- **Deep Analog (2026)** https://arxiv.org/abs/2608.14702 : reference-conditioned residual 3D LUT, self-supervised on procedurally generated colour transforms.
  Interesting training trick (synthetic edits) that we could replicate with our OWN slider renders to build a prior (see Part 3).

## Part 2: Classical colour / tone transfer (verified facts and source notes)

- **Reinhard et al. 2001, "Color Transfer between Images"** (IEEE CG&A): per-channel mean/std matching in decorrelated lαβ.
  Summary sources: https://helios2.mi.parisdescartes.fr/~jdelon/enseignement/tp_image/org/TP_color_transfer.html ,
  https://libcom.readthedocs.io/en/latest/color_transfer.html . Known failure: it is purely statistical and sensitive to how much
  AREA each colour occupies (beach with lots of sky/sand vs forest => moves the forest's green mean towards sand), as noted in secondary
  reviews of its limitations. Mean/std are NOT robust statistics; percentiles/medians are better.
- **Pitié, Kokaram, Dahyot 2005 (ICCV), N-D pdf transfer** https://edepositireland.ie/handle/2262/19800 : iterates random 3-D rotations + 1-D histogram
  matching of marginals until the 3-D distribution converges. Uses only 1-D marginals (cheap), non-parametric, but known to amplify
  noise/grain and to be content-proportion sensitive (it forces the WHOLE distribution to match).
- **Pitié & Kokaram 2007, Monge-Kantorovitch linear (MKL)** https://edepositireland.ie/handle/2262/24720 : closed-form linear map
  T = Σ_s^(-1/2) (Σ_s^(1/2) Σ_t Σ_s^(1/2))^(1/2) Σ_s^(-1/2); optimal in the sense of minimal colour change. 3x3 matrix, trivially cheap,
  but second-order only (still area-proportion sensitive); good as a DIAGNOSTIC (what 3x3 colour matrix would move target to reference?)
  and as a regulariser on chroma/colour-grading axes, not as the final answer.
- **Rabin, Delon, Gousseau 2010 (ICIP)** https://helios2.mi.parisdescartes.fr/~jdelon/demos/artefact_removal_html : regularising the transport MAP
  (displacement field) with a non-local filter removes the noise/detail damage histogram matching causes. Analogy for us: regularise the
  *tone curve*, do not apply the raw CDF map.
- **Bonneel, Rabin, Peyré, Pfister 2015, sliced Wasserstein barycenters** https://hal.archives-ouvertes.fr/hal-00881872 : a *barycenter* of several
  distributions = principled multi-reference aggregation (rather than averaging statistics). 1-D version = average of quantile functions
  (per-percentile mean) which is trivially what we need for tone.
- **Relaxed / partial OT for colour transfer** (Rabin, Ferradans, Papadakis, "Adaptive Color Transfer With Relaxed Optimal Transport", ICIP 2014)
  https://hal.archives-ouvertes.fr/hal-01002830 (page blocked by bot-protection in this session; only the title/venue confirmed via search
  listings; the content-robustness claim comes from general knowledge of the method, flag as UNVERIFIED here). Idea: do not force mass
  conservation, so colours present in only one image (sky in the reference) are allowed to be left unmatched.
- **Region/tone-zone aware**: ColorFM preprint https://arxiv.org/pdf/2607.07119 (per-semantic-region transfer with unmatched pixels pooled into
  a global residual pair; also a tone-masked cast transfer from darkest/brightest 15% of pixels weighted by luminance mask with quadratic
  exponents). Ly/Beucher/Bilodeau region-based correction https://people.cmm.minesparis.psl.eu/users/beucher/publi/Color_correction_2015.pdf .
  Survey: Faridul et al. 2014/2016 https://diglib.eg.org/handle/10.1111/cgf12671 (round-trip evaluation). Preprints only seen as excerpts.
- **Illuminant estimation family** (Gray-world, White-patch, Shades-of-Gray p-norm interpolating them, Grey-Edge of van de Weijer):
  https://staff.fnwi.uva.nl/th.gevers/pub/GeversPAMI11-2.pdf . Shades-of-Gray with p~6 is the robust compromise; Grey-edge uses derivatives and is
  less biased by large coloured surfaces (sand, foliage) => better scene-invariance.

## Part 3: Slider solving, tone-curve priors, robust optimisation (verified facts)

- **Bychkovsky et al., CVPR 2011, "Learning Photographic Global Tonal Adjustment"** https://people.csail.mit.edu/vladb/photoadjust/db_imageadjust.pdf (read in full):
  adjustment = remapping curve of CIE-Lab L (spline, 51 control points). KEY: (1) they **normalise exposure first** by linearly remapping each
  image so min=0, max=100, to remove camera/photographer exposure bias; (2) the curves live in a tiny **PCA space - the first PCA coefficient
  alone is "a good approximation to the full curve"**; (3) features = log-intensity **percentiles every 2%**, at 3 blur scales, plus
  CDF/equalisation curve projected on 5 PCA comps, and **highlight-clipping fractions at 1,2,3,5,10,15%**; regressors = least squares, LASSO,
  Gaussian-process regression (GPR); error metric ΔL in Lab (JND ~2.3); personalisation needs only ~3 example photos. So: tone adjustments
  are low-dimensional; a 2-3 parameter (PCA / slider-basis) fit to percentile curves is enough, and anything beyond is over-fitting.
- **Neural Photo-Finishing (Tseng et al., SIGGRAPH Asia 2022)** https://light.princeton.edu/publication/neural-photo-finishing/ : differentiable proxy of
  Adobe Camera Raw used for "slider regression" (fit sliders by gradient descent) and an encoder that predicts ACR sliders for a style collection.
  Notes the slider space is large and small early changes have big downstream effects. We have no proxy but we have the real pipeline.
- **Goal-conditioned RL for photo-finishing tuning** https://arxiv.org/html/2503.07300v1 : black-box pipelines (Lightroom/darktable), 9 params
  (exposure, colour balance x3, saturation, contrast, highlights, shadows, texture). CMA-ES baseline needs 200 queries; their TD3 policy ~10.
  For different-content "stylisation" the reward is a StyleScore = VGG Gram-matrix distance + **luminance (Y) and chroma (UV) HISTOGRAM distances**
  (weights 100 / 50) minus a content penalty. No explicit parameter-magnitude penalty (episodes end if mean intensity leaves a set range;
  actions clipped). => the histogram-distance, not mean-difference, is the content-robust part and is free of ML.
- **SciPy least_squares** https://docs.scipy.org/doc/scipy/reference/generated/scipy.optimize.least_squares.html : documents the exact toolbox we want:
  trust-region-reflective (bounds, iterates strictly feasible), robust losses (huber, soft_l1, cauchy) with f_scale as inlier margin, x_scale
  variable scaling. We do not need SciPy; this is a recipe for a ~150-line Rust damped Gauss-Newton / LM with box constraints.
- **Monotone curve fitting** (secondary web results): raw CDF-matching map T(v)=F_ref^-1(F_src(v)) -> smooth (Gaussian ~2 bins) -> enforce monotone
  with weighted PAV/isotonic regression or monotone cubic (PCHIP / Fritsch-Carlson) -> optionally fit a low-parameter curve
  (https://isis.astrogeology.usgs.gov/8.1.0/Application/presentation/Tabbed/histmatch/histmatch.html uses percentile-knot piecewise-linear
  maps with 0.5/99.5% cut-offs; https://ar5iv.labs.arxiv.org/html/2006.05527 PAV; https://www.stat.berkeley.edu/~ryantibs/papers/neariso.pdf nearly-isotonic regression).
- **Photoshop Match Color** (Luminance, Color Intensity, Fade, Neutralize): https://www.peachpit.com/articles/article.aspx?p=706213 - the
  well-known UX trio; "Neutralize" = remove dominant cast (gray-world style), "Fade" = our Strength.
- Not verified from a primary source in this session (general knowledge, flagged): Tikhonov/ridge spreading load across collinear columns,
  Huber loss, Levenberg-Marquardt damping, CLIP embeddings (Radford et al. 2021, https://arxiv.org/abs/2103.00020), trimmed means/MAD
  outlier rejection, skin-tone-anchored WB (needs a face/skin detector; no source fetched).

---

# RANKED TECHNIQUES (what to adopt)

Legend: gain on (a) rail-hitting, (b) +-2 EV / +-3200 K cap, (c) end-point mismatch; effort S/M/L; ML = none unless stated.

### 1. Ridge-regularised, box-bounded Gauss-Newton / LM in normalised slider space  [fixes (a)] - effort M - no ML
- Source: Tikhonov regularisation / LM; trust-region-reflective with robust loss as in SciPy docs above; slope-bounds idea from Exposure (tone slopes [0.5,2], colour curves [0.9,1.1]) https://ar5iv.labs.arxiv.org/html/1709.09602.
- Plain: instead of coordinate descent on a raw objective, minimise  Σ_i ρ(r_i(p)/s_i) + λ Σ_j w_j ((p_j - p0_j)/R_j)^2  where p0 = the target's current slider values, R_j = "comfort range" of slider j,
  w_j = UX cost (Contrast and Saturation expensive, Exposure/WB cheap, Whites/Blacks cheap). The Jacobian comes from ONE finite-difference pass (about 25 sliders -> ~25 renders; we already pay this).
  Ridge makes correlated columns (exposure vs whites vs contrast vs curve) SHARE the correction in proportion to cheapness, which is exactly the "split across controls" behaviour wanted; no hand priority needed.
- Mapping: residual vector r = [percentile diffs of L (P1,P5,P25,P50,P75,P95,P99), chroma percentiles per luminance zone, neutral-chromaticity a,b, zone-mean a,b]; sliders = Develop controls; bounds = soft box (barrier or hard clamp inside the "comfort" range, hard rails beyond only if Strength demands).
- Gain: (a) large (this is the direct fix). (b) none by itself. (c) neutral-to-positive.

### 2. Closed-form warm start for exposure and WB + adaptive continuation (no hard caps)  [fixes (b)] - effort S - no ML
- Source: Bychkovsky exposure normalisation https://people.csail.mit.edu/vladb/photoadjust/db_imageadjust.pdf ; Exposure filter is simply 2^E https://arxiv.org/abs/1709.09602 ; grey-edge/shades-of-gray https://staff.fnwi.uva.nl/th.gevers/pub/GeversPAMI11-2.pdf.
- Plain: exposure EV0 = log2(Y_ref_median_linear / Y_tgt_median_linear) (ideally after the current tone sliders; use a render of the current state, and take log ratio of the L50 in linear), clamp to say +-4 EV, then LM refines. WB: estimate the target's and the reference's illuminant chromaticity with Shades-of-Gray (p~6) or grey-edge on the rendered proxy, convert the gap to a Temp/Tint step via a finite-difference of ONE probe render. Then continue LM passes while improvement > eps (trust region grows when the model predicts well) so no fixed +-2 EV / +-1600 K/sweep ceiling.
- Gain: (b) fully removes the cap (3 -EV targets that ended at +2 EV reach their answer in 1 jump). (a) fewer compensating rail moves.

### 3. Percentile-curve matching, then project onto sliders hierarchically  [fixes (c), helps (a)] - effort M - no ML
- Source: histogram specification / CDF matching; PAV or PCHIP monotone fit; Bychkovsky percentile features + low-dim PCA tone space; Rabin-Delon-Gousseau regularised transport maps https://helios2.mi.parisdescartes.fr/~jdelon/demos/artefact_removal_html .
- Plain: build the desired L mapping Q_ref(F_tgt(L)) on the PROXY RENDER of the target's CURRENT state, smooth + force monotone (PAV/PCHIP), express as desired delta curve d(L) at ~9 knots weighted heavier at the ends.
  Measure each tone slider's delta-curve once by finite difference: B_j(L) for exposure, whites, blacks, highlights, shadows, contrast, curve regions. Solve a SMALL weighted ridge NNLS d(L) ~ Σ_j δ_j B_j(L) with slider priority via ridge weights (cheap sliders first: exposure by median, blacks/whites by P1/P99-P5/P95, shadows/highlights by P25/P75 range, contrast only by residual slope, curve regions last).
  Then use the full closed loop only to refine. (This is the "projection onto our parametric curve and whites/blacks" the brief asked for.)
- Gain: (c) strong because the loss explicitly weights end-point percentiles and whites/blacks are the matching sliders; (a) strong because contrast is last-resort.
- Do not apply the raw CDF map (noise/banding, content-proportion sensitive); that is why it is projected.

### 4. Scene-invariant statistics: quantiles instead of mean/std, zone-conditioned chroma, neutral-region chromaticity  [robustness] - effort M - no ML
- Source: Reinhard 2001 limits (area-proportion sensitivity) summaries https://libcom.readthedocs.io/en/latest/color_transfer.html ; StarEnhancer/Neural Preset normalise-then-stylise idea https://arxiv.org/abs/2303.13511 ; tone-masked cast transfer in ColorFM https://arxiv.org/pdf/2607.07119 ; luminance + chroma histogram distances in the RL paper https://arxiv.org/html/2503.07300v1 ; relaxed/partial OT idea https://hal.archives-ouvertes.fr/hal-01002830 (unverified content).
- Plain: (i) L: P1..P99 quantiles (above). (ii) Chroma: per-zone (shadow/mid/highlight, soft luminance weights) median and P90 chroma and trimmed-mean a,b rather than global mean a,b: a beach's blue sky/sand and a forest's green dominate global means, but zone-medians over low-chroma pixels do not. (iii) WB target = chromaticity of the low-chroma (neutral-ish) pixels or of the shades-of-gray estimate, and compare reference vs target ILLUMINANT-corrected (i.e. match the *change* in cast, not the absolute colour of a scene). (iv) Colour grading shadows/highlights targets from the darkest/brightest 15% (cast) with quadratic luminance weights (the ColorFM trick), midtone grading from the 35-65% band. (v) Saturation from chroma P50/P90 ratio; vibrance from the low-chroma-pixel (P10-P40) boost. Everything compares CDFs / robust quantiles = a 1-D unbalanced-transport spirit without segmentation.
- Gain: robustness to the beach-vs-forest case; (c) neutral; (a) small (stops saturation chasing area proportions, which is the source of -77/-49 saturation).
- Segmentation-aware transfer (ColorFM, Ly/Beucher) is OUT of scope: needs a segmenter (ML) and seams; zones-by-luminance gets 80% of the benefit.

### 5. Huber (soft-L1) loss with per-statistic scale and inverse-variance weights  [fixes (a), robustness] - effort S - no ML
- Source: SciPy robust losses https://docs.scipy.org/doc/scipy/reference/generated/scipy.optimize.least_squares.html ; for scale s_i use the spread of the statistic across the references (MAD) floor 1 JND (ΔL 2.3, Bychkovsky).
- Plain: residuals in units of "JND or reference spread"; Huber so one outlier statistic (e.g. a blown sky) can't drag sliders to the rails; statistics where references DISAGREE (large MAD) get lower weight automatically.
- Gain: (a) moderate.

### 6. Prior over typical slider values (MAP), learned from the user's own catalog or fixed table  [fixes (a)] - effort S (fixed) / M (learned) - no ML
- Plain: replace plain "toward current" ridge with a per-slider Gaussian/Laplace prior N(p0 + μ_j, σ_j) where σ_j = typical edit spread (e.g. contrast ~25, saturation ~20, temp ~800 K, EV ~0.7); estimated from existing edited photos in the catalog (histograms of slider values) or hand table. Laplace (L1) gives sparse solutions: most sliders stay zero, which Lightroom/C1 users expect.
- Mapping: just w_j and R_j in technique 1, optionally L1 via IRLS.
- Gain: (a) strong; user-visible: sliders look "human".

### 7. Continuation (homotopy) in the penalty and coarse-to-fine proxy size  [fixes (a),(b), cheaper] - effort S - no ML
- Plain: solve first with large λ (only exposure/WB/whites/blacks move), then relax λ by x0.3 per pass until the objective gain per pass < 1e-3 or residual < 1 JND. Use 256 px proxies for the first passes, 512 px for the final one. Stops early = fewer renders than 66 despite larger range.

### 8. Multi-reference aggregation by per-quantile (1-D barycenter) weighted median/trimmed mean, with outlier rejection  [robustness] - effort S - optional tiny ML
- Source: sliced/1-D Wasserstein barycenter = average of quantile functions, Bonneel et al. https://hal.archives-ouvertes.fr/hal-00881872 ; Pitié/Kokaram 2005 marginals https://edepositireland.ie/handle/2262/19800 ; similarity weights from colour histograms (already have), optional CLIP embedding (https://arxiv.org/abs/2103.00020, unverified use; ~150M-param image tower, not tiny: skip unless wanted).
- Plain: target for each percentile = weighted MEDIAN (or 20%-trimmed weighted mean) over references of that percentile, not mean of means; weights = existing exp(-8d) floored at 0.2/N and capped so one reference cannot exceed 50%; reject a reference whose statistic vector is >3 MAD from the weighted median (warn in the UI: "Reference 4 differs from the others"). Spread (MAD) feeds technique 5 as scale.
- Gain: (c) mild (cleaner end-point targets); robustness to a bad reference.

### 9. Tiny learned warm-start regressor (ridge/GP/small MLP) trained on synthetic slider renders  [speed, (a)] - effort L - tiny ML, CPU
- Source: Bychkovsky (linear/LASSO/GPR on percentile features; low-dim PCA) https://people.csail.mit.edu/vladb/photoadjust/db_imageadjust.pdf ; Deep Analog's self-supervised procedurally generated transforms https://arxiv.org/abs/2608.14702 ; Neural Photo-Finishing slider regression https://light.princeton.edu/publication/neural-photo-finishing/ ; RL tuning in 10 queries vs 200 https://arxiv.org/html/2503.07300v1 .
- Plain: render random slider vectors through OUR pipeline on a few dozen proxies, store (stat_after - stat_before) -> slider delta. A ridge/GPR on ~40 features gives the initial sliders; the closed-loop LM only refines (about 10-20 renders instead of 66). Inference is a matrix multiply in pure Rust, matches the pure-Rust rule. Do AFTER 1-8; gains mostly speed.

### 10. Zone-wise MKL (3x3 chroma covariance map) as a diagnostic / grading initialiser  [low priority] - effort S - no ML
- Source: Pitié & Kokaram 2007 https://edepositireland.ie/handle/2262/24720 . Closed-form optimal linear map between (a,b) covariances per zone; its rotation/scale suggests hue-shift and saturation-ratio per zone for colour-grading wheels. Second-order only so content-proportion sensitive: use with trimmed covariance and shrink toward identity. Skip if time-limited.

### Not recommended / out of scope
- N-D pdf transfer (Pitié 2005), full OT colour mapping, deep colour transfer (Neural Preset, StarEnhancer, 3D-LUT nets, PieNet, CURL, DeepLPF): they output pixels/LUTs/curves, not Develop sliders, and mostly need a GPU net. Borrow only: normalise-then-stylise (Neural Preset), bounded slopes (Exposure), monotonic+smooth regularisers (3D-LUT papers), colour-space-diverse loss (CURL), local-filter priors if we ever solve masks (DeepLPF).
- Skin-anchored WB: needs face/skin detection; plausible later as an extra neutral-ish anchor weight. Unverified, defer.

---

# RECOMMENDED SOLVER v2 (sketch)

1. **Analyse references** -> per reference: L quantiles (P1,2,5,10,25,50,75,90,95,98,99 in Oklab L of the *rendered* reference, linear-light median for exposure), per-zone (soft shadow/mid/high weights) chroma median + P90, trimmed-mean a,b per zone, neutral-region chromaticity (shades-of-gray p=6 on the render, also low-chroma-pixel median). Aggregate with weighted median/trimmed mean per statistic (weights = existing similarity, floored/capped), store MAD per statistic.
2. **Normalise target start**: render target at current sliders (256 px). Warm start: EV0 from linear-median ratio (clamp +-4); Temp/Tint from illuminant-gap (one probe render to calibrate K per mired/tint unit).
3. **Jacobian pass** (about 25 probe renders at 256 px, central diff only for the sliders actually in play) -> J. Recompute only if the LM step is rejected or exposure/WB moved more than ~0.5 EV / 800 K.
4. **Tone projection**: from target quantiles and averaged reference quantiles build the monotone delta curve (PAV/PCHIP), weight end-points x2, and solve a ridge NNLS over [Exposure, Blacks, Whites, Shadows, Highlights, Contrast, curve-region knots] using J columns; ridge weights w_j order cheap->expensive: Exposure < Blacks/Whites < Shadows/Highlights < Contrast < curve regions. Result seeds p.
5. **Joint LM** over all sliders: loss = Σ Huber(r_i/s_i) with s_i = max(1 JND, reference MAD) + λ Σ w_j ((p_j-p0_j)/R_j)^2 (+ optional L1 term via IRLS); bounds = comfort box (e.g. |Contrast|<=60, |Sat|<=40, |Temp step|<=6000 K, |EV|<=5) hard rails outside only when Strength demands. Trust-region step control, continuation: lambda x1, x0.3, x0.1 over up to 3 passes, early exit when gain < 1e-3 (256 px for passes 1-2, 512 px final). Budget ~40-60 renders worst case, ~25 typical.
6. **Strength**: blend in *slider space around p0* per Photoshop-style "Fade" (already have); additionally report rails hit and the final per-statistic residuals so the UI can say "Contrast was limited".
7. **Validation**: rerun the 12 harness pairs (REVIEW-matchlook-fable.md section C). Acceptance: contrast|<=60 on >=11/12, no temp at cap, mean |ΔL5|,|ΔL95| <= mean |ΔL50|+1, mean ΔE not worse than ~3.

Expected results (HYPOTHESES, not measured): (a) rail hits 8/12 -> <=2/12; (b) -3 EV targets within 0.3 EV in 1-2 passes; (c) L5/L95 errors roughly halved because those statistics now drive Blacks/Whites explicitly. Test each technique alone in the harness to confirm before merging.
