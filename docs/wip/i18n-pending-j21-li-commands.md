# J21 pending Library native-menu strings

Source: `crates/lc-ui-egui/src/menubar.rs`.

- `menu command is unavailable`
- `menu item is no longer available`

The native macOS system-row labels reuse the existing editor source strings. Library locale
catalogs can fall back to these English strings until the shared i18n consolidation:
`About {app}`, `Settings…`, `Settings`, `Hide {app}`, `Quit {app}`, `Minimize`.
