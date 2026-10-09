// Native scene stages. CPU twins: primary.rs, eigf.rs and llf.rs.
// a,b,c,d: input planes; e: immutable control LUTs; out: result. P[0] = pixel count.
fn rgb_a(i: u32) -> vec3<f32> {return vec3<f32>(a[3u*i],a[3u*i+1u],a[3u*i+2u]);}
fn put_rgb(i: u32,v: vec3<f32>) {out[3u*i]=v.x; out[3u*i+1u]=v.y; out[3u*i+2u]=v.z;}
fn rms(v: vec3<f32>) -> f32 {return sqrt((v.x*v.x+v.y*v.y+v.z*v.z)/3.0);}
fn primary_log(v: vec3<f32>) -> f32 {
    let n=max(rms(v),1e-7); let mx=max(max(v.x,v.y),v.z); let t=sstep(0.0,3.0,log2(n/0.18));
    return log2(max(n+(mx-n)*t,1e-7)/0.18);
}
fn tg(v: f32) -> f32 {return 1.6*pf(1u)*sstep(0.0,4.0,v)+1.8*pf(2u)*(1.0-sstep(-6.0,0.0,v))+0.8*pf(3u)*sstep(3.0,7.0,v)+0.8*pf(4u)*(1.0-sstep(-10.0,-4.0,v));}
@compute @workgroup_size(256)
fn p_scale(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} put_rgb(i,rgb_a(i)*pf(1u));
}
@compute @workgroup_size(256)
fn p_log(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} out[i]=primary_log(rgb_a(i));
}
@compute @workgroup_size(256)
fn p_biased(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} out[i]=max(rms(rgb_a(i)),0.0)+0.004;
}
@compute @workgroup_size(256)
fn p_lum(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} out[i]=exp(a[i]*0.6931471805599453)*0.18;
}
// Exact upstream corner-aligned interpolation, including repeated last-pixel weights.
fn interp_a(i: u32,ch: u32) -> f32 {
    let w=pu(1u); let h=pu(2u); let ow=pu(3u); let oh=pu(4u); let nc=pu(5u);
    let fx=f32(i%ow)/f32(ow)*f32(w); let fy=f32(i/ow)/f32(oh)*f32(h);
    let x0=min(u32(floor(fx)),w-1u); let x1=min(x0+1u,w-1u); let dx=f32(x1)-fx;
    let y0=min(u32(floor(fy)),h-1u); let y1=min(y0+1u,h-1u); let dy=f32(y1)-fy;
    return (1.0-dy)*(a[(y1*w+x0)*nc+ch]*dx+a[(y1*w+x1)*nc+ch]*(1.0-dx))
        +dy*(a[(y0*w+x0)*nc+ch]*dx+a[(y0*w+x1)*nc+ch]*(1.0-dx));
}
@compute @workgroup_size(256)
fn p_interp(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} for(var ch=0u;ch<pu(5u);ch++) {out[i*pu(5u)+ch]=interp_a(i,ch);}
}
// Deriche order-zero, no approximation of recurrence or endpoints. One invocation per line/channel.
@compute @workgroup_size(64)
fn p_deriche(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let k=g.x+g.y*ng.x*64u; if(k>=pu(0u)) {return;} let w=pu(1u); let h=pu(2u); let nc=pu(3u); let hor=pu(4u)!=0u;
    let line=k/nc; let ch=k%nc; let len=select(h,w,hor); let stride=select(w*nc,nc,hor); let off=select(line*nc,line*w*nc,hor)+ch;
    let lo=b[ch]; let hi=b[4u+ch]; let a0=pf(5u); let a1=pf(6u); let a2=pf(7u); let a3=pf(8u); let b1=pf(9u); let b2=pf(10u);
    var xp=clamp(a[off],lo,hi); var yp=xp*pf(11u); var yb=yp;
    for(var j=0u;j<len;j++) {let ix=off+j*stride; let xc=clamp(a[ix],lo,hi); let yc=a0*xc+a1*xp-b1*yp-b2*yb;
        out[ix]=yc; xp=xc; yb=yp; yp=yc;}
    var xn=clamp(a[off+(len-1u)*stride],lo,hi); var xa=xn; var yn=xn*pf(12u); var ya=yn;
    for(var j=i32(len)-1;j>=0;j--) {let ix=off+u32(j)*stride; let xc=clamp(a[ix],lo,hi); let yc=a2*xn+a3*xa-b1*yn-b2*ya;
        out[ix]=out[ix]+yc; xa=xn; xn=xc; ya=yn; yn=yc;}
}
var<workgroup> reduce_lo: array<vec4<f32>,256>;
var<workgroup> reduce_hi: array<vec4<f32>,256>;
@compute @workgroup_size(256)
fn p_bounds(@builtin(global_invocation_id) g: vec3<u32>,@builtin(local_invocation_index) t: u32,@builtin(workgroup_id) wg: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); var lo=vec4<f32>(pf(2u)); var hi=vec4<f32>(pf(3u));
    if(i<pu(0u)) { for(var ch=0u;ch<pu(1u);ch++) {lo[ch]=min(lo[ch],a[i*pu(1u)+ch]); hi[ch]=max(hi[ch],a[i*pu(1u)+ch]);}}
    reduce_lo[t]=lo; reduce_hi[t]=hi; workgroupBarrier();
    for(var s=128u;s>0u;s=s/2u) {if(t<s) {reduce_lo[t]=min(reduce_lo[t],reduce_lo[t+s]); reduce_hi[t]=max(reduce_hi[t],reduce_hi[t+s]);} workgroupBarrier();}
    if(t==0u) {let off=(wg.x+wg.y*ng.x)*8u; for(var ch=0u;ch<4u;ch++) {out[off+ch]=reduce_lo[0][ch]; out[off+4u+ch]=reduce_hi[0][ch];}}
}
@compute @workgroup_size(256)
fn p_bounds_join(@builtin(global_invocation_id) g: vec3<u32>,@builtin(local_invocation_index) t: u32,@builtin(workgroup_id) wg: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); var lo=vec4<f32>(pf(2u)); var hi=vec4<f32>(pf(3u));
    if(i<pu(0u)) {for(var ch=0u;ch<4u;ch++) {lo[ch]=min(lo[ch],a[8u*i+ch]); hi[ch]=max(hi[ch],a[8u*i+4u+ch]);}}
    reduce_lo[t]=lo; reduce_hi[t]=hi; workgroupBarrier();
    for(var s=128u;s>0u;s=s/2u) {if(t<s) {reduce_lo[t]=min(reduce_lo[t],reduce_lo[t+s]); reduce_hi[t]=max(reduce_hi[t],reduce_hi[t+s]);} workgroupBarrier();}
    if(t==0u) {let off=(wg.x+wg.y*ng.x)*8u; for(var ch=0u;ch<4u;ch++) {out[off+ch]=reduce_lo[0][ch]; out[off+4u+ch]=reduce_hi[0][ch];}}
}
@compute @workgroup_size(256)
fn p_quant(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} out[i]=clamp(exp2(floor(log2(a[i])/pf(1u))*pf(1u)),0.00006103515625,4.0);
}
@compute @workgroup_size(256)
fn p_moments(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let nc=pu(1u); let v=a[i]; out[nc*i]=v; out[nc*i+1u]=v*v;
    if(nc==4u) {out[nc*i+2u]=b[i]; out[nc*i+3u]=b[i]*v;}
}
@compute @workgroup_size(256)
fn p_variance(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let nc=pu(1u); let m=a[nc*i]; out[nc*i]=m; out[nc*i+1u]=a[nc*i+1u]-m*m;
    if(nc==4u) {out[nc*i+2u]=a[nc*i+2u]; out[nc*i+3u]=a[nc*i+3u]-m*a[nc*i+2u];}
}
@compute @workgroup_size(256)
fn p_eigf_apply(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let nc=pu(1u); let v=a[i]; let m=b[nc*i]; let norm=max(m*v,1e-6); let vr=b[nc*i+1u]/norm;
    var aa=vr/(vr+pf(2u)); var bb=m-aa*m;
    if(nc==4u) {let nm=max(b[nc*i+2u]*c[i],1e-6); aa=(b[nc*i+3u]/sqrt(norm*nm))/(vr+pf(2u)); bb=b[nc*i+2u]-aa*m;}
    let q=max(v*aa+bb,0.0000152587890625); out[i]=select(q,sqrt(v*q),pu(3u)!=0u);
}
@compute @workgroup_size(256)
fn p_cross_pre(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let v=a[i]; let p=b[i]; out[4u*i]=v; out[4u*i+1u]=p; out[4u*i+2u]=v*v; out[4u*i+3u]=v*p;
}
@compute @workgroup_size(256)
fn p_cross_apply(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let v=a[i]; let m=b[4u*i]; let mp=b[4u*i+1u];
    let aa=(b[4u*i+3u]-m*mp)/(max(b[4u*i+2u]-m*m,0.0)+pf(1u)*max(m*v,1e-6)); out[i]=aa*v+mp-aa*m;
}
@compute @workgroup_size(256)
fn p_gain_ab(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let m=a[4u*i]; let mp=a[4u*i+1u];
    let aa=(a[4u*i+3u]-m*mp)/(max(a[4u*i+2u]-m*m,0.0)+0.05); out[2u*i]=aa; out[2u*i+1u]=mp-aa*m;
}
// Guided upsampling uses the reference's centre-aligned bilinear resample weights.
fn bilinear_b(i: u32,ch: u32) -> f32 {
    let w=pu(1u); let h=pu(2u); let pw=pu(3u); let ph=pu(4u);
    let fx=(f32(i%w)+0.5)*f32(pw)/f32(w)-0.5; let fy=(f32(i/w)+0.5)*f32(ph)/f32(h)-0.5;
    let x=i32(floor(fx)); let y=i32(floor(fy)); let dx=fx-f32(x); let dy=fy-f32(y);
    let x0=u32(clamp(x,0,i32(pw)-1)); let x1=u32(clamp(x+1,0,i32(pw)-1)); let y0=u32(clamp(y,0,i32(ph)-1)); let y1=u32(clamp(y+1,0,i32(ph)-1));
    return (b[2u*(y0*pw+x0)+ch]*(1.0-dx)+b[2u*(y0*pw+x1)+ch]*dx)*(1.0-dy)
        +(b[2u*(y1*pw+x0)+ch]*(1.0-dx)+b[2u*(y1*pw+x1)+ch]*dx)*dy;
}
@compute @workgroup_size(256)
fn p_up_gain(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} out[i]=bilinear_b(i,0u)*a[i]+bilinear_b(i,1u);
}
@compute @workgroup_size(256)
fn p_gain_apply(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} var ev=b[i]; if(pu(1u)!=0u) {ev=c[i]+(ev-c[i])*(1.0-sstep(0.66,1.0,d[i]));} put_rgb(i,rgb_a(i)*exp2(ev));
}
@compute @workgroup_size(256)
fn p_band(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} out[i]=clamp(log2(max(a[i],1e-7)/max(b[i],1e-7)),-0.5,0.5);
}
@compute @workgroup_size(256)
fn p_eigf_gain(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} out[i]=0.25*tg(log2(max(a[i]/0.18,1e-7)))+0.5*tg(log2(max(b[i]/0.18,1e-7)))+0.25*tg(log2(max(c[i]/0.18,1e-7)));
}
// vkdt local-Laplacian stencil path: ceil-sized levels, replicated edges, down to 1 pixel.
fn coarse_b(x: i32,y: i32,w: u32,h: u32,ch: u32,nc: u32) -> f32 {return b[(u32(clamp(y,0,i32(h)-1))*w+u32(clamp(x,0,i32(w)-1)))*nc+ch];}
fn expand_b(x: u32,y: u32,w: u32,h: u32,ch: u32,nc: u32) -> f32 {
    var s=0.0; let ex=(x&1u)==0u; let ey=(y&1u)==0u;
    for(var j=select(0,-1,ey);j<=1;j++) {var wy=0.5; if(ey) {wy=select(0.125,0.75,j==0);}
        for(var k=select(0,-1,ex);k<=1;k++) {var wx=0.5; if(ex) {wx=select(0.125,0.75,k==0);} s+=wx*wy*coarse_b(i32(x/2u)+k,i32(y/2u)+j,w,h,ch,nc);}}
    return s;
}
@compute @workgroup_size(256)
fn p_ll_reduce(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let k=lin_index(g,ng); if(k>=pu(0u)) {return;} let w=pu(1u); let h=pu(2u); let nc=pu(3u);let cw=(w+1u)/2u; let x=k%cw; let y=k/cw;
    let weights=array<f32,5>(0.0625,0.25,0.375,0.25,0.0625);
    for(var ch=0u;ch<nc;ch++) {var sum=0.0;
        for(var j=0u;j<5u;j++) {for(var i=0u;i<5u;i++) {let xx=u32(clamp(i32(2u*x+i)-2,0,i32(w)-1)); let yy=u32(clamp(i32(2u*y+j)-2,0,i32(h)-1)); sum+=a[(yy*w+xx)*nc+ch]*weights[i]*weights[j];}}
        out[k*nc+ch]=sum;
    }
}
fn tone_basis(v: f32) -> vec4<f32> {return vec4<f32>(1.6*sstep(0.0,4.0,v),1.8*(1.0-sstep(-6.0,0.0,v)),0.8*sstep(3.0,7.0,v),0.8*(1.0-sstep(-10.0,-4.0,v)));}
@compute @workgroup_size(256)
fn p_ll_remap(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let lo=min(b[0],-6.0); let hi=max(b[4],4.0); let step=max(hi-lo,1.0)/9.0;
    let gam=lo+f32(pu(1u))*step; let sigma=max(step,1.0); let v=a[i]; let delta=v-gam; let keep=exp(-1.5*delta*delta/(sigma*sigma));
    let remapped=vec4<f32>(v)+tone_basis(gam)*keep+tone_basis(v)*(1.0-keep);
    for(var ch=0u;ch<4u;ch++) {out[4u*i+ch]=remapped[ch];}
}
@compute @workgroup_size(256)
fn p_ll_acc(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let w=pu(1u); let h=pu(2u); let lo=min(d[0],-6.0); let hi=max(d[4],4.0); let step=max(hi-lo,1.0)/9.0;
    let t=clamp((c[i]-lo)/step,0.0,9.0); let low=u32(min(floor(t),8.0)); let f=t-f32(low); let gam=pu(3u); var weight=0.0;
    if(gam==low) {weight=1.0-f;} else if(gam==low+1u) {weight=f;}
    for(var ch=0u;ch<4u;ch++) {var ex=0.0; if(pu(4u)!=0u) {ex=expand_b(i%w,i/w,(w+1u)/2u,(h+1u)/2u,ch,4u);} out[4u*i+ch]=out[4u*i+ch]+weight*(a[4u*i+ch]-ex);}
}
@compute @workgroup_size(256)
fn p_ll_assemble(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let w=pu(1u); let h=pu(2u);
    for(var ch=0u;ch<4u;ch++) {out[4u*i+ch]=a[4u*i+ch]+expand_b(i%w,i/w,(w+1u)/2u,(h+1u)/2u,ch,4u);}
}
@compute @workgroup_size(256)
fn p_ll_gain(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} for(var ch=0u;ch<4u;ch++) {out[4u*i+ch]=a[4u*i+ch]-b[i];}
}
@compute @workgroup_size(256)
fn p_ll_mix(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} out[i]=clamp(a[4u*i]*pf(1u)+a[4u*i+1u]*pf(2u)+a[4u*i+2u]*pf(3u)+a[4u*i+3u]*pf(4u),-3.0,3.0);
}
// darktable's padded pyramid (Clarity). Two channels retain both remappings' rounding.
@compute @workgroup_size(256)
fn p_dt_pad(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let iw=pu(1u); let ih=pu(2u); let pad=pu(3u); let w=iw+2u*pad;
    let x=u32(clamp(i32(i%w)-i32(pad),0,i32(iw)-1)); let y=u32(clamp(i32(i/w)-i32(pad),0,i32(ih)-1)); out[i]=clamp((a[y*iw+x]+12.0)/24.0,0.0,1.0);
}
fn dt_curve(x: f32,gam: f32,clarity: f32) -> f32 {
    let sigma=1.5/24.0; let delta=x-gam; var v=0.0;
    if(delta>2.0*sigma) {v=gam+sigma+(delta-sigma);} else if(delta< -2.0*sigma) {v=gam-sigma+(delta+sigma);}
    else if(delta>0.0) {let t=clamp(delta/(2.0*sigma),0.0,1.0); v=gam+sigma*2.0*(1.0-t)*t+t*t*(sigma+sigma);}
    else {let t=clamp(-delta/(2.0*sigma),0.0,1.0); v=gam-sigma*2.0*(1.0-t)*t+t*t*(-sigma-sigma);}
    let arg=-delta*delta/(2.0*sigma*sigma/3.0); let fast=bitcast<f32>(u32(max(i32(1065353216.0+arg*11401300.0),0)));
    return v+clarity*delta*fast;
}
@compute @workgroup_size(256)
fn p_dt_remap(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let gam=(f32(pu(1u))+0.5)/12.0; out[2u*i]=dt_curve(a[i],gam,1.0); out[2u*i+1u]=dt_curve(a[i],gam,0.0);
}
fn dt_vert(x: u32,y: u32,w: u32,nc: u32,ch: u32) -> f32 {
    let r0=a[(y*w+x)*nc+ch]+a[((y+4u)*w+x)*nc+ch];
    let r1=a[((y+1u)*w+x)*nc+ch]+a[((y+2u)*w+x)*nc+ch]+a[((y+3u)*w+x)*nc+ch];
    let r02=r0+a[((y+2u)*w+x)*nc+ch]+a[((y+2u)*w+x)*nc+ch]; return r02+r1*4.0;
}
@compute @workgroup_size(256)
fn p_dt_reduce(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let w=pu(1u); let h=pu(2u); let nc=pu(3u); let cw=(w+1u)/2u; let ch=(h+1u)/2u;
    if(cw<=2u || ch<=2u) {for(var k=0u;k<nc;k++) {out[nc*i+k]=0.0;} return;}
    let x=clamp(i%cw,1u,cw-2u); let y=clamp(i/cw,1u,ch-2u); let bx=2u*(x-1u); let by=2u*(y-1u);
    for(var k=0u;k<nc;k++) {
        let v0=dt_vert(bx,by,w,nc,k); let v1=dt_vert(bx+1u,by,w,nc,k); let v2=dt_vert(bx+2u,by,w,nc,k); let v3=dt_vert(bx+3u,by,w,nc,k); let v4=dt_vert(bx+4u,by,w,nc,k);
        var v=(v0+4.0*v1+6.0*v2+4.0*v3+v4)/256.0;
        // Upstream calculates alternating output columns with a differently associated sum.
        if((x&1u)==0u) {v=(v0+4.0*(v1+v3)+6.0*v2+v4)/256.0;}
        out[nc*i+k]=v;
    }
}
fn dt_expand(x: u32,y: u32,w: u32,nc: u32,k: u32) -> f32 {
    let cw=(w+1u)/2u; let ind=(y/2u)*cw+x/2u; let v=b[ind*nc+k]; let mode=(x&1u)+2u*(y&1u);
    if(mode==0u) {return (6.0*(b[(ind-cw)*nc+k]+b[(ind-1u)*nc+k]+6.0*v+b[(ind+1u)*nc+k]+b[(ind+cw)*nc+k])+b[(ind-cw-1u)*nc+k]+b[(ind-cw+1u)*nc+k]+b[(ind+cw-1u)*nc+k]+b[(ind+cw+1u)*nc+k])/64.0;}
    if(mode==1u) {return (24.0*(v+b[(ind+1u)*nc+k])+4.0*(b[(ind-cw)*nc+k]+b[(ind-cw+1u)*nc+k]+b[(ind+cw)*nc+k]+b[(ind+cw+1u)*nc+k]))/64.0;}
    if(mode==2u) {return (24.0*(v+b[(ind+cw)*nc+k])+4.0*(b[(ind-1u)*nc+k]+b[(ind+1u)*nc+k]+b[(ind+cw-1u)*nc+k]+b[(ind+cw+1u)*nc+k]))/64.0;}
    return 0.25*(v+b[(ind+1u)*nc+k]+b[(ind+cw)*nc+k]+b[(ind+cw+1u)*nc+k]);
}
@compute @workgroup_size(256)
fn p_dt_acc(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let w=pu(1u); let h=pu(2u); let v=c[i]; var hi=1u;
    for(;hi<11u && (f32(hi)+0.5)/12.0<=v;hi++) {}
    let low=(f32(hi)-0.5)/12.0; let high=(f32(hi)+0.5)/12.0; let t=clamp((v-low)/(high-low),0.0,1.0); var weight=0.0;
    if(pu(3u)==hi-1u) {weight=1.0-t;} else if(pu(3u)==hi) {weight=t;}
    let x=clamp(i%w,1u,((w-1u)&~1u)-1u); let y=clamp(i/w,1u,((h-1u)&~1u)-1u);
    for(var k=0u;k<2u;k++) {out[2u*i+k]=out[2u*i+k]+(a[2u*i+k]-dt_expand(x,y,w,2u,k))*weight*pf(4u);}
}
@compute @workgroup_size(256)
fn p_dt_assemble(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let w=pu(1u); let h=pu(2u);
    // fill_boundary2 replicates the last two pixels of even dimensions.
    let x=clamp(i%w,1u,((w-1u)&~1u)-1u); let y=clamp(i/w,1u,((h-1u)&~1u)-1u);
    for(var k=0u;k<2u;k++) {out[2u*i+k]=dt_expand(x,y,w,2u,k)+a[2u*i+k];}
}
@compute @workgroup_size(256)
fn p_dt_top(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} out[2u*i]=a[i]; out[2u*i+1u]=a[i];
}
@compute @workgroup_size(256)
fn p_dt_crop(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let iw=pu(1u); let pad=pu(2u); let w=iw+2u*pad; let j=(i/iw+pad)*w+i%iw+pad; out[i]=(a[2u*j]-a[2u*j+1u])*24.0;
}
@compute @workgroup_size(256)
fn p_detail_apply(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let ll=d[i]; var ev=0.0;
    if(pu(4u)!=0u) {ev=b[i]*pf(1u)*exp(-(ll/3.5)*(ll/3.5));}
    if(pu(5u)!=0u) {ev+=c[i]*pf(2u);} if(pu(6u)!=0u) {ev+=c[pu(0u)+i]*pf(3u);}
    var v=rgb_a(i)*exp2(ev);
    if(pu(4u)!=0u && pu(7u)!=2u) {let lw=y_ls(1.0); var h=rgb_hsb(v,lw); var k=1.0+0.15*abs(ev);
        if(pu(7u)==0u) {k=1.0-0.15*abs(pf(1u))*sstep(2.0,6.0,ll)*clamp(h.y,0.0,1.0);} h.y*=k; v=hsb_rgb(gamut_hsb(h,lw),lw);}
    put_rgb(i,v);
}
@compute @workgroup_size(256)
fn p_mix(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} let t=c[pu(1u)+i]; let v=rgb_a(i); let q=vec3<f32>(b[3u*i],b[3u*i+1u],b[3u*i+2u]); put_rgb(i,v+(q-v)*t);
}

