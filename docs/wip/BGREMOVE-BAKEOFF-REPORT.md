# Background-removal model evaluation (CPU, tract-onnx 0.23.8)

Worktree: `.claude/worktrees/bg-models` (branch wip/bg-models). Harness: `crates/li-seg/examples/bg_eval.rs`.
Models: `~/.local/share/local-image-dev/model-eval/models/`, results in `results/`.

## Candidate sources (URL / size / sha256 / licence)

| id | file | URL | bytes | sha256 | licence |
|---|---|---|---|---|---|
| u2netp | u2netp.onnx | https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx | 4574861 | 309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8 | Apache-2.0 (U²-Net, Qin et al.) |
| silueta | silueta.onnx | .../rembg/releases/download/v0.0.0/silueta.onnx | 44173029 | 75da6c8d2f8096ec743d071951be73b4a8bc7b3e51d9a6625d63644f90ffeedb | U²-Net derivative, licence not stated by rembg (assume Apache-2.0) |
| u2net | u2net.onnx | .../rembg/releases/download/v0.0.0/u2net.onnx | 175997641 | 8d10d2f3bb75ae3b6d527c77944fc5e7dcd94b29809d47a739a7a728a912b491 | Apache-2.0 |
| isnet (current) | isnet-general-use.onnx | .../rembg/releases/download/v0.0.0/isnet-general-use.onnx | 178648008 | 60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a | Apache-2.0 (DIS) |
| MODNet | modnet.onnx | https://huggingface.co/Xenova/modnet/resolve/main/onnx/model.onnx | 25888640 | 07c308cf0fc7e6e8b2065a12ed7fc07e1de8febb7dc7839d7b7f15dd66584df9 | Apache-2.0 |
| RMBG-1.4 | rmbg-1.4.onnx | https://huggingface.co/briaai/RMBG-1.4/resolve/main/onnx/model.onnx | 176153355 | 8cafcf770b06757c4eaced21b1a88e57fd2b66de01b8045f35f01535ba742e0f | BRIA RMBG-1.4 licence, CC BY-NC style (non-commercial; HF repo is gated) |
| RMBG-2.0 | bria-rmbg-2.0.onnx | https://github.com/danielgatis/rembg/releases/download/v0.0.0/bria-rmbg-2.0.onnx (HF original is gated, 401) | 1024331469 | 5b486f08200f513f460da46dd701db5fbb47d79b4be4b708a19444bcd4e79958 | CC BY-NC 4.0 (BRIA), commercial needs paid agreement |
| BiRefNet lite (swin-tiny) | birefnet-lite-swin-tiny.onnx | https://huggingface.co/onnx-community/BiRefNet_lite-ONNX/resolve/main/onnx/model.onnx (identical size to ZhengPeng7 release v1 BiRefNet-general-bb_swin_v1_tiny-epoch_232.onnx) | 224005088 | 5600024376f572a557870a5eb0afb1e5961636bef4e1e22132025467d0f03333 | MIT |
| BiRefNet lite 384 (emoro24, std ops) | birefnet-lite-384-emoro-v1.onnx | https://github.com/emoro24/birefnet-lite-onnx/releases/download/v1/birefnet-lite-384.onnx | 188675201 | ce5dd7c713f4573b0a7a6ea7abb8aca6728eb585b5fe0f38a6f3ae0eee44f2ba | MIT |
| BiRefNet lite 1024 fp16 (emoro24 v2) | birefnet-lite-1024-fp16-emoro-v2.onnx | .../emoro24/birefnet-lite-onnx/releases/download/v2/birefnet-lite-1024-fp16.onnx | 122969369 | 945ef7bcca23823f823ca00a22c1c08fedd9391bff8620658dfe292dfcea0a22 | MIT |
| BiRefNet general 1024 | birefnet-general.onnx | https://github.com/ZhengPeng7/BiRefNet/releases/download/v1/BiRefNet-general-epoch_244.onnx | 972666916 | 58f621f00f5d756097615970a88a791584600dcf7c45b18a0a6267535a1ebd3c | MIT |
| BiRefNet general 512 | birefnet-general-512.onnx | https://huggingface.co/onnx-community/BiRefNet_512x512-ONNX/resolve/main/onnx/model.onnx | 940413603 | 617a11ca04cb13a2817bfad605969b4a9a040fceedbaaa46348a3957f0f6c254 | MIT |
| BEN2 Base | BEN2_Base.onnx | https://huggingface.co/PramaLLC/BEN2/resolve/main/BEN2_Base.onnx | 222932053 | 22cea62108ff53b7ccc20f7a008bf30494228d84b1687f29ecbe76936a998101 | MIT (trained on DIS5K + 22K proprietary set) |
| InSPyReNet SwinB Plus-Ultra | inspyrenet-swinb-plus-ultra.onnx | https://huggingface.co/OS-Software/InSPyReNet-SwinB-Plus-Ultra-ONNX/resolve/main/onnx/model.onnx | 395316574 | c108dd92b3ddfe3a2d9e9ac2b74730cc5acbb2ddc7ea863330c43f56ae832aa3 | MIT (community conversion of plemeri/InSPyReNet) |

