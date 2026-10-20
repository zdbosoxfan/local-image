//! A single model choice per function, shared by every enhancement entry point.
use super::Method;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Choice {
    Curated { id: String },
    Custom { name: String, file: String, bytes: u64, sha256: String, tile: usize, scale: u32 },
    Seed { unet: String, vae: String },
}

pub fn default_choice(method: Method) -> Choice {
    match method {
        Method::Faithful => Choice::Curated { id: "realplksr-2x".into() },
        Method::Rescue => Choice::Curated { id: "bsrgan-2x".into() },
        Method::Generative => Choice::Seed { unet: "seedvr2_7b_fp16.safetensors".into(), vae: "seedvr2_ema_vae_fp16.safetensors".into() },
    }
}

pub fn load(dir: &Path, method: Method) -> Choice {
    std::fs::read(dir.join("enhance-functions.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<BTreeMap<String, Choice>>(&b).ok())
        .and_then(|mut choices| choices.remove(method.key()))
        .filter(|choice| validate(method, choice).is_ok())
        .unwrap_or_else(|| default_choice(method))
}

pub fn save(dir: &Path, method: Method, choice: Option<Choice>) -> Result<()> {
    if let Some(c) = &choice {
        supported(method, c)?;
    }
    static SAVES: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _save = SAVES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    std::fs::create_dir_all(dir)?;
    let lock = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(dir.join(".enhance-functions.lock"))?;
    lock.lock()?;
    let path = dir.join("enhance-functions.json");
    let mut choices: BTreeMap<String, Choice> = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).context("Invalid enhancement choices file")?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(e) => return Err(e.into()),
    };
    if let Some(c) = choice {
        choices.insert(method.key().into(), c);
    } else {
        choices.remove(method.key());
    }
    li_fs::write_atomic(&path, &serde_json::to_vec_pretty(&choices)?)?;
    Ok(())
}

/// The product only submits accelerator-backed choices. Structural custom validation remains
/// available to CPU reference tools, but it cannot authorize a production setting.
pub fn supported(method: Method, choice: &Choice) -> Result<()> {
    validate(method, choice)?;
    if matches!(choice, Choice::Custom { .. }) {
        bail!("Custom CPU ONNX enlargement is unavailable; choose an approved GPU enlargement model.");
    }
    Ok(())
}

pub fn validate(method: Method, choice: &Choice) -> Result<()> {
    let safe = |s: &str| !s.is_empty() && !s.contains(['/', '\\', '\0']) && s != "." && s != "..";
    match choice {
        Choice::Curated { id } if method != Method::Generative && li_seg::spec(id).is_some_and(|s| s.task == li_seg::Task::Upscale) => Ok(()),
        Choice::Custom { file, tile, scale, sha256, .. }
            if method != Method::Generative
                && safe(file)
                && file.ends_with(".onnx")
                && (16..=1024).contains(tile)
                && matches!(scale, 2 | 4)
                && sha256.len() == 64
                && sha256.bytes().all(|b| b.is_ascii_hexdigit()) =>
        {
            Ok(())
        }
        Choice::Seed { unet, vae } if method == Method::Generative && [unet, vae].iter().all(|s| safe(s) && s.ends_with(".safetensors")) => Ok(()),
        _ => bail!("Unsupported enhancement model choice"),
    }
}

pub fn custom_path(dir: &Path, choice: &Choice) -> Result<std::path::PathBuf> {
    let Choice::Custom { file, .. } = choice else { bail!("Not a custom ONNX model") };
    validate(Method::Faithful, choice)?;
    Ok(dir.join("segmentation").join(file))
}

/// Validate and run a small sample before publishing a user-selected model. Input and output
/// must be float NCHW RGB in 0–1; dimensions identify its fixed tile and native 2×/4× scale.
pub fn import_custom(dir: &Path, method: Method, source: &Path) -> Result<Choice> {
    // Reject before reading, compiling, copying or changing a model choice.
    let _ = (dir, method, source);
    bail!("Custom CPU ONNX enlargement is unavailable; choose an approved GPU enlargement model.")
}

#[cfg(test)]
fn import_reference(dir: &Path, method: Method, source: &Path) -> Result<Choice> {
    if method == Method::Generative || source.extension().is_none_or(|e| e != "onnx") {
        bail!("Choose an ONNX restoration model");
    }
    let bytes = std::fs::read(source)?;
    let model = li_seg::enhance::Upscaler::from_custom_onnx(&bytes)?;
    model.run(&image::RgbImage::new(2, 2), &|| Ok(()), &|_, _| {})?;
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let file = format!("custom-upscale-{}-{}.onnx", method.key(), &sha256[..16]);
    let choice = Choice::Custom {
        name: source.file_name().context("Model has no filename")?.to_string_lossy().into_owned(),
        file,
        bytes: bytes.len() as u64,
        sha256,
        tile: model.tile,
        scale: model.scale as u32,
    };
    let path = custom_path(dir, &choice)?;
    std::fs::create_dir_all(path.parent().context("Model folder")?)?;
    li_fs::write_atomic(&path, &bytes)?;
    Ok(choice)
}

pub fn load_custom(dir: &Path, choice: &Choice) -> Result<li_seg::enhance::Upscaler> {
    let Choice::Custom { bytes: expected, sha256, tile, scale, .. } = choice else { bail!("Not a custom ONNX model") };
    let bytes = std::fs::read(custom_path(dir, choice)?)?;
    if bytes.len() as u64 != *expected || !format!("{:x}", Sha256::digest(&bytes)).eq_ignore_ascii_case(sha256) {
        bail!("The custom enhancement model changed; choose it again.");
    }
    li_seg::enhance::Upscaler::from_onnx(&bytes, *tile, *scale as usize)
}

