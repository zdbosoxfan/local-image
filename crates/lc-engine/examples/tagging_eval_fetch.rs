//! Fetch only the licensed evaluation photos, never model weights or owner-library data.
//! Curl supplies HTTP and sha256sum supplies hashing; neither is linked into the app.
use serde_json::Value;
use std::{path::PathBuf, process::Command};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest = root.join("crates/lc-engine/tests/references/tagging-eval/manifest.json");
    let rows: Vec<Value> = serde_json::from_slice(&std::fs::read(manifest)?)?;
    let dir = root.join("target/tagging-eval/images");
    std::fs::create_dir_all(&dir)?;
    for row in rows {
        let name = row["file"].as_str().ok_or("file")?;
        if name.contains(['/', '\\']) || name.starts_with('.') {
            return Err("invalid fixture filename".into());
        }
        let file = dir.join(name);
        if !file.exists() {
            let url = row["download_url"].as_str().ok_or("download_url")?;
            if !url.starts_with("https://") {
                return Err("HTTPS required".into());
            }
            let part = file.with_extension("part");
            let result = Command::new("curl")
                .args([
                    "--fail",
                    "--location",
                    "--silent",
                    "--show-error",
                    "--retry",
                    "3",
                    "--retry-delay",
                    "6",
                    "--max-time",
                    "120",
                    "--user-agent",
                    "LocalImageEvaluation/1.0 (open-source GPL application; local research)",
                    "--output",
                ])
                .arg(&part)
                .arg(url)
                .status()?;
            if !result.success() {
                return Err(format!("fetch failed: {name}").into());
            }
            std::fs::rename(part, &file)?;
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        if std::fs::metadata(&file)?.len() != row["bytes"].as_u64().ok_or("bytes")? {
            return Err(format!("size mismatch: {name}").into());
        }
        let hash = Command::new("sha256sum").arg(&file).output()?;
        let hash = String::from_utf8(hash.stdout)?;
        if hash.split_whitespace().next() != row["sha256"].as_str() {
            return Err(format!("SHA-256 mismatch: {name}").into());
        }
        println!("verified {name}");
    }
    Ok(())
}
