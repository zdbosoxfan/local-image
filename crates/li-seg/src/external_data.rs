//! ONNX models that keep their weights in separate files (ONNX external data, such as
//! `model.onnx_data` next to `model.onnx`). A custom model is copied into the models folder as one
//! self-contained file, so it keeps working once the picked folder is gone.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, ensure};
use prost::Message;
use tract_onnx::pb;
use tract_onnx::prelude::Framework;

/// The protobuf format's limit on one message, so on one ONNX file.
const MAX_MODEL_BYTES: u64 = (2 << 30) - 1;

/// `pb::tensor_proto::DataLocation::External`.
const EXTERNAL: i32 = 1;

struct WeightsBoundary {
    dir: PathBuf,
    blobs: Option<PathBuf>,
}

impl WeightsBoundary {
    fn new(dir: &Path) -> Result<Self> {
        let dir = dir.canonicalize().context("Could not read the model's folder")?;
        // Only the canonical snapshot layout grants access to a repository's blobs.
        // Do not canonicalize the blob root: a symlink there must not widen the boundary.
        let blobs = dir.ancestors().find_map(|revision| {
            let snapshots = revision.parent()?;
            if snapshots.file_name()? != "snapshots" {
                return None;
            }
            let repo = snapshots.parent()?;
            let mut parts = repo.file_name()?.to_str()?.strip_prefix("models--")?.split("--");
            let (Some(org), Some(name), None) = (parts.next(), parts.next(), parts.next()) else { return None };
            if org.is_empty() || name.is_empty() {
                return None;
            }
            Some(repo.join("blobs"))
        });
        Ok(Self { dir, blobs })
    }

    fn contains(&self, path: &Path) -> bool {
        path.starts_with(&self.dir) || self.blobs.as_ref().is_some_and(|blobs| path.starts_with(blobs))
    }
}

struct CheckedWeights<'a> {
    path: PathBuf,
    file: std::fs::File,
    boundary: &'a WeightsBoundary,
}

impl<'a> CheckedWeights<'a> {
    fn new(dir: &Path, boundary: &'a mut Option<WeightsBoundary>, location: &str) -> Result<Self> {
        // Reject Windows path syntax on every host, before even resolving the model folder.
        let relative = !location.starts_with('\\')
            && location.as_bytes().get(1) != Some(&b':')
            && !location.split(['/', '\\']).any(|part| part == "..")
            && Path::new(location).components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
        if !relative {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("The model's weights file {location:?} must be inside the model's folder (a relative path without '..' or a prefix)"),
            )
            .into());
        }
        if boundary.is_none() {
            *boundary = Some(WeightsBoundary::new(dir)?);
        }
        let boundary = boundary.as_ref().expect("the external tensor initialized the model boundary");
        let path = boundary.dir.join(location).canonicalize().with_context(|| format!("The model's weights file {location:?} is missing"))?;
        ensure!(
            boundary.contains(&path),
            "The model's weights file {location:?} points to {}; it must be inside the model's folder or the same Hugging Face repository's blobs folder",
            path.display()
        );
        let file = std::fs::File::open(&path).with_context(|| format!("The model's weights file {} is missing", path.display()))?;
        Ok(Self { path, file, boundary })
    }

    fn open(self) -> Result<std::fs::File> {
        #[cfg(target_os = "linux")]
        let opened_path = {
            use std::os::fd::AsRawFd;
            std::fs::read_link(format!("/proc/self/fd/{}", self.file.as_raw_fd())).context("Could not resolve the opened model weights")?
        };
        #[cfg(target_os = "macos")]
        let opened_path = {
            use std::os::unix::ffi::OsStrExt;
            let path = rustix::fs::getpath(&self.file).context("Could not resolve the opened model weights")?;
            PathBuf::from(std::ffi::OsStr::from_bytes(path.as_bytes()))
        };
        // Experimental Windows rechecks the path, but a swap between open and this lookup
        // can still evade it. Linux/macOS check the descriptor's path without resolving it again.
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let opened_path = self.path.canonicalize()?;
        ensure!(
            self.boundary.contains(&opened_path),
            "The opened model's weights file points to {}; it must be inside the model's folder or the same Hugging Face repository's blobs folder",
            opened_path.display()
        );
        let current = self.path.canonicalize()?;
        ensure!(
            current == self.path,
            "The model's weights file {} changed while it was being opened; it now points to {}",
            self.path.display(),
            current.display()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let opened = self.file.metadata()?;
            let current = std::fs::metadata(&self.path)?;
            ensure!(
                current.dev() == opened.dev() && current.ino() == opened.ino(),
                "The model's weights file {} changed while it was being opened",
                self.path.display()
            );
        }
        Ok(self.file)
    }
}

