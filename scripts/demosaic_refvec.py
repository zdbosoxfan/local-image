#!/usr/bin/env python3
"""Build out-of-build scalar upstream demosaic fixtures. No C enters Cargo.
Sources are pinned whole under target/refvec/demosaic/upstream; pass --sources
only to reuse the project's read-only reference checkout. See fixture README.
"""
from pathlib import Path
import argparse, re, subprocess, urllib.request, hashlib
DT='733bd69f32cac7ff5e41025115942772add1f088'
RT='5f486d3678b34c74ba0c63571c17babe20935019'
p=argparse.ArgumentParser();p.add_argument('--sources',type=Path);a=p.parse_args()
work=Path('target/refvec/demosaic');work.mkdir(parents=True,exist_ok=True)
fixtures=Path('crates/lc-raw/tests/fixtures')
HASHES={
 ('dt','src/common/darktable.h'):'f5d0bbb8d22db4f502aa174d5b2046a4765d7209bca61d090169a7de9793af18',
 ('dt','src/iop/demosaicing/xtrans.c'):'175898cb103295f07ed458cf9a2acd3ddb797b152aa58b3706a901ff0bcc2caa',
 ('dt','src/iop/demosaicing/lmmse.c'):'8b60d1079038e83b25913cb1df353434d10fafd923498960817cee1c53792ec8',
 ('rt','rtengine/demosaic_algos.cc'):'de1bae94d451881a99e5c45e099ebcc24a9ffa91943fcd3b55671523f4649d49',
}
def source(project,pin,path):
    dest=work/'upstream'/project/path;dest.parent.mkdir(parents=True,exist_ok=True)
    if a.sources:
        orig=a.sources/project/path
        dest.write_bytes(orig.read_bytes())
    elif not dest.exists():
        url=f'https://raw.githubusercontent.com/{"darktable-org/darktable" if project=="dt" else "RawTherapee/RawTherapee"}/{pin}/{path}'
        dest.write_bytes(urllib.request.urlopen(url).read())
    digest=hashlib.sha256(dest.read_bytes()).hexdigest()
    assert digest==HASHES[project,path],f'Unexpected source SHA-256: {project}/{path}: {digest}'
    print(project,path,digest)
    return dest.read_text()
shim=r'''
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <math.h>
#include <float.h>
#include <assert.h>
#include <algorithm>
using std::min;using std::max;
#define DT_OMP_FOR(...)
#define restrict __restrict__
#define DT_OMP_PRAGMA(...)
#define DT_OMP_DECLARE_SIMD(...)
#define DT_OMP_SIMD(...)
#define MIN(a,b) ((a)<(b)?(a):(b))
#define MAX(a,b) ((a)>(b)?(a):(b))
#define CLAMPS(a,b,c) fminf(c,fmaxf(b,a))
#define CLAMPF(a,b,c) fminf(c,fmaxf(b,a))
#define FC(row,col,filters) ((filters >> ((((row) << 1 & 14) + ((col) & 1)) << 1)) & 3)
#define FCNxtrans(y,x,xt) (xt[((y)%6+6)%6][((x)%6+6)%6])
#define dt_print(...)
#define DT_DEBUG_ALWAYS 0
#define dt_alloc_align_float(n) ((float*)calloc(n,sizeof(float)))
#define dt_free_align free
#define dt_iop_image_copy(o,i,n) memcpy(o,i,(n)*sizeof(float))
typedef int gboolean;
typedef int dt_iop_demosaic_lmmse_t;
#define DT_LMMSE_REFINE_2 2
#define DT_LMMSE_TILESIZE 136
#define DT_MARKESTEIJN_TS 122
typedef float dt_aligned_pixel_t[4];
static float sqrf(float v){return v*v;}
static char *dt_alloc_perthread(size_t n,size_t,size_t *sz){*sz=n;return (char*)calloc(n,1);}
static char *dt_get_perthread(char *p,size_t){return p;}
static float median9f(const float *p){float s[9];memcpy(s,p,36);std::sort(s,s+9);return s[4];}
static void demosaic_ppg(float*,const float*,int,int,uint32_t,float,int){}
static void _vng_lininterpolate(float*,const float*,int,int,uint32_t,const uint8_t (*)[6],int){}
static float noise(unsigned i){return (float)(((i*1664525u+1013904223u)>>8)&65535)/65536.f;}
'''
# Retain the upstream working-tile sizes; they are algorithm/cache constants,
# not a tuning choice for this PC. The complete header remains reference data.
header=source('dt',DT,'src/common/darktable.h')
for name in ['DT_LMMSE_TILESIZE','DT_MARKESTEIJN_TS']:
    value=re.search(r'#define '+name+r' (\d+)',header).group(1)
    shim=re.sub(r'#define '+name+r' \d+', '#define '+name+' '+value,shim)
# Remove upstream compiler fast-math pragmas: reference must use scalar semantics.
def scalar(s):return re.sub(r'#ifdef __GNUC__.*?#endif','',s,flags=re.S)
def build(name,body,main):
    path=work/(name+'.cc');path.write_text(shim+body+main)
    exe=work/name
    subprocess.run(['g++','-O2','-ffp-contract=off','-U__SSE2__',str(path),'-o',str(exe)],check=True)
    subprocess.run([str(exe)],check=True)
