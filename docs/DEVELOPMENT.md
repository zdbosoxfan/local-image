# Build, test and package Local Image 2

Local Image 2 is one Rust workspace (edition 2024, Rust 1.98.1 toolchain) on `main`.
The earlier 0.7 Python/WebView implementation remains available in Git history.

## Prerequisites

* Rust 1.98.1 (`rustup toolchain install 1.98.1`) with `rustfmt` and `clippy`.
* Linux: the windowing and dialog libraries egui, winit and rfd need:
  ```sh
  sudo apt-get install libxkbcommon-dev libwayland-dev libx11-dev libxrandr-dev libxi-dev \
    libgl1-mesa-dev libgtk-3-dev mesa-vulkan-drivers
  ```
* macOS: Apple Silicon, macOS 13+, Apple command line tools (Metal kernels compile through the system runtime).
  See [MACOS.md](MACOS.md) for native packaging, Metal validation and remote generation.
* Windows: the MSVC toolchain (Visual Studio Build Tools) and the Windows SDK.
* [craft-fonts](https://github.com/storytold/craft-fonts) supplies Japanese, Simplified Chinese,
  Arabic and Thai fallback fonts. CI and release builds use commit
  `3d9ab3a132ed0eff457e424226a435a901da30d3`, with `CRAFT_FONTS_DIR` pointing at that checkout
  and `CRAFT_FONTS_REQUIRED=1`. Local builds may omit it, but then cannot prove those scripts render.
* Full SVG/PDF export tests require **Inkscape 1.4+** with `--app-id-tag`, and Poppler's
  **pdftoppm** on `PATH`. These are independent test renderers, not runtime app requirements.
  On Ubuntu use the [official stable Inkscape PPA](https://wiki.inkscape.org/wiki/Installing_Inkscape)
  if the distribution package is too old, then install `inkscape poppler-utils`.
  Experimental Windows test hosts can use Chocolatey [Inkscape 1.4.4](https://community.chocolatey.org/packages/inkscape)
  and [Poppler 26.7.0](https://community.chocolatey.org/packages/poppler/26.7.0).
  Set `LOCAL_IMAGE_INKSCAPE` to the executable when it is not on `PATH`; on Windows use
  `C:\Program Files\Inkscape\bin\inkscape.com`. Full nightly hosts must verify both tools before starting the workspace suite.

## Crypto provider exception

TLS uses rustls with the ring provider because the pure-Rust rustls-rustcrypto provider carried
unfixed RustSec advisories. Under the 2026-10-25 TLS security decision, ring (`Apache-2.0 AND ISC`)
is the only allowed C/assembly dependency, and only under rustls (directly or via rustls-webpki).
The `deps-pure` check enforces this exception: `ring` is allowed only under `rustls` or
`rustls-webpki`, and its `cc` build dependency only under `ring`; existing known `cc` parents
are listed separately in xtask.

## Build and run

```sh
cargo run --release -p local-image                 # the app
cargo run --release -p local-image -- photo.jpg    # open files
cargo run --release -p local-image-cli -- --help   # headless: convert, info, run, batch, MCP
cargo build --release -p local-image --features heif   # with HEIC/HEIF support
```

Generative workflows can use a local or remote [ComfyUI](https://github.com/comfyanonymous/ComfyUI)
server, configured in **Edit › Preferences › Local AI…**, or an optional cloud provider with
your own credentials. Native selection, tagging and other on-device tools use separately
downloaded models; native RAW and RGB AI denoise require a supported GPU. Core editing
does not require an AI server. See [MACOS.md](MACOS.md) for backend requirements.

## Layout

```
apps/local-image       the desktop app (window, services, raw import, Library mode host, branding)
apps/local-image-cli   the command line (a library the app also links: packages ship one executable,
                       and `local-image` started as `local-image-cli` runs the CLI)
crates/pc-*            the editor: document model, engine and commands, compositor, PSD, colour
                       management, paint, text, codecs, egui UI (from PhotoCraft)
crates/lc-*            raw development, pipeline, catalogue, export, Library UI (from LightCraft)
crates/li-ai           ComfyUI client, model families, detection, workflow builders, custom
                       workflows, presets, Model Browser data, verified downloads, mock server
crates/li-ai/families  one JSON profile per model family, plus index.json (versions)
crates/li-seg          the CPU segmentation model behind Quick selection and backgrounds
```

## Tests

```sh
cargo +1.98.1 xtask test-fast    # portable PR lane, warm cache target: < 5 minutes
cargo +1.98.1 test --offline --workspace --no-fail-fast # nightly full lane
cargo test -p li-ai             # AI: graphs, detection, profiles, browser, every operation vs the mock
cargo test -p photocraft-engine -p photocraft-ui-egui --lib
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

During development, run focused regressions first (for example,
`cargo test -p photocraft-engine --lib prefs::tests`), then affected suites at a stable integration
boundary. Run the workspace suite and strict lint for the final candidate. Keep one Cargo build
lane per target directory, retain its cache, and avoid routine `cargo clean` or overlapping full
suites. The launcher and fast CI use four build jobs; do not override the launcher’s concurrency.
See [RELEASE.md](RELEASE.md) for the full nightly and mandatory hardware GPU lanes.

### The mock ComfyUI

`li-ai`'s tests run every AI operation against an in-process mock of ComfyUI's HTTP API that also
serves stand-ins for Hugging Face, Civitai, the ComfyUI-Manager list and the official workflow
templates (fixtures in `crates/li-ai/tests/fixtures/templates`). For demos and screenshots, run it
as a server and point the app at it:

```sh
cargo run --release -p li-ai --example mock_comfy -- 8199 /tmp/li-models &
export LOCAL_IMAGE_COMFY=127.0.0.1:8199 LOCAL_IMAGE_TEST_DOWNLOAD_HOST=127.0.0.1:8199 \
  LOCAL_IMAGE_HF_BASE=http://127.0.0.1:8199/hf LOCAL_IMAGE_CIVITAI_BASE=http://127.0.0.1:8199/civitai \
  LOCAL_IMAGE_MANAGER_LIST=http://127.0.0.1:8199/manager/model-list.json \
  LOCAL_IMAGE_MODELS_DIR=/tmp/li-models LOCAL_IMAGE_DATA_DIR=/tmp/li-data
cargo run --release -p local-image
```

### Screenshots

The `snapshot` example renders the whole UI offscreen and saves a PNG; `--script` drives it with
control-protocol calls plus `sleep` and `click` steps:

```sh
cargo run -p photocraft-ui-egui --example snapshot -- --out browser.png --size 1440x900 --scale 1 \
  --script '[["ui.menu.invoke", {"id": "li.browseModels"}], ["sleep", {"ms": 1500}]]'
```

## Model family profiles

A family is a JSON file in `crates/li-ai/families/` (schema in `crates/li-ai/src/family.rs`).
Derived families set `base` and override only what differs. To add or change one:

1. Edit or add the profile; bump its `version` and the matching entry in `index.json`.
2. `cargo test -p li-ai` checks every profile (files need a 64-hex SHA-256, a size, and a
   `.safetensors` or `.gguf` name) and builds every family's graphs against the mock.
3. Once merged to `v2`, apps pick it up with **Model Browser › Update Model Profiles**.

## Packaging

Packaging is Rust too (`xtask/`, run through the `cargo xtask` alias):

* **macOS:** `cargo xtask package macos` creates an ARM64 `.app` and `.pkg` installer with
  HEIF enabled and SAM off, plus its checksum. See [MACOS.md](MACOS.md) for signing.
* **Linux:** `cargo xtask package linux` writes
  `dist/release/local-image-<version>-linux-<arch>.tar.gz` and its `.sha256` (app, CLI, README,
  licences and all referenced attribution texts). Run `cargo xtask attributions` after changing
  the curated credits, port records or dependency graph. After unpacking, `./local-image --install` adds it to the application menu for the
  current user (`--install --system` for `/usr/local`, `--uninstall` to remove it).
* **Windows:** `cargo xtask package windows --arch x64` builds with a static CRT, checks the PE
  headers, and writes an MSI (WiX v5: `dotnet tool install -g wix`) and a portable zip
  (`portable.txt` keeps settings in `LocalImageData\` beside the exe). Both are signed when
  `WINDOWS_CERTIFICATE` or the Azure Trusted Signing variables are set (`cargo xtask sign`).
* `cargo xtask check-icons` checks the MSI's shortcut icons (ICE50) without building.
* `cargo xtask deps-pure` rejects C-building crates across all platforms, except the known `cc`
  parents in xtask and the [2026-10-25 TLS security decision](#crypto-provider-exception):
  `ring` under `rustls` or `rustls-webpki`, and `cc` under `ring`. It also runs as an xtask unit
  test. HTTPS uses rustls
  with ring's C/assembly crypto (set per agent in `crates/li-ai/src/tls.rs`), and blake3 uses
  its `pure` feature.

The version comes from `[workspace.package] version` in the root `Cargo.toml`.
