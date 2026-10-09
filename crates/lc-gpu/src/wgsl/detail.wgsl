// Scalar twins of detail/nr.rs and detail/haze.rs. darktable GPL-3.0-or-later,
// commit 733bd69f32cac7ff5e41025115942772add1f088; see licenses/darktable-NOTICE.md.
// Bindings a,b,c inputs, dst output; P[0] item count.
fn drgb(i:u32)->vec3<f32> {return vec3<f32>(a[i*3u],a[i*3u+1u],a[i*3u+2u]);}
fn dput(i:u32,v:vec3<f32>) {dst[i*3u]=v.x;dst[i*3u+1u]=v.y;dst[i*3u+2u]=v.z;}
fn dpow(x:f32,e:f32)->f32 {if(x<=0.0){return 0.0;}return pow(x,e);}
fn matrix_at(offset:u32,v:vec3<f32>)->vec3<f32> {
    return vec3<f32>(pf(offset)*v.x+pf(offset+1u)*v.y+pf(offset+2u)*v.z,
        pf(offset+3u)*v.x+pf(offset+4u)*v.y+pf(offset+5u)*v.z,
        pf(offset+6u)*v.x+pf(offset+7u)*v.y+pf(offset+8u)*v.z);
}
// b: device-resolved VST metadata (a,b,p,bias,wb,to9,from9).
fn vst_matrix(offset:u32,v:vec3<f32>)->vec3<f32> {
    return vec3<f32>(b[offset]*v.x+b[offset+1u]*v.y+b[offset+2u]*v.z,
        b[offset+3u]*v.x+b[offset+4u]*v.y+b[offset+5u]*v.z,
        b[offset+6u]*v.x+b[offset+7u]*v.y+b[offset+8u]*v.z);
}
@compute @workgroup_size(256)
fn nr_forward(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}
    let rgb=drgb(i);let expn=1.0-b[2u]*0.5;let scale=2.0/((2.0-b[2u])*sqrt(b[0u]));
    let v=vec3<f32>(dpow(rgb.x+b[1u],expn),dpow(rgb.y+b[1u],expn),dpow(rgb.z+b[1u],expn))*scale;
    dput(i,vst_matrix(5u,v));
}
@compute @workgroup_size(256)
fn nr_backward(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}
    let v=max(vst_matrix(14u,drgb(i)),vec3<f32>(0.0));
    let z=(v+sqrt(max(v*v+b[3u]*b[4u],vec3<f32>(0.0))))*(sqrt(b[0u])*(2.0-b[2u])*0.25);
    let e=1.0/(1.0-b[2u]*0.5);
    dput(i,vec3<f32>(dpow(z.x,e),dpow(z.y,e),dpow(z.z,e))-b[1u]);
}
fn dmexp(x:f32)->f32 {
    let i1=f32(0x3f800000u);let i2=f32(0x3f000000u);let k=i1+x*(i2-i1);
    if(k<f32(0x800000u)){return 0.0;}return bitcast<f32>(u32(k));
}
// P:n,w,h,level,inv_sigma2; 5x5 nonseparable EAW.
@compute @workgroup_size(256)
fn nr_eaw(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}
    let w=pu(1u);let h=pu(2u);let step=i32(1u<<pu(3u));let x=i32(i%w);let y=i32(i/w);
    let f=array<f32,5>(0.0625,0.25,0.375,0.25,0.0625);let center=drgb(i);
    var sum=vec3<f32>(0.0);var weight=0.0;
    for(var j=0u;j<5u;j++) {let yy=u32(clamp(y+(i32(j)-2)*step,0,i32(h)-1));
        for(var k=0u;k<5u;k++) {let xx=u32(clamp(x+(i32(k)-2)*step,0,i32(w)-1));
            let v=drgb(yy*w+xx);let sq=(center-v)*(center-v);let d=(sq.x+sq.y+sq.z)*pf(4u);
            let ww=f[j]*f[k]*dmexp(max(d*0.02-9.0,0.0));weight+=ww;sum+=ww*v;
        }
    } dput(i,sum/weight);
}
@compute @workgroup_size(256)
fn nr_detail(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}
    dput(i,drgb(i)-vec3<f32>(b[i*3u],b[i*3u+1u],b[i*3u+2u]));
}
var<workgroup> dpartial:array<vec3<f32>,256>;
@compute @workgroup_size(256)
fn nr_sum(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) lid:u32,@builtin(workgroup_id) group:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);var v=vec3<f32>(0.0);if(i<pu(0u)){let d=drgb(i);v=d*d;}
    dpartial[lid]=v;workgroupBarrier();
    for(var step=128u;step>0u;step=step/2u) {if(lid<step){dpartial[lid]+=dpartial[lid+step];}workgroupBarrier();}
    if(lid==0u){dput(group.y*nw.x+group.x,dpartial[0u]);}
}
// a accumulated, b detail; P:n,t0,t1,t2
@compute @workgroup_size(256)
fn nr_synthesize(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}
    let d=vec3<f32>(b[i*3u],b[i*3u+1u],b[i*3u+2u]);let t=vec3<f32>(c[0],c[1],c[2]);
    dput(i,drgb(i)+min(d+t,vec3<f32>(0.0))+max(d-t,vec3<f32>(0.0)));
}
@compute @workgroup_size(256)
fn nr_residual(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}
    dput(i,drgb(i)+vec3<f32>(b[i*3u],b[i*3u+1u],b[i*3u+2u]));
}
// a original,b filtered; P:n,lum,col
@compute @workgroup_size(256)
fn nr_join(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}
    let orig=drgb(i);let f=vec3<f32>(b[i*3u],b[i*3u+1u],b[i*3u+2u]);let y=lum2020(orig);let fy=max(lum2020(f),1e-12);
    var out=f;
    if(pu(2u)==0u){out=orig;if(y>1e-12){out=orig*fy/y;}}
    else if(pu(1u)==0u){out=f*y/fy;}
    dput(i,out);
}
// a guide RGB,b input plane; P:n,mode (0 mean4,1 moments9).
@compute @workgroup_size(256)
fn haze_moments(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}
    let v=drgb(i);let p=b[i];
    if(pu(1u)==0u){dst[i*4u]=p;dst[i*4u+1u]=v.x;dst[i*4u+2u]=v.y;dst[i*4u+3u]=v.z;}
    else {let values=array<f32,9>(v.x*p,v.y*p,v.z*p,v.x*v.x,v.x*v.y,v.x*v.z,v.y*v.y,v.y*v.z,v.z*v.z);
        for(var k=0u;k<9u;k++){dst[i*9u+k]=values[k];}}
}
// a mean4,b moments9; P:n,eps.
fn haze_coeff(inp:f32,r:f32,gg:f32,blue:f32,v:array<f32,9>,eps:f32)->vec4<f32> {
    let rr=v[3]-r*r+eps;let rg=v[4]-r*gg;let rb=v[5]-r*blue;
    let g2=v[6]-gg*gg+eps;let gb=v[7]-gg*blue;let bb=v[8]-blue*blue+eps;
    let det=rr*(g2*bb-gb*gb)-rg*(rg*bb-rb*gb)+rb*(rg*gb-rb*g2);
    var ar=0.0;var ag=0.0;var ab=0.0;var b0=inp;
    if(abs(det)>4.0*1.1920928955078125e-7) {
        let cr=v[0]-r*inp;let cg=v[1]-gg*inp;let cb=v[2]-blue*inp;
        ar=(cr*(g2*bb-gb*gb)-rg*(cg*bb-cb*gb)+rb*(cg*gb-cb*g2))/det;
        ag=(rr*(cg*bb-cb*gb)-cr*(rg*bb-rb*gb)+rb*(rg*cb-rb*cg))/det;
        ab=(rr*(g2*cb-gb*cg)-rg*(rg*cb-rb*cg)+cr*(rg*gb-rb*g2))/det;
        b0=inp-ar*r-ag*gg-ab*blue;
    }
    return vec4<f32>(ar,ag,ab,b0);
}
@compute @workgroup_size(256)
fn haze_solve(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}var v:array<f32,9>;for(var k=0u;k<9u;k++){v[k]=b[i*9u+k];}
    let q=haze_coeff(a[i*4u],a[i*4u+1u],a[i*4u+2u],a[i*4u+3u],v,pf(1u));for(var k=0u;k<4u;k++){dst[i*4u+k]=q[k];}
}
// RGB guide means/covariances are shared by both transmission signs.
@compute @workgroup_size(256)
fn haze_guide_moments(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}let v=drgb(i);
    let q=array<f32,9>(v.x,v.y,v.z,v.x*v.x,v.x*v.y,v.x*v.z,v.y*v.y,v.y*v.z,v.z*v.z);for(var k=0u;k<9u;k++){dst[i*9u+k]=q[k];}
}
@compute @workgroup_size(256)
fn haze_input_moments(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}let v=drgb(i);let p=b[i];let q=vec4<f32>(p,v*p);for(var k=0u;k<4u;k++){dst[i*4u+k]=q[k];}
}
@compute @workgroup_size(256)
fn haze_solve_shared(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}var v:array<f32,9>;for(var k=0u;k<3u;k++){v[k]=b[i*4u+k+1u];}for(var k=3u;k<9u;k++){v[k]=a[i*9u+k];}
    let q=haze_coeff(b[i*4u],a[i*9u],a[i*9u+1u],a[i*9u+2u],v,pf(1u));for(var k=0u;k<4u;k++){dst[i*4u+k]=q[k];}
}
@compute @workgroup_size(256)
fn haze_apply(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}
    let v=drgb(i);dst[i]=b[i*4u]*v.x+b[i*4u+1u]*v.y+b[i*4u+2u]*v.z+b[i*4u+3u];
}
@compute @workgroup_size(256)
fn haze_dark(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}
    var v=drgb(i);if(pu(1u)!=0u){v/=max(vec3<f32>(b[0],b[1],b[2]),vec3<f32>(1e-6));}dst[i]=min(min(v.x,v.y),v.z);
}
// a b2, P:n,w,h,mask threshold,edge factor
@compute @workgroup_size(256)
fn sharp_preview(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}
    let w=pu(1u);let h=pu(2u);let x=i%w;let y=i/w;
    let xl=u32(max(i32(x)-1,0));let xr=min(x+1u,w-1u);let yu=u32(max(i32(y)-1,0));let yd=min(y+1u,h-1u);
    let gx=(a[y*w+xr]-a[y*w+xl])*0.5;let gy=(a[yd*w+x]-a[yu*w+x])*0.5;
    var m=1.0;if(pf(3u)>0.0){m=sstep(pf(3u)*0.5,pf(3u),sqrt(gx*gx+gy*gy)*pf(4u));}dst[i]=m;
}
