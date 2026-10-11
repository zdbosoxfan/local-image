#!/usr/bin/env python3
"""Record one serialized, offline candidate validation with isolated settings."""
import datetime,fcntl,hashlib,json,os,re,subprocess,sys,time
from pathlib import Path
ROOT=Path.cwd(); OUT=ROOT/'docs/wip/v2-recovery-validation'
label=sys.argv[1]; args=sys.argv[2:]
assert '--offline' in args, 'Cargo validation must be offline'
assert not (OUT/(label+'.json')).exists(), 'Use a new evidence label'
lock=(ROOT/'target/recovery-cargo.lock');lock.parent.mkdir(exist_ok=True)
with lock.open('a') as held:
 fcntl.flock(held,fcntl.LOCK_EX|fcntl.LOCK_NB)
 env=os.environ.copy(); profile=ROOT/'target/recovery-profiles'/label
 for name,d in [('LOCAL_IMAGE_DATA_DIR','local-image'),('LOCAL_IMAGE_MODELS_DIR','models'),('XDG_CONFIG_HOME','config'),('XDG_DATA_HOME','data'),('XDG_CACHE_HOME','cache'),('XDG_STATE_HOME','state'),('TMPDIR','tmp')]:
  p=profile/d;p.mkdir(parents=True,exist_ok=True);env[name]=str(p)
 env.update(LIGHTCRAFT_GPU='1',LIGHTCRAFT_GPU_BACKEND='vulkan',WGPU_BACKEND='vulkan',WGPU_POWER_PREF='high',LC_REQUIRE_GPU='1',LOCAL_IMAGE_REQUIRE_GPU='1',LOCAL_IMAGE_COMFY='127.0.0.1:9')
 files=subprocess.check_output(['git','ls-files','-co','--exclude-standard','-z']).decode().split('\0'); manifest={}
 for f in sorted(set(files)):
  if f and not f.startswith(('docs/','target/','.git/')) and Path(f).is_file():manifest[f]=hashlib.sha256(Path(f).read_bytes()).hexdigest()
 fingerprint=hashlib.sha256(json.dumps(manifest,sort_keys=True).encode()).hexdigest()
 (OUT/(label+'.sources.json')).write_text(json.dumps(manifest,indent=2)+'\n')
 command=[env['CARGO'],*args];record={'label':label,'command':command,'cwd':str(ROOT),'head':subprocess.check_output(['git','rev-parse','HEAD']).decode().strip(),'source_sha256':fingerprint,'cargo_lock_sha256':hashlib.sha256(Path('Cargo.lock').read_bytes()).hexdigest(),'environment':{k:v for k,v in env.items() if k.startswith(('CARGO','RUST','LIGHTCRAFT_GPU','WGPU','LC_REQUIRE','LOCAL_IMAGE','XDG_CONFIG','XDG_DATA','XDG_CACHE','XDG_STATE','TMPDIR'))},'started':datetime.datetime.now(datetime.timezone.utc).isoformat(),'log':str(OUT/(label+'.log')),'status':'running'}
 (OUT/(label+'.json')).write_text(json.dumps(record,indent=2)+'\n')
 start=time.monotonic()
 with (OUT/(label+'.log')).open('w') as log:
  result=subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT)
 data=(OUT/(label+'.log')).read_bytes();content=data.decode(errors='replace')
 summaries=re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out',content)
 record.update(status='finished',exit=result.returncode,elapsed_seconds=time.monotonic()-start,finished=datetime.datetime.now(datetime.timezone.utc).isoformat(),log_sha256=hashlib.sha256(data).hexdigest(),test_summaries=summaries,counts={k:sum(int(s[i+1]) for s in summaries) for i,k in enumerate(['passed','failed','ignored','measured','filtered'])})
 (OUT/(label+'.json')).write_text(json.dumps(record,indent=2)+'\n'); print(json.dumps({k:record[k] for k in ['label','exit','elapsed_seconds','counts','log','source_sha256']},indent=2));print('\n'.join(content.splitlines()[-25:]))
 sys.exit(result.returncode)
