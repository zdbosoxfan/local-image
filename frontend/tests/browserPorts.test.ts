import {test} from 'node:test';
import assert from 'node:assert/strict';
import {collectDroppedFiles} from '../src/editor/browserPorts.ts';
const file=(name:string)=>({isFile:true,isDirectory:false,file:(done:(value:File)=>void)=>done(new File(['fixture'],name))}) as unknown as FileSystemEntry;
function directory(batches:FileSystemEntry[][],calls:string[]){return {isFile:false,isDirectory:true,createReader(){let index=0;return {readEntries(done:(entries:FileSystemEntry[])=>void){calls.push('read');done(batches[index++]??[]);}};}} as unknown as FileSystemEntry;}
test('folder drops traverse nested directories and every Chromium result batch',async()=>{
 const calls:string[]=[],nested=directory([[file('nested.tif')]],calls);
 const entries=directory([[file('first.png'),nested],[file('second.JPEG'),file('notes.txt')],[file('third.webp')]],calls);
 const files=await collectDroppedFiles([entries,file('last.png')]);
 assert.deepEqual(files.map(value=>value.name),['first.png','nested.tif','second.JPEG','third.webp','last.png']);
 assert.equal(calls.length,6,'Reads continue through the empty terminal batch at each directory depth');
});
test('failed folder reads reject without returning an incomplete image collection',async()=>{
 const bad={isFile:true,isDirectory:false,file:(_done:unknown,reject:(error:Error)=>void)=>reject(Error('Folder permission lost'))} as unknown as FileSystemEntry;
 await assert.rejects(collectDroppedFiles([file('first.png'),bad]),/Folder permission lost/);
});
