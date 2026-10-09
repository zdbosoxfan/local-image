#!/usr/bin/env python3
"""Extract pinned darktable numeric kernels, compile outside Cargo, emit small reference CSVs."""
import argparse
import re
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('--sources', type=Path, default=Path('/home/zdavidson/.local/share/local-image-dev/engine-sources/dt-src'))
args = parser.parse_args()
root = Path(__file__).resolve().parents[4]
out = Path(__file__).resolve().parent
src = {n: (args.sources / n).read_text() for n in ['denoiseprofile.c', 'eaw.c', 'math.h', 'hazeremoval.c', 'guided_filter.c', 'box_filters.cc']}

def extract(file, name):
    s = src[file]
    pattern = re.compile(r'^(?:static\s+)?(?:inline\s+)?[\w *]+\b' + name + r'\s*\(', re.M)
    m = pattern.search(s)
    if not m:
        raise ValueError(name)
    i = s.index('{', m.end())
    j = i + 1
    depth = 1
    while depth:
        if s[j] == '{': depth += 1
        elif s[j] == '}': depth -= 1
        j += 1
    return s[m.start():j] + '\n'

header = r'''
#ifdef __cplusplus
#define restrict __restrict__
#endif
#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <float.h>
#include <assert.h>
#include <sys/types.h>
#define MIN(a,b) ((a)<(b)?(a):(b))
#define MAX(a,b) ((a)>(b)?(a):(b))
#define CLAMP(x,a,b) MIN(MAX(x,a),b)
#define DT_OMP_FOR(...)
#define DT_OMP_SIMD(...)
#define DT_OMP_FOR_SIMD(...)
#define DT_OMP_PRAGMA(...)
#define DT_ALIGNED_ARRAY
#define PREFETCH_NTA(...)
#define for_each_channel(c,...) for(int c=0;c<3;c++)
#define for_four_channels(c,...) for(int c=0;c<4;c++)
#define TRUE 1
#define FALSE 0
#define BOXFILTER_KAHAN_SUM 256
#define MAX_VECT 16
#define dt_omploop_sfence()
typedef int gboolean;
typedef float dt_aligned_pixel_t[4];
typedef float dt_colormatrix_t[4][4];
static void copy_pixel_nontemporal(float *d,const float *s){memcpy(d,s,4*sizeof(float));}
static int dwt_interleave_rows(int r,int h,int m){return r;}
static void dt_vector_min(float *out,const float *a,const float *b){for(int c=0;c<4;c++)out[c]=MIN(a[c],b[c]);}
static void dt_vector_max(float *out,const float *a,const float *b){for(int c=0;c<4;c++)out[c]=MAX(a[c],b[c]);}
static void dt_vector_exp(const float *a,float *out){for(int c=0;c<4;c++)out[c]=expf(a[c]);}
static void dt_apply_transposed_color_matrix(const float *v,const dt_colormatrix_t m,float *o){for(int c=0;c<3;c++)o[c]=m[0][c]*v[0]+m[1][c]*v[1]+m[2][c]*v[2];o[3]=0;}
static float noise(int x,int y,int k){uint32_t v=(uint32_t)x*0x8da6b343u^(uint32_t)y*0xd8163841u^(uint32_t)k*0xcb1ab31fu;v^=v>>13;v*=0x5bd1e995u;v^=v>>15;return (v&65535)/32768.f-1.f;}
'''
nrdata = r'''
#define DT_IOP_DENOISE_PROFILE_BANDS 7
#define MODE_RGB 0
#define DT_DENOISE_PROFILE_ALL 0
#define DT_DENOISE_PROFILE_R 1
#define DT_DENOISE_PROFILE_G 2
#define DT_DENOISE_PROFILE_B 3
#define DT_DENOISE_PROFILE_Y0 4
#define DT_DENOISE_PROFILE_U0V0 5
typedef struct {float force[6][7];int wavelet_color_mode;} dt_iop_denoiseprofile_data_t;
'''
nr = header + nrdata + extract('math.h', 'fast_mexp2f')
for f in ['invert_matrix','set_up_conversion_matrices','precondition_Y0U0V0','backtransform_Y0U0V0','variance_stabilizing_xform']:
    nr += extract('denoiseprofile.c', f)
