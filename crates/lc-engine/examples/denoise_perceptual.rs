//! Supplemental semantic similarity, using the existing verified CLIP bundle (not LPIPS).
use std::path::PathBuf;
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    let models = PathBuf::from(args.get(1).ok_or("CLIP model directory")?);
    let data = PathBuf::from(args.get(2).ok_or("evaluation directory")?);
    let clip = li_seg::Clip::load(&models, li_seg::Group::Tagging.official()).map_err(|e| format!("{e:#}"))?;
    for scene in ["test-canon", "test-sony", "fuji"] {
        for index in 0..2 {
            let embedding = |name: &str| -> Result<Vec<f32>, String> {
                let b = std::fs::read(data.join(format!("{scene}-{index}-{name}.png"))).map_err(|e| e.to_string())?;
                let img = lightcraft_codecs::decode(&b, Default::default()).map_err(|e| e.to_string())?.to_srgb8();
                clip.embed_image(img.data.as_flattened(), img.width, img.height, li_seg::Crops::Center).map_err(|e| format!("{e:#}"))
            };
            let reference = embedding("reference")?;
            for name in ["input", "rawnind-linear", "nind-rgb", "rawnind-bayer", "wavelet-cpu"] {
                if !data.join(format!("{scene}-{index}-{name}.png")).exists() {
                    continue;
                }
                let out = embedding(name)?;
                let cosine: f32 = out.iter().zip(&reference).map(|(a, b)| a * b).sum();
                println!("{scene},{index},{name},CLIP_cosine={cosine:.8}");
            }
        }
    }
    Ok(())
}
