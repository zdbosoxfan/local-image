use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use li_ai::catalog::{FileSpec, Preset, Role};
use li_ai::download::{Removal, download_file, download_preset, free_space, hash_file, host_allowed, human_bytes, remove_listed_file, remove_preset};
use li_ai::{JobControl, ModelId};
use sha2::{Digest, Sha256};

static DIR_SEQ: AtomicUsize = AtomicUsize::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let n = DIR_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("li-ai-dl-{tag}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn sha256(body: &[u8]) -> String {
    hex::encode(Sha256::digest(body))
}

fn file_spec(folder: &str, name: &str, body: &[u8]) -> FileSpec {
    FileSpec {
        role: Role::Unet,
        folder: folder.into(),
        name: name.into(),
        bytes: body.len() as u64,
        sha256: sha256(body),
        url: format!("https://huggingface.co/x/{name}"),
        compatible: Vec::new(),
    }
}

fn put_file(model_dir: &Path, folder: &str, name: &str, body: &[u8]) -> FileSpec {
    let spec = file_spec(folder, name, body);
    std::fs::create_dir_all(model_dir.join(folder)).unwrap();
    std::fs::write(model_dir.join(folder).join(name), body).unwrap();
    spec
}

fn preset(model: &str, files: Vec<FileSpec>) -> Preset {
    Preset {
        model: ModelId::intern(model),
        variant: "test".into(),
        label: "Test".into(),
        files,
        access_url: None,
        required_nodes: Vec::new(),
        required_choices: Vec::new(),
        installed: false,
    }
}

#[test]
fn human_bytes_formats_ranges() {
    assert_eq!(human_bytes(0), "0 bytes");
    assert_eq!(human_bytes(999), "999 bytes");
    assert_eq!(human_bytes(1000), "1 KB");
    assert_eq!(human_bytes(999_999), "1000 KB");
    assert_eq!(human_bytes(1_000_000), "1 MB");
    assert_eq!(human_bytes(1_000_000_000), "1.0 GB");
    assert_eq!(human_bytes(7_256_783_064), "7.3 GB");
}

#[test]
fn host_allowed_enforces_https_allow_list() {
    let allowed = [
        "https://github.com/a/b",
        "https://release-assets.githubusercontent.com/a",
        "https://objects.githubusercontent.com/a",
        "https://raw.githubusercontent.com/kisakutanaka/SkySegmentation/4f1715a/models/x.onnx",
        "https://huggingface.co/x/resolve/abc/f.safetensors",
        "https://cdn-lfs.huggingface.co/x",
        "https://cdn-lfs.hf.co/x",
        "https://us.aws.cdn.hf.co/x",
        "https://civitai.com/x",
        "https://civitai.red/x",
        "https://cas-bridge.xethub.hf.co/x",
        "https://foo.huggingface.co/x",
    ];
    for url in allowed {
        assert!(host_allowed(url), "{url}");
    }

    assert!(!host_allowed("http://huggingface.co/x"));
    assert!(!host_allowed("https://huggingface.co.evil.com/x"));
    assert!(!host_allowed("https://raw.githubusercontent.com.evil.com/x"));
    assert!(!host_allowed("https://example.com/x"));
    assert!(!host_allowed("ftp://huggingface.co/x"));
    assert!(!host_allowed(""));
    assert!(!host_allowed("https://"));
}

#[test]
fn hash_file_missing_file_errors() {
    let dir = temp_dir("hash-missing");
    assert!(hash_file(&dir.join("nope.bin")).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn remove_preset_with_no_files_does_nothing() {
    let dir = temp_dir("remove-empty");
    let p = preset("empty", Vec::new());
    let r = remove_preset(&dir, &p, std::slice::from_ref(&p), &|_| panic!("dispose should not be called")).unwrap();
    assert_eq!(r, Removal::default());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn remove_listed_file_errors_for_outside_or_missing_files() {
    let dir = temp_dir("remove-listed-err");
    std::fs::write(dir.join("outside.txt"), b"x").unwrap();

    assert!(remove_listed_file(&dir, &["loras"], "../outside.txt", &|_| panic!("dispose called")).is_err());
    assert!(remove_listed_file(&dir, &["loras"], "missing.safetensors", &|_| panic!("dispose called")).is_err());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn download_preset_verifies_existing_files_and_reports_progress() {
    let dir = temp_dir("dl-preset");
    let f1_body = b"model weights";
    let f2_body = b"vae weights";
    let f1 = put_file(&dir, "diffusion_models", "unet.safetensors", f1_body);
    let f2 = put_file(&dir, "vae", "vae.safetensors", f2_body);
    let p = preset("dl-preset", vec![f1, f2]);

    let ctl = JobControl::default();
    let progress = RefCell::new(Vec::new());
    download_preset(&dir, &p, &ctl, &|p| progress.borrow_mut().push(p)).unwrap();

    let progress = progress.borrow();
    assert_eq!(progress.len(), 2);
    assert_eq!(progress[0].file, "unet.safetensors");
    assert_eq!(progress[0].done_bytes, f1_body.len() as u64);
    assert_eq!(progress[0].total_bytes, (f1_body.len() + f2_body.len()) as u64);
    assert_eq!(progress[1].file, "vae.safetensors");
    assert_eq!(progress[1].done_bytes, progress[0].total_bytes);

    assert!(dir.join("diffusion_models/unet.safetensors").exists());
    assert!(dir.join("vae/vae.safetensors").exists());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn download_preset_errors_when_existing_file_is_corrupt() {
    let dir = temp_dir("dl-corrupt");
    let expected = b"expected model";
    let spec = file_spec("diffusion_models", "unet.safetensors", expected);
    std::fs::create_dir_all(dir.join("diffusion_models")).unwrap();
    std::fs::write(dir.join("diffusion_models/unet.safetensors"), vec![b'X'; expected.len()]).unwrap();

    let p = preset("dl-corrupt", vec![spec]);
    let ctl = JobControl::default();
    let progress = RefCell::new(Vec::new());
    let err = download_preset(&dir, &p, &ctl, &|p| progress.borrow_mut().push(p)).unwrap_err();

    assert!(err.to_string().contains("checksum does not match"));
    assert!(progress.borrow().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn download_preset_empty_preset_is_noop() {
    let dir = temp_dir("dl-empty");
    let p = preset("empty", Vec::new());
    let ctl = JobControl::default();
    let progress = RefCell::new(Vec::new());

    download_preset(&dir, &p, &ctl, &|p| progress.borrow_mut().push(p)).unwrap();
    assert!(progress.borrow().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn download_file_refuses_existing_destination() {
    let dir = temp_dir("dl-existing");
    let dest = dir.join("model.bin");
    std::fs::write(&dest, b"already here").unwrap();

    let ctl = JobControl::default();
    let err = download_file("https://huggingface.co/x/model.bin", &dest, 4, &sha256(b"xxxx"), &ctl, &|_| {}).unwrap_err();

    assert!(err.to_string().contains("already exists"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn download_file_errors_when_parent_is_a_file() {
    let dir = temp_dir("dl-parent");
    let blocked = dir.join("blocked");
    std::fs::write(&blocked, b"file not dir").unwrap();
    let dest = blocked.join("model.bin");

    let ctl = JobControl::default();
    let err = download_file("https://huggingface.co/x/model.bin", &dest, 0, &sha256(b""), &ctl, &|_| {}).unwrap_err();

    assert!(!err.to_string().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn free_space_missing_dir_is_none() {
    let missing = std::env::temp_dir().join("li-ai-free-space-definitely-missing");
    assert!(free_space(&missing).is_none());
}