nr += re.sub(r'^#include[^\n]*\n', '', src['eaw.c'], flags=re.M)
nr += r'''
int main(void){
  dt_aligned_pixel_t wb={1,1,1,0};
  dt_colormatrix_t to={{1.f/3,1.f/3,1.f/3,0},{.5,0,-.5,0},{.25,-.5,.25,0},{0}},from={{0}},tt={{0}},ft={{0}};
  set_up_conversion_matrices(to,from,wb);
  for(int r=0;r<3;r++)for(int c=0;c<3;c++){tt[c][r]=to[r][c];ft[c][r]=from[r][c];printf("matrix,%d,%d,%.9g,%.9g\n",r,c,to[r][c],from[r][c]);}
  for(int i=0;i<15;i++){
    float in[4]={.002f+.013f*i,.008f+.017f*i,.003f+.009f*i,0},v[4],back[4];dt_aligned_pixel_t p={1.23f,1.23f,1.23f,0};
    precondition_Y0U0V0(in,v,1,1,.00027f,p,.00001f,tt);memcpy(back,v,sizeof v);
    backtransform_Y0U0V0(back,1,1,.00027f,p,.00001f,-.7f,wb,ft);
    printf("vst,%d,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g\n",i,in[0],in[1],in[2],v[0],v[1],v[2],back[0],back[1],back[2]);
  }
  for(int i=0;i<20;i++){float x=i*7.31f;printf("exp,%d,%.9g,%.9g\n",i,x,fast_mexp2f(x));}
  enum {W=37,H=29,N=W*H};float in[N*4],co[N*4],det[N*4],acc[N*4];
  for(int y=0;y<H;y++)for(int x=0;x<W;x++)for(int c=0;c<4;c++){in[(y*W+x)*4+c]=c<3?12.f+.05f*x+.03f*y+(x>=W/2?20.f:0.f)+2.f*noise(x,y,c+1):0;acc[(y*W+x)*4+c]=0;}
  for(int level=0;level<3;level++){
    dt_aligned_pixel_t sum={0},thr={0},boost={1,1,1,1};dt_iop_denoiseprofile_data_t d={0};d.wavelet_color_mode=1;
    for(int c=0;c<6;c++)for(int j=0;j<7;j++)d.force[c][j]=c==4?.43f:.67f;
    const float varf=sqrtf(70.f)/16.f;float sb=powf(varf,level);
    eaw_dn_decompose(co,in,det,sum,level,1.f/(sb*sb),W,H);
    variance_stabilizing_xform(thr,level,3,N,sum,&d);eaw_synthesize(acc,acc,det,thr,boost,W,H);
    printf("band,%d,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g\n",level,sum[0],sum[1],sum[2],thr[0],thr[1],thr[2]);
    for(int i=0;i<N;i+=43)printf("eaw,%d,%d,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g\n",level,i,co[i*4],co[i*4+1],co[i*4+2],det[i*4],det[i*4+1],det[i*4+2],acc[i*4],acc[i*4+1],acc[i*4+2]);
    memcpy(in,co,sizeof in);
  }
  return 0;
}
'''
def run(name, text, compiler, ext):
    path = root / 'target' / 'refvec' / name
    path.mkdir(parents=True,exist_ok=True)
    file = path / ('reference.' + ext)
    file.write_text(text)
    subprocess.run([compiler,'-O2','-ffp-contract=off',str(file),'-lm','-o',str(path/'reference')],check=True)
    data=subprocess.check_output([str(path/'reference')])
    (out/('detail-'+name+'.csv')).write_bytes(data)
    print(name, len(data), 'bytes')
run('nr', nr, 'gcc', 'c')