#[cfg(test)]
mod tests {

    #[test]
    fn review_custom_cpu_import_never_changes_production_choice_or_files() {
        let dir = std::env::temp_dir().join(format!("review-enhance-import-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("nearest.onnx");
        let bytes = include_bytes!("../../../li-seg/tests/fixtures/nearest_2x_32.onnx");
        std::fs::write(&source, bytes).unwrap();
        for method in Method::ALL {
            save(&dir, method, Some(default_choice(method))).unwrap();
            let before = std::fs::read(dir.join("enhance-functions.json")).unwrap();
            assert!(import_custom(&dir, method, &source).unwrap_err().to_string().contains("Custom CPU ONNX"));
            assert_eq!(std::fs::read(dir.join("enhance-functions.json")).unwrap(), before);
            assert_eq!(std::fs::read(&source).unwrap(), bytes);
            assert!(!dir.join("segmentation").exists());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn choices_durable_write_fault_preserves_previous_json_without_staging_leak() {
        let dir = std::env::temp_dir().join(format!("enhance-choices-{}", uuid::Uuid::new_v4()));
        save(&dir, Method::Faithful, Some(default_choice(Method::Faithful))).unwrap();
        let path = dir.join("enhance-functions.json");
        let bytes = std::fs::read(&path).unwrap();
        {
            let _fault = li_fs::fail_writes_after(3);
            assert!(save(&dir, Method::Rescue, Some(default_choice(Method::Rescue))).is_err());
        }
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2); // JSON and existing lock file.
        let before = li_fs::syncs_on_this_thread();
        save(&dir, Method::Rescue, Some(default_choice(Method::Rescue))).unwrap();
        assert!(li_fs::syncs_on_this_thread() > before);
        let choices: BTreeMap<String, Choice> = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(choices.len(), 2);
        assert_eq!(choices[Method::Faithful.key()], default_choice(Method::Faithful));
        assert_eq!(choices[Method::Rescue.key()], default_choice(Method::Rescue));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ds2_choices_preserve_concurrent_updates_and_corrupt_files() {
        let dir = std::env::temp_dir().join(format!("ds2-choices-{}", uuid::Uuid::new_v4()));
        std::thread::scope(|scope| {
            for method in [Method::Faithful, Method::Rescue] {
                let dir = &dir;
                scope.spawn(move || {
                    for _ in 0..8 {
                        save(dir, method, Some(default_choice(method))).unwrap();
                    }
                });
            }
        });
        let path = dir.join("enhance-functions.json");
        let choices: BTreeMap<String, Choice> = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(choices.len(), 2);
        std::fs::write(&path, b"{broken").unwrap();
        assert!(save(&dir, Method::Faithful, None).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{broken");
        std::fs::remove_dir_all(dir).unwrap();
    }

    use super::*;
    #[test]
    fn enhance_function_choices_default_custom_validation_and_shared_execution() {
        let dir = std::env::temp_dir().join(format!("enhance-choice-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for method in Method::ALL {
            assert_eq!(load(&dir, method), default_choice(method));
        }
        let alternative = Choice::Curated { id: "realplksr-2x".into() };
        save(&dir, Method::Rescue, Some(alternative.clone())).unwrap();
        assert_eq!(load(&dir, Method::Rescue), alternative);
        save(&dir, Method::Rescue, None).unwrap();
        let source = dir.join("nearest.onnx");
        std::fs::write(&source, include_bytes!("../../../li-seg/tests/fixtures/nearest_2x_32.onnx")).unwrap();
        let custom = import_reference(&dir, Method::Faithful, &source).unwrap();
        assert!(save(&dir, Method::Faithful, Some(custom.clone())).is_err());
        assert!(matches!(&custom, Choice::Custom { scale: 2, tile: 32, .. }));
        let input = image::RgbaImage::from_pixel(17, 13, image::Rgba([30, 100, 200, 255]));
        // Model import/validation retains its independent CPU reference test. The app route
        // follows the owner's GPU/NPU-only policy and must refuse a legacy CPU selection.
        let reference_model = load_custom(&dir, &custom).unwrap();
        let rgb = image::DynamicImage::ImageRgba8(input.clone()).to_rgb8();
        let out = reference_model.run(&rgb, &|| Ok(()), &|_, _| {}).unwrap();
        assert_eq!(out.dimensions(), (34, 26));
        assert_eq!(*out.get_pixel(10, 10), image::Rgb([30, 100, 200]));
        let error = crate::Ai::new("127.0.0.1:1")
            .enhance(&input, 34, 26, Method::Faithful, super::super::Backend::Cpu, &dir, 1, &crate::JobControl::new())
            .unwrap_err();
        assert!(error.to_string().contains("CPU inference is disabled"));
        let mut upper = custom.clone();
        if let Choice::Custom { sha256, .. } = &mut upper {
            *sha256 = sha256.to_ascii_uppercase();
        }
        assert!(load_custom(&dir, &upper).is_ok());
        std::fs::write(custom_path(&dir, &custom).unwrap(), b"corrupt").unwrap();
        assert!(load_custom(&dir, &custom).is_err());
        let escaped = Choice::Custom { file: "../outside.onnx".into(), name: "bad".into(), bytes: 1, sha256: "0".repeat(64), tile: 32, scale: 2 };
        assert!(save(&dir, Method::Faithful, Some(escaped)).is_err());
        assert!(import_custom(&dir, Method::Generative, &source).is_err());
        save(&dir, Method::Faithful, None).unwrap();
        assert_eq!(load(&dir, Method::Faithful), default_choice(Method::Faithful));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
