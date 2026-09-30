# Frontend dependency verification

Checked 2026-09-30 against the official npm registry, plus the primary documentation below. These are stable registry releases, not versions copied from Fluent UI's moving `master` branch. The local development runtime reports Node `v22.23.2`, which satisfies both Vite and its React plugin.

| Package | Verified stable version | License | Relevant compatibility |
| --- | --- | --- | --- |
| `react` | `19.3.0` | MIT | Node >=0.10.0 |
| `react-dom` | `19.3.0` | MIT | React ^19.3.0 |
| `@fluentui/react-components` | `9.74.9` | MIT | React and @types/react >=16.14.0 <20.0.0 |
| `@griffel/react` | `1.7.8` | MIT | React >=16.14.0 <20.0.0 |
| `vite` | `8.3.1` | MIT | Node ^20.19.0 or >=22.12.0 |
| `@vitejs/plugin-react` | `6.1.1` | MIT | Vite ^8.0.0; Node ^20.19.0 or >=22.12.0 |
| `typescript` | `7.0.2` | Apache-2.0 | Node >=16.20.0 |
| `@types/react` | `19.3.0` | MIT | No additional peers |
| `@types/react-dom` | `19.3.0` | MIT | @types/react ^19.3.0 |

Fluent's declared Griffel dependency is `^1.5.32`, compatible with the explicitly pinned `1.7.8`. The React plugin's `oxc-transform-react`, `@rolldown/plugin-babel`, and `babel-plugin-react-compiler` peers are marked optional. Vite's advertised preprocessing and tooling peers are also optional; the application does not need to add them simply to satisfy metadata.

The exact executed metadata command for each package in the table was:

```powershell
npm.cmd view <package> version license peerDependencies engines --json --fetch-retries=0 --fetch-timeout=15000
```

Additional executed verification:

```powershell
node --version
npm.cmd view @vitejs/plugin-react@6.1.1 peerDependenciesMeta --json --fetch-retries=0 --fetch-timeout=15000
npm.cmd view vite@8.3.1 peerDependenciesMeta --json --fetch-retries=0 --fetch-timeout=15000
npm.cmd view @fluentui/react-components@9.74.9 dependencies.@griffel/react --json --fetch-retries=0 --fetch-timeout=15000
```

The first sandboxed registry read failed with `EACCES`; read-only registry commands succeeded through the approved network escalation. `npm.cmd` is used because this machine's PowerShell execution policy blocks `npm.ps1`. These commands read metadata and do not install dependencies. Package installation, the checked-in lockfile, production build, browser tests, and bundled license notices provide separate evidence; metadata alone does not certify them.

## Primary references and integration decisions

- [React incremental adoption](https://react.dev/learn/add-react-to-an-existing-project) supports mounting React into selected regions of an existing page, with Vite as a build integration. The first milestone leaves the persistent canvas in its existing implementation.
- [React useSyncExternalStore](https://react.dev/reference/react/useSyncExternalStore) requires immutable snapshots, stable results between changes, and unsubscribe cleanup. Those requirements define the bridge's accepted snapshot contract.
- [Vite backend integration](https://vite.dev/guide/backend-integration) specifies a build manifest and backend-generated script, CSS, and optional module-preload tags. `backend/frontend_dist` is a deliberate package directory; only validated, manifest-listed hashed files are exposed by `/frontend-assets/`.
- [Vite supported Node versions](https://vite.dev/guide/) are also checked against the exact registry release metadata rather than assuming that a locally installed Node executable is new enough.
- [Fluent UI v9 usage](https://raw.githubusercontent.com/microsoft/fluentui/master/packages/react-components/react-components/README.md) demonstrates one themed FluentProvider. This reference informs API usage only; the pinned release comes from the registry.
- [Griffel createDOMRenderer](https://griffel.js.org/react/api/create-dom-renderer/) documents `styleElementAttributes.nonce` with RendererProvider. The renderer receives the nonce from the current page bootstrap; no nonce or browser credential belongs in a public bundle.
- [WebView2 security guidance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/security) supports checking message sources and navigation. The existing host's trusted origin and `/remove` path checks remain in place.

## Delivery constraints

The React mode is opt-in with `LOCAL_IMAGE_FRONTEND=react`. The default and rollback value are `legacy`. React mode wraps existing CSS in `@scope (:root) to ([data-react-owned])` so legacy selectors cannot target React-owned regions, mapping inner `:root` selectors to `:scope` to preserve root tokens and density selectors. A direct headless Edge 154 check confirmed `CSSScopeRule`, the retained system font, grid columns and body flex layout, and exclusion of a React-owned button from legacy minimum height. Production acceptance must still verify support in the actual WebView2 runtime; generated CSS has inherited properties, which the one Fluent theme and application CSS must set explicitly.

HTML is not cached. `/remove` issues a fresh CSP nonce and signed browser token on every response. The token is authenticated by a process-local secret, has no wall-clock expiry that would interrupt an owned native picker, and becomes invalid when the backend process restarts. Existing trusted in-process test and native-adjacent callers remain compatible with the process token; that raw secret is no longer rendered into HTML.

Production script, CSS, and font sources are restricted to the same origin's `/frontend-assets/` path plus the page's inline nonce where relevant. No `unsafe-inline`, `unsafe-eval`, remote UI code, or remote font source is introduced. The manifest, source maps, Python files, and state files are not publicly served. PyInstaller and the Windows build preflight require the built manifest and `THIRD_PARTY_NOTICES.txt`; end users do not need Node or Vite. Windows packaging and native acceptance still need their separate execution gates.

## Runtime license omissions

An inventory of all 92 resolved runtime packages found four packages without a top-level LICENSE or NOTICE file: `@fluentui/react-icons@2.0.343`, `embla-carousel@8.6.0`, `embla-carousel-autoplay@8.6.0`, and `embla-carousel-fade@8.6.0`. Their exact npm metadata was queried for `gitHead`, repository, declared license and tarball URL. The corresponding MIT license texts are saved under `frontend/licenses/`, each with provenance pointing to the authoritative repository at the registry-reported commit. Fluent icons use commit `e17e4b6a88cb4f5daf6331c2a7bb7cc7217f9b11`; all three Embla packages use `0fe65834136f1aa35e4c1a4a477e5ccb4bb5ee54`. Fallback use must stay conditional on these exact package versions; an upgrade requires re-verification.
