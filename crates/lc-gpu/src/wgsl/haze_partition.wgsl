// Parallel, stable lists of the CPU Hoare partition's left/right stop positions.
// Pair k swaps L[k] with R[countR-1-k] only while L < R. This preserves the
// reference's unusual inclusion of the pivot in the first right-hand scan.
var<workgroup> select_counts:array<vec2<u32>,256>;
fn stops(i:u32)->vec2<u32>{
    var v=vec2<u32>(0u);
    if(bitcast<u32>(b[3])==0u && i>bitcast<u32>(b[0]) && i<bitcast<u32>(b[1])){
        let q=a[i];let pivot=b[4];v=vec2<u32>(u32(!(q<pivot)),u32(!(q>pivot)));
    }return v;
}
@compute @workgroup_size(256)
fn haze_partition_count(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) lid:u32,@builtin(workgroup_id) wg:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>){
    let i=lin_index(g,nw);var v=vec2<u32>(0u);if(i<pu(0u)){v=stops(i);}select_counts[lid]=v;workgroupBarrier();
    for(var step=128u;step>0u;step/=2u){if(lid<step){select_counts[lid]+=select_counts[lid+step];}workgroupBarrier();}
    if(lid==0u){let j=2u*(wg.y*nw.x+wg.x);dst[j]=bitcast<f32>(select_counts[0].x);dst[j+1u]=bitcast<f32>(select_counts[0].y);}
}
@compute @workgroup_size(256)
fn haze_scan_blocks(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) lid:u32,@builtin(workgroup_id) wg:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>){
    let i=lin_index(g,nw);var v=vec2<u32>(0u);if(i<pu(0u)){v=vec2<u32>(bitcast<u32>(a[2u*i]),bitcast<u32>(a[2u*i+1u]));}
    select_counts[lid]=v;workgroupBarrier();
    for(var step=1u;step<256u;step*=2u){var prev=vec2<u32>(0u);if(lid>=step){prev=select_counts[lid-step];}workgroupBarrier();select_counts[lid]+=prev;workgroupBarrier();}
    if(i<pu(0u)){let p=select_counts[lid]-v;dst[2u*i]=bitcast<f32>(p.x);dst[2u*i+1u]=bitcast<f32>(p.y);}
    if(lid==255u){let j=2u*(pu(0u)+wg.y*nw.x+wg.x);dst[j]=bitcast<f32>(select_counts[lid].x);dst[j+1u]=bitcast<f32>(select_counts[lid].y);}
}
@compute @workgroup_size(256)
fn haze_scan_add(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>){
    let i=lin_index(g,nw);if(i>pu(1u)){return;}let group=select(i/256u,pu(2u),i==pu(1u));
    var v=vec2<u32>(0u);if(i<pu(1u)){v=vec2<u32>(bitcast<u32>(a[2u*i]),bitcast<u32>(a[2u*i+1u]));}
    v+=vec2<u32>(bitcast<u32>(b[2u*group]),bitcast<u32>(b[2u*group+1u]));dst[2u*i]=bitcast<f32>(v.x);dst[2u*i+1u]=bitcast<f32>(v.y);
}
@compute @workgroup_size(256)
fn haze_partition_fill(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) lid:u32,@builtin(workgroup_id) wg:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>){
    let i=lin_index(g,nw);var v=vec2<u32>(0u);if(i<pu(0u)){v=stops(i);}select_counts[lid]=v;workgroupBarrier();
    for(var step=1u;step<256u;step*=2u){var prev=vec2<u32>(0u);if(lid>=step){prev=select_counts[lid-step];}workgroupBarrier();select_counts[lid]+=prev;workgroupBarrier();}
    let group=2u*(wg.y*nw.x+wg.x);let offsets=vec2<u32>(bitcast<u32>(c[group]),bitcast<u32>(c[group+1u]))+select_counts[lid]-vec2<u32>(1u);
    if(v.x!=0u){dst[offsets.x]=bitcast<f32>(i);}if(v.y!=0u){dst[pu(0u)+offsets.y]=bitcast<f32>(i);}
}
@compute @workgroup_size(1)
fn haze_partition_split(){
    for(var j=0u;j<16u;j++){dst[j]=b[j];}if(bitcast<u32>(b[3])!=0u){return;}
    let nl=bitcast<u32>(c[2u*pu(1u)]);let nr=bitcast<u32>(c[2u*pu(1u)+1u]);var lo=0u;var hi=min(nl,nr);
    while(lo<hi){let mid=(lo+hi)/2u;if(bitcast<u32>(a[mid])<bitcast<u32>(a[pu(2u)+nr-1u-mid])){lo=mid+1u;}else{hi=mid;}}
    var left=bitcast<u32>(b[1]);if(lo<nl){left=bitcast<u32>(a[lo]);}
    var right=bitcast<u32>(b[1]);if(lo>0u){right=bitcast<u32>(a[pu(2u)+nr-lo]);}
    dst[5]=bitcast<f32>(min(left,right));dst[6]=bitcast<f32>(lo);dst[7]=bitcast<f32>(nl);dst[8]=bitcast<f32>(nr);
}