Derived by me (Python `onnx`, offline tooling only): `BEN2_Base_fp32.onnx` (fp16->fp32 rewrite, 409092075 B, sha256 8a12f0f9...ad1) and `isnet-general-use-512/768.onnx` (Resize size constants rescaled).

## Test images (all CC0 from Wikimedia Commons, downloaded at ~1920 px long side)
- `portrait.jpg`: Portrait with black hair, 3080644.jpg - CC0 - https://commons.wikimedia.org/wiki/File:Portrait_with_black_hair,_3080644.jpg
- `pet.jpg`: Domestic shorthair cat portrait in grass.jpg - CC0 - https://commons.wikimedia.org/wiki/File:Domestic_shorthair_cat_portrait_in_grass.jpg
- `product.jpg`: Herschel backpack by rocks (Unsplash).jpg - CC0 - https://commons.wikimedia.org/wiki/File:Herschel_backpack_by_rocks_(Unsplash).jpg
- `street.jpg`: Crowd crossing the street, Hungary 2011.jpg - CC0 - https://commons.wikimedia.org/wiki/File:Crowd_crossing_the_street,_Hungary_2011.jpg
- `branches.jpg`: Bare tree in Kent, WA (2025) - 0010.jpg - CC0 - https://commons.wikimedia.org/wiki/File:Bare_tree_in_Kent,_WA_(2025)_-_0010.jpg
- `glass.jpg`: A bottle of cold water with droplets.jpg - CC0 - https://commons.wikimedia.org/wiki/File:A_bottle_of_cold_water_with_droplets.jpg

## tract compatibility (tract-onnx 0.23.8, same load path as li-seg: fixed [1,3,S,S] input, into_optimized)

| model | loads+runs? | notes |
|---|---|---|
| u2netp / silueta / u2net / MODNet / RMBG-1.4 | YES | but NOT through li-seg's `Segmenter::load` as written: `load_cfg` calls `.with_ignore_output_shapes(false)` which turns declared output shapes ON; these files declare symbolic output dims -> `Failed analyse for node "Sigmoid_1050" ... Impossible to unify Val(1) with Sym(Sigmoid..._dim_0)`. Fix: don't call it (tract default ignores output shapes) for Subject models. IS-Net works as-is. |
| IS-Net general at 512 / 768 | NO as shipped (`/stage1/Concat_2 ... Impossible to unify Val(32) with Val(16)`: the exporter baked 39 Resize target sizes as constants for 1024). YES after rescaling those 39 constants (script `patch_isnet_size.py`, produces a 512/768 file we would have to host) | |
| BiRefNet lite (swin-tiny, official/onnx-community) | YES | opset 17, deformable conv already decomposed (no DeformConv / GridSample ops), fixed 1024 input |
| BiRefNet general 1024 (official release) and 512 (onnx-community) | YES | same; 5.3 GB RAM at 1024 |
| BiRefNet lite 384 (emoro24, fp32) | YES | |
| BiRefNet lite 1024 fp16 (emoro24 v2) | NO | `Translating node #25 ".../attn/qkv/MatMul/MatMulAddFusion" Gemm ToTypedTranslator: Output mismatch after rewiring expansion for output #0: expected 67081,288,F16 got 67081,288,F32` (tract cannot Gemm in f16) |
| BEN2 Base (official fp16-mixed) | NO | `Translating node #9320 "/multifieldcrossatt/attention.0/Gemm" Gemm ToTypedTranslator ... expected 256,128,F16 got 256,128,F32`. YES after rewriting fp16 -> fp32 (script `fp16_to_fp32.py`, 409 MB file we would have to host) |
| RMBG-2.0 | YES only with `.with_ignore_value_info(true).with_ignore_output_shapes(true)` (as li-seg already does for depth). Plain load: `Failed analyse for node #716 "/model/bb/patch_embed/proj/Conv": Unifying shapes 1,192,(height)/4,(width)/4 and 1,192,256,256`. Output already has sigmoid applied. |
| InSPyReNet SwinB Plus-Ultra (OS-Software ONNX) | YES | but failed to find any subject on the crowd photo |

