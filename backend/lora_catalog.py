"""Exact author-published LoRA artifacts reviewed on 2026-09-29.

Style/compatibility notes describe the publisher's evidence, not an app benchmark.
Every entry is pinned to a commit and LFS digest; no repository code is executed.
"""


def entry(model, repo, title, filename, revision, size, digest, *, description,
          style, trigger='', usage='text-to-image', license='apache-2.0',
          license_note='', experimental=False, strength=1.0, steps=4, guidance=1):
    return {'model': model, 'repo_id': repo, 'title': title, 'filename': filename,
            'revision': revision, 'bytes': size, 'sha256': digest,
            'description': description, 'style': style, 'trigger_phrase': trigger,
            'usage': usage, 'license': license, 'license_note': license_note,
            'experimental': experimental, 'source_url': 'https://huggingface.co/' + repo + '/blob/' + revision + '/README.md',
            'recommended_steps': steps, 'recommended_strength': strength,
            'recommended_settings': {'steps': steps, 'guidance': guidance}}


QWEN_LICENSE = 'The Qwen Image 2.1 base research license also applies; research/evaluation use, not a commercial-use clearance.'
KLEIN9_LICENSE = 'Publisher describes these adapters as research/non-commercial despite Apache metadata. The Klein 9B base license also applies.'
FLEET_REPO = 'rehan-fal/klein-style-fleet'
FLEET_REV = 'e30fb26b037b8257a922f180786a787afdc2d391'
DEVER_REPO = 'DeverStyle/Flux.2-Klein-Loras'
DEVER_REV = '96237bf014e53e2baf24e3b756cdf20c4ca18953'

