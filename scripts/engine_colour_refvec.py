#!/usr/bin/env python3
"""Extract offline upstream functions, build standalone reference harnesses outside Cargo.
Usage: python3 scripts/engine_colour_refvec.py /path/to/engine-sources
Only emitted CSV fixtures are source-controlled. gcc/g++ -O0 -ffp-contract=off.
"""
import csv, json, re, subprocess, sys, tomllib
from pathlib import Path
root = Path(__file__).resolve().parents[1]
sources = Path(sys.argv[1])
def function(src, marker):
    start = src.index(marker)
    brace = src.index('{', start)
    depth = 1
    end = brace + 1
    while depth:
        depth += (src[end] == '{') - (src[end] == '}')
        end += 1
    return src[start:end]
def build(name, code, fixture, cpp=False):
    path = root / 'target/refvec' / name
    path.mkdir(parents=True, exist_ok=True)
    file = path / ('main.cc' if cpp else 'main.c')
    file.write_text(code)
    exe = path / 'reference'
    subprocess.run(['g++' if cpp else 'gcc', '-O0', '-ffp-contract=off', str(file), '-lm', '-o', str(exe)], check=True)
    dest = root / fixture
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(subprocess.check_output([str(exe)]))
s = (sources / 'dt/src/iop/sigmoid.c').read_text()
preamble = '''#include <math.h>
#include <stdio.h>
#include <stddef.h>
#define MIDDLE_GREY 0.1845f
#define dt_isnan isnan
#define DT_OMP_DECLARE_SIMD()
#define for_each_channel(c, ...) for(int c=0;c<3;c++)
#define min3f(p) fminf(p[0],fminf(p[1],p[2]))
typedef float dt_aligned_pixel_t[4];
typedef struct {size_t min,mid,max;} dt_iop_sigmoid_value_order_t;
typedef struct {float middle_grey_contrast,contrast_skewness,display_white_target,display_black_target;} Params;
typedef struct {float paper_power,film_power,white_target,black_target,film_fog,paper_exposure;} Data;
'''
log = function(s, 'static inline float _generalized_loglogistic_sigmoid(')
commit = s[s.index('  // Calculate a reference slope', s.index('void commit_params')):s.index('  module_data->color_processing',s.index('void commit_params'))]
code = preamble + log + '\nstatic Data params(Params p) { const Params *params=&p; Data data; Data *module_data=&data;\n' + commit + '\nreturn data;}\n'
for name in ['static inline void _desaturate_negative_values(', 'static void _pixel_channel_order(', 'static inline void _preserve_hue_and_energy(']:
    code += function(s,name) + '\n'
code += '''int main(void) {
float settings[][4]={{1.5f,0,100,.0152f},{2.0f,-.4f,100,.1f},{1.2f,.5f,80,.02f}};
float xs[]={-1,0,1e-6f,.001f,.01f,.05f,.1845f,.5f,1,2,8,128,1e20f};
for(int j=0;j<3;j++) { Data d=params((Params){settings[j][0],settings[j][1],settings[j][2],settings[j][3]});
for(int i=0;i<13;i++) printf("curve,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g\\n",settings[j][0],settings[j][1],settings[j][2],settings[j][3],xs[i],_generalized_loglogistic_sigmoid(xs[i],d.white_target,d.paper_exposure,d.film_fog,d.film_power,d.paper_power)); }
float pixels[][3]={{0,0,0},{.18f,.18f,.18f},{3,.6f,.1f},{.2f,.9f,.1f},{.1f,.2f,8},{-.1f,.2f,.5f},{-1,-.5f,-.1f},{.4f,.4f,.1f},{.1f,.4f,.4f}};
Data d=params((Params){1.5f,0,100,.0152f});
for(int j=0;j<9;j++) for(int k=0;k<3;k++) { float hue=.5f*k; dt_aligned_pixel_t c={pixels[j][0],pixels[j][1],pixels[j][2],0},pos,per,out; dt_iop_sigmoid_value_order_t order;
_desaturate_negative_values(c,pos); for(int i=0;i<3;i++) per[i]=_generalized_loglogistic_sigmoid(pos[i],d.white_target,d.paper_exposure,d.film_fog,d.film_power,d.paper_power);
_pixel_channel_order(pos,&order); _preserve_hue_and_energy(pos,per,out,order,hue);
printf("pixel,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g\\n",c[0],c[1],c[2],hue,out[0],out[1],out[2]); }
}
'''
build('sigmoid',code,'crates/lc-pipeline/tests/fixtures/sigmoid.csv')
s = (sources / 'dt/src/common/curve_tools.c').read_text()
code = '#include <stdio.h>\n#include <stdlib.h>\n#include <math.h>\n#include <float.h>\n#define EPSILON (2*FLT_MIN)\n'
code += function(s,'float *monotone_hermite_set(int n, float x[], float y[])\n{') + '\n'
code += function(s,'float catmull_rom_val(int n, float x[], float xval, float y[], float tangents[])\n{') + '\nint main(void) {\n'
data = (root/'crates/lc-pipeline/src/base_curve_data.rs').read_text()
for idx,line in enumerate([l for l in data.splitlines() if l.startswith('(')]):
    pairs = re.findall(r'\[([0-9.]+),\s*([0-9.]+)\]',line)
    code += '{float x[]={'+','.join(x+'f' for x,y in pairs)+'}, y[]={'+','.join(y+'f' for x,y in pairs)+'};\n'
    code += f'float *m=monotone_hermite_set({len(pairs)},x,y); for(int i=0;i<=32;i++) {{float v=i/32.0f; printf("{idx},%.9g,%.9g\\n",v,catmull_rom_val({len(pairs)},x,v,y,m));}} free(m);}}\n'
