#!/usr/bin/env bash
# Install Local Image from this folder: per user into ~/.local (default), or with --system into
# /usr/local (run with sudo). --uninstall removes what it installed. Settings and the library in
# ~/.config/local-image and ~/.local/share/local-image are never touched.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
prefix="$HOME/.local"
mode=install
for a in "$@"; do
  case "$a" in
    --system) prefix=/usr/local ;;
    --uninstall) mode=uninstall ;;
    *) echo "usage: install.sh [--system] [--uninstall]" >&2; exit 2 ;;
  esac
done
desktop=io.github.zdbosoxfan.LocalImage
if [ "$mode" = uninstall ]; then
  rm -f "$prefix/bin/local-image" "$prefix/bin/local-image-cli" "$prefix/share/applications/$desktop.desktop"
  find "$prefix/share/icons/hicolor" -name "$desktop.png" -delete 2>/dev/null || true
  echo "Removed Local Image from $prefix"
  exit 0
fi
install -Dm755 "$here/bin/local-image" "$prefix/bin/local-image"
install -Dm755 "$here/bin/local-image-cli" "$prefix/bin/local-image-cli"
install -Dm644 "$here/share/applications/$desktop.desktop" "$prefix/share/applications/$desktop.desktop"
(cd "$here/share/icons" && find hicolor -name "$desktop.png" -exec install -Dm644 {} "$prefix/share/icons/{}" \;)
command -v update-desktop-database >/dev/null && update-desktop-database -q "$prefix/share/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor" || true
echo "Installed Local Image into $prefix (run: local-image)"
case ":$PATH:" in *":$prefix/bin:"*) ;; *) echo "Note: $prefix/bin is not on your PATH" ;; esac
