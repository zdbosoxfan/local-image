// Capture sharpening: twin of lc-pipeline/src/capture.rs. darktable capture.c at
// 733bd69f32cac7ff5e41025115942772add1f088; GPL-3.0-or-later.
// Copyright (C) 2025-2026 darktable developers; algorithm by Ingo Weyrich (RawTherapee).
// P: width, height, sigma, threshold, corner boost, centre, clip enabled, clip.
// RL uses the true, disc-truncated Gaussian table, NOT separable/box approximations.

fn capture_rgb(i: u32) -> vec3<f32> {
    return vec3<f32>(img[3u*i], img[3u*i+1u], img[3u*i+2u]);
}

// One residual correction prevents an approximate hardware reciprocal from moving an
// exactly integral sigma/0.01 just below the kernel-index truncation boundary.
fn capture_quotient(n: f32, d: f32) -> f32 {
    let q = n / d;
    return q + fma(-q, d, n) / d;
}

@compute @workgroup_size(16,16)
fn capture_lum(@builtin(global_invocation_id) g: vec3<u32>) {
    let w=pu(0u); let h=pu(1u);
    if(g.x>=w || g.y>=h) {return;}
    let i=g.y*w+g.x;
    dst[i]=max(lum2020(capture_rgb(i)),0.0);
}

@compute @workgroup_size(16,16)
fn capture_mask(@builtin(global_invocation_id) g: vec3<u32>) {
    let w=pu(0u); let h=pu(1u);
    if(g.x>=w || g.y>=h) {return;}
    let i=g.y*w+g.x;
    let rw=f32(w)/2.0; let rh=f32(h)/2.0;
    let fr=f32(g.y)-rh; let fc=f32(g.x)-rw;
    let sc=sqrt(fr*fr+fc*fc)/max(min(rw,rh),1.0);
    let cboost=1.0+8.0*pf(5u)*pf(5u);
    let radial=max(sc-0.5-pf(5u),0.0);
    let radial_sq=radial*radial;
    let corr=cboost*pf(4u)*radial_sq;
    let border=min(min(min(h-g.y-1u,w-g.x-1u),min(g.x,g.y)),8u);
    let sigma=(pf(2u)+corr)*0.125*f32(border);
    indices[i]=u32(clamp(capture_quotient(sigma,0.01),0.0,255.0));

    // Gather the interior black/clipped neighbours that scatter zeros in the CPU
    // mask. The 21-pixel disc is symmetric; the gather avoids write races entirely.
    var allowed=g.x>1u && g.y>1u && g.x+2u<w && g.y+2u<h;
    for(var dy=-2;dy<=2;dy++) {
        let dxs=select(2,1,abs(dy)==2);
        for(var dx=-dxs;dx<=dxs;dx++) {
            let y=i32(g.y)+dy; let x=i32(g.x)+dx;
            if(y<=1 || x<=1 || y+2>=i32(h) || x+2>=i32(w)) {continue;}
            let j=u32(y)*w+u32(x);
            let clipped=pu(6u)!=0u && any(capture_rgb(j)>=vec3<f32>(pf(7u)));
            if(clipped || a[j]<0.001) {allowed=false;}
        }
    }
    if(!allowed) {dst[i]=0.0; return;}

    // Same f32 addition order as blend_mask: three full rows, then alternating
    // top/bottom pixels of the two short rows. No dot product / variance identity.
    var sum=0.0; var sq=0.0;
    for(var y=g.y-1u;y<g.y+2u;y++) {
        for(var x=g.x-2u;x<g.x+3u;x++) {
            let v=a[y*w+x]; sum=sum+v; sq=sq+v*v;
        }
    }
    for(var x=g.x-1u;x<g.x+2u;x++) {
        let up=a[(g.y-2u)*w+x]; sum=sum+up; sq=sq+up*up;
        let down=a[(g.y+2u)*w+x]; sum=sum+down; sq=sq+down*down;
    }
    let ss=max(sq-sum*sum/21.0,0.0);
    let sd=sqrt(ss/21.0);
    let mean=max(sum/21.0,1.52587890625e-05);
    let t=log(1.0+sd/sqrt(mean));
    let threshold=0.6*pf(3u)*pf(3u);
    let offset=-2.5+200.0*threshold/2.0;
    let weight=1.0/(1.0+exp(offset-200.0*t));
    dst[i]=clamp(1.01011*(weight-0.01),0.0,1.0);
}

