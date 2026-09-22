"""Package the generated icon as standard Windows icon sizes; no image retouching."""
import json
from pathlib import Path
from PIL import Image

root = Path(__file__).resolve().parent
source = root / 'local-remove-icon.png'
sizes = [(n, n) for n in (16, 20, 24, 32, 40, 48, 64, 96, 128, 256)]
with Image.open(source) as image:
    assert image.mode == 'RGBA', f'Generated source must retain alpha: {image.mode}'
    assert image.getextrema()[3][0] == 0, 'Generated source must have transparent pixels'
    image.save(root / 'local-remove.ico', format='ICO', sizes=sizes)
with Image.open(root / 'local-remove.ico') as icon:
    assert icon.ico.sizes() == set(sizes)
    print(json.dumps({'source_size': list(Image.open(source).size), 'icon_sizes': sorted(icon.ico.sizes()), 'alpha_preserved': True}))
