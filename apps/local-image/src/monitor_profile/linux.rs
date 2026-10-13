//! Pure-Rust Linux detection. KWin config first, live KScreen if ambiguous, X atoms only on
//! an actual X11 window, then colord. I/O is exclusively on monitor_profile's worker.
use super::{Detection, Display, Output, run_helper};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc, time::Duration};

fn id(name: &str) -> u32 {
    // Stable across reordered monitor enumeration and repeated readings.
    name.bytes().fold(2166136261u32, |h, b| (h ^ u32::from(b)).wrapping_mul(16777619)).max(1)
}

fn profile(path: &str) -> Option<Arc<Vec<u8>>> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > 16 * 1024 * 1024 {
        return None;
    }
    icc_atom(&std::fs::read(path).ok()?, 8).map(Arc::new)
}

/// X ICC convention: CARDINAL 8 data, full ICC header, declared size within the atom.
pub(super) fn icc_atom(bytes: &[u8], format: u8) -> Option<Vec<u8>> {
    if format != 8 || bytes.len() < 132 || bytes.get(36..40) != Some(b"acsp") {
        return None;
    }
    let size = u32::from_be_bytes(bytes[..4].try_into().ok()?) as usize;
    if !(132..=16 * 1024 * 1024).contains(&size) || size > bytes.len() {
        return None;
    }
    Some(bytes[..size].to_vec())
}

/// Connector → ICC path. KWin retains disconnected displays; ambiguous reused connectors
/// deliberately fall through to live KScreen rather than using a stale calibration.
pub(super) fn kwin_profiles(value: &Value) -> HashMap<String, String> {
    let mut paths: HashMap<String, Vec<String>> = HashMap::new();
    let mut connectors: HashMap<String, usize> = HashMap::new();
    if let Some(groups) = value.as_array() {
        for group in groups.iter().filter(|g| g["name"] == "outputs") {
            for o in group["data"].as_array().into_iter().flatten() {
                if let Some(name) = o["connectorName"].as_str() {
                    *connectors.entry(name.into()).or_default() += 1;
                }
                if o["colorProfileSource"] != "ICC" {
                    continue;
                }
                if let (Some(name), Some(path)) = (o["connectorName"].as_str(), o["iccProfilePath"].as_str())
                    && !path.is_empty()
                {
                    paths.entry(name.into()).or_default().push(path.into());
                }
            }
        }
    }
    paths
        .into_iter()
        .filter_map(|(name, mut p)| {
            p.sort();
            p.dedup();
            (p.len() == 1 && connectors.get(&name) == Some(&1)).then(|| (name, p.remove(0)))
        })
        .collect()
}

pub(super) fn kscreen_outputs(value: &Value) -> Vec<(Output, String)> {
    value["outputs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["connected"] == true && o["enabled"] == true)
        .filter_map(|o| {
            let name = o["name"].as_str()?.to_string();
            let scale = o["scale"].as_f64().filter(|s| s.is_finite() && *s > 0.0).unwrap_or(1.0);
            let mut w = o["size"]["width"].as_f64()? / scale;
            let mut h = o["size"]["height"].as_f64()? / scale;
            if matches!(o["rotation"].as_u64(), Some(2 | 8)) {
                std::mem::swap(&mut w, &mut h);
            }
            Some((Output { name, frame: [o["pos"]["x"].as_f64()?, o["pos"]["y"].as_f64()?, w, h] }, o["iccProfilePath"].as_str().unwrap_or_default().into()))
        })
        .collect()
}

pub(super) fn detect(outputs: &[Output], x11: bool) -> Detection {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| std::path::PathBuf::from(p).join(".config")));
    let paths = config
        .and_then(|p| std::fs::read(p.join("kwinoutputconfig.json")).ok())
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .map(|v| kwin_profiles(&v))
        .unwrap_or_default();
    // KScreen also supplies topology when reading before a window exists (diagnostic tests).
    let live = if outputs.is_empty() || outputs.iter().any(|o| !paths.contains_key(&o.name)) {
        run_helper(std::process::Command::new("kscreen-doctor").arg("-j"), Duration::from_secs(2))
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .map(|v| kscreen_outputs(&v))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let topology: Vec<Output> = if outputs.is_empty() { live.iter().map(|(o, _)| o.clone()).collect() } else { outputs.to_vec() };
    let mut displays: Vec<Display> = topology
        .iter()
        .map(|o| {
            let path = paths.get(&o.name).or_else(|| live.iter().find(|(out, _)| out.name == o.name).map(|(_, path)| path));
            Display {
                id: id(&o.name),
                name: o.name.clone(),
                frame: o.frame,
                profile_name: path.cloned().filter(|p| !p.is_empty()),
                icc: path.and_then(|p| profile(p)),
                compositor_managed: !x11 && path.is_some_and(|p| !p.is_empty()),
            }
        })
        .collect();
    if x11 {
        read_x11(&mut displays);
    }
    if displays.is_empty() || displays.iter().any(|d| d.icc.is_none()) {
        let _ = read_colord(&mut displays);
    }
    if displays.is_empty() { Err("no display profiles were reported by KWin, X11 or colord".into()) } else { Ok(displays) }
}

