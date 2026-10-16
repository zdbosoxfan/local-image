//! A small, validated FP32 ONNX graph for restoration compute. ONNX decoding uses the existing
//! tract reader; no external runtime or new dependency. Operator semantics follow ONNX opset 20.
use anyhow::{Context, Result, bail, ensure};
use std::{collections::HashMap, path::Path};
use tract_onnx::{pb, prelude::Framework};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    pub c: usize,
    pub h: usize,
    pub w: usize,
}
impl Shape {
    pub fn checked_len(self) -> Option<usize> {
        self.c.checked_mul(self.h)?.checked_mul(self.w)
    }
    pub fn len(self) -> usize {
        self.c * self.h * self.w
    }
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
}
#[derive(Clone, Debug)]
pub enum Op {
    Conv { weights: Vec<f32>, bias: Vec<f32>, kernel: [usize; 2], stride: [usize; 2], pad: [usize; 2], transpose: bool },
    Activation { alpha: f32, sigmoid: bool },
    Pool { kernel: [usize; 2], stride: [usize; 2] },
    Concat,
    Shuffle { block: usize, crd: bool },
}
#[derive(Clone, Debug)]
pub struct Node {
    pub inputs: Vec<usize>,
    pub shape: Shape,
    pub op: Op,
}
#[derive(Clone, Debug)]
pub struct Graph {
    pub input: Shape,
    pub nodes: Vec<Node>,
    pub output: usize,
}
fn tensor(t: &pb::TensorProto) -> Result<Vec<f32>> {
    ensure!(t.data_type == 1 && t.external_data.is_empty(), "only embedded FP32 ONNX weights are supported");
    let data = if t.raw_data.is_empty() {
        t.float_data.clone()
    } else {
        ensure!(t.raw_data.len().is_multiple_of(4), "truncated FP32 tensor");
        t.raw_data.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect()
    };
    let count = t.dims.iter().try_fold(1usize, |a, &d| a.checked_mul(usize::try_from(d).ok()?)).context("invalid tensor dimensions")?;
    ensure!(data.len() == count && data.iter().all(|v| v.is_finite()), "invalid ONNX weights");
    Ok(data)
}
impl Graph {
    pub fn load(path: &Path, input: Shape) -> Result<Self> {
        let proto = tract_onnx::onnx().proto_model_for_path(path)?;
        Self::from_proto(proto, input)
    }
    pub fn from_proto(proto: pb::ModelProto, input: Shape) -> Result<Self> {
        ensure!(
            input.checked_len().is_some_and(|n| n > 0 && n <= 1 << 29) && input.h.is_multiple_of(16) && input.w.is_multiple_of(16),
            "invalid restoration input"
        );
        let g = proto.graph.context("ONNX has no graph")?;
        ensure!(g.input.len() == 1 && g.output.len() == 1, "expected one restoration input and output");
        let mut ids = HashMap::from([(g.input[0].name.clone(), 0usize)]);
        let mut constants: HashMap<String, pb::TensorProto> = g.initializer.into_iter().map(|t| (t.name.clone(), t)).collect();
        let mut nodes: Vec<Node> = Vec::new();
        for n in g.node {
            let attr = |name: &str| n.attribute.iter().find(|a| a.name == name);
            let int = |name: &str, default: i64| attr(name).map_or(default, |a| a.i);
            let pair = |name: &str, default: [usize; 2]| -> Result<[usize; 2]> {
                let Some(a) = attr(name) else { return Ok(default) };
                ensure!(a.ints.len() == 2, "invalid {name}");
                Ok([usize::try_from(a.ints[0])?, usize::try_from(a.ints[1])?])
            };
            ensure!(n.output.len() == 1, "multiple outputs on {}", n.op_type);
            if n.op_type == "Constant" {
                let t = attr("value").and_then(|a| a.t.clone()).context("unsupported ONNX constant")?;
                constants.insert(n.output[0].clone(), t);
                continue;
            }
            if n.op_type == "Identity" {
                let name = n.input.first().context("identity has no input")?;
                if let Some(t) = constants.get(name).cloned() {
                    constants.insert(n.output[0].clone(), t);
                } else {
                    ids.insert(n.output[0].clone(), *ids.get(name).context("unknown identity input")?);
                }
                continue;
            }
            let inputs: Vec<usize> = n.input.iter().filter_map(|name| ids.get(name).copied()).collect();
            let shape_of = |id: usize| if id == 0 { input } else { nodes[id - 1].shape };
            let a = shape_of(*inputs.first().context("unknown ONNX input")?);
            let (shape, op) = match n.op_type.as_str() {
                "Conv" | "ConvTranspose" => {
                    ensure!(inputs.len() == 1 && int("group", 1) == 1 && pair("dilations", [1, 1])? == [1, 1], "unsupported grouped/dilated convolution");
                    ensure!(attr("auto_pad").is_none_or(|a| a.s.is_empty() || a.s == b"NOTSET"), "unsupported automatic padding");
                    let t = constants.get(n.input.get(1).context("missing convolution weights")?).context("missing weights")?;
                    ensure!(t.dims.len() == 4, "convolution weights must be OIHW/IOHW");
                    let transpose = n.op_type == "ConvTranspose";
                    let ci = usize::try_from(t.dims[usize::from(!transpose)])?;
                    let co = usize::try_from(t.dims[usize::from(transpose)])?;
                    ensure!(ci == a.c, "convolution channel mismatch");
                    let kernel = [usize::try_from(t.dims[2])?, usize::try_from(t.dims[3])?];
                    let stride = pair("strides", [1, 1])?;
                    ensure!(kernel.into_iter().all(|v| v > 0) && stride.into_iter().all(|v| v > 0), "invalid convolution geometry");
                    let pads = attr("pads").map(|a| a.ints.clone()).unwrap_or(vec![0; 4]);
                    ensure!(pads.len() == 4 && pads.iter().all(|&v| v >= 0), "invalid padding");
                    let pad = [pads[0] as usize, pads[1] as usize];
                    let output_padding = pair("output_padding", [0, 0])?;
                    let dim = |v: usize, axis: usize| -> Result<usize> {
                        let padding = pad[axis].checked_add(usize::try_from(pads[axis + 2])?).context("invalid convolution padding")?;
                        let extent = kernel[axis].checked_add(output_padding[axis]).context("invalid output padding")?;
                        if transpose {
                            v.checked_sub(1)
                                .and_then(|v| v.checked_mul(stride[axis]))
                                .and_then(|v| v.checked_add(extent))
                                .and_then(|v| v.checked_sub(padding))
                                .context("invalid deconvolution size")
                        } else {
                            Ok(v.checked_add(padding).and_then(|v| v.checked_sub(kernel[axis])).context("kernel outside input")? / stride[axis] + 1)
                        }
                    };
                    let weights = tensor(t)?;
                    let bias = match n.input.get(2).filter(|n| !n.is_empty()) {
                        Some(b) => tensor(constants.get(b).context("missing bias")?)?,
                        None => vec![0.; co],
                    };
                    ensure!(bias.len() == co, "bias channel mismatch");
                    (Shape { c: co, h: dim(a.h, 0)?, w: dim(a.w, 1)? }, Op::Conv { weights, bias, kernel, stride, pad, transpose })
                }
                "LeakyRelu" | "Relu" | "Sigmoid" => {
                    ensure!(inputs.len() == 1, "invalid activation inputs");
                    (a, Op::Activation { alpha: if n.op_type == "Relu" { 0. } else { attr("alpha").map_or(0.01, |a| a.f) }, sigmoid: n.op_type == "Sigmoid" })
                }
                "MaxPool" => {
                    let kernel = pair("kernel_shape", [2, 2])?;
                    let stride = pair("strides", [1, 1])?;
                    ensure!(
                        kernel.into_iter().all(|v| v > 0) && stride.into_iter().all(|v| v > 0) && a.h >= kernel[0] && a.w >= kernel[1],
                        "invalid pooling geometry"
                    );
                    ensure!(int("ceil_mode", 0) == 0 && attr("pads").is_none_or(|a| a.ints.iter().all(|&p| p == 0)), "unsupported pooling padding");
                    (Shape { c: a.c, h: (a.h - kernel[0]) / stride[0] + 1, w: (a.w - kernel[1]) / stride[1] + 1 }, Op::Pool { kernel, stride })
                }
                "Concat" => {
                    ensure!(int("axis", 1) == 1 && inputs.len() == 2, "only two-input channel concat is supported");
                    let b = shape_of(inputs[1]);
                    ensure!(a.w == b.w && a.h == b.h, "concat size mismatch");
                    (Shape { c: a.c.checked_add(b.c).context("concat channels overflow")?, ..a }, Op::Concat)
                }
                "DepthToSpace" => {
                    let block = usize::try_from(int("blocksize", 0))?;
                    let square = block.checked_mul(block).filter(|s| *s > 0).context("invalid PixelShuffle block")?;
                    ensure!(a.c.is_multiple_of(square), "invalid PixelShuffle");
                    let mode = attr("mode").map(|a| a.s.as_slice()).unwrap_or(b"DCR");
                    ensure!(mode == b"DCR" || mode == b"CRD", "invalid PixelShuffle mode");
                    (
                        Shape {
                            c: a.c / square,
                            h: a.h.checked_mul(block).context("PixelShuffle height overflow")?,
                            w: a.w.checked_mul(block).context("PixelShuffle width overflow")?,
                        },
                        Op::Shuffle { block, crd: mode == b"CRD" },
                    )
                }
                op => bail!("unsupported restoration ONNX operator {op}"),
            };
            ensure!(shape.checked_len().is_some_and(|n| n > 0 && n <= 1 << 29), "restoration tensor too large");
            nodes.push(Node { inputs, shape, op });
            ids.insert(n.output[0].clone(), nodes.len());
        }
        let output = *ids.get(&g.output[0].name).context("unknown restoration output")?;
        Ok(Self { input, nodes, output })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn proto(op: &str, attributes: Vec<pb::AttributeProto>, weights: bool) -> pb::ModelProto {
        pb::ModelProto {
            graph: Some(pb::GraphProto {
                input: vec![pb::ValueInfoProto { name: "in".into(), ..Default::default() }],
                output: vec![pb::ValueInfoProto { name: "out".into(), ..Default::default() }],
                initializer: if weights {
                    vec![pb::TensorProto { name: "weights".into(), data_type: 1, dims: vec![1, 1, 1, 1], float_data: vec![1.], ..Default::default() }]
                } else {
                    vec![]
                },
                node: vec![pb::NodeProto {
                    op_type: op.into(),
                    input: if weights { vec!["in".into(), "weights".into()] } else { vec!["in".into()] },
                    output: vec!["out".into()],
                    attribute: attributes,
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        }
    }
    fn ints(name: &str, ints: Vec<i64>) -> pb::AttributeProto {
        pb::AttributeProto { name: name.into(), ints, ..Default::default() }
    }
    #[test]
    fn ds2_malicious_shapes_return_errors_without_panics() {
        let input = Shape { c: 1, h: 16, w: 16 };
        let block = pb::AttributeProto { name: "blocksize".into(), i: i64::MAX, ..Default::default() };
        assert!(Graph::from_proto(proto("DepthToSpace", vec![block], false), input).is_err());
        assert!(Graph::from_proto(proto("Conv", vec![ints("pads", vec![i64::MAX; 4])], true), input).is_err());
        assert!(Graph::from_proto(proto("ConvTranspose", vec![ints("output_padding", vec![i64::MAX; 2])], true), input).is_err());
        assert!(Graph::from_proto(proto("Relu", vec![], false), Shape { c: usize::MAX, h: 16, w: 16 }).is_err());
    }
    #[test]
    fn ds2_maxpool_omitted_stride_is_one() {
        let graph = Graph::from_proto(proto("MaxPool", vec![ints("kernel_shape", vec![3, 3])], false), Shape { c: 1, h: 16, w: 16 }).unwrap();
        assert_eq!(graph.nodes[0].shape, Shape { c: 1, h: 14, w: 14 });
    }
}