DeformConv was NOT a problem: none of the official exports contain it.

## Threading finding (important)
tract's matmul multithreading is the `multithread-mm` feature of `tract-linalg`, which is OFF in our build (`cargo tree -e features` shows only "default"; nothing in `crates/` sets a tract Executor). Everything in li-seg therefore runs on ONE core. I measured the speed-up with a scratch crate (not in the repo; `tract-linalg = { features = ["multithread-mm"] }` + `set_default_executor(Executor::multithread(n))`). Conv nets gain ~3x (IS-Net 1024: 5.6 s -> 1.9 s), transformer nets gain little (BiRefNet lite 1024: 30 s -> 24 s) because only matmul is parallel. Enabling it needs a direct `tract-linalg` dependency with that feature in li-seg (Cargo.toml/Cargo.lock change; not done here).

## Timings (portrait.jpg 1920x1199, tract in the dev profile of the worktree, 3 warm runs, median; AMD 9800X3D)
"st" = how li-seg runs today (tract matmul on ONE thread). "mt8" = same code with tract-linalg `multithread-mm` and 8 threads (scratch build, see Threading finding).
Machine load (`uptime` 1-min) during these runs was 1-7 (other builders compiling); raw logs: `batch_timing.log`, `batch_mt.log`, `results/timing/`. Load = tract read + optimise once per process. RAM = `/usr/bin/time -v` max RSS.

| model (input) | file size | tract OK | load | st median | mt8 median | peak RAM |
|---|---|---|---|---|---|---|
| U2Net-p 320 | 4.6 MB | yes | 0.1 s | 1.03 s | 0.39 s | 0.28 GB |
| silueta 320 | 44 MB | yes | 0.8 s | 2.48 s | n/m | 0.36 GB |
| U2Net 320 | 176 MB | yes | 0.1 s | 1.87 s | n/m | 0.53 GB |
| MODNet 512 | 26 MB | yes | 0.2 s | 0.95 s | 0.43 s | 0.20 GB |
| IS-Net 512 (patched file) | 179 MB | yes (patched) | 0.2 s | 1.18 s | 0.40 s | 0.51 GB |
| IS-Net 768 (patched file) | 179 MB | yes (patched) | 0.2 s | 3.05 s | n/m | 0.69 GB |
| IS-Net 1024 (shipped) | 179 MB | yes | 0.2 s | 5.65 s | 1.86 s (4 thr 2.4 s, 16 thr 2.05 s) | 0.98 GB |
| RMBG-1.4 512 / 768 / 1024 | 176 MB | yes (no patch; sizes accepted) | 0.2 s | 1.16 / 2.89 / 5.59 s | ~0.5 / ~1.5 / 2.35 s | 0.51 / 0.64 / 0.82 GB |
| BiRefNet-lite 384 (emoro24 fp32) | 189 MB | yes | 1.9 s | 1.87 s | 0.98 s | 0.75 GB |
| BiRefNet-lite 1024 | 224 MB | yes | 9.0 s | 30.2 s | 24.5 s (4 thr 26.8, 16 thr 24.8) | 4.33 GB |
| InSPyReNet SwinB 1024 | 395 MB | yes | 2.5-4.3 s | 22.2 s | 10.7 s | 2.4 GB |
| BEN2 Base 1024 | 223 MB fp16 / 409 MB fp32 | fp16: NO; fp32 rewrite: yes | 1.4 s | 24.2 s | 14.2 s | 2.8 GB |
| BiRefNet general 512 | 940 MB | yes | 8.6 s | 10.6 s | 8.1 s | 2.3 GB |
| BiRefNet general 1024 | 973 MB | yes | 15 s | 45.3 s | 34.1 s | 5.4 GB (worst seen: 94 s at machine load 39) |
| RMBG-2.0 1024 | 1.02 GB | yes (ignore value-info) | 6.5 s | 46.5 s | 35.0 s | 5.3 GB |
| BiRefNet fp16 lite (emoro v2) | 123 MB | NO (f16 Gemm) | - | - | - | - |

