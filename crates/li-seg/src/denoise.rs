//! AI Denoise: the RawNIND UtNet2 denoiser (linear Rec.2020 variant, `model_linear.onnx` from
//! darktable-ai's `rawdenoise-nind` package) run on the CPU with `tract`, tile by tile.
//!
//! The model takes linear Rec.2020 (camera white balance applied, white ≈ 1) as NCHW
//! `[1, 3, H, W]` with H and W divisible by 16 and returns the denoised image at an arbitrary
//! learned gain. The graph is exported with a static 512 × 512 input; it is fully convolutional, so
//! we declare a 256 × 256 input instead (smaller tiles, same result) and fall back to 512 when that
//! doesn't load.
//!
//! The tiling and gain matching follow darktable-ai's `models/rawdenoise-nind/demo.py`
//! (GPL-3.0; mirror-padded edges, overlapping tiles, one scalar gain matching the output's mean to
//! the input's), with two changes: tiles are blended with feathered weights instead of keeping
//! only each tile's core, and the gain is matched once over the whole image (a per-tile gain goes
//! wild — even negative — on dark, flat tiles). The model: Brummer & De Vleeschouwer, "Learning
//! Joint Denoising, Demosaicing, and Compression from the Raw Natural Image Noise Dataset" (2025),
//! <https://github.com/trougnouf/rawnind_jddc>, GPL-3.0.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use tract_onnx::prelude::*;

/// Preferred tile side (px).
pub const TILE: usize = 256;
/// Tile side used when the model does not load at [`TILE`] (the graph's own input size).
pub const FALLBACK_TILE: usize = 512;
/// Overlap between neighbouring tiles (px).
pub const OVERLAP: usize = 32;
/// Pixels at a tile's inner edge that get no weight at all (the network's border is the least
/// reliable part of a tile).
pub const MARGIN: usize = 8;

/// The run was cancelled through the progress callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cancelled")
    }
}

impl std::error::Error for Cancelled {}

type Runner = dyn Fn(Tensor) -> TractResult<TVec<TValue>> + Send + Sync;

/// A loaded denoiser (cheap to clone; tiles can run on several threads at once).
#[derive(Clone)]
pub struct Denoiser {
    tile: usize,
    run: Arc<Runner>,
}

impl std::fmt::Debug for Denoiser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Denoiser").field("tile", &self.tile).finish()
    }
}

impl Denoiser {
    /// Load `model_linear.onnx` at [`TILE`], else at [`FALLBACK_TILE`].
    pub fn load(path: &Path) -> Result<Self> {
        match Self::load_with_tile(path, TILE) {
            Ok(d) => Ok(d),
            Err(e) => {
                log::info!("AI Denoise: the model does not run at {TILE} px tiles ({e:#}); using {FALLBACK_TILE}");
                Self::load_with_tile(path, FALLBACK_TILE)
            }
        }
    }

    /// Load the model with a `tile × tile` input (a multiple of 16).
    pub fn load_with_tile(path: &Path, tile: usize) -> Result<Self> {
        let onnx = tract_onnx::onnx();
        let mut proto = onnx.proto_model_for_path(path).with_context(|| format!("Could not read the denoise model {}", path.display()))?;
        // The export bakes 512 × 512 into the graph's input, output and intermediate shapes: drop
        // them so every shape follows the input fact.
        if let Some(g) = proto.graph.as_mut() {
            g.value_info.clear();
            for v in g.output.iter_mut() {
                v.r#type = None;
            }
        }
        let model = onnx.model_for_proto_model(&proto).with_context(|| format!("Could not read the denoise model {}", path.display()))?;
        Self::from_model(model, tile)
    }

