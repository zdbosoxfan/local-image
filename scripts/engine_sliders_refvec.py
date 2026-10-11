#!/usr/bin/env python3
"""Pinned upstream C kernels, compiled only under ignored target/refvec/sliders.
No copied/translated Rust implementation is used to generate expected values.
"""
import re,subprocess,sys,json,hashlib
from pathlib import Path
root=Path(__file__).resolve().parents[1]
src=Path(sys.argv[1]) if len(sys.argv)>1 else Path('/home/zdavidson/.local/share/local-image-dev/engine-sources/dt')
work=root/'target/refvec/sliders';work.mkdir(parents=True,exist_ok=True)
fixtures=root/'crates/lc-pipeline/tests/fixtures/sliders';fixtures.mkdir(parents=True,exist_ok=True)
def fn(path,name):
    s=(src/path).read_text();m=re.search(r'^(?:static\s+)?(?:inline\s+)?[\w *]+\b'+re.escape(name)+r'\s*\(',s,re.M)
    if not m:raise ValueError(name)
    start=m.start();i=s.index('{',m.end());j=i+1;depth=1
    while depth:depth+=(s[j]=='{')-(s[j]=='}');j+=1
    return s[start:j]+'\n'
def stripped(path):return re.sub(r'^#include[^\n]*\n','',(src/path).read_text(),flags=re.M)
header=r'''
#include <math.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <float.h>
#include <assert.h>
#define MIN(a,b) ((a)<(b)?(a):(b))
#define MAX(a,b) ((a)>(b)?(a):(b))
#define CLAMP(x,a,b) MIN(MAX(x,a),b)
#define CLAMPS CLAMP
#define CLAMPF CLAMP
#define DT_OMP_FOR(...)
#define DT_OMP_SIMD(...)
#define DT_OMP_FOR_SIMD(...)
#define DT_OMP_DECLARE_SIMD(...)
#define __DT_CLONE_TARGETS__
#define DT_ALIGNED_ARRAY
#define DT_ALIGNED_PIXEL
#define for_each_channel(c,...) for(int c=0;c<3;c++)
#define for_four_channels(c,...) for(int c=0;c<4;c++)
#define dt_omploop_sfence()
#define TRUE 1
#define FALSE 0
#define _(s) s
#define MIN_FLOAT (1.f/65536.f)
#define dt_control_log(...)
#define sqf(x) ((x)*(x))
#define sqrf(x) ((x)*(x))
#define dt_fast_hypotf(x,y) sqrtf((x)*(x)+(y)*(y))
#define interpolatef(a,b,c) ((a)*(b)+(1.f-(a))*(c))
#define CLIP(x) CLAMP(x,0.f,1.f)
#define NORM_MIN (1.f/65536.f)
#define M_PI_F ((float)M_PI)
#define M_PI_2f ((float)M_PI_2)
#define DT_2PI_F ((float)(2*M_PI))
#define deg2radf(x) ((x)*M_PI_F/180.f)
typedef int gboolean;
typedef float dt_aligned_pixel_t[4];
typedef float dt_colormatrix_t[3][4];
union float_int {float f;int k;};
static float *dt_alloc_align_float(size_t n){return calloc(n+64,sizeof(float));}
static void dt_free_align(void*p){free(p);}
static void copy_pixel(float*d,const float*s){memcpy(d,s,4*sizeof(float));}
static void dt_iop_image_copy(float*d,const float*s,size_t n){memcpy(d,s,n*sizeof(float));}
'''
gauss=header+r'''
typedef enum {DT_IOP_GAUSSIAN_ZERO,DT_IOP_GAUSSIAN_ONE,DT_IOP_GAUSSIAN_TWO} dt_gaussian_order_t;
typedef struct {int width,height,channels,order;float sigma,*buf,*max,*min;} dt_gaussian_t;
static dt_gaussian_t *dt_gaussian_init(int w,int h,int nc,const float*max,const float*min,float sigma,int order){
 dt_gaussian_t*g=calloc(1,sizeof(*g));g->width=w;g->height=h;g->channels=nc;g->sigma=sigma;g->order=order;
 g->buf=dt_alloc_align_float(w*h*nc);g->max=dt_alloc_align_float(4);g->min=dt_alloc_align_float(4);memcpy(g->min,min,nc*4);memcpy(g->max,max,nc*4);return g;}
static void dt_gaussian_free(dt_gaussian_t*g){free(g->buf);free(g->min);free(g->max);free(g);}
typedef enum {DT_GF_BLENDING_LINEAR,DT_GF_BLENDING_GEOMEAN} dt_iop_guided_filter_blending_t;
'''
for name in ['_compute_gauss_params','dt_gaussian_blur','dt_gaussian_blur_4c']:gauss+=fn('src/common/gaussian.c',name)
gauss+=fn('src/common/gaussian.h','dt_gaussian_mean_blur')
for name in ['fast_clamp','interpolate_bilinear','quantize']:gauss+=fn('src/common/fast_guided_filter.h',name)
gauss+=stripped('src/common/eigf.h')
def run(name,code):
    c=work/(name+'.c');c.write_text(code)
    exe=work/name;subprocess.run(['gcc','-O0','-ffp-contract=off',str(c),'-lm','-o',str(exe)],check=True)
    data=subprocess.check_output([str(exe)]);(fixtures/(name+'.csv')).write_bytes(data);print(name,len(data),'bytes')