fn read_x11(displays: &mut Vec<Display>) {
    use x11rb::{
        connection::Connection,
        protocol::xproto::{AtomEnum, ConnectionExt},
    };
    let Ok((conn, _)) = x11rb::connect(None) else { return };
    let no_topology = displays.is_empty();
    let topology_count = displays.len();
    for (n, screen) in conn.setup().roots.iter().enumerate() {
        let name = if n == 0 { "_ICC_PROFILE".into() } else { format!("_ICC_PROFILE_{n}") };
        let Some(atom) = conn.intern_atom(true, name.as_bytes()).ok().and_then(|c| c.reply().ok()).map(|r| r.atom).filter(|a| *a != 0) else { continue };
        let Some(icc) = conn
            .get_property(false, screen.root, atom, AtomEnum::CARDINAL, 0, 4 * 1024 * 1024)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| (r.bytes_after == 0).then(|| icc_atom(&r.value, r.format)).flatten())
        else {
            continue;
        };
        let icc = Arc::new(icc);
        if no_topology {
            displays.push(Display {
                id: id(&name),
                name,
                frame: [0.0, 0.0, f64::from(screen.width_in_pixels), f64::from(screen.height_in_pixels)],
                profile_name: Some("X11 ICC atom".into()),
                icc: Some(icc),
                compositor_managed: false,
            });
        } else {
            // A root atom describes a screen's default display, not every RandR output.
            // Only assign it to the primary output; colord can resolve the remaining outputs.
            use x11rb::protocol::randr::ConnectionExt as _;
            let primary = conn
                .randr_get_monitors(screen.root, true)
                .ok()
                .and_then(|c| c.reply().ok())
                .and_then(|r| r.monitors.into_iter().find(|m| m.primary))
                .and_then(|m| conn.get_atom_name(m.name).ok().and_then(|c| c.reply().ok()))
                .map(|r| String::from_utf8_lossy(&r.name).into_owned());
            for d in displays.iter_mut().filter(|d| d.icc.is_none()) {
                if primary.as_ref() == Some(&d.name) || (topology_count == 1 && n == 0) {
                    d.icc = Some(icc.clone());
                    d.profile_name = Some("X11 ICC atom".into());
                }
            }
        }
    }
}

fn read_colord(displays: &mut [Display]) -> Result<(), zbus::Error> {
    use zbus::{
        blocking::{Proxy, connection::Builder},
        zvariant::OwnedObjectPath,
    };
    let conn = Builder::system()?.method_timeout(Duration::from_secs(2)).build()?;
    let manager = Proxy::new(&conn, "org.freedesktop.ColorManager", "/org/freedesktop/ColorManager", "org.freedesktop.ColorManager")?;
    let devices: Vec<OwnedObjectPath> = manager.call("GetDevicesByKind", &("display",))?;
    for device in devices.into_iter().take(64) {
        let p = Proxy::new(&conn, "org.freedesktop.ColorManager", device, "org.freedesktop.ColorManager.Device")?;
        let metadata: HashMap<String, String> = p.get_property("Metadata")?;
        if !displays.iter().any(|d| d.icc.is_none() && metadata.get("OutputName").or_else(|| metadata.get("XRANDR_name")) == Some(&d.name)) {
            continue;
        }
        if !p.get_property::<bool>("Enabled")? {
            continue;
        }
        // The Profiles property is not ordered by the selected default. The documented
        // wildcard qualifier returns that default and honours profiling inhibitors.
        let Ok(path) = p.call::<_, _, OwnedObjectPath>("GetProfileForQualifiers", &(vec!["*"],)) else { continue };
        let p = Proxy::new(&conn, "org.freedesktop.ColorManager", path, "org.freedesktop.ColorManager.Profile")?;
        let filename: String = p.get_property("Filename")?;
        for d in displays.iter_mut().filter(|d| d.icc.is_none()) {
            // Never assign one monitor's profile to an unrelated monitor by fuzzy model name.
            if metadata.get("OutputName").or_else(|| metadata.get("XRANDR_name")) == Some(&d.name) {
                d.icc = profile(&filename);
                d.profile_name = Some(filename.clone());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linux_detection_kwin_and_kscreen_fixtures() {
        let v = serde_json::json!([{"name":"outputs","data":[{"connectorName":"DP-1","edidIdentifier":"fixture-a","colorProfileSource":"ICC","iccProfilePath":"/a.icc"},{"connectorName":"DP-2","colorProfileSource":"EDID","iccProfilePath":"/old.icc"}]}]);
        assert_eq!(kwin_profiles(&v).get("DP-1").map(String::as_str), Some("/a.icc"));
        assert!(!kwin_profiles(&v).contains_key("DP-2"));
        let v = serde_json::json!({"outputs":[{"name":"DP-1","connected":true,"enabled":true,"scale":2.0,"pos":{"x":-960,"y":0},"size":{"width":1920,"height":1080},"iccProfilePath":"/a.icc"},{"name":"off","connected":false,"enabled":false}]});
        let o = kscreen_outputs(&v);
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].0.frame, [-960.0, 0.0, 960.0, 540.0]);
        assert!(kscreen_outputs(&serde_json::json!({})).is_empty());
    }
    #[test]
    fn linux_detection_x11_atom_bytes() {
        let p = photocraft_engine::color_cmds::resolve_profile("srgb", None, None).unwrap().to_bytes().as_ref().clone();
        assert_eq!(icc_atom(&p, 8).unwrap(), p);
        assert!(icc_atom(&p, 32).is_none());
        assert!(icc_atom(&p[..100], 8).is_none());
        let mut bad = p;
        bad[0..4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(icc_atom(&bad, 8).is_none());
    }
}

#[cfg(test)]
mod desktop_test {
    #[test]
    #[ignore = "read-only connected desktop profile detection, run explicitly"]
    fn display_output_detects_connected_linux_profiles() {
        let displays = super::detect(&[], false).expect("connected desktop outputs");
        assert!(displays.iter().any(|d| d.icc.is_some()));
        for d in displays {
            if let Some(icc) = d.icc {
                photocraft_engine::color_cmds::profile_from_bytes(&icc).expect("valid runtime ICC");
            }
            eprintln!("runtime display {}: compositor managed {}, frame {:?}", d.name, d.compositor_managed, d.frame);
        }
    }
}
