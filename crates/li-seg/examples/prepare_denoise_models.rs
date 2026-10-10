//! Verify packages fetched by the app's download flow in its test temp dir, then install them
//! through the normal bundle installer. Optional second argument is the approved test cache.
fn main() -> anyhow::Result<()> {
    let package_dir = std::env::args_os().nth(1).map(std::path::PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("local-image-denoise-vetting"));
    let dir = std::env::temp_dir().join("local-image-denoise-installed-models");
    for group in [li_seg::Group::RawDenoise, li_seg::Group::RgbDenoise] {
        let spec = group.official();
        let pkg = package_dir.join(format!("{}.dtmodel", spec.id));
        li_seg::denoise::install_bundle(&pkg, spec, &dir)?;
        println!("{} installed {} verified bytes", spec.id, li_seg::verified_denoise_bytes(&dir, spec).ok_or_else(|| anyhow::anyhow!("verification failed"))?);
        if let Some(cache) = std::env::args_os().nth(2) {
            let cache = std::path::PathBuf::from(cache);
            std::fs::create_dir_all(cache.join("segmentation"))?;
            for f in spec.files() {
                std::fs::copy(li_seg::companion_path(&dir, f.file), li_seg::companion_path(&cache, f.file))?;
            }
        }
    }
    Ok(())
}
