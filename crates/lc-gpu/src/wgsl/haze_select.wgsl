// The CPU's median-of-three partition is order-sensitive on repeated window minima.
// Median preparation/commit are constant work. Large partitions run on all SMs;
// this final scalar selector sees only the remaining interval after those passes.
fn hs(i:u32)->u32{return bitcast<u32>(a[i]);}
fn swap_values(i:u32,j:u32){let v=b[i];b[i]=b[j];b[j]=v;}
@compute @workgroup_size(1)
fn haze_select_init(){
    let n=select(pu(1u),bitcast<u32>(c[0]),pu(2u)!=0u);
    for(var j=0u;j<16u;j++){dst[j]=0.0;}dst[1]=bitcast<f32>(n);dst[2]=bitcast<f32>(min(u32(f32(n)*0.95),max(n,1u)-1u));dst[3]=bitcast<f32>(u32(n<=1u));
}
@compute @workgroup_size(1)
fn haze_select_prepare(){
    for(var j=0u;j<16u;j++){dst[j]=a[j];}if(hs(3u)!=0u){return;}let first=hs(0u);let pivot=hs(1u)-1u;let nth=hs(2u);
    if(b[first]>=b[pivot]){swap_values(first,pivot);}if(b[first]>=b[nth]){swap_values(first,nth);}if(b[pivot]>=b[nth]){swap_values(pivot,nth);}dst[4]=b[pivot];
}
@compute @workgroup_size(256)
fn haze_select_swap(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>){
    let i=lin_index(g,nw);if(hs(3u)!=0u || i>=hs(6u)){return;}swap_values(bitcast<u32>(c[i]),bitcast<u32>(c[pu(1u)+hs(8u)-1u-i]));
}
@compute @workgroup_size(1)
fn haze_select_commit(){
    for(var j=0u;j<16u;j++){dst[j]=a[j];}if(hs(3u)!=0u){return;}let cut=hs(5u);let nth=hs(2u);swap_values(hs(1u)-1u,cut);
    var first=hs(0u);var last=hs(1u);if(nth<cut){last=cut;}else if(nth>cut){first=cut+1u;}
    dst[0]=bitcast<f32>(first);dst[1]=bitcast<f32>(last);dst[3]=bitcast<f32>(u32(nth==cut || last<=first+1u));
}
@compute @workgroup_size(1)
fn haze_quick_select(){
    var count=select(pu(1u),bitcast<u32>(c[0]),pu(2u)!=0u);if(pu(3u)!=0u){count=hs(1u);}
    if(count==0u){dst[0]=0.0;return;}
    var nth=min(u32(f32(count)*0.95),count-1u);var first=0u;var last=count;
    if(pu(3u)!=0u){nth=hs(2u);first=hs(0u);last=hs(1u);if(hs(3u)!=0u){dst[0]=b[nth];return;}}
    while(last>first+1u){
        let pivot=last-1u;
        if(b[first]>=b[pivot]){let v=b[first];b[first]=b[pivot];b[pivot]=v;}
        if(b[first]>=b[nth]){let v=b[first];b[first]=b[nth];b[nth]=v;}
        if(b[pivot]>=b[nth]){let v=b[pivot];b[pivot]=b[nth];b[nth]=v;}
        let val=b[pivot];var aa=first;var bb=last;
        loop{
            aa++;while(aa<bb && b[aa]<val){aa++;}
            bb--;while(aa<bb && b[bb]>val){bb--;}
            if(aa>=bb){break;}let v=b[aa];b[aa]=b[bb];b[bb]=v;
        }
        let v=b[pivot];b[pivot]=b[aa];b[aa]=v;
        if(nth==aa){break;}if(nth<aa){last=aa;}else{first=aa+1u;}
    }
    dst[0]=b[nth];
}
