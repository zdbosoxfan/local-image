"""Entry point used by the Windows app and Capture One's Edit With menu."""
import ctypes
import json
from pathlib import Path
import subprocess
import sys
import time
import urllib.error
import urllib.request

CONNECTOR = Path(r'C:\Users\Owner\Documents\RapidRAW-AI-Connector')
BASE = 'http://127.0.0.1:5000'
CREATE_NO_WINDOW = 0x08000000


def api(path, payload=None, key=None):
    headers = {'Content-Type':'application/json'}
    if key: headers['x-local-launcher'] = key
    request = urllib.request.Request(BASE+path, data=json.dumps(payload).encode() if payload is not None else None, headers=headers)
    with urllib.request.urlopen(request, timeout=120) as response:
        return json.load(response)


def main():
    subprocess.Popen(['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(CONNECTOR/'Start-RapidRAW-AI.ps1'),'-NoOpen'],creationflags=CREATE_NO_WINDOW)
    deadline=time.monotonic()+180
    while True:
        try:
            api('/api/local-remove/status')
            break
        except (OSError,ValueError):
            if time.monotonic()>deadline: raise RuntimeError('Local Remove could not start. Check the connector logs.')
            time.sleep(1)
    paths=[Path(arg).resolve() for arg in sys.argv[1:] if arg!='--no-open']
    urls=[]
    key=(CONNECTOR/'local-remove-data'/'launcher.key').read_text(encoding='ascii').strip()
    for path in paths:
        session=api('/api/local-remove/open-local',{'path':str(path)},key)
        urls.append(BASE+'/remove?session='+session['id'])
    if not urls: urls=[BASE+'/remove']
    if '--no-open' in sys.argv:
        print(json.dumps({'urls':urls})); return
    browsers=[Path(r'C:\Program Files\BraveSoftware\Brave-Origin\Application\brave.exe'),
              Path(r'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe')]
    browser=next((p for p in browsers if p.is_file()),None)
    if browser is None: raise RuntimeError('Install Microsoft Edge or Brave to open Local Remove.')
    for url in urls:
        subprocess.Popen([str(browser),'--app='+url,'--new-window','--window-size=1360,900'],creationflags=CREATE_NO_WINDOW)


if __name__=='__main__':
    try: main()
    except Exception as error:
        log=CONNECTOR/'logs'/'local-remove-launcher-error.txt'; log.write_text(str(error),encoding='utf-8')
        if '--no-open' in sys.argv: raise
        ctypes.windll.user32.MessageBoxW(0,str(error),'Local Remove could not start',0x10)
