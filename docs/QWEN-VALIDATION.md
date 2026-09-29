# Qwen Image 2.1 and Cutout validation — 0.4.0

Validated on 29 September 2026 against local ComfyUI. Both Compact INT8 and Full
BF16 were present in the live loader inventory. The main acceptance run used
INT8; BF16 was separately exercised on the difficult cat extraction. These are
individual local runs, not a general speed or quality benchmark.

User guides: [Cutout workspace](CUTOUT-WORKSPACE.md),
[Qwen models, setup, and license](QWEN-IMAGE-21.md), and
[test commands](DEVELOPMENT.md).

## Fixtures and provenance

The source fixtures were generated with the built-in **imagegen** tool, not with
Qwen. Both are 1536 × 1024 PNGs under `qa-artifacts/fixtures/`. Reconstructed prompt
descriptions, rather than verbatim tool prompts:

- **backpack-cup.png:** a detailed tan canvas backpack with straps and buckles on
  a textured stone garden ledge; a separate red ceramic cup on the right;
  foliage and brick behind it, natural sunlight. Tests product edges, an object
  removal with surrounding texture, and background replacement.
- **cat-fur.png:** a cream-and-ginger long-haired cat sitting on a dark blue
  armchair in a furnished, plant-filled room lit by a window. Fine fur, whiskers,
  paws, and tail provide a harder extraction case.

The derived removal, extraction alpha, and generated studio background were
produced by actual Qwen Image 2.1 inference through ComfyUI. Local Remove applies
the alpha to the existing photo pixels, then performs background compositing,
shadows, and transforms locally. The exported subject is not a newly generated
replacement for the source subject.

## Live results

Evidence: `qa-artifacts/qwen/live-final/results.json`,
`qa-artifacts/qwen/bf16/results.json`, and
`qa-artifacts/qwen/packaged-live/results.json`, with matching status files and PNGs.
All three result files report `complete: true`. QA artifacts are local, ignored files;
they are not included in a fresh source checkout.

| Operation | Variant | Recorded seconds | Verified output |
| --- | --- | ---: | --- |
| Remove red cup and its shadow | INT8 | 22.66 | RGB repair, 1536 × 1024 |
| Extract backpack | INT8 | 14.52 | RGBA, alpha 0–255 |
| Generate empty studio background | INT8 | 8.34 | Fully opaque 1536 × 1024 composite |
| Apply editable shadow | Local compositor | 0.00* | Opaque composite with shadow |
| Move, scale, and rotate subject | Local compositor | 0.02 | Transformed composite |
| Extract cat with fine fur | INT8 | 14.58 | RGBA, alpha 0–255 |
| Extract cat with fine fur | BF16 | 24.92 | RGBA, alpha 0–255 |
| Extract cat through packaged backend | INT8 | 14.52 | RGBA, alpha 0–255 |

*The runner rounds to two decimals; 0.00 does not mean zero execution time.*

The runner used 25 Euler steps, the simple scheduler, and guidance 1. Removal and
backpack extraction used seed 42; background generation and cat extraction used
seed 123. Edit instructions requested the cup and its shadow removed while keeping
the backpack; the backpack alone extracted with straps, handle, and buckles; and
the cat alone extracted with fur, whiskers, paws, and tail. The background request
described a warm beige studio wall, matte stone tabletop, upper-left window light,
and an empty foreground. Full instructions are in `tests/smoke_qwen_live.py`.

The object-removal test checked the entire zero-valued region outside the drawn
selection. Independent comparison of the final PNG with the original confirmed
**1,512,573 outside-mask pixels unchanged**, **0 changed pixels**, and **0 maximum
channel difference**. This preservation is enforced by masked compositing, rather
than relying on the model to leave the rest of the image untouched.

The backpack cutout contains 74.057% fully transparent pixels and 0.586% soft-alpha
pixels. The cat contains 87.687% transparent and 1.394% soft-alpha pixels with
INT8; BF16 produced 87.692% and 1.356%, respectively. These percentages describe
the outputs and are not accuracy scores. The final studio composites have alpha
255 everywhere. The packaged-backend cat run matched INT8's transparency and
soft-alpha fractions. Editable project export also completed.

## Visual findings and limits

The cup was removed, the backpack was isolated, and Qwen generated an empty studio
background that could be combined with the cutout, a shadow, and changed subject
placement. The cat outputs retain the overall subject but show **blue chair
fringes along parts of the fine-fur boundary and loss of some fine whiskers**.
BF16 did not eliminate those visible problems in this example. Difficult fur,
hair, and translucent edges still need brush or pen refinement; these two fixtures
do not establish general extraction quality.

Initial runs revealed alpha 1 residue in otherwise transparent areas and alpha
254 in otherwise opaque regions. Final output cleanup runs after resizing and
maps only 0–1 to 0 and 254–255 to 255, preserving every value from 2 through 253.
Background plates are made fully opaque without changing their RGB pixels.
This removes endpoint residue; it does not repair color contamination or missing
whiskers. At guidance 1, negative conditioning is inactive, so empty-background
constraints also appear in the positive instruction. An empty result is not
guaranteed for other prompts.

## Automated application checks

Reported final Python suite results:

| Suite | Tests |
| --- | ---: |
| App paths | 5 |
| Managed AI | 23 |
| Setup routes | 6 |
| Frontend rendering | 3 |
| Layer projects | 28 |
| Backend texture | 21 |
| Fast inpaint | 8, including one skipped private fixture |
| Qwen adapter | 14 |
| Cutout | 24 |
| Qwen setup/downloads | 8 |

All executed tests passed. The five Playwright suites passed: navigation,
projects/layers, browser, setup, and cutout. Checks cover the conditional bottom
filmstrip, workspace switching, alpha refinement, background import/library,
shadows, transforms and refinements after transforms, project round trips, PNG
export, and model availability. Unit tests also check unchanged pixels outside
removal masks, alpha endpoints, native-precision composition, download integrity,
and native authorization. Mocked AI responses in application tests are separate
from the live GPU evidence above.

The browser and setup suites each cover four desktop sizes. Setup tests include
BF16 download dispatch and Qwen-only readiness. Browser evidence is under
`qa-artifacts/cutout-ui/`, `qa-artifacts/browser-final/`, and
`qa-artifacts/setup-review/`.

The packaged 0.4.0 native self-test passed 12 trusted-origin, 7 project-boundary,
7 close-handshake, and 6 setup-bridge checks. Packaged smoke tests passed compressed
16-bit TIFF input, both CPU healing methods, project export, source preservation,
and rejection of unauthorized configuration changes. The packaged Cutout
Playwright suite passed with mocked AI capabilities/generation. Source and packaged
assets matched by hash. The actual native WebView probe passed using runtime
154.0.4258.37, loading `http://127.0.0.1:51247/remove` with title **Local Remove**;
evidence is `qa-artifacts/packaged-webview.json`. This probe succeeded in normal
unrestricted execution after restricted-shell initialization stalled.

The built installer is `dist/installer/Local-Remove-Setup-0.4.0.exe`
(78,602,162 bytes), with its adjacent `.exe.sha256` checksum. These results cover
the built package and probe; they do not claim a separate clean-machine installer
acceptance run.
