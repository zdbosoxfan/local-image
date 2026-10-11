use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use li_ai::catalog::{FileSpec, Preset, Role};
use li_ai::download::{
    Removal, allow_test_host, download_file, download_preset, free_space, hash_file, host_allowed, human_bytes, listed_file, missing_files, removal_plan,
    remove_listed_file, remove_preset, target_path, verify_existing,
};
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

fn file_spec_custom(folder: &str, name: &str, bytes: u64, sha: String) -> FileSpec {
    FileSpec {
        role: Role::Unet,
        folder: folder.into(),
        name: name.into(),
        bytes,
        sha256: sha,
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
fn allow_test_host_admits_exact_plain_http_authority() {
    let authority = "127.0.0.1:39991";
    assert!(!host_allowed(&format!("http://{authority}/x")));
    allow_test_host(authority);
    assert!(host_allowed(&format!("http://{authority}/x")));
    assert!(!host_allowed(&format!("http://{authority}.evil:80/x")));
}

#[test]
fn hash_file_reads_exact_bytes() {
    let dir = temp_dir("hash");

    let empty = dir.join("empty.bin");
    std::fs::write(&empty, b"").unwrap();
    let (n, sha) = hash_file(&empty).unwrap();
    assert_eq!(n, 0);
    assert_eq!(sha, sha256(b""));

    let hello = dir.join("hello.bin");
    std::fs::write(&hello, b"hello").unwrap();
    let (n, sha) = hash_file(&hello).unwrap();
    assert_eq!(n, 5);
    assert_eq!(sha, sha256(b"hello"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hash_file_missing_file_errors() {
    let dir = temp_dir("hash-missing");
    assert!(hash_file(&dir.join("nope.bin")).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_existing_missing_file_is_false() {
    let dir = temp_dir("verify-missing");
    let spec = file_spec("vae", "f.bin", b"hello");
    assert!(!verify_existing(&dir.join("vae/f.bin"), &spec).unwrap());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_existing_size_mismatch_is_error() {
    let dir = temp_dir("verify-size");
    let path = dir.join("f.bin");
    std::fs::write(&path, b"abc").unwrap();
    let spec = file_spec_custom("", "f.bin", 5, sha256(b"abc"));
    assert!(verify_existing(&path, &spec).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_existing_hash_mismatch_is_error() {
    let dir = temp_dir("verify-hash");
    let path = dir.join("f.bin");
    std::fs::write(&path, b"abc").unwrap();
    let spec = file_spec_custom("", "f.bin", 3, sha256(b"XXX"));
    assert!(verify_existing(&path, &spec).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_existing_accepts_exact_and_compatible() {
    let dir = temp_dir("verify-compat");
    let path = dir.join("f.bin");
    std::fs::write(&path, b"hello").unwrap();

    let good = file_spec_custom("", "f.bin", 5, sha256(b"hello"));
    assert!(verify_existing(&path, &good).unwrap());

    let mut with_compat = good.clone();
    with_compat.sha256 = sha256(b"WRONG PRIMARY");
    with_compat.compatible = vec![(5, sha256(b"hello"))];
    assert!(verify_existing(&path, &with_compat).unwrap());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn target_path_joins_folder_and_name() {
    let spec = file_spec("vae/sub", "model.safetensors", b"x");
    assert_eq!(target_path(Path::new("/tmp/models"), &spec), PathBuf::from("/tmp/models/vae/sub/model.safetensors"));
}

#[test]
fn missing_files_lists_only_absent_files() {
    let dir = temp_dir("missing");
    let present = put_file(&dir, "loras", "a.safetensors", b"one");
    let absent = file_spec("loras", "b.safetensors", b"two");
    let p = preset("missing-test", vec![present, absent]);

    let missing = missing_files(&dir, &p);
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].name, "b.safetensors");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn removal_plan_keeps_shared_files_and_sums_bytes() {
    let dir = temp_dir("plan");
    let a = put_file(&dir, "diffusion_models", "a.safetensors", b"aaaa");
    let b = put_file(&dir, "diffusion_models", "b.safetensors", b"bbbb");
    let te = put_file(&dir, "text_encoders", "te.safetensors", b"teee");

    let pa = preset("plan-a", vec![a, te.clone()]);
    let pb = preset("plan-b", vec![b, te]);
    let all = vec![pa.clone(), pb];

    let plan = removal_plan(&dir, &pa, &all);
    assert_eq!(plan.shared, vec!["te.safetensors".to_owned()]);
    assert_eq!(plan.remove.len(), 1);
    assert_eq!(plan.remove[0].0, dir.join("diffusion_models/a.safetensors"));
    assert_eq!(plan.remove[0].1, 4);
    assert_eq!(plan.bytes(), 4);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn remove_preset_removes_only_matching_own_files() {
    let dir = temp_dir("remove-own");
    let a = put_file(&dir, "diffusion_models", "a.safetensors", b"aaaa");
    let lora = put_file(&dir, "loras", "helper.safetensors", b"expected lora");
    let te = put_file(&dir, "text_encoders", "te.safetensors", b"shared te");

    let pa = preset("remove-a", vec![a, lora, te.clone()]);
    let pb = preset("remove-b", vec![te]);
    let all = vec![pa.clone(), pb];

    let disposed = RefCell::new(Vec::new());
    let r = remove_preset(&dir, &pa, &all, &|p| {
        disposed.borrow_mut().push(p.to_path_buf());
        std::fs::remove_file(p)
    })
    .unwrap();

    assert_eq!(r.removed, vec!["a.safetensors".to_owned(), "helper.safetensors".to_owned()]);
    assert_eq!(r.freed, (b"aaaa".len() + b"expected lora".len()) as u64);
    assert_eq!(r.shared, vec!["te.safetensors".to_owned()]);
    assert_eq!(r.mismatched, Vec::<String>::new());
    assert_eq!(disposed.borrow().len(), 2);

    assert!(!dir.join("diffusion_models/a.safetensors").exists());
    assert!(!dir.join("loras/helper.safetensors").exists());
    assert!(dir.join("text_encoders/te.safetensors").exists());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn remove_preset_skips_files_with_mismatched_checksum() {
    let dir = temp_dir("remove-mismatch");
    let spec = file_spec("loras", "helper.safetensors", b"expected lora");
    std::fs::create_dir_all(dir.join("loras")).unwrap();
    std::fs::write(dir.join("loras/helper.safetensors"), vec![b'X'; spec.bytes as usize]).unwrap();

    let pa = preset("remove-mismatch", vec![spec]);
    let all = vec![pa.clone()];

    let disposed = RefCell::new(Vec::new());
    let r = remove_preset(&dir, &pa, &all, &|p| {
        disposed.borrow_mut().push(p.to_path_buf());
        std::fs::remove_file(p)
    })
    .unwrap();

    assert_eq!(r.removed, Vec::<String>::new());
    assert_eq!(r.freed, 0);
    assert_eq!(r.shared, Vec::<String>::new());
    assert_eq!(r.mismatched, vec!["helper.safetensors".to_owned()]);
    assert!(disposed.borrow().is_empty());
    assert!(dir.join("loras/helper.safetensors").exists());

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
fn listed_files_stay_inside_model_folder() {
    let dir = temp_dir("listed");
    std::fs::create_dir_all(dir.join("loras/sub")).unwrap();
    std::fs::write(dir.join("loras/sub/style.safetensors"), b"1234").unwrap();
    std::fs::write(dir.join("secret.txt"), b"x").unwrap();

    assert!(listed_file(&dir, &["loras"], "../secret.txt").is_none());
    assert!(listed_file(&dir, &["loras"], "../../secret.txt").is_none());
    assert!(listed_file(&dir, &["loras"], "/etc/passwd").is_none());
    assert!(listed_file(&dir, &["checkpoints"], "sub/style.safetensors").is_none());

    let found = listed_file(&dir, &["checkpoints", "loras"], "sub/style.safetensors").unwrap();
    assert_eq!(found, dir.join("loras/sub/style.safetensors"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn remove_listed_file_disposes_first_existing_and_returns_bytes() {
    let dir = temp_dir("remove-listed");
    std::fs::create_dir_all(dir.join("loras/sub")).unwrap();
    std::fs::write(dir.join("loras/sub/style.safetensors"), b"1234567").unwrap();

    let disposed = RefCell::new(Vec::new());
    let bytes = remove_listed_file(&dir, &["checkpoints", "loras"], "sub/style.safetensors", &|p| {
        disposed.borrow_mut().push(p.to_path_buf());
        std::fs::remove_file(p)
    })
    .unwrap();

    assert_eq!(bytes, 7);
    assert_eq!(disposed.borrow().len(), 1);
    assert!(!dir.join("loras/sub/style.safetensors").exists());

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