CURATED = (
    # Preserve the existing first item/identity when upgrading installed profiles.
    entry('z-image-turbo', 'ostris/z_image_turbo_childrens_drawings', "Children's drawings",
          'z_image_turbo_childrens_drawings.safetensors', '7fcd66a99149c58741990fca28562a4a581af7a9', 170128264,
          '25b9959eb2054ccb9dd1815e90add6da3818f550e8e60a081392095830ff0f7a',
          description='Playful, rough hand-drawn illustrations with simple shapes. No trigger phrase is required; start with one adapter at strength 1.',
          style='Hand-drawn / cartoon', steps=8),
    entry('z-image-turbo', 'suayptalha/Z-Image-Turbo-Realism-LoRA', 'Photographic realism',
          'pytorch_lora_weights.safetensors', '8dc179cb56844d4ddc6e4527c30e98347eeae971', 85094800,
          'b38b074964f1f6564c22fee42125accd7a218a916966b5d0f0b2f63d4939c2f7',
          description='Photographic scenes, portrait textures and natural or studio lighting. Start the prompt with Realism; 8 steps is the app base-model starting point.',
          style='Photorealism', trigger='Realism', steps=8),
    entry('qwen', 'prithivMLmods/Qwen-Image-2.1-Natural-Exposure-LoRA', 'Natural exposure',
          'Qwen-Image-2.1-Natural-Exposure-LoRA-4000.safetensors', '382d066d079854a86c9513f3c5a4026ada21ddbe', 83943952,
          'a8edea397ce55ae6e1a442f3ba963eb78beeb515d4fe7d217d496542317292b1',
          description='Reference-image edit for more neutral exposure and natural color. Add a photo and use the trigger sentence. Experimental; difficult lighting and fine detail may change.',
          style='Natural photo editing', trigger='Transform the image with balanced neutral exposure', usage='reference-edit',
          license='qwen-research', license_note=QWEN_LICENSE, experimental=True, steps=25),
    entry('qwen', 'WarmBloodAban/Qwen-Image-2.1-LoRAs', 'Anime character consistency',
          'Qwen2.1_Anime_consistency.safetensors', 'c4ab5473bfdf585fc19cfd1e280f79d2b0c79947', 167830408,
          '0c171eb802ea8051b511f2d93c1743eeadd030316255aa98fa65d40809366752',
          description='Reference-image editing for anime characters, expressions and model-sheet consistency. Add a character reference and describe the edit. Experimental; identity is not guaranteed.',
          style='Anime editing', usage='reference-edit', license_note=QWEN_LICENSE,
          experimental=True, strength=0.7, steps=25),
    entry('qwen', 'Airmongsity/Qwen-Image-2.1-Sts2-Cards-Drawer', 'Faceted card illustration',
          'deckbuilder_cardart_style_lora_v1_fp16.safetensors', 'adce1c8fc91491d419d8b097822338fb6b1a5129', 100703552,
          'af780e9a292b658ab896a9b3d9fffb44df7d1c04839189c1fe1fd7cfb3159636',
          description='Bold faceted illustration, dark shadow shapes and bright rim light. Apply 25 steps and guidance 3; begin with sts2 card art and describe concrete shapes/materials. Faces and machinery can be weak.',
          style='Graphic illustration', trigger='sts2 card art', license='qwen-research',
          license_note=QWEN_LICENSE, strength=0.9, steps=25, guidance=3),
    entry('flux2-klein-4b', FLEET_REPO, 'Watercolor wash',
          'watercolor/pytorch_lora_weights.safetensors', FLEET_REV, 96365272,
          'ac5e76e1a07c19c32c3465be41f6772dd58e94e2673274f78b9c637dd71b5137',
          description='A watercolor illustration style trained on a small synthetic image set. Prefix the subject with wtrclr style. Publisher recommends distilled Klein 4B at 4 steps and strength 1.',
          style='Watercolor', trigger='wtrclr style'),
    entry('flux2-klein-4b', FLEET_REPO, 'Claymation miniature',
          'claymation/pytorch_lora_weights.safetensors', FLEET_REV, 96365272,
          '32bfa0d7450d725e7a007ead6a05bd2759ee0cad8fa082a1b1bb04d344b5c988',
          description='Clay-like characters and miniature scenes, trained on a small synthetic image set. Prefix the subject with claymtn style. Use distilled Klein 4B at 4 steps and strength 1.',
          style='Clay / 3D cartoon', trigger='claymtn style'),
    entry('flux2-klein-9b', DEVER_REPO, 'Orange splatter illustration',
          'dever_orange_splatter_illustration_f2k_9b (dvr_osi_style).safetensors', DEVER_REV, 165704432,
          'd05ea3d4e06004fd48ab66ac82375f85c9eae1081c0a880cfcb428be03a5c911',
          description='Orange-accented splatter illustration. Include dvr_osi_style and a subject. Use 4 steps as the distilled base-model starting point; adjust strength to taste.',
          style='Splatter illustration', trigger='dvr_osi_style', license_note=KLEIN9_LICENSE),
    entry('flux2-klein-9b', DEVER_REPO, 'Teal dark illustration',
          'dever_teal_dark_f2k_9b (dvr_tldr_style).safetensors', DEVER_REV, 165704400,
          'bb4faa8cb07bd9f9c90220d6ec0cb269f69f265e562a2be783424330344ebb61',
          description='Dark, teal-accented illustration for new images or reference edits. Include dvr_tldr_style; the publisher suggests black background for edits. Strong photographic wording can reduce the style.',
          style='Dark graphic illustration', trigger='dvr_tldr_style', usage='both', license_note=KLEIN9_LICENSE),
    entry('flux2-klein-9b', DEVER_REPO, 'Blueprint wireframe',
          'dever_blueprint_wireframe_f2k_9b (dvr_wf_style).safetensors', DEVER_REV, 165704424,
          '4157aeb1c3c84757244c0c76afd7a2ed678b09842ab64f15662b506fa5fd8529',
          description='Blueprint and wireframe-like line illustration for new images or reference edits. Include dvr_wf_style. Treat it as an artistic effect, not a dimensionally accurate technical drawing.',
          style='Blueprint / wireframe', trigger='dvr_wf_style', usage='both', license_note=KLEIN9_LICENSE),
    # Kept for historical projects; FLUX Dev is no longer a selectable generator.
    entry('flux2-dev', 'ByteZSzn/Flux.2-Turbo-ComfyUI', 'FLUX.2 Turbo acceleration',
          'Flux_2-Turbo-LoRA_comfyui.safetensors', '57231752b9c5632e42911c73ee0ed138d381a6d4', 2760814880,
          '011487390b8020baf22a9d543930c90d74a4809b7241bee6b0622777b17b413b',
          description='Historical Dev accelerator. Set 8 steps, guidance 4 and strength 1; settings never change automatically.',
          style='Acceleration', license='See publisher and FLUX Non-Commercial License', steps=8, guidance=4),
)

PRESENTATION_FIELDS = ('title', 'description', 'style', 'trigger_phrase', 'usage', 'license',
                       'license_note', 'experimental', 'source_url', 'recommended_steps',
                       'recommended_strength', 'recommended_settings')
