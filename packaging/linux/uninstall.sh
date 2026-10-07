#!/usr/bin/env bash
# Remove only this per-user application; leave all Local Image user data intact.
set -euo pipefail
fail() { printf 'Local Image: %s\n' "$*" >&2; exit 1; }
case "${1:-}" in
  -h|--help) printf 'Uninstall the per-user Local Image application: ./uninstall.sh\nYour images, models, settings, and recovery files are kept.\n'; exit 0 ;;
  '') ;;
  *) fail 'Use ./uninstall.sh without arguments.' ;;
esac
[[ -n "${HOME:-}" && "$HOME" = /* ]] || fail 'HOME must be an absolute path.'
case "${XDG_DATA_HOME:-}" in
  /*) data_home="$XDG_DATA_HOME" ;;
  *) data_home="$HOME/.local/share" ;;
esac
app_root="$data_home/local-image-app"
marker='local-image-linux-bundle-v1'
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
# An installed uninstaller keeps working even if XDG_DATA_HOME later changes.
if [[ "$script_dir" = */local-image-app/releases/* ]]; then
  app_root="${script_dir%/releases/*}"
  data_home="${app_root%/local-image-app}"
fi
[[ ! -L "$app_root" ]] || fail 'Refusing an application directory that is a symbolic link.'
if [[ ! -e "$app_root" ]]; then
  printf 'Local Image is not installed in %s. Your user data was kept.\n' "$app_root"
  exit 0
fi
[[ -d "$app_root" && ! -L "$app_root/INSTALLATION" && -f "$app_root/INSTALLATION" ]] || fail 'Refusing to remove an unmanaged application folder.'
[[ "$(cat -- "$app_root/INSTALLATION")" = "$marker" ]] || fail 'Refusing to remove an unmanaged application folder.'
mkdir -- "$app_root/.install-lock" 2>/dev/null || fail 'An installation or uninstall is already running.'
trap 'rmdir -- "$app_root/.install-lock" 2>/dev/null || true' EXIT
launcher="$HOME/.local/bin/local-image"
desktop_dir="$data_home/applications"
desktop="$desktop_dir/local-image.desktop"
if [[ ! -L "$launcher" && -f "$launcher" ]] && grep -Fxq -- "# Local Image application root: $app_root" "$launcher"; then
  rm -f -- "$launcher"
fi
escaped_root="${app_root//\\/\\\\}"
if [[ ! -L "$desktop" && -f "$desktop" ]] && grep -Fxq -- "X-LocalImage-InstallRoot=$escaped_root" "$desktop"; then
  rm -f -- "$desktop"
fi
rm -rf -- "$app_root"
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$desktop_dir" >/dev/null 2>&1 || true
fi
printf 'Local Image was uninstalled.\nYour images, models, settings, and recovery files remain in %s/local-image.\n' "$data_home"