/// The loaded model with every external tensor read into the file itself. Self-contained models
/// retain their original bytes, so size checks, validation and copying never reopen the source.
pub(crate) fn inlined(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(MAX_MODEL_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_MODEL_BYTES, "This model doesn't fit in one ONNX file (2 GB at most)");
    let mut proto = tract_onnx::onnx()
        .proto_model_for_read(&mut std::io::Cursor::new(&bytes))
        .with_context(|| format!("Could not read {} as an ONNX model", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new(".")).join(".");
    let mut boundary = None;
    let mut read = 0u64;
    let Some(graph) = proto.graph.as_mut() else { return Ok(bytes) };
    if !inline_graph(graph, &dir, &mut boundary, &mut read)? {
        return Ok(bytes);
    }
    let len = proto.encoded_len() as u64;
    ensure!(len <= MAX_MODEL_BYTES, "This model's weights ({} MB) don't fit in one ONNX file (2 GB at most)", len >> 20);
    Ok(proto.encode_to_vec())
}

/// Inlines the external tensors of `graph` and of the graphs inside its nodes. Returns whether
/// there were any.
fn inline_graph(graph: &mut pb::GraphProto, dir: &Path, boundary: &mut Option<WeightsBoundary>, read: &mut u64) -> Result<bool> {
    let mut found = false;
    for t in &mut graph.initializer {
        found |= inline_tensor(t, dir, boundary, read)?;
    }
    for node in &mut graph.node {
        for a in &mut node.attribute {
            for t in a.t.iter_mut().chain(&mut a.tensors) {
                found |= inline_tensor(t, dir, boundary, read)?;
            }
            for g in a.g.iter_mut().chain(&mut a.graphs) {
                found |= inline_graph(g, dir, boundary, read)?;
            }
        }
    }
    Ok(found)
}

fn inline_tensor(t: &mut pb::TensorProto, dir: &Path, boundary: &mut Option<WeightsBoundary>, read: &mut u64) -> Result<bool> {
    if t.data_location != Some(EXTERNAL) {
        return Ok(false);
    }
    let entry = |key: &str| t.external_data.iter().find(|e| e.key == key).map(|e| e.value.as_str());
    let location = entry("location").with_context(|| format!("The weights of {} have no file name", t.name))?;
    let offset: u64 = entry("offset").map(str::parse).transpose().context("invalid external data offset")?.unwrap_or(0);
    let length: Option<u64> = entry("length").map(str::parse).transpose().context("invalid external data length")?;
    let checked = CheckedWeights::new(dir, boundary, location)?;
    let file = checked.path.clone();
    let mut f = checked.open()?;
    let available = f.metadata()?.len().checked_sub(offset).with_context(|| format!("{} is shorter than the model says", file.display()))?;
    let length = length.unwrap_or(available);
    ensure!(length <= available, "{} is shorter than the model says", file.display());
    *read = read.saturating_add(length);
    ensure!(*read <= MAX_MODEL_BYTES, "This model's weights don't fit in one ONNX file (2 GB at most)");
    f.seek(SeekFrom::Start(offset))?;
    let mut data = Vec::with_capacity(length as usize);
    f.take(length).read_to_end(&mut data)?;
    t.raw_data = data;
    t.external_data.clear();
    t.data_location = None;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_locations_are_refused_before_filesystem_lookup() {
        let missing = std::env::temp_dir().join(format!("li-seg-missing-folder-{}", std::process::id()));
        assert!(!missing.exists());
        for location in [
            "/missing/weights",
            "//attacker.invalid/share/weights",
            r"\\attacker.invalid\share\weights",
            r"C:\weights",
            r"C:weights",
            r"\\?\C:\weights",
            r"\\.\device",
            "../weights",
            r"data\..\weights",
        ] {
            let error = CheckedWeights::new(&missing, &mut None, location).err().expect("invalid location is refused");
            assert_eq!(error.downcast_ref::<std::io::Error>().map(std::io::Error::kind), Some(std::io::ErrorKind::InvalidInput), "{location}: {error:#}");
            assert!(error.to_string().contains("must be inside the model's folder"), "{location}: {error:#}");
        }
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn opened_weights_outside_the_boundary_are_refused() {
        let dir = std::env::temp_dir().join(format!("li-seg-descriptor-boundary-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("picked")).unwrap();
        std::fs::write(dir.join("picked/weights"), b"safe").unwrap();
        std::fs::write(dir.join("outside"), b"evil").unwrap();
        let mut boundary = None;
        let mut checked = CheckedWeights::new(&dir.join("picked"), &mut boundary, "weights").unwrap();
        // Supply the descriptor an open racing with a directory swap could return.
        checked.file = std::fs::File::open(dir.join("outside")).unwrap();
        let result = checked.open();
        std::fs::remove_dir_all(dir).unwrap();
        let error = result.unwrap_err();
        assert!(format!("{error:#}").contains("must be inside the model's folder"), "{error:#}");
    }

    #[test]
    #[cfg(unix)]
    fn weights_swapped_after_the_path_check_are_refused() {
        let dir = std::env::temp_dir().join(format!("li-seg-weights-swap-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("picked/data")).unwrap();
        std::fs::create_dir_all(dir.join("outside")).unwrap();
        let folder = dir.join("picked").canonicalize().unwrap();
        let path = folder.join("data/weights");
        std::fs::write(&path, b"safe").unwrap();
        std::fs::write(dir.join("outside/weights"), b"evil").unwrap();
        let mut boundary = None;
        let checked = CheckedWeights::new(&folder, &mut boundary, "data/weights").unwrap();
        std::fs::rename(folder.join("data"), folder.join("original")).unwrap();
        std::os::unix::fs::symlink(dir.join("outside"), folder.join("data")).unwrap();
        let result = checked.open();
        std::fs::remove_file(folder.join("data")).unwrap();
        std::fs::rename(folder.join("original"), folder.join("data")).unwrap();
        let error = result.unwrap_err();
        assert!(format!("{error:#}").contains("changed while"), "{error:#}");

        let checked = CheckedWeights::new(&folder, &mut boundary, "data/weights").unwrap();
        std::fs::rename(&path, folder.join("original-weights")).unwrap();
        std::fs::write(&path, b"evil").unwrap();
        let result = checked.open();
        std::fs::remove_dir_all(dir).unwrap();
        let error = result.unwrap_err();
        assert!(format!("{error:#}").contains("changed while"), "{error:#}");
    }
}
