// Exercise the actual native request/reply code without a browser, user files,
// or real waits. The fake clock distinguishes human dialogs from machine work.
const fs=require('node:fs');
const vm=require('node:vm');
const path=require('node:path');
const assert=require('node:assert/strict');
const source=fs.readFileSync(path.join(__dirname,'..','backend','frontend','editor.js'),'utf8');
const start=source.indexOf('const NATIVE_DIALOG_ACTIONS=');
const end=source.indexOf('async function openNative(',start);
assert.ok(start>=0&&end>start,'Load the actual native bridge function area');
const nativeSource=source.slice(start,end);
assert.ok(nativeSource.includes("nativeBridge.addEventListener('message'"),'Use the production reply handler');

class Clock {
  constructor(){this.now=1700000000000;this.next=1;this.tasks=new Map();this.cleared=[];}
  setTimeout(callback,delay){const id=this.next++;this.tasks.set(id,{callback,due:this.now+delay});return id;}
  clearTimeout(id){this.cleared.push(id);this.tasks.delete(id);}
  advance(milliseconds){
    const until=this.now+milliseconds;
    while(true){
      const next=[...this.tasks].filter(([,task])=>task.due<=until).sort((a,b)=>a[1].due-b[1].due)[0];
      if(!next)break;
      const [id,task]=next;this.now=task.due;this.tasks.delete(id);task.callback();
    }
    this.now=until;
  }
}
class Bridge {
  constructor(){this.messages=[];this.listeners=[];this.throwOnPost=false;}
  addEventListener(type,listener){assert.equal(type,'message');this.listeners.push(listener);}
  postMessage(message){if(this.throwOnPost)throw Error('Bridge transport failed');this.messages.push(message);}
  postMessageWithAdditionalObjects(message,files){this.postMessage(message);this.lastFiles=files;}
  reply(data){for(const listener of this.listeners)listener({data});}
}
function harness(){
  const clock=new Clock(),bridge=new Bridge();
  class FakeDate extends Date {static now(){return clock.now;}}
  const context=vm.createContext({window:{chrome:{webview:bridge}},Date:FakeDate,
    setTimeout:clock.setTimeout.bind(clock),clearTimeout:clock.clearTimeout.bind(clock)});
  vm.runInContext(`let nativeBridge=null,nativeRequestNumber=0,nativeReady=false,nativeProjects=false,nativeSetup=false;
    const nativePending=new Map();let controlsCalled=0;function controls(){controlsCalled++;}
    function handleNativeClose(){throw Error('Unexpected close request');}`,context);
  vm.runInContext(nativeSource,context);
  return {clock,bridge,run:code=>vm.runInContext(code,context)};
}
function track(promise){
  const result={state:'pending'};
  result.promise=promise.then(value=>{result.state='resolved';result.value=value;return result;},error=>{result.state='rejected';result.error=error;return result;});
  return result;
}
const tick=()=>new Promise(resolve=>setImmediate(resolve));
const reply=(h,id,result,extra={})=>h.bridge.reply({type:'local-remove-native',id,result,error:null,...extra});
async function connect(h){
  const connected=h.run('connectNative()');
  const request=h.bridge.messages.at(-1);assert.equal(request.action,'ready');
  reply(h,request.id,{native:true,projects:true,setup:true});await connected;
  assert.equal(h.run('nativeReady&&nativeProjects&&nativeSetup'),true);
  assert.equal(h.clock.tasks.size,0);assert.equal(h.run('nativePending.size'),0);
}

