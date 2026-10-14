# compat-c-shell pending translations

`apps/local-image/src/library_host.rs` (`open_library_app`) passes this Library notice through
`lightcraft_ui_egui::i18n::tr`; add it to the Library catalog in the final i18n consolidation.

- `Some Library settings used defaults: {fields}.` (replaces "… Original values were kept in
  Preferences for recovery.", which is no longer true: the damaged original is not kept in the section)

No longer used; drop in the consolidation (J29 owns `crates/lc-ui-egui/src/i18n.rs`, so it was not edited here).

Editor catalogs (`crates/pc-ui-egui/src/i18n/*.tsv`), from the removed "Remove old models" button:

- `Remove old models ({size})`
- `Moves the models the app no longer uses (U²-Net, TinySkyNet, MiDaS) to the Trash.`
- `the old models`

Library lists (`lc-ui-egui/src/i18n.rs` `PENDING_SAVE_TRANSLATIONS` and the final-review list; also
`pc-ui-egui/src/i18n/pending_vector.rs`), from the removed `library-ui.json` reader:

- `Library settings couldn't be read: {error}. Choose your existing Library folder. Your stored settings will not be replaced.`
- `Some Library settings used defaults: {fields}. A copy of the damaged settings was kept.`
- `couldn't preserve damaged Library settings: {error}`
- `Some Library settings used defaults: {fields}. Original values were kept in Preferences for recovery.`
