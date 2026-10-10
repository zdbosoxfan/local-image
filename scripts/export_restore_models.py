#!/usr/bin/env python3
"""Build-time/reference tool only; NEVER called by Local Image.
Run in a throwaway venv under target/, with the exact upstream checkouts and versions
listed in docs/RESTORE-MODELS.md. Takes only approved, hash-checked official weights.
"""
import argparse
import hashlib
import json
import pathlib
import sys

import onnx
import torch


def verified(path, expected):
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != expected:
        raise ValueError(f"Unapproved checkpoint: {path}, SHA-256 {digest}")


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("kind", choices=["ddcolor", "scratches"])
    parser.add_argument("--source", required=True, type=pathlib.Path)
    parser.add_argument("--checkpoint", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    args = parser.parse_args()
    torch.set_num_threads(3)
    torch.manual_seed(0)
    if args.kind == "ddcolor":
        verified(args.checkpoint, "81dd643904f4664c3718513e3320ae3db0c567f0d3e18398606659adee7bfc17")
        sys.path.insert(0, str(args.source.resolve()))
        from basicsr.archs.ddcolor_arch import DDColor

        model = DDColor(encoder_name="convnext-t", decoder_name="MultiScaleColorDecoder",
                        input_size=[512, 512], num_output_channels=2, last_norm="Spectral",
                        do_normalize=False, num_queries=100, num_scales=3, dec_layers=9)
        state = torch.load(args.checkpoint, map_location="cpu", weights_only=True)["params"]
        # Official exporter uses strict=False. These are the only unused keys, an
        # ImageNet classification head removed from the inference architecture.
        del state["encoder.arch.head_cls.weight"]
        del state["encoder.arch.head_cls.bias"]
        model.load_state_dict(state, strict=True)
        model.eval()
        x = torch.linspace(0, 1, 3 * 512 * 512).reshape(1, 3, 512, 512)
        with torch.no_grad():
            before = model(x)
            # Freeze eval-mode spectral normalisation into the exact inference weights.
            # Otherwise torch exports constant vector-vector MatMul, unsupported by tract.
            for module in model.modules():
                if hasattr(module, "weight_orig"):
                    torch.nn.utils.remove_spectral_norm(module)
            assert (model(x) - before).abs().max().item() < 1e-5
        names, dynamic = ["ab"], None
    else:
        verified(args.checkpoint, "b2d7ab04e9b3885c6b1991bb7a0b823129dd6e3ac078a9fd059ebd2a7ba59a95")
        sys.path.insert(0, str(args.source.resolve() / "Global"))
        from detection_models.networks import UNet

        model = UNet(in_channels=1, out_channels=1, depth=4, conv_num=2, wf=6,
                     padding=True, batch_norm=True, up_mode="upsample", with_tanh=False,
                     sync_bn=False, antialiasing=True)
        model.load_state_dict(torch.load(args.checkpoint, map_location="cpu", weights_only=True)["model_state"], strict=True)
        model.eval()
        x = torch.linspace(-1, 1, 256 * 256).reshape(1, 1, 256, 256)
        names = ["logits"]
        dynamic = {"input": {2: "height", 3: "width"}, "logits": {2: "height", 3: "width"}}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with torch.no_grad():
        torch.onnx.export(model, x, str(args.output), opset_version=17, input_names=["input"],
                          output_names=names, dynamic_axes=dynamic, dynamo=False)
    onnx.checker.check_model(onnx.load(args.output))
    print(json.dumps({"file": args.output.name, "bytes": args.output.stat().st_size,
                      "sha256": hashlib.sha256(args.output.read_bytes()).hexdigest()}))


if __name__ == "__main__":
    main()
