"""Read-only timing of the current full-frame layer-preview path."""
import json
from pathlib import Path
import time
import urllib.request

BASE='http://127.0.0.1:5000'
SID='14022ca7-b189-44e2-9b3c-0f8e61ca513d'
opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
session=json.load(opener.open(BASE+'/api/local-remove/session/'+SID))
measure=[]
for _ in range(3):
    started=time.perf_counter()
    data=opener.open(BASE+'/api/local-remove/session/'+SID+'/preview?full=true&v='+str(session['revision'])).read()
    measure.append({'seconds':round(time.perf_counter()-started,4),'bytes':len(data)})
result={'session':SID,'dimensions':[session['width'],session['height']],'layers':len(session['layers']),'old_full_frame_preview_requests':measure}
Path(__file__).with_name('performance-before.json').write_text(json.dumps(result,indent=2))
print(json.dumps(result))
