//! Assemble shared helpers without changing the compositor's public `SHADER: &str` API.
fn main() -> std::io::Result<()> {
    let common = "../li-gpu/wgsl/common.wgsl";
    let body = "src/compose.wgsl";
    println!("cargo:rerun-if-changed={common}");
    println!("cargo:rerun-if-changed={body}");
    let source = std::fs::read_to_string(common)? + "\n" + &std::fs::read_to_string(body)?;
    let output = std::env::var_os("OUT_DIR").ok_or_else(|| std::io::Error::other("Cargo did not provide OUT_DIR"))?;
    std::fs::write(std::path::PathBuf::from(output).join("compose.wgsl"), source)
}
