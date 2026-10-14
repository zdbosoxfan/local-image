#!/usr/bin/env python3
"""Render RawNIND reference stacks with darktable-cli, without opening a GUI.
Run after scripts/rawnind_camdata_samples.py. All config/history/output remains under target/.
Uses darktable 5.6 module-12 parameters from its upstream introspection schema, and actual
imported per-file rawprepare/WB parameters from the CLI-created library. No owner library is used.
"""
from pathlib import Path
import shutil
import sqlite3
import struct
import subprocess

root = Path(__file__).resolve().parents[1]
ref = root / 'target/camdata-reference'
ref.mkdir(parents=True, exist_ok=True)
config = ref / 'dt-reproduce-config'
config.mkdir(exist_ok=True)
base = ref / 'dt-reproduce.db'
initial = ref / 'dt-reproduce-initial'
initial.mkdir(exist_ok=True)
output = ref / 'dt-reproduce-linear'
output.mkdir(exist_ok=True)
files = sorted((root / 'target/rawnind').glob('Bayer_7D-1_*.cr2'))


def run(args, log):
    with (ref / log).open('w') as f:
        subprocess.run(['flatpak', 'run', '--command=darktable-cli', 'org.darktable.Darktable'] + args,
                       stdout=f, stderr=subprocess.STDOUT, check=True)


for src in files:
    run([str(src), str(initial), '--out-ext', 'tif', '--width', '512', '--height', '512',
         '--core', '--configdir', str(config), '--library', str(base)], f'dt-import-{src.stem}.log')
# Version 12: 14 scalar floats; wavelet-auto mode; 6 x 7 x/y band curves; five flags.
floats = [1., 7., 1., 1., 0., 0., .1, 1.] + [-1., 0., 0.] + [0., 0., 0.]
params = (struct.pack('<14f', *floats) + struct.pack('<i', 4)
          + struct.pack('<84f', *([i / 6 for _ in range(6) for i in range(7)] + [.5] * 42))
          + struct.pack('<5i', 1, 1, 1, 1, 1))
for src in files:
    key = 'gt' if '_GT_' in src.name else 'iso' + src.name.split('_ISO')[1].split('_')[0]
    for mode in (['off'] if key == 'gt' else ['off', 'on']):
        db = ref / f'dt-reproduce-{key}-{mode}.db'
        shutil.copyfile(base, db)
        c = sqlite3.connect(db)
        image_id = c.execute('select id from images where filename=?', (src.name,)).fetchone()[0]
        num = c.execute('select max(num)+1 from history where imgid=?', (image_id,)).fetchone()[0]
        blend, bv = c.execute('select blendop_params,blendop_version from history where imgid=? order by num limit 1', (image_id,)).fetchone()
        c.execute('update history set enabled=0 where imgid=? and operation in ("sigmoid","exposure","channelmixerrgb","highlights")', (image_id,))
        c.execute('insert into history(imgid,num,module,operation,op_params,enabled,blendop_params,blendop_version,multi_priority,multi_name,multi_name_hand_edited) values(?,?,?,?,?,?,?,?,?,?,?)',
                  (image_id, num, 12, 'denoiseprofile', params, int(mode == 'on'), blend, bv, 0, '', 0))
        c.execute('update images set history_end=? where id=?', (num + 1, image_id))
        c.commit()
        c.close()
        dest = output / f'{key}-{mode}.tif'
        dest.unlink(missing_ok=True)
        run([str(src), str(dest), '--library', str(db), '--icc-type', 'LIN_REC2020',
             '--core', '--configdir', str(config), '--conf', 'plugins/imageio/format/tiff/bpp=32'],
            f'dt-reproduce-{key}-{mode}.log')
        print(dest, flush=True)
