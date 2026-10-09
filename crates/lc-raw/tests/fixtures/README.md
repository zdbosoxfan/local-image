# Independent upstream reference vectors

Source: darktable `733bd69f32cac7ff5e41025115942772add1f088` and RawTherapee
`5f486d3678b34c74ba0c63571c17babe20935019`, GPL-3.0-or-later.
Upstream remains outside the worktree. These are deterministic little-endian IEEE floats,
not image assets or client photographs. No C/C++ is compiled by Cargo.

Tests cover all four Bayer phases for VNG (64x64: full, linear, linear + two medians), AMaZE
(255x251: sparse samples including borders and 128-pixel tile boundaries), RCD (128x128 interior),
the dual mask (64x64), capture-radius scans (128x128), and segmentation (192x180: four Bayer
phases plus X-Trans, all seven recovery modes, radius 8, Poisson noise, and non-neutral WB).
AMaZE also has independent RawTherapee scalar outputs (`*-rt.f32`). Its negative-output clamp
is accounted for in the tight 2e-6 assertion; unclamped differences are measured separately.

Tolerances: VNG and new dual mask 2e-7; AMaZE, segmentation, site-corrected RCD and channel-corrected
opposed 2e-6; capture radius 2e-7. Tests print measured maximum/RMS discrepancies. Radius fixtures
include infinities for flat input. `rcd/*-original.f32` and `opposed/original.f32` retain the
unmodified upstream outputs to **measure** the documented algorithm differences, rather than
label them rounding error. `*-site` changes only the diagonal CFA parity in RCD's half-width
high-pass packing. `opposed/corrected` changes only the clipped-channel test to read its channel.
The opposed crop includes a clipped blue/HDR green disc with red remaining unclipped to
exercise the per-channel mask difference. It is divisible by 3 and its clipped region is well inside the border, so this
fixture is unaffected by the historical expanded edge mask and rounded-up grid differences.

Regenerate from the repository root. Requirements: Python 3, GCC/G++, a read-only pinned darktable
source trees at the task's reference paths. Save each Python block below at its indicated path.
The extraction code supplies numeric host types, disables OpenMP/GUI caching, and uses zeroed
allocation to make unwritten scratch borders deterministic. Segmentation's C `goto finish` is
replaced by an equivalent enclosing `if` for C++ compatibility. Its Gaussian and box-filter
kernels are extracted unchanged; the Scharr shim uses the fast-math square-root form. The RCD
PPG border stub is intentionally unused by the sampled interior (16-pixel exclusion).
The AMaZE harness uses upstream's original shared scratch allocation, not independent arrays.
The highlight API takes an effective normalized clip threshold. The shim uses module magic 1
so the kernel receives .99, matching the Rust input; darktable's UI first multiplies its clip
slider by .987 for these modes. This host parameter convention is not an omitted kernel step.

```sh
mkdir -p target/refvec/{vng,amaze,amaze-rt,segbased,rcd,dual,capture-radius,capture-rl,opposed}
mkdir -p crates/lc-raw/tests/fixtures/{vng,amaze,segbased,rcd,dual,capture-radius,opposed}
mkdir -p crates/lc-pipeline/tests/fixtures/capture-rl
python3 target/refvec/vng/build.py
python3 target/refvec/amaze/build.py
python3 target/refvec/amaze-rt/build.py
python3 target/refvec/segbased/build.py
python3 target/refvec/raw-build.py
python3 target/refvec/opposed/build.py
gcc -O2 -ffp-contract=off target/refvec/vng/reference.c -lm -o target/refvec/vng/reference
g++ -O2 -ffp-contract=off target/refvec/amaze/reference.cc -o target/refvec/amaze/reference
g++ -O2 -ffp-contract=off -U__SSE2__ target/refvec/amaze-rt/reference.cc -o target/refvec/amaze-rt/reference
g++ -O2 -ffp-contract=off -fpermissive target/refvec/segbased/reference.cc -o target/refvec/segbased/reference
for name in vng amaze amaze-rt segbased; do target/refvec/$name/reference; done
for name in rcd dual capture-radius capture-rl; do
  gcc -O2 -ffp-contract=off target/refvec/$name/reference.c -lm -o target/refvec/$name/reference
  target/refvec/$name/reference
done
gcc -O2 -ffp-contract=off target/refvec/rcd/adapted.c -lm -o target/refvec/rcd/adapted
target/refvec/rcd/adapted
for name in reference corrected; do
  g++ -O2 -ffp-contract=off -fpermissive target/refvec/opposed/$name.cc -o target/refvec/opposed/$name
  target/refvec/opposed/$name
done
PATH="$HOME/.cargo/bin:$PATH" CARGO_INCREMENTAL=0 cargo +1.98.1 test --offline -p lightcraft-raw --lib -- --nocapture
```

