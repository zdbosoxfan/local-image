use crate::{LightcraftApp, state::ViewMode};
use serde_json::{Value, json};

pub(super) fn run(app: &mut LightcraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    Some(match id {
        "view.reference" => {
            // the reference: the one set before, else the active photo (the next one becomes active)
            let vis = app.session.visible_cloned();
            let live = |id: u64| crate::panels::compare::live(app, lightcraft_catalog::PhotoId(id));
            let active = app.session.active().map(|a| a.0);
            let reference = app.ui.reference.filter(|r| live(*r)).or_else(|| active.filter(|a| live(*a)));
            let Some(r) = reference else {
                return Some(Err(match active {
                    Some(id) => li_i18n::fmt(
                        li_i18n::library::t("Photo {id} is in Recently Deleted: restore it to use it as the reference"),
                        &[("id", &id.to_string())],
                    ),
                    None => li_i18n::library::t("Select a photo to use as the reference").into(),
                }));
            };
            app.ui.reference = Some(r);
            if app.session.active().is_none_or(|a| a.0 == r)
                && let Some(i) = vis.iter().position(|x| x.0 == r)
                && let Some(next) = vis.get(i + 1).or(i.checked_sub(1).and_then(|j| vis.get(j)))
            {
                let _ = app.session.execute("library.select", &json!({"ids": [next.0]}));
            }
            app.ui.view = ViewMode::Reference;
            Ok(json!({"reference": r, "active": app.session.active().map(|a| a.0)}))
        }
        "photo.setReference" => {
            let id = match p.get("id").filter(|value| !value.is_null()) {
                Some(value) => match value.as_u64() {
                    Some(id) => Some(id),
                    None => return Some(Err(li_i18n::library::t("photo.setReference: `id` must be a photo id (whole number)").into())),
                },
                None => app.session.active().map(|photo| photo.0),
            };
            let Some(id) = id else { return Some(Err(li_i18n::library::t("Select a photo to use as the reference").into())) };
            match app.session.catalog.photo(lightcraft_catalog::PhotoId(id)) {
                None => return Some(Err(li_i18n::fmt(li_i18n::library::t("No photo {id}"), &[("id", &id.to_string())]))),
                Some(photo) if photo.deleted => {
                    return Some(Err(li_i18n::fmt(
                        li_i18n::library::t("Photo {id} is in Recently Deleted: restore it to use it as the reference"),
                        &[("id", &id.to_string())],
                    )));
                }
                Some(_) => {}
            }
            app.ui.reference = Some(id);
            Ok(json!({"reference": id}))
        }
        _ => return None,
    })
}
