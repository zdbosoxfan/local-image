#!/usr/bin/env python3
"""Controlled darktable 5.6.2 CLI CPSNR, only cached procedural DNGs under target/.
Run the demosaic_quality Rust example first. Requires external Flatpak darktable
and numpy for reference analysis; neither is a runtime/build dependency.
"""
from pathlib import Path
import sqlite3,struct,subprocess,numpy as np,json
root=Path('target/demosaic-quality');work=Path('target/refvec/demosaic');work.mkdir(parents=True,exist_ok=True)
cli=['flatpak','run','--command=darktable-cli','org.darktable.Darktable']
version=subprocess.check_output(cli+['--version'],text=True).splitlines()[0]
assert version=='darktable 5.6.2',f'update/review the reference pin before using {version}'
def export(sensor,dest,db,history=False):
    args=cli+[str(root/f'{sensor}.dng'),str(dest),'--apply-custom-presets','false','--icc-type','LIN_REC709']
    if history:args+=['--library',str(db)]
    args+=['--core','--configdir',str(work/'dt-cli-compare-config'),'--disable-opencl','--conf','plugins/darkroom/workflow=none','--conf','plugins/darkroom/chromatic-adaptation=legacy']
    if not history:args+=['--library',str(db)]
    subprocess.run(args,check=True)
def pfm(path):
    with path.open('rb') as f:
        assert f.readline().strip()==b'PF';w,h=map(int,f.readline().split());scale=float(f.readline())
        return np.frombuffer(f.read(),dtype='<f4' if scale<0 else '>f4').reshape(h,w,3)[::-1]*abs(scale)
results=[]
for sensor,method,enum in [('XTrans','Markesteijn3',1026),('Bayer','Lmmse',6)]:
    db=work/f'compare-{sensor}.db'
    # Keep the reviewed database; reuse it on subsequent runs instead of deleting files.
    if not db.exists():export(sensor,root/f'{sensor}-bootstrap.pfm',db)
    c=sqlite3.connect(db)
    version_,blob=c.execute('select module,op_params from history where operation="demosaic"').fetchone()
    assert version_==6 and len(blob)==48,(version_,len(blob))
    blob=bytearray(blob);struct.pack_into('<I',blob,12,enum);struct.pack_into('<I',blob,16,2)
    c.execute('update history set op_params=? where operation="demosaic"',(bytes(blob),))
    c.execute('update history set enabled=0 where operation="highlights"');c.commit();c.close()
    out=root/f'{sensor}-darktable-controlled.pfm';export(sensor,out,db,True)
    expected=pfm(out);actual=np.fromfile(root/f'{sensor}-{method}.f32',dtype='<f4').reshape(expected.shape)
    a,b=expected[24:-24,24:-24],actual[24:-24,24:-24];delta=(a-b).astype(np.float64)
    result=dict(sensor=sensor,method=method,cpsnr_db=float(-10*np.log10(np.mean(delta**2))),max_absolute=float(np.max(np.abs(delta))),reference=version)
    if sensor=='XTrans':
        old=np.fromfile(root/'XTrans-XtransDirectional.f32',dtype='<f4').reshape(expected.shape)[24:-24,24:-24]
        result['old_directional_cpsnr_db']=float(-10*np.log10(np.mean((a.astype(np.float64)-old)**2)))
    results.append(result)
print(json.dumps(results,indent=2));(work/'cli-metrics.json').write_text(json.dumps(results,indent=2)+'\n')