## `target/refvec/vng/build.py`

```python
from pathlib import Path
src=Path('/home/zdavidson/.local/share/local-image-dev/engine-sources/dt/src/iop/demosaicing/vng.c').read_text().split('#ifdef HAVE_OPENCL')[0]
basics=Path('/home/zdavidson/.local/share/local-image-dev/engine-sources/dt/src/iop/demosaicing/basics.c').read_text()
smooth=basics[basics.index('#define SWAPmed'):basics.index('#undef SWAP')]
shim='''#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <math.h>
#include <limits.h>
#include <stddef.h>
typedef int gboolean;
typedef float dt_aligned_pixel_t[4];
#define DT_OMP_FOR(...)
#define FILTERS_ARE_4BAYER(f) 0
#define FC(row,col,filters) ((filters >> ((((row) << 1 & 14) + ((col) & 1)) << 1)) & 3)
static int fcol(int r,int c,uint32_t f,const uint8_t x[6][6]) { return FC(r,c,f); }
static void dt_vector_clipneg(float *p) { for(int c=0;c<4;c++) p[c]=fmaxf(0,p[c]); }
#define dt_alloc_aligned malloc
#define dt_free_align free
#define dt_print(...)
#define dt_iop_image_copy(o,i,n) memcpy(o,i,(n)*sizeof(float))
#define SWAP(a,b) {float tmp=b; b=a; a=tmp;}
'''
main='''
int main() {
 const int w=64,h=64;
 float in[w*h], out[w*h*4];
 for(int i=0;i<w*h;i++) { uint32_t n=(uint32_t)i*1664525u+1013904223u; in[i]=(float)((n>>8)&65535)/65536.f; }
 const char* names[]={"RGGB","BGGR","GRBG","GBRG"};
 const uint32_t fs[]={0x94949494,0x16161616,0x61616161,0x49494949};
 for(int f=0;f<4;f++) for(int mode=0;mode<3;mode++) {
  memset(out,0,sizeof(out));
  vng_interpolate(out,in,w,h,fs[f],NULL,mode!=0);
  if(mode==2)color_smoothing(out,w,h,2);
  char path[256]; sprintf(path,"crates/lc-raw/tests/fixtures/vng/%s-%d.f32",names[f],mode);
  FILE *fp=fopen(path,"wb");
  for(int i=0;i<w*h;i++)fwrite(out+4*i,sizeof(float),3,fp);
  fclose(fp);
 }
}
'''
Path('target/refvec/vng/reference.c').write_text(shim+src+smooth+main)
```

## `target/refvec/amaze/build.py`

```python
from pathlib import Path
src=Path('/home/zdavidson/.local/share/local-image-dev/engine-sources/dt/src/iop/demosaicing/amaze.cc').read_text()
src=src[src.index('static inline float _clampnan'):].replace('G_END_DECLS','')
shim='''#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <cstdio>
#include <algorithm>
#define AMAZETS 160
#define DT_OMP_PRAGMA(...)
#define FC(row,col,filters) ((filters >> ((((row) << 1 & 14) + ((col) & 1)) << 1)) & 3)
#define MAX(a,b) ((a)>(b)?(a):(b))
#define MIN(a,b) ((a)<(b)?(a):(b))
static float sqrf(float x){return x*x;}
static float interpolatef(float a,float b,float c){return a*(b-c)+c;}
'''
main='''
int main() {
 const int w=255,h=251;
 float in[w*h], out[w*h*4];
 for(int i=0;i<w*h;i++) {uint32_t n=(uint32_t)i*1664525u+1013904223u; in[i]=(float)((n>>8)&65535)/65536.f;}
 const char* names[]={"RGGB","BGGR","GRBG","GBRG"};
 const uint32_t fs[]={0x94949494,0x16161616,0x61616161,0x49494949};
 for(int f=0;f<4;f++) {
 memset(out,0,sizeof(out)); amaze_demosaic(in,out,w,h,fs[f],1.f);
 char path[256];sprintf(path,"crates/lc-raw/tests/fixtures/amaze/%s.f32",names[f]);FILE *fp=fopen(path,"wb");
 // Sparse regular sample includes all four edges, all phases, and 128-pixel tile boundaries.
 for(int y=0;y<h;y++)for(int x=0;x<w;x++)if(x%17<2 || y%17<2 || x>=126 && x<=130 || y>=126 && y<=130)fwrite(out+4*(y*w+x),sizeof(float),3,fp);
 fclose(fp);
 }
}
'''
Path('target/refvec/amaze/reference.cc').write_text(shim+src+main)
```

