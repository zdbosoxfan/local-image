import {useLayoutEffect,useRef,useSyncExternalStore} from 'react';
import {Button,Checkbox,Dialog,DialogActions,DialogBody,DialogContent,DialogSurface,DialogTitle,Link} from '@fluentui/react-components';
import type {EditorDialogsController} from './editorDialogs.ts';
import './shell.css';

const safeLink=(value:string|undefined)=>{try{const url=new URL(value||'');return ['https:','http:'].includes(url.protocol)?url.href:undefined;}catch{return undefined;}};
export function EditorDialogs({controller}:{controller:EditorDialogsController}){
  const snapshot=useSyncExternalStore(controller.subscribe,controller.getSnapshot);
  const lastOpen=useRef(snapshot),open=snapshot.kind!=='none';
  useLayoutEffect(()=>{if(open)lastOpen.current=snapshot;},[open,snapshot]);
  // Fluent retains the surface during exit; cancellation itself stays immediate.
  const state=open?snapshot:lastOpen.current;
  const title=state.kind==='overwrite'?'Overwrite original image?':state.kind==='close'?state.plan.all?'Close Local Image?':'Close image?':'Image credits';
  return <Dialog open={open} modalType={state.kind==='credits'?'modal':'alert'} onOpenChange={(_,data)=>{if(!data.open)controller.close();}}>
    <DialogSurface data-react-owned="true" className="li-editor-dialog" aria-label={title}><DialogBody><DialogTitle>{title}</DialogTitle><DialogContent>
      {state.kind==='overwrite'&&<><p>This replaces <strong>{state.filename}</strong> with the current result in its original format.</p><p>Choose Save a copy to keep the original.</p><Checkbox checked={state.dontAsk} label="Don't ask again before overwriting originals" onChange={(_,data)=>controller.setDontAsk(data.checked===true)}/></>}
      {state.kind==='close'&&<><p>{state.plan.warning}</p><ul className="li-close-documents">{state.plan.names.map((name,index)=><li key={index}>{name}</li>)}</ul><p>Save an editable project to keep original images and layers.</p>{state.plan.pendingSelection&&<p>Unapplied brush and pen selections are cleared when you close and are not saved in the project.</p>}</>}
      {state.kind==='credits'&&<><div className="li-credit-list">{state.credits.map((credit,index)=><section key={index}><h3>{credit.label}</h3>{credit.title&&<strong>{credit.title}</strong>}<p>{credit.attribution||credit.creator||''}</p>{safeLink(credit.source_url)&&<Link href={safeLink(credit.source_url)} target="_blank" rel="noopener noreferrer">Original source</Link>}{safeLink(credit.license_url)&&<> · <Link href={safeLink(credit.license_url)} target="_blank" rel="noopener noreferrer">{credit.license||'License'}</Link></>}</section>)}</div><p>Credits stay in editable projects. Include required attribution when sharing an exported image.</p><p role={state.error?'alert':'status'}>{state.status}</p></>}
    </DialogContent><DialogActions>
      <Button autoFocus onClick={controller.close}>{state.kind==='credits'?'Close':'Cancel'}</Button>
      {state.kind==='overwrite'&&<><Button onClick={()=>controller.respond(state.id,'unique')}>Save a copy</Button><Button appearance="primary" onClick={()=>controller.respond(state.id,'overwrite')}>Overwrite original</Button></>}
      {state.kind==='close'&&<><Button onClick={()=>controller.respond(state.id,'discard')}>{state.plan.dirtyCount?'Discard layers and close':`Close ${state.plan.all?'application':'image'}`}</Button>{state.plan.dirtyCount>0&&<Button appearance="primary" onClick={()=>controller.respond(state.id,'save')}>{state.plan.nativeProjects?state.plan.dirtyCount>1?'Save projects and close…':'Save project and close…':`Download project${state.plan.dirtyCount>1?'s':''}…`}</Button>}</>}
      {state.kind==='credits'&&<><Button onClick={()=>void controller.copyCredits()}>Copy credits</Button><Button onClick={()=>controller.downloadCredits()}>Save credits .txt</Button></>}
    </DialogActions></DialogBody></DialogSurface>
  </Dialog>;
}
