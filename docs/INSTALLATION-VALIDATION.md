# Local Image 0.6.0 installation validation

Validated on September 29, 2026. Version 0.6.0 separates installed application files from per-user state and optional AI storage. The release is a Windows EXE containing the native WebView2 host, Python backend, editor assets and dependencies. ComfyUI and model weights are separate downloads.

## What changed

Setup offers all-users installation in Program Files or current-user installation in AppData, an editable application destination, and independent model and portable ComfyUI folders. Its AI choices are existing-runtime discovery, a dedicated portable runtime, or setup later. Installer preferences are imported into each launching user's profile once. Existing settings and legacy Local Remove profiles are retained.

The installer validates folder syntax, Unicode names, protected destinations and application path depth before copying files. The build calculates the application-root allowance from the longest bundled paths; this package allows 155 characters. Legacy shortcut cleanup checks that a shortcut targets the installation being updated. Updates refuse to overwrite a busy backend, and uninstall removes application files while retaining profiles and AI storage.

ComfyUI discovery covers current Desktop installation records, legacy Desktop configuration, source installations and Windows portable layouts. Native folder pickers configure separate runtime and model destinations. Downloads check write access, free space, expected size and publisher SHA-256. A managed ComfyUI runs with its input, output, temporary and user files under the launching user's AppData. The initial AI health check has a three-second total timeout.

## Automated regression

| Check | Result | Scope |
| --- | --- | --- |
| Python regression | 308 passed, one expected skip; 309 discovered across 26 modules | Editing, project round trips, generation graphs, setup, download integrity, paths, stock, LoRAs and texture repair |
| Native storage tests | 17 checks passed | Fresh and legacy profiles, Unicode storage, one-time preferences, drive-root handling and folder rejection |
| Native host self-test | Passed | 12 origin/download, seven project, seven close-handshake and six setup bridge checks |
| Source interface suites | Eight passed | Setup, navigation, project/layer handling, browser editing, cutout, generation, refinement and stock |
| Packaged interface suites | Eight passed | Same suites against the deployed executable's backend, with copied-asset verification |
| Installer plans | 23 cases passed | Production wizard/CLI handling; setup choices, Unicode folders, protected/malformed paths and 155/156-character app destinations |

The startup tests use ephemeral local HTTP servers. A responsive server is detected; a server that accepts a request without answering stops delaying startup after the bounded check. They do not contact the user's ComfyUI.

Evidence is stored locally under ignored `qa-artifacts/v06/`: `python-regression.json`, `native-storage.json`, `source-ui/results.json`, `packaged-ui/results.json`, `installer-plan-latest/results.json` and the final release installer plan results. QA outputs are not bundled with the app. The reproducible tests are in `tests/`, including `installer_plan_smoke.py`, `smoke_deployment.py` and `smoke_windows_installer_qa.py`.

## Packaged deployment

The frozen package was copied into a simulated Program Files location and tested with a real Windows ACL denying writes and deletes to that copy. The harness confirmed that writes were denied, then restored the original ACL entries, inheritance protection and write access after the test.

Two Unicode-named user folders supplied fresh AppData profiles. First startup completed in 4.813 seconds on this PC. The native host imported the chosen model/runtime folders, and relaunch preserved edited settings. A second profile's launch failed promptly while the first profile owned the app port; its shutdown attempt left the first backend alive. Each profile subsequently started and shut down with its own configuration and credential.

The hidden native WebView2 probe loaded the Local Image editor successfully with browser data in the test profile. Real CPU testing opened a generated compressed 16-bit TIFF, ran both Quick Heal methods, saved an editable project, checked the original pixels and rejected unauthenticated configuration changes. Browser suites also exercised local composition, imports and exports. GPU and stock-provider responses in the interface suites were controlled fixtures.

Application-tree hashes remained unchanged. Read-only ComfyUI discovery found the existing installation, and the external ComfyUI queue was unchanged throughout. Evidence: `qa-artifacts/v06/deployment/results.json`, the native probe and CPU logs in that folder, and `packaged-ui/packaged-assets.json`.

## Actual installation and uninstallation

A normal Windows account installed the same frozen package with the production installer code, changing only the installer application and project-registration identifiers to unique QA values. This allowed a full per-user install/uninstall cycle without updating the registered production app.

The test verified the installed native version, bundled backend, Unicode folder preferences, project associations and uninstall registration. `/NOICONS` suppressed new shortcuts. The QA installation's own uninstaller removed its application files and registration identifiers. Markers in the selected model folder, portable runtime parent and application profile survived. Existing application registrations, complete project-association registry trees and shortcut hashes matched their original snapshots afterward.

Evidence: `qa-artifacts/v06/qa-installer/results.json`, `install.log` and `uninstall.log`. The release installer retains the normal `LocalRemove.Windows` and `LocalRemove.Project` identities for upgrades. QA identifier overrides are explicit compiler options and are not used in the release build.

## Package checks and limits

All 41 bundled Python modules match the current source after normalizing embedded source filenames. The HTML, workflow, JavaScript, stylesheet and icons match source. Ten 32-bit icon frames, from 16 through 256 pixels, match the executable resources. The package contains no model weights, personal photos, generated test images, account tokens, launcher credentials, settings, recovery state or browser profiles.

Validation used one physical Windows PC, simulated fresh AppData profiles, an actual protected installation copy and a real per-user QA installation. An elevated all-users install/uninstall cycle and a second physical PC were not tested. This release's checks did not repeat multi-gigabyte portable/model downloads or GPU inference; the setup tests cover download integrity and extraction, and the earlier [0.5.0 model-quality and GPU validation](LOCAL-IMAGE-VALIDATION.md) remains separate evidence. Code signing is not included in this preview build.