## `target/refvec/segbased/build.py`

```python
from pathlib import Path
import re
root=Path('/home/zdavidson/.local/share/local-image-dev/engine-sources/dt/src')
shim=r'''
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <math.h>
#include <float.h>
#include <algorithm>
#define DT_OMP_FOR(...)
#define DT_OMP_PRAGMA(...)
#define DT_OMP_DECLARE_SIMD(...)
#define DT_OMP_SIMD(...)
#define DT_ALIGNED_ARRAY
#define MAX_VECT 16
#define MAX(a,b) ((a)>(b)?(a):(b))
#define MIN(a,b) ((a)<(b)?(a):(b))
#define ABS(a) abs(a)
#define CLAMPF(a,b,c) fminf(c,fmaxf(b,a))
#define DT_2PI_F 6.283185307179586f
#define DT_DISTANCE_TRANSFORM_MAX 1e20f
#define DT_DISTANCE_TRANSFORM_NONE 0
#define DT_DISTANCE_TRANSFORM_MASK 1
#define TRUE 1
#define FALSE 0
#define NUM_RECOVERY_MODES 7
#define DT_RECOVERY_MODE_ADAPT 5
#define DT_RECOVERY_MODE_OFF 0
#define DT_IOP_HIGHLIGHTS_SEGMENTS 0
#define DT_HIGHLIGHTS_MASK_OFF 0
#define DT_HIGHLIGHTS_MASK_COMBINE 1
#define DT_HIGHLIGHTS_MASK_CANDIDATING 2
#define DT_HIGHLIGHTS_MASK_STRENGTH 3
#define FC(row,col,filters) ((filters >> ((((row) << 1 & 14) + ((col) & 1)) << 1)) & 3)
#define for_three_channels(c) for(int c=0;c<3;c++)
#define for_each_channel(c) for(int c=0;c<4;c++)
#define dt_print(...)
#define dt_print_pipe(...)
#define STR_YESNO(x) ""
#define dt_iop_gui_enter_critical_section(...)
#define dt_iop_gui_leave_critical_section(...)
#define dt_alloc_align_type(t,n) ((t*)calloc(n,sizeof(t)))
#define dt_alloc_align_float(n) ((float*)calloc(n,sizeof(float)))
#define dt_alloc_align_int(n) ((int*)calloc(n,sizeof(int)))
#define dt_calloc_aligned(n) calloc(n,1)
#define dt_alloc_aligned(n) calloc(n,1)
#define dt_free_align free
#define dt_iop_image_copy(o,i,n) memcpy(o,i,(n)*sizeof(float))
typedef int gboolean;
typedef int dt_hash_t;
typedef int dt_distance_transform_t;
typedef float dt_aligned_pixel_t[4];
struct dt_iop_roi_t {int x,y,width,height;};
struct dt_dev_chroma_t {int late_correction;double D65coeffs[3],as_shot[3];};
struct dev_t_ {dt_dev_chroma_t chroma;int gui_attached;};
struct dt_iop_module_t {void *gui_data;dev_t_ *dev;};
struct dt_iop_buffer_dsc_t {struct {int enabled;float coeffs[3];} temperature;};
struct pipe_t_ {dt_iop_buffer_dsc_t dsc;int iwidth,iheight;float iscale;};
struct dt_dev_pixelpipe_iop_t {uint8_t xtrans[6][6];uint32_t filters;pipe_t_ *pipe;dt_iop_module_t *module;};
struct dt_iop_highlights_gui_data_t {int opphash,oppclipped,oppresult;float oppchroma[3];};
struct dt_iop_highlights_data_t {float clip,strength,candidating,noise_level;int recovery,combine;};
static float highlights_clip_magics[]={1.f};
static float sqrf(float x){return x*x;}
static float fcube(float x){return x*x*x;}
static int feqf(float x,float y,float e){return fabsf(x-y)<e;}
static int fcol(int y,int x,uint32_t f,const uint8_t xt[6][6]){return f==9?xt[y%6][x%6]:FC(y,x,f);}
static int dt_pipe_is_full(void*){return 0;}
static int dt_get_num_threads(){return 1;}
static int dt_get_thread_num(){return 0;}
static int _opposed_hash(void*){return 0;}
static size_t dt_round_size(size_t n,size_t a){return (n+a-1)/a*a;}
static void dt_iop_image_fill(float *p,float v,int w,int h,int ch){for(int i=0;i<w*h*ch;i++)p[i]=v;}
static float *dt_alloc_perthread_float(size_t n,size_t *sz){*sz=n;return dt_alloc_align_float(n);}
static void *dt_get_perthread(void *p,size_t){return p;}
static void dt_iop_copy_image_roi(float *o,const float*i,int, const dt_iop_roi_t*r,const dt_iop_roi_t*){memcpy(o,i,r->width*r->height*sizeof(float));}
static float scharr_gradient(const float *p,int w){const float gx=47.f/255.f*(p[-w-1]-p[-w+1]+p[w-1]-p[w+1])+162.f/255.f*(p[-1]-p[1]);const float gy=47.f/255.f*(p[-w-1]-p[w-1]+p[-w+1]-p[w+1])+162.f/255.f*(p[-w]-p[w]);return sqrtf(gx*gx+gy*gy);}
'''
seg=(root/'iop/hlreconstruct/segmentation.c').read_text()
gauss=(root/'common/gaussian.c').read_text()
gauss=gauss[gauss.index('static void _calc_9x9_gauss_coeffs'):gauss.index('size_t dt_gaussian_memory_use')]+gauss[gauss.index('static void _fast_9x9_kernel_1'):gauss.index('DT_OMP_DECLARE_SIMD(aligned(in, out:64))\nstatic void _fast_9x9_kernel_2')]+'''void dt_gaussian_fast_blur(float*i,float*o,int w,int h,float s,float lo,float hi,int){_fast_9x9_kernel_1(i,o,w,h,s,lo,hi);}\n'''
box=(root/'common/box_filters.cc').read_text();box=box[box.index('template <size_t N, bool compensated = false>'):box.index('static inline float _window_max')]+'''void dt_box_mean(float*b,size_t h,size_t w,int,size_t r,uint32_t it){_box_mean<1>(b,h,w,r,it);}\n'''
noise=(root/'develop/noise_generator.h').read_text();noise=noise[noise.index('static inline uint32_t splitmix32'):noise.index('DT_OMP_DECLARE_SIMD(uniform(distribution, param)')]
dist=(root/'common/distance_transform.c').read_text();dist=dist[dist.index('static void _image_distance_transform'):dist.index('// clang-format off')]
segb=(root/'iop/hlreconstruct/segbased.c').read_text()
ref=segb[segb.index('static inline float _calc_refavg'):segb.index('static void _initial_gradients')]
opp=(root/'iop/hlreconstruct/opposed.c').read_text();opp=opp[opp.index('static inline size_t _raw_to_cmap'):opp.index('// A slightly modified version')]+opp[opp.index('static float *_process_opposed'):opp.index('#ifdef HAVE_OPENCL')]
segb=segb[segb.index('#define HL_RGB_PLANES'):];segb=segb.replace(ref,'')
# C++ forbids jumping across initializers: upstream C finish-goto becomes a flag and encloses processing.
segb=segb.replace('if((anyclipped < 20) && vmode == DT_HIGHLIGHTS_MASK_OFF)\n    goto finish;', 'if(anyclipped >= 20 || vmode != DT_HIGHLIGHTS_MASK_OFF) {').replace('  finish:', '  }')
main=r'''
int main(){
 const int w=192,h=180;
 float in[w*h],out[w*h];
 const char*names[]={"RGGB","BGGR","GRBG","GBRG","XTRANS"};const uint32_t fs[]={0x94949494,0x16161616,0x61616161,0x49494949,9};
 const uint8_t xt[6][6]={{1,1,0,1,1,2},{1,1,2,1,1,0},{2,0,1,0,2,1},{1,1,2,1,1,0},{1,1,0,1,1,2},{0,2,1,2,0,1}};
 dev_t_ dev={};dt_iop_module_t mod={NULL,&dev};pipe_t_ pipe={};pipe.iwidth=w;pipe.iheight=h;pipe.iscale=1;pipe.dsc.temperature.enabled=1;
 for(int c=0;c<3;c++)pipe.dsc.temperature.coeffs[c]=1;
 dt_dev_pixelpipe_iop_t piece={};piece.pipe=&pipe;piece.module=&mod;memcpy(piece.xtrans,xt,sizeof(xt));dt_iop_roi_t roi={0,0,w,h};
 for(int pat=0;pat<5;pat++)for(int caseidx=0;caseidx<9;caseidx++){
 const int mode=caseidx<7?caseidx:caseidx==7?0:6;
 const float wb[3]={caseidx<7?1.f:1.7f,caseidx<7?1.f:.85f,caseidx<7?1.f:2.3f};
 for(int c=0;c<3;c++)pipe.dsc.temperature.coeffs[c]=wb[c];
 piece.filters=fs[pat];
 for(int y=0;y<h;y++)for(int x=0;x<w;x++){
 const int c=fcol(y,x,fs[pat],xt);const float v=.22f+.005f*x+.001f*y;
 // Broad smooth channel clipping, an all-channel clipped disc, isolated clipped specks and borders.
 const int disc=(x-90)*(x-90)+(y-80)*(y-80)<40*40;
 in[y*w+x]=disc?1.06f:MIN(1.f,v*(c==0?1.4f:c==1?1.f:.8f));
 if((x*13+y*7)%331==0)in[y*w+x]=1.06f;
 in[y*w+x]*=wb[c];
 }
 dt_iop_highlights_data_t d={.99f,.2f,.4f,mode==6?.01f:0.f,mode,mode==4?8:2};
 float*tmp=_process_opposed(&mod,&piece,in,out,&roi,&roi,1,d.clip);_process_segmentation(&piece,in,out,&roi,&roi,&d,0,tmp);free(tmp);
 char path[256];sprintf(path,"crates/lc-raw/tests/fixtures/segbased/%s-%d.f32",names[pat],caseidx);FILE*fp=fopen(path,"wb");
 for(int y=0;y<h;y+=3)for(int x=0;x<w;x+=3){float v=out[y*w+x]/wb[fcol(y,x,fs[pat],xt)];fwrite(&v,sizeof(float),1,fp);}fclose(fp);
 }
}
'''
Path('target/refvec/segbased/reference.cc').write_text(shim+seg+gauss+box+noise+dist+ref+opp+segb+main)
```

