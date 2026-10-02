import { test } from 'node:test';
import assert from 'node:assert/strict';
import { resolveEditorKey, type KeyInput, type KeyboardState, type KeyTarget } from '../src/editor/keyboardController.ts';
const state: KeyboardState = {modalOpen:false,menuOpen:false,hasDocument:true,hiddenEditor:false,canReturn:true,busy:false,showOriginal:false,workspace:'retouch',tool:'brush',handActive:false};
const target: KeyTarget = {textEntry:false,activates:false,folderEntry:false,brushSlider:false,layerEditing:true};
const key = (value: string, change: Partial<KeyInput> = {}): KeyInput => ({key:value,code:value===' '?'Space':value,ctrlKey:false,metaKey:false,altKey:false,shiftKey:false,...change});
test('file, history and menu routes preserve modifier distinctions',()=>{
  assert.deepEqual(resolveEditorKey(key('n',{ctrlKey:true}),state,target),{kind:'command',command:'newWorkspace'});
  assert.deepEqual(resolveEditorKey(key('s',{ctrlKey:true,altKey:true}),state,target),{kind:'command',command:'saveProject'});
  assert.deepEqual(resolveEditorKey(key('s',{ctrlKey:true,shiftKey:true}),state,target),{kind:'command',command:'saveUnique'});
  assert.deepEqual(resolveEditorKey(key('z',{ctrlKey:true,shiftKey:true}),state,target),{kind:'command',command:'redo'});
  assert.deepEqual(resolveEditorKey(key('e',{ctrlKey:true,altKey:true,shiftKey:true}),state,target),{kind:'command',command:'mergeLayers'});
  assert.deepEqual(resolveEditorKey(key('f',{altKey:true}),state,target),{kind:'menu',menu:'File'});
  assert.deepEqual(resolveEditorKey(key('F10'),state,target),{kind:'menu',menu:null});
});
test('New workspace is available from empty views and focused inputs but does not bypass a modal',()=>{
  const input={...target,textEntry:true};
  assert.deepEqual(resolveEditorKey(key('n',{ctrlKey:true}),{...state,hasDocument:false},input),{kind:'command',command:'newWorkspace'});
  assert.deepEqual(resolveEditorKey(key('n',{metaKey:true}),{...state,workspace:'generate',hiddenEditor:true},input),{kind:'command',command:'newWorkspace'});
  assert.deepEqual(resolveEditorKey(key('n',{ctrlKey:true}),{...state,modalOpen:true},input),{kind:'block'});
  assert.equal(resolveEditorKey(key('n',{ctrlKey:true,shiftKey:true}),state,input),null);
  assert.equal(resolveEditorKey(key('n',{ctrlKey:true,altKey:true}),state,input),null);
});
test('focused inputs and controls retain editing, Space and Enter behavior',()=>{
  const input={...target,textEntry:true};
  for(const k of [key('b'),key('Delete'),key('z',{ctrlKey:true}),key(' '),key('Enter')]) assert.equal(resolveEditorKey(k,state,input),null);
  assert.deepEqual(resolveEditorKey(key('s',{ctrlKey:true}),state,input),{kind:'command',command:'overwrite'});
  assert.deepEqual(resolveEditorKey(key('s',{ctrlKey:true}),{...state,canReturn:false},input),{kind:'command',command:'exportImage'});
  assert.equal(resolveEditorKey(key(' '),state,{...target,activates:true}),null);
  assert.equal(resolveEditorKey(key('Enter'),{...state,tool:'pen'},{...target,activates:true}),null);
  assert.deepEqual(resolveEditorKey(key(']'),state,{...input,brushSlider:true}),{kind:'brush',direction:1});
  assert.equal(resolveEditorKey(key(' '),state,{...target,folderEntry:true}),null);
  assert.equal(resolveEditorKey(key('+',{defaultPrevented:true}),state,target),null);
  assert.equal(resolveEditorKey(key('Delete'),state,{...target,layerEditing:false}),null);
  assert.equal(resolveEditorKey(key('F2'),state,{...target,layerEditing:false}),null);
});
test('hidden generation views and modals cannot edit a retained canvas',()=>{
  for(const k of [key('z',{ctrlKey:true}),key('b'),key(' '),key('Delete'),key('+')]) assert.equal(resolveEditorKey(k,{...state,hiddenEditor:true},target),null);
  assert.deepEqual(resolveEditorKey(key('s',{ctrlKey:true}),{...state,hiddenEditor:true},target),{kind:'command',command:'exportImage'});
  assert.deepEqual(resolveEditorKey(key('w',{ctrlKey:true}),{...state,hiddenEditor:true},target),{kind:'command',command:'closeImage'});
  assert.deepEqual(resolveEditorKey(key('s',{ctrlKey:true}),{...state,modalOpen:true},target),{kind:'block'});
  assert.equal(resolveEditorKey(key('b'),{...state,modalOpen:true},target),null);
});
test('canvas shortcuts preserve pan, brush, navigation and selection boundaries',()=>{
  assert.deepEqual(resolveEditorKey(key(' '),state,target),{kind:'space',held:true});
  assert.equal(resolveEditorKey(key('b'),state,target,true),null);
  assert.deepEqual(resolveEditorKey(key('PageDown'),state,target),{kind:'navigate',direction:1});
  assert.deepEqual(resolveEditorKey(key('ArrowLeft',{altKey:true}),state,target),{kind:'navigate',direction:-1});
  assert.deepEqual(resolveEditorKey(key('Enter'),{...state,tool:'pen'},target),{kind:'command',command:'finishPath'});
  assert.deepEqual(resolveEditorKey(key('v'),state,target),{kind:'tool',tool:'move'});
  assert.equal(resolveEditorKey(key('b'),{...state,busy:true},target),null);
  assert.equal(resolveEditorKey(key('b'),{...state,showOriginal:true},target),null);
});