run('eigf',gauss+r'''
int main(){enum{W=37,H=29,N=W*H};float p[N];
 for(int mode=0;mode<4;mode++){for(int y=0;y<H;y++)for(int x=0;x<W;x++)p[y*W+x]=.002f+.013f*x+.0007f*y+(x>18?.7f:0.f)+.002f*sinf(x*.7f+y*.3f);
 float sg=mode==0?1.7f:6.3f;fast_eigf_surface_blur(p,W,H,sg,.08f,3,mode==3?DT_GF_BLENDING_GEOMEAN:DT_GF_BLENDING_LINEAR,1.f,mode>1?.5f:0.f,exp2f(-14),4.f);
 for(int i=0;i<N;i++)printf("%d,%d,%.9g\n",mode,i,p[i]);}
}
''')
ll=header+fn('src/common/math.h','dt_fast_expf')
ll+=r'''
typedef struct {int x,y,width,height;float scale;} dt_iop_roi_t;
struct {int dump_pfm_module;} darktable;
static void dt_dump_pfm(const char*a,const float*b,int c,int d,int e,const char*f){}
'''
h=stripped('src/common/locallaplacian.h');ll+=h[h.index('typedef struct'):h.index('void local_laplacian_boundary_free')]
c=stripped('src/common/locallaplacian.c');ll+=c[c.index('#define max_levels'):c.index('size_t local_laplacian_memory_use')]
run('llf',ll+r'''
int main(){enum{W=50,H=37,N=W*H};float p[N*4],o[N*4];
 for(int y=0;y<H;y++)for(int x=0;x<W;x++){int i=y*W+x;p[4*i]=100.f*(.3f+.2f*(sinf(x*.3f)*cosf(y*.2f))+(x>25?.3f:0.f));p[4*i+1]=p[4*i+2]=0;}
 for(int mode=0;mode<3;mode++){local_laplacian_internal(p,o,W,H,.2f,mode==1?.6f:1.f,mode==1?1.4f:1.f,mode==2?.8f:0.f,0);
 for(int i=0;i<N;i++)printf("%d,%d,%.9g\n",mode,i,o[i*4]*.01f);}
}
''')
ucs=header+'#define DT_UCS_L_STAR_RANGE 2.098883786377f\n#define DT_UCS_L_STAR_UPPER_LIMIT 2.09885f\n'
for name in ['Y_to_dt_UCS_L_star','dt_UCS_L_star_to_Y','xyY_to_dt_UCS_UV','dt_UCS_LUV_to_JCH','xyY_to_dt_UCS_JCH','dt_UCS_JCH_to_xyY','dt_UCS_JCH_to_HSB','dt_UCS_HSB_to_JCH','dt_UCS_JCH_to_HCB','dt_UCS_HCB_to_JCH']:
    ucs+=fn('src/common/colorspaces_inline_conversions.h',name)
run('ucs',ucs+r'''
int main(){for(int i=0;i<41;i++){float y=exp2f(-12.f+i*.5f),xy[4]={.25f+.005f*i,.23f+.003f*i,y,0},j[4],h[4],back[4];float lw=Y_to_dt_UCS_L_star(1.f);
 xyY_to_dt_UCS_JCH(xy,lw,j);dt_UCS_JCH_to_HSB(j,h);dt_UCS_HSB_to_JCH(h,back);dt_UCS_JCH_to_xyY(back,lw,xy);
 printf("%d,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g\n",i,j[0],j[1],j[2],h[0],h[1],h[2],xy[0],xy[1],xy[2]);}}
''')
# Equalizer's matrix/filter bodies run unchanged, sharing the extracted Gaussian/interpolator.
ce=gauss+'\n#define SATSIZE 4096\n'+fn('src/common/math.h','scharr_gradient')
for name in ['_get_scaling','_init_satweights','_get_satweight','_init_covariance','_finish_covariance','_prepare_prefilter','_apply_prefilter','_prefilter_chromaticity','_guide_with_chromaticity']:
    if name=='_init_satweights':ce+='static float satweights[2*SATSIZE+1];static float lastcontrast=NAN;\n'
    ce+=fn('src/iop/colorequal.c',name)
