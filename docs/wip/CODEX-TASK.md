# Codex task: fix the known failing tests on the working branch

These tests fail on the working branch (they appeared when the vector integration was merged with the ds2-fix batch);
the merge queue currently tolerates them via ~/.local/share/local-image-dev/land-known-failures.txt:
- photocraft-engine layer_multi_cmds::tests::merge_selected_into_top_most (undo restores two selected layers; test expects one)
- photocraft-ui-egui fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40 (icon padding blend/16)
- photocraft-ui-egui layer_reveal::tests::a_new_active_layer_opens_its_groups_and_scrolls_into_view (layer_reveal_tests.rs:111)
- photocraft-ui-egui vector_ui::tests::ds_finding_borrowed_shape_options_allow_corner_editing (vector_ui.rs:1215)
For each: decide whether the app behaviour or the test is wrong (check git history of both sides: ds2-fix c0652fae and the
vector integration 1974facf, and what Photoshop/Affinity do), fix the right side, and explain in the report. Do not
weaken a test to make it pass; if a test's expectation is wrong, say why and declare it with a line starting
"TESTS-REMOVED:" or "TEST-CHANGED:" in the report. Rules: RUSTC_WRAPPER=li-rustc-gate (set), Rust 1.98.1, --offline.
Run those four tests and the modules you changed once; workspace check; strict clippy on touched crates. Leave changes
uncommitted. Report: docs/wip/CODEX-REPORT-known-failures.md with exact commands and outputs.
