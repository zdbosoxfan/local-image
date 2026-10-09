//! Project links (Help menu, About dialog). Local Image's own pages are on GitHub; ArtCraft is
//! credited as the home of PhotoCraft and LightCraft.

/// The ArtCraft website.
pub const WEBSITE: &str = "https://getartcraft.com";
/// This app's page: the Local Image repository.
pub const APP_PAGE: &str = "https://github.com/zdbosoxfan/local-image";
/// This app's source repository.
pub const GITHUB: &str = "https://github.com/zdbosoxfan/local-image";
/// The user documentation (docs/ in the repository).
pub const HELP: &str = "https://github.com/zdbosoxfan/local-image/tree/v2/docs";
/// Where feedback and bug reports go.
pub const FEEDBACK: &str = "https://github.com/zdbosoxfan/local-image/issues/new";

/// (UI command id, menu label, URL) for each link, in Help-menu order.
pub const LINKS: &[(&str, &str, &str)] = &[
    ("app.help", "Local Image Help", HELP),
    ("app.website", "Local Image Website", APP_PAGE),
    ("app.github", "Local Image on GitHub", GITHUB),
    ("app.artcraft", "ArtCraft Website", WEBSITE),
    ("app.feedback", "Send Feedback…", FEEDBACK),
];

/// The URL behind a link command id.
pub fn url_of(cmd: &str) -> Option<&'static str> {
    LINKS.iter().find(|(id, _, _)| *id == cmd).map(|(_, _, u)| *u)
}

/// Open `url` in the user's browser (through the host's `open_url` service).
pub fn open(app: &mut crate::LightcraftApp, url: &str) -> Result<serde_json::Value, String> {
    let open = app.services.open_url.as_mut().ok_or("can't open links here")?;
    open(url)?;
    Ok(serde_json::json!({ "url": url }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_https_and_name_this_app() {
        for (id, label, url) in LINKS {
            assert!(url.starts_with("https://"), "{id}");
            assert!(!label.is_empty());
            assert!(!label.contains("LightCraft"), "{id}: {label}");
            assert_eq!(url_of(id), Some(*url));
        }
        assert_eq!(url_of("app.discord"), None, "no Discord link");
        for url in [APP_PAGE, GITHUB, HELP, FEEDBACK] {
            assert!(url.starts_with("https://github.com/zdbosoxfan/local-image"), "{url}");
        }
    }
}
