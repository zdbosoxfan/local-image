# Compact desktop workspace, 0.3.1

This revision restores the compact Photoshop-style structure the user preferred before the 0.3.0 redesign. The design basis and primary Adobe/Microsoft references are in [DESKTOP-REFERENCES.md](DESKTOP-REFERENCES.md).

## Changes

- Replace the 58 px header, 49 px options row, 47 px document row and 37 px status row with 28/36/30/24 px desktop chrome. Restore a 40 px icon tool strip and 240 px Layers dock.
- Remove the instructional repair panel, illustration, promotional copy, separate layer cards, and persistent duplicates of Open, Save, Export and Merge. Put repair settings and one Apply action in the contextual options row.
- Keep occasional commands in File, Edit, Layer, Select and View menus. Copy format and recent sessions use side-opening submenus. Settings is reached through Edit. Model details are collapsed inside Settings.
- Use neutral gray surfaces, joined layer rows, small square controls and conventional menu selection states. Show a folder strip only for multiple images. Hand mode hides repair options and preserves the pending selection.
- Preserve native setup/download/start/eject controls, model configuration, editable projects, comparison, and overwrite/close protections.

## Verification

Verification is performed in an isolated development profile with synthetic images. No personal photos are used, and no human focus group is claimed.

- Local asset rendering and profile path tests pass.
- JavaScript navigation and project/layer regression suites pass, including single-photo strip suppression and preserving selections across Hand mode.
- Setup browser checks cover four desktop sizes, complete Tab/Shift+Tab focus traversal, collapsed model details, native bridge actions, status/errors/progress and browser-only restrictions.
- The complete browser editing pass covers five top-level menus, side-submenu keyboard/hover behavior, checked output formats and persistence, panel-menu focus, Alt/F10/Tab navigation, real Quick Heal, layer visibility, image export and project saving. It also verifies unavailable AI cannot submit a repair, Hand mode preserves selections, and single-photo collections do not expose a folder strip.
- Layout assertions pass at 1440×900, 1280×720, 1024×768 and 800×560: usable canvas proportions, flat layer rows, one compact options row, and reachable Apply and File saving commands.
- Visual inspection covers neutral chrome, flat Layers, menu appearance and setup layout. At 800 px width the toolbar remains one 36 px row with Apply in view.

## Installed acceptance

Built and installed 0.3.1 on Windows 11. Verified the installed launcher, backend, HTML, CSS and JavaScript hashes against the tested package. Native self-tests pass (12 origin, 7 project boundary, 7 close handshake and 6 setup bridge checks), and the WebView2 probe loads the trusted editor route successfully. The installed backend reports 0.3.1 and serves the compact layout with side submenus and no repair panel.

Opened the installed desktop window and verified it is responsive. Existing model and dedicated ComfyUI folders are retained; all four required files are present and the AI service remains ready. This UI revision did not change inference or downloader behavior; real FLUX/GPU acceptance for those components is recorded in [AI-SETUP-VALIDATION.md](AI-SETUP-VALIDATION.md).


## September 30 review preview: generation and Assets

This remains an unmerged draft on the private repository. The review build adds a bottom-canvas prompt/action composer, closed single-open inspector disclosures, independent Edit/Create/Draft & Refine toolbar modes, linked aspect ratios, sourced step recommendations, and Generated images docked inside Assets. Pexels and Unsplash join Openverse in a plain source dropdown. Their API keys are entered in the app and protected with Windows DPAPI; no keys were available for live Pexels/Unsplash search testing. Provider fixtures verify search/import attribution, Unsplash hotlink/download tracking, CSRF and credential host isolation. Live Openverse returned 12 images and a valid thumbnail.

Generation and upscale validation use actual connected workflow constraints. There is no application 4 MP/4096 generation cap or 16 MP/4096 upscale cap. The connected latent nodes report 16384 per side and no area cap; Qwen's reference encoder and Z variations do not inherit an unused latent-node maximum. Qwen uses 16-pixel generation alignment and 32-pixel reference alignment. SeedVR2 retains its real even-dimension requirement because its native postprocessor crops odd output edges. Defaults remain conservative and are separate from those limits.

Focused backend tests cover exact 4K/8K graph dimensions, current node constraints, wrong-size output rejection, legacy metadata, 4096 by 2304 library recovery, sampling defaults and upscale alignment. Browser checks pass without script injection for the composer, linked sizes, Stock source connections, Generated docking/expansion, selection/deletion and actual local image imports/exports. Screenshots were inspected at 1440 and 1024 pixels. A real Windows close test exposed a transient file-handle race; bounded staging/rollback rename retries now pass both transient-release and persistent-failure recovery tests, and the browser close test passes against the real endpoint.

A real Z-Image Turbo run at 3840 by 2160, eight steps, completed and exported an exact-size PNG in 45.59 seconds on the RTX 5090. This validates the larger-output path, not image fidelity: visual inspection found texture artifacts in direct 4K generation. The image is a technical test artifact, not a selected promotional example. Evidence lives under `qa-artifacts/ui-redesign/large-generation/`.

The real WebView2 preview probe passed with native setup enabled, all three mode controls in the toolbar, collapsed sections, adjacent prompt/action, no standalone Library shortcut, all three Stock providers, Qwen 40-step guidance and no pixel cap. Existing visible user windows were not closed.
