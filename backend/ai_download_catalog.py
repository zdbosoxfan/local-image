"""Pinned upstream artifacts, verified against publisher metadata on 2026-09-22.

ComfyUI: https://api.github.com/repos/Comfy-Org/ComfyUI/releases/tags/v0.37.0
Models: Hugging Face repository revisions and LFS SHA-256 metadata.
The Qwen3 text encoder is a FLUX dependency, not the Qwen image-edit model.
"""

COMFY_RELEASE = {
    'version': '0.37.0',
    'name': 'ComfyUI_windows_portable_nvidia.7z',
    'url': 'https://github.com/Comfy-Org/ComfyUI/releases/download/v0.37.0/ComfyUI_windows_portable_nvidia.7z',
    'bytes': 1925204508,
    'sha256': '7805f634fab51f63a238aaf0cfe2a9833bb7c86ddfc8400a60919f44460d7d65',
}

FLUX_FILES = (
    {'folder': 'diffusion_models', 'name': 'flux-2-klein-base-4b.safetensors', 'label': 'FLUX Klein base model',
     'url': 'https://huggingface.co/black-forest-labs/FLUX.2-klein-base-4B/resolve/a3b4f4849157f664bdbc776fd7453c2783562f4d/flux-2-klein-base-4b.safetensors',
     'bytes': 7751105712, 'sha256': '9c5fed22b76baea749d88fc2abe3ad53245e7b21a0d353a762665eea00043b92'},
    {'folder': 'text_encoders', 'name': 'qwen_3_4b.safetensors', 'label': 'FLUX text encoder',
     'url': 'https://huggingface.co/Comfy-Org/vae-text-encorder-for-flux-klein-4b/resolve/5f526678002e43af5551dadb73ce2e8c91b43afe/split_files/text_encoders/qwen_3_4b.safetensors',
     'bytes': 8044982048, 'sha256': '6c671498573ac2f7a5501502ccce8d2b08ea6ca2f661c458e708f36b36edfc5a'},
    {'folder': 'vae', 'name': 'flux2-vae.safetensors', 'label': 'FLUX image decoder',
     'url': 'https://huggingface.co/Comfy-Org/vae-text-encorder-for-flux-klein-4b/resolve/5f526678002e43af5551dadb73ce2e8c91b43afe/split_files/vae/flux2-vae.safetensors',
     'bytes': 336211292, 'sha256': '868fe7b343cc8f3a19dbcfcafbc3d5f888802be3f89bd81b65b3621a066ce8f3',
     'compatible_existing': ({'bytes': 336213556, 'sha256': 'd64f3a68e1cc4f9f4e29b6e0da38a0204fe9a49f2d4053f0ec1fa1ca02f9c4b5',
         'source': 'https://huggingface.co/Comfy-Org/flux2-dev/blob/ab9055628ea245000e610f2aa2c96f4746093546/split_files/vae/flux2-vae.safetensors'},)},
    {'folder': 'loras', 'name': 'flux-2-klein-object-remove.safetensors', 'label': 'Object removal adapter',
     'url': 'https://huggingface.co/fal/flux-2-klein-4B-object-remove-lora/resolve/0e3f58790356bf1319b263fc56b333c294b42ff7/kDEkt5q7tDLKOpQJIVMPx_pytorch_lora_weights_comfy_converted.safetensors',
     'bytes': 76038936, 'sha256': 'dc197de62e174863f83fc4052465603f4389de7c3a78e17f3d41a1f6f11488ec'},
)
