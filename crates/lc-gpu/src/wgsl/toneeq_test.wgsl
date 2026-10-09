// Compiled only in unit-test devices, including production-device-first tests.
@compute @workgroup_size(256)
fn teq_round_probe(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) ng:vec3<u32>) {
    let i=lin_index(g,ng);if(i>=pu(0u)){return;}
    let x=a[3u*i];let y=a[3u*i+1u];let z=a[3u*i+2u];
    dst[5u*i]=teq_add(x,y);
    dst[5u*i+1u]=teq_add(teq_mul(x,y),z);
    dst[5u*i+2u]=teq_mul(x,y);
    dst[5u*i+3u]=0.0;if((bitcast<u32>(y)&0x7fffffffu)!=0u){dst[5u*i+3u]=teq_div(x,y);}
    dst[5u*i+4u]=teq_sqrt(bitcast<f32>(bitcast<u32>(x)&0x7fffffffu));
}
