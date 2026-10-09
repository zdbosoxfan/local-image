// Box passes (clamped edges) over an image of NC interleaved channels; three of each approximate a
// Gaussian (`lightcraft_raster::blur::gaussian`). Each thread slides a running sum over CH pixels.
// P: w, h, nc, r, ch. Bindings: src, dst.

// Pixel `i` (all nc ≤ 3 channels at once: one contiguous read per pixel).
fn ld(i: u32, nc: u32) -> vec3<f32> {
    let j = i * nc;
    if (nc == 1u) {
        return vec3<f32>(src[j], 0.0, 0.0);
    }
    if (nc == 2u) {
        return vec3<f32>(src[j], src[j + 1u], 0.0);
    }
    return vec3<f32>(src[j], src[j + 1u], src[j + 2u]);
}

fn st(i: u32, nc: u32, v: vec3<f32>) {
    let j = i * nc;
    dst[j] = v.x;
    if (nc > 1u) {
        dst[j + 1u] = v.y;
    }
    if (nc > 2u) {
        dst[j + 2u] = v.z;
    }
}

// Horizontal: one thread per CH consecutive pixels of a row.
@compute @workgroup_size(64, 4)
fn box_h(@builtin(global_invocation_id) g: vec3<u32>) {
    let w = pu(0u);
    let h = pu(1u);
    let nc = pu(2u);
    let r = i32(pu(3u));
    let ch = pu(4u);
    let y = g.y;
    let x0 = g.x * ch;
    if (y >= h || x0 >= w) {
        return;
    }
    let last = i32(w) - 1;
    let inv = 1.0 / f32(2 * r + 1);
    let row = y * w;
    let x1 = min(x0 + ch, w);
    var acc = vec3<f32>(0.0);
    for (var k = -r; k <= r; k++) {
        acc += ld(row + u32(clamp(i32(x0) + k, 0, last)), nc);
    }
    for (var x = x0; x < x1; x++) {
        st(row + x, nc, acc * inv);
        acc = acc + ld(row + u32(min(i32(x) + r + 1, last)), nc) - ld(row + u32(max(i32(x) - r, 0)), nc);
    }
}

// Vertical: one thread per CH consecutive rows of a column (the CPU re-primes every BAND rows the
// same way).
@compute @workgroup_size(64, 4)
fn box_v(@builtin(global_invocation_id) g: vec3<u32>) {
    let w = pu(0u);
    let h = pu(1u);
    let nc = pu(2u);
    let r = i32(pu(3u));
    let ch = pu(4u);
    let x = g.x;
    let y0 = g.y * ch;
    if (x >= w || y0 >= h) {
        return;
    }
    let last = i32(h) - 1;
    let inv = 1.0 / f32(2 * r + 1);
    let y1 = min(y0 + ch, h);
    var acc = vec3<f32>(0.0);
    for (var k = -r; k <= r; k++) {
        acc += ld(u32(clamp(i32(y0) + k, 0, last)) * w + x, nc);
    }
    for (var y = y0; y < y1; y++) {
        st(y * w + x, nc, acc * inv);
        acc = acc + ld(u32(min(i32(y) + r + 1, last)) * w + x, nc) - ld(u32(max(i32(y) - r, 0)) * w + x, nc);
    }
}

// Pixel-scale Gaussian, host supplies one-sided taps after w,h,nc,r.
@compute @workgroup_size(16,16)
fn conv_h(@builtin(global_invocation_id) g:vec3<u32>) {
    let w=pu(0u); let h=pu(1u); let nc=pu(2u); let r=pu(3u);
    if(g.x>=w || g.y>=h) {return;}
    for(var c=0u;c<nc;c++) {
        var v=src[(g.y*w+g.x)*nc+c]*pf(4u);
        for(var k=1u;k<=r;k++) {
            let lo=u32(max(i32(g.x)-i32(k),0)); let hi=min(g.x+k,w-1u);
            v=v+src[(g.y*w+lo)*nc+c]*pf(4u+k);
            v=v+src[(g.y*w+hi)*nc+c]*pf(4u+k);
        }
        dst[(g.y*w+g.x)*nc+c]=v;
    }
}
@compute @workgroup_size(16,16)
fn conv_v(@builtin(global_invocation_id) g:vec3<u32>) {
    let w=pu(0u); let h=pu(1u); let nc=pu(2u); let r=pu(3u);
    if(g.x>=w || g.y>=h) {return;}
    for(var c=0u;c<nc;c++) {
        var v=src[(g.y*w+g.x)*nc+c]*pf(4u);
        for(var k=1u;k<=r;k++) {
            let lo=u32(max(i32(g.y)-i32(k),0)); let hi=min(g.y+k,h-1u);
            v=v+src[(lo*w+g.x)*nc+c]*pf(4u+k);
            v=v+src[(hi*w+g.x)*nc+c]*pf(4u+k);
        }
        dst[(g.y*w+g.x)*nc+c]=v;
    }
}

// Cropped-window extrema, w,h,r,max(0=min,1=max),vertical.
@compute @workgroup_size(16,16)
fn extrema(@builtin(global_invocation_id) g:vec3<u32>) {
    let w=pu(0u); let h=pu(1u); let r=i32(pu(2u));
    if(g.x>=w || g.y>=h) {return;}
    var v=src[g.y*w+g.x];
    for(var k=-r;k<=r;k++) {
        var x=i32(g.x);var y=i32(g.y);
        if(pu(4u)==0u) {x=clamp(x+k,0,i32(w)-1);} else {y=clamp(y+k,0,i32(h)-1);}
        let q=src[u32(y)*w+u32(x)];
        if(pu(3u)==0u) {v=min(v,q);} else {v=max(v,q);}
    }
    dst[g.y*w+g.x]=v;
}

// darktable box mean: cropped window, Kahan sum, arbitrary interleaved channel count.
// One thread owns an entire row/column, preserving CPU's summation order.
// P: w,h,nc,r,vertical. Host uses groups1(lines*nc).
@compute @workgroup_size(256)
fn mean_box(@builtin(global_invocation_id) g:vec3<u32>,@builtin(num_workgroups) nw:vec3<u32>) {
    let index=lin_index(g,nw);let w=pu(0u);let h=pu(1u);let nc=pu(2u);let r=pu(3u);
    let vertical=pu(4u)!=0u;
    let lines=select(h,w,vertical);let len=select(w,h,vertical);
    if(index>=lines*nc) {return;}
    let line=index/nc;let c=index%nc;var sum=0.0;var err=0.0;
    for(var k=0u;k<=min(r,len-1u);k++) {
        let pos=select(line*w+k,k*w+line,vertical)*nc+c;
        let y=src[pos]-err;let t=sum+y;err=(t-sum)-y;sum=t;
    }
    for(var k=0u;k<len;k++) {
        let lo=u32(max(i32(k)-i32(r),0));let hi=min(k+r,len-1u);
        let pos=select(line*w+k,k*w+line,vertical)*nc+c;
        dst[pos]=sum/f32(hi-lo+1u);
        if(k>=r) {
            let sub=select(line*w+k-r,(k-r)*w+line,vertical)*nc+c;
            let y=-src[sub]-err;let t=sum+y;err=(t-sum)-y;sum=t;
        }
        if(k+r+1u<len) {
            let add=select(line*w+k+r+1u,(k+r+1u)*w+line,vertical)*nc+c;
            let y=src[add]-err;let t=sum+y;err=(t-sum)-y;sum=t;
        }
    }
}