@compute @workgroup_size(16,16)
fn capture_blend(@builtin(global_invocation_id) g: vec3<u32>) {
    let w=pu(0u); let h=pu(1u);
    if(g.x>=w || g.y>=h) {return;}
    let i=g.y*w+g.x;
    let wt=1.0/(1.0+exp(5.0-10.0*(a[i]-b[i])));
    dst[i]=clamp(wt*a[i]+(1.0-wt)*b[i],0.0,1.0);
}

// A 16x16 workgroup loads one 24x24 tile including the largest (4px) halo.
// Every lane participates before any bounds/mask return, including odd-size edges.
var<workgroup> capture_tile: array<f32,576>;
fn capture_load_tile(l: vec3<u32>, group: vec3<u32>) {
    let w=pu(0u); let h=pu(1u);
    for(var j=l.y*16u+l.x;j<576u;j=j+256u) {
        let x=i32(group.x*16u+j%24u)-4;
        let y=i32(group.y*16u+j/24u)-4;
        var v=0.0;
        if(x>=0 && y>=0 && x<i32(w) && y<i32(h)) {v=a[u32(y)*w+u32(x)];}
        capture_tile[j]=v;
    }
    workgroupBarrier();
}

fn capture_convolve(g: vec3<u32>, l: vec3<u32>, index: u32) -> f32 {
    // CPU: small=(0.66f32/0.01f32) as u8 == 66. Preserve its row-major
    // accumulation, including zero coefficients in the disc's bounding square.
    let bd=select(4,2,index<66u);
    var val=0.0;
    for(var dy=-bd;dy<=bd;dy++) {
        let y=i32(g.y)+dy;
        if(y<0 || y>=i32(pu(1u))) {continue;}
        for(var dx=-bd;dx<=bd;dx++) {
            let x=i32(g.x)+dx;
            if(x<0 || x>=i32(pu(0u))) {continue;}
            let k=kernels[index*25u+u32(5*abs(dy)+abs(dx))];
            let j=u32(i32(l.y)+4+dy)*24u+u32(i32(l.x)+4+dx);
            val=val+k*capture_tile[j];
        }
    }
    return val;
}

@compute @workgroup_size(16,16)
fn capture_div(@builtin(global_invocation_id) g: vec3<u32>,
               @builtin(local_invocation_id) l: vec3<u32>, @builtin(workgroup_id) group: vec3<u32>) {
    capture_load_tile(l,group);
    let w=pu(0u); let h=pu(1u);
    if(g.x>=w || g.y>=h) {return;}
    let i=g.y*w+g.x;
    var v=1.0;
    if(blend[i]>0.0) {v=capture_quotient(b[i],max(capture_convolve(g,l,indices[i]),0.001));}
    dst[i]=v;
}

@compute @workgroup_size(16,16)
fn capture_mul(@builtin(global_invocation_id) g: vec3<u32>,
               @builtin(local_invocation_id) l: vec3<u32>, @builtin(workgroup_id) group: vec3<u32>) {
    capture_load_tile(l,group);
    let w=pu(0u); let h=pu(1u);
    if(g.x>=w || g.y>=h) {return;}
    let i=g.y*w+g.x;
    var v=b[i];
    if(blend[i]>0.0) {v=v*capture_convolve(g,l,indices[i]);}
    dst[i]=v;
}

@compute @workgroup_size(16,16)
fn capture_apply(@builtin(global_invocation_id) g: vec3<u32>) {
    let w=pu(0u); let h=pu(1u);
    if(g.x>=w || g.y>=h) {return;}
    let i=g.y*w+g.x;
    var rgb=capture_rgb(i);
    if(blend[i]>0.0) {
        let new_l=clamp(blend[i],0.0,1.0)*(b[i]-a[i])+a[i];
        rgb=rgb*capture_quotient(new_l,max(a[i],0.001));
    }
    dst[3u*i]=rgb.x; dst[3u*i+1u]=rgb.y; dst[3u*i+2u]=rgb.z;
}
