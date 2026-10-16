//! Inspect embedded correction metadata without developing pixels.
//! `cargo run --offline -p lightcraft-raw --example embedded_lens_info -- FILE...`

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path)?;
        let info = lightcraft_raw::probe_info(&bytes)?;
        println!("{path}: {:?} {}x{} {:?}", info.format, info.width, info.height, info.vendor_lens);
    }
    Ok(())
}
