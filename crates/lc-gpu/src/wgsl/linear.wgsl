// Scene calibration and premultiplied AI patches. Scalar twins of colorcal.rs / patches.rs.
fn lrgb(i:u32)->vec3<f32>{return vec3<f32>(a[3u*i],a[3u*i+1u],a[3u*i+2u]);}
fn lput(i:u32,v:vec3<f32>){dst[3u*i]=v.x;dst[3u*i+1u]=v.y;dst[3u*i+2u]=v.z;}
fn lmatrix(o:u32,v:vec3<f32>)->vec3<f32>{return vec3<f32>(pf(o)*v.x+pf(o+1u)*v.y+pf(o+2u)*v.z,pf(o+3u)*v.x+pf(o+4u)*v.y+pf(o+5u)*v.z,pf(o+6u)*v.x+pf(o+7u)*v.y+pf(o+8u)*v.z);}
fn toward(val:f32,delta:f32,white:f32,corr:f32)->f32{let t=corr*delta+val;if(val>white){return max(t,white);}return min(t,white);}
@compute @workgroup_size(256)
fn colour_cal(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>){
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}var rgb=lrgb(i);let clip=pu(1u)!=0u;
    if(clip){rgb=max(rgb,vec3<f32>(0.0));}
    var xyz=lmatrix(4u,rgb);
    if(pu(3u)==0u){xyz=lmatrix(13u,xyz);}else{
        let yy=xyz.y;
        if(abs(yy)>=1e-9){let lms=lmatrix(31u,xyz/yy);var t=lms/vec3<f32>(pf(49u),pf(50u),pf(51u));
            if(t.z>0.0){t.z=pow(t.z,pf(52u));}xyz=lmatrix(40u,t*vec3<f32>(pf(53u),pf(54u),pf(55u)))*yy;}
    }
    if(pf(2u)>0.0){
        let sum=xyz.x+xyz.y+xyz.z;let yy=xyz.y;var xy=vec2<f32>(0.31271,0.32902);if(sum>0.0){xy=xyz.xy/sum;}
        let denom=-2.0*xy.x+12.0*xy.y+3.0;var uv=vec2<f32>(4.0*xy.x,9.0*xy.y)/denom;
        let white=vec2<f32>(0.19783,0.46832);let delta=white-uv;let big=yy*(delta.x*delta.x+delta.y*delta.y);var corr=0.0;if(big>0.0){corr=pow(big,pf(2u));}
        uv=vec2<f32>(toward(uv.x,delta.x,white.x,corr),toward(uv.y,delta.y,white.y,corr));
        let dd=6.0*uv.x-16.0*uv.y+12.0;xy=vec2<f32>(9.0*uv.x,4.0*uv.y)/dd;
        if(clip){xy=max(xy,vec2<f32>(0.0));}xy.y=max(xy.y,0.000015258789);
        let s=xy.x+xy.y;if(s>=1.0){xy/=s;}
        xyz=vec3<f32>(xy.x*yy/xy.y,yy,(1.0-xy.x-xy.y)*yy/xy.y);
    }
    rgb=lmatrix(22u,xyz);if(clip){rgb=max(rgb,vec3<f32>(0.0));}lput(i,rgb);
}
fn patch_at(x:i32,y:i32,w:u32,h:u32)->vec4<f32>{let j=4u*(u32(clamp(y,0,i32(h)-1))*w+u32(clamp(x,0,i32(w)-1)));return vec4<f32>(b[j],b[j+1u],b[j+2u],b[j+3u]);}
// P:n,output w,patch w,h,opacity, output->patch affine6.
@compute @workgroup_size(256)
fn patch_apply(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>){
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}let w=pu(2u);let h=pu(3u);let px=f32(i%pu(1u))+0.5;let py=f32(i/pu(1u))+0.5;
    let x=pf(5u)*px+pf(7u)*py+pf(9u);let y=pf(6u)*px+pf(8u)*py+pf(10u);var rgb=lrgb(i);
    if(x>=0.0 && y>=0.0 && x<=f32(w) && y<=f32(h)){
        let fx=x-0.5;let fy=y-0.5;let x0=i32(floor(fx));let y0=i32(floor(fy));let tx=fx-floor(fx);let ty=fy-floor(fy);
        let taps=array<vec4<f32>,4>(patch_at(x0,y0,w,h),patch_at(x0+1,y0,w,h),patch_at(x0,y0+1,w,h),patch_at(x0+1,y0+1,w,h));
        let weights=array<f32,4>((1.0-tx)*(1.0-ty),tx*(1.0-ty),(1.0-tx)*ty,tx*ty);var sum=vec4<f32>(0.0);
        for(var j=0u;j<4u;j++){let p=taps[j];sum+=vec4<f32>(p.rgb*p.a*weights[j],p.a*weights[j]);}
        if(sum.a>1e-6){let alpha=sum.a*pf(4u);if(alpha>0.0){rgb=max(rgb+(sum.rgb/sum.a-rgb)*alpha,vec3<f32>(0.0));}}
    }lput(i,rgb);
}
// Premultiplied integer minification before patch placement. b: patch data.
@compute @workgroup_size(256)
fn patch_shrink(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>){
    let i=lin_index(g,nw);if(i>=pu(0u)){return;}let w=pu(1u);let h=pu(2u);let f=pu(3u);let ow=pu(4u);let x0=(i%ow)*f;let y0=(i/ow)*f;
    var sum=vec4<f32>(0.0);var count=0.0;
    for(var y=y0;y<min(y0+f,h);y++){for(var x=x0;x<min(x0+f,w);x++){let p=patch_at(i32(x),i32(y),w,h);sum+=vec4<f32>(p.rgb*p.a,p.a);count+=1.0;}}
    var p=vec4<f32>(0.0);if(sum.a>1e-6){p=vec4<f32>(sum.rgb/sum.a,sum.a/max(count,1.0));}
    let j=i*4u;dst[j]=p.x;dst[j+1u]=p.y;dst[j+2u]=p.z;dst[j+3u]=p.w;
}
