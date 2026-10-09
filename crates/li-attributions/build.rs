//! Build every licence file `assets/attributions.json` refers to into the binary
//! (`$OUT_DIR/licence_texts.rs`), so the Attributions page can show the full text offline. The list
//! follows the JSON: regenerating it with `cargo xtask attributions` is all a new licence file
//! needs.

use std::{env, fs, path::PathBuf};

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).join("../..");
    let json_path = root.join("assets/attributions.json");
    println!("cargo:rerun-if-changed={}", json_path.display());
    let text = fs::read_to_string(&json_path).unwrap_or_else(|e| panic!("{}: {e}", json_path.display()));
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", json_path.display()));
    let mut files: Vec<String> = Vec::new();
    let mut add = |entry: &serde_json::Value| {
        for f in entry.get("licence_files").and_then(|l| l.as_array()).into_iter().flatten().filter_map(|f| f.as_str()) {
            files.push(f.to_string());
        }
    };
    add(&v["app"]);
    for s in v["sections"].as_array().into_iter().flatten() {
        for e in s["entries"].as_array().into_iter().flatten() {
            add(e);
        }
    }
    files.sort();
    files.dedup();
    let mut out = String::from(
        "/// (path from the repository root, text) of every licence file the attributions refer to.\npub static LICENCE_TEXTS: &[(&str, &str)] = &[\n",
    );
    for f in &files {
        assert!(!f.contains("..") && !f.starts_with('/'), "licence file {f} must be a path inside the repository");
        let path = root.join(f).canonicalize().unwrap_or_else(|e| panic!("licence file {f}: {e}"));
        println!("cargo:rerun-if-changed={}", path.display());
        out.push_str(&format!("    ({f:?}, include_str!({:?})),\n", path.display().to_string()));
    }
    out.push_str("];\n");
    fs::write(PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("licence_texts.rs"), out).expect("write licence_texts.rs");
}