    /// A denoiser from an already parsed model (tests build small ones in memory).
    pub fn from_model(model: InferenceModel, tile: usize) -> Result<Self> {
        if tile == 0 || tile % 16 != 0 {
            bail!("the tile size must be a multiple of 16");
        }
        let plan = model
            .with_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec!(1, 3, tile, tile)))?
            .into_optimized()?
            .into_runnable()?;
        let d = Denoiser { tile, run: Arc::new(move |t: Tensor| plan.run(tvec!(t.into()))) };
        // a shape the graph doesn't really support shows up here, not halfway through a photo
        let out = d.run_tile(&vec![0.1; 3 * tile * tile])?;
        if out.len() != 3 * tile * tile {
            bail!("the model returned {} values for a {tile} px tile", out.len());
        }
        Ok(d)
    }

    pub fn tile(&self) -> usize {
        self.tile
    }

    /// One CHW tile (`3 × tile × tile` values) through the model.
    fn run_tile(&self, chw: &[f32]) -> Result<Vec<f32>> {
        let t = self.tile;
        let input = tract_ndarray::Array4::from_shape_vec((1, 3, t, t), chw.to_vec())?;
        let out = (self.run)(input.into())?;
        let v = out.first().context("the model returned nothing")?.to_plain_array_view::<f32>()?;
        Ok(v.iter().copied().collect())
    }

    /// Denoise a `w × h` linear Rec.2020 image. `progress(done, total)` is called after every
    /// tile; returning false cancels (the error is then [`Cancelled`]).
    pub fn denoise(&self, rgb: &[[f32; 3]], w: usize, h: usize, progress: &(dyn Fn(usize, usize) -> bool + Sync)) -> Result<Vec<[f32; 3]>> {
        let mut out = run_tiled(rgb, w, h, self.tile, OVERLAP.min(self.tile / 4), |t| self.run_tile(t), progress)?;
        match_gain(&mut out, mean(rgb));
        Ok(out)
    }
}

/// Mirror index: `i` reflected into `0..n` (any distance outside, `n ≥ 1`).
fn mirror(i: isize, n: usize) -> usize {
    if n == 1 {
        return 0;
    }
    let p = 2 * (n as isize - 1);
    let m = i.rem_euclid(p);
    (if m < n as isize { m } else { p - m }) as usize
}

/// Tile origins along an axis of `n` px (`n ≥ tile`): every `tile − overlap` px, the last one
/// ending exactly at `n`.
pub fn tile_starts(n: usize, tile: usize, overlap: usize) -> Vec<usize> {
    if n <= tile {
        return vec![0];
    }
    let step = tile - overlap;
    let mut v: Vec<usize> = (0..).map(|i| i * step).take_while(|s| s + tile < n).collect();
    v.push(n - tile);
    v.dedup();
    v
}

fn smoothstep(x: f32) -> f32 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Weight of position `i` (0..tile) in a tile: 0 over the first [`MARGIN`] px of an inner edge,
/// rising smoothly to 1 at `overlap`; 1 along the image's own border (`lead` / `trail`: whether a
/// neighbouring tile overlaps that side).
pub fn axis_weight(i: usize, tile: usize, overlap: usize, lead: bool, trail: bool) -> f32 {
    let margin = MARGIN.min(overlap / 4);
    let ramp = |d: usize| if d < margin { 0.0 } else { smoothstep(((d - margin) as f32 + 0.5) / (overlap - margin) as f32) };
    let mut w = 1.0;
    if lead && i < overlap {
        w *= ramp(i);
    }
    if trail && tile - 1 - i < overlap {
        w *= ramp(tile - 1 - i);
    }
    w
}

/// Per-axis sum of the weights of every tile covering each position (the blend's normaliser; the
/// 2-D weight is the product of the axes', so its sum is the product of these).
fn weight_sums(n: usize, starts: &[usize], tile: usize, overlap: usize) -> Vec<f32> {
    let mut s = vec![0.0f32; n];
    for (k, &o) in starts.iter().enumerate() {
        let (lead, trail) = (k > 0, k + 1 < starts.len());
        for i in 0..tile.min(n - o) {
            s[o + i] += axis_weight(i, tile, overlap, lead, trail);
        }
    }
    s
}

