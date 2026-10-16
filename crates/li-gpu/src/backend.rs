//! Which wgpu backends Local Image lets wgpu use — for its compute device and for the desktop
//! window (eframe) (issue #136). The crash sentinel around window startup is [`crate::startup`].
//!
//! wgpu loads the system driver of *every* backend in the instance's set while it enumerates
//! adapters, even when it then picks another one: with Vulkan in the set, a broken Vulkan driver
//! can crash the process (an access violation inside the driver DLL, which nothing can catch)
//! although DX12 would have been used. So Windows defaults to DX12 alone. macOS uses Metal, Linux
//! Vulkan (plus GL for the window when the build has it).
//!
//! Overrides, read once per process:
//! - `LOCAL_IMAGE_GPU_BACKEND` = `dx12` | `vulkan` | `metal` | `gl` (comma lists allowed), `auto`
//!   (the platform default), or `off` (no GPU compute; the window still needs one backend and
//!   keeps the platform default);
//! - else `WGPU_BACKEND` (wgpu's own variable, same names) for both the window and compute.
//!
//! Backends this build doesn't contain (e.g. `gl`: the GL backend isn't compiled in) are ignored
//! with a warning and the platform default is used.

pub use wgpu::Backends;

/// What the environment asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendChoice {
    /// The platform default.
    Auto,
    /// No GPU compute.
    Off,
    /// These backends.
    Use(Backends),
}

/// Parse a backend list (`"dx12"`, `"vulkan,gl"`, `"auto"`, `"off"`…). `None`: nothing usable in
/// it (empty or unknown names).
pub fn parse_backends(s: &str) -> Option<BackendChoice> {
    let s = s.trim().to_ascii_lowercase();
    match s.as_str() {
        "" => None,
        "auto" | "default" => Some(BackendChoice::Auto),
        "off" | "none" | "cpu" | "0" | "false" | "no" => Some(BackendChoice::Off),
        _ => {
            let b = Backends::from_comma_list(&s);
            (!b.is_empty()).then_some(BackendChoice::Use(b))
        }
    }
}

/// The choice from `LOCAL_IMAGE_GPU_BACKEND` (`local_image`) or else `WGPU_BACKEND` (`wgpu`): the
/// first one that parses wins; an unknown value is skipped (with a warning).
pub fn choose(local_image: Option<&str>, wgpu: Option<&str>) -> BackendChoice {
    for (name, v) in [("LOCAL_IMAGE_GPU_BACKEND", local_image), ("WGPU_BACKEND", wgpu)] {
        let Some(v) = v else { continue };
        match parse_backends(v) {
            // WGPU_BACKEND is wgpu's variable: it selects backends, it never turns the GPU off
            Some(BackendChoice::Off) if name == "WGPU_BACKEND" => log::warn!("gpu: WGPU_BACKEND={v} ignored (use LOCAL_IMAGE_GPU_BACKEND=off)"),
            Some(c) => return c,
            None => log::warn!("gpu: {name}={v} names no known backend; ignored"),
        }
    }
    BackendChoice::Auto
}

/// The desktop startup override shared by the shell and its compute consumers.
/// `WGPU_BACKEND` retains the shell's highest priority. A valid
/// `LOCAL_IMAGE_GPU_BACKEND` backend list selects that same window device when no wgpu
/// override exists. Its `off` only disables compute through [`env_off`], since the
/// window still needs a renderer. `auto` leaves the shell's platform/recovery policy in charge.
pub fn startup_environment(local_image: Option<&str>, wgpu: Option<&str>) -> Option<String> {
    for (name, value) in [("WGPU_BACKEND", wgpu), ("LOCAL_IMAGE_GPU_BACKEND", local_image)] {
        let Some(value) = value else { continue };
        match parse_backends(value) {
            Some(BackendChoice::Use(_)) => return Some(value.trim().to_ascii_lowercase()),
            Some(BackendChoice::Auto) => return None,
            Some(BackendChoice::Off) if name == "WGPU_BACKEND" => log::warn!("gpu: WGPU_BACKEND={value} ignored (use LOCAL_IMAGE_GPU_BACKEND=off)"),
            Some(BackendChoice::Off) => return None,
            None => log::warn!("gpu: {name}={value} names no known backend; ignored"),
        }
    }
    None
}

/// The backends compute devices use when nothing is asked for on `os` (`std::env::consts::OS`).
pub fn default_compute_backends(os: &str) -> Backends {
    match os {
        // never touch the Vulkan driver unless asked: see the module docs (issue #136)
        "windows" => Backends::DX12,
        "macos" | "ios" => Backends::METAL,
        _ => Backends::PRIMARY,
    }
}

/// The backends the window uses when nothing is asked for on `os`.
pub fn default_window_backends(os: &str) -> Backends {
    match os {
        "windows" => Backends::DX12,
        "macos" | "ios" => Backends::METAL,
        // eframe's default (GL only when the build has it)
        _ => Backends::PRIMARY | Backends::GL,
    }
}

/// Resolve `choice` against the backends this build contains (`compiled`): `None` = GPU off.
/// Asked-for backends that aren't compiled in fall back to `default`.
pub fn resolve(choice: BackendChoice, default: Backends, compiled: Backends) -> Option<Backends> {
    match choice {
        BackendChoice::Off => None,
        BackendChoice::Auto => Some(default),
        BackendChoice::Use(b) if b.intersects(compiled) => Some(b & compiled),
        BackendChoice::Use(b) => {
            log::warn!("gpu: backend {b:?} is not in this build ({compiled:?}); using {default:?}");
            Some(default)
        }
    }
}

