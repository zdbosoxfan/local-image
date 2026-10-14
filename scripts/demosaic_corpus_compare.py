#!/usr/bin/env python3
"""Merge-time controlled CLI demosaic comparison on the ten pinned CC0 files.
Prepare the files with demosaic_samples.py and the Rust demosaic_quality example's
--reference-raw option first. This script is a benchmark; run only when authorized.
"""
from pathlib import Path
import csv,json,sqlite3,struct,subprocess,numpy as np
root=Path('target/demosaic-quality/raw-references');work=Path('target/refvec/demosaic')
cli=['flatpak','run','--command=darktable-cli','org.darktable.Darktable']
version=subprocess.check_output(cli+['--version'],text=True).splitlines()[0]
assert version=='darktable 5.6.2',f'Review reference version {version}'
def export(id_,dest,db,history=False):
    args=cli+[str(root/f'{id_}.dng'),str(dest),'--apply-custom-presets','false','--icc-type','LIN_REC709']
    if history:args+=['--library',str(db)]
    args+=['--core','--configdir',str(work/'dt-corpus-config'),'--disable-opencl','--conf','plugins/darkroom/workflow=none','--conf','plugins/darkroom/chromatic-adaptation=legacy']
    if not history:args+=['--library',str(db)]
    subprocess.run(args,check=True)
def pfm(path):
    with path.open('rb') as f:
        assert f.readline().strip()==b'PF';w,h=map(int,f.readline().split());scale=float(f.readline())
        return np.frombuffer(f.read(),'<f4' if scale<0 else '>f4').reshape(h,w,3)[::-1]*abs(scale)
results=[]
for row in csv.DictReader(Path('crates/lc-raw/tests/fixtures/demosaic-corpus.tsv').open(),delimiter='\t'):
    id_=row['id'];db=work/f'corpus-{id_}.db'
    if not db.exists():export(id_,root/f'{id_}-bootstrap.pfm',db)
    methods=[('Rcd',5),('Lmmse',6)] if row['model'].startswith('GFX') else [('Markesteijn3',1026),('DualMarkesteijnVng',3074)]
    for method,enum in methods:
        c=sqlite3.connect(db);module,blob=c.execute('select module,op_params from history where operation="demosaic"').fetchone()
        assert module==6 and len(blob)==48,(module,len(blob))
        blob=bytearray(blob);struct.pack_into('<I',blob,12,enum);struct.pack_into('<I',blob,16,2);struct.pack_into('<f',blob,20,.2)
        c.execute('update history set op_params=? where operation="demosaic"',(bytes(blob),));c.execute('update history set enabled=0 where operation="highlights"');c.commit();c.close()
        dest=root/f'{id_}-{method}-darktable.pfm';export(id_,dest,db,True)
        a=pfm(dest)[24:-24,24:-24].astype(np.float64)
        b=np.fromfile(root/f'{id_}-{method}.f32','<f4').reshape(512,512,3)[24:-24,24:-24]
        delta=a-b;result=dict(id=id_,model=row['model'],method=method,cpsnr_db=float(-10*np.log10(np.mean(delta**2))),max_absolute=float(np.max(np.abs(delta))),reference=version)
        if not row['model'].startswith('GFX'):
            old=np.fromfile(root/f'{id_}-XtransDirectional.f32','<f4').reshape(512,512,3)[24:-24,24:-24]
            result['old_directional_cpsnr_db']=float(-10*np.log10(np.mean((a-old)**2)))
        results.append(result)
(work/'corpus-metrics.json').write_text(json.dumps(results,indent=2)+'\n');print(json.dumps(results,indent=2))