/// Run `model` (one CHW `3 × tile × tile` tile in, the same out) over a `w × h` image in
/// overlapping tiles, mirror-padding where the image is smaller than a tile, and blend the tiles
/// with feathered weights. Tiles of a row run in parallel. No gain matching (see [`match_gain`]).
pub fn run_tiled(
    rgb: &[[f32; 3]],
    w: usize,
    h: usize,
    tile: usize,
    overlap: usize,
    model: impl Fn(&[f32]) -> Result<Vec<f32>> + Sync,
    progress: &(dyn Fn(usize, usize) -> bool + Sync),
) -> Result<Vec<[f32; 3]>> {
    if w == 0 || h == 0 || rgb.len() != w * h {
        bail!("image buffer size mismatch");
    }
    if overlap * 2 >= tile {
        bail!("the overlap must be less than half a tile");
    }
    // the padded frame the tiles cover (at least one tile in each direction)
    let (pw, ph) = (w.max(tile), h.max(tile));
    let (xs, ys) = (tile_starts(pw, tile, overlap), tile_starts(ph, tile, overlap));
    let (sx, sy) = (weight_sums(pw, &xs, tile, overlap), weight_sums(ph, &ys, tile, overlap));
    let total = xs.len() * ys.len();
    let done = std::sync::atomic::AtomicUsize::new(0);
    let mut out = vec![[0.0f32; 3]; w * h];
    let plane = tile * tile;
    for (ky, &oy) in ys.iter().enumerate() {
        let tiles: Vec<Vec<f32>> = xs
            .par_iter()
            .map(|&ox| {
                let mut chw = vec![0.0f32; 3 * plane];
                for y in 0..tile {
                    let sy = mirror((oy + y) as isize, h);
                    for x in 0..tile {
                        let p = rgb[sy * w + mirror((ox + x) as isize, w)];
                        for c in 0..3 {
                            chw[c * plane + y * tile + x] = p[c].max(0.0);
                        }
                    }
                }
                let r = model(&chw)?;
                if r.len() != 3 * plane {
                    bail!("the model returned {} values for a {tile} px tile", r.len());
                }
                let n = done.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                if !progress(n, total) {
                    return Err(anyhow::Error::new(Cancelled));
                }
                Ok(r)
            })
            .collect::<Result<_>>()?;
        let (lead_y, trail_y) = (ky > 0, ky + 1 < ys.len());
        for (kx, (&ox, t)) in xs.iter().zip(&tiles).enumerate() {
            let (lead_x, trail_x) = (kx > 0, kx + 1 < xs.len());
            for y in 0..tile.min(h.saturating_sub(oy)) {
                let wy = axis_weight(y, tile, overlap, lead_y, trail_y);
                if wy == 0.0 {
                    continue;
                }
                let row = (oy + y) * w;
                for x in 0..tile.min(w.saturating_sub(ox)) {
                    let wgt = wy * axis_weight(x, tile, overlap, lead_x, trail_x);
                    if wgt == 0.0 {
                        continue;
                    }
                    let o = &mut out[row + ox + x];
                    for c in 0..3 {
                        o[c] += wgt * t[c * plane + y * tile + x];
                    }
                }
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            let n = sx[x] * sy[y];
            let o = &mut out[y * w + x];
            for c in o.iter_mut() {
                *c /= n.max(1e-12);
            }
        }
    }
    Ok(out)
}

/// Mean of all samples (negatives count as zero, as the model sees them).
pub fn mean(rgb: &[[f32; 3]]) -> f64 {
    if rgb.is_empty() {
        return 0.0;
    }
    rgb.iter().map(|p| p.iter().map(|v| v.max(0.0) as f64).sum::<f64>()).sum::<f64>() / (rgb.len() * 3) as f64
}

/// Scale `out` so its mean is `target` (one gain for the whole image; sign-preserving, as
/// upstream: an output at a negative learned gain comes back positive), then clip negatives. A
/// degenerate (zero-mean) output is left at its own scale.
pub fn match_gain(out: &mut [[f32; 3]], target: f64) {
    let m = out.iter().map(|p| p.iter().map(|v| *v as f64).sum::<f64>()).sum::<f64>() / (out.len().max(1) * 3) as f64;
    let g = if m.abs() < 1e-12 || !m.is_finite() { 1.0 } else { (target / m) as f32 };
    for p in out.iter_mut() {
        for c in p.iter_mut() {
            *c = (*c * g).max(0.0);
        }
    }
}

// ------------------------------------------------------------------------------ install

/// Read entry `name` from a zip archive (stored or deflated), at most `limit` bytes.
pub fn zip_entry(zip: &[u8], name: &str, limit: usize) -> Result<Vec<u8>> {
    let u16_at = |o: usize| -> Result<usize> { zip.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).context("truncated zip") };
    let u32_at = |o: usize| -> Result<usize> { zip.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).context("truncated zip") };
    // end of central directory: the last `PK\5\6` within the trailing comment's reach
    let lo = zip.len().saturating_sub(22 + 65535);
    let eocd = (lo..zip.len().saturating_sub(21)).rev().find(|&i| zip[i..i + 4] == [0x50, 0x4b, 0x05, 0x06]).context("not a zip archive")?;
    let (count, mut p) = (u16_at(eocd + 10)?, u32_at(eocd + 16)?);
    for _ in 0..count {
        if u32_at(p)? != 0x0201_4b50 {
            bail!("damaged zip directory");
        }
        let (method, csize, usize_, nlen, xlen, clen, local) =
            (u16_at(p + 10)?, u32_at(p + 20)?, u32_at(p + 24)?, u16_at(p + 28)?, u16_at(p + 30)?, u16_at(p + 32)?, u32_at(p + 42)?);
        let entry = zip.get(p + 46..p + 46 + nlen).context("truncated zip")?;
        p += 46 + nlen + xlen + clen;
        if entry != name.as_bytes() {
            continue;
        }
        if usize_ > limit {
            bail!("{name} is larger than expected");
        }
        if u32_at(local)? != 0x0403_4b50 {
            bail!("damaged zip entry {name}");
        }
        let start = local + 30 + u16_at(local + 26)? + u16_at(local + 28)?;
        let data = zip.get(start..start + csize).context("truncated zip")?;
        return match method {
            0 => Ok(data.to_vec()),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, limit).map_err(|e| anyhow::anyhow!("could not unpack {name}: {e:?}")),
            m => bail!("{name} uses an unsupported zip compression ({m})"),
        };
    }
    bail!("{name} is not in the package")
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Install a downloaded model package: extract the model `spec` names (a
/// [`Task::Denoise`](crate::Task::Denoise) entry), check its size and SHA-256, and write it to
/// `dest` (through a temporary file, so `dest` is either complete or absent). Leaves `pkg` alone.
pub fn install_package(pkg: &Path, spec: &crate::ModelSpec, dest: &Path) -> Result<()> {
    let crate::Task::Denoise { inner, inner_bytes, inner_sha256 } = spec.task else { bail!("{} is not a packaged model", spec.id) };
    let zip = std::fs::read(pkg).with_context(|| format!("Could not read {}", pkg.display()))?;
    let bytes = zip_entry(&zip, inner, inner_bytes as usize)?;
    if bytes.len() as u64 != inner_bytes || sha256_hex(&bytes) != inner_sha256 {
        bail!("The model in the package does not match the expected file (size or checksum).");
    }
    let dir = dest.parent().context("destination has no folder")?;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.part", dest.file_name().and_then(|n| n.to_str()).unwrap_or("model")));
    let r = (|| -> Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, dest)?;
        Ok(())
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

/// The installed denoise model, when there is one (`<models>/segmentation/<file>`).
pub fn installed(models_dir: &Path) -> Option<std::path::PathBuf> {
    let spec = crate::spec(DENOISE_ID)?;
    Some(crate::model_path(models_dir, spec)).filter(|p| p.is_file())
}

/// Id of the denoise model in [`crate::MODELS`].
pub const DENOISE_ID: &str = "rawdenoise-nind";

/// A process-wide cache of the loaded denoiser (loading takes a moment).
pub fn shared(models_dir: &Path) -> Result<Denoiser> {
    static SLOT: std::sync::Mutex<Option<(std::path::PathBuf, Denoiser)>> = std::sync::Mutex::new(None);
    let path = installed(models_dir).context("The AI Denoise model is not installed.")?;
    let mut g = SLOT.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((p, d)) = g.as_ref()
        && *p == path
    {
        return Ok(d.clone());
    }
    let d = Denoiser::load(&path)?;
    *g = Some((path, d.clone()));
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: usize, h: usize) -> Vec<[f32; 3]> {
        (0..w * h).map(|i| [((i % w) as f32 * 0.37).sin() * 0.2 + 0.3, ((i / w) as f32 * 0.21).cos() * 0.1 + 0.2, 0.05 + (i % 7) as f32 * 0.01]).collect()
    }

    fn all(_: usize, _: usize) -> bool {
        true
    }

    #[test]
    fn tiles_cover_the_axis_and_end_at_the_edge() {
        assert_eq!(tile_starts(256, 256, 32), vec![0]);
        assert_eq!(tile_starts(100, 256, 32), vec![0]);
        assert_eq!(tile_starts(500, 256, 32), vec![0, 224, 244]);
        for n in [257, 480, 481, 1000, 4000] {
            let s = tile_starts(n, 256, 32);
            assert_eq!(*s.last().unwrap() + 256, n);
            for p in s.windows(2) {
                assert!(p[1] > p[0] && p[0] + 256 >= p[1] + 32, "{n}: {s:?}");
            }
        }
    }

    #[test]
    fn blend_weights_never_vanish_and_feather_inner_edges() {
        for n in [256usize, 300, 700, 1023] {
            let s = tile_starts(n, 256, 32);
            let sums = weight_sums(n, &s, 256, 32);
            assert!(sums.iter().all(|v| *v > 0.5), "{n}");
        }
        // an inner edge starts at zero and ramps up; the image border keeps full weight
        assert_eq!(axis_weight(0, 256, 32, true, true), 0.0);
        assert!(axis_weight(20, 256, 32, true, true) > 0.0 && axis_weight(20, 256, 32, true, true) < 1.0);
        assert_eq!(axis_weight(128, 256, 32, true, true), 1.0);
        assert_eq!(axis_weight(0, 256, 32, false, true), 1.0);
        assert_eq!(axis_weight(255, 256, 32, true, false), 1.0);
    }

    #[test]
    fn identity_model_returns_the_image_exactly() {
        for (w, h) in [(300, 200), (64, 40), (600, 530)] {
            let src = img(w, h);
            let out = run_tiled(&src, w, h, 256, 32, |t| Ok(t.to_vec()), &all).unwrap();
            for (a, b) in src.iter().zip(&out) {
                for c in 0..3 {
                    assert!((a[c] - b[c]).abs() < 1e-5, "{w}×{h}: {a:?} {b:?}");
                }
            }
        }
    }

    #[test]
    fn gain_is_matched_once_over_the_image() {
        let (w, h) = (520, 300);
        let mut src = img(w, h);
        // a dark, flat corner where a per-tile mean would be ~0
        for y in 0..64 {
            for x in 0..64 {
                src[y * w + x] = [0.0; 3];
            }
        }
        for gain in [0.5f32, 3.0, -1.0] {
            let mut out = run_tiled(&src, w, h, 256, 32, |t| Ok(t.iter().map(|v| v * gain).collect()), &all).unwrap();
            match_gain(&mut out, mean(&src));
            for (a, b) in src.iter().zip(&out) {
                for c in 0..3 {
                    assert!((a[c] - b[c]).abs() < 1e-4, "gain {gain}: {a:?} {b:?}");
                }
            }
        }
        // a zero output stays put instead of blowing up
        let mut z = vec![[0.0f32; 3]; 4];
        match_gain(&mut z, 0.5);
        assert_eq!(z, vec![[0.0; 3]; 4]);
    }

    #[test]
    fn cancelling_stops_the_run() {
        let (w, h) = (700, 300);
        let src = img(w, h);
        let r = run_tiled(&src, w, h, 256, 32, |t| Ok(t.to_vec()), &|n, _| n < 2);
        assert!(r.unwrap_err().is::<Cancelled>());
    }

    /// A one-node ONNX graph (a 1×1 convolution that halves every channel), built in memory, run
    /// through the real tract path at a 32 px tile; gain matching brings it back.
    #[test]
    fn tiny_onnx_model_runs_through_tract() {
        use tract_onnx::pb::*;
        let tensor_f32 = |name: &str, dims: Vec<i64>, data: Vec<f32>| TensorProto { name: name.into(), dims, data_type: 1, float_data: data, ..Default::default() };
        let vi = |name: &str| ValueInfoProto {
            name: name.into(),
            r#type: Some(TypeProto {
                value: Some(type_proto::Value::TensorType(type_proto::Tensor {
                    elem_type: 1,
                    shape: Some(TensorShapeProto {
                        dim: [1i64, 3, 64, 64]
                            .iter()
                            .map(|d| tensor_shape_proto::Dimension { value: Some(tensor_shape_proto::dimension::Value::DimValue(*d)), ..Default::default() })
                            .collect(),
                    }),
                })),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut wts = vec![0.0f32; 9];
        for c in 0..3 {
            wts[c * 3 + c] = 0.5;
        }
        let graph = GraphProto {
            node: vec![NodeProto { input: vec!["input".into(), "w".into()], output: vec!["output".into()], op_type: "Conv".into(), ..Default::default() }],
            initializer: vec![tensor_f32("w", vec![3, 3, 1, 1], wts)],
            input: vec![vi("input")],
            output: vec![vi("output")],
            ..Default::default()
        };
        let proto = ModelProto {
            ir_version: 8,
            opset_import: vec![OperatorSetIdProto { domain: String::new(), version: 17 }],
            graph: Some(graph),
            ..Default::default()
        };
        let model = tract_onnx::onnx().model_for_proto_model(&proto).unwrap();
        let d = Denoiser::from_model(model, 32).unwrap();
        assert_eq!(d.tile(), 32);
        let (w, h) = (70, 45);
        let src = img(w, h);
        let out = d.denoise(&src, w, h, &all).unwrap();
        for (a, b) in src.iter().zip(&out) {
            for c in 0..3 {
                assert!((a[c] - b[c]).abs() < 1e-4, "{a:?} {b:?}");
            }
        }
    }

    fn zip_of(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data, deflate) in entries {
            let body = if *deflate { miniz_oxide::deflate::compress_to_vec(data, 6) } else { data.to_vec() };
            let off = out.len() as u32;
            let method: u16 = if *deflate { 8 } else { 0 };
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0]);
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&[0; 8]); // time, date, crc (not checked)
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&body);
            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&[20, 0, 20, 0, 0, 0]);
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&[0; 8]);
            central.extend_from_slice(&(body.len() as u32).to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&off.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let cd = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }

    #[test]
    fn zip_entries_unpack_and_the_package_is_checked() {
        let model: Vec<u8> = (0..5000u32).map(|i| (i * 7 % 251) as u8).collect();
        let zip = zip_of(&[("pkg/config.json", b"{}", false), ("pkg/model_linear.onnx", &model, true)]);
        assert_eq!(zip_entry(&zip, "pkg/config.json", 100).unwrap(), b"{}");
        assert_eq!(zip_entry(&zip, "pkg/model_linear.onnx", 10_000).unwrap(), model);
        assert!(zip_entry(&zip, "pkg/model_linear.onnx", 100).is_err(), "over the limit");
        assert!(zip_entry(&zip, "nope", 100).is_err());

        let dir = std::env::temp_dir().join(format!("li-seg-pkg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pkg = dir.join("p.dtmodel");
        std::fs::write(&pkg, &zip).unwrap();
        let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
        let good = crate::ModelSpec {
            task: crate::Task::Denoise { inner: "pkg/model_linear.onnx", inner_bytes: model.len() as u64, inner_sha256: leak(sha256_hex(&model)) },
            ..*crate::spec(DENOISE_ID).unwrap()
        };
        let dest = dir.join("segmentation").join("m.onnx");
        install_package(&pkg, &good, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), model);
        let bad = crate::ModelSpec { task: crate::Task::Denoise { inner: "pkg/model_linear.onnx", inner_bytes: model.len() as u64, inner_sha256: "00" }, ..good };
        let dest2 = dir.join("segmentation").join("m2.onnx");
        assert!(install_package(&pkg, &bad, &dest2).is_err());
        assert!(!dest2.exists(), "nothing is left behind");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The real model on a crop of a high-ISO photo: noise goes down, brightness stays. Run with
    /// `LI_DENOISE_MODEL=…/model_linear.onnx LI_DENOISE_IMAGE=…/noisy.jpg cargo test -p li-seg
    /// --lib -- --ignored real_model` (`LI_DENOISE_FULL=1` also times the whole image).
    #[test]
    #[ignore = "needs the RawNIND model (LI_DENOISE_MODEL) and a noisy photo (LI_DENOISE_IMAGE)"]
    fn real_model_reduces_noise() {
        let (Some(model), Some(photo)) = (std::env::var_os("LI_DENOISE_MODEL"), std::env::var_os("LI_DENOISE_IMAGE")) else { return };
        let t0 = std::time::Instant::now();
        if let Err(e) = Denoiser::load_with_tile(Path::new(&model), TILE) {
            eprintln!("{TILE} px tiles: {e:#}");
        }
        let d = Denoiser::load(Path::new(&model)).unwrap();
        eprintln!("loaded at {} px tiles in {:.1}s", d.tile(), t0.elapsed().as_secs_f64());
        let im = image::open(&photo).unwrap().to_rgb8();
        let lin = |v: u8| {
            let c = v as f32 / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        // sRGB → linear Rec.2020
        let m = [[0.6274, 0.3293, 0.0433], [0.0691, 0.9195, 0.0114], [0.0164, 0.0880, 0.8956]];
        let to2020 = |p: &image::Rgb<u8>| {
            let l = [lin(p[0]), lin(p[1]), lin(p[2])];
            [0, 1, 2].map(|r| m[r][0] * l[0] + m[r][1] * l[1] + m[r][2] * l[2])
        };
        let (cw, ch) = (512usize, 512usize);
        let (x0, y0) = (im.width() as usize / 2 - cw / 2, im.height() as usize / 2 - ch / 2);
        let crop: Vec<[f32; 3]> = (0..cw * ch).map(|i| to2020(im.get_pixel((x0 + i % cw) as u32, (y0 + i / cw) as u32))).collect();
        let t1 = std::time::Instant::now();
        let out = d.denoise(&crop, cw, ch, &all).unwrap();
        let secs = t1.elapsed().as_secs_f64();
        let tiles = tile_starts(cw, d.tile(), OVERLAP).len() * tile_starts(ch, d.tile(), OVERLAP).len();
        let per_tile = secs / tiles as f64;
        let step = (d.tile() - OVERLAP) as f64;
        let tiles_24mp = (6000.0 / step).ceil() * (4000.0 / step).ceil();
        eprintln!("512² crop: {secs:.2}s for {tiles} tiles ({per_tile:.3}s/tile, {} threads) → 24 MP ≈ {:.0}s", rayon::current_num_threads(), per_tile * tiles_24mp);
        // high-frequency energy: mean |x − 3×3 box blur| on the green channel
        let hf = |img: &[[f32; 3]]| {
            let mut s = 0.0f64;
            for y in 1..ch - 1 {
                for x in 1..cw - 1 {
                    let mut b = 0.0;
                    for dy in 0..3 {
                        for dx in 0..3 {
                            b += img[(y + dy - 1) * cw + x + dx - 1][1];
                        }
                    }
                    s += (img[y * cw + x][1] - b / 9.0).abs() as f64;
                }
            }
            s
        };
        let (a, b) = (hf(&crop), hf(&out));
        eprintln!("high-frequency energy {a:.1} → {b:.1}; mean {:.4} → {:.4}", mean(&crop), mean(&out));
        assert!(b < a * 0.7, "noise {a} → {b}");
        assert!((mean(&out) - mean(&crop)).abs() < mean(&crop) * 0.01);
        if std::env::var_os("LI_DENOISE_FULL").is_some() {
            let (w, h) = (im.width() as usize, im.height() as usize);
            let all_px: Vec<[f32; 3]> = im.pixels().map(to2020).collect();
            let t2 = std::time::Instant::now();
            let _ = d.denoise(&all_px, w, h, &all).unwrap();
            eprintln!("full {w}×{h} ({:.1} MP): {:.0}s", (w * h) as f64 / 1e6, t2.elapsed().as_secs_f64());
        }
    }
}