@compute @workgroup_size(256)
fn p_clip(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng); if(i>=pu(0u)) {return;} out[i]=clamp(a[3u*i],0.0,1.0);
}

// Tiled transpose makes both recursive sweeps read/write coalesced columns.
// The recurrence itself retains the reference's operation order and endpoints.
var<workgroup> transpose_tile: array<vec4<f32>,272>;
@compute @workgroup_size(16,16)
fn p_transpose(@builtin(global_invocation_id) g: vec3<u32>,@builtin(local_invocation_id) t: vec3<u32>,@builtin(workgroup_id) wg: vec3<u32>) {
    let w=pu(1u);let h=pu(2u);let nc=pu(3u);var v=vec4<f32>(0.0);
    if(g.x<w && g.y<h) {for(var ch=0u;ch<nc;ch++) {v[ch]=a[(g.y*w+g.x)*nc+ch];}}
    transpose_tile[t.y*17u+t.x]=v;workgroupBarrier();
    let x=wg.y*16u+t.x;let y=wg.x*16u+t.y;
    if(x<h && y<w) {let value=transpose_tile[t.x*17u+t.y];for(var ch=0u;ch<nc;ch++) {out[(y*h+x)*nc+ch]=value[ch];}}
}

// Skin's three signed targets share the same guide means/variance.
@compute @workgroup_size(256)
fn p_skin_moments(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng);if(i>=pu(0u)) {return;}let p=a[4u*i+pu(1u)];out[2u*i]=p;out[2u*i+1u]=p*b[i];
}
@compute @workgroup_size(256)
fn p_skin_low(@builtin(global_invocation_id) g: vec3<u32>,@builtin(num_workgroups) ng: vec3<u32>) {
    let i=lin_index(g,ng);if(i>=pu(0u)) {return;}let v=a[i];let mg=b[2u*i];let mp=c[2u*i];
    let aa=(c[2u*i+1u]-mg*mp)/(max(b[2u*i+1u]-mg*mg,0.0)+0.05*max(mg*v,1e-6));out[i]=aa*v+mp-aa*mg;
}
