# LaMa on-device AI Remove

Copyright 2021 Samsung Research. LaMa / big-LaMa is Apache-2.0:
https://github.com/advimman/lama/blob/main/LICENSE

The unmodified FP32 ONNX export is published by Carve-Photos as Apache-2.0:
https://huggingface.co/Carve/LaMa-ONNX/blob/c3c0c9e468934d62e79c329e35d82dd09ff8c444/README.md

`lama_fp32.onnx`, 208,044,816 bytes; SHA-256
`1faef5301d78db7dda502fe59966957ec4b79dd64e16f03ed96913c7a4eb68d6`.

Local Image downloads the weights separately. Neither training photographs nor the
checkpoint are included in the application. Runtime inference is safe Rust through
the existing tract dependencies. See `docs/LAMA-MODELS.md` for the tensor contract,
large-photo policy and reproducible evaluation.