code+='}\n'
build('basecurve',code,'crates/lc-pipeline/tests/fixtures/basecurve.csv')
s = (sources/'src/rt_dcp.cc').read_text()
# Extract the unmodified table evaluator, using its own data shape. Linear encoding avoids
# RT's global gamma LUTs (the Rust path uses an analytic sRGB transfer, tested separately).
fn = function(s,'inline void DCPProfile::hsdApply(').replace('inline void DCPProfile::hsdApply','void hsdApply')
fn=fn.replace(') const\n{', ')\n{').replace('Color::gammatab_srgb1[v * 65535.f]','v').replace('Color::igammatab_srgb1[v_encoded * val_scale * 65535.f]','v_encoded * val_scale')
code = '''#include <vector>
#include <algorithm>
#include <cstdio>
using std::max;
struct HsbModify {float hue_shift,sat_scale,val_scale;};
struct HsdTableInfo {int val_divisions;bool srgb_gamma;struct {float h_scale,s_scale,v_scale;int max_sat_index0,max_hue_index0,max_val_index0,hue_step,val_step;} pc;};
''' + fn + '''
int main() {for(int dims=1;dims<=3;dims+=2) { HsdTableInfo t={dims,false,{4.f/6.f,2.f,float(dims-1),1,3,dims-2,3,12}};
std::vector<HsbModify> data; for(int v=0;v<dims;v++) for(int h=0;h<4;h++) for(int s=0;s<3;s++) data.push_back({float(5*h-2*s+3*v),1.f+.05f*(h+s+v),.9f+.01f*(h+s+v)});
for(int i=0;i<24;i++) {float h0=(i*17%360)/60.f,s0=.01f+.012f*i,v0=.05f+.02f*i;float h=h0,s=s0,v=v0; hsdApply(t,data,h,s,v); printf("%d,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g\\n",dims,h0*60.f,s0,v0,h*60.f,s,v);}}
}
'''
build('dcp',code,'crates/lc-color/tests/fixtures/dcp.csv',True)
# Exact RawTherapee matrix blending and reciprocal-CCT weighting, supplied temperatures.
fn=function(s,'DCPProfile::Matrix mix3x3(').replace('DCPProfile::Matrix','Matrix')
code='''#include <array>
#include <cstdio>
using Matrix=std::array<std::array<double,3>,3>;
''' + fn + '''
int main(){ Matrix a={{{.9,.2,-.15},{-.3,1.25,.08},{.02,-.12,.85}}}, b={{{.7,.3,-.1},{-.35,1.3,.1},{.05,-.2,1}}};
for(double t: {2000.,2856.,3200.,4500.,5500.,6504.,9000.}) {double mix;
if(t<=2856.) mix=1.;else if(t>=6504.) mix=0.;else { const double invT=1./t; mix=(invT-(1./6504.))/((1./2856.)-(1./6504.)); }
Matrix m=mix3x3(a,mix,b,1.-mix);printf("%.17g,%.17g",t,mix);for(auto r:m)for(double v:r)printf(",%.17g",v);puts("");}}
'''
build('illuminants',code,'crates/lc-color/tests/fixtures/illuminants.csv',True)
# Independent camera-data spots: read original TOMLs, never the generated Rust table.
camera_root=sources/'dnglab/dnglab-0.8.0/rawler/data/cameras'
with (root/'crates/lc-raw/tests/fixtures/camera-matrices.csv').open('w', newline='') as out:
    rows=csv.writer(out, lineterminator='\n')
    for name in ['canon/1000d.toml','nikon/1aw1.toml','sony/a1.toml','fuji/e550.toml','panasonic/cm1.toml','olympus/c5050z.toml']:
        camera=tomllib.loads((camera_root/name).read_text())
        for illuminant, code in [('A',17),('D65',21)]:
            rows.writerow([camera['make'],camera['model'],code,*camera['cameras']['color_matrix'][illuminant]])
print('Generated sigmoid, all base presets, DCP 2D/3D, dual-illuminant vectors, and camera TOML spots.')
