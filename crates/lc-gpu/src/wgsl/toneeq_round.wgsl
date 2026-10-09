// Tone-equalizer guidance has discontinuities at rounded log2 bin boundaries.
// WGSL permits floating-point reassociation and contraction even across let
// bindings. Integer rounding makes the CPU's separate operations observable.
// Three low bits hold guard, round and sticky; ties round to even. Normal and
// subnormal finite operands are supported, including cancellation to zero.
// Division requires a nonzero denominator; sqrt requires a nonnegative input.
fn teq_shift_jam(v:u32,shift:u32)->u32 {
    if(shift==0u){return v;}
    if(shift>=32u){return select(0u,1u,v!=0u);}
    return (v>>shift)|select(0u,1u,(v<<(32u-shift))!=0u);
}
fn teq_add(x:f32,y:f32)->f32 {
    var a=bitcast<u32>(x);var b=bitcast<u32>(y);
    if((a&0x7fffffffu)<(b&0x7fffffffu)){let t=a;a=b;b=t;}
    // Preserve overflow from an earlier operation when adding a finite value.
    if((a&0x7fffffffu)>=0x7f800000u){return bitcast<f32>(a);}
    let sign=a&0x80000000u;
    var exp=max((a>>23u)&255u,1u);let bexp=max((b>>23u)&255u,1u);
    let am=(a&0x7fffffu)|select(0u,0x800000u,(a&0x7f800000u)!=0u);
    let bm=(b&0x7fffffu)|select(0u,0x800000u,(b&0x7f800000u)!=0u);
    let small=teq_shift_jam(bm<<3u,exp-bexp);
    var m=am<<3u;
    if(((a^b)&0x80000000u)==0u){m+=small;}else{m-=small;}
    if(m==0u){return bitcast<f32>(select(0u,sign,a==b));}
    if(m>=0x8000000u){m=teq_shift_jam(m,1u);exp++;}
    let shift=min(countLeadingZeros(m)-5u,exp-1u);
    m=m<<shift;exp-=shift;
    let tail=m&7u;
    var rounded=(m>>3u)+select(0u,1u,tail>4u||(tail==4u&&(m&8u)!=0u));
    if(rounded>=0x1000000u){rounded=rounded>>1u;exp++;}
    if(exp>=255u){return bitcast<f32>(sign|0x7f800000u);}
    if(rounded<0x800000u){exp=0u;}
    return bitcast<f32>(sign|(exp<<23u)|(rounded&0x7fffffu));
}

// A normalized finite nonzero operand: 24-bit significand, unbiased exponent.
fn teq_parts(bits:u32)->vec2<i32> {
    let e=i32((bits>>23u)&255u);let m=bits&0x7fffffu;
    if(e!=0){return vec2<i32>(i32(m|0x800000u),e-127);}
    let shift=countLeadingZeros(m)-8u;
    return vec2<i32>(i32(m<<shift),-126-i32(shift));
}
// Exact product of two significands (at most 25 bits each), low word first.
fn teq_product(x:u32,y:u32)->vec2<u32> {
    let low=(x&65535u)*(y&65535u);
    let mid=(x>>16u)*(y&65535u)+(y>>16u)*(x&65535u)+(low>>16u);
    return vec2<u32>((low&65535u)|(mid<<16u),(x>>16u)*(y>>16u)+(mid>>16u));
}
fn teq_less(x:vec2<u32>,y:vec2<u32>)->bool {
    return x.y<y.y||(x.y==y.y&&x.x<y.x);
}
fn teq_wide_sub(x:vec2<u32>,y:vec2<u32>)->vec2<u32> {
    return vec2<u32>(x.x-y.x,x.y-y.y-select(0u,1u,x.x<y.x));
}
fn teq_wide_shift(x:u32,shift:u32)->vec2<u32> {
    return vec2<u32>(x<<shift,x>>(32u-shift));
}
fn teq_pack(sign:u32,exponent:i32,mantissa:u32,tail_nonzero:bool)->f32 {
    var e=exponent;var q=mantissa;
    if(e< -150){return bitcast<f32>(sign);}
    if(e< -126){
        let shift=u32(-126-e);let tail=q&((1u<<shift)-1u);let half=1u<<(shift-1u);
        q=(q>>shift)+select(0u,1u,tail>half||(tail==half&&(tail_nonzero||((q>>shift)&1u)!=0u)));
        return bitcast<f32>(sign|q);
    }
    if(q>=0x1000000u){q=q>>1u;e++;}
    if(e>127){return bitcast<f32>(sign|0x7f800000u);}
    return bitcast<f32>(sign|(u32(e+127)<<23u)|(q&0x7fffffu));
}
fn teq_mul(x:f32,y:f32)->f32 {
    let a=bitcast<u32>(x);let b=bitcast<u32>(y);let sign=(a^b)&0x80000000u;
    if((a&0x7fffffffu)==0u||(b&0x7fffffffu)==0u){return bitcast<f32>(sign);}
    let ap=teq_parts(a);let bp=teq_parts(b);let product=teq_product(u32(ap.x),u32(bp.x));
    let shift=select(23u,24u,(product.y&0x8000u)!=0u);let e=ap.y+bp.y+i32(shift)-23;
    var q=(product.x>>shift)|(product.y<<(32u-shift));
    let tail=product.x&((1u<<shift)-1u);let half=1u<<(shift-1u);
    if(e>= -126){q+=select(0u,1u,tail>half||(tail==half&&(q&1u)!=0u));}
    return teq_pack(sign,e,q,tail!=0u);
}
fn teq_div(x:f32,y:f32)->f32 {
    let a=bitcast<u32>(x);let b=bitcast<u32>(y);let sign=(a^b)&0x80000000u;
    if((a&0x7fffffffu)==0u){return bitcast<f32>(sign);}
    let ap=teq_parts(a);let bp=teq_parts(b);let mx=u32(ap.x);let my=u32(bp.x);
    let shift=select(23u,24u,mx<my);let numerator=teq_wide_shift(mx,shift);
    let e=ap.y-bp.y+23-i32(shift);
    // Native division is only a seed. Exact integer residual comparisons
    // correct every permitted approximation before the result is rounded.
    var q=u32(f32(mx)/f32(my)*f32(1u<<shift));
    var product=teq_product(q,my);
    while(teq_less(numerator,product)){q--;product=teq_product(q,my);}
    var remainder=teq_wide_sub(numerator,product).x;
    while(remainder>=my){q++;remainder-=my;}
    if(e>= -126){q+=select(0u,1u,remainder*2u>my||(remainder*2u==my&&(q&1u)!=0u));}
    return teq_pack(sign,e,q,remainder!=0u);
}
fn teq_sqrt(x:f32)->f32 {
    let bits=bitcast<u32>(x);
    if((bits&0x7fffffffu)==0u){return x;}
    let p=teq_parts(bits);let odd=u32(p.y)&1u;
    let radicand=teq_wide_shift(u32(p.x),23u+odd);
    var q=u32(sqrt(f32(p.x)*f32(1u<<(23u+odd))));
    while(teq_less(radicand,teq_product(q,q))){q--;}
    while(!teq_less(radicand,teq_product(q+1u,q+1u))){q++;}
    // Compare the exact radicand with the square of the rounding midpoint.
    let four=vec2<u32>(radicand.x<<2u,(radicand.y<<2u)|(radicand.x>>30u));
    let midpoint=teq_product(2u*q+1u,2u*q+1u);
    q+=select(0u,1u,teq_less(midpoint,four)||(all(midpoint==four)&&(q&1u)!=0u));
    return teq_pack(0u,(p.y-i32(odd))/2,q,false);
}
