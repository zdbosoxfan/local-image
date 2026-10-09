//! AMaZE (Aliasing Minimization and Zipper Elimination), complete scalar kernel.
//! Port of darktable `src/iop/demosaicing/amaze.cc` at
//! 733bd69f32cac7ff5e41025115942772add1f088, cross-checked with RawTherapee
//! 5f486d3678b34c74ba0c63571c17babe20935019. Copyright (c) 2008-2010 Emil
//! Martinec; speed optimizations by Ingo Weyrich; (C) 2011-2024 darktable
//! developers. GPL-3.0-or-later. See docs/PORTS.md and the project notices.
//! Separate owned planes replace upstream's lifetime-overlapping scratch buffer.
#![allow(unused_parens, unused_mut, non_snake_case)]
use super::Mosaic;
use crate::Rgb32f;
#[derive(Clone, Copy, Default)]
struct Hv {h:f32,v:f32}
#[inline] fn fc(m:&Mosaic,y:i32,x:i32)->i32 {m.cfa.color_at((x&1) as usize,(y&1) as usize) as i32}
#[inline] fn interpolatef(a:f32,b:f32,c:f32)->f32 {a*(b-c)+c}
#[inline] fn square(v:f32)->f32 {v*v}
#[inline] fn lim(x:f32,a:f32,b:f32)->f32 {a.max(x.min(b))}
#[inline] fn ulim(x:f32,a:f32,b:f32)->f32 {lim(x,a.min(b),a.max(b))}
#[inline] fn xmul2f(x:f32)->f32 {if x.to_bits()&0x7fff_ffff==0 {x} else {f32::from_bits(x.to_bits().wrapping_add(1<<23))}}
#[inline] fn xdiv2f(x:f32)->f32 {xdiv(x,1)}
#[inline] fn xdiv(x:f32,n:i32)->f32 {if x.to_bits()&0x7fff_ffff==0 {x} else {f32::from_bits(x.to_bits().wrapping_sub((n as u32)<<23))}}
#[inline] fn clampnan(x:f32,a:f32,b:f32)->f32 {if x.is_nan() {(a+b)*0.5} else if x.is_infinite() {lim(x,a,b)} else {x}}
pub(crate) fn amaze(m:&Mosaic)->Rgb32f {
    // Upstream's 16-pixel mirror padding requires a 33-pixel minimum extent.
    // Extend tiny crops with parity-preserving reflection, then crop the result.
    if m.w < 33 || m.h < 33 {
        let (w,h)=(m.w.max(34),m.h.max(34));
        let data:Vec<f32>=(0..w*h).map(|i|m.get((i%w) as isize,(i/w) as isize)).collect();
        let padded=amaze(&Mosaic {w,h,data:&data,cfa:m.cfa});
        return Rgb32f::from_fn(m.w,m.h,|x,y|padded.get(x,y));
    }
    let width=m.w as i32;
    let height=m.h as i32;
    let input=m.data;
    let clip_pt=1.0f32;
    let mut out=vec![0.0f32;m.w*m.h*4];
let mut rgbgreen=vec![0.0f32;176*160+16];
let mut delhvsqsum=vec![0.0f32;176*160+16];
let mut dirwts0=vec![0.0f32;176*160+16];
let mut dirwts1=vec![0.0f32;176*160+16];
let mut vcd=vec![0.0f32;176*160+16];
let mut hcd=vec![0.0f32;176*160+16];
let mut vcdalt=vec![0.0f32;176*160+16];
let mut hcdalt=vec![0.0f32;176*160+16];
let mut cddiffsq=vec![0.0f32;176*160+16];
let mut hvwt=vec![0.0f32;176*160+16];
let mut delp=vec![0.0f32;176*160+16];
let mut delm=vec![0.0f32;176*160+16];
let mut rbint=vec![0.0f32;176*160+16];
let mut dgintv=vec![0.0f32;176*160+16];
let mut dginth=vec![0.0f32;176*160+16];
let mut Dgrbsq1m=vec![0.0f32;176*160+16];
let mut Dgrbsq1p=vec![0.0f32;176*160+16];
let mut cfa=vec![0.0f32;176*160+16];
let mut pmwt=vec![0.0f32;176*160+16];
let mut rbm=vec![0.0f32;176*160+16];
let mut rbp=vec![0.0f32;176*160+16];
let mut nyqutest=vec![0.0f32;176*160+16];
let mut Dgrb=vec![vec![0.0f32;160*80];2];
let mut Dgrb2=vec![Hv::default();160*80];
let mut nyquist=vec![0i32;160*80];
let mut nyquist2=vec![0i32;160*80];
let clip_pt8: f32 = (0.8f32 * clip_pt);
let ts: i32 = 160;
let _tsh: i32 = (ts / 2);
let mut ex: i32;
let mut ey: i32;
if (fc(m, 0, 0) == 1) {
if (fc(m, 0, 1) == 0) {
ey = 0;
ex = 1;
}
else {
ey = 1;
ex = 0;
}
}
else {
if (fc(m, 0, 0) == 0) {
ey = 0;
ex = 0;
}
else {
ey = 1;
ex = 1;
}
}
let v1: i32 = ts;
let v2: i32 = (2 * ts);
let v3: i32 = (3 * ts);
let p1: i32 = (-(ts) + 1);
let p2: i32 = ((-(2) * ts) + 2);
let p3: i32 = ((-(3) * ts) + 3);
let m1: i32 = (ts + 1);
let m2: i32 = ((2 * ts) + 2);
let m3: i32 = ((3 * ts) + 3);
let eps: f32 = 1e-5f32;
let epssq: f32 = 1e-10f32;
let arthresh: f32 = 0.75f32;
let gaussodd: [f32; 4] = [0.14659727707323927f32, 0.103592713382435f32, 0.0732036125103057f32, 0.0365543548389495f32];
let nyqthresh: f32 = 0.5f32;
let gaussgrad: [f32; 6] = [(nyqthresh * 0.07384411893421103f32), (nyqthresh * 0.06207511968171489f32), (nyqthresh * 0.0521818194747806f32), (nyqthresh * 0.03687419286733595f32), (nyqthresh * 0.03099732204057846f32), (nyqthresh * 0.018413194161458882f32)];
let gausseven: [f32; 2] = [0.13719494435797422f32, 0.05640252782101291f32];
let gquinc: [f32; 4] = [0.169917f32, 0.108947f32, 0.069855f32, 0.0287182f32];
{
let mut top: i32 = -(16);
while (top < height) {
{
{
let mut left: i32 = -(16);
while (left < width) {
{
nyquist.fill(0);
let bottom: i32 = ((top + ts)).min((height + 16));
let right: i32 = ((left + ts)).min((width + 16));
let rr1: i32 = (bottom - top);
let cc1: i32 = (right - left);
let rrmin: i32 = (if (top < 0) { 16 } else { 0 });
let ccmin: i32 = (if (left < 0) { 16 } else { 0 });
let rrmax: i32 = (if (bottom > height) { (height - top) } else { rr1 });
let ccmax: i32 = (if (right > width) { (width - left) } else { cc1 });
if (rrmin > 0) {
{
let mut rr: i32 = 0;
while (rr < 16) {
{
let mut cc: i32 = ccmin;
let mut row: i32 = ((32 - rr) + top);
while (cc < ccmax) {
{
cfa[(((rr * ts) + cc)) as usize] = input[(((row * width) + (cc + left))) as usize];
rgbgreen[(((rr * ts) + cc)) as usize] = cfa[(((rr * ts) + cc)) as usize];
}
cc += 1;
}
}
rr += 1;
}
}
}
{
let mut rr: i32 = rrmin;
while (rr < rrmax) {
{
let row: i32 = (rr + top);
{
let mut cc: i32 = ccmin;
while (cc < ccmax) {
{
let indx1: i32 = ((rr * ts) + cc);
cfa[(indx1) as usize] = input[(((row * width) + (cc + left))) as usize];
rgbgreen[(indx1) as usize] = cfa[(indx1) as usize];
}
cc += 1;
}
}
}
rr += 1;
}
}
if (rrmax < rr1) {
{
let mut rr: i32 = 0;
while (rr < 16) {
{
let mut cc: i32 = ccmin;
while (cc < ccmax) {
{
cfa[((((rrmax + rr) * ts) + cc)) as usize] = input[(((((height - rr) - 2) * width) + (left + cc))) as usize];
rgbgreen[((((rrmax + rr) * ts) + cc)) as usize] = cfa[((((rrmax + rr) * ts) + cc)) as usize];
}
cc += 1;
}
}
rr += 1;
}
}
}
if (ccmin > 0) {
{
let mut rr: i32 = rrmin;
while (rr < rrmax) {
{
let mut cc: i32 = 0;
let mut row: i32 = (rr + top);
while (cc < 16) {
{
cfa[(((rr * ts) + cc)) as usize] = input[(((row * width) + ((32 - cc) + left))) as usize];
rgbgreen[(((rr * ts) + cc)) as usize] = cfa[(((rr * ts) + cc)) as usize];
}
cc += 1;
}
}
rr += 1;
}
}
}
if (ccmax < cc1) {
{
let mut rr: i32 = rrmin;
while (rr < rrmax) {
{
let mut cc: i32 = 0;
while (cc < 16) {
{
cfa[((((rr * ts) + ccmax) + cc)) as usize] = input[((((top + rr) * width) + ((width - cc) - 2))) as usize];
rgbgreen[((((rr * ts) + ccmax) + cc)) as usize] = cfa[((((rr * ts) + ccmax) + cc)) as usize];
}
cc += 1;
}
}
rr += 1;
}
}
}
if ((rrmin > 0) && (ccmin > 0)) {
{
let mut rr: i32 = 0;
while (rr < 16) {
{
let mut cc: i32 = 0;
while (cc < 16) {
{
cfa[(((rr * ts) + cc)) as usize] = input[((((32 - rr) * width) + (32 - cc))) as usize];
rgbgreen[(((rr * ts) + cc)) as usize] = cfa[(((rr * ts) + cc)) as usize];
}
cc += 1;
}
}
rr += 1;
}
}
}
if ((rrmax < rr1) && (ccmax < cc1)) {
{
let mut rr: i32 = 0;
while (rr < 16) {
{
let mut cc: i32 = 0;
while (cc < 16) {
{
cfa[(((((rrmax + rr) * ts) + ccmax) + cc)) as usize] = input[(((((height - rr) - 2) * width) + ((width - cc) - 2))) as usize];
rgbgreen[(((((rrmax + rr) * ts) + ccmax) + cc)) as usize] = cfa[(((((rrmax + rr) * ts) + ccmax) + cc)) as usize];
}
cc += 1;
}
}
rr += 1;
}
}
}
if ((rrmin > 0) && (ccmax < cc1)) {
{
let mut rr: i32 = 0;
while (rr < 16) {
{
let mut cc: i32 = 0;
while (cc < 16) {
{
cfa[((((rr * ts) + ccmax) + cc)) as usize] = input[((((32 - rr) * width) + ((width - cc) - 2))) as usize];
rgbgreen[((((rr * ts) + ccmax) + cc)) as usize] = cfa[((((rr * ts) + ccmax) + cc)) as usize];
}
cc += 1;
}
}
rr += 1;
}
}
}
if ((rrmax < rr1) && (ccmin > 0)) {
{
let mut rr: i32 = 0;
while (rr < 16) {
{
let mut cc: i32 = 0;
while (cc < 16) {
{
cfa[((((rrmax + rr) * ts) + cc)) as usize] = input[(((((height - rr) - 2) * width) + (32 - cc))) as usize];
rgbgreen[((((rrmax + rr) * ts) + cc)) as usize] = cfa[((((rrmax + rr) * ts) + cc)) as usize];
}
cc += 1;
}
}
rr += 1;
}
}
}
{
let mut rr: i32 = 2;
while (rr < (rr1 - 2)) {
{
let mut cc: i32 = 2;
let mut indx: i32 = ((rr * ts) + cc);
while (cc < (cc1 - 2)) {
{
let delh: f32 = ((cfa[((indx + 1)) as usize] - cfa[((indx - 1)) as usize])).abs();
let delv: f32 = ((cfa[((indx + v1)) as usize] - cfa[((indx - v1)) as usize])).abs();
dirwts0[(indx) as usize] = (((eps + ((cfa[((indx + v2)) as usize] - cfa[(indx) as usize])).abs()) + ((cfa[(indx) as usize] - cfa[((indx - v2)) as usize])).abs()) + delv);
dirwts1[(indx) as usize] = (((eps + ((cfa[((indx + 2)) as usize] - cfa[(indx) as usize])).abs()) + ((cfa[(indx) as usize] - cfa[((indx - 2)) as usize])).abs()) + delh);
delhvsqsum[(indx) as usize] = (square(delh) + square(delv));
}
cc += 1;
indx += 1;
}
}
rr += 1;
}
}
{
let mut rr: i32 = 4;
while (rr < (rr1 - 4)) {
{
let mut fcswitch: bool = ((fc(m, rr, 4) & 1) != 0);
{
let mut cc: i32 = 4;
let mut indx: i32 = ((rr * ts) + cc);
while (cc < (cc1 - 4)) {
{
let cru: f32 = ((cfa[((indx - v1)) as usize] * (dirwts0[((indx - v2)) as usize] + dirwts0[(indx) as usize])) / ((dirwts0[((indx - v2)) as usize] * (eps + cfa[(indx) as usize])) + (dirwts0[(indx) as usize] * (eps + cfa[((indx - v2)) as usize]))));
let crd: f32 = ((cfa[((indx + v1)) as usize] * (dirwts0[((indx + v2)) as usize] + dirwts0[(indx) as usize])) / ((dirwts0[((indx + v2)) as usize] * (eps + cfa[(indx) as usize])) + (dirwts0[(indx) as usize] * (eps + cfa[((indx + v2)) as usize]))));
let crl: f32 = ((cfa[((indx - 1)) as usize] * (dirwts1[((indx - 2)) as usize] + dirwts1[(indx) as usize])) / ((dirwts1[((indx - 2)) as usize] * (eps + cfa[(indx) as usize])) + (dirwts1[(indx) as usize] * (eps + cfa[((indx - 2)) as usize]))));
let crr: f32 = ((cfa[((indx + 1)) as usize] * (dirwts1[((indx + 2)) as usize] + dirwts1[(indx) as usize])) / ((dirwts1[((indx + 2)) as usize] * (eps + cfa[(indx) as usize])) + (dirwts1[(indx) as usize] * (eps + cfa[((indx + 2)) as usize]))));
let guha: f32 = (cfa[((indx - v1)) as usize] + xdiv2f((cfa[(indx) as usize] - cfa[((indx - v2)) as usize])));
let gdha: f32 = (cfa[((indx + v1)) as usize] + xdiv2f((cfa[(indx) as usize] - cfa[((indx + v2)) as usize])));
let glha: f32 = (cfa[((indx - 1)) as usize] + xdiv2f((cfa[(indx) as usize] - cfa[((indx - 2)) as usize])));
let grha: f32 = (cfa[((indx + 1)) as usize] + xdiv2f((cfa[(indx) as usize] - cfa[((indx + 2)) as usize])));
let mut guar: f32;
let mut gdar: f32;
let mut glar: f32;
let mut grar: f32;
if (((1.0f32 - cru)).abs() < arthresh) {
guar = (cfa[(indx) as usize] * cru);
}
else {
guar = guha;
}
if (((1.0f32 - crd)).abs() < arthresh) {
gdar = (cfa[(indx) as usize] * crd);
}
else {
gdar = gdha;
}
if (((1.0f32 - crl)).abs() < arthresh) {
glar = (cfa[(indx) as usize] * crl);
}
else {
glar = glha;
}
if (((1.0f32 - crr)).abs() < arthresh) {
grar = (cfa[(indx) as usize] * crr);
}
else {
grar = grha;
}
let hwt: f32 = (dirwts1[((indx - 1)) as usize] / (dirwts1[((indx - 1)) as usize] + dirwts1[((indx + 1)) as usize]));
let vwt: f32 = (dirwts0[((indx - v1)) as usize] / (dirwts0[((indx + v1)) as usize] + dirwts0[((indx - v1)) as usize]));
let Gintvha: f32 = ((vwt * gdha) + ((1.0f32 - vwt) * guha));
let Ginthha: f32 = ((hwt * grha) + ((1.0f32 - hwt) * glha));
if fcswitch {
vcd[(indx) as usize] = (cfa[(indx) as usize] - ((vwt * gdar) + ((1.0f32 - vwt) * guar)));
hcd[(indx) as usize] = (cfa[(indx) as usize] - ((hwt * grar) + ((1.0f32 - hwt) * glar)));
vcdalt[(indx) as usize] = (cfa[(indx) as usize] - Gintvha);
hcdalt[(indx) as usize] = (cfa[(indx) as usize] - Ginthha);
}
else {
vcd[(indx) as usize] = (((vwt * gdar) + ((1.0f32 - vwt) * guar)) - cfa[(indx) as usize]);
hcd[(indx) as usize] = (((hwt * grar) + ((1.0f32 - hwt) * glar)) - cfa[(indx) as usize]);
vcdalt[(indx) as usize] = (Gintvha - cfa[(indx) as usize]);
hcdalt[(indx) as usize] = (Ginthha - cfa[(indx) as usize]);
}
fcswitch = !(fcswitch);
if (((cfa[(indx) as usize] > clip_pt8) || (Gintvha > clip_pt8)) || (Ginthha > clip_pt8)) {
guar = guha;
gdar = gdha;
glar = glha;
grar = grha;
vcd[(indx) as usize] = vcdalt[(indx) as usize];
hcd[(indx) as usize] = hcdalt[(indx) as usize];
}
dgintv[(indx) as usize] = (square((guha - gdha))).min(square((guar - gdar)));
dginth[(indx) as usize] = (square((glha - grha))).min(square((glar - grar)));
}
cc += 1;
indx += 1;
}
}
}
rr += 1;
}
}
{
let mut rr: i32 = 4;
while (rr < (rr1 - 4)) {
{
{
let mut cc: i32 = 4;
let mut indx: i32 = ((rr * ts) + cc);
let mut c: i32 = (fc(m, rr, cc) & 1);
while (cc < (cc1 - 4)) {
{
let hcdvar: f32 = ((3.0f32 * ((square(hcd[((indx - 2)) as usize]) + square(hcd[(indx) as usize])) + square(hcd[((indx + 2)) as usize]))) - square(((hcd[((indx - 2)) as usize] + hcd[(indx) as usize]) + hcd[((indx + 2)) as usize])));
let hcdaltvar: f32 = ((3.0f32 * ((square(hcdalt[((indx - 2)) as usize]) + square(hcdalt[(indx) as usize])) + square(hcdalt[((indx + 2)) as usize]))) - square(((hcdalt[((indx - 2)) as usize] + hcdalt[(indx) as usize]) + hcdalt[((indx + 2)) as usize])));
let vcdvar: f32 = ((3.0f32 * ((square(vcd[((indx - v2)) as usize]) + square(vcd[(indx) as usize])) + square(vcd[((indx + v2)) as usize]))) - square(((vcd[((indx - v2)) as usize] + vcd[(indx) as usize]) + vcd[((indx + v2)) as usize])));
let vcdaltvar: f32 = ((3.0f32 * ((square(vcdalt[((indx - v2)) as usize]) + square(vcdalt[(indx) as usize])) + square(vcdalt[((indx + v2)) as usize]))) - square(((vcdalt[((indx - v2)) as usize] + vcdalt[(indx) as usize]) + vcdalt[((indx + v2)) as usize])));
if (hcdaltvar < hcdvar) {
hcd[(indx) as usize] = hcdalt[(indx) as usize];
}
if (vcdaltvar < vcdvar) {
vcd[(indx) as usize] = vcdalt[(indx) as usize];
}
let mut Gintv: f32;
let mut Ginth: f32;
if (c != 0) {
Ginth = (-(hcd[(indx) as usize]) + cfa[(indx) as usize]);
Gintv = (-(vcd[(indx) as usize]) + cfa[(indx) as usize]);
if (hcd[(indx) as usize] > (0 as f32)) {
if ((3.0f32 * hcd[(indx) as usize]) > (Ginth + cfa[(indx) as usize])) {
hcd[(indx) as usize] = (-(ulim(Ginth, cfa[((indx - 1)) as usize], cfa[((indx + 1)) as usize])) + cfa[(indx) as usize]);
}
else {
let hwt: f32 = (1.0f32 - ((3.0f32 * hcd[(indx) as usize]) / ((eps + Ginth) + cfa[(indx) as usize])));
hcd[(indx) as usize] = ((hwt * hcd[(indx) as usize]) + ((1.0f32 - hwt) * (-(ulim(Ginth, cfa[((indx - 1)) as usize], cfa[((indx + 1)) as usize])) + cfa[(indx) as usize])));
}
}
if (vcd[(indx) as usize] > (0 as f32)) {
if ((3.0f32 * vcd[(indx) as usize]) > (Gintv + cfa[(indx) as usize])) {
vcd[(indx) as usize] = (-(ulim(Gintv, cfa[((indx - v1)) as usize], cfa[((indx + v1)) as usize])) + cfa[(indx) as usize]);
}
else {
let vwt: f32 = (1.0f32 - ((3.0f32 * vcd[(indx) as usize]) / ((eps + Gintv) + cfa[(indx) as usize])));
vcd[(indx) as usize] = ((vwt * vcd[(indx) as usize]) + ((1.0f32 - vwt) * (-(ulim(Gintv, cfa[((indx - v1)) as usize], cfa[((indx + v1)) as usize])) + cfa[(indx) as usize])));
}
}
if (Ginth > clip_pt) {
hcd[(indx) as usize] = (-(ulim(Ginth, cfa[((indx - 1)) as usize], cfa[((indx + 1)) as usize])) + cfa[(indx) as usize]);
}
if (Gintv > clip_pt) {
vcd[(indx) as usize] = (-(ulim(Gintv, cfa[((indx - v1)) as usize], cfa[((indx + v1)) as usize])) + cfa[(indx) as usize]);
}
}
else {
Ginth = (hcd[(indx) as usize] + cfa[(indx) as usize]);
Gintv = (vcd[(indx) as usize] + cfa[(indx) as usize]);
if (hcd[(indx) as usize] < (0 as f32)) {
if ((3.0f32 * hcd[(indx) as usize]) < -((Ginth + cfa[(indx) as usize]))) {
hcd[(indx) as usize] = (ulim(Ginth, cfa[((indx - 1)) as usize], cfa[((indx + 1)) as usize]) - cfa[(indx) as usize]);
}
else {
let mut hwt: f32 = (1.0f32 + ((3.0f32 * hcd[(indx) as usize]) / ((eps + Ginth) + cfa[(indx) as usize])));
hcd[(indx) as usize] = ((hwt * hcd[(indx) as usize]) + ((1.0f32 - hwt) * (ulim(Ginth, cfa[((indx - 1)) as usize], cfa[((indx + 1)) as usize]) - cfa[(indx) as usize])));
}
}
if (vcd[(indx) as usize] < (0 as f32)) {
if ((3.0f32 * vcd[(indx) as usize]) < -((Gintv + cfa[(indx) as usize]))) {
vcd[(indx) as usize] = (ulim(Gintv, cfa[((indx - v1)) as usize], cfa[((indx + v1)) as usize]) - cfa[(indx) as usize]);
}
else {
let vwt: f32 = (1.0f32 + ((3.0f32 * vcd[(indx) as usize]) / ((eps + Gintv) + cfa[(indx) as usize])));
vcd[(indx) as usize] = ((vwt * vcd[(indx) as usize]) + ((1.0f32 - vwt) * (ulim(Gintv, cfa[((indx - v1)) as usize], cfa[((indx + v1)) as usize]) - cfa[(indx) as usize])));
}
}
if (Ginth > clip_pt) {
hcd[(indx) as usize] = (ulim(Ginth, cfa[((indx - 1)) as usize], cfa[((indx + 1)) as usize]) - cfa[(indx) as usize]);
}
if (Gintv > clip_pt) {
vcd[(indx) as usize] = (ulim(Gintv, cfa[((indx - v1)) as usize], cfa[((indx + v1)) as usize]) - cfa[(indx) as usize]);
}
cddiffsq[(indx) as usize] = square((vcd[(indx) as usize] - hcd[(indx) as usize]));
}
c = (!((c != 0)) as i32);
}
cc += 1;
indx += 1;
}
}
}
rr += 1;
}
}
{
let mut rr: i32 = 6;
while (rr < (rr1 - 6)) {
{
{
let mut cc: i32 = (6 + (fc(m, rr, 2) & 1));
let mut indx: i32 = ((rr * ts) + cc);
while (cc < (cc1 - 6)) {
{
let uave: f32 = (((vcd[(indx) as usize] + vcd[((indx - v1)) as usize]) + vcd[((indx - v2)) as usize]) + vcd[((indx - v3)) as usize]);
let dave: f32 = (((vcd[(indx) as usize] + vcd[((indx + v1)) as usize]) + vcd[((indx + v2)) as usize]) + vcd[((indx + v3)) as usize]);
let lave: f32 = (((hcd[(indx) as usize] + hcd[((indx - 1)) as usize]) + hcd[((indx - 2)) as usize]) + hcd[((indx - 3)) as usize]);
let rave: f32 = (((hcd[(indx) as usize] + hcd[((indx + 1)) as usize]) + hcd[((indx + 2)) as usize]) + hcd[((indx + 3)) as usize]);
let mut Dgrbvvaru: f32 = (((square((vcd[(indx) as usize] - uave)) + square((vcd[((indx - v1)) as usize] - uave))) + square((vcd[((indx - v2)) as usize] - uave))) + square((vcd[((indx - v3)) as usize] - uave)));
let mut Dgrbvvard: f32 = (((square((vcd[(indx) as usize] - dave)) + square((vcd[((indx + v1)) as usize] - dave))) + square((vcd[((indx + v2)) as usize] - dave))) + square((vcd[((indx + v3)) as usize] - dave)));
let mut Dgrbhvarl: f32 = (((square((hcd[(indx) as usize] - lave)) + square((hcd[((indx - 1)) as usize] - lave))) + square((hcd[((indx - 2)) as usize] - lave))) + square((hcd[((indx - 3)) as usize] - lave)));
let mut Dgrbhvarr: f32 = (((square((hcd[(indx) as usize] - rave)) + square((hcd[((indx + 1)) as usize] - rave))) + square((hcd[((indx + 2)) as usize] - rave))) + square((hcd[((indx + 3)) as usize] - rave)));
let hwt: f32 = (dirwts1[((indx - 1)) as usize] / (dirwts1[((indx - 1)) as usize] + dirwts1[((indx + 1)) as usize]));
let vwt: f32 = (dirwts0[((indx - v1)) as usize] / (dirwts0[((indx + v1)) as usize] + dirwts0[((indx - v1)) as usize]));
let vcdvar: f32 = ((epssq + (vwt * Dgrbvvard)) + ((1.0f32 - vwt) * Dgrbvvaru));
let hcdvar: f32 = ((epssq + (hwt * Dgrbhvarr)) + ((1.0f32 - hwt) * Dgrbhvarl));
Dgrbvvaru = ((dgintv[(indx) as usize] + dgintv[((indx - v1)) as usize]) + dgintv[((indx - v2)) as usize]);
Dgrbvvard = ((dgintv[(indx) as usize] + dgintv[((indx + v1)) as usize]) + dgintv[((indx + v2)) as usize]);
Dgrbhvarl = ((dginth[(indx) as usize] + dginth[((indx - 1)) as usize]) + dginth[((indx - 2)) as usize]);
Dgrbhvarr = ((dginth[(indx) as usize] + dginth[((indx + 1)) as usize]) + dginth[((indx + 2)) as usize]);
let mut vcdvar1: f32 = ((epssq + (vwt * Dgrbvvard)) + ((1.0f32 - vwt) * Dgrbvvaru));
let mut hcdvar1: f32 = ((epssq + (hwt * Dgrbhvarr)) + ((1.0f32 - hwt) * Dgrbhvarl));
let varwt: f32 = (hcdvar / (vcdvar + hcdvar));
let diffwt: f32 = (hcdvar1 / (vcdvar1 + hcdvar1));
if ((((0.5f32 - varwt) * (0.5f32 - diffwt)) > (0 as f32)) && (((0.5f32 - diffwt)).abs() < ((0.5f32 - varwt)).abs())) {
hvwt[((indx >> 1)) as usize] = varwt;
}
else {
hvwt[((indx >> 1)) as usize] = diffwt;
}
}
cc += 2;
indx += 2;
}
}
}
rr += 1;
}
}
{
let mut rr: i32 = 6;
while (rr < (rr1 - 6)) {
{
let mut cc: i32 = (6 + (fc(m, rr, 2) & 1));
let mut indx: i32 = ((rr * ts) + cc);
{
while (cc < (cc1 - 6)) {
{
nyqutest[((indx >> 1)) as usize] = (((((gaussodd[(0) as usize] * cddiffsq[(indx) as usize]) + (gaussodd[(1) as usize] * (((cddiffsq[((indx - m1)) as usize] + cddiffsq[((indx + p1)) as usize]) + cddiffsq[((indx - p1)) as usize]) + cddiffsq[((indx + m1)) as usize]))) + (gaussodd[(2) as usize] * (((cddiffsq[((indx - v2)) as usize] + cddiffsq[((indx - 2)) as usize]) + cddiffsq[((indx + 2)) as usize]) + cddiffsq[((indx + v2)) as usize]))) + (gaussodd[(3) as usize] * (((cddiffsq[((indx - m2)) as usize] + cddiffsq[((indx + p2)) as usize]) + cddiffsq[((indx - p2)) as usize]) + cddiffsq[((indx + m2)) as usize]))) - ((((((gaussgrad[(0) as usize] * delhvsqsum[(indx) as usize]) + (gaussgrad[(1) as usize] * (((delhvsqsum[((indx - v1)) as usize] + delhvsqsum[((indx + 1)) as usize]) + delhvsqsum[((indx - 1)) as usize]) + delhvsqsum[((indx + v1)) as usize]))) + (gaussgrad[(2) as usize] * (((delhvsqsum[((indx - m1)) as usize] + delhvsqsum[((indx + p1)) as usize]) + delhvsqsum[((indx - p1)) as usize]) + delhvsqsum[((indx + m1)) as usize]))) + (gaussgrad[(3) as usize] * (((delhvsqsum[((indx - v2)) as usize] + delhvsqsum[((indx - 2)) as usize]) + delhvsqsum[((indx + 2)) as usize]) + delhvsqsum[((indx + v2)) as usize]))) + (gaussgrad[(4) as usize] * (((((((delhvsqsum[(((indx - v2) - 1)) as usize] + delhvsqsum[(((indx - v2) + 1)) as usize]) + delhvsqsum[(((indx - ts) - 2)) as usize]) + delhvsqsum[(((indx - ts) + 2)) as usize]) + delhvsqsum[(((indx + ts) - 2)) as usize]) + delhvsqsum[(((indx + ts) + 2)) as usize]) + delhvsqsum[(((indx + v2) - 1)) as usize]) + delhvsqsum[(((indx + v2) + 1)) as usize]))) + (gaussgrad[(5) as usize] * (((delhvsqsum[((indx - m2)) as usize] + delhvsqsum[((indx + p2)) as usize]) + delhvsqsum[((indx - p2)) as usize]) + delhvsqsum[((indx + m2)) as usize]))));
}
cc += 2;
indx += 2;
}
}
}
rr += 1;
}
}
let mut nystartrow: i32 = 0;
let mut nyendrow: i32 = 0;
let mut nystartcol: i32 = (ts + 1);
let mut nyendcol: i32 = 0;
{
let mut rr: i32 = 6;
while (rr < (rr1 - 6)) {
{
{
let mut cc: i32 = (6 + (fc(m, rr, 2) & 1));
let mut indx: i32 = ((rr * ts) + cc);
while (cc < (cc1 - 6)) {
{
if (nyqutest[((indx >> 1)) as usize] > 0.0f32) {
nyquist[((indx >> 1)) as usize] = 1;
nystartrow = (if (nystartrow != 0) { nystartrow } else { rr });
nyendrow = rr;
nystartcol = (if (nystartcol > cc) { cc } else { nystartcol });
nyendcol = (if (nyendcol < cc) { cc } else { nyendcol });
}
}
cc += 2;
indx += 2;
}
}
}
rr += 1;
}
}
let mut doNyquist: bool = ((nystartrow != nyendrow) && (nystartcol != nyendcol));
if doNyquist {
nyendrow += 1;
nyendcol += 1;
nystartcol -= (nystartcol & 1);
nystartrow = (8).max(nystartrow);
nyendrow = ((rr1 - 8)).min(nyendrow);
nystartcol = (8).max(nystartcol);
nyendcol = ((cc1 - 8)).min(nyendcol);
nyquist2.fill(0);
{
let mut rr: i32 = nystartrow;
while (rr < nyendrow) {
{
{
let mut indx: i32 = (((rr * ts) + nystartcol) + (fc(m, rr, 2) & 1));
while (indx < ((rr * ts) + nyendcol)) {
{
let mut nyquisttemp: i32 = (((((((nyquist[(((indx - v2) >> 1)) as usize] + nyquist[(((indx - m1) >> 1)) as usize]) + nyquist[(((indx + p1) >> 1)) as usize]) + nyquist[(((indx - 2) >> 1)) as usize]) + nyquist[(((indx + 2) >> 1)) as usize]) + nyquist[(((indx - p1) >> 1)) as usize]) + nyquist[(((indx + m1) >> 1)) as usize]) + nyquist[(((indx + v2) >> 1)) as usize]);
nyquist2[((indx >> 1)) as usize] = (if (nyquisttemp > 4) { 1 } else { (if (nyquisttemp < 4) { 0 } else { nyquist[((indx >> 1)) as usize] }) });
}
indx += 2;
}
}
}
rr += 1;
}
}
{
let mut rr: i32 = nystartrow;
while (rr < nyendrow) {
{
let mut indx: i32 = (((rr * ts) + nystartcol) + (fc(m, rr, 2) & 1));
while (indx < ((rr * ts) + nyendcol)) {
{
if (nyquist2[((indx >> 1)) as usize] != 0) {
let mut sumcfa: f32 = 0.0f32;
let mut sumh: f32 = 0.0f32;
let mut sumv: f32 = 0.0f32;
let mut sumsqh: f32 = 0.0f32;
let mut sumsqv: f32 = 0.0f32;
let mut areawt: f32 = 0.0f32;
{
let mut i: i32 = -(6);
while (i < 7) {
{
let mut indx1: i32 = ((indx + (i * ts)) - 6);
{
let mut j: i32 = -(6);
while (j < 7) {
{
if (nyquist2[((indx1 >> 1)) as usize] != 0) {
let mut cfatemp: f32 = cfa[(indx1) as usize];
sumcfa += cfatemp;
sumh += (cfa[((indx1 - 1)) as usize] + cfa[((indx1 + 1)) as usize]);
sumv += (cfa[((indx1 - v1)) as usize] + cfa[((indx1 + v1)) as usize]);
sumsqh += (square((cfatemp - cfa[((indx1 - 1)) as usize])) + square((cfatemp - cfa[((indx1 + 1)) as usize])));
sumsqv += (square((cfatemp - cfa[((indx1 - v1)) as usize])) + square((cfatemp - cfa[((indx1 + v1)) as usize])));
areawt += (1 as f32);
}
}
j += 2;
indx1 += 2;
}
}
}
i += 2;
}
}
sumh = (sumcfa - xdiv2f(sumh));
sumv = (sumcfa - xdiv2f(sumv));
areawt = xdiv2f(areawt);
let hcdvar: f32 = (epssq + (((areawt * sumsqh) - (sumh * sumh))).abs());
let vcdvar: f32 = (epssq + (((areawt * sumsqv) - (sumv * sumv))).abs());
hvwt[((indx >> 1)) as usize] = (hcdvar / (vcdvar + hcdvar));
}
}
indx += 2;
}
}
rr += 1;
}
}
}
{
let mut rr: i32 = 8;
while (rr < (rr1 - 8)) {
{
let mut indx: i32 = (((rr * ts) + 8) + (fc(m, rr, 2) & 1));
while (indx < (((rr * ts) + cc1) - 8)) {
{
let hvwtalt: f32 = xdiv((((hvwt[(((indx - m1) >> 1)) as usize] + hvwt[(((indx + p1) >> 1)) as usize]) + hvwt[(((indx - p1) >> 1)) as usize]) + hvwt[(((indx + m1) >> 1)) as usize]), 2);
hvwt[((indx >> 1)) as usize] = (if (((0.5f32 - hvwt[((indx >> 1)) as usize])).abs() < ((0.5f32 - hvwtalt)).abs()) { hvwtalt } else { hvwt[((indx >> 1)) as usize] });
Dgrb[(0) as usize][((indx >> 1)) as usize] = interpolatef(hvwt[((indx >> 1)) as usize], vcd[(indx) as usize], hcd[(indx) as usize]);
rgbgreen[(indx) as usize] = (cfa[(indx) as usize] + Dgrb[(0) as usize][((indx >> 1)) as usize]);
Dgrb2[((indx >> 1)) as usize].h = (if (nyquist2[((indx >> 1)) as usize] != 0) { square((rgbgreen[(indx) as usize] - xdiv2f((rgbgreen[((indx - 1)) as usize] + rgbgreen[((indx + 1)) as usize])))) } else { 0.0f32 });
Dgrb2[((indx >> 1)) as usize].v = (if (nyquist2[((indx >> 1)) as usize] != 0) { square((rgbgreen[(indx) as usize] - xdiv2f((rgbgreen[((indx - v1)) as usize] + rgbgreen[((indx + v1)) as usize])))) } else { 0.0f32 });
}
indx += 2;
}
}
rr += 1;
}
}
if doNyquist {
{
let mut rr: i32 = nystartrow;
while (rr < nyendrow) {
{
let mut indx: i32 = (((rr * ts) + nystartcol) + (fc(m, rr, 2) & 1));
while (indx < ((rr * ts) + nyendcol)) {
{
if (nyquist2[((indx >> 1)) as usize] != 0) {
let gvarh: f32 = (epssq + ((((gquinc[(0) as usize] * Dgrb2[((indx >> 1)) as usize].h) + (gquinc[(1) as usize] * (((Dgrb2[(((indx - m1) >> 1)) as usize].h + Dgrb2[(((indx + p1) >> 1)) as usize].h) + Dgrb2[(((indx - p1) >> 1)) as usize].h) + Dgrb2[(((indx + m1) >> 1)) as usize].h))) + (gquinc[(2) as usize] * (((Dgrb2[(((indx - v2) >> 1)) as usize].h + Dgrb2[(((indx - 2) >> 1)) as usize].h) + Dgrb2[(((indx + 2) >> 1)) as usize].h) + Dgrb2[(((indx + v2) >> 1)) as usize].h))) + (gquinc[(3) as usize] * (((Dgrb2[(((indx - m2) >> 1)) as usize].h + Dgrb2[(((indx + p2) >> 1)) as usize].h) + Dgrb2[(((indx - p2) >> 1)) as usize].h) + Dgrb2[(((indx + m2) >> 1)) as usize].h))));
let gvarv: f32 = (epssq + ((((gquinc[(0) as usize] * Dgrb2[((indx >> 1)) as usize].v) + (gquinc[(1) as usize] * (((Dgrb2[(((indx - m1) >> 1)) as usize].v + Dgrb2[(((indx + p1) >> 1)) as usize].v) + Dgrb2[(((indx - p1) >> 1)) as usize].v) + Dgrb2[(((indx + m1) >> 1)) as usize].v))) + (gquinc[(2) as usize] * (((Dgrb2[(((indx - v2) >> 1)) as usize].v + Dgrb2[(((indx - 2) >> 1)) as usize].v) + Dgrb2[(((indx + 2) >> 1)) as usize].v) + Dgrb2[(((indx + v2) >> 1)) as usize].v))) + (gquinc[(3) as usize] * (((Dgrb2[(((indx - m2) >> 1)) as usize].v + Dgrb2[(((indx + p2) >> 1)) as usize].v) + Dgrb2[(((indx - p2) >> 1)) as usize].v) + Dgrb2[(((indx + m2) >> 1)) as usize].v))));
Dgrb[(0) as usize][((indx >> 1)) as usize] = (((hcd[(indx) as usize] * gvarv) + (vcd[(indx) as usize] * gvarh)) / (gvarv + gvarh));
rgbgreen[(indx) as usize] = (cfa[(indx) as usize] + Dgrb[(0) as usize][((indx >> 1)) as usize]);
}
}
indx += 2;
}
}
rr += 1;
}
}
}
{
let mut rr: i32 = 6;
while (rr < (rr1 - 6)) {
{
if ((fc(m, rr, 2) & 1) == 0) {
{
let mut cc: i32 = 6;
let mut indx: i32 = ((rr * ts) + cc);
while (cc < (cc1 - 6)) {
{
delp[((indx >> 1)) as usize] = ((cfa[((indx + p1)) as usize] - cfa[((indx - p1)) as usize])).abs();
delm[((indx >> 1)) as usize] = ((cfa[((indx + m1)) as usize] - cfa[((indx - m1)) as usize])).abs();
Dgrbsq1p[((indx >> 1)) as usize] = (square((cfa[((indx + 1)) as usize] - cfa[(((indx + 1) - p1)) as usize])) + square((cfa[((indx + 1)) as usize] - cfa[(((indx + 1) + p1)) as usize])));
Dgrbsq1m[((indx >> 1)) as usize] = (square((cfa[((indx + 1)) as usize] - cfa[(((indx + 1) - m1)) as usize])) + square((cfa[((indx + 1)) as usize] - cfa[(((indx + 1) + m1)) as usize])));
}
cc += 2;
indx += 2;
}
}
}
else {
{
let mut cc: i32 = 6;
let mut indx: i32 = ((rr * ts) + cc);
while (cc < (cc1 - 6)) {
{
Dgrbsq1p[((indx >> 1)) as usize] = (square((cfa[(indx) as usize] - cfa[((indx - p1)) as usize])) + square((cfa[(indx) as usize] - cfa[((indx + p1)) as usize])));
Dgrbsq1m[((indx >> 1)) as usize] = (square((cfa[(indx) as usize] - cfa[((indx - m1)) as usize])) + square((cfa[(indx) as usize] - cfa[((indx + m1)) as usize])));
delp[((indx >> 1)) as usize] = ((cfa[(((indx + 1) + p1)) as usize] - cfa[(((indx + 1) - p1)) as usize])).abs();
delm[((indx >> 1)) as usize] = ((cfa[(((indx + 1) + m1)) as usize] - cfa[(((indx + 1) - m1)) as usize])).abs();
}
cc += 2;
indx += 2;
}
}
}
}
rr += 1;
}
}
{
let mut rr: i32 = 8;
while (rr < (rr1 - 8)) {
{
{
let mut cc: i32 = (8 + (fc(m, rr, 2) & 1));
let mut indx: i32 = ((rr * ts) + cc);
let mut indx1: i32 = (indx >> 1);
while (cc < (cc1 - 8)) {
{
let mut crse: f32 = (xmul2f(cfa[((indx + m1)) as usize]) / ((eps + cfa[(indx) as usize]) + cfa[((indx + m2)) as usize]));
let mut crnw: f32 = (xmul2f(cfa[((indx - m1)) as usize]) / ((eps + cfa[(indx) as usize]) + cfa[((indx - m2)) as usize]));
let mut crne: f32 = (xmul2f(cfa[((indx + p1)) as usize]) / ((eps + cfa[(indx) as usize]) + cfa[((indx + p2)) as usize]));
let mut crsw: f32 = (xmul2f(cfa[((indx - p1)) as usize]) / ((eps + cfa[(indx) as usize]) + cfa[((indx - p2)) as usize]));
let mut rbse: f32;
let mut rbnw: f32;
let mut rbne: f32;
let mut rbsw: f32;
if (((1.0f32 - crse)).abs() < arthresh) {
rbse = (cfa[(indx) as usize] * crse);
}
else {
rbse = (cfa[((indx + m1)) as usize] + xdiv2f((cfa[(indx) as usize] - cfa[((indx + m2)) as usize])));
}
if (((1.0f32 - crnw)).abs() < arthresh) {
rbnw = (cfa[(indx) as usize] * crnw);
}
else {
rbnw = (cfa[((indx - m1)) as usize] + xdiv2f((cfa[(indx) as usize] - cfa[((indx - m2)) as usize])));
}
if (((1.0f32 - crne)).abs() < arthresh) {
rbne = (cfa[(indx) as usize] * crne);
}
else {
rbne = (cfa[((indx + p1)) as usize] + xdiv2f((cfa[(indx) as usize] - cfa[((indx + p2)) as usize])));
}
if (((1.0f32 - crsw)).abs() < arthresh) {
rbsw = (cfa[(indx) as usize] * crsw);
}
else {
rbsw = (cfa[((indx - p1)) as usize] + xdiv2f((cfa[(indx) as usize] - cfa[((indx - p2)) as usize])));
}
let wtse: f32 = (((eps + delm[(indx1) as usize]) + delm[(((indx + m1) >> 1)) as usize]) + delm[(((indx + m2) >> 1)) as usize]);
let wtnw: f32 = (((eps + delm[(indx1) as usize]) + delm[(((indx - m1) >> 1)) as usize]) + delm[(((indx - m2) >> 1)) as usize]);
let wtne: f32 = (((eps + delp[(indx1) as usize]) + delp[(((indx + p1) >> 1)) as usize]) + delp[(((indx + p2) >> 1)) as usize]);
let wtsw: f32 = (((eps + delp[(indx1) as usize]) + delp[(((indx - p1) >> 1)) as usize]) + delp[(((indx - p2) >> 1)) as usize]);
rbm[(indx1) as usize] = (((wtse * rbnw) + (wtnw * rbse)) / (wtse + wtnw));
rbp[(indx1) as usize] = (((wtne * rbsw) + (wtsw * rbne)) / (wtne + wtsw));
let rbvarm: f32 = (epssq + ((gausseven[(0) as usize] * (((Dgrbsq1m[(((indx - v1) >> 1)) as usize] + Dgrbsq1m[(((indx - 1) >> 1)) as usize]) + Dgrbsq1m[(((indx + 1) >> 1)) as usize]) + Dgrbsq1m[(((indx + v1) >> 1)) as usize])) + (gausseven[(1) as usize] * (((((((Dgrbsq1m[((((indx - v2) - 1) >> 1)) as usize] + Dgrbsq1m[((((indx - v2) + 1) >> 1)) as usize]) + Dgrbsq1m[((((indx - 2) - v1) >> 1)) as usize]) + Dgrbsq1m[((((indx + 2) - v1) >> 1)) as usize]) + Dgrbsq1m[((((indx - 2) + v1) >> 1)) as usize]) + Dgrbsq1m[((((indx + 2) + v1) >> 1)) as usize]) + Dgrbsq1m[((((indx + v2) - 1) >> 1)) as usize]) + Dgrbsq1m[((((indx + v2) + 1) >> 1)) as usize]))));
pmwt[(indx1) as usize] = (rbvarm / ((epssq + ((gausseven[(0) as usize] * (((Dgrbsq1p[(((indx - v1) >> 1)) as usize] + Dgrbsq1p[(((indx - 1) >> 1)) as usize]) + Dgrbsq1p[(((indx + 1) >> 1)) as usize]) + Dgrbsq1p[(((indx + v1) >> 1)) as usize])) + (gausseven[(1) as usize] * (((((((Dgrbsq1p[((((indx - v2) - 1) >> 1)) as usize] + Dgrbsq1p[((((indx - v2) + 1) >> 1)) as usize]) + Dgrbsq1p[((((indx - 2) - v1) >> 1)) as usize]) + Dgrbsq1p[((((indx + 2) - v1) >> 1)) as usize]) + Dgrbsq1p[((((indx - 2) + v1) >> 1)) as usize]) + Dgrbsq1p[((((indx + 2) + v1) >> 1)) as usize]) + Dgrbsq1p[((((indx + v2) - 1) >> 1)) as usize]) + Dgrbsq1p[((((indx + v2) + 1) >> 1)) as usize])))) + rbvarm));
if (rbp[(indx1) as usize] < cfa[(indx) as usize]) {
if (xmul2f(rbp[(indx1) as usize]) < cfa[(indx) as usize]) {
rbp[(indx1) as usize] = ulim(rbp[(indx1) as usize], cfa[((indx - p1)) as usize], cfa[((indx + p1)) as usize]);
}
else {
let pwt: f32 = (xmul2f((cfa[(indx) as usize] - rbp[(indx1) as usize])) / ((eps + rbp[(indx1) as usize]) + cfa[(indx) as usize]));
rbp[(indx1) as usize] = ((pwt * rbp[(indx1) as usize]) + ((1.0f32 - pwt) * ulim(rbp[(indx1) as usize], cfa[((indx - p1)) as usize], cfa[((indx + p1)) as usize])));
}
}
if (rbm[(indx1) as usize] < cfa[(indx) as usize]) {
if (xmul2f(rbm[(indx1) as usize]) < cfa[(indx) as usize]) {
rbm[(indx1) as usize] = ulim(rbm[(indx1) as usize], cfa[((indx - m1)) as usize], cfa[((indx + m1)) as usize]);
}
else {
let mwt: f32 = (xmul2f((cfa[(indx) as usize] - rbm[(indx1) as usize])) / ((eps + rbm[(indx1) as usize]) + cfa[(indx) as usize]));
rbm[(indx1) as usize] = ((mwt * rbm[(indx1) as usize]) + ((1.0f32 - mwt) * ulim(rbm[(indx1) as usize], cfa[((indx - m1)) as usize], cfa[((indx + m1)) as usize])));
}
}
if (rbp[(indx1) as usize] > clip_pt) {
rbp[(indx1) as usize] = ulim(rbp[(indx1) as usize], cfa[((indx - p1)) as usize], cfa[((indx + p1)) as usize]);
}
if (rbm[(indx1) as usize] > clip_pt) {
rbm[(indx1) as usize] = ulim(rbm[(indx1) as usize], cfa[((indx - m1)) as usize], cfa[((indx + m1)) as usize]);
}
}
cc += 2;
indx += 2;
indx1 += 1;
}
}
}
rr += 1;
}
}
{
let mut rr: i32 = 10;
while (rr < (rr1 - 10)) {
{
let mut cc: i32 = (10 + (fc(m, rr, 2) & 1));
let mut indx: i32 = ((rr * ts) + cc);
let mut indx1: i32 = (indx >> 1);
while (cc < (cc1 - 10)) {
{
let pmwtalt: f32 = xdiv((((pmwt[(((indx - m1) >> 1)) as usize] + pmwt[(((indx + p1) >> 1)) as usize]) + pmwt[(((indx - p1) >> 1)) as usize]) + pmwt[(((indx + m1) >> 1)) as usize]), 2);
if (((0.5f32 - pmwt[(indx1) as usize])).abs() < ((0.5f32 - pmwtalt)).abs()) {
pmwt[(indx1) as usize] = pmwtalt;
}
rbint[(indx1) as usize] = xdiv2f(((cfa[(indx) as usize] + (rbm[(indx1) as usize] * (1.0f32 - pmwt[(indx1) as usize]))) + (rbp[(indx1) as usize] * pmwt[(indx1) as usize])));
}
cc += 2;
indx += 2;
indx1 += 1;
}
}
rr += 1;
}
}
{
let mut rr: i32 = 12;
while (rr < (rr1 - 12)) {
{
let mut cc: i32 = (12 + (fc(m, rr, 2) & 1));
let mut indx: i32 = ((rr * ts) + cc);
let mut indx1: i32 = (indx >> 1);
while (cc < (cc1 - 12)) {
{
if (((0.5f32 - pmwt[((indx >> 1)) as usize])).abs() < ((0.5f32 - hvwt[((indx >> 1)) as usize])).abs()) {
cc += 2;
indx += 2;
indx1 += 1;
continue;
}
let cru: f32 = ((cfa[((indx - v1)) as usize] * 2.0f32) / ((eps + rbint[(indx1) as usize]) + rbint[((indx1 - v1)) as usize]));
let crd: f32 = ((cfa[((indx + v1)) as usize] * 2.0f32) / ((eps + rbint[(indx1) as usize]) + rbint[((indx1 + v1)) as usize]));
let crl: f32 = ((cfa[((indx - 1)) as usize] * 2.0f32) / ((eps + rbint[(indx1) as usize]) + rbint[((indx1 - 1)) as usize]));
let crr: f32 = ((cfa[((indx + 1)) as usize] * 2.0f32) / ((eps + rbint[(indx1) as usize]) + rbint[((indx1 + 1)) as usize]));
let mut gu: f32;
let mut gd: f32;
let mut gl: f32;
let mut gr: f32;
if (((1.0f32 - cru)).abs() < arthresh) {
gu = (rbint[(indx1) as usize] * cru);
}
else {
gu = (cfa[((indx - v1)) as usize] + xdiv2f((rbint[(indx1) as usize] - rbint[((indx1 - v1)) as usize])));
}
if (((1.0f32 - crd)).abs() < arthresh) {
gd = (rbint[(indx1) as usize] * crd);
}
else {
gd = (cfa[((indx + v1)) as usize] + xdiv2f((rbint[(indx1) as usize] - rbint[((indx1 + v1)) as usize])));
}
if (((1.0f32 - crl)).abs() < arthresh) {
gl = (rbint[(indx1) as usize] * crl);
}
else {
gl = (cfa[((indx - 1)) as usize] + xdiv2f((rbint[(indx1) as usize] - rbint[((indx1 - 1)) as usize])));
}
if (((1.0f32 - crr)).abs() < arthresh) {
gr = (rbint[(indx1) as usize] * crr);
}
else {
gr = (cfa[((indx + 1)) as usize] + xdiv2f((rbint[(indx1) as usize] - rbint[((indx1 + 1)) as usize])));
}
let mut Gintv: f32 = (((dirwts0[((indx - v1)) as usize] * gd) + (dirwts0[((indx + v1)) as usize] * gu)) / (dirwts0[((indx + v1)) as usize] + dirwts0[((indx - v1)) as usize]));
let mut Ginth: f32 = (((dirwts1[((indx - 1)) as usize] * gr) + (dirwts1[((indx + 1)) as usize] * gl)) / (dirwts1[((indx - 1)) as usize] + dirwts1[((indx + 1)) as usize]));
if (Gintv < rbint[(indx1) as usize]) {
if (((2 as f32) * Gintv) < rbint[(indx1) as usize]) {
Gintv = ulim(Gintv, cfa[((indx - v1)) as usize], cfa[((indx + v1)) as usize]);
}
else {
let mut vwt: f32 = ((2.0f32 * (rbint[(indx1) as usize] - Gintv)) / ((eps + Gintv) + rbint[(indx1) as usize]));
Gintv = ((vwt * Gintv) + ((1.0f32 - vwt) * ulim(Gintv, cfa[((indx - v1)) as usize], cfa[((indx + v1)) as usize])));
}
}
if (Ginth < rbint[(indx1) as usize]) {
if (((2 as f32) * Ginth) < rbint[(indx1) as usize]) {
Ginth = ulim(Ginth, cfa[((indx - 1)) as usize], cfa[((indx + 1)) as usize]);
}
else {
let hwt: f32 = ((2.0f32 * (rbint[(indx1) as usize] - Ginth)) / ((eps + Ginth) + rbint[(indx1) as usize]));
Ginth = ((hwt * Ginth) + ((1.0f32 - hwt) * ulim(Ginth, cfa[((indx - 1)) as usize], cfa[((indx + 1)) as usize])));
}
}
if (Ginth > clip_pt) {
Ginth = ulim(Ginth, cfa[((indx - 1)) as usize], cfa[((indx + 1)) as usize]);
}
if (Gintv > clip_pt) {
Gintv = ulim(Gintv, cfa[((indx - v1)) as usize], cfa[((indx + v1)) as usize]);
}
rgbgreen[(indx) as usize] = ((Ginth * (1.0f32 - hvwt[(indx1) as usize])) + (Gintv * hvwt[(indx1) as usize]));
Dgrb[(0) as usize][((indx >> 1)) as usize] = (rgbgreen[(indx) as usize] - cfa[(indx) as usize]);
}
cc += 2;
indx += 2;
indx1 += 1;
}
}
rr += 1;
}
}
{
let mut rr: i32 = (13 - ey);
while (rr < (rr1 - 12)) {
{
let mut indx1: i32 = ((((rr * ts) + 13) - ex) >> 1);
while (indx1 < ((((rr * ts) + cc1) - 12) >> 1)) {
{
Dgrb[(1) as usize][(indx1) as usize] = Dgrb[(0) as usize][(indx1) as usize];
Dgrb[(0) as usize][(indx1) as usize] = (0 as f32);
}
indx1 += 1;
}
}
rr += 2;
}
}
{
let mut rr: i32 = 14;
while (rr < (rr1 - 14)) {
{
let mut cc: i32 = (14 + (fc(m, rr, 2) & 1));
let mut indx: i32 = ((rr * ts) + cc);
let mut c: i32 = (1 - (fc(m, rr, cc) / 2));
while (cc < (cc1 - 14)) {
{
let wtnw: f32 = (1.0f32 / (((eps + ((Dgrb[(c) as usize][(((indx - m1) >> 1)) as usize] - Dgrb[(c) as usize][(((indx + m1) >> 1)) as usize])).abs()) + ((Dgrb[(c) as usize][(((indx - m1) >> 1)) as usize] - Dgrb[(c) as usize][(((indx - m3) >> 1)) as usize])).abs()) + ((Dgrb[(c) as usize][(((indx + m1) >> 1)) as usize] - Dgrb[(c) as usize][(((indx - m3) >> 1)) as usize])).abs()));
let wtne: f32 = (1.0f32 / (((eps + ((Dgrb[(c) as usize][(((indx + p1) >> 1)) as usize] - Dgrb[(c) as usize][(((indx - p1) >> 1)) as usize])).abs()) + ((Dgrb[(c) as usize][(((indx + p1) >> 1)) as usize] - Dgrb[(c) as usize][(((indx + p3) >> 1)) as usize])).abs()) + ((Dgrb[(c) as usize][(((indx - p1) >> 1)) as usize] - Dgrb[(c) as usize][(((indx + p3) >> 1)) as usize])).abs()));
let wtsw: f32 = (1.0f32 / (((eps + ((Dgrb[(c) as usize][(((indx - p1) >> 1)) as usize] - Dgrb[(c) as usize][(((indx + p1) >> 1)) as usize])).abs()) + ((Dgrb[(c) as usize][(((indx - p1) >> 1)) as usize] - Dgrb[(c) as usize][(((indx + m3) >> 1)) as usize])).abs()) + ((Dgrb[(c) as usize][(((indx + p1) >> 1)) as usize] - Dgrb[(c) as usize][(((indx - p3) >> 1)) as usize])).abs()));
let wtse: f32 = (1.0f32 / (((eps + ((Dgrb[(c) as usize][(((indx + m1) >> 1)) as usize] - Dgrb[(c) as usize][(((indx - m1) >> 1)) as usize])).abs()) + ((Dgrb[(c) as usize][(((indx + m1) >> 1)) as usize] - Dgrb[(c) as usize][(((indx - p3) >> 1)) as usize])).abs()) + ((Dgrb[(c) as usize][(((indx - m1) >> 1)) as usize] - Dgrb[(c) as usize][(((indx + m3) >> 1)) as usize])).abs()));
Dgrb[(c) as usize][((indx >> 1)) as usize] = (((((wtnw * ((((1.325f32 * Dgrb[(c) as usize][(((indx - m1) >> 1)) as usize]) - (0.175f32 * Dgrb[(c) as usize][(((indx - m3) >> 1)) as usize])) - (0.075f32 * Dgrb[(c) as usize][((((indx - m1) - 2) >> 1)) as usize])) - (0.075f32 * Dgrb[(c) as usize][((((indx - m1) - v2) >> 1)) as usize]))) + (wtne * ((((1.325f32 * Dgrb[(c) as usize][(((indx + p1) >> 1)) as usize]) - (0.175f32 * Dgrb[(c) as usize][(((indx + p3) >> 1)) as usize])) - (0.075f32 * Dgrb[(c) as usize][((((indx + p1) + 2) >> 1)) as usize])) - (0.075f32 * Dgrb[(c) as usize][((((indx + p1) + v2) >> 1)) as usize])))) + (wtsw * ((((1.325f32 * Dgrb[(c) as usize][(((indx - p1) >> 1)) as usize]) - (0.175f32 * Dgrb[(c) as usize][(((indx - p3) >> 1)) as usize])) - (0.075f32 * Dgrb[(c) as usize][((((indx - p1) - 2) >> 1)) as usize])) - (0.075f32 * Dgrb[(c) as usize][((((indx - p1) - v2) >> 1)) as usize])))) + (wtse * ((((1.325f32 * Dgrb[(c) as usize][(((indx + m1) >> 1)) as usize]) - (0.175f32 * Dgrb[(c) as usize][(((indx + m3) >> 1)) as usize])) - (0.075f32 * Dgrb[(c) as usize][((((indx + m1) + 2) >> 1)) as usize])) - (0.075f32 * Dgrb[(c) as usize][((((indx + m1) + v2) >> 1)) as usize])))) / (((wtnw + wtne) + wtsw) + wtse));
}
cc += 2;
indx += 2;
}
}
rr += 1;
}
}
{
let mut rr: i32 = 16;
while (rr < (rr1 - 16)) {
{
let mut row: i32 = (rr + top);
let mut col: i32 = (left + 16);
let mut indx: i32 = ((rr * ts) + 16);
if ((fc(m, rr, 2) & 1) == 1) {
{
while (indx < ((((rr * ts) + cc1) - 16) - (cc1 & 1))) {
{
if ((col < width) && (row < height)) {
let temp: f32 = (1.0f32 / ((((hvwt[(((indx - v1) >> 1)) as usize] + 2.0f32) - hvwt[(((indx + 1) >> 1)) as usize]) - hvwt[(((indx - 1) >> 1)) as usize]) + hvwt[(((indx + v1) >> 1)) as usize]));
out[((((row * width) + col) * 4)) as usize] = clampnan((rgbgreen[(indx) as usize] - (((((hvwt[(((indx - v1) >> 1)) as usize] * Dgrb[(0) as usize][(((indx - v1) >> 1)) as usize]) + ((1.0f32 - hvwt[(((indx + 1) >> 1)) as usize]) * Dgrb[(0) as usize][(((indx + 1) >> 1)) as usize])) + ((1.0f32 - hvwt[(((indx - 1) >> 1)) as usize]) * Dgrb[(0) as usize][(((indx - 1) >> 1)) as usize])) + (hvwt[(((indx + v1) >> 1)) as usize] * Dgrb[(0) as usize][(((indx + v1) >> 1)) as usize])) * temp)), 0.0f32, 1.0f32);
out[(((((row * width) + col) * 4) + 2)) as usize] = clampnan((rgbgreen[(indx) as usize] - (((((hvwt[(((indx - v1) >> 1)) as usize] * Dgrb[(1) as usize][(((indx - v1) >> 1)) as usize]) + ((1.0f32 - hvwt[(((indx + 1) >> 1)) as usize]) * Dgrb[(1) as usize][(((indx + 1) >> 1)) as usize])) + ((1.0f32 - hvwt[(((indx - 1) >> 1)) as usize]) * Dgrb[(1) as usize][(((indx - 1) >> 1)) as usize])) + (hvwt[(((indx + v1) >> 1)) as usize] * Dgrb[(1) as usize][(((indx + v1) >> 1)) as usize])) * temp)), 0.0f32, 1.0f32);
}
indx += 1;
col += 1;
if ((col < width) && (row < height)) {
out[((((row * width) + col) * 4)) as usize] = clampnan((rgbgreen[(indx) as usize] - Dgrb[(0) as usize][((indx >> 1)) as usize]), 0.0f32, 1.0f32);
out[(((((row * width) + col) * 4) + 2)) as usize] = clampnan((rgbgreen[(indx) as usize] - Dgrb[(1) as usize][((indx >> 1)) as usize]), 0.0f32, 1.0f32);
}
}
indx += 1;
col += 1;
}
}
if ((cc1 & 1) != 0) {
if ((col < width) && (row < height)) {
let temp: f32 = (1.0f32 / ((((hvwt[(((indx - v1) >> 1)) as usize] + 2.0f32) - hvwt[(((indx + 1) >> 1)) as usize]) - hvwt[(((indx - 1) >> 1)) as usize]) + hvwt[(((indx + v1) >> 1)) as usize]));
out[((((row * width) + col) * 4)) as usize] = clampnan((rgbgreen[(indx) as usize] - (((((hvwt[(((indx - v1) >> 1)) as usize] * Dgrb[(0) as usize][(((indx - v1) >> 1)) as usize]) + ((1.0f32 - hvwt[(((indx + 1) >> 1)) as usize]) * Dgrb[(0) as usize][(((indx + 1) >> 1)) as usize])) + ((1.0f32 - hvwt[(((indx - 1) >> 1)) as usize]) * Dgrb[(0) as usize][(((indx - 1) >> 1)) as usize])) + (hvwt[(((indx + v1) >> 1)) as usize] * Dgrb[(0) as usize][(((indx + v1) >> 1)) as usize])) * temp)), 0.0f32, 1.0f32);
out[(((((row * width) + col) * 4) + 2)) as usize] = clampnan((rgbgreen[(indx) as usize] - (((((hvwt[(((indx - v1) >> 1)) as usize] * Dgrb[(1) as usize][(((indx - v1) >> 1)) as usize]) + ((1.0f32 - hvwt[(((indx + 1) >> 1)) as usize]) * Dgrb[(1) as usize][(((indx + 1) >> 1)) as usize])) + ((1.0f32 - hvwt[(((indx - 1) >> 1)) as usize]) * Dgrb[(1) as usize][(((indx - 1) >> 1)) as usize])) + (hvwt[(((indx + v1) >> 1)) as usize] * Dgrb[(1) as usize][(((indx + v1) >> 1)) as usize])) * temp)), 0.0f32, 1.0f32);
}
}
}
else {
{
while (indx < ((((rr * ts) + cc1) - 16) - (cc1 & 1))) {
{
if ((col < width) && (row < height)) {
out[((((row * width) + col) * 4)) as usize] = clampnan((rgbgreen[(indx) as usize] - Dgrb[(0) as usize][((indx >> 1)) as usize]), 0.0f32, 1.0f32);
out[(((((row * width) + col) * 4) + 2)) as usize] = clampnan((rgbgreen[(indx) as usize] - Dgrb[(1) as usize][((indx >> 1)) as usize]), 0.0f32, 1.0f32);
}
indx += 1;
col += 1;
if ((col < width) && (row < height)) {
let temp: f32 = (1.0f32 / ((((hvwt[(((indx - v1) >> 1)) as usize] + 2.0f32) - hvwt[(((indx + 1) >> 1)) as usize]) - hvwt[(((indx - 1) >> 1)) as usize]) + hvwt[(((indx + v1) >> 1)) as usize]));
out[((((row * width) + col) * 4)) as usize] = clampnan((rgbgreen[(indx) as usize] - (((((hvwt[(((indx - v1) >> 1)) as usize] * Dgrb[(0) as usize][(((indx - v1) >> 1)) as usize]) + ((1.0f32 - hvwt[(((indx + 1) >> 1)) as usize]) * Dgrb[(0) as usize][(((indx + 1) >> 1)) as usize])) + ((1.0f32 - hvwt[(((indx - 1) >> 1)) as usize]) * Dgrb[(0) as usize][(((indx - 1) >> 1)) as usize])) + (hvwt[(((indx + v1) >> 1)) as usize] * Dgrb[(0) as usize][(((indx + v1) >> 1)) as usize])) * temp)), 0.0f32, 1.0f32);
out[(((((row * width) + col) * 4) + 2)) as usize] = clampnan((rgbgreen[(indx) as usize] - (((((hvwt[(((indx - v1) >> 1)) as usize] * Dgrb[(1) as usize][(((indx - v1) >> 1)) as usize]) + ((1.0f32 - hvwt[(((indx + 1) >> 1)) as usize]) * Dgrb[(1) as usize][(((indx + 1) >> 1)) as usize])) + ((1.0f32 - hvwt[(((indx - 1) >> 1)) as usize]) * Dgrb[(1) as usize][(((indx - 1) >> 1)) as usize])) + (hvwt[(((indx + v1) >> 1)) as usize] * Dgrb[(1) as usize][(((indx + v1) >> 1)) as usize])) * temp)), 0.0f32, 1.0f32);
}
}
indx += 1;
col += 1;
}
}
if ((cc1 & 1) != 0) {
if ((col < width) && (row < height)) {
out[((((row * width) + col) * 4)) as usize] = clampnan((rgbgreen[(indx) as usize] - Dgrb[(0) as usize][((indx >> 1)) as usize]), 0.0f32, 1.0f32);
out[(((((row * width) + col) * 4) + 2)) as usize] = clampnan((rgbgreen[(indx) as usize] - Dgrb[(1) as usize][((indx >> 1)) as usize]), 0.0f32, 1.0f32);
}
}
}
}
rr += 1;
}
}
{
let mut rr: i32 = 16;
while (rr < (rr1 - 16)) {
{
let row: i32 = (rr + top);
{
let mut cc: i32 = 16;
while (cc < (cc1 - 16)) {
{
let col: i32 = (cc + left);
let indx: i32 = ((rr * ts) + cc);
if ((col < width) && (row < height)) {
out[(((((row * width) + col) * 4) + 1)) as usize] = clampnan(rgbgreen[(indx) as usize], 0.0f32, 1.0f32);
}
}
cc += 1;
}
}
}
rr += 1;
}
}
}
left += (ts - 32);
}
}
}
top += (ts - 32);
}
}
Rgb32f {width:m.w,height:m.h,data:out.chunks_exact(4).map(|p|[p[0],p[1],p[2]]).collect()}
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upstream_vectors() {
        let (w,h)=(255,251);
        for pat in ["RGGB","BGGR","GRBG","GBRG"] {
            let cfa=crate::Cfa::bayer(pat).unwrap();
            let data:Vec<f32>=(0u32..(w*h) as u32).map(|i|((i.wrapping_mul(1664525).wrapping_add(1013904223)>>8)&65535) as f32/65536.0).collect();
            let out=amaze(&Mosaic {w,h,data:&data,cfa:&cfa});
            let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/amaze");
            let bytes=std::fs::read(root.join(format!("{pat}.f32"))).unwrap();
            let actual=out.data.iter().enumerate().filter(|(i,_)| {let (x,y)=(i%w,i/w);x%17<2 || y%17<2 || (126..=130).contains(&x) || (126..=130).contains(&y)}).flat_map(|(_,p)|p);
            let mut max=0.0f32;
            let mut count=0;
            for (a,b) in actual.zip(bytes.chunks_exact(4)) {
                let b=f32::from_le_bytes(b.try_into().unwrap());
                assert!(a.is_finite());
                max=max.max((a-b).abs());count+=1;
            }
            eprintln!("AMaZE {pat}: {count} samples max abs {max:.9}");
            assert!(max<=2e-6,"{pat}: {max}");
        }
    }
}
