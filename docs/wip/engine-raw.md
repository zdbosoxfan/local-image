# WIP: raw-processing quality (demosaic / highlights / port fidelity)

Status at pause (2026-10-09). No code changes yet: this is a research-only checkpoint. The worktree
branch was reset from `main` to the tip of `claude/sleepy-franklin-egimjb` (`fbc02a9`) because it
had been created from the wrong base.

## Done

* Read the current ports: `crates/lc-raw/src/demosaic/{mod,rcd,dual,bilinear}.rs`,
  `highlight.rs`, `highlight/opposed.rs`, `capture.rs`, and the engine wiring in
  `crates/lc-engine/src/files.rs` (`RawOptions`, `load_bytes_now`), `tests_toolset.rs` (golden
  hashes cover only the default options), `crates/lc-develop/src/tools.rs` (`Demosaic`,
  `HighlightMode`), and the UI in `crates/lc-ui-egui/src/panels/develop_tools.rs` (lines 141-160).
* Fetched the upstream sources for reference, outside the repo (never committed):
  `/tmp/claude-1000/-home-zdavidson-Documents-Local-Image---Testing/d1bb3608-dca2-4e30-892b-54147f9b368f/scratchpad/refvec/upstream/`
  * `dt/`: darktable at `733bd69f32cac7ff5e41025115942772add1f088`. Files: vng.c, amaze.cc,
    dual.c, rcd.c, ppg.c, basics.c, demosaic.c, capture.c, hlreconstruct/{segbased,segmentation,
    opposed,inpaint,laplacian,lch}.c, highlights.c, distance_transform.{c,h}, detail.c,
    negadoctor.c, toneequal.c, gaussian_elimination.h, luminance_mask.h, channelmixerrgb.c,
    chromatic_adaptation.h, darktable.h, math.h, imageop_math.h, gaussian.c,
    fast_guided_filter.h and imagebuf.h.
  * `rt/`: RawTherapee pinned at `5f486d3678b34c74ba0c63571c17babe20935019` (dev, 2026). Files:
    amaze_demosaic_RT.cc (last changed in `711f2744`), dual_demosaic_RT.cc, demosaic_algos.cc
    (vng4), rt_math.h and sleef.h.
  * Download method: curl config files `dt.curl` / `rt2.curl` in the `refvec` directory.

## Findings that affect the plan

* **darktable's dual demosaic does not run full VNG4.** At `733bd69f`, `dual_demosaic()` calls
  `vng_interpolate(..., only_vng_linear = TRUE)`, which is VNG's 4-colour linear interpolation
  with separate G1/G2 and the greens mixed afterwards. It then runs
  `color_smoothing(..., DT_DEMOSAIC_SMOOTH_2)` (two passes of the 3×3 median on R−G and B−G, in
  `basics.c`) and blends with `interpolatef(mask, high, vng)`.
* RawTherapee's RCD+VNG4 and AMaZE+VNG4 run the full `vng4_demosaic`.
* darktable offers "AMaZE (dual)" (`DT_IOP_DEMOSAIC_AMAZE_DUAL`), so a dual-AMaZE option is
  warranted.
* darktable passes `procmin` to `amaze_demosaic(in, out, w, h, filters, procmin)`. Its dual runs
  after capture sharpening inside demosaic. Ours runs capture later in the pipeline, which is an
  existing documented difference.
* The binned preview path (`bin_factor` → `develop_binned`) ignores the demosaic method. All
  highlight modes currently run on demosaiced RGB. Segmentation needs a CFA-stage hook in both
  the full path (`RawImage::normalized` → before `demosaic_with`) and the binned path.

## Plan / next steps (in order)

1. `demosaic/vng.rs`: port `vng.c` faithfully (lininterpolate, the full VNG with the `terms` and
   `chood` tables, and mix greens), plus `color_smoothing` from basics.c.
   * Add `Method::DualRcdVng` and `Method::Vng4`. Keep `Method::DualRcd` (bilinear) unchanged so
     saved `"dualRcd"` settings decode bit-identically. In `lc-develop`, show the new variant in
     `Demosaic::ALL` and hide the legacy one unless it is selected.
   * Decide by measurement between darktable's linear+smoothing and RawTherapee's full VNG4.
     Default to darktable-faithful and document the choice.
2. `demosaic/amaze.rs`: port darktable `amaze.cc`, cross-checked against RawTherapee
   `amaze_demosaic_RT.cc`. Add `Method::Amaze` and `Method::DualAmazeVng`, Bayer only.
3. `highlight/segbased.rs` + `highlight/segmentation.rs`: port darktable `segbased.c` and
   `segmentation.c` (it also needs `distance_transform.c` and the opposed CFA-variant pieces).
   * Add `HighlightMode::Segmentation`, applied on the normalized CFA before demosaic.
   * For binned previews, run it on the binned CFA with scaled parameters, and measure and
     document the residual difference.
4. Reference vectors: build standalone gcc harnesses in `refvec/<name>/` for RCD, dual mask,
   opposed, capture radius + RL, negadoctor, colorcal, toneeq, VNG4, AMaZE and segbased. Commit
   only the fixtures (small .bin files) under each crate's `tests/fixtures/`, with a README, and
   add Rust tests with tolerances.
5. Wire `RawOptions::method`, `files.rs`, and the UI labels. Then run quality metrics (zone plate,
   Siemens star, colour edges; PSNR and false colour) and timings at 24 MP, update docs
   (PORTS.md, darktable-NOTICE.md, DEVELOP-DESIGN.md §4.1), run fmt/clippy/tests, and commit.

## Constraints to remember

* Golden hashes (`lc-engine` tests_toolset, `lc-pipeline` tests_toolset) must stay unchanged, and
  new behaviour may only arrive through new options.
* `unsafe` is denied, and no C ever enters the build.
* Client ARWs under `scratchpad/raw/` are for local measurement only.
* Don't touch color.rs, DCP or camera matrices, or the lc-ui-egui files other agents own
  (detail.rs, right.rs, enhance.rs, dialogs.rs, menus.rs).
