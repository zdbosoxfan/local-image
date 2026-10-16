#!/bin/bash
# Secrets are consumed only by this manually dispatched release job.
set -euo pipefail
[[ $(uname -m) == arm64 ]]
keychain="$RUNNER_TEMP/local-image-signing.keychain-db"
cleanup() {
  rm -f -- "$RUNNER_TEMP/local-image-signing.p12"
  if [[ -f "$keychain" ]]; then security delete-keychain "$keychain"; fi
}
trap cleanup EXIT
signed=false
if [[ -n ${MACOS_CERTIFICATE:-} && -n ${MACOS_CERTIFICATE_PASSWORD:-} && -n ${MACOS_KEYCHAIN_PASSWORD:-} && -n ${MACOS_SIGN_IDENTITY:-} && -n ${MACOS_INSTALLER_SIGN_IDENTITY:-} ]]; then
  printf '%s' "$MACOS_CERTIFICATE" | base64 --decode > "$RUNNER_TEMP/local-image-signing.p12"
  security create-keychain -p "$MACOS_KEYCHAIN_PASSWORD" "$keychain"
  security set-keychain-settings -lut 21600 "$keychain"
  security unlock-keychain -p "$MACOS_KEYCHAIN_PASSWORD" "$keychain"
  security import "$RUNNER_TEMP/local-image-signing.p12" -k "$keychain" -P "$MACOS_CERTIFICATE_PASSWORD" -T /usr/bin/codesign -T /usr/bin/productbuild
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$MACOS_KEYCHAIN_PASSWORD" "$keychain"
  security list-keychains -d user -s "$keychain" "$HOME/Library/Keychains/login.keychain-db"
  signed=true
else
  echo 'Developer ID secrets absent or incomplete: producing an ad-hoc test app; signing/notarization skipped.'
  unset MACOS_SIGN_IDENTITY MACOS_INSTALLER_SIGN_IDENTITY
fi
cargo +1.98.1 xtask package macos
app="${CARGO_TARGET_DIR:-target}/macos-package/Local Image.app"
notarize=false
if [[ $signed == true && -n ${MACOS_NOTARY_APPLE_ID:-} && -n ${MACOS_NOTARY_TEAM_ID:-} && -n ${MACOS_NOTARY_PASSWORD:-} ]]; then
  notarize=true
  ditto -c -k --keepParent "$app" "$RUNNER_TEMP/local-image-notary.zip"
  xcrun notarytool submit "$RUNNER_TEMP/local-image-notary.zip" --apple-id "$MACOS_NOTARY_APPLE_ID" --team-id "$MACOS_NOTARY_TEAM_ID" --password "$MACOS_NOTARY_PASSWORD" --wait
  xcrun stapler staple "$app"
  xcrun stapler validate "$app"
  spctl --assess --type execute --verbose=2 "$app"
else
  echo 'Notarization credentials absent or signing skipped: notarization skipped.'
fi
stage="${CARGO_TARGET_DIR:-target}/macos-dmg"
rm -rf -- "$stage"
mkdir -p "$stage"
ditto "$app" "$stage/Local Image.app"
ln -s /Applications "$stage/Applications"
# Derive the output name from the installer built by xtask.
checksum() {
  (cd -- "$(dirname -- "$1")" && shasum -a 256 "$(basename -- "$1")" > "$(basename -- "$1").sha256")
}
packages=(dist/release/*macos-arm64.pkg)
[[ ${#packages[@]} == 1 && -f ${packages[0]} ]] || { echo 'Expected exactly one fresh installer in dist/release' >&2; exit 1; }
for pkg in "${packages[@]}"; do
  dmg="${pkg%.pkg}.dmg"
  hdiutil create -volname 'Local Image' -srcfolder "$stage" -ov -format UDZO "$dmg"
  if [[ $signed == true ]]; then codesign --force --sign "$MACOS_SIGN_IDENTITY" --timestamp "$dmg"; fi
  if [[ $notarize == true ]]; then
    for artifact in "$pkg" "$dmg"; do
      xcrun notarytool submit "$artifact" --apple-id "$MACOS_NOTARY_APPLE_ID" --team-id "$MACOS_NOTARY_TEAM_ID" --password "$MACOS_NOTARY_PASSWORD" --wait
      xcrun stapler staple "$artifact"
      xcrun stapler validate "$artifact"
    done
    spctl --assess --type install --verbose=2 "$pkg"
  fi
  checksum "$pkg"
  checksum "$dmg"
done
# Preserve the signed/stapled bundle and symlinks for download as an artifact.
ditto -c -k --keepParent "$app" dist/release/local-image-macos-arm64.app.zip
checksum dist/release/local-image-macos-arm64.app.zip
