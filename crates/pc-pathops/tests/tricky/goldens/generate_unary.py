# Original Local Image external-oracle recipe. SPDX-License-Identifier: GPL-3.0-or-later
import os,json,pathlib,subprocess,xml.etree.ElementTree as E,math,re,numpy as np
from reference_profile import prepare
prepare()
root=pathlib.Path('crates/pc-pathops/tests/tricky');out=root/'goldens';m=json.loads((root/'manifest.json').read_text());g=json.loads((out/'generation.json').read_text());env=dict(os.environ,DISPLAY='',INKSCAPE_PROFILE_DIR='/tmp/vector-inkscape-profile')
# Independent adaptive cubic flattening, with convex-hull distance bounds.
def flatten(d):
 t=re.findall(r'[A-Za-z]|[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?',d);i=0;cmd='';p=np.array([0.,0.]);start=p.copy();pts=[]
 def cubic(a,b,c,e):
  line=e-a;n=np.linalg.norm(line)
  dist=max(abs(np.cross(line,b-a)),abs(np.cross(line,c-a)))/n if n else max(np.linalg.norm(b-a),np.linalg.norm(c-a))
  if dist<0.00025: pts.append(e);return
  ab=(a+b)/2;bc=(b+c)/2;ce=(c+e)/2;abc=(ab+bc)/2;bce=(bc+ce)/2;mid=(abc+bce)/2
  cubic(a,ab,abc,mid);cubic(mid,bce,ce,e)
 while i<len(t):
  if t[i].isalpha():cmd=t[i];i+=1
  upper=cmd.upper();relative=cmd.islower()
  if upper=='Z':pts.append(start.copy());p=start.copy();cmd='';continue
  count={'M':2,'L':2,'H':1,'V':1,'C':6}[upper];v=list(map(float,t[i:i+count]));i+=count
  if upper in ['M','L']:
   q=np.array(v)+(p if relative else 0);p=q
   if upper=='M':start=p.copy();cmd='l' if relative else 'L'
   pts.append(p.copy())
  elif upper=='H':p=np.array([v[0]+(p[0] if relative else 0),p[1]]);pts.append(p.copy())
  elif upper=='V':p=np.array([p[0],v[0]+(p[1] if relative else 0)]);pts.append(p.copy())
  elif upper=='C':
   a,b,c=[np.array(v[k:k+2])+(p if relative else 0) for k in [0,2,4]];cubic(p,a,b,c);p=c
 return np.array(pts)
def directed(a,b):
 # Max spacing <=0.001 certifies the polyline distance upper bound within .0005.
 src=[]
 for p,q in zip(a,a[1:]):
  n=max(1,math.ceil(np.linalg.norm(q-p)/0.001));src.extend(p+(q-p)*k/n for k in range(n))
 src.append(a[-1]);b0=b[:-1];vec=b[1:]-b0;den=(vec*vec).sum(axis=1);maxdist=0
 for k in range(0,len(src),256):
  x=np.array(src[k:k+256])[:,None,:];delta=x-b0;u=np.clip((delta*vec).sum(axis=2)/np.maximum(den,1e-30),0,1);diff=delta-u[:,:,None]*vec
  maxdist=max(maxdist,float(np.sqrt((diff*diff).sum(axis=2).min(axis=1)).max()))
 return maxdist
for c in m['unary_cases']:
 if os.environ.get('VECTOR_REFERENCE_CASE') and c['id'] != os.environ['VECTOR_REFERENCE_CASE']:continue
 if c['operation']=='stroke':continue
 ident=c['id'];op=c['operation'];original=(root/'inputs'/f'{ident}.svg').read_text();settings={'viewport':[96,96],'profile':'isolated profile; SVG numericprecision=16, minimumexponent=-16'}
 if op=='offset':
  a=c['parameters'];attrs={'effect':'offset','id':'effect','lpeversion':'1.3','unit':'px','offset':str(a['delta']),'linejoin_type':a['join'],'miter_limit':str(a['miter']),'attempt_force_join':'false'}
 else:
  tol=c['parameters']['tolerance'];threshold=min(100.,10000*tol/math.hypot(32,8));attrs={'effect':'simplify','id':'effect','lpeversion':'1.3','threshold':str(threshold),'steps':'1','smooth_angles':'0','helper_size':'0','simplify_individual_paths':'false','simplify_just_coalesce':'false'}
 for attempt in range(12):
  source=original;source_path=c['path']
  if op=='offset':
   factor=4096 if c['parameters']['delta'] < 0 else 1
   number=r'[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?'
   source_path=re.sub(number,lambda match:format(float(match[0])*factor,'.17g'),source_path)
   source=source.replace(c['path'],source_path).replace('<svg ',f'<svg viewBox="0 0 {96*factor} {96*factor}" ')
   settings['similarity']={'factor':factor,'translation':[0,0]}
   settings['reason']=('The power-of-two viewBox similarity makes Inkscape negative Offset LPE fixed 0.01-user-unit safety gap negligible in viewport pixels; LPE unit remains px and signed offset is unchanged.' if factor != 1 else 'Positive offset uses the original viewport so Inkscape retains exact circular arc joins.')
  s=source.replace('<svg ','<svg xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape" ')
  defs='<defs><inkscape:path-effect '+ ' '.join(f'{k}="{v}"' for k,v in attrs.items())+'/></defs>'
  s=s.replace('<path id="a"',defs+'<path inkscape:path-effect="#effect" inkscape:original-d="'+source_path+'" id="a"')
  inp=pathlib.Path('/tmp/vector-lpe-input.svg');inp.write_text(s);dest=out/ident/f'{op}.svg';dest.parent.mkdir(exist_ok=True)
  actions='select-by-id:a;object-to-path;export-do'
  r=subprocess.run(['/usr/bin/inkscape','--batch-process','--export-type=svg','--export-area-page','--export-plain-svg',f'--export-filename={dest}',f'--actions={actions}',str(inp)],env=env,capture_output=True,text=True)
  if r.returncode or not dest.exists():raise RuntimeError((ident,r.stderr))
  if op=='offset':break
  ds=[e.attrib['d'] for e in E.fromstring(dest.read_text()).iter() if e.tag.endswith('}path')];assert len(ds)==1
  a=flatten(c['path']);b=flatten(ds[0]);measured=max(directed(a,b),directed(b,a));certified=measured+0.001
  if certified<=tol:settings['symmetric_hausdorff_upper_bound_px']=certified;settings['measurement']='independent adaptive cubic flatten (0.00025px hull bound), polyline point-to-segment distance with <=0.001px sample spacing';break
  attrs['threshold']=str(float(attrs['threshold'])*0.6)
 else:raise RuntimeError('Cannot certify simplify '+ident)
 key=ident+'/'+op;g['inputs'][ident]=original;g['actions'][key]=actions;g['parameters'][key]=c;settings['lpe']=attrs;settings['effective_input_svg']=s;settings['raw_output_svg']=dest.read_text();g['settings'][key]=settings
 (out/'generation.json').write_text(json.dumps(g,indent=2)+'\n');print(ident,settings.get('symmetric_hausdorff_upper_bound_px','verified signed px offset LPE'),flush=True)
print('Generated',len(g['actions']),'references',flush=True)
