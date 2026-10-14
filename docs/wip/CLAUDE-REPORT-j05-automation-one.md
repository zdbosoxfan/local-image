# J5 — one automation layer (D29) — Claude builder report

Status: done. One commit on `wip/cl-j05-automation-one`.

## What merged

Before: two MCP servers. `pc-automation` (rmcp SDK; headless editor session or bridge to the app;
token auth, connection limits, capability-scoped workspace, response budgets) and `lc-mcp`
(1,615 lines incl. tests: hand-written JSON-RPC over stdio, its own base64, an unauthenticated TCP
client). `lc-mcp` had no binary and no dependents, so the Library's tools were unreachable; the CLI
had no Library commands.

After: **one server, one security model, one CLI.**

- `crates/pc-automation/src/library/` (new): `LibraryHeadless` (the Library `Session` behind the
  tools; moved from `lc-mcp/src/headless.rs`) and `library/tools.rs` (the helper tools, moved from
  `lc-mcp/src/tools.rs` with their logic and JSON schemas kept).
- `PhotocraftMcp` serves them as **`library_*` tools** next to the editor tools (dynamic rmcp routes on
  the same `ToolRouter`): `library_command_list`, `library_command_run`, `library_import`,
  `library_query_photos`, `library_select_photos`, `library_list_controls`, `library_get_develop`,
  `library_set_develop`, `library_apply_preset`, `library_crop`, `library_render_photo`,
  `library_export`. The Library session is created on the first Library call (editor-only clients pay
  nothing). Server instructions describe the Library loop.
