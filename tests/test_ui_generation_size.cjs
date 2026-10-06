// Historical geometry oracle is pinned; production TypeScript parity is checked
// by frontend/src/features/generation/sizeMath.test.mjs. The retired legacy UI
// branch was removed with that interface; React coverage is test_ui_react_generation.cjs.
const assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const {fitDimensions, dimensionBounds} = require('./fixtures/legacy-frontend-e42/generation-size.cjs');
const large = {min_dimension:256,max_dimension:4096,dimension_step:32,max_pixels:4194304};
const small = {min_dimension:256,max_dimension:1536,dimension_step:64,max_pixels:1048576};
// These area-capped fixtures exercise externally supplied limits; they are not
// product defaults. Connected Comfy workflows currently expose no pixel cap.
const connected = {min_dimension:16,max_dimension:16384,dimension_step:16,max_pixels:null};
const fit = (width,height,ratio=1,axis='width',limits=large,locked=true) => fitDimensions({width,height,ratio,axis,locked},limits);
function valid(value,limits) {
  assert.ok(value.width>=limits.min_dimension && value.height>=limits.min_dimension);
  if (limits.max_dimension != null) assert.ok(value.width<=limits.max_dimension && value.height<=limits.max_dimension);
  assert.equal(value.width%limits.dimension_step,0);assert.equal(value.height%limits.dimension_step,0);
  if (limits.max_pixels != null) assert.ok(value.width*value.height<=limits.max_pixels);
}
assert.deepEqual(fit(4096,1024),{width:2048,height:2048});
assert.deepEqual(fit(2048,1024,3/2),{width:2016,height:1344});
assert.deepEqual(fit(1536,1600,3/2,'height'),{width:2400,height:1600});
assert.deepEqual(fit(4096,1024,16/9),{width:2560,height:1440});
assert.deepEqual(fit(4096,4096,16/9,'height',small),{width:1024,height:576});
assert.deepEqual(fit(4096,2048,1,'width',large,false),{width:2048,height:2048});
assert.deepEqual(fit(1024,4096,1,'height',large,false),{width:1024,height:4096});
assert.deepEqual(fit(0,NaN),{width:1024,height:1024});
assert.equal(dimensionBounds({width:1024,height:2048,locked:false},large).maxWidth,2048);
assert.equal(dimensionBounds({width:1024,height:576,ratio:16/9,locked:true},large).maxWidth,2560);
assert.equal(dimensionBounds({width:1024,height:576,ratio:16/9,locked:true},large).widthStep,512);
assert.deepEqual(fit(3840,2160,16/9,'width',connected),{width:3840,height:2160});
assert.deepEqual(fit(4096,2304,16/9,'width',connected),{width:4096,height:2304});
assert.deepEqual(fit(8192,8192,1,'width',connected),{width:8192,height:8192});
assert.deepEqual(fit(8000,4500,16/9,'width',{max_pixels:null}),{width:8000,height:4500});
assert.equal(dimensionBounds({width:8000,height:4500,ratio:16/9,locked:true},{max_pixels:null}).maxWidth,Infinity);
assert.deepEqual(fit(4096,2304,16/9,'width',{width:{min:16,max:8192,step:16},height:{min:32,max:4096,step:32},max_pixels:null}),{width:4096,height:2304});
const largeInputStart=performance.now();
assert.deepEqual(fit(1e9,1e9,16/9,'width',{}),{width:1000000000,height:562500000});
assert.deepEqual(fit(1e308,1e308,1,'width',{}),{width:1024,height:1024});
fit(1e9,16,1e9,'width',{});
assert.ok(performance.now()-largeInputStart<1000,'Huge uncapped input is bounded work, not a pixel-by-pixel search');
for (const limits of [small,large,{...large,dimension_step:16,max_pixels:2097152}]) {
  for (const ratio of [1,3/2,2/3,16/9,1000/733]) {
    for (const axis of ['width','height']) for (const size of [0,1,257,512,1024,1695,4096,Infinity,99999]) {
      const result=fit(size,size,ratio,axis,limits);valid(result,limits);
      if ([1,3/2,2/3,16/9].includes(ratio)) assert.ok(Math.abs(result.width/result.height-ratio)<1e-9);
    }
  }
}
console.log('PASS linked geometry: 270 mixed-limit cases plus uncapped 4K/8K, missing ceilings and per-axis connected-node bounds.');
