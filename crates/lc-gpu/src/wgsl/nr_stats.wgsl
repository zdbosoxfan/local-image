// Robust 4x4 tile noise estimation, exact radix order statistics and recursive band reduction.
// The sample locations, finite guards and quantile ranks are detail/nr.rs's reference.
@compute @workgroup_size(256)
fn nr_samples(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}
    let tile=i/16384u;let j=i%16384u;let w=pu(1u);let h=pu(2u);
    let x0=(tile%4u)*w/4u;let y0=(tile/4u)*h/4u;
    let x1=min((tile%4u+1u)*w/4u,x0+64u);let y1=min((tile/4u+1u)*h/4u,y0+64u);
    var value=bitcast<f32>(0xffffffffu);
    let tw=u32(max(i32(x1)-i32(x0)-1,0));let th=u32(max(i32(y1)-i32(y0)-1,0));
    if(j<tw*th*3u){
        let ch=j%3u;let x=x0+(j/3u)%tw;let y=y0+(j/3u)/tw;
        let aa=a[3u*(y*w+x)+ch];let bb=a[3u*(y*w+x+1u)+ch];
        let cc=a[3u*((y+1u)*w+x)+ch];let dd=a[3u*((y+1u)*w+x+1u)+ch];
        let mean=max((aa+bb+cc+dd)*0.25,1e-5);let hp=(aa-bb-cc+dd)*0.5;
        if(abs(hp)<=3.402823466e38 && abs(mean)<=3.402823466e38){value=abs(hp)/sqrt(mean);}
    }
    dst[i]=value;
}
var<workgroup> counts:array<u32,256>;
var<workgroup> prefix:u32;
var<workgroup> rank:u32;
var<workgroup> valid:u32;
fn count_reduce(lid:u32,v:u32) {
    counts[lid]=v;workgroupBarrier();
    for(var step=128u;step>0u;step/=2u){if(lid<step){counts[lid]+=counts[lid+step];}workgroupBarrier();}
}
@compute @workgroup_size(256)
fn nr_median(@builtin(local_invocation_index) lid:u32,@builtin(workgroup_id) wg:vec3<u32>) {
    let off=wg.x*16384u;var count=0u;
    for(var j=lid;j<16384u;j+=256u){if(bitcast<u32>(a[off+j])<=0x7f800000u){count++;}}
    count_reduce(lid,count);
    if(lid==0u){valid=counts[0];rank=valid/2u;prefix=0u;}workgroupBarrier();
    for(var bit=30i;bit>=0i;bit--){
        let mask=0xffffffffu<<u32(bit);count=0u;
        for(var j=lid;j<16384u;j+=256u){let v=bitcast<u32>(a[off+j]);if(v<=0x7f800000u && (v&mask)==prefix){count++;}}
        count_reduce(lid,count);
        if(lid==0u && rank>=counts[0]){rank-=counts[0];prefix|=1u<<u32(bit);}workgroupBarrier();
    }
    if(lid==0u){let sd=bitcast<f32>(prefix)/0.67448975;dst[wg.x]=select(-1.0,sd*sd,valid>=24u);}
}
// P:n,scale,maximum amount, then unity-WB conversion matrices (to/from).
@compute @workgroup_size(1)
fn nr_vst() {
    var slopes:array<f32,16>;var count=0u;
    for(var i=0u;i<16u;i++){let v=a[i];if(v>=0.0){var j=count;while(j>0u && slopes[j-1u]>v){slopes[j]=slopes[j-1u];j--;}slopes[j]=v;count++;}}
    var model=0.0001;if(count>0u){model=clamp(slopes[count/4u],1e-8,0.02);}
    let shadows=clamp(0.1-0.1*log(model),0.7,1.8);let scale=pf(1u);
    let wb=(0.4+1.6*pf(2u))*2.5*scale;
    dst[0]=model*0.05/pow(0.05,shadows);dst[1]=0.0;dst[2]=max(shadows+0.1*log(scale),0.0);dst[3]=-0.5*log(scale);dst[4]=wb;
    for(var i=0u;i<9u;i++){dst[5u+i]=pf(3u+i)/wb;dst[14u+i]=pf(12u+i)*wb;}
    dst[23]=model;
}
@compute @workgroup_size(256)
fn nr_reduce(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) lid:u32,@builtin(workgroup_id) wg:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let i=lin_index(g,nw);var v=vec3<f32>(0.0);if(i<pu(0u)){v=vec3<f32>(a[3u*i],a[3u*i+1u],a[3u*i+2u]);}
    partial[lid]=v;workgroupBarrier();
    for(var step=128u;step>0u;step/=2u){if(lid<step){partial[lid]+=partial[lid+step];}workgroupBarrier();}
    if(lid==0u){let j=3u*(wg.y*nw.x+wg.x);dst[j]=partial[0].x;dst[j+1u]=partial[0].y;dst[j+2u]=partial[0].z;}
}
var<workgroup> partial:array<vec3<f32>,256>;
@compute @workgroup_size(1)
fn nr_thresholds() {
    for(var ch=0u;ch<3u;ch++){
        let vy=a[ch]/max(f32(pu(1u))-1.0,1.0);let sx=sqrt(max(vy-pf(2u),1e-6));let force=pf(3u+ch);
        dst[ch]=8.0*(force*force*4.0)*pf(2u)/sx;
    }
}