for name in ['markesteijn','lmmse','dcb']:(fixtures/name).mkdir(exist_ok=True)
xt=source('dt',DT,'src/iop/demosaicing/xtrans.c');xt=xt[:xt.index('#define TS DT_FDC_TS')]
main=r'''
int main(){const int w=257,h=251;float in[w*h],out[w*h*4];
const uint8_t base[6][6]={{1,1,0,1,1,2},{1,1,2,1,1,0},{2,0,1,0,2,1},{1,1,2,1,1,0},{1,1,0,1,1,2},{0,2,1,2,0,1}};
for(int i=0;i<w*h;i++)in[i]=noise(i);
for(int dy=0;dy<6;dy++)for(int dx=0;dx<6;dx++) {uint8_t xt[6][6];for(int y=0;y<6;y++)for(int x=0;x<6;x++)xt[y][x]=base[(y+dy)%6][(x+dx)%6];
memset(out,0,sizeof(out));xtrans_markesteijn_interpolate(out,in,w,h,xt,3,9);
char path[256];sprintf(path,"crates/lc-raw/tests/fixtures/markesteijn/%d-%d.f32",dx,dy);FILE*f=fopen(path,"wb");
for(int y=20;y<h-20;y+=7)for(int x=20;x<w-20;x+=7)fwrite(out+4*(y*w+x),4,3,f);fclose(f);}}
'''
build('markesteijn',xt,main)
lm=scalar(source('dt',DT,'src/iop/demosaicing/lmmse.c'))
main=r'''
int main(){const int w=128,h=128;float in[w*h],out[w*h*4];for(int i=0;i<w*h;i++)in[i]=noise(i);
const char*names[]={"RGGB","BGGR","GRBG","GBRG"};const uint32_t fs[]={0x94949494,0x16161616,0x61616161,0x49494949};
for(int p=0;p<4;p++){memset(out,0,sizeof(out));lmmse_demosaic(out,in,w,h,fs[p],2,1.f);
char path[256];sprintf(path,"crates/lc-raw/tests/fixtures/lmmse/%s.f32",names[p]);FILE*f=fopen(path,"wb");for(int y=24;y<104;y+=3)for(int x=24;x<104;x+=3)fwrite(out+4*(y*w+x),4,3,f);fclose(f);}}
'''
build('lmmse',lm,main)
rt=source('rt',RT,'rtengine/demosaic_algos.cc')
# Use the unchanged DCB numerical kernels, with a storage/phase adapter. Borders
# are measured separately; the reference comparison samples the interior.
body=rt[rt.index('// First pass green interpolation'):rt.index('// image refinement')]
body=body.replace('RawImageSource::','').replace('FC(', 'CFA(')
body=re.sub(r'/\*.*?\*/','',body,flags=re.S)
body=re.sub(r'//[^\n]*','',body)
prefix=r'''
#define CACHESIZE 212
#define TILEBORDER 10
static unsigned pattern[2][2];
#define CFA(r,c) pattern[((r)%2+2)%2][((c)%2+2)%2]
static void dcb_initTileLimits(int&xmin,int&ymin,int&xmax,int&ymax,int,int,int b){xmin=ymin=b;xmax=ymax=CACHESIZE-b;}
'''
main=r'''
int main(){const int w=128,h=128;float in[w*h];for(int i=0;i<w*h;i++)in[i]=noise(i);
const char*names[]={"RGGB","BGGR","GRBG","GBRG"};const unsigned pats[4][2][2]={{{0,1},{1,2}},{{2,1},{1,0}},{{1,0},{2,1}},{{1,2},{0,1}}};
for(int p=0;p<4;p++){memcpy(pattern,pats[p],sizeof(pattern));float image[212*212][3]={};float saved[212*212][2];uint8_t map[212*212]={};
for(int y=0;y<212;y++)for(int x=0;x<212;x++){int yy=abs(y-10),xx=abs(x-10);if(yy>=h)yy=2*h-yy-2;if(xx>=w)xx=2*w-xx-2;image[y*212+x][CFA(yy,xx)]=in[yy*w+xx];saved[y*212+x][0]=image[y*212+x][0];saved[y*212+x][1]=image[y*212+x][2];}
dcb_hid(image,0,0);for(int k=0;k<3;k++){for(int j=0;j<3;j++)dcb_hid2(image,0,0);dcb_map(image,map,0,0);dcb_correction(image,map,0,0);}
dcb_color(image,0,0);dcb_pp(image,0,0);dcb_map(image,map,0,0);dcb_correction2(image,map,0,0);dcb_map(image,map,0,0);dcb_correction(image,map,0,0);dcb_color(image,0,0);
for(int k=0;k<2;k++){dcb_map(image,map,0,0);dcb_correction(image,map,0,0);}
for(int i=0;i<212*212;i++){image[i][0]=saved[i][0];image[i][2]=saved[i][1];}dcb_color(image,0,0);
char path[256];sprintf(path,"crates/lc-raw/tests/fixtures/dcb/%s.f32",names[p]);FILE*f=fopen(path,"wb");for(int y=24;y<104;y+=3)for(int x=24;x<104;x+=3){float v[3];for(int c=0;c<3;c++)v[c]=fmaxf(0.f,image[(y+10)*212+x+10][c]);fwrite(v,4,3,f);}fclose(f);}}
'''
build('dcb',prefix+body,main)
