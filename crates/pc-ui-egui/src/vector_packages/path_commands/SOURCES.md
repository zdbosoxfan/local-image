# Package 36 UI source record

Original GPL-3.0-or-later Local Image UI and egui_kittest replay fixtures. Behavior references,
exact fixture parameters, licenses and tool versions are recorded in
`crates/pc-engine/src/vector_packages/path_commands/SOURCES.md`. Existing package 19's
context-data bridge, captured UI transactions and offscreen GPU harness informed integration;
no second app, document tree or renderer is introduced. The full-app tests use PhotocraftApp
and require production menu/frame/display hooks; an isolated widget is not called app acceptance.
