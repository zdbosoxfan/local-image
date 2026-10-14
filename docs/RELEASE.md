# Release and test lanes

Local Image V2's supported release targets are Linux x86_64 (Vulkan) and Apple Silicon
macOS 13+ (Metal). Windows source and packaging code remain experimental: the owner does
not validate Windows releases. Intel macOS is not a packaged target. No CUDA dependency is
needed to edit photos; CPU fallbacks adapt to the machine. ComfyUI has separate requirements.

## Fast feedback

Use Rust 1.98.1, the pinned fonts from [DEVELOPMENT.md](DEVELOPMENT.md), and the existing target
cache. Do not override the launcher’s `CARGO_BUILD_JOBS=4` or delete the cache.

```sh
PATH="$HOME/.cargo/bin:$PATH" cargo +1.98.1 xtask test-fast
```

This portable smoke lane runs the complete library suites of both geometry and colour crates,
metadata, catalog/document data, document formats and xtask, then the new update service and UI
interaction tests in both UI crates and li-ai. Qualified module filters keep hardware tests
out of this portable lane; the offscreen GPU screenshot test is run separately. It fails on any failure,
uses `--offline --locked`, and adds no dependencies. Its target is under five minutes with an
incremental cache; a cold build of the app's UI/dependency graph is outside that target. The
printed duration includes incremental compilation. It does not replace affected-crate tests,
full nightly validation, lint or hardware acceptance. `.github/workflows/ci.yml` runs it on
Ubuntu 22.04 and the arm64 `macos-15` runner for pushes and PRs. CI fetches locked inputs first,
then builds/tests offline. Font inputs are pinned to an audited commit.

## Full and GPU nights

`.github/workflows/nightly.yml` runs an unfiltered `cargo test --workspace --no-fail-fast`,
strict lint, native compositor parity, Develop numerical conformance, and an asset audit after
screenshots exist. No failing test may be allow-listed. The installed landing list
`land-known-failures.txt` is empty as of 2026-10-13; it is external coordinator state and is not
edited by this job. Historical failure reports describe past failures, not exceptions.

Nightly machines must explicitly opt in through the `local-image-nightly` label in addition
to `vulkan` (Linux X64) or `metal` (macOS ARM64). Only trusted branches may dispatch there.
Install the same prerequisites, independent rendering oracles (Inkscape and Poppler), pinned
fonts and licensed fixtures described in [GPU-VALIDATION.md](GPU-VALIDATION.md) and
[MACOS.md](MACOS.md). A headless hardware render is sufficient; do not open app windows.
Reserve the hardware through the owner’s queue before acceptance, keep one Cargo build per
job, and avoid concurrent interactive ComfyUI work. These workflows are supplied as files;
no runner is installed or workflow dispatched by this task.

`LOCAL_IMAGE_REQUIRE_GPU=1` and `LC_REQUIRE_GPU=1` make missing adapters fail. Hardware runs
must report the actual native adapter; software Vulkan/llvmpipe and adapter skips do not count.
The two explicit nightly GPU commands are a minimum conformance gate. The existing ignored
model, real-photo, UI snapshot and performance acceptance tests remain milestone gates; use
the documented fixtures and per-test filters rather than blindly invoking every ignored test
(some require paid/accepted models or external oracles). No GPU or interactive-speed claim is
made by portable CI.

## Artifact builds

Manually dispatch `.github/workflows/release-artifacts.yml` on a reviewed branch. It only uploads
Actions artifacts with SHA-256 files; it never pushes a tag, creates a GitHub release, or uploads
an artifact to the releases API. Keep the existing macOS hardware workflow for broader Apple
integration acceptance.

Linux: `cargo +1.98.1 xtask package linux` builds the HEIF-enabled app and CLI with SAM off and
creates the existing tarball with all notices. `packaging/linux/appimage.sh` packages those
binaries into an AppDir and an x86_64 AppImage. It uses the AppImage project’s standard AppRun,
desktop entry and icon layout. The external packaging tool and AppImage container runtime are not Rust build dependencies.
The workflow verifies the published tool and type-2 runtime SHA-256 values before use; the `continuous` URL
moves, so tool refreshes must deliberately update that hash after review (a change fails closed).
The AppImage runtime still requires the host's Linux display/graphics drivers; Ubuntu 22.04
sets the glibc baseline. Check distribution compatibility on real supported hosts before a
public release. AppImage's launcher forwards files/arguments to the app; the CLI is in the
AppDir's `usr/bin`. The tarball remains available for direct CLI use.

macOS: `packaging/macos/release.sh` invokes the existing `cargo +1.98.1 xtask package macos`.
It packages the native app, a drag-to-Applications `.dmg`, the existing `.pkg`, and a bundle ZIP.
SAM stays off; models are separate downloads. Without credentials it creates an ad-hoc test
bundle and reports that signing/notarization was skipped. Such a build is not a notarized public
release and can trigger Gatekeeper. Secrets are read only by the manual release job:

| Repository secret | Purpose |
| --- | --- |
| `MACOS_CERTIFICATE` | Base64 PKCS#12 containing Developer ID Application and Installer certificates |
| `MACOS_CERTIFICATE_PASSWORD` | PKCS#12 password |
| `MACOS_KEYCHAIN_PASSWORD` | Password for the temporary runner keychain |
| `MACOS_SIGN_IDENTITY` | Developer ID Application identity |
| `MACOS_INSTALLER_SIGN_IDENTITY` | Developer ID Installer identity |
| `MACOS_NOTARY_APPLE_ID` | Apple ID for notarization |
| `MACOS_NOTARY_TEAM_ID` | Apple Developer team |
| `MACOS_NOTARY_PASSWORD` | App-specific password |

Complete signing credentials enable nested-code signing with hardened runtime and timestamps.
Complete notarization credentials additionally submit the bundle, installer and disk image via
`notarytool --wait`, staple/validate tickets and assess the app/installer with `spctl`. Failure
stops the job; it cannot fall back to unsigned output after a configured signing failure. Partial
or absent secrets skip the corresponding stage clearly. The temporary certificate/keychain is
removed on exit. Upload permission is read-only for repository contents, and no credentials are
included in artifacts. Do not enable shell command tracing in this script.

## Update checks and privacy

**Help → Check for Updates…** checks the public GitHub releases JSON through rustls and the
existing RustCrypto provider. A background worker has a 15-second request deadline and a 2 MiB
response cap. No telemetry, photos, machine identifiers, tokens or model data are sent. GitHub
necessarily sees the request's source IP and a generic application User-Agent. Requests do not
redirect to other services. Rate limits, invalid JSON and network failures are shown in the UI;
Retry is explicit. No binary is downloaded or installed.

**Preferences → General → Check for updates at startup** defaults off, including in old settings.
When enabled it checks once per launch, silently unless a newer release is found. Stable builds
only offer stable releases; alpha/beta builds also consider prereleases. Versions are compared
numerically with SemVer prerelease ordering, so a stable 0.7 release cannot downgrade V2. The
only offered destination is this project's canonical GitHub release page. Opening that page is
always the user's choice.

References followed: [GitHub Releases REST API](https://docs.github.com/en/rest/releases/releases#list-releases),
[SemVer precedence](https://semver.org/#spec-item-11),
[AppDir specification](https://docs.appimage.org/reference/appdir.html),
[AppImage manual packaging](https://docs.appimage.org/packaging-guide/manual.html),
[Apple notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow),
and [Apple Developer ID](https://developer.apple.com/developer-id/).
