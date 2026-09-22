# Windows 0.3.0 and AI setup validation

Verified on 22 September 2026 on Windows 11 x64 with an NVIDIA RTX 5090.
This continues the initial source-interface review in UI-VALIDATION.md.

## Installed acceptance

- Built the standalone Python backend, native WebView2 host, and per-user Inno installer. Installed under the normal Windows profile; verified native startup and desktop/Start menu targets. Application code does not require the source checkout or a separate Python installation.
- Native host self-test and WebView2 navigation probe passed. Packaged smoke exercised both healing methods on a compressed 16-bit TIFF, editable project saving, original preservation, and rejection of unauthenticated configuration changes.
- Detected the existing ComfyUI installation and reused the existing shared FLUX folder. Verified all four components by SHA-256, including the supported older official VAE. No existing model files were replaced.
- Downloaded the complete pinned ComfyUI 0.37.0 NVIDIA archive (1,925,204,508 bytes), verified its SHA-256, and extracted it using the bundled 7-Zip helper into a new dedicated LocalRemove-ComfyUI folder. Started that runtime on loopback port 8188; left the earlier service on 8189 untouched.
- Independently downloaded the complete pinned 76,038,936-byte FLUX removal adapter from Hugging Face through the downloader and verified its SHA-256. The installed model verification operation reused existing files.
- Ran a real FLUX edit on a newly generated 512-by-384 scene. It completed in 22.92 seconds, produced one editable FLUX layer, removed the selected synthetic object, preserved the original file byte-for-byte, and saved an editable project and preview. The test closed only its own session afterward. This is a single acceptance measurement, not a general performance claim.
- Sent the queue-checked GPU unload request. The server stayed running with an empty queue; reported free GPU memory rose from 16,293,183,272 to 32,299,286,528 bytes. A small CUDA/PyTorch context allocation remained. The UI correctly says the unload was requested rather than claiming immediate total GPU release.
- Removed the dedicated old Documents launcher/browser files and four obsolete Qwen image-edit weights. Preserved FLUX's Qwen3 text encoder, other applications, shared backend code, and photo/recovery data. No copies of those four obsolete weight files remained in the checked Documents, Downloads, ComfyUI-Installs, or ComfyUI-Shared folders.

## Defects caught and corrected

- The live model host redirects to an additional official Hugging Face CDN hostname. Added the verified hostname and rejection tests for lookalike domains.
- A running ComfyUI process is not sufficient proof of readiness. Check the required workflow nodes, encoder type, and all four loader filenames; report missing paths or unsupported runtimes without restarting another service.
- Windows package folder redirection can resolve a logical AppData directory to a different physical path. Preserve the native host's logical absolute profile identity and retain launcher-key authorization. Verified ordinary startup through Windows Explorer outside the development host's package context.
- The full official ComfyUI archive uses BCJ2 compression, which the Python decoder could inspect but could not decode. Bundle the official native 7-Zip helper, matching source and notices. Validate archive metadata before decoding and the extracted file set afterward. A real BCJ2 regression and the complete installed archive passed.
- Installation could complete during a status request's service probe. Read installation state after that await so the completed job returns the new runtime and enables Start consistently; an asynchronous regression covers this race.
- A packaged backend's DLL search path was inherited by ComfyUI, which locked an application DLL during updates. Clear the packaged DLL directory and bundled PATH entries when launching the separate runtime, then restore the application process's settings. Regression coverage verifies restoration after both successful and failed launches. The final installed process loaded no DLLs from the app bundle and used Windows' own `msvcp140.dll`; a second real FLUX edit, project save, and GPU unload passed. The native desktop window was opened and verified responsive.

## Regression coverage

Python suites cover profile storage, setup/download/extraction, authenticated setup routes, FLUX-only settings and legacy setting migration, project/layer behavior, native-pixel healing, and frontend rendering. All applicable checks pass; the existing protected-photo texture fixture remains skipped because that private fixture is unavailable.

JavaScript navigation and project/layer suites pass. Real Edge setup tests cover 1366x768, 1024x768, and 768x900 layouts; all seven native-action payloads; keyboard focus containment; download progress/error states; busy-state controls; running-but-unready ComfyUI; and browser-only restrictions. Their native bridge is a fixture, so they do not themselves install software.

These are developer reviews and automated tests, not a human focus group. Capture One handoff and every native picker interaction were not manually exercised in this pass. The GPU test uses synthetic content, not a representative photographic quality study.