## `target/refvec/raw-build.py`

```python
from pathlib import Path
import re
root=Path('/home/zdavidson/.local/share/local-image-dev/engine-sources/dt/src')
# Common C-only numeric host shim; no code participates in the Cargo build.
shim=r'''
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <math.h>
#include <float.h>
#include <limits.h>
#define DT_ALIGNED_PIXEL
#define DT_OMP_FOR(...)
#define DT_OMP_PRAGMA(...)
#define DT_OMP_DECLARE_SIMD(...)
#define DT_OMP_SIMD(...)
#define DT_OMP_FOR_SIMD(...)
#define for_each_channel(c) for(int c=0;c<4;c++)
#define for_three_channels(c) for(int c=0;c<3;c++)
#define MAX(a,b) ((a)>(b)?(a):(b))
#define MIN(a,b) ((a)<(b)?(a):(b))
#define ABS(a) abs(a)
#define CLAMP(a,b,c) MIN(c,MAX(b,a))
#define CLAMPF(a,b,c) fminf(c,fmaxf(b,a))
#define CLIP(a) CLAMPF(a,0.f,1.f)
#define FC(row,col,filters) ((filters >> ((((row) << 1 & 14) + ((col) & 1)) << 1)) & 3)
#define FCNxtrans(y,x,xt) (xt[(y)%6][(x)%6])
#define TRUE 1
#define FALSE 0
#define dt_print(...)
#define dt_alloc_align_float(n) ((float*)calloc(n,sizeof(float)))
#define dt_calloc_align_float(n) ((float*)calloc(n,sizeof(float)))
#define dt_free_align free
#define dt_iop_image_alloc(w,h,ch) dt_alloc_align_float((w)*(h)*(ch))
typedef int gboolean;
typedef float dt_aligned_pixel_t[4];
static float sqrf(float x){return x*x;}
static float interpolatef(float a,float b,float c){return a*(b-c)+c;}
static float scharr_gradient(const float *p,int w){const float gx=47.f/255.f*(p[-w-1]-p[-w+1]+p[w-1]-p[w+1])+162.f/255.f*(p[-1]-p[1]);const float gy=47.f/255.f*(p[-w-1]-p[w-1]+p[-w+1]-p[w+1])+162.f/255.f*(p[-w]-p[w]);return hypotf(gx,gy);}
static float fcube(float x){return x*x*x;}
static float dt_fast_expf(float x){int k=0x3f800000u+x*(0x402df854u-0x3f800000u);union {float f;int k;}u;u.k=k>0?k:0;return u.f;}
static float noise(unsigned int i){return (float)(((i*1664525u+1013904223u)>>8)&65535)/65536.f;}
'''
def write(name,body,main):Path('target/refvec/'+name+'/reference.c').write_text(shim+body+main)
rcd=(root/'iop/demosaicing/rcd.c').read_text().split('#ifdef HAVE_OPENCL')[0];rcd=re.sub(r'#ifdef __GNUC__.*?#endif','',rcd,flags=re.S)
body='#define DT_RCD_TILESIZE 212\nstatic void demosaic_ppg(float*o,const float*i,int w,int h,uint32_t f,float t,int b){}\n'+rcd
main=r'''
int main(){const int w=128,h=128;float in[w*h],out[w*h*4];for(int i=0;i<w*h;i++)in[i]=noise(i);
const char*names[]={"RGGB","BGGR","GRBG","GBRG"};const uint32_t fs[]={0x94949494,0x16161616,0x61616161,0x49494949};
for(int f=0;f<4;f++){memset(out,0,sizeof(out));rcd_demosaic(out,in,w,h,fs[f],1.f);char path[256];sprintf(path,"crates/lc-raw/tests/fixtures/rcd/%s-original.f32",names[f]);FILE*fp=fopen(path,"wb");for(int y=16;y<112;y+=3)for(int x=16;x<112;x+=3)fwrite(out+4*(y*w+x),sizeof(float),3,fp);fclose(fp);}}
'''
write('rcd',body,main)
# Correct the documented half-width packing so each diagonal statistic is evaluated at its CFA site.
adapted=body.replace('int col = 3, indx =','int col = 3 + (FC(row, 1, filters) & 1), indx =')
Path('target/refvec/rcd/adapted.c').write_text(shim+adapted+main.replace('-original.f32','-site.f32'))
gauss=(root/'common/gaussian.c').read_text();gauss=gauss[gauss.index('static void _calc_9x9_gauss_coeffs'):gauss.index('size_t dt_gaussian_memory_use')]+gauss[gauss.index('static void _fast_9x9_kernel_1'):gauss.index('DT_OMP_DECLARE_SIMD(aligned(in, out:64))\nstatic void _fast_9x9_kernel_2')]
detail=(root/'develop/masks/detail.c').read_text();detail=detail[detail.index('float *dt_masks_calc_scharr_mask'):detail.index('float *dt_masks_calc_detail_mask')]
body='typedef struct {struct {struct {int enabled;float coeffs[3];}temperature;}dsc;} dt_dev_pixelpipe_t;\n'+detail+gauss
main=r'''
int main(){const int w=64,h=64;float in[w*h*4],tmp[w*h],out[w*h];for(int y=0;y<h;y++)for(int x=0;x<w;x++)for(int c=0;c<4;c++)in[4*(y*w+x)+c]=.2f+.003f*x+.001f*y+.015f*noise((y*w+x)*3+c)+(x>31?.08f:0.f);
dt_dev_pixelpipe_t p={0};float*mask=dt_masks_calc_scharr_mask(&p,in,w,h,1);dt_masks_calc_detail_blend(mask,tmp,w*h,.005f*powf(.2f,1.1f),1);_fast_9x9_kernel_1(tmp,out,w,h,2.f,0.f,1.f);FILE*fp=fopen("crates/lc-raw/tests/fixtures/dual/mask.f32","wb");fwrite(out,sizeof(float),w*h,fp);fclose(fp);free(mask);}
'''
write('dual',body,main)
cap=(root/'iop/demosaicing/capture.c').read_text()
body=cap[cap.index('#define RAWEPS'):cap.index('static float _calc_auto_radius')]
main=r'''
int main(){const int w=128,h=128;float in[w*h];FILE*fp=fopen("crates/lc-raw/tests/fixtures/capture-radius/ratios.f32","wb");
uint32_t fs[]={0x94949494,0x16161616,0x61616161,0x49494949};
uint8_t xt[6][6]={{1,1,0,1,1,2},{1,1,2,1,1,0},{2,0,1,0,2,1},{1,1,2,1,1,0},{1,1,0,1,1,2},{0,2,1,2,0,1}};
for(int scene=0;scene<3;scene++){for(int y=0;y<h;y++)for(int x=0;x<w;x++)in[y*w+x]=.05f+.6f*(x%23)/23.f+.0005f*noise(y*w+x);if(scene==1)for(int i=0;i<w*h;i++)if(i%37==0)in[i]=1.f;if(scene==2)for(int i=0;i<w*h;i++)in[i]=.3f;
for(int p=0;p<4;p++){float r=_calcRadiusBayer(in,w,h,fs[p]);fwrite(&r,4,1,fp);}float r=_calcRadiusMono(in,w,h);fwrite(&r,4,1,fp);r=_calcRadiusXtrans(in,w,h,xt);fwrite(&r,4,1,fp);}fclose(fp);}
'''
write('capture-radius',body,main)
body='''#define CAPTURE_KERNEL_ALIGN 32
#define CAPTURE_GAUSS_FRACTION .01f
#define CAPTURE_SMALL .66f
#define CAPTURE_YMIN .001f
'''+cap[cap.index('static inline void _calc_9x9_gauss_coeffs'):cap.index('static unsigned char *_cs_precalc')]+cap[cap.index('static inline void _blur_mul'):cap.index('static void _prepare_blend')]
main=r'''
int main(){const int w=64,h=64;float in[w*h],est[w*h],ratio[w*h],blend[w*h],kernels[256*32];unsigned char idx[w*h];
for(int k=1;k<256;k++)_calc_9x9_gauss_coeffs(kernels+k*32,k*.01f);memset(kernels,0,32*4);kernels[0]=1;
for(int i=0;i<w*h;i++){in[i]=.1f+.7f*noise(i);est[i]=in[i];ratio[i]=1.f;blend[i]=i%29?1.f:0.f;idx[i]=i%3==0?30:i%3==1?70:150;}
for(int k=0;k<8;k++){_blur_div(est,ratio,in,blend,kernels,idx,w,h);_blur_mul(ratio,est,blend,kernels,idx,w,h);}FILE*fp=fopen("crates/lc-pipeline/tests/fixtures/capture-rl/estimate.f32","wb");fwrite(est,4,w*h,fp);fclose(fp);}
'''
write('capture-rl',body,main)
```

