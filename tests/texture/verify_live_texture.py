"""Live endpoint acceptance on a copied crop, without changing the user's session."""
import base64
import hashlib
import io
import json
from pathlib import Path
import re
import shutil
import time
import urllib.request
import numpy as np
from PIL import Image, ImageDraw

HERE=Path(__file__).resolve().parent
CONNECTOR=Path(r'C:\Users\Owner\Documents\RapidRAW-AI-Connector')
BASE='http://127.0.0.1:5000'
opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
html=opener.open(BASE+'/remove').read().decode()
token=re.search(r"TOKEN='([^']+)'",html).group(1)
key=(CONNECTOR/'local-remove-data/launcher.key').read_text().strip()
def api(path,data=None,native=False):
    headers={'x-local-remove-token':token,'Origin':BASE,'Content-Type':'application/json'}
    if native:headers['x-local-launcher']=key
    request=urllib.request.Request(BASE+path,None if data is None else json.dumps(data).encode(),headers)
    with opener.open(request,timeout=60) as response:return json.load(response)
testdir=HERE/'fixtures'/('live-'+str(time.time_ns()));testdir.mkdir(parents=True)
source_path=testdir/'Bush repair test.png'
shutil.copy2(HERE/'baseline-crops/case2-source.png',source_path)
source_hash=hashlib.sha256(source_path.read_bytes()).hexdigest()
collection=api('/api/local-remove/open-files',{'paths':[str(source_path)]},True)['collection']
siddata=api(f"/api/local-remove/collection/{collection['id']}/entry/{collection['entries'][0]['id']}/open",{})['session']
sid=siddata['id']
mask_bytes=(HERE/'baseline-crops/case2-mask.png').read_bytes()
start=time.perf_counter()
session=api(f'/api/local-remove/session/{sid}/remove',{'revision':0,'model':'heal','heal_method':'texture','mask':base64.b64encode(mask_bytes).decode()})
elapsed=time.perf_counter()-start
assert session['layers'][-1]['heal_method']=='texture'
root=CONNECTOR/'local-remove-data/sessions'/sid
layer=session['layers'][-1]
source=Image.open(source_path).convert('RGB');result=source.copy()
result.paste(Image.open(root/layer['color']),(layer['x'],layer['y']),Image.open(root/layer['mask']))
mask=Image.open(io.BytesIO(mask_bytes)).convert('L')
assert np.array_equal(np.asarray(result)[np.asarray(mask)==0],np.asarray(source)[np.asarray(mask)==0])
assert hashlib.sha256(source_path.read_bytes()).hexdigest()==source_hash
old=Image.open(HERE/'baseline-crops/case2-telea.png').convert('RGB')
panel=Image.new('RGB',(1536,544),(30,30,30));draw=ImageDraw.Draw(panel)
for index,(label,image) in enumerate([('Original bush',source),('Previous Quick Heal',old),('New Texture repair',result)]):
    panel.paste(image,(index*512,32));draw.text((index*512+12,10),label,fill='white')
output=HERE.parent.parent/'outputs/Local Remove - texture repair comparison.jpg'
panel.save(output,quality=95)
report={'ok':True,'elapsed_seconds':round(elapsed,3),'source_unchanged':True,'outside_mask_changed':0,'session':sid,'collection':collection['id'],'source':str(source_path),'url':BASE+'/remove?session='+sid,'layers':len(session['layers'])}
(HERE/'live-texture-verification.json').write_text(json.dumps(report,indent=2))
print(json.dumps(report))
