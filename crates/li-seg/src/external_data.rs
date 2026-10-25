//! ONNX models that keep their weights in separate files (ONNX external data, such as
//! `model.onnx_data` next to `model.onnx`). A custom model is copied into the models folder as one
//! self-contained file, so it keeps working once the picked folder is gone.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path};

use anyhow::{Context, Result, bail, ensure};
use prost::Message;
use tract_onnx::pb;
use tract_onnx::prelude::Framework;

/// The protobuf format's limit on one message, so on one ONNX file.
const MAX_MODEL_BYTES: u64 = (2 << 30) - 1;

/// `pb::tensor_proto::DataLocation::External`.
const EXTERNAL: i32 = 1;

/// The model at `path` with every external tensor read into the file itself; `None` when it has
/// none (a plain copy will do).
pub(crate) fn inlined(path: &Path) -> Result<Option<Vec<u8>>> {
    let mut proto = tract_onnx::onnx().proto_model_for_path(path).with_context(|| format!("Could not read {} as an ONNX model", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut read = 0u64;
    let Some(graph) = proto.graph.as_mut() else { return Ok(None) };
    if !inline_graph(graph, dir, &mut read)? {
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
    // The location comes from the model file: it must stay inside the model's folder.
    if Path::new(location).components().any(|c| !matches!(c, Component::Normal(_) | Component::CurDir)) {
        bail!("The model's weights file {location:?} must be inside the model's folder");
    }
    let offset: u64 = entry("offset").map(str::parse).transpose().context("invalid external data offset")?.unwrap_or(0);
    let length: Option<u64> = entry("length").map(str::parse).transpose().context("invalid external data length")?;
    let file = dir.join(location);
    let mut f = std::fs::File::open(&file).with_context(|| format!("The model's weights file {} is missing", file.display()))?;
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
