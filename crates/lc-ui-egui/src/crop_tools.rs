use crate::LightcraftApp;
use serde_json::Value;

impl LightcraftApp {
    pub(crate) fn auto_straighten(&mut self, ctx: &egui::Context) {
        match self.run("crop.autoStraighten", serde_json::json!({})) {
            Ok(result) if result.get("changed").and_then(Value::as_bool) == Some(false) => {
                self.toast(ctx, li_i18n::library::t("No horizon or vertical lines to level by"));
            }
            Ok(_) => {}
            Err(error) => self.toast_error(ctx, error),
        }
    }
}
