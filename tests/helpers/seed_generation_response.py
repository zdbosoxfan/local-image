"""CPU-made response fixture for browser routing tests; never executes a model."""
import json
import os
from pathlib import Path
import sys
import tempfile
import urllib.request

root = Path(__file__).resolve().parents[2]
profile = Path(sys.argv[1]).resolve()
base = sys.argv[2]
payload = json.loads(sys.argv[3])
if not profile.is_relative_to((root / 'qa-artifacts').resolve()):
    raise RuntimeError('Use only this checkout QA profile.')
with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(base + '/api/local-remove/runtime', timeout=5) as response:
    runtime = json.load(response)
if Path(runtime['data_root']).resolve() != profile:
    raise RuntimeError('The running backend profile does not match the fixture destination.')
os.environ.update(LOCAL_IMAGE_DATA_DIR=str(profile), LOCAL_REMOVE_DATA_DIR=str(profile), COMFY_HOST='127.0.0.1', COMFY_PORT='51999')
sys.path.insert(0, str(root / 'backend'))
from PIL import Image
import local_remove as editor
import image_generation
import generation_library
from stock_attribution import collect_attributions

request = image_generation.GenerationRequest(**payload)
parameters = image_generation.generation_parameters(request)
if parameters['width'] * parameters['height'] > 16_777_216:
    raise RuntimeError('The controlled test fixture is limited to 16 megapixels; this is not an application limit.')
image = Image.new('RGBA' if parameters['transparent'] else 'RGB', (parameters['width'], parameters['height']), (120, 80, 150, 0) if parameters['transparent'] else (120, 80, 150))
image.paste((200, 90, 50, 255) if parameters['transparent'] else (200, 90, 50), (image.width // 4, image.height // 4, image.width * 3 // 4, image.height * 3 // 4))
credits = []
for identifier in request.reference_session_ids:
    credits.extend(collect_attributions(editor.read_session(identifier)))
with tempfile.TemporaryDirectory(prefix='controlled-generation-', dir=profile) as directory:
    document = image_generation.create_generated_session(image, parameters, Path(directory), credits)
skip_library = sys.argv[4:] == ['--skip-library']
if sys.argv[4:] and not skip_library:
    raise RuntimeError('Unknown fixture option.')
if not skip_library:
    generation_library.add_generated(image, document)
print(json.dumps({'session': document, **parameters, 'library_warning': 'Controlled library persistence failure; working image retained.' if skip_library else ''}))
