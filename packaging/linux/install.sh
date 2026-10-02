#!/usr/bin/env bash
# Install the downloaded, self-contained Linux bundle for this user.
set -euo pipefail
umask 077

fail() { printf 'Local Image: %s\n' "$*" >&2; exit 1; }
case "${1:-}" in
  -h|--help)
    printf 'Install Local Image for the current user: ./install.sh\nNo administrator access, Python, or Node.js is needed.\n'
    exit 0 ;;
  '') ;;
  *) fail 'Use ./install.sh without arguments.' ;;
esac

[[ -n "${HOME:-}" && "$HOME" = /* ]] || fail 'HOME must be an absolute path.'
case "${XDG_DATA_HOME:-}" in
  /*) data_home="$XDG_DATA_HOME" ;;
  *) data_home="$HOME/.local/share" ;;
esac
app_root="$data_home/local-image-app"
bin_dir="$HOME/.local/bin"
desktop_dir="$data_home/applications"
launcher="$bin_dir/local-image"
desktop="$desktop_dir/local-image.desktop"
package_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
marker='local-image-linux-bundle-v1'

# A desktop entry is a line-based format. Spaces and Unicode are supported;
# control characters and '=' cannot represent an executable path safely.
for path in "$app_root" "$launcher"; do
  [[ "$path" != *$'\n'* && "$path" != *$'\r'* && "$path" != *$'\t'* ]] || fail 'Installation paths cannot contain control characters.'
done
[[ "$launcher" != *=* ]] || fail 'The home directory cannot contain = in a desktop executable path.'
[[ -x "$package_dir/local-image" && -x "$package_dir/backend/LocalImageBackend" ]] || fail 'Run install.sh from the extracted Linux download, alongside local-image and backend/LocalImageBackend.'
[[ -f "$package_dir/icon.png" && -f "$package_dir/VERSION" && -f "$package_dir/local-image.desktop" && -f "$package_dir/uninstall.sh" ]] || fail 'The download is incomplete; extract it again.'
version="$(cat -- "$package_dir/VERSION")"
[[ "$version" =~ ^[[:alnum:]][[:alnum:].+_~-]*$ ]] || fail 'The download has an invalid version identifier.'
[[ ! -L "$app_root" ]] || fail "Refusing an application directory that is a symbolic link: $app_root"
if [[ -e "$app_root" ]]; then
  [[ -d "$app_root" && ! -L "$app_root/INSTALLATION" && -f "$app_root/INSTALLATION" ]] || fail "An unmanaged folder already exists at $app_root; no files were changed."
  [[ "$(cat -- "$app_root/INSTALLATION")" = "$marker" ]] || fail "An unmanaged folder already exists at $app_root; no files were changed."
fi
[[ ! -L "$app_root/releases" ]] || fail 'Refusing a releases directory that is a symbolic link.'
[[ ! -e "$app_root/current" || -L "$app_root/current" ]] || fail 'Refusing an unmanaged current installation.'
if [[ -e "$launcher" || -L "$launcher" ]]; then
  [[ ! -L "$launcher" && -f "$launcher" ]] || fail "An unmanaged launcher already exists at $launcher; no files were changed."
  grep -Fxq -- "# Local Image application root: $app_root" "$launcher" || fail "An unmanaged launcher already exists at $launcher; no files were changed."
fi
if [[ -e "$desktop" || -L "$desktop" ]]; then
  [[ ! -L "$desktop" && -f "$desktop" ]] || fail "An unmanaged desktop entry already exists at $desktop; no files were changed."
  grep -Fxq -- 'X-LocalImage-Managed=true' "$desktop" || fail "An unmanaged desktop entry already exists at $desktop; no files were changed."
fi

mkdir -p -- "$app_root" "$app_root/releases" "$bin_dir" "$desktop_dir"
mkdir -- "$app_root/.install-lock" 2>/dev/null || fail 'An installation or uninstall is already running. If it was interrupted, remove the empty .install-lock directory in the application folder and retry.'
stage='' launcher_tmp='' desktop_tmp='' current_tmp=''
cleanup() {
  [[ -z "$stage" ]] || rm -rf -- "$stage"
  [[ -z "$launcher_tmp" ]] || rm -f -- "$launcher_tmp"
  [[ -z "$desktop_tmp" ]] || rm -f -- "$desktop_tmp"
  [[ -z "$current_tmp" ]] || rm -f -- "$current_tmp"
  rmdir -- "$app_root/.install-lock" 2>/dev/null || true
}
trap cleanup EXIT
printf '%s\n' "$marker" > "$app_root/INSTALLATION"
stage="$(mktemp -d -- "$app_root/releases/.install-XXXXXXXX")"
cp -a -- "$package_dir/." "$stage/"
release_name="$version-${stage##*.install-}"

# Shell quoting is separate from desktop-entry quoting: neither file is eval'd.
shell_quote() { printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"; }
desktop_exec_quote() {
  local value="$1"
  value="${value//\\/\\\\\\\\}"
  value="${value//\"/\\\\\"}"
  value="${value//\$/\\\\\$}"
  value="${value//\`/\\\\\`}"
  value="${value//%/%%}"
  printf '"%s"' "$value"
}
desktop_string() { printf '%s' "${1//\\/\\\\}"; }
launcher_tmp="$(mktemp -- "$bin_dir/.local-image-launcher-XXXXXXXX")"
{
  printf '#!/bin/sh\n# Local Image managed launcher\n# Local Image application root: %s\nexec ' "$app_root"
  shell_quote "$app_root/current/local-image"
  printf ' "$@"\n'
} > "$launcher_tmp"
chmod 755 -- "$launcher_tmp"

desktop_tmp="$(mktemp --suffix=.desktop -- "$desktop_dir/.local-image-desktop-XXXXXXXX")"
while IFS= read -r line || [[ -n "$line" ]]; do
  case "$line" in
    'Exec=@EXEC@ %F') printf 'Exec=%s %%F\n' "$(desktop_exec_quote "$launcher")" ;;
    'Icon=@ICON@') printf 'Icon=%s\n' "$(desktop_string "$app_root/current/icon.png")" ;;
    'X-LocalImage-InstallRoot=@INSTALL_ROOT@') printf 'X-LocalImage-InstallRoot=%s\n' "$(desktop_string "$app_root")" ;;
    *) printf '%s\n' "$line" ;;
  esac
done < "$package_dir/local-image.desktop" > "$desktop_tmp"
chmod 644 -- "$desktop_tmp"
if command -v desktop-file-validate >/dev/null 2>&1; then
  desktop-file-validate "$desktop_tmp" || fail 'The desktop entry could not be validated; the current version remains active.'
fi

mv -- "$stage" "$app_root/releases/$release_name"
stage=''
current_tmp="$app_root/.current-${release_name}"
ln -s -- "releases/$release_name" "$current_tmp"
mv -Tf -- "$current_tmp" "$app_root/current"
current_tmp=''
mv -Tf -- "$launcher_tmp" "$launcher"
launcher_tmp=''
mv -Tf -- "$desktop_tmp" "$desktop"
desktop_tmp=''
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$desktop_dir" >/dev/null 2>&1 || true
fi
printf 'Installed Local Image %s for this user.\nOpen Local Image from your applications menu.\nApplication: %s\nYour images, models, and settings remain in %s/local-image.\nTo uninstall: %s/current/uninstall.sh\n' "$version" "$app_root" "$data_home" "$app_root"
