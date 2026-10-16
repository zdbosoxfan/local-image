# J25 pending translations

The Library's task panel (`lc-ui-egui/src/tasks.rs`, shared `li_ui_kit::progress` widget) passes these
labels through `tr`; add them to the Library catalog in the final i18n consolidation if missing.

- `Background Tasks` (panel window id/title; not drawn while the title bar is hidden)
- `Stopping…`
- `Smart Sort`
- `People`
- `{label} cancelled` (toast when a cancelled task's result is discarded; currently built with
  `format!` from the translated task label, as the old "Assisted Culling cancelled" toast was)
