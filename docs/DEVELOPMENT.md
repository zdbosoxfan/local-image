# Build, test and package Local Image 2

Local Image 2 is one Rust workspace (edition 2024, stable toolchain). The 0.7 Python/WebView build
lives on `main` until V2 replaces it.

## Prerequisites

* Rust stable (`rustup default stable`) with `rustfmt` and `clippy`.
* Linux: the windowing and dialog libraries egui, winit and rfd need:
  ```sh
  sudo apt-get install libxkbcommon-dev libwayland-dev libx11-dev libxrandr-dev libxi-dev \
    libgl1-mesa-dev libgtk-3-dev mesa-vulkan-drivers
  ```
* Windows: the MSVC toolchain (Visual Studio Build Tools) and the Windows SDK.
* Optional: [craft-fonts](https://github.com/storytold/craft-fonts) next to the repository
  (`CRAFT_FONTS_DIR=../craft-fonts`) for Chinese and Japanese UI text.

## Build and run

```sh
cargo run --release -p local-image                 # the app
cargo run --release -p local-image -- photo.jpg    # open files
cargo run --release -p local-image-cli -- --help   # headless: convert, info, run, batch, MCP
cargo build --release -p local-image --features heif   # with HEIC/HEIF support
```

AI features need a running [ComfyUI](https://github.com/comfyanonymous/ComfyUI); set it up in
**Edit › Preferences › Local AI…**. Without one, everything else works.

## Layout

```
apps/local-image       the desktop app (window, services, raw import, Library mode host, branding)
apps/local-image-cli   the command line
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
cargo test --workspace          # everything (what CI runs, with fmt and clippy -D warnings)
cargo test -p li-ai             # AI: graphs, detection, profiles, browser, every operation vs the mock
cargo test -p photocraft-engine -p photocraft-ui-egui --lib
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Set `CARGO_INCREMENTAL=0` on machines short of disk space.

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

* **Linux:** `cargo xtask package linux` writes
  `dist/release/local-image-<version>-linux-<arch>.tar.gz` and its `.sha256` (app, CLI, README,
  licences). After unpacking, `./local-image --install` adds it to the application menu for the
  current user (`--install --system` for `/usr/local`, `--uninstall` to remove it).
* **Windows:** `cargo xtask package windows --arch x64` builds with a static CRT, checks the PE
  headers, and writes an MSI (WiX v5: `dotnet tool install -g wix`) and a portable zip
  (`portable.txt` keeps settings in `LocalImageData\` beside the exe). Both are signed when
  `WINDOWS_CERTIFICATE` or the Azure Trusted Signing variables are set (`cargo xtask sign`).
* `cargo xtask check-icons` checks the MSI's shortcut icons (ICE50) without building.

The version comes from `[workspace.package] version` in the root `Cargo.toml`.