End to end (separate process, load + one inference, 3 runs) for InSPyReNet: st 24.4 / 26.9 / 27.8 s wall (median 26.9 s, 2.4 GB); mt8 14.8 / 15.0 / 15.2 s (median 15.0 s). Under heavy contention (machine load 20+) single-thread InSPyReNet took 43.8 s inference + 4.3 s load = 48 s, i.e. over the 45 s limit - so for the Better tier enabling multithread-mm is strongly advised.

## Accuracy vs ground truth (16 images: 8 from P3M-500-NP portrait mattes, 8 from DIS5K validation; fixed subset; 1920 px long side)
IoU at 0.5, MAE over whole image, edge MAE = mean |pred-gt| in a 10 px band around the GT edge (lower is better). Caveats: BiRefNet-general / RMBG-2.0 / BEN2 may have seen P3M-10k or DIS5K train data (the test images are held-out splits but not independent of the training distribution); DIS images are mostly single, fine-structure objects (an unfair test for 320-512 px models).
```
model                        | P3M IoU     MAE edgeMAE (n) | DIS IoU     MAE edgeMAE (n)
ben2-fp32                    |   0.987  0.0046   0.077 (8) |   0.704  0.0308   0.159 (8)
birefnet-general-1024        |   0.993  0.0027   0.049 (8) |   0.791  0.0181   0.126 (8)
birefnet-general-512         |   0.991  0.0035   0.065 (8) |   0.709  0.0269   0.156 (8)
birefnet-lite-1024           |   0.991  0.0032   0.059 (8) |   0.706  0.0345   0.121 (8)
birefnet-lite-384            |   0.986  0.0051   0.093 (8) |   0.339  0.1312   0.342 (8)
inspyrenet                   |   0.991  0.0035   0.060 (8) |   0.889  0.0118   0.091 (8)
isnet-1024                   |   0.884  0.0444   0.151 (8) |   0.878  0.0150   0.109 (8)
isnet-512                    |   0.923  0.0361   0.138 (8) |   0.418  0.1957   0.282 (8)
isnet-768                    |   0.884  0.0517   0.154 (8) |   0.764  0.0270   0.156 (8)
modnet                       |   0.977  0.0092   0.100 (8) |   0.054  0.1326   0.424 (8)
rmbg-1.4-512                 |   0.984  0.0124   0.089 (8) |   0.303  0.5432   0.407 (8)
rmbg-1.4-768                 |   0.989  0.0037   0.066 (8) |   0.602  0.0653   0.152 (8)
rmbg-1.4                     |   0.991  0.0033   0.061 (8) |   0.891  0.0094   0.075 (8)
rmbg-2.0                     |                                |   0.886  0.0132   0.077 (3)
silueta                      |   0.921  0.0367   0.156 (8) |   0.269  0.1404   0.383 (8)
u2net                        |   0.922  0.0348   0.154 (8) |   0.270  0.1394   0.373 (8)
u2netp                       |   0.958  0.0175   0.156 (8) |   0.276  0.1409   0.368 (8)
```
(rmbg-2.0 GT run was stopped when Max tier was dropped; 3 DIS images only: IoU 0.886 / edge MAE 0.077. RMBG-1.4 and BiRefNet-general 1024 give essentially the same 0.99 on P3M.)