async function main(){
  const dialogActions=['batchExportFolder','openFiles','openFolder','openProject','saveProject',
    'chooseBackgroundFolder','configureAi','setupChooseComfyDirectory','setupChooseModelDirectory','setupChooseInstallDirectory'];
  const h=harness();await connect(h);
  for(const action of dialogActions){
    for(const cancel of [false,true]){
      const pending=track(h.run(`nativeRequest(${JSON.stringify(action)},null,{job_id:'prepared-queue'})`));
      const request=h.bridge.messages.at(-1);
      assert.equal(request.action,action);assert.equal(request.job_id,'prepared-queue');
      assert.equal(h.run('nativePending.size'),1);
      assert.equal(h.clock.tasks.size,0,action+' waits for the native dialog response without a deadline');
      h.clock.advance(600001);await tick();
      assert.equal(pending.state,'pending',action+' remains pending after a user takes longer than ten minutes');
      h.bridge.reply({type:'unrelated-message',id:request.id,result:'ignore'});
      reply(h,'unknown-request','ignore');await tick();
      assert.equal(pending.state,'pending','Unrelated messages cannot settle the dialog');
      const outcome=cancel?null:{exported:2,credits:2};
      reply(h,request.id,outcome);await pending.promise;
      assert.equal(pending.state,'resolved');assert.deepEqual(pending.value,outcome);
      assert.equal(h.run('nativePending.size'),0,'OK and Cancel both release the pending request');
      assert.equal(h.clock.tasks.size,0);
      reply(h,request.id,{exported:999});assert.deepEqual(pending.value,outcome,'A duplicate reply cannot replace the settled result');
    }
  }

  const explicitCancel=track(h.run("nativeRequest('openFolder')"));
  reply(h,h.bridge.messages.at(-1).id,{ignored:true},{cancelled:true});await explicitCancel.promise;
  assert.equal(explicitCancel.value,null);assert.equal(h.run('nativePending.size'),0);
  const nativeError=track(h.run("nativeRequest('batchExportFolder')"));
  reply(h,h.bridge.messages.at(-1).id,null,{error:'Folder is unavailable'});await nativeError.promise;
  assert.equal(nativeError.state,'rejected');assert.match(nativeError.error.message,/Folder is unavailable/);
  assert.equal(h.run('nativePending.size'),0);

  // All setup calls that only start backend work, and file drops, keep the
  // machine-command deadline; downloads themselves continue in backend jobs.
  const machineActions=['drop','setupUseInstallation','setupDownloadModels','setupDownloadQwen','setupStart',
    'setupDownloadGenerationModel','loraDownload','setupEject','setupInstall'];
  for(const action of machineActions){
    const pending=track(h.run(`nativeRequest(${JSON.stringify(action)})`));
    assert.equal(h.clock.tasks.size,1,action+' retains a machine timeout');
    h.clock.advance(599999);await tick();assert.equal(pending.state,'pending');
    h.clock.advance(1);await pending.promise;
    assert.equal(pending.state,'rejected');assert.match(pending.error.message,/desktop command timed out/);
    assert.equal(h.run('nativePending.size'),0);assert.equal(h.clock.tasks.size,0);
  }
  const completedMachine=track(h.run("nativeRequest('setupStart')"));
  reply(h,h.bridge.messages.at(-1).id,{started:true});await completedMachine.promise;
  assert.equal(completedMachine.state,'resolved');assert.equal(h.clock.tasks.size,0,'A reply cancels the machine deadline');
  h.clock.advance(600001);await tick();assert.equal(completedMachine.state,'resolved');

  const ready=harness(),connecting=ready.run('connectNative()');
  ready.clock.advance(2999);await tick();assert.equal(ready.run('nativePending.size'),1);
  ready.clock.advance(1);await connecting;
  assert.equal(ready.run('nativeReady||nativeProjects||nativeSetup'),false,'The ready handshake still expires at three seconds');
  assert.equal(ready.run('nativePending.size'),0);assert.equal(ready.clock.tasks.size,0);

  h.bridge.throwOnPost=true;
  for(const expression of ["nativeRequest('openFolder')","nativeRequest('setupStart')","nativeRequest('drop',[{name:'example.png'}])"]){
    const failed=track(h.run(expression));await failed.promise;
    assert.equal(failed.state,'rejected');assert.match(failed.error.message,/Bridge transport failed/);
    assert.equal(h.run('nativePending.size'),0);assert.equal(h.clock.tasks.size,0,'A transport throw releases any timer');
  }
  h.bridge.throwOnPost=false;h.bridge.postMessageWithAdditionalObjects=undefined;
  const unavailable=track(h.run("nativeRequest('drop',[{name:'example.png'}])"));await unavailable.promise;
  assert.equal(unavailable.state,'rejected');assert.match(unavailable.error.message,/Use File/);
  assert.equal(h.run('nativePending.size'),0);assert.equal(h.clock.tasks.size,0);
  h.run('nativeBridge=null');const absent=track(h.run("nativeRequest('openFolder')"));await absent.promise;
  assert.equal(absent.state,'rejected');assert.match(absent.error.message,/desktop app/);
  assert.equal(h.run('nativePending.size'),0);
  console.log('PASS: ten native picker/settings actions retain replies after >10 minutes; OK/Cancel/error clear pending; unrelated and duplicate replies ignored; ready remains 3 seconds; nine machine actions remain 10 minutes; successful commands cancel deadlines; transport/missing-file-bridge errors clean timers and pending state.');
}
main().catch(error=>{console.error(error);process.exitCode=1;});
