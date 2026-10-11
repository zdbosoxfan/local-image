# Original Local Image external-oracle recipe. SPDX-License-Identifier: GPL-3.0-or-later
# A common power-of-two similarity preserves every input's visible 96x96 geometry,
# while moving Livarot's fixed coordinate quantization below the thinnest 1e-4 region.
import os,json,pathlib,subprocess,re,xml.etree.ElementTree as E
from reference_profile import prepare
prepare()
root=pathlib.Path('crates/pc-pathops/tests/tricky');out=root/'goldens'
manifest=json.loads((root/'manifest.json').read_text());record=json.loads((out/'generation.json').read_text())
profile=pathlib.Path('/tmp/vector-inkscape-profile');prefs=profile/'preferences.xml'
tree=E.parse(prefs)
for group in tree.getroot().iter('group'):
    if group.attrib.get('id')=='svgoutput':group.set('numericprecision','16');group.set('minimumexponent','-16')
tree.write(prefs,encoding='utf-8',xml_declaration=True)
env=dict(os.environ,DISPLAY='',INKSCAPE_PROFILE_DIR=str(profile))
number=r'[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?'
def scaled(data):
    assert not re.search(r'[AaQqSsTt]',data), 'recipe expects corpus M/L/H/V/C/Z geometry'
    return re.sub(number,lambda m:format(float(m[0])*factor,'.17g'),data)
for case in manifest['cases']:
    factor=65536 if case['family'] in ('near-degenerate-sliver','near-coincident-edge') else 4096
    ident=case['id'];original=(root/'inputs'/f'{ident}.svg').read_text()
    effective=f'''<!-- Original Local Image synthetic input. SPDX-License-Identifier: CC0-1.0 -->
<svg xmlns="http://www.w3.org/2000/svg" width="{96*factor}" height="{96*factor}" viewBox="0 0 {96*factor} {96*factor}">
<g fill-rule="{case['fill_rule']}" fill="black"><path id="a" d="{scaled(case['a'])}"/><path id="b" d="{scaled(case['b'])}"/></g></svg>'''
    inp=pathlib.Path('/tmp/vector-conditioned-input.svg');inp.write_text(effective)
    record['inputs'][ident]=original
    for op,action in manifest['operations'].items():
        actions=f'select-by-id:a,b;{action};export-do';dest=out/ident/f'{op}.svg'
        r=subprocess.run(['/usr/bin/inkscape','--batch-process','--export-area-page','--export-plain-svg',f'--export-filename={dest}',f'--actions={actions}',str(inp)],env=env,text=True,capture_output=True)
        if r.returncode or not dest.exists():raise RuntimeError((ident,op,r.stderr))
        raw_output=dest.read_text();svg=E.fromstring(raw_output);svg.set('width','96');svg.set('height','96');svg.set('viewBox',f'0 0 {96*factor} {96*factor}')
        E.register_namespace('','http://www.w3.org/2000/svg')
        dest.write_text('<!-- Original Local Image synthetic output. SPDX-License-Identifier: CC0-1.0 -->\n'+E.tostring(svg,encoding='unicode')+'\n')
        key=f'{ident}/{op}';record['actions'][key]=actions
        record['settings'][key]={'viewport':[96,96],'profile':'isolated profile; SVG numericprecision=16, minimumexponent=-16','fill_rule':case['fill_rule'],'effective_input_svg':effective,'raw_output_svg':raw_output,'output_normalization':'Only width/height set to 96 and inverse-similarity viewBox retained; path data and styles unchanged.','similarity':{'factor':factor/case['scale'],'translation':[-case['origin'][0]*factor/case['scale'],-case['origin'][1]*factor/case['scale']]},'reason':'Power-of-two similarity preserves input presentation and boolean set identities, prevents fixed Livarot quantization from erasing 1e-4 slivers.'}
    (out/'generation.json').write_text(json.dumps(record,indent=2)+'\n')
    print(ident,flush=True)
