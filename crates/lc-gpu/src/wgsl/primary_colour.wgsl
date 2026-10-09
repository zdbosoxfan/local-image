// Native UCS colour stages. CPU twins: balance.rs, colorequal.rs, primary::skin.
fn rgb_a(i: u32) -> vec3<f32> {return vec3<f32>(a[3u*i],a[3u*i+1u],a[3u*i+2u]);}
fn put_rgb(i: u32,v: vec3<f32>) {out[3u*i]=v.x; out[3u*i+1u]=v.y; out[3u*i+2u]=v.z;}
fn par3(i: u32) -> vec3<f32> {return vec3<f32>(pf(i),pf(i+1u),pf(i+2u));}
@compute @workgroup_size(256)
fn p_balance(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} var ych=yrg_ych(lms_yrg(mul3(UCS_RGB_LMS,max(rgb_a(i),vec3<f32>(0.0))))); ych.x=max(ych.x,0.0);
    let offset=pow(ych.x,0.41012058)-pf(36u); let nn=offset/pf(36u); let alpha=1.0/(1.0+exp(nn*pf(33u))); let beta=1.0/(1.0+exp(-nn*pf(34u)));
    let gamma=exp(-offset*offset*pf(35u)/4.0)*(1.0-alpha)*(1.0-alpha)*(1.0-beta)*(1.0-beta)*8.0; let weights=vec3<f32>(alpha,gamma,beta);
    let co=cos(pf(14u)); let sn=sin(pf(14u)); let hc=ych.z; let hs=ych.w; ych.z=co*hc-sn*hs; ych.w=sn*hc+co*hs;
    let saturation=pf(3u)+dot(weights,par3(7u)); let brilliance=max(1.0+pf(13u)+dot(weights,par3(10u)),0.0);
    var vib=0.0; if(pf(1u)!=0.0) {vib=pf(1u)*(1.0-pow(ych.y,abs(pf(1u))));}
    ych.y*=max(1.0+pf(2u)+dot(weights,par3(4u))+vib,0.0); ych=gamut_ych(ych);
    var rgb=mul3(UCS_LMS_GRADING,yrg_lms(ych_yrg(ych)));
    let global=par3(15u); let shadows=par3(18u); let highlights=par3(21u); let midtones=par3(24u);
    for(var k=0u;k<3u;k++) {rgb[k]+=global[k]; rgb[k]*=(1.0-beta)*((1.0-alpha)+alpha*shadows[k])+beta*highlights[k];
        let s=select(1.0,-1.0,rgb[k]<0.0); rgb[k]=vector_pow(abs(rgb[k])/pf(28u),midtones[k])*s*pf(28u);}
    var yrg=lms_yrg(mul3(UCS_GRADING_LMS,rgb)); yrg.x=pow(max(yrg.x/pf(28u),0.0),pf(27u))*pf(28u); yrg.x=pf(29u)*pow(yrg.x/pf(29u),pf(30u));
    let xyz=mul3(UCS_LMS_XYZ,yrg_lms(yrg)); let lw=y_ls(pf(28u)); var hcb=jch_hcb(xyy_jch(xyz_xyy(xyz),lw)); let radius=sqrt(hcb.y*hcb.y+hcb.z*hcb.z);
    var ss=0.0; var cc=0.0; if(radius>0.0) {ss=hcb.y/radius; cc=hcb.z/radius;} let p=max(hcb.y,TINY); let ww=ss*hcb.y+cc*hcb.z; let ma=sqrt(p*p+ww*ww)/p;
    let aa=soft(max(1.0+saturation,0.0),0.5*ma,ma); let pp=(aa-1.0)*p; let wp=sqrt(max(p*p*(1.0-aa*aa)+ww*ww,0.0))*brilliance;
    hcb.y=max(cc*pp+ss*wp,0.0); hcb.z=max(-ss*pp+cc*wp,0.0);
    let hsb=gamut_hsb(jch_hsb(hcb_jch(hcb)),lw); put_rgb(i,max(hsb_rgb(hsb,lw),vec3<f32>(0.0)));
}
@compute @workgroup_size(256)
fn p_equal_pre(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let v=rgb_a(i); let xyz=mul3(UCS_RGB_XYZ,v); let xy=xyz_xyy(xyz); let uv=xy_uv(xy.x,xy.y); let nc=pu(1u);
    if(nc==2u) {out[2u*i]=uv.x; out[2u*i+1u]=uv.y;} else if(nc==1u) {out[i]=y_ls(xyz.y);} else {
        let mn=min(min(v.x,v.y),v.z); let mx=max(max(v.x,v.y),v.z); var s=0.0; if(mx>0.000015258789 && mx-mn>0.000015258789) {s=(mx-mn)/mx;} out[i]=s;}
}
fn satweight(v: f32) -> f32 {
    let t=4096.0*(1.0+clamp(v,-1.0,1.0-1.0/4096.0)); let i=u32(floor(t)); return e[2560u+i]+(t-f32(i))*(e[2560u+i+1u]-e[2560u+i]);
}
@compute @workgroup_size(256)
fn p_uv_cov(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let u=a[2u*i]; let v=a[2u*i+1u];
    if(pu(1u)==0u) {out[4u*i]=u*u; out[4u*i+1u]=u*v; out[4u*i+2u]=u*v; out[4u*i+3u]=v*v;}
    else {out[4u*i]=u*b[2u*i]; out[4u*i+1u]=v*b[2u*i]; out[4u*i+2u]=u*b[2u*i+1u]; out[4u*i+3u]=v*b[2u*i+1u];}
}
// UV coefficients in separate four-channel A / two-channel B buffers (P[4]).
@compute @workgroup_size(256)
fn p_uv_ab(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let u=a[2u*i]; let v=a[2u*i+1u]; let eps=pf(1u);
    let s0=c[4u*i]-u*u+eps; let s1=c[4u*i+1u]-u*v; let s2=c[4u*i+2u]-u*v; let s3=c[4u*i+3u]-v*v+eps; let det=s0*s3-s1*s2;
    var mtarget=vec2<f32>(b[2u*i],b[2u*i+1u]); if(pu(2u)!=0u) {mtarget.y=e[pu(3u)+i];}
    for(var ch=0u;ch<2u;ch++) {var aa=0.0; var ab=0.0; if(abs(det)>0.000000476837158203125) {
        let cu=d[4u*i+2u*ch]-u*mtarget[ch]; let cv=d[4u*i+2u*ch+1u]-v*mtarget[ch]; aa=(cu*s3-cv*s1)/det; ab=(-cu*s2+cv*s0)/det;}
        if(pu(4u)==0u) {out[4u*i+2u*ch]=aa;out[4u*i+2u*ch+1u]=ab;} else {out[2u*i+ch]=mtarget[ch]-aa*u-ab*v;}}
}
@compute @workgroup_size(256)
fn p_extract(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} for(var ch=0u;ch<pu(2u);ch++) {out[pu(2u)*i+ch]=a[pu(1u)*i+pu(3u)+ch];}
}
@compute @workgroup_size(256)
fn p_uv_apply(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} for(var ch=0u;ch<2u;ch++) {out[2u*i+ch]=b[4u*i+2u*ch]*a[2u*i]+b[4u*i+2u*ch+1u]*a[2u*i+1u]+c[2u*i+ch];}
}
@compute @workgroup_size(256)
fn p_equal_uv(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let weight=satweight(c[i]-0.1); for(var ch=0u;ch<2u;ch++) {out[2u*i+ch]=a[2u*i+ch]+weight*(b[2u*i+ch]-a[2u*i+ch]);}
}
@compute @workgroup_size(256)
fn p_equal_hsb(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let j=luv_jch(b[i],y_ls(1.0),vec2<f32>(a[2u*i],a[2u*i+1u])); let v=jch_hsb(j);
    out[4u*i]=v.x; out[4u*i+1u]=v.y; out[4u*i+2u]=v.z; out[4u*i+3u]=j.y;
}
fn sat_at(x: i32,y: i32,w: u32,h: u32) -> f32 {return b[u32(clamp(y,0,i32(h)-1))*w+u32(clamp(x,0,i32(w)-1))];}
@compute @workgroup_size(256)
fn p_equal_corr(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let hue=a[4u*i]; let sat=a[4u*i+1u]; let chroma=a[4u*i+3u]; let kind=pu(1u);
    if(kind==0u) {out[2u*i]=1.0; out[2u*i+1u]=0.0; if(chroma>0.000015258789) {out[2u*i]=lut_hue(1024u,hue); out[2u*i+1u]=sat*(lut_hue(1536u,hue)-1.0);}}
    else if(kind==1u) {out[i]=0.0; if(chroma>0.000015258789) {out[i]=lut_hue(512u,hue);}}
    else {
        let w=pu(2u); let h=pu(3u); let x=i32(clamp(i%w,1u,max(w-2u,1u))); let y=i32(clamp(i/w,1u,max(h-2u,1u)));
        let gx=47.0/255.0*(sat_at(x-1,y-1,w,h)-sat_at(x+1,y-1,w,h)+sat_at(x-1,y+1,w,h)-sat_at(x+1,y+1,w,h))+162.0/255.0*(sat_at(x-1,y,w,h)-sat_at(x+1,y,w,h));
        let gy=47.0/255.0*(sat_at(x-1,y-1,w,h)-sat_at(x-1,y+1,w,h)+sat_at(x+1,y-1,w,h)-sat_at(x+1,y+1,w,h))+162.0/255.0*(sat_at(x,y-1,w,h)-sat_at(x,y+1,w,h));
        let delta=max(sqrt(gx*gx+gy*gy)-0.02,0.0); out[i]=4.0*sqrt(pf(4u))*pf(5u)*pf(5u)*delta*delta;
    }
}
@compute @workgroup_size(256)
fn p_equal_finish(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} var h=vec3<f32>(a[4u*i],a[4u*i+1u],a[4u*i+2u]); let sat=c[i]; h.x+=d[i];
    h.y=max(h.y*(1.0+2.0*(b[2u*i]-1.0)*satweight(sat-0.1)),0.0);
    h.z=max(h.z*(1.0+8.0*b[2u*i+1u]*(1.0-clamp(d[pu(0u)+i],0.0,1.0))*satweight(sat-0.1-0.01*pf(1u))),0.0);
    let lw=y_ls(1.0); if(pu(2u)!=0u) {let yy=ls_y(clamp(h.z*lw,0.0,LS_UPPER)); put_rgb(i,vec3<f32>(yy*exp2(lut_hue(2048u,h.x)*1.3)));}
    else {put_rgb(i,hsb_rgb(gamut_hsb(h,lw),lw));}
}
@compute @workgroup_size(256)
fn p_skin_pre(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let v=rgb_a(i); let j=xyy_jch(xyz_xyy(mul3(UCS_RGB_XYZ,v)),y_ls(1.0));
    if(pu(1u)==0u) {out[4u*i]=j.x;out[4u*i+1u]=j.y*cos(j.z);out[4u*i+2u]=j.y*sin(j.z);out[4u*i+3u]=j.z;}
    else {out[2u*i]=max(sqrt((v.x*v.x+v.y*v.y+v.z*v.z)/3.0),1e-6);out[2u*i+1u]=j.y;}
}
@compute @workgroup_size(256)
fn p_skin_finish(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let cc=b[4u*i]; let aa=b[4u*i+1u]; let bb=b[4u*i+2u]; let hue=b[4u*i+3u]; let chroma=d[2u*i+1u];
    let refc=par3(1u); let dh=abs(wrap_angle(hue-refc.z)); let wh=pf(6u); let wc=pf(7u); let wl=pf(8u);
    var weight=(1.0-sstep(wh*0.5,wh,dh))*(1.0-sstep(wc*0.5,wc,abs(chroma-refc.y)))*(1.0-sstep(wl*0.5,wl,abs(cc-refc.x)))*sstep(0.005,0.03,chroma);
    if(pu(9u)!=0u) {weight*=1.0-sstep(0.12,0.3,-wrap_angle(hue-refc.z));} if(weight<=0.0) {put_rgb(i,rgb_a(i)); return;}
    let u=weight*pf(4u); let v=weight*pf(5u)*0.25; let newa=aa+u*(refc.y*cos(refc.z)-c[pu(0u)+i]); let newb=bb+u*(refc.y*sin(refc.z)-c[2u*pu(0u)+i]);
    var hh=0.0; if(newa!=0.0 || newb!=0.0) {hh=atan2(newb,newa);} let j=vec3<f32>(max(cc+v*(refc.x-c[i]),0.0),sqrt(newa*newa+newb*newb),hh);
    put_rgb(i,hsb_rgb(gamut_hsb(jch_hsb(j),y_ls(1.0)),y_ls(1.0)));
}