fn env_choice() -> BackendChoice {
    static C: std::sync::OnceLock<BackendChoice> = std::sync::OnceLock::new();
    *C.get_or_init(|| {
        let li = std::env::var("LOCAL_IMAGE_GPU_BACKEND").ok();
        let wg = std::env::var("WGPU_BACKEND").ok();
        choose(li.as_deref(), wg.as_deref())
    })
}

/// The backends of the compute device (`None`: `LOCAL_IMAGE_GPU_BACKEND=off`).
pub fn compute_backends() -> Option<Backends> {
    resolve(env_choice(), default_compute_backends(std::env::consts::OS), wgpu::Instance::enabled_backend_features())
}

/// The backends for the desktop window's renderer (eframe/egui-wgpu). The window always needs
/// one: `off` keeps the platform default.
pub fn window_backends() -> Backends {
    let default = default_window_backends(std::env::consts::OS);
    let choice = match env_choice() {
        BackendChoice::Off => BackendChoice::Auto,
        c => c,
    };
    resolve(choice, default, wgpu::Instance::enabled_backend_features()).unwrap_or(default)
}

/// `LOCAL_IMAGE_GPU_BACKEND=off`.
pub fn env_off() -> bool {
    env_choice() == BackendChoice::Off
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn j12_startup_environment_preserves_shell_priority_and_compute_off() {
        assert_eq!(startup_environment(None, None), None);
        assert_eq!(startup_environment(Some("vulkan"), Some(" DX12 ")), Some("dx12".into()));
        assert_eq!(startup_environment(Some("vk,gl"), None), Some("vk,gl".into()));
        assert_eq!(startup_environment(Some("metal"), Some("unknown")), Some("metal".into()));
        assert_eq!(startup_environment(Some("off"), Some("vulkan")), Some("vulkan".into()));
        assert_eq!(choose(Some("off"), Some("vulkan")), BackendChoice::Off, "LOCAL_IMAGE_GPU_BACKEND=off still turns compute off");
        assert_eq!(startup_environment(Some("off"), None), None);
        assert_eq!(startup_environment(Some("auto"), None), None);
        assert_eq!(startup_environment(Some("vulkan"), Some("auto")), None);
        assert_eq!(startup_environment(Some("vulkan"), Some("off")), Some("vulkan".into()));
        assert_eq!(startup_environment(Some("unknown"), Some("unknown")), None);
    }

    #[test]
    fn parses_backend_names() {
        assert_eq!(parse_backends("dx12"), Some(BackendChoice::Use(Backends::DX12)));
        assert_eq!(parse_backends(" D3D12 "), Some(BackendChoice::Use(Backends::DX12)));
        assert_eq!(parse_backends("vulkan"), Some(BackendChoice::Use(Backends::VULKAN)));
        assert_eq!(parse_backends("vk,gl"), Some(BackendChoice::Use(Backends::VULKAN | Backends::GL)));
        assert_eq!(parse_backends("metal"), Some(BackendChoice::Use(Backends::METAL)));
        assert_eq!(parse_backends("auto"), Some(BackendChoice::Auto));
        for off in ["off", "OFF", "none", "cpu", "0"] {
            assert_eq!(parse_backends(off), Some(BackendChoice::Off), "{off}");
        }
        assert_eq!(parse_backends(""), None);
        assert_eq!(parse_backends("directx9"), None);
    }

    #[test]
    fn local_image_variable_wins_over_wgpu_backend() {
        assert_eq!(choose(None, None), BackendChoice::Auto);
        assert_eq!(choose(None, Some("dx12")), BackendChoice::Use(Backends::DX12));
        assert_eq!(choose(Some("vulkan"), Some("dx12")), BackendChoice::Use(Backends::VULKAN));
        assert_eq!(choose(Some("off"), Some("dx12")), BackendChoice::Off);
        // unknown LOCAL_IMAGE_GPU_BACKEND value: WGPU_BACKEND still applies
        assert_eq!(choose(Some("bogus"), Some("gl")), BackendChoice::Use(Backends::GL));
        // WGPU_BACKEND never turns the GPU off
        assert_eq!(choose(None, Some("off")), BackendChoice::Auto);
    }

    /// Issue #136: Windows must not load the Vulkan driver unless asked to.
    #[test]
    fn windows_defaults_to_dx12_only() {
        assert_eq!(default_compute_backends("windows"), Backends::DX12);
        assert_eq!(default_window_backends("windows"), Backends::DX12);
        assert!(!default_window_backends("windows").contains(Backends::VULKAN));
        assert_eq!(default_compute_backends("macos"), Backends::METAL);
        assert!(default_compute_backends("linux").contains(Backends::VULKAN));
        assert!(default_window_backends("linux").contains(Backends::VULKAN | Backends::GL));
    }

    #[test]
    fn resolves_against_the_build() {
        let compiled = Backends::DX12 | Backends::VULKAN;
        let def = Backends::DX12;
        assert_eq!(resolve(BackendChoice::Auto, def, compiled), Some(def));
        assert_eq!(resolve(BackendChoice::Off, def, compiled), None);
        assert_eq!(resolve(BackendChoice::Use(Backends::VULKAN), def, compiled), Some(Backends::VULKAN));
        assert_eq!(resolve(BackendChoice::Use(Backends::VULKAN | Backends::GL), def, compiled), Some(Backends::VULKAN));
        // GL isn't compiled in: the default, not an empty set (no adapter at all)
        assert_eq!(resolve(BackendChoice::Use(Backends::GL), def, compiled), Some(def));
    }
}
