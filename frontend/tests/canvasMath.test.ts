import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import { clampCamera, fitCamera, layerGeometry, movedTransform, pointOnLayer, previewCoordinate, rotatedTransform, scaledTransform, zoomAt } from '../src/editor/canvasMath.ts';
import type { Camera, LayerTransform } from '../src/editor/canvasMath.ts';

// Immutable pre-migration arithmetic; provenance/integrity is checked separately.
const oracle=JSON.parse(fs.readFileSync(new URL('../../tests/fixtures/legacy-frontend-e42/camera-regions.json',import.meta.url),'utf8')) as Record<string,string>;
const plain=(value:unknown)=>JSON.parse(JSON.stringify(value));
const equal=(a:unknown,b:unknown)=>assert.deepEqual(plain(a),plain(b));

test('camera fit, clamping, anchor zoom and clamped coordinates equal pinned legacy arithmetic',()=>{
  const context=vm.createContext({baseCanvas:{width:1000,height:700},mask:{width:1000,height:700},viewport:{clientWidth:800,clientHeight:560},session:{width:4000},zoom:1,panX:0,panY:0,fitMode:false,viewportWidth:0,viewportHeight:0,gesture:null,MIN_PHOTO_ZOOM:.01,MAX_PHOTO_ZOOM:8,
    pixelRatio:()=>context.session.width/context.baseCanvas.width,localPoint:(point:unknown)=>point,endGesture(){},applyCamera:()=>vm.runInContext('clampCamera()',context)});
  vm.runInContext(oracle.clampCamera+oracle.fitImage+oracle.setPhotoZoom+oracle.coord,context);
  for(const [pw,ph,vw,vh,native]of[[3000,2000,800,560,12000],[640,480,1366,768,640],[1,1,8,8,100],[1200,2600,560,800,6000]]){
    context.baseCanvas={width:pw,height:ph};context.mask={width:pw,height:ph};context.viewport={clientWidth:vw,clientHeight:vh};context.session={width:native};
    vm.runInContext('fitImage()',context);
    const fitted=clampCamera(fitCamera(pw,ph,vw,vh,native/pw),pw,ph,vw,vh);
    equal({zoom:context.zoom,panX:context.panX,panY:context.panY,fitMode:context.fitMode},fitted);
    for(const value of[-2,.01,.333,1,8,20]){
      const before:Camera={zoom:context.zoom,panX:context.panX,panY:context.panY,fitMode:context.fitMode},anchor={x:vw*.27,y:vh*.61};
      context.value=value;context.anchor=anchor;vm.runInContext('setPhotoZoom(value,anchor)',context);
      const next=clampCamera(zoomAt(before,value,native/pw,anchor),pw,ph,vw,vh);
      equal({zoom:context.zoom,panX:context.panX,panY:context.panY,fitMode:context.fitMode},next);
      for(const point of[{x:-10,y:-50},{x:vw/2,y:vh/2},{x:vw+20,y:vh+10}]){context.point=point;equal(vm.runInContext('coord(point)',context),previewCoordinate(point,next,pw,ph));}
    }
  }
});

test('layer handle geometry and move/scale/rotate updates equal actual stacked gesture formulas',()=>{
  const arithmetic=oracle.stackGesture;
  const context=vm.createContext({session:{width:641,height:479},zoom:.7,pixelRatio:()=>4,transform:(node:{transform:LayerTransform})=>node.transform});
  vm.runInContext(oracle.layerGeometry+`function gestureResult(d,p){const o=d.original;${arithmetic};return d.next;}`,context);
  for(const rotation of[-179,-90,0,37,179])for(const scale of[.05,.8,1,3.9]){
    const original={offset_x:13.5,offset_y:-29.25,scale,rotation},bounds=[23,31,517,401],node={transform:original,bounds};context.node=node;
    const geometry=layerGeometry(641,479,bounds,original,4,.7);equal(vm.runInContext('geometry(node,node.transform)',context),geometry);
    for(const point of[{x:0,y:0},{x:320,y:239},{x:517,y:401}]){context.point=point;equal(vm.runInContext('pointOnLayer(point,node,node.transform)',context),pointOnLayer(point,641,479,original));}
    const first={x:245,y:197},point={x:419.5,y:301.2};
    for(const kind of['move','scale','rotate'])for(const corner of[0,1,2,3]){
      context.drag={kind,corner,original,geometry,start:first};context.point=point;
      const expected=vm.runInContext('gestureResult(drag,point)',context);
      const next=kind==='move'?movedTransform(original,first,point):kind==='scale'?scaledTransform(original,geometry,corner,point,641,479):rotatedTransform(original,geometry,first,point,641,479);
      equal(expected,next);
    }
  }
});
