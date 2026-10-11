# Original Local Image external-oracle recipe. SPDX-License-Identifier: GPL-3.0-or-later
# Exact common similarity lowers Livarot's absolute intersection quantization; stroke
# widths, absolute dash lengths and dash offsets scale with the path, preserving SVG units.
import os,json,pathlib,subprocess,re,xml.etree.ElementTree as E
from reference_profile import prepare
prepare()
root=pathlib.Path('crates/pc-pathops/tests/tricky');out=root/'goldens';m=json.loads((root/'manifest.json').read_text());g=json.loads((out/'generation.json').read_text())
env=dict(os.environ,DISPLAY='',INKSCAPE_PROFILE_DIR='/tmp/vector-inkscape-profile');factor=4096
number=r'[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?'
def scaled(value):return re.sub(number,lambda match:format(float(match[0])*factor,'.17g'),value)
E.register_namespace('','http://www.w3.org/2000/svg')
for case in m['unary_cases']:
 if case['operation']!='stroke':continue
 if os.environ.get('VECTOR_REFERENCE_CASE') and case['id']!=os.environ['VECTOR_REFERENCE_CASE']:continue
 ident=case['id'];original=(root/'inputs'/f'{ident}.svg').read_text();svg=E.fromstring(original)
 svg.set('width',str(96*factor));svg.set('height',str(96*factor));svg.set('viewBox',f'0 0 {96*factor} {96*factor}')
 for path in svg.iter():
  if path.tag.endswith('}path'):
   for attr in ('d','stroke-width','stroke-dasharray','stroke-dashoffset'):
    if attr in path.attrib:path.set(attr,scaled(path.attrib[attr]))
 effective='<!-- Original Local Image synthetic input. SPDX-License-Identifier: CC0-1.0 -->\n'+E.tostring(svg,encoding='unicode')+'\n'
 inp=pathlib.Path('/tmp/vector-conditioned-stroke.svg');inp.write_text(effective);dest=out/ident/'stroke.svg';actions='select-by-id:a;object-stroke-to-path;export-do'
 result=subprocess.run(['/usr/bin/inkscape','--batch-process','--export-area-page','--export-plain-svg',f'--export-filename={dest}',f'--actions={actions}',str(inp)],env=env,text=True,capture_output=True)
 if result.returncode or not dest.exists():raise RuntimeError((ident,result.stderr))
 raw=dest.read_text();svg=E.fromstring(raw);svg.set('width','96');svg.set('height','96');svg.set('viewBox',f'0 0 {96*factor} {96*factor}')
 dest.write_text('<!-- Original Local Image synthetic output. SPDX-License-Identifier: CC0-1.0 -->\n'+E.tostring(svg,encoding='unicode')+'\n')
 key=ident+'/stroke';g['inputs'][ident]=original;g['actions'][key]=actions;g['parameters'][key]=case;g['settings'][key]={'viewport':[96,96],'profile':'isolated profile; SVG numericprecision=16, minimumexponent=-16','effective_input_svg':effective,'raw_output_svg':raw,'similarity':{'factor':factor,'translation':[0,0]},'output_normalization':'Only root width/height reset to 96; path data/styles unchanged.','reason':'Common similarity scales every stroke/dash length and reduces absolute Livarot intersection quantization in overlapping cap/join contours.'}
 (out/'generation.json').write_text(json.dumps(g,indent=2)+'\n');print(ident,flush=True)
