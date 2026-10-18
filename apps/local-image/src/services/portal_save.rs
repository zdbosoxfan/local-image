//! The XDG desktop portal's FileChooser.SaveFile, which reports the file type picked (see
//! [`super::save_dialog`]). The dialog has no parent window: naming the window to the portal
//! needs a window handle the services don't have, and on Wayland an xdg-foreign handle, which
//! takes `unsafe` code the workspace forbids.

use std::collections::HashMap;

use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use super::save_dialog::{Filter, SaveRequest, Saved, file_uri_path};

/// Shows the portal's Save dialog and waits for it. `Ok(None)` is Cancel; `Err` means the portal
/// couldn't show it (no portal, no FileChooser), so the caller falls back to rfd.
pub(super) fn save_file(conn: &zbus::blocking::Connection, req: &SaveRequest) -> Result<Option<Saved>, String> {
    use zbus::blocking::Proxy;
    let unique = conn.unique_name().ok_or("the portal connection has no unique name")?.as_str().trim_start_matches(':').replace('.', "_");
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let token = format!("local_image_save_{}_{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let path = format!("/org/freedesktop/portal/desktop/request/{unique}/{token}");
    // Listen before asking, so a quick answer isn't missed.
    let request = Proxy::new(conn, "org.freedesktop.portal.Desktop", path.as_str(), "org.freedesktop.portal.Request").map_err(|e| e.to_string())?;
    let mut replies = request.receive_signal("Response").map_err(|e| e.to_string())?;
    let chooser = Proxy::new(conn, "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.FileChooser")
        .map_err(|e| e.to_string())?;
    let handle: OwnedObjectPath = chooser.call("SaveFile", &("", "Save As", options(req, &token))).map_err(|e| e.to_string())?;
    // A portal older than 0.9 ignores `handle_token` and answers on a request path of its own,
    // which it returns: listen there instead, as the Request documentation asks.
    let reply = if handle.as_str() == path {
        replies.next()
    } else {
        let request = Proxy::new(conn, "org.freedesktop.portal.Desktop", handle.as_str(), "org.freedesktop.portal.Request").map_err(|e| e.to_string())?;
        request.receive_signal("Response").map_err(|e| e.to_string())?.next()
    }
    .ok_or("the save dialog closed without an answer")?;
    let (code, results): (u32, HashMap<String, OwnedValue>) = reply.body().deserialize().map_err(|e| e.to_string())?;
    answer(code, &results)
}

/// The `SaveFile` options for `req`.
fn options<'a>(req: &'a SaveRequest, token: &'a str) -> HashMap<&'static str, Value<'a>> {
    let mut o: HashMap<&'static str, Value<'a>> = HashMap::from([("handle_token", Value::from(token)), ("modal", Value::from(true))]);
    if let Some(name) = &req.name {
        o.insert("current_name", Value::from(name.as_str()));
    }
    if let Some(folder) = &req.folder {
        // A byte string with its terminating NUL, as the portal specifies.
        use std::os::unix::ffi::OsStrExt;
        let mut bytes = folder.as_os_str().as_bytes().to_vec();
        bytes.push(0);
        o.insert("current_folder", Value::from(bytes));
    }
    let wire = |(label, exts): &Filter| (label.clone(), exts.iter().map(|e| (0u32, format!("*.{e}"))).collect::<Vec<_>>());
    if !req.filters.is_empty() {
        o.insert("filters", Value::from(req.filters.iter().map(wire).collect::<Vec<_>>()));
    }
    if let Some(f) = req.current.and_then(|i| req.filters.get(i)) {
        o.insert("current_filter", Value::from(wire(f)));
    }
    o
}

/// Reads the portal's `Response`: code 0 with the first `uris` entry. 1 is Cancel, and 2 a dialog
/// closed some other way, such as with Esc in GNOME's: no file either, and not a reason to show
/// rfd's dialog instead.
fn answer(code: u32, results: &HashMap<String, OwnedValue>) -> Result<Option<Saved>, String> {
    match code {
        0 => {}
        1 | 2 => return Ok(None),
        _ => return Err(format!("the save dialog failed (portal response {code})")),
    }
    let uris: Vec<String> = results
        .get("uris")
        .ok_or("no file in the portal's answer")?
        .try_clone()
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|e: zbus::zvariant::Error| e.to_string())?;
    let uri = uris.first().ok_or("no file in the portal's answer")?;
    let path = file_uri_path(uri).ok_or_else(|| format!("not a local file: {uri}"))?;
    let filter = results.get("current_filter").and_then(|v| {
        let (label, _globs): (String, Vec<(u32, String)>) = v.try_clone().ok()?.try_into().ok()?;
        Some(label)
    });
    Ok(Some(Saved { path, filter }))
}
