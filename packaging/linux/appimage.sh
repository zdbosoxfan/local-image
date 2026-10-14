#!/bin/bash
# Assemble the existing xtask payload into the standard AppDir format.
set -euo pipefail
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || { echo 'AppImage packaging requires native Linux x86_64' >&2; exit 1; }
: "${APPIMAGETOOL:?set APPIMAGETOOL to the verified appimagetool executable}"
: "${APPIMAGE_RUNTIME:?set APPIMAGE_RUNTIME to the verified type-2 runtime}"
repo=$(cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$repo"
cargo +1.98.1 xtask package linux --skip-build
stage="${CARGO_TARGET_DIR:-$repo/target}/appimage"
mkdir -p "$stage"
appdir="$stage/LocalImage.AppDir"
rm -rf -- "$appdir"
mkdir -p "$appdir/usr/bin"
# xtask emits exactly one package for this build; avoid reusing older dist files.
payload="${CARGO_TARGET_DIR:-$repo/target}/linux-package"
mapfile -t packages < <(find "$payload" -mindepth 1 -maxdepth 1 -type d)
[[ ${#packages[@]} == 1 ]]
cp -a "${packages[0]}/." "$appdir/usr/bin/"
cp packaging/linux/AppRun "$appdir/AppRun"
chmod +x "$appdir/AppRun"
cp packaging/linux/io.github.zdbosoxfan.LocalImage.desktop "$appdir/"
cp assets/app-icon/local-image-1024.png "$appdir/local-image.png"
output="${DIST:-$repo/dist/release}/$(basename -- "${packages[0]}").AppImage"
ARCH=x86_64 "$APPIMAGETOOL" --appimage-extract-and-run --runtime-file "$APPIMAGE_RUNTIME" --mksquashfs-opt -processors --mksquashfs-opt "${CARGO_BUILD_JOBS:-4}" "$appdir" "$output"
(cd -- "$(dirname -- "$output")" && sha256sum "$(basename -- "$output")" > "$(basename -- "$output").sha256")
# No display is opened. The version check exercises the launcher and runtime.
"$output" --appimage-extract-and-run --version >/dev/null
