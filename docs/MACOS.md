# macOS on Apple Silicon

Local Image 2 supports Apple Silicon (M1 or later) on macOS 13 or later. The native ARM64
package needs no Rosetta, Python, Homebrew or Rust at runtime. HEIC/HEIF support and local
SAM 3 masks are enabled in the macOS package; model weights are separate downloads.

## Install

Open `local-image-<version>-macos-arm64.pkg` and follow Installer. It installs
`/Applications/Local Image.app`, with the CLI in `Contents/MacOS/local-image-cli`.
Finder's Open With can open native `.pcraft` documents, images, raw photos, PDFs and SVGs.
Settings, AI data and models live in `~/Library/Application Support/Local Image`.

Development packages use an ad-hoc app signature and an unsigned installer. Public distribution
requires a Developer ID Application identity, Developer ID Installer identity and Apple
notarization. Do not describe an ad-hoc package as notarized. With a signed package, submit it
using `xcrun notarytool submit <package> --keychain-profile <profile> --wait`, then staple and
validate it using `xcrun stapler staple <package>` and `spctl -a -t install -vv <package>`.

## Acceleration

* The window and compositing engine use wgpu's Metal backend. Automatic rendering selects
  the Apple GPU; Performance preferences and System Info report the active adapter.
* Develop uses Metal compute for its supported raw/development kernels and AI denoise graphs.
  Capture sharpening uses reference CPU control planes to preserve zero-mask decisions, while
  its deconvolution iterations run on Metal. Unsupported operations keep their existing CPU paths.
  ARM64 SIMD and the shared Rayon thread pool accelerate CPU processing; the application respects the unified memory budget.
* SAM 3 Object/Describe masks use Candle Metal with the `sam` feature in packaged builds.
  Download/licence acceptance is required before using the actual checkpoint.
* The small ONNX models use tract's native ARM64 CPU kernels. They do not use the Neural Engine.
* Local ComfyUI can use PyTorch MPS. The app never forces CUDA or CPU mode on a Mac; unsupported
  PyTorch operators can fall back to CPU. Large generative models should run on a remote GPU
  server. Model-specific MPS support varies; the package does not claim that every model runs
  on the Mac. No Core ML/Neural Engine integration is claimed.

## Remote generation (recommended for large models)

1. On your GPU machine, install the models and run ComfyUI listening on an address reachable
   from your Mac, for example `python main.py --listen 0.0.0.0 --port 8188`.
2. On the Mac open **Preferences > Local AI**, select **Use a remote ComfyUI server**, enter
   the server's LAN IP or Tailscale hostname (without `http://`) and port, then click
   **Connect / Check Server**. The connected GPU and available models come from that server.
3. Generate, fill, remove and enhance use the remote API. Source images/masks are uploaded and
   finished images are downloaded into the Mac's document/library. There is no shared folder
   requirement. Install/remove generative model weights on the server; on-device models remain
   independent and can still be downloaded onto the Mac.

For an SSH tunnel, run `ssh -N -L 8188:127.0.0.1:8188 user@gpu-server`, leave it running,
then use remote mode with `127.0.0.1` and port `8188`. The explicit remote checkbox prevents
Local Image from starting a local server or placing generative weights on the wrong machine.
Direct connections use HTTP/WebSocket; use a trusted LAN, Tailscale or SSH tunnel. The client
currently does not provide HTTPS, proxy-path or authentication settings for a public endpoint.

## Build and validate

Install Rust stable and Apple's command line tools. Candle 0.9.2 compiles its embedded Metal
kernels through the system Metal runtime; it needs no separate `metal` compiler executable. Clone the pinned fonts as described in DEVELOPMENT.md.

```sh
export MACOSX_DEPLOYMENT_TARGET=13.0
export CRAFT_FONTS_DIR=/absolute/path/to/craft-fonts CRAFT_FONTS_REQUIRED=1
cargo xtask package macos
./script/build_and_run.sh --verify
```

`cargo xtask package macos --skip-build` packages existing native release binaries. The caller
must ensure those binaries have `local-image/heif,local-image/sam,local-image-cli/heif` enabled.
It checks ARM64 architecture, rejects non-system dylibs, includes attribution texts, creates
an icon and document associations, signs nested code, verifies the app signature, builds the
Installer package and writes a SHA-256 file. It disables Installer bundle relocation so an
upgrade replaces the app in Applications instead of a copy elsewhere.
Set `MACOS_SIGN_IDENTITY` and `MACOS_INSTALLER_SIGN_IDENTITY` for Developer ID signatures.

```sh
WGPU_BACKEND=metal LOCAL_IMAGE_REQUIRE_GPU=1 cargo test -p photocraft-gpu --test gpu --test vector_geometry
LC_REQUIRE_GPU=1 cargo test -p lightcraft-gpu --lib ds_finding_native_haze_selection_matches_sorted_quantile -- --ignored --nocapture
cargo test -p li-ai
cargo test -p local-image --features heif,sam
cargo test -p xtask
```

The macOS workflow runs native ARM64 tests and uploads the `.pkg` and checksum. GPU acceptance
must use physical Metal hardware; an adapter skip or software fallback is not proof of acceleration.
Real generative model acceptance belongs on the remote server, not the Mac. Optional SAM model
acceptance needs the checkpoint; small synthetic tensor tests can verify Metal without it.

References: [Comfy Desktop for macOS](https://docs.comfy.org/installation/desktop/macos),
[Rust Apple targets](https://doc.rust-lang.org/rustc/platform-support/apple-darwin.html),
[GitHub macOS runner labels](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
