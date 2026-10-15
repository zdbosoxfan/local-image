//! Explicit portable smoke lane. Full tests remain a separate, unfiltered gate.
use std::process::Command;

use anyhow::Result;

const CORE: &[&str] = &[
    "lightcraft-geom",
    "photocraft-geom",
    "li-color",
    "photocraft-color",
    "lightcraft-meta",
    "lightcraft-catalog",
    "photocraft-doc",
    "photocraft-format",
    "xtask",
];

pub fn fast() -> Result<()> {
    let start = std::time::Instant::now();
    let mut core = command();
    for package in CORE {
        core.args(["-p", package]);
    }
    crate::run("fast: portable core suites", &mut core)?;
    let mut ui = command();
    ui.args([
        "--lib",
        "-p",
        "photocraft-ui-egui",
        "-p",
        "lightcraft-ui-egui",
        "-p",
        "li-ai",
        "--",
        "updates_ui::tests::release_",
        "updates::tests::release_",
        "tests_updates::release_",
    ]);
    crate::run("fast: release service and UI interactions", &mut ui)?;
    println!("fast lane passed in {:.1}s (including incremental compilation)", start.elapsed().as_secs_f64());
    Ok(())
}

fn command() -> Command {
    let mut command = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    command.current_dir(crate::root()).args(["test", "--offline", "--locked"]);
    command
}

#[cfg(test)]
mod tests {
    #[test]
    fn portable_core_packages_exist_in_the_workspace() -> anyhow::Result<()> {
        let root = crate::root();
        let mut manifests = vec![root.join("xtask/Cargo.toml")];
        for directory in ["crates", "apps"] {
            for entry in std::fs::read_dir(root.join(directory))? {
                let manifest = entry?.path().join("Cargo.toml");
                if manifest.is_file() {
                    manifests.push(manifest);
                }
            }
        }
        let mut packages = std::collections::BTreeSet::new();
        for manifest in manifests {
            let value: toml::Value = toml::from_str(&std::fs::read_to_string(manifest)?)?;
            if let Some(name) = value.get("package").and_then(|p| p.get("name")).and_then(toml::Value::as_str) {
                packages.insert(name.to_owned());
            }
        }
        for package in super::CORE {
            assert!(packages.contains(*package), "portable core lane refers to missing package {package}");
        }
        Ok(())
    }

    #[test]
    fn release_fast_lane_is_offline_and_has_no_failure_allowlist() {
        let command = super::command();
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["test", "--offline", "--locked"]);
        for name in super::CORE {
            assert!(!name.contains("gpu"));
        }
        assert!(command.get_envs().next().is_none(), "inherit the owner's build concurrency");
    }
}
