// darktable UCS 22 / Filmlight Yrg, GPL-3.0-or-later. CPU twin: lc-pipeline/ucs.rs.
// Matrices are generated from the reference's f32 constants, gamut LUT is control data.
const LS_RANGE = 2.0988838;
const LS_UPPER = 2.09885;
const TINY = 1.17549435e-38;
fn y_ls(y: f32) -> f32 { let t = pow(max(y,0.0),0.63165135); return LS_RANGE*t/(t+1.1242677); }
fn ls_y(l: f32) -> f32 { return pow(1.1242677*l/(LS_RANGE-l),1.5831519); }
fn xyz_xyy(x: vec3<f32>) -> vec3<f32> {
    let v=max(x,vec3<f32>(0.0)); let s=v.x+v.y+v.z;
    if(s>0.0) { return vec3<f32>(v.x/s,v.y/s,v.y); } return vec3<f32>(0.31271,0.32902,v.y);
}
fn xyy_xyz(v: vec3<f32>) -> vec3<f32> {
    if(v.y==0.0) {return vec3<f32>(0.0);} return vec3<f32>(v.z*v.x/v.y,v.z,v.z*(1.0-v.x-v.y)/v.y);
}
fn nonzero(d: f32) -> f32 {return select(min(d,-TINY),max(d,TINY),d>=0.0);}
fn xy_uv(x: f32,y: f32) -> vec2<f32> {
    let d=nonzero(0.31870728*x+2.1674369*y+0.29132055);
    let u=(-0.783941*x+0.277513*y+0.15383658)/d;
    let v=(0.74527354*x-0.20537587*y-0.16547838)/d;
    let us=1.3965623*u/(abs(u)+1.4921735); let vs=1.4513954*v/(abs(v)+1.5248864);
    return vec2<f32>(-1.1249839*us-0.9804837*vs,1.8632332*us+1.9718531*vs);
}
fn uv_xy(uv: vec2<f32>) -> vec2<f32> {
    let us=-5.0375224*uv.x-2.5048563*uv.y; let vs=4.7600294*uv.x+2.874013*uv.y;
    let u=-1.4921735*us/(abs(us)-1.3965623); let v=-1.5248864*vs/(abs(vs)-1.4513954);
    let d=nonzero(0.94025474*u+v-0.025632597);
    return vec2<f32>((0.16717147*u+0.1412998*v-0.008015313)/d,(-0.15095909*u-0.15518506*v-0.008433124)/d);
}
fn luv_jch(l: f32,lw: f32,uv: vec2<f32>) -> vec3<f32> {
    let m2=uv.x*uv.x+uv.y*uv.y;
    // atan2(0,0) is undefined in WGSL; CPU atan2 gives zero.
    var hue=0.0; if(m2>0.0) {hue=atan2(uv.y,uv.x);}
    return vec3<f32>(l/lw,15.932994*pow(max(l,0.0),0.65239975)*pow(m2,0.6007557)/lw,hue);
}
fn xyy_jch(v: vec3<f32>,lw: f32) -> vec3<f32> {return luv_jch(y_ls(v.z),lw,xy_uv(v.x,v.y));}
fn jch_xyy(v: vec3<f32>,lw: f32) -> vec3<f32> {
    let l=clamp(v.x*lw,0.0,LS_UPPER); var m=0.0;
    if(l!=0.0) {m=pow(max(v.y*lw/(15.932994*pow(l,0.65239975)),0.0),0.83228507);}
    let xy=uv_xy(m*vec2<f32>(cos(v.z),sin(v.z))); return vec3<f32>(xy,ls_y(l));
}
fn jch_hsb(v: vec3<f32>) -> vec3<f32> {
    let b=v.x*(pow(max(v.y,0.0),1.3365422)+1.0); var s=0.0; if(b>0.0) {s=v.y/b;} return vec3<f32>(v.z,s,b);
}
fn hsb_jch(v: vec3<f32>) -> vec3<f32> {let c=v.y*v.z; return vec3<f32>(v.z/(pow(max(c,0.0),1.3365422)+1.0),c,v.x);}
fn jch_hcb(v: vec3<f32>) -> vec3<f32> {return vec3<f32>(v.z,v.y,v.x*(pow(max(v.y,0.0),1.3365422)+1.0));}
fn hcb_jch(v: vec3<f32>) -> vec3<f32> {return vec3<f32>(v.z/(pow(max(v.y,0.0),1.3365422)+1.0),v.y,v.x);}
fn soft(x: f32,s: f32,h: f32) -> f32 {if(x>s) {return s+(1.0-exp(-(x-s)/(h-s)))*(h-s);} return x;}
// `e` contains the gamut at offset 0, followed by optional equalizer/saturation LUTs.
fn lut_hue(off: u32,h: f32) -> f32 {
    let x=512.0*(h+PI)/TAU; let x0=floor(x);
    let i=u32((i32(x0)%512+512)%512); let j=u32((i32(ceil(x))%512+512)%512);
    let y=e[off+i]; if(i!=j) {return y+(x-x0)*(e[off+j]-y);} return y;
}
fn gamut_hsb(v: vec3<f32>,lw: f32) -> vec3<f32> {
    let j=hsb_jch(v); let mc=15.932994*pow(max(j.x*lw,0.0),0.65239975)*pow(lut_hue(0u,j.z),0.6007557)/lw;
    let b=jch_hsb(vec3<f32>(j.x,mc,j.z)); return vec3<f32>(v.x,soft(v.y,0.8*b.y,b.y),v.z);
}
fn rgb_hsb(v: vec3<f32>,lw: f32) -> vec3<f32> {return jch_hsb(xyy_jch(xyz_xyy(mul3(UCS_RGB_XYZ,v)),lw));}
fn hsb_rgb(v: vec3<f32>,lw: f32) -> vec3<f32> {return mul3(UCS_XYZ_RGB,xyy_xyz(jch_xyy(hsb_jch(v),lw)));}
fn lms_yrg(v: vec3<f32>) -> vec3<f32> {
    let y=0.68990272*v.x+0.34832189*v.y; let s=v.x+v.y+v.z; var n=vec3<f32>(0.0); if(s!=0.0) {n=v/s;}
    let r=mul3(UCS_LMS_GRADING,n); return vec3<f32>(y,r.x,r.y);
}
fn yrg_lms(v: vec3<f32>) -> vec3<f32> {
    let l=mul3(UCS_GRADING_LMS,vec3<f32>(v.y,v.z,1.0-v.y-v.z)); let d=0.68990272*l.x+0.34832189*l.y;
    var t=0.0; if(d!=0.0) {t=v.x/d;} return l*t;
}
fn yrg_ych(v: vec3<f32>) -> vec4<f32> {
    let r=v.y-0.21902143; let g=v.z-0.54371398; let c=sqrt(g*g+r*r); var h=vec2<f32>(1.0,0.0);
    if(c!=0.0) {h=vec2<f32>(r,g)/c;} return vec4<f32>(v.x,c,h);
}
fn ych_yrg(v: vec4<f32>) -> vec3<f32> {return vec3<f32>(v.x,v.y*v.z+0.21902143,v.y*v.w+0.54371398);}
fn gamut_ych(v: vec4<f32>) -> vec4<f32> {
    let r=ych_yrg(v); var mc=v.y;
    if(r.y<0.0) {mc=min(mc,-0.21902143/v.z);} if(r.z<0.0) {mc=min(mc,-0.54371398/v.w);}
    if(r.y+r.z>1.0) {mc=min(mc,(1.0-0.21902143-0.54371398)/(v.z+v.w));} return vec4<f32>(v.x,mc,v.zw);
}
fn cpu_round(x: f32) -> f32 {return sign(x)*floor(abs(x)+0.5);}
fn vector_pow(v: f32,p: f32) -> f32 {
    let bits=bitcast<u32>(v); let m=bitcast<f32>((bits&0x007fffffu)|0x3f800000u); let ex=f32((bits&0x7f800000u)>>23u)-127.0;
    let l=((((0.05965155*m-0.46572563)*m+1.4811665)*m-2.5207496)*m+2.8882704)*(m-1.0)+ex;
    let x=clamp(l*p,-126.99999,129.0); let ip=cpu_round(x-0.5); let f=x-ip;
    let mult=bitcast<f32>(u32(127+i32(ip))<<23u);
    return mult*((((0.013534167*f+0.052011464)*f+0.24144275)*f+0.69300383)*f+1.0000026);
}
