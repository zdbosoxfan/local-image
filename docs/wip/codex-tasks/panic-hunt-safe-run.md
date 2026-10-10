# Codex task: make the command fuzz (`panic_hunt`) safe to run, then run it and fix what it finds

From the opus-scan pass (branch `wip/opus-scan`). All Rust, no new dependencies, no `unsafe`, no GUI
windows, `cargo +1.98.1 … --offline`.

## Problem

`crates/pc-engine/tests/it/panic_hunt.rs` fuzzes every registered command with adversarial params
(AGENTS.md rule 9: a command returns `Err`, never panics or hangs). It is `#[ignore]`d and was not
run in the scan because some commands have side effects outside the process: e.g. Edit in external
editor spawns a program (`pc-engine/src/layer_menu_cmds.rs` ~line 371, `Command::new(program).spawn()`),
printing runs `lp` (`pc-engine/src/print_cmds.rs` ~line 373), file save/export commands write files.
On the owner's desktop that could open windows, print, or write into real folders.

## What to do

1. Give the fuzz a deny-list (or better, a `CommandSpec` flag such as `side_effects: true`) for
   commands that spawn processes, print, open dialogs, or touch the network, and point every
   file-writing command at a temp dir (set the session's working directory / pass `path` inside
   `std::env::temp_dir()`), so the run is hermetic.
2. Run it: `cargo test -p photocraft-engine --test it panic_hunt -- --ignored --nocapture`.
3. For each command that panics or hangs: reproduce with a focused unit test next to the command,
   fix it to return `EngineError::BadParams`/`Other`, and keep the test.
4. Consider the same for the Library engine (`lightcraft_engine::Session::execute` command registry)
   if it has no equivalent.

## Tests

The fuzz itself passes (still opt-in), plus one regression test per fixed command.
