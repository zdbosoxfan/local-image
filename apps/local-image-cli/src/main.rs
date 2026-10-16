#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

fn main() {
    // One compute pool sized from the hardware, before anything uses rayon (`LOCAL_IMAGE_THREADS`
    // and `RAYON_NUM_THREADS` override it).
    lightcraft_sysmem::init(lightcraft_sysmem::Overrides::default());
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = photocraft_cli::run(&args, &mut std::io::stdout(), &mut std::io::stderr());
    std::process::exit(code);
}
