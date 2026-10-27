//! The installed `local-image-cli` is a symlink to the `local-image` executable: started under that
//! name, the app binary must be the command line (same output and exit status) and never open a
//! window.
#![cfg(unix)]
#![expect(clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed")]

use std::path::PathBuf;
use std::process::Command;

fn cli_link() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("local-image-cli-dispatch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let link = dir.join("local-image-cli");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_local-image"), &link).unwrap();
    link
}

#[test]
fn the_app_started_as_local_image_cli_runs_the_command_line() {
    let link = cli_link();

    let version = Command::new(&link).arg("--version").output().unwrap();
    assert_eq!(version.status.code(), Some(0));
    let stdout = String::from_utf8(version.stdout).unwrap();
    assert!(stdout.starts_with("local-image-cli "), "{stdout}");

    let help = Command::new(&link).arg("--help").output().unwrap();
    assert_eq!(help.status.code(), Some(0));
    assert_eq!(String::from_utf8(help.stdout).unwrap(), photocraft_cli::USAGE);

    // No arguments is a usage error on stderr, not a window.
    let bare = Command::new(&link).output().unwrap();
    assert_eq!(bare.status.code(), Some(2));
    assert!(bare.stdout.is_empty());
    assert_eq!(String::from_utf8(bare.stderr).unwrap(), photocraft_cli::USAGE);

    let _ = std::fs::remove_dir_all(link.parent().unwrap());
}

#[test]
fn the_app_under_its_own_name_is_not_the_command_line() {
    let out = Command::new(env!("CARGO_BIN_EXE_local-image")).arg("--version").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.starts_with("local-image ") && !stdout.starts_with("local-image-cli"), "{stdout}");
}