## `target/refvec/opposed/build.py`

```python
from pathlib import Path
s=Path('target/refvec/segbased/build.py').read_text();exec(s[:s.index("seg=(root/")])
shim=shim.replace('dt_iop_module_t *module;};','dt_iop_module_t *module;void*data;};')+'\n#define DT_IOP_HIGHLIGHTS_OPPOSED 0\n'
opp=(root/'iop/hlreconstruct/opposed.c').read_text();opp=opp[opp.index('static inline float _calc_linear_refavg'):opp.index('static float *_process_opposed')]
main=r'''
int main(){const int w=96,h=96;float in[w*h*4],out[w*h*4];dev_t_ dev={};dt_iop_module_t mod={NULL,&dev};pipe_t_ pipe={};pipe.dsc.temperature.enabled=1;for(int c=0;c<3;c++)pipe.dsc.temperature.coeffs[c]=1;dt_iop_highlights_data_t d={};d.clip=.99f;dt_dev_pixelpipe_iop_t piece={};piece.pipe=&pipe;piece.module=&mod;piece.data=&d;dt_iop_roi_t roi={0,0,w,h};
for(int y=0;y<h;y++)for(int x=0;x<w;x++){const float v=.4f+.002f*x+.001f*y;for(int c=0;c<3;c++)in[4*(y*w+x)+c]=v*(c==0?.8f:c==1?1.f:1.2f);if((x-48)*(x-48)+(y-48)*(y-48)<21*21){in[4*(y*w+x)+1]=2.2f;in[4*(y*w+x)+2]=1.05f;}in[4*(y*w+x)+3]=0;}
_process_linear_opposed(&mod,&piece,in,out,&roi);FILE*fp=fopen("crates/lc-raw/tests/fixtures/opposed/original.f32","wb");for(int i=0;i<w*h;i+=3)fwrite(out+i*4,4,3,fp);fclose(fp);}
'''
Path('target/refvec/opposed/reference.cc').write_text(shim+opp+main)
Path('target/refvec/opposed/corrected.cc').write_text(shim+opp.replace('input[idx] >= clips[c]','input[idx+c] >= clips[c]')+main.replace('original.f32','corrected.f32'))
```