- **Security (shared with the editor):**
  - No automation roots → the Library reads and writes no files.
  - With `--automation-read-root` / `--automation-write-root`: request paths are forward-slash
    relative paths; photo bytes are read only through the read-root capability (all five Library file
    hooks — loader, raw loader, probe, embedded preview, byte reader — are installed on the capability);
    exports, renders and saved images are written only through the write-root capability (atomic,
    fsynced `AuthorizedWorkspace::write`). Symlinks out of the root are not followed. Results name files
    relative to the roots (host root paths are not shown to the agent).
  - Library commands are **deny-by-default** (`library_command_allowed`): only commands that change the
    in-memory catalog/develop state or report state run; `library.import`, `app.export` and
    `export.checkTarget` are mapped onto the capabilities. Denied examples: `library.browse`,
    `library.autoImport`, XMP sidecar writes, preset/profile file import/export, DNG convert, duplicate,
    rename, folder moves, merges, smart-sort export, model download, save-over-original.
  - "Never write over an original" (#93) holds for exports, renders and saved images in every mode.
  - Response budgets: images ≤ 5 MB, render long edge ≤ 2048 px, JSON replies bounded (as editor tools).
- **Deleted:** `crates/lc-mcp` entirely (JSON-RPC server, base64 helper, unauthenticated `Remote`
  client, UI tools that only worked through it). No forwards kept (owner: nothing old is supported).
- **CLI (`local-image-cli library …`, trusted local paths):** `import` (into a `--library` folder;
  add/copy/move with destination, organize and rename templates), `develop` (`--set control=value`,
  `--settings json`, `--preset`/`--amount`), `export` (`--out` | `--dir`, `--format`, `--quality`,
  `--long-edge`, `--naming`, develop flags), `snapshot` (render one photo to a file), `run` (`--cmd`/
  `--params`), `commands`. Photos are files/folders (imported first) or `--id <n>` in `--library`.
  `local-image-cli mcp` (headless) now also serves the Library tools.

## User-visible changes (documented)

- Library MCP tools are named `library_*` and use the editor server's model: `library_command_list` +
  `library_command_run` reach every command; the old one-tool-per-command `cmd_*` set (≈295 tools) and
  the five `lightcraft://` resources are not served (same data via `library_command_run`, e.g.
  `library.state`, `catalog.query`, `photo.inspect`, `develop.get`, `develop.controls`).
- `library_render_photo` max size 2048 px (was 4096) — the shared preview budget.
- Automation imports add files in place only (copy/move change originals on disk: CLI only).
- Library tools in `--bridge` mode return a clear error: the desktop app's control channel does not
  expose its Library (see "Left").
- `library` snapshot = photo render snapshot, not a UI screenshot (a UI snapshot would pull the
  whole egui Library stack into the CLI).

## Quality / speed (before → after)

| | before (`lc-mcp`) | after (`pc-automation`) |
|---|---|---|
| MCP servers / JSON-RPC implementations | 2 / 2 | 1 / 1 (rmcp) |
| Library tools reachable by an agent | 0 (no binary) | 12 |
| Library CLI subcommands | 0 | 6 |
| Library file access policy | ambient host paths, no auth | capability-scoped, deny-by-default commands |
| `render_photo` PNG 512 px, median of 7 (demo photo, warm) | 50.2 ms | 39.0 ms |
| `render_photo` PNG 1024 px | 159.8 ms | 160.4 ms (PNG encode dominates; same) |
| `render_photo` JPEG 512 px | 44.0 ms | 3.8 ms |
| `render_photo` JPEG 1024 px | 174.4 ms | 22.7 ms |

The old path wrote a temporary PNG, read it back and, for JPEG, decoded and re-encoded it; the new
path encodes the render once in memory (same pixels: the removed intermediate was lossless PNG; same
encoder calls). Measured with temporary tests (one run each, dev profile = optimized + debuginfo,
shared machine), then removed. No new external crates (Cargo.lock: `lightcraft-mcp` removed,
`pc-automation` gains workspace deps `lightcraft-engine/-codecs/-raster`). The CLI binary now links
the Library engine (needed for the Library subcommands).

## Tests (minimum, filtered by name)

- `cargo test -p photocraft-automation --lib j05_`: **18 passed** — the `lc-mcp/src/tests.rs`,
  `headless.rs` and `tests/e2e.rs` tests re-targeted at the `library_*` tools (definitions, errors,
  command run + undo, set_develop, crop, exposure render e2e, import copy/move templates, original
  guard #93, export selection + naming tokens, extension registry) plus new policy tests (workspace
  roundtrip through capabilities with no host paths in results, original guard + path escapes +
  copy/move refusal, symlink escape on unix, deny-by-default commands, no-roots-no-files) and an rmcp
  duplex client test (`server::tests::j05_library_tools_over_mcp`: list + import + develop + render
  over the protocol, denied command, bridge message).
- `cargo test -p local-image-cli --lib j05_`: **1 passed** (import into a library folder, develop by
  id persists across invocations, export by id and by file, snapshot, original guard, usage errors).
- `cargo check`, `cargo fmt`, strict `cargo clippy --all-targets -D warnings` on
  `photocraft-automation` and `local-image-cli`: clean.
- Existing `pc-automation` tests not run (owner rule); they are unchanged and the change to them is
  additive (`lists_expected_tools` checks a subset of names and name/description shape, which the new
  tools satisfy).

TEST-CHANGED: none (the `lc-mcp` tests moved with their assertions to `pc-automation`; the lc-mcp
`tools_list_has_helpers_and_every_command` check of one `cmd_*` tool per command became "every command
is listed by `library_command_list`", since the per-command tools are not served).

## Outside edits

- `crates/pc-automation/src/workspace.rs` (same crate, not in the owned list): root paths kept on the
  capabilities (`read_root_path`/`write_root_path`), `list_files` (capability folder scan, ≤100k
  files, no symlinks), `write_target_exists`, `relative_path` made `pub(crate)`.
- `crates/pc-automation/src/lib.rs`, `crates/pc-automation/Cargo.toml` (module + deps).
- Root `Cargo.toml` (removed the `lightcraft-mcp` workspace entry), `Cargo.lock`.

## Left

- **Library over the live app (bridge):** `apps/local-image/src/control_server.rs` routes every control
  request to the editor (`photocraft_ui_egui::control`); the Library host never sees them. Exposing the
  in-app Library needs a host-side route (e.g. `library.execute`) in `apps/local-image` — outside J5's
  files. The tools are ready for it (`LibraryHeadless::call` keeps the control-method shape).
- `local-image-cli serve` (JSON-lines, `rpc.rs`) stays editor-only; the Library is on MCP and the CLI.
- XMP sidecars are not read on workspace imports' capability path except where the engine reads them
  by host path below the read root (metadata only; documented residual: a pre-existing symlinked
  `.xmp` inside the read root could be followed by that read).
