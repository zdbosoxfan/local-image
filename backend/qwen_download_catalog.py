"""Qwen 2.1 publisher artifacts verified against HF LFS metadata 2026-09-29.

https://huggingface.co/api/models/Comfy-Org/Qwen-Image-2.1/tree/
cb504a4090723e43f17ad01cec0359490e2de613/{diffusion_models,text_encoders,vae}
"""
REVISION = 'cb504a4090723e43f17ad01cec0359490e2de613'
BASE_URL = f'https://huggingface.co/Comfy-Org/Qwen-Image-2.1/resolve/{REVISION}/'
LICENSE_URL = 'https://github.com/QwenLM/Qwen-Image-2.1/blob/main/LICENSE'
LICENSE_NOTE = 'Qwen Research License: noncommercial research/evaluation; commercial use requires a separate Qwen license.'


def artifact(folder, name, size, digest):
    return {'folder': folder, 'name': name, 'bytes': size, 'sha256': digest,
            'url': BASE_URL + folder + '/' + name}


VAE = artifact('vae', 'qwen_image_2.1_vae_bf16.safetensors', 675509688,
               'bb21f7473051e1ac368515dd3f2e15cd44d7a11748ee8823e1ddca3e4876b7c9')
QWEN_FILES = {
    'int8': (
        artifact('diffusion_models', 'qwen_image_2.1_int8_convrot.safetensors', 7256783064,
                 'cb74113cb03faecd79611b01fd7fd642f0aa60d6f0b95086abee214d75eaa57d'),
        artifact('text_encoders', 'qwen3vl_8b_int8_convrot.safetensors', 9350798360,
                 '8bfd0f6e12abf2d2d697ecc888e5e90b0d6741d6708f05799f53afa560452e8f'), VAE),
    'bf16': (
        artifact('diffusion_models', 'qwen_image_2.1_bf16.safetensors', 14230280616,
                 '89f4158d066cc33906a199fca85634f766892dd78f49b6698dabf187ac86c4bc'),
        artifact('text_encoders', 'qwen3vl_8b_bf16.safetensors', 17534334616,
                 '68bdc82bc1b66851162ae656225e7e2068166b603db19bd5d5a3b90eb12669a9'), VAE),
}
