#!/usr/bin/env bash
# Build Local Image for Linux and package it as a tarball with an installer script:
#
#   packaging/linux/package.sh            # x86_64 (or the host architecture)
#   SKIP_BUILD=1 packaging/linux/package.sh
#
# Produces dist/release/local-image-<version>-linux-<arch>.tar.gz holding the app and CLI, the
# desktop entry, icons, README, LICENSE and licenses/, and install.sh (per-user install into
# ~/.local; run it with --system for /usr/local).
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
version="${LOCAL_IMAGE_VERSION:-$(sed -n '/^\[workspace.package\]/,/^\[/{s/^version *= *"\(.*\)"/\1/p}' Cargo.toml | head -1)}"
arch="$(uname -m)"
dist="${DIST:-$root/dist/release}"
target_dir="${CARGO_TARGET_DIR:-$root/target}"
mkdir -p "$dist"
echo "Local Image $version for Linux $arch"
if [ -z "${SKIP_BUILD:-}" ]; then
  PHOTOCRAFT_BUILD_SHA="$(git rev-parse HEAD 2>/dev/null || true)" PHOTOCRAFT_BUILD_DATE="$(date -u +%F)" \
    cargo build --release --locked -p local-image -p local-image-cli
fi
name="local-image-$version-linux-$arch"
stage="$target_dir/linux-package/$name"
rm -rf "$stage"
mkdir -p "$stage/bin" "$stage/share/applications" "$stage/share/icons"
cp "$target_dir/release/local-image" "$target_dir/release/local-image-cli" "$stage/bin/"
cp packaging/linux/io.github.zdbosoxfan.LocalImage.desktop "$stage/share/applications/"
cp -r assets/app-icon/hicolor "$stage/share/icons/"
cp README.md LICENSE "$stage/"
cp -r licenses "$stage/licenses"
cp packaging/linux/install.sh "$stage/install.sh"
chmod +x "$stage/install.sh" "$stage/bin/"*
"$stage/bin/local-image-cli" --version
tar -C "$target_dir/linux-package" -czf "$dist/$name.tar.gz" "$name"
(cd "$dist" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
ls -l "$dist/$name.tar.gz"
