# Milestone 1 captures

Captured on 2026-09-30 by installed Edge 154. These are actual browser screenshots, not design mockups or historical README screenshots. The capture/test sources are checked in. [evidence.json](evidence.json) records screenshot hashes, tested source inventories and hashes of the complete local reports. The milestone's commands and open acceptance gates are in [FRONTEND-MILESTONE-1.md](../FRONTEND-MILESTONE-1.md).

| Capture | What it proves |
| --- | --- |
| [Before fixture, 1366×768](before-fixture-1366.png) | Pinned `e42aab6` composed legacy page, deterministic HTTP/preview fixture |
| [React fixture, 1366×768](react-fixture-1366.png) | Same synthetic 4096×2732 scene and controlled transport, new command strip/Layers layout |
| [Large/200%](react-large-text-200.png) | Actual browser rendering of the app's Large preference, controlled fixture transport; not Windows OS accessibility certification |
| [Real backend, 800×560](real-backend-800.png) | Actual Python CPU editing, downloaded/reopened project, actual layer state; compact window |
| [Real backend, 1366×768](real-backend-1366.png) | Same real workflow; transformed alpha mask with a refined transparent hole and two repair patches |
| [Real backend, 1920×1080](real-backend-1920.png) | Same reopened document at a large desktop viewport |

The simple colored test image intentionally makes pixel/alpha and repair assertions deterministic. It is separate from the synthetic still-life layout fixture. Neither uses personal photos or model inference. The real-backend captures do not replace API responses with mocks. They show Edge, not the WinForms window. The separate current-source WebView2 probe establishes navigation/title only; native pickers, native-save completion, full native interaction and Windows DPI still require acceptance.

The detailed ignored artifacts remain under `qa-artifacts/migration/` and `qa-artifacts/integration/`, including request/response evidence, persisted sessions, `.lremove`/PNG/TIFF files, build logs, exact command outputs and precision-test hashes.