# C++ templates are extracted unchanged; no project headers/runtime or C in the Cargo build.
alloc = r'''
static size_t dt_round_size(size_t x,size_t a){return (x+a-1)/a*a;}
static float *dt_alloc_align_float(size_t n){return (float*)calloc(n,sizeof(float));}
static float *dt_alloc_perthread_float(size_t n,size_t *sz){*sz=n;return dt_alloc_align_float(n);}
static float *dt_get_perthread(float *v,size_t n){return v;}
static void dt_free_align(void *v){free(v);}
'''
box = src['box_filters.cc']
box = box[box.index('template <size_t N, bool compensated = false>'):box.index('void dt_box_mean(')]
wrappers = r'''
static void dt_box_min(float *p,int h,int w,int nc,int r){_box_min_1ch(p,h,w,r);}
static void dt_box_max(float *p,int h,int w,int nc,int r){_box_max_1ch(p,h,w,r);}
static void dt_box_mean_horizontal(float *p,int w,int nc,int r,float *s){switch(nc&255){case 1:_blur_horizontal<1,true>(p,w,r,s);break;case 4:_blur_horizontal<4,true>(p,w,r,s);break;case 9:_blur_horizontal<9,true>(p,w,r,s);break;}}
static void dt_box_mean_vertical(float *p,int h,int w,int nc,int r){size_t sz;float *s=_alloc_scratch_space(nc&255,h,w,r,&sz);_blur_vertical_1ch<true>(p,h,w*(nc&255),r,s,sz);free(s);}
static void dt_box_mean(float *p,int h,int w,int nc,int r,int it){switch(nc&255){case 1:_box_mean<1,true>(p,h,w,r,it);break;case 4:_box_mean<4,true>(p,h,w,r,it);break;case 9:_box_mean<9,true>(p,h,w,r,it);break;}}
typedef struct {float *data;int width,height;} gray_image;
static gray_image new_gray_image(int w,int h){return gray_image{dt_alloc_align_float(w*h),w,h};}
static void free_gray_image(gray_image *p){free(p->data);}
static void copy_gray_image(gray_image a,gray_image b){memcpy(b.data,a.data,a.width*a.height*sizeof(float));}
'''
gf = src['guided_filter.c']
gf = gf[gf.index('#define GF_TILE_SIZE'):gf.index('#ifdef HAVE_OPENCL', gf.index('void guided_filter('))]
haze = header + alloc + box + wrappers + gf
haze += r'''
typedef dt_aligned_pixel_t rgb_pixel;
typedef struct {const float *data;int width,height,stride;} const_rgb_image;
'''
for f in ['_pointer_swap_f','_dark_channel','_transition_map','_partition','_quick_select','_ambient_light']:
    haze += extract('hazeremoval.c',f)
haze += r'''
int main(void){
  enum{W=37,H=29,N=W*H};float in[N*4],trans[N],refined[N],box[N];
  for(int y=0;y<H;y++)for(int x=0;x<W;x++)for(int c=0;c<4;c++)in[(y*W+x)*4+c]=c<3?.15f+.012f*x+.004f*y+.03f*c+(x>W/2?.12f:0.f)+.008f*noise(x,y,c+1):0;
  const_rgb_image img={in,W,H,4};rgb_pixel air={0};const float distance=_ambient_light(img,6,&air,FALSE);
  printf("air,%.9g,%.9g,%.9g,%.9g\n",air[0],air[1],air[2],distance);
  for(int sign=-1;sign<=1;sign+=2){
    const float strength=sign*.7f;gray_image p={trans,W,H};
    _transition_map(img,p,6,air,strength);dt_box_min(trans,H,W,1,6);
    guided_filter(in,trans,refined,W,H,4,9,sqrtf(.025f),1.f,-FLT_MAX,FLT_MAX);
    const float tmin=CLAMP(expf(-.7f*distance),1.f/1024.f,1.f);
    for(int i=0;i<N;i+=31){float t=MAX(refined[i],tmin);printf("haze,%d,%d,%.9g,%.9g,%.9g,%.9g,%.9g\n",sign,i,trans[i],refined[i],(in[i*4]-air[0])/t+air[0],(in[i*4+1]-air[1])/t+air[1],(in[i*4+2]-air[2])/t+air[2]);}
  }
  for(int i=0;i<N;i++)box[i]=.03f*(i%17)-.2f;
  dt_box_mean(box,H,W,1,5,1);
  for(int i=0;i<N;i+=31)printf("box,%d,%.9g\n",i,box[i]);
  return 0;
}
'''
run('haze',haze,'g++','cc')