## Quality notes from the contact sheets (`results/sheets/<image>_<group>.png`, groups fast / fast-final / better / max)
Each sheet: column 0 original; per model: cutout on checkerboard (row 1), zoomed crop (row 2), zoomed alpha (row 3).
- U2Net-p / silueta / U2Net 320: blocky or hazy; fail on portrait hair (silueta: grey haze all over the hair), miss the tree entirely.
- MODNet 512: nice on the portrait, finds nothing in tree/street, DIS objects fail (portrait-only model).
- IS-Net 1024 (shipped): finds fine detail (best on twigs among the old models) but leaves a semi-transparent grey haze next to flyaway hair (portrait), patchy transparency in the crowd; IS-Net 512/768 are cleaner on hair than 1024 but get cat-chest haze (512) and a stray twig (backpack).
- RMBG-1.4 @1024: clean, haze-free hair (best fast model on portrait), whiskers on the cat, strong on branches, no haze on glass. Weakness: on the backpack it also keeps the rock under it (arguably a second subject); 512 is unreliable (produces whole-frame mask on backpack: no confident subject, min-max stretch); 768 is fine on portraits.
- BiRefNet-lite 384 (fp32): clean silhouettes, soft hair, fast, but finds NOTHING on the branches and the crowd (raw max ~0.001) and does badly on DIS (IoU 0.34).
- InSPyReNet 1024: excellent hair/fur strands and whole-cat body, best fine branches (many twigs, light blue fringe), glass bottle clean; best DIS IoU (0.889). Fails on the crowd photo (no subject found; ambiguous scene).
- BEN2 (fp32 rewrite): good hair on the portrait but the tree gets blue blotchy haze, crowd has ghost people, glass edge slightly hazy; DIS IoU 0.70.
- BiRefNet lite 1024: very clean masks on portrait/glass/people, but thinner twigs than IS-Net; BiRefNet general 512: smooth, loses whiskers/fine twigs; BiRefNet general 1024 / RMBG-2.0: best hair edges and clean glass; general-1024 adds a sky patch on the tree; both 35-47 s (Max tier, dropped).

## FINAL RECOMMENDATION (two tiers; Max dropped by the owner)

### Tier 1 - FAST: RMBG-1.4 (BRIA), input 768 on the current single-thread build (2.9 s), 1024 once multithread-mm is on (~2.3 s)
- Why: clean, haze-free hair/fur/glass; P3M IoU 0.991 / edge MAE 0.061, DIS IoU 0.891 (1024) - far better than the shipped IS-Net 1024 (0.884 / 0.151 on P3M: the haze). Same compute as IS-Net 1024 (it is IS-Net-based), 0.8 GB RAM, 0.2 s load.
- Licence: BRIA RMBG-1.4 licence, non-commercial (CC BY-NC style), weights free to download (HF repo `gated: False` when checked 2026-10-13, direct resolve URL works without a token). Fine for a GPL, never-sold app; record it in Settings > Local AI.
- Speed choice: at 1024 single-thread it is 5.6 s (like the shipped IS-Net); to hit <= ~3 s either run it at 768 (2.9 s st, still good on portraits: P3M IoU 0.989, but DIS objects drop to 0.60 and it more often keeps the ground/rock) or turn on tract's `multithread-mm` (1024 -> ~2.3 s, same quality, which I recommend).
- Runner-up: BiRefNet-lite 384 (emoro24 fp32, MIT, 189 MB, 1.9 s st / 1.0 s mt): equally clean edges on people/products/glass and Apache/MIT-free, but it misses the subject on branches and crowds - needs a fallback. (IS-Net 512, Apache-2.0, 1.2 s st, is the permissive-licence alternative but needs a patched file we host; P3M IoU 0.92, haze on fur.)
- Integration (RMBG-1.4):
  - URL: https://huggingface.co/briaai/RMBG-1.4/resolve/2ceba5a5efaec153162aedea169f76caf9b46cf8/onnx/model.onnx (pinned commit)
  - bytes 176153355; sha256 8cafcf770b06757c4eaced21b1a88e57fd2b66de01b8045f35f01535ba742e0f
  - input [1,3,S,S] RGB, S=1024 (768 also accepted by tract: the graph reads sizes dynamically); squeeze-resize (not letterbox); x/255, then (x - 0.5)/1.0 per channel = li-seg `Norm::IsNet`
  - output: one [1,1,S,S] map already in 0-1 (sigmoid in the graph); li-seg's min-max stretch then bilinear upscale works as-is (what I used)
  - li-seg change needed: `Segmenter::load_cfg` must not call `.with_ignore_output_shapes(false)` for Subject models (the file declares symbolic output dims) - drop the call (tract default) or pass `true`.

