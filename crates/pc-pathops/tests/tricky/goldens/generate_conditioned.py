# Original Local Image independent SVG construction. SPDX-License-Identifier: GPL-3.0-or-later
# Two analytical equivalences avoid Inkscape's short-dash inner-miter clipping and
# large concave Offset LPE approximation. Inkscape itself converts and unions all
# primitives; no Local Image operation, library or generated path is used.
import pathlib,json,math,re,os,subprocess,xml.etree.ElementTree as E
from reference_profile import prepare
prepare()
root=pathlib.Path('crates/pc-pathops/tests/tricky');out=root/'goldens';manifest=json.loads((root/'manifest.json').read_text());record=json.loads((out/'generation.json').read_text());factor=4096
E.register_namespace('','http://www.w3.org/2000/svg');ns='{http://www.w3.org/2000/svg}'
def polygon(svg,points):E.SubElement(svg,ns+'polygon',{'points':' '.join(f'{x*factor:.17g},{y*factor:.17g}' for x,y in points),'fill':'black'})
def band(svg,a,b,half):
 dx,dy=b[0]-a[0],b[1]-a[1];length=math.hypot(dx,dy);n=(-dy/length*half,dx/length*half)
 polygon(svg,[(a[0]+n[0],a[1]+n[1]),(a[0]-n[0],a[1]-n[1]),(b[0]-n[0],b[1]-n[1]),(b[0]+n[0],b[1]+n[1])])
for ident in ['offset-star-round-20','stroke-butt-miter-dashed']:
 case=next(c for c in manifest['unary_cases'] if c['id']==ident);original=(root/'inputs'/f'{ident}.svg').read_text();source=E.fromstring(original);path=next(e for e in source.iter() if e.tag.endswith('}path'));numbers=list(map(float,re.findall(r'[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?',path.attrib['d'])));points=list(zip(numbers[::2],numbers[1::2]));svg=E.Element(ns+'svg',{'width':str(96*factor),'height':str(96*factor),'viewBox':f'0 0 {96*factor} {96*factor}'})
 if case['operation']=='offset':
  delta=case['parameters']['delta'];polygon(svg,points)
  for a,b in zip(points,points[1:]+points[:1]):band(svg,a,b,delta)
  for x,y in points:
   vertices=[(x+delta*math.cos(i*math.tau/16),y+delta*math.sin(i*math.tau/16)) for i in range(17)]
   d=f'M{vertices[0][0]*factor:.17g},{vertices[0][1]*factor:.17g}'
   d+=''.join(f' A{delta*factor},{delta*factor} 0 0 1 {a*factor:.17g},{b*factor:.17g}' for a,b in vertices[1:])+' Z'
   E.SubElement(svg,ns+'path',{'d':d,'fill':'black'})
  reason='Independent exact Minkowski identity for a polygon with a round positive offset: source fill union edge rectangles union radius-delta vertex disks. Disks use 16 exact SVG circular arcs to bound Inkscape relative curve-flattening error; Inkscape converts polygons and performs the union. Avoids its Offset LPE default relative approximation at concave intersections.'
 else:
  params=case['parameters'];width=params['width'];half=width/2;pattern=[d*width for d in params['dashes']];offset=params['dash_offset']*width;lengths=[math.dist(a,b) for a,b in zip(points,points[1:])];cum=[0.]
  for length in lengths:cum.append(cum[-1]+length)
  def at(t):
   i=min(len(lengths)-1,next((i for i,end in enumerate(cum[1:]) if t<=end),len(lengths)-1));u=(t-cum[i])/lengths[i];return tuple(points[i][k]+(points[i+1][k]-points[i][k])*u for k in range(2))
  slot=0
  while offset>=pattern[slot]:offset-=pattern[slot];slot=(slot+1)%len(pattern)
  start=0.;remaining=pattern[slot]-offset
  while start<cum[-1]:
   end=min(start+remaining,cum[-1])
   if slot%2==0:
    parts=[at(start)]+[points[i] for i in range(1,len(points)-1) if start<cum[i]<end]+[at(end)]
    for a,b in zip(parts,parts[1:]):band(svg,a,b,half)
    for a,b,c in zip(parts,parts[1:],parts[2:]):
     u=((b[0]-a[0])/math.dist(a,b),(b[1]-a[1])/math.dist(a,b));v=((c[0]-b[0])/math.dist(b,c),(c[1]-b[1])/math.dist(b,c));cross=u[0]*v[1]-u[1]*v[0];side=-1 if cross>0 else 1;n=(-u[1]*half*side,u[0]*half*side);m=(-v[1]*half*side,v[0]*half*side);p=(b[0]+n[0],b[1]+n[1]);q=(b[0]+m[0],b[1]+m[1]);t=((q[0]-p[0])*v[1]-(q[1]-p[1])*v[0])/cross;tip=(p[0]+t*u[0],p[1]+t*u[1]);assert math.dist(tip,b)<=params['miter']*half;polygon(svg,[b,p,tip,q])
   start=end;slot=(slot+1)%len(pattern);remaining=pattern[slot]
  reason='Independent SVG straight-stroke identity: exact arc-length dash intervals, butt-capped segment rectangles and bounded outer miter wedges, unioned by Inkscape. Avoids Inkscape clipping the inner miter beyond a short dash endpoint; source and all manifest dash/join parameters are unchanged.'
 effective=E.tostring(svg,encoding='unicode');inp=pathlib.Path('/tmp/vector-analytical-inkscape-input.svg');inp.write_text(effective);operation=case['operation'];dest=out/ident/f'{operation}.svg';actions='select-all:all;object-to-path;path-union;export-do';result=subprocess.run(['/usr/bin/inkscape','--batch-process','--export-type=svg','--export-plain-svg','--export-area-page',f'--export-filename={dest}',f'--actions={actions}',str(inp)],env=dict(os.environ,DISPLAY='',INKSCAPE_PROFILE_DIR='/tmp/vector-inkscape-profile'),capture_output=True,text=True)
 if result.returncode:raise RuntimeError(result.stderr)
 raw=dest.read_text();svg=E.fromstring(raw);svg.set('width','96');svg.set('height','96');dest.write_text('<!-- Original Local Image synthetic output. SPDX-License-Identifier: CC0-1.0 -->\n'+E.tostring(svg,encoding='unicode')+'\n');key=ident+'/'+operation;record['actions'][key]=actions;record['settings'][key]={'viewport':[96,96],'profile':'isolated profile; SVG numericprecision=16, minimumexponent=-16','effective_input_svg':effective,'raw_output_svg':raw,'similarity':{'factor':factor,'translation':[0,0]},'output_normalization':'Only root width/height reset to 96; path data/styles unchanged.','analytical_equivalence':True,'reason':reason};(out/'generation.json').write_text(json.dumps(record,indent=2)+'\n');print(ident,flush=True)
