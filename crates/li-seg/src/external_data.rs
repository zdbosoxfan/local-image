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

struct CheckedWeights {
    path: PathBuf,
    #[cfg(unix)]
    metadata: std::fs::Metadata,
}

impl CheckedWeights {
    fn new(dir: &Path, location: &str) -> Result<Self> {
        let dir = dir.canonicalize().context("Could not read the model's folder")?;
        let path = dir.join(location).canonicalize().with_context(|| format!("The model's weights file {location:?} is missing"))?;
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
        let relative = Path::new(location).components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
        ensure!(
            relative && (path.starts_with(&dir) || blobs.is_some_and(|blobs| path.starts_with(blobs))),
            "The model's weights file {location:?} points to {}; it must be inside the model's folder or the same Hugging Face repository's blobs folder",
            path.display()
        );
        #[cfg(unix)]
        let metadata = std::fs::metadata(&path)?;
        Ok(Self {
            path,
            #[cfg(unix)]
            metadata,
        })
    }

    fn open(&self) -> Result<std::fs::File> {
        let file = std::fs::File::open(&self.path).with_context(|| format!("The model's weights file {} is missing", self.path.display()))?;
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
            let opened = file.metadata()?;
            let current = std::fs::metadata(&self.path)?;
            ensure!(
                opened.dev() == self.metadata.dev() && opened.ino() == self.metadata.ino() && current.dev() == opened.dev() && current.ino() == opened.ino(),
                "The model's weights file {} changed while it was being opened",
                self.path.display()
            );
        }
        Ok(file)
    }
}

/// The model at `path` with every external tensor read into the file itself; `None` when it has
/// none (a plain copy will do).
pub(crate) fn inlined(path: &Path) -> Result<Option<Vec<u8>>> {
    let mut proto = tract_onnx::onnx().proto_model_for_path(path).with_context(|| format!("Could not read {} as an ONNX model", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new(".")).join(".");
    let mut read = 0u64;
    let Some(graph) = proto.graph.as_mut() else { return Ok(None) };
    if !inline_graph(graph, &dir, &mut read)? {
        return Ok(None);
    }
    let len = proto.encoded_len() as u64;
    ensure!(len <= MAX_MODEL_BYTES, "This model's weights ({} MB) don't fit in one ONNX file (2 GB at most)", len >> 20);
    Ok(Some(proto.encode_to_vec()))
}

/// Inlines the external tensors of `graph` and of the graphs inside its nodes. Returns whether
/// there were any.
fn inline_graph(graph: &mut pb::GraphProto, dir: &Path, read: &mut u64) -> Result<bool> {
    let mut found = false;
    for t in &mut graph.initializer {
        found |= inline_tensor(t, dir, read)?;
    }
    for node in &mut graph.node {
        for a in &mut node.attribute {
            for t in a.t.iter_mut().chain(&mut a.tensors) {
                found |= inline_tensor(t, dir, read)?;
            }
            for g in a.g.iter_mut().chain(&mut a.graphs) {
                found |= inline_graph(g, dir, read)?;
            }
        }
    }
    Ok(found)
}

fn inline_tensor(t: &mut pb::TensorProto, dir: &Path, read: &mut u64) -> Result<bool> {
    if t.data_location != Some(EXTERNAL) {
        return Ok(false);
    }
    let entry = |key: &str| t.external_data.iter().find(|e| e.key == key).map(|e| e.value.as_str());
    let location = entry("location").with_context(|| format!("The weights of {} have no file name", t.name))?;
    let offset: u64 = entry("offset").map(str::parse).transpose().context("invalid external data offset")?.unwrap_or(0);
    let length: Option<u64> = entry("length").map(str::parse).transpose().context("invalid external data length")?;
    let checked = CheckedWeights::new(dir, location)?;
    let file = &checked.path;
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn weights_swapped_after_the_path_check_are_refused() {
        let dir = std::env::temp_dir().join(format!("li-seg-weights-swap-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("picked/data")).unwrap();
        std::fs::create_dir_all(dir.join("outside")).unwrap();
        let folder = dir.join("picked").canonicalize().unwrap();
        let path = folder.join("data/weights");
        std::fs::write(&path, b"safe").unwrap();
        std::fs::write(dir.join("outside/weights"), b"evil").unwrap();
        let checked = CheckedWeights::new(&folder, "data/weights").unwrap();
        std::fs::rename(folder.join("data"), folder.join("original")).unwrap();
        std::os::unix::fs::symlink(dir.join("outside"), folder.join("data")).unwrap();
        let result = checked.open();
        std::fs::remove_file(folder.join("data")).unwrap();
        std::fs::rename(folder.join("original"), folder.join("data")).unwrap();
        let error = result.unwrap_err();
        assert!(format!("{error:#}").contains("changed while"), "{error:#}");

        let checked = CheckedWeights::new(&folder, "data/weights").unwrap();
        std::fs::rename(&path, folder.join("original-weights")).unwrap();
        std::fs::write(&path, b"evil").unwrap();
        let result = checked.open();
        std::fs::remove_dir_all(dir).unwrap();
        let error = result.unwrap_err();
        assert!(format!("{error:#}").contains("changed while"), "{error:#}");
    }
}