### Tier 2 - BETTER QUALITY: InSPyReNet SwinB Plus-Ultra, 1024 (community ONNX, no conversion needed)
- Why: compared with BEN2 across all six images and the GT subset it is better everywhere: P3M IoU 0.991 / edge MAE 0.060 (BEN2 0.987 / 0.077), DIS IoU 0.889 / edge MAE 0.091 (BEN2 0.704 / 0.159); BEN2 showed blotchy blue haze on the tree and ghosting in the crowd. Best fur and branch detail of everything under 45 s. Loads natively in tract (BEN2 does NOT: needs our own fp16->fp32 rewrite).
- Time: 22 s inference + 2.5 s load = ~27 s end to end (median of 3) with the current single-thread build; ~15 s with multithread-mm (8 threads). Worst case seen under heavy machine load: 48 s single-thread, so turn multithread-mm on. Peak RAM 2.4 GB. Model 395 MB.
- Licence: MIT (InSPyReNet, plemeri) - the ONNX is a community conversion by OS-Software (HF), no separate restriction stated. Trained on public SOD/DIS-type data.
- Known weakness: found no subject on the ambiguous crowd photo (output max < 0.5), so handle "no subject" gracefully (li-seg `predict` already returns None when max < 0.5).
- Runner-up: BiRefNet general 512 (onnx-community, MIT, 940 MB download, 10.6 s + 8.6 s load ~ 19 s end to end st, 2.3 GB; P3M IoU 0.991, DIS 0.709, smooth but loses whiskers/twigs). Third: BEN2 only if we host the fp32 rewrite (409 MB, sha256 8a12f0f9119f2e62e8ab845324b7838c7c52fa2eeaa7d2e177fbf250f7b57ad1).
- Integration (InSPyReNet):
  - URL: https://huggingface.co/OS-Software/InSPyReNet-SwinB-Plus-Ultra-ONNX/resolve/fe628d2d1465f775c707368a80248a2d5b5ad71f/onnx/model.onnx (pinned commit; repo `gated: False`)
  - bytes 395316574; sha256 c108dd92b3ddfe3a2d9e9ac2b74730cc5acbb2ddc7ea863330c43f56ae832aa3
  - input name `input`, [1,3,1024,1024] fixed, RGB, squeeze-resize; x/255 then ImageNet mean (0.485,0.456,0.406) / std (0.229,0.224,0.225) = `Norm::ImageNet`, NO peak scaling (li-seg `predict` divides by the image peak; skip that or it is harmless for normal photos)
  - output [1,1,1024,1024] already 0-1 (sigmoid applied; raw range 0..1). Do NOT min-max stretch: when there is no subject the stretch would blow noise up into a mask - use the values as-is (clamp 0-1), keep the `max < 0.5 => no subject` rule, bilinear upscale to photo size.
  - same `with_ignore_output_shapes` fix as above (not strictly needed for this file, which loads either way).

### Things we must do in the app to integrate
1. Enable tract multithreading: add a direct `tract-linalg` dependency with feature `multithread-mm` in `crates/li-seg/Cargo.toml` and call `tract_linalg::multithread::set_default_executor(Executor::multithread(n))` once at start (n = min(8, cores)). Conv nets speed up ~2.5-3x, transformers ~1.2-2x. (Cargo.lock changes by one dependency line; tract-linalg and rayon are already in the lock.) NOT done here.
2. Fix output-shape handling in `Segmenter::load_cfg` (above) and add a per-model "post" mode (min-max for IS-Net/U2Net-family, as-is for RMBG-1.4 / InSPyReNet).
3. Models are not on the same download host as rembg: RMBG-1.4 and InSPyReNet both come from Hugging Face (pin by commit as above). No file conversion is needed for either pick.
4. Example harness: `crates/li-seg/examples/bg_eval.rs` (uncommitted in worktree wip/bg-models). Contact sheets: `results/sheets/*.png`; raw alpha/cutout PNGs: `results/raw/<model>/<image>_{alpha,cutout}.png` and `results/gt/<model>/`. M4 Max: expect similar or faster single-thread numbers (NEON kernels in tract are well optimised) - not measured.
