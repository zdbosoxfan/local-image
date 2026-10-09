// Exact order-statistic ranks for haze's 95% dark/brightness criteria, device-wide reductions.
// Brightness samples follow the CPU's reverse-first-half / forward-second-half order.
fn source_index(i:u32,n:u32)->u32{let mid=n/2u;if(i<mid){return mid-1u-i;}return i;}
var<workgroup> sums:array<u32,256>;
@compute @workgroup_size(256)
fn haze_count(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) lid:u32,@builtin(workgroup_id) wg:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>){
    let i=lin_index(g,nw);var count=0u;if(i<pu(0u)){count=u32(a[source_index(i,pu(0u))]>=c[0]);}
    sums[lid]=count;workgroupBarrier();
    for(var step=128u;step>0u;step/=2u){if(lid<step){sums[lid]+=sums[lid+step];}workgroupBarrier();}
    if(lid==0u){dst[wg.y*nw.x+wg.x]=bitcast<f32>(sums[0]);}
}
@compute @workgroup_size(1)
fn haze_prefix(){var total=0u;for(var i=0u;i<pu(1u);i++){dst[i]=bitcast<f32>(total);total+=bitcast<u32>(a[i]);}dst[pu(1u)]=bitcast<f32>(total);}
@compute @workgroup_size(256)
fn haze_bright_fill(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) lid:u32,@builtin(workgroup_id) wg:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>){
    let i=lin_index(g,nw);var selected=0u;var index=0u;
    if(i<pu(0u)){index=source_index(i,pu(0u));selected=u32(a[index]>=c[0]);}
    sums[lid]=selected;workgroupBarrier();
    for(var step=1u;step<256u;step*=2u){var prev=0u;if(lid>=step){prev=sums[lid-step];}workgroupBarrier();sums[lid]+=prev;workgroupBarrier();}
    if(selected!=0u){let off=bitcast<u32>(c[1u+wg.y*nw.x+wg.x])+sums[lid]-1u;dst[off]=b[3u*index]+b[3u*index+1u]+b[3u*index+2u];}
}
var<workgroup> air_sums:array<vec4<f32>,256>;
@compute @workgroup_size(256)
fn haze_air_sum(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) lid:u32,@builtin(workgroup_id) wg:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>){
    let i=lin_index(g,nw);var v=vec4<f32>(0.0);if(i<pu(0u)){let rgb=vec3<f32>(b[3u*i],b[3u*i+1u],b[3u*i+2u]);if(a[i]>=c[0] && rgb.x+rgb.y+rgb.z>=c[1]){v=vec4<f32>(rgb,1.0);}}
    air_sums[lid]=v;workgroupBarrier();
    for(var step=128u;step>0u;step/=2u){if(lid<step){air_sums[lid]+=air_sums[lid+step];}workgroupBarrier();}
    if(lid==0u){let j=4u*(wg.y*nw.x+wg.x);for(var ch=0u;ch<4u;ch++){dst[j+ch]=air_sums[0][ch];}}
}
@compute @workgroup_size(256)
fn haze_air_reduce(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) lid:u32,@builtin(workgroup_id) wg:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>){
    let i=lin_index(g,nw);var v=vec4<f32>(0.0);if(i<pu(0u)){v=vec4<f32>(a[4u*i],a[4u*i+1u],a[4u*i+2u],a[4u*i+3u]);}air_sums[lid]=v;workgroupBarrier();
    for(var step=128u;step>0u;step/=2u){if(lid<step){air_sums[lid]+=air_sums[lid+step];}workgroupBarrier();}
    if(lid==0u){let j=4u*(wg.y*nw.x+wg.x);for(var ch=0u;ch<4u;ch++){dst[j+ch]=air_sums[0][ch];}}
}
@compute @workgroup_size(1)
fn haze_air_finish(){for(var ch=0u;ch<3u;ch++){dst[ch]=a[ch]/max(a[3],1.0);}var distance=log(3.402823466e38)*0.5;if(b[0]>0.0){distance=-1.125*log(b[0]);}dst[3]=distance;}