run('colorequal-filter',ce+r'''
int main(){enum{W=37,H=29,N=W*H};float uv[N*2],sat[N],corr[N*2],bc[N],grad[N];_init_satweights(0.f);
 for(int y=0;y<H;y++)for(int x=0;x<W;x++){int i=y*W+x;uv[2*i]=.03f*sinf(.13f*x)+.01f*(x>18);uv[2*i+1]=.04f*cosf(.17f*y);sat[i]=.3f;corr[2*i]=.1f;corr[2*i+1]=1.f+.2f*sinf(x*.1f);bc[i]=.03f*cosf(y*.1f);grad[i]=.05f;}
 _prefilter_chromaticity(uv,sat,W,H,7.3f,1e-5f,.1f);
 for(int i=0;i<N;i++)printf("0,%d,%.9g,%.9g\n",i,uv[2*i],uv[2*i+1]);
 _guide_with_chromaticity(uv,corr,sat,bc,grad,W,H,8.5f,1e-6f,.1f,.1f);
 for(int i=0;i<N;i++)printf("1,%d,%.9g,%.9g\n",i,corr[2*i+1],bc[i]);
}
''')

# Full color-balance pixel body, unmodified; only the host matrices/LUT are supplied.
conv='src/common/colorspaces_inline_conversions.h'
helper='src/common/darktable_ucs_22_helpers.h'
bal=ucs+r'''
#undef __SSE__
#undef __SSE2__
#define LOG_POLY_DEGREE 5
#define EXP_POLY_DEGREE 4
#define LUT_ELEM 512
static const struct {float x,y;} D65xyY={.3127f,.3290f};
'''
for name in ['dt_vector_min','dt_vector_max','dt_vector_round','dt_vector_log2','dt_vector_exp2','dt_vector_powf','dt_vector_clipneg','scalar_product']:
    bal+=fn('src/common/math.h',name)
# Definitions of transposed CIE2006/Filmlight matrices, verbatim.
cs=(src/conv).read_text()
for name in ['XYZ_D65_to_LMS_2006_D65_trans','LMS_2006_D65_to_XYZ_D65_trans','filmlightRGB_D65_to_LMS_D65_trans','LMS_D65_to_filmlightRGB_D65_trans']:
    bal+=re.search(r'static const dt_colormatrix_t '+name+r'\s*=.*?;',cs,re.S).group(0)+'\n'
for name in ['dt_apply_transposed_color_matrix','dt_D65_XYZ_to_xyY','dt_xyY_to_XYZ','XYZ_to_LMS','LMS_to_XYZ','gradingRGB_to_LMS','LMS_to_gradingRGB','LMS_to_Yrg','Yrg_to_LMS','Yrg_to_Ych','Ych_to_Yrg','gamut_check_Yrg','_dt_pow_diff','dt_XYZ_2_JzAzBz','dt_JzAzBz_2_XYZ']:
    bal+=fn(conv,name)