## `target/refvec/amaze-rt/build.py`

```python
from pathlib import Path
src=Path('/home/zdavidson/.local/share/local-image-dev/engine-sources/refvec/upstream/rt/rtengine/amaze_demosaic_RT.cc').read_text()
src=src[src.index('namespace\n{'):].replace('RawImageSource::amaze_demosaic_RT','amaze_demosaic_RT')
shim=r'''
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <cstdio>
#include <algorithm>
#include <vector>
#include <memory>
#include <iostream>
using std::min;using std::max;
template<class T> using array2D=std::vector<std::vector<T>>;
struct StopWatch {StopWatch(const char*){}};
struct Listener {void setProgress(double){} void setProgressStr(const char*){}};
Listener *plistener=nullptr;
namespace Glib {struct ustring {static const char *compose(const char*,const char*){return "";}};}
#define M(x) (x)
#define SQR(x) ((x)*(x))
static int W=255,H=251,border=4;static float initialGain=1.f;
static unsigned pattern[2][2];
#define FC(r,c) pattern[(r)&1][(c)&1]
static float intp(float a,float b,float c){return a*(b-c)+c;}
static float median(float a,float b,float c){return a+b+c-std::min(a,std::min(b,c))-std::max(a,std::max(b,c));}
static float xmul2f(float x){uint32_t v;memcpy(&v,&x,4);if(v&0x7fffffff)v+=1u<<23;memcpy(&x,&v,4);return x;}
static float xdiv2f(float x){uint32_t v;memcpy(&v,&x,4);if(v&0x7fffffff)v-=1u<<23;memcpy(&x,&v,4);return x;}
static float xdivf(float x,int n){uint32_t v;memcpy(&v,&x,4);if(v&0x7fffffff)v-=(uint32_t)n<<23;memcpy(&x,&v,4);return x;}
static void border_interpolate(int,int,int,const array2D<float>&,array2D<float>&,array2D<float>&,array2D<float>&){}
'''
# Scalar upstream build, no SSE/OpenMP. rt_math median is the bounded middle value.
shim=shim.replace('return a+b+c-std::min(a,std::min(b,c))-std::max(a,std::max(b,c));','return max(min(a,b),min(max(a,b),c));')
main=r'''
int main(){const char *names[]={"RGGB","BGGR","GRBG","GBRG"};const unsigned patterns[4][2][2]={{{0,1},{1,2}},{{2,1},{1,0}},{{1,0},{2,1}},{{1,2},{0,1}}};
array2D<float> in(H,std::vector<float>(W)),r=in,g=in,b=in;
for(int y=0;y<H;y++)for(int x=0;x<W;x++){uint32_t n=(uint32_t)(y*W+x)*1664525u+1013904223u;in[y][x]=(float)((n>>8)&65535)/65536.f*65535.f;}
for(int p=0;p<4;p++){memcpy(pattern,patterns[p],sizeof(pattern));rtengine::amaze_demosaic_RT(0,0,W,H,in,r,g,b,1,false);
char path[256];sprintf(path,"crates/lc-raw/tests/fixtures/amaze/%s-rt.f32",names[p]);FILE *fp=fopen(path,"wb");
for(int y=0;y<H;y++)for(int x=0;x<W;x++)if(x%17<2 || y%17<2 || x>=126 && x<=130 || y>=126 && y<=130){float out[]={r[y][x]/65535.f,g[y][x]/65535.f,b[y][x]/65535.f};fwrite(out,4,3,fp);}fclose(fp);}}
'''
Path('target/refvec/amaze-rt/reference.cc').write_text(shim+src+main)
```

Segmentation cases 0–6 select the seven recovery modes with neutral WB; cases 7/8 repeat
Off/AdaptiveFlat at WB `[1.7, 0.85, 2.3]`, undoing WB when writing camera-CFA samples.
The RT AMaZE shim supplies only array storage, scalar `intp`/median/exponent helpers and
inactive progress/border adapters. `border=4` retains the kernel's own mirror boundary;
`-U__SSE2__` selects upstream's complete scalar branch, not a replacement kernel.
