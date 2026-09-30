// Read-only CDP evidence from the recorded, actual packaged WebView2 target.
// No Edge browser is launched and no native message or filesystem command is
// injected. Native window inputs belong to the scoped Computer Use session.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),{execFileSync}=require('node:child_process');
const root=path.resolve(__dirname,'../..');
const inside=(value,directory)=>{const resolved=path.resolve(value),relative=path.relative(path.resolve(directory),resolved);if(relative.startsWith('..')||path.isAbsolute(relative))throw Error('Path is outside this QA run');return resolved;};
const hash=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
class NativeAcceptanceCdp {
 constructor(manifestPath){
  this.manifestPath=inside(manifestPath,path.join(root,'qa-artifacts'));this.manifest=JSON.parse(fs.readFileSync(this.manifestPath,'utf8'));
  this.run=inside(this.manifest.runRoot,path.join(root,'qa-artifacts'));this.host=inside(this.manifest.host,path.join(root,'dist'));
  if(!this.manifest.applicationLaunched||!this.manifest.startupVerified||hash(this.host)!==this.manifest.hostSha256)throw Error('A verified current QA launch is required before attaching');
  this.sequence=0;this.pending=new Map();this.events=[];this.closed=false;this.socket=null;
 }
 async connect(){
  const helper=path.join(root,'qa-artifacts/native-acceptance/tools/NativeQaWindow.exe');
  const identity=JSON.parse(execFileSync(helper,['windows',this.manifestPath],{cwd:root,encoding:'utf8',windowsHide:true}));
  if(identity.pid!==this.manifest.pid)throw Error('QA process identity changed');
  if(!this.manifest.debugListener?.listeners?.length||this.manifest.debugListener.listeners.some(item=>!item.owned||!['127.0.0.1','::1'].includes(item.address)))throw Error('The recorded debug listener is not exclusively QA-owned loopback');
  const runtime=await(await fetch(this.manifest.backendUrl+'/api/local-remove/runtime')).json();
  if(runtime.application!=='local-remove'||path.resolve(runtime.data_root)!==path.resolve(this.manifest.profile))throw Error('Unexpected backend/profile; no target was attached');
  const endpoint=`http://127.0.0.1:${this.manifest.cdpPort}`;
  const targets=await(await fetch(endpoint+'/json/list')).json();
  const matching=targets.filter(target=>target.id===this.manifest.editorTargetId&&target.type==='page'&&this.trusted(target.url));
  if(matching.length!==1)throw Error('The recorded trusted WebView2 target is missing or ambiguous');
  const socketUrl=new URL(matching[0].webSocketDebuggerUrl);
  if(socketUrl.protocol!=='ws:'||socketUrl.hostname!=='127.0.0.1'||Number(socketUrl.port)!==this.manifest.cdpPort)throw Error('CDP must remain on the recorded loopback endpoint');
  this.socket=new WebSocket(socketUrl.href);
  this.socket.addEventListener('message',event=>this.receive(JSON.parse(event.data)));
  this.socket.addEventListener('close',()=>{this.closed=true;for(const value of this.pending.values()){clearTimeout(value.timer);value.reject(Error('QA CDP connection closed'));}this.pending.clear();});
  await new Promise((resolve,reject)=>{this.socket.addEventListener('open',resolve,{once:true});this.socket.addEventListener('error',()=>reject(Error('Could not attach to the QA WebView2 target')),{once:true});});
  await this.send('Runtime.enable');await this.send('Page.enable');await this.send('Network.enable');
  return this.inspect();
 }
 trusted(value){try{const parsed=new URL(value);return parsed.origin==='http://127.0.0.1:51247'&&parsed.pathname==='/remove';}catch{return false;}}
 receive(message){
  if(message.id){const waiting=this.pending.get(message.id);if(!waiting)return;clearTimeout(waiting.timer);this.pending.delete(message.id);if(message.error)waiting.reject(Error(message.error.message));else waiting.resolve(message.result);return;}
  const data=message.params||{};
  // Never record request headers, post bodies, cookies or token-containing HTML.
  if(message.method==='Network.requestWillBeSent'){let target;try{target=new URL(data.request.url);}catch{return;}this.events.push({type:'request',method:data.request.method,origin:target.origin,path:target.pathname});}
  else if(message.method==='Runtime.exceptionThrown')this.events.push({type:'pageerror',text:data.exceptionDetails?.text||'Runtime exception',description:data.exceptionDetails?.exception?.description?.slice(0,1500)});
  else if(message.method==='Page.frameNavigated'&&!data.frame?.parentId)this.events.push({type:'navigation',trusted:this.trusted(data.frame.url),url:data.frame.url});
 }
 send(method,params={}){
  if(this.closed||!this.socket) return Promise.reject(Error('CDP is not attached'));
  const id=++this.sequence;
  return new Promise((resolve,reject)=>{const timer=setTimeout(()=>{this.pending.delete(id);reject(Error('QA evidence request timed out'));},15000);this.pending.set(id,{resolve,reject,timer});this.socket.send(JSON.stringify({id,method,params}));});
 }
 async inspect(){
  const response=await this.send('Runtime.evaluate',{returnByValue:true,expression:`JSON.stringify({url:location.href,title:document.title,readyState:document.readyState,reactReady:document.body.dataset.reactReady||null,nativeTransport:!!window.chrome?.webview,width:innerWidth,height:innerHeight,dpr:devicePixelRatio,density:document.documentElement.dataset.uiDensity||null,dialogs:[...document.querySelectorAll('[role=dialog],[role=alertdialog],dialog[open]')].map(node=>({label:node.getAttribute('aria-label')||node.querySelector('h1,h2,h3')?.textContent||'',text:node.textContent.slice(0,1800)})),styles:[...document.querySelectorAll('style')].map(node=>({hasNonce:!!node.nonce,bytes:node.textContent.length})),allStylesNonced:[...document.querySelectorAll('style')].every(node=>node.nonce===window.__LOCAL_IMAGE_BOOTSTRAP__?.nonce),editor:(()=>{const s=window.LocalImageEditor?.getSnapshot();return s?{nativeReady:s.nativeReady,nativeProjects:s.nativeProjects,busy:s.busy,workspace:s.workspace,selectionActive:s.selectionActive,status:s.status,statusError:s.statusError,document:s.document?{id:s.document.id,name:s.document.name,revision:s.document.revision,canReturn:s.document.can_return,projectName:s.document.project_name,projectSaved:s.document.project_saved,projectDirty:s.document.project_dirty,layerCount:s.document.layer_stack?.length}:null}:null;})(),canvas:window.LocalImageEditor?.canvas.getSnapshot()??null})`});
  if(response.exceptionDetails)throw Error('Read-only WebView2 inspection failed');
  const value=JSON.parse(response.result.value);if(!this.trusted(value.url))throw Error('Native test page left the exact trusted origin/path');
  return value;
 }
 async capture(name){
  if(!/^[a-z0-9_-]+$/i.test(name))throw Error('Use a simple screenshot name');
  const state=await this.inspect(),result=await this.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false,fromSurface:true});
  const target=inside(path.join(this.run,name+'.png'),this.run);fs.writeFileSync(target,Buffer.from(result.data,'base64'));
  const evidence={kind:'actual packaged WebView2 content capture; native owned dialogs are outside this image',file:target,sha256:hash(target),state};
  fs.writeFileSync(inside(path.join(this.run,name+'.json'),this.run),JSON.stringify(evidence,null,2));return evidence;
 }
 artifact(filename){const file=inside(filename,this.run),stats=fs.statSync(file);if(!stats.isFile())throw Error('Expected a completed QA output file');return{file,bytes:stats.size,sha256:hash(file),modified:stats.mtime.toISOString()};}
 record(name,status,details={}){
  if(!['passed','failed','blocked','pending'].includes(status))throw Error('Explicit checkpoint outcome is required');
  const row={at:new Date().toISOString(),name,status,details};fs.appendFileSync(path.join(this.run,'checkpoints.jsonl'),JSON.stringify(row)+'\n');return row;
 }
 detach(){if(this.socket)this.socket.close();this.socket=null;fs.writeFileSync(path.join(this.run,'webview-events.json'),JSON.stringify(this.events,null,2));}
}
module.exports={NativeAcceptanceCdp};
if(require.main===module){
 const manifest=process.argv[2],capture=process.argv[3];if(!manifest)throw Error('Supply a verified QA launch manifest');
 (async()=>{const session=new NativeAcceptanceCdp(manifest);try{const state=await session.connect();console.log(JSON.stringify(capture?await session.capture(capture):state,null,2));}finally{session.detach();}})().catch(error=>{console.error(error.message);process.exitCode=1;});
}
