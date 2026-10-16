// FP32 NCHW restoration. ONNX Conv/ConvTranspose and DepthToSpace, opset 20.
// P: ci, hi, wi, co, ho, wo, kh, kw, sy, sx, py, px, transpose, op, alpha, block, crd.
var<workgroup> da: array<f32, 256>;
var<workgroup> dw: array<f32, 256>;
fn denoise_sample(position: u32, k: u32) -> f32 {
    let plane = P[4]*P[5];
    if position >= plane || k >= P[0]*P[6]*P[7] { return 0.0; }
    let c=k/(P[6]*P[7]); let ky=(k/P[7])%P[6]; let kx=k%P[7];
    var y=i32(position/P[5]*P[8])+i32(ky)-i32(P[10]);
    var x=i32(position%P[5]*P[9])+i32(kx)-i32(P[11]);
    if P[12]!=0u {
        y=i32(position/P[5])+i32(P[10])-i32(ky);
        x=i32(position%P[5])+i32(P[11])-i32(kx);
        if y<0 || x<0 || y%i32(P[8])!=0 || x%i32(P[9])!=0 { return 0.0; }
        y/=i32(P[8]); x/=i32(P[9]);
    }
    if y<0 || x<0 || y>=i32(P[1]) || x>=i32(P[2]) { return 0.0; }
    return a[c*P[1]*P[2]+u32(y)*P[2]+u32(x)];
}
fn denoise_weight(c: u32,k: u32) -> f32 {
    if c>=P[3] || k>=P[0]*P[6]*P[7] { return 0.0; }
    if P[12]==0u { return weights[c*P[0]*P[6]*P[7]+k]; }
    let ci=k/(P[6]*P[7]); let kernel=k%(P[6]*P[7]);
    return weights[(ci*P[3]+c)*P[6]*P[7]+kernel];
}
@compute @workgroup_size(8,8,1)
fn denoise_conv(@builtin(workgroup_id) group:vec3<u32>, @builtin(local_invocation_id) local:vec3<u32>) {
    let pos=group.x*16u+local.x; let co=group.y*16u+local.y;
    let lane=local.y*8u+local.x;
    var acc=vec4<f32>(0.0);
    let nk=P[0]*P[6]*P[7];
    for(var base=0u;base<nk;base+=16u) {
        for(var j=lane;j<256u;j+=64u) {
            da[j]=denoise_sample(group.x*16u+j/16u,base+j%16u);
            dw[j]=denoise_weight(group.y*16u+j/16u,base+j%16u);
        }
        workgroupBarrier();
        for(var k=0u;k<16u;k++) {
            let av=vec2<f32>(da[local.x*16u+k],da[(local.x+8u)*16u+k]);
            let bv=vec2<f32>(dw[local.y*16u+k],dw[(local.y+8u)*16u+k]);
            acc=fma(vec4<f32>(av.x,av.y,av.x,av.y),vec4<f32>(bv.x,bv.x,bv.y,bv.y),acc);
        }
        workgroupBarrier();
    }
    let plane=P[4]*P[5];
    if pos<plane && co<P[3] { dst[co*plane+pos]=acc.x+bias[co]; }
    if pos+8u<plane && co<P[3] { dst[co*plane+pos+8u]=acc.y+bias[co]; }
    if pos<plane && co+8u<P[3] { dst[(co+8u)*plane+pos]=acc.z+bias[co+8u]; }
    if pos+8u<plane && co+8u<P[3] { dst[(co+8u)*plane+pos+8u]=acc.w+bias[co+8u]; }
}
@compute @workgroup_size(256)
fn denoise_point(@builtin(global_invocation_id) id:vec3<u32>, @builtin(num_workgroups) groups:vec3<u32>) {
    let i=lin_index(id,groups); let plane=P[4]*P[5]; if i>=P[3]*plane {return;}
    let c=i/plane; let y=(i%plane)/P[5]; let x=i%P[5];
    switch P[13] {
        case 1u: { let v=a[i]; dst[i]=select(v*bitcast<f32>(P[14]),v,v>=0.0); }
        case 2u: { dst[i]=1.0/(1.0+exp(-a[i])); }
        case 3u: {
            var v=-3.402823e38;
            for(var ky=0u;ky<P[6];ky++) { for(var kx=0u;kx<P[7];kx++) {
                v=max(v,a[c*P[1]*P[2]+(y*P[8]+ky)*P[2]+x*P[9]+kx]);
            }} dst[i]=v;
        }
        case 4u: { let size=P[0]*P[1]*P[2]; if i<size {dst[i]=a[i];} else {dst[i]=b[i-size];} }
        case 5u: {
            let block=P[15]; let offset=(y%block)*block+x%block;
            var ic=offset*P[3]+c; if P[16]!=0u {ic=c*block*block+offset;}
            dst[i]=a[ic*P[1]*P[2]+(y/block)*P[2]+x/block];
        }
        default: { dst[i]=a[i]; }
    }
}