for name in ['lookup_gamut','soft_clip']:bal+=fn(helper,name)
bal+=fn('src/iop/colorbalancergb.c','opacity_masks')
bal+='typedef enum {DT_COLORBALANCE_SATURATION_JZAZBZ,DT_COLORBALANCE_SATURATION_UCS} dt_iop_colorbalancrgb_saturation_t;\n'
bal+=re.search(r'typedef struct dt_iop_colorbalancergb_data_t.*?} dt_iop_colorbalancergb_data_t;', (src/'src/iop/colorbalancergb.c').read_text(),re.S).group(0)
body=fn('src/iop/colorbalancergb.c','process')
body=body[body.index('    // clip pipeline RGB'):body.index('    if(mask_display)')]
# Matrices of the standard Rec2020/D65 working space. A direct D65 pipeline omits ICC CATs.
bal+=r'''
static const dt_colormatrix_t rgbxyz={{.6369580483f,.1446169036f,.1688809752f,0},{.2627002120f,.6779980715f,.0593017165f,0},{0,.0280726930f,1.0609850577f,0}};
static const dt_colormatrix_t xyzrgb={{1.7166511880f,-.3556707838f,-.2533662814f,0},{-.6666843518f,1.6164812366f,.0157685458f,0},{.0176398574f,-.0427706133f,.9421031212f,0}};
static void pixel(dt_iop_colorbalancergb_data_t*d,const float*in,float*out,const float*gamut_LUT){
 int k=0;dt_colormatrix_t input_matrix_trans,output_matrix_trans;
 for(int i=0;i<3;i++)for(int j=0;j<3;j++){
 input_matrix_trans[i][j]=0;for(int t=0;t<3;t++)input_matrix_trans[i][j]+=XYZ_D65_to_LMS_2006_D65_trans[t][j]*rgbxyz[t][i];
 output_matrix_trans[i][j]=xyzrgb[j][i];}
 const float*global=d->global,*shadows=d->shadows,*highlights=d->highlights,*midtones=d->midtones;
 const float*chroma=d->chroma,*saturation=d->saturation,*brilliance=d->brilliance;
 float L_white=Y_to_dt_UCS_L_star(d->white_fulcrum);
 float hue_rotation_matrix[2][2]={{cosf(d->hue_angle),-sinf(d->hue_angle)},{sinf(d->hue_angle),cosf(d->hue_angle)}};
'''+body+'dt_vector_clipneg(pix_out);copy_pixel(out,pix_out);}\n'
run('balance',bal+r'''
int main(){dt_iop_colorbalancergb_data_t d={0};float lut[512];
 d.white_fulcrum=1;d.grey_fulcrum=.18f;d.mask_grey_fulcrum=powf(.18f,.4101205819200422f);
 d.shadows_weight=d.highlights_weight=4;d.midtones_weight=8;d.midtones_Y=.93f;d.contrast=1.08f;
 for(int c=0;c<3;c++){d.global[c]=.002f*(c+1);d.shadows[c]=.9f+.05f*c;d.highlights[c]=1.15f-.04f*c;d.midtones[c]=.94f+.04f*c;
 d.chroma[c]=.03f*(c-1);d.saturation[c]=.05f*(c-1);d.brilliance[c]=.02f*(c-1);}
 d.vibrance=.35f;d.chroma_global=.08f;d.saturation_global=.25f;d.brilliance_global=.04f;d.hue_angle=.1f;
 for(int mode=0;mode<2;mode++){d.saturation_formula=mode==0?DT_COLORBALANCE_SATURATION_UCS:DT_COLORBALANCE_SATURATION_JZAZBZ;
 for(int i=0;i<512;i++)lut[i]=(mode==0?.025f:.7f)*(1.f+.1f*sinf(i*.03f));
 for(int i=0;i<41;i++){float v=exp2f(-12.f+i*.5f),in[4]={v*(.3f+.01f*i),v*.7f,v*(.9f-.01f*i),0},out[4];pixel(&d,in,out,lut);
 printf("%d,%d,%.9g,%.9g,%.9g\n",mode,i,out[0],out[1],out[2]);}}
}
''')
run('jz',bal+r'''
int main(){for(int i=0;i<41;i++){float v=exp2f(-12.f+i*.5f),x[4]={v*.95f,v,v*1.08f,0},j[4],b[4];dt_XYZ_2_JzAzBz(x,j);dt_JzAzBz_2_XYZ(j,b);
 printf("%d,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g\n",i,j[0],j[1],j[2],b[0],b[1],b[2]);}}
''')

rbf_c=header+'#define DT_OMP_PRAGMA(...)\n#define dt_print(...)\n#define LUT_ELEM 512\n#define NODES 8\n'
rbf_c+='static double*dt_alloc_align_double(size_t n){return calloc(n,sizeof(double));}\n'
rbf_c+=stripped('src/iop/choleski.h')
# Substitute only the node-position adapter, to exercise the LR extension with exact C math.
rbf_c+='static float _get_hue_node(int k,float shift){return -M_PI_F+k*DT_2PI_F/8.f+shift;}\n'
rbf_c+=fn('src/iop/colorequal.c','_cosine_coeffs')+fn('src/iop/colorequal.c','_periodic_RBF_interpolate')
run('colorequal-rbf',rbf_c+r"""
int main(){for(int mode=0;mode<2;mode++){float n[8],l[512];for(int k=0;k<8;k++)n[k]=.2f*cosf(k*1.1f)+(mode?1.f:0.f);
_periodic_RBF_interpolate(n,M_PI_F,l,0.f,mode);for(int i=0;i<512;i++)printf("%d,%d,%.9g\n",mode,i,l[i]);}}
""")
manifest={'darktable':'733bd69f32cac7ff5e41025115942772add1f088','compiler':'gcc -O0 -ffp-contract=off','fixtures':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in fixtures.glob('*.csv')}}
(fixtures/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
