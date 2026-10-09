// The CPU's median-of-three partition is order-sensitive on repeated window minima.
// Operate on device scratch in the identical order; ordinary edits never dispatch this tool.
@compute @workgroup_size(1)
fn haze_quick_select(){
    let count=select(pu(1u),bitcast<u32>(c[0]),pu(2u)!=0u);
    if(count==0u){dst[0]=0.0;return;}
    let nth=min(u32(f32(count)*0.95),count-1u);var first=0u;var last=count;
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
